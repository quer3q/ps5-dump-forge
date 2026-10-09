//! Per-job verification: read the output back through its own reader and
//! compare paths, empty dirs, sizes and BLAKE3 hashes against what the source held.
//! Full mode hashes every byte back; fast mode a seeded sample of them ([`plan`]). Both
//! compare 8 MiB slices, so the source is hashed once.

use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{anyhow, bail};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::extract::CHUNK;
use crate::jobs::Ctx;
use crate::preflight::listing;
use crate::{VerifyMode, VerifySummary};

/// Source files are hashed in fixed slices of this size, so fast mode can compare any one.
pub(crate) const SLICE: u64 = 8 * 1024 * 1024;
/// Fast mode hashes files up to this size whole.
const WHOLE_MAX: u64 = 2 * SLICE;
/// Fast mode's random slices: at most this many bytes, and at most 1% of the unsampled rest.
const BUDGET_MAX: u64 = 1024 * 1024 * 1024;
/// UFS2 with 64 KiB blocks maps a file's first 12 blocks directly and the next 8192 through
/// one indirect block; the double indirect block starts here (512.75 MiB).
const UFS2_SEAM: u64 = (12 + 8192) * 64 * 1024;
/// The sample plan's version, logged with the seed so a run can be reproduced.
const POLICY: u32 = 1;

/// How the content is compared once every structural check has passed.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Mode {
    Full,
    /// [`plan`]'s sample for `seed`; `seam` adds the slice holding [`UFS2_SEAM`] (a UFS2 image).
    Fast {
        seed: u64,
        seam: bool,
    },
}

/// A fresh seed from the OS, below 2^53 so the UI's JSON reader shows it exactly.
pub(crate) fn fresh_seed() -> anyhow::Result<u64> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness for fast verification: {e}"))?;
    Ok(u64::from_le_bytes(bytes) & ((1 << 53) - 1))
}

/// A source file's BLAKE3 per [`SLICE`] (the last slice may be short; an empty file has
/// none). Same size and same slices is the same bytes, so no whole-file hash is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Digest {
    pub slices: Vec<blake3::Hash>,
}

/// Builds a [`Digest`] from bytes fed front to back in pieces of any size: slice boundaries
/// don't depend on how the file is read, and nothing is buffered.
///
/// A whole slice is hashed on the [`pool`], so the thread reading the source only copies it:
/// one core hashes ~2.4 GB/s, which made hashing, not the disk, a write's limit.
struct Digester {
    len: u64,
    /// The current slice's bytes.
    buf: Vec<u8>,
    slices: Vec<Slice>,
}

enum Slice {
    Ready(blake3::Hash),
    Hashing(Receiver<blake3::Hash>),
}

/// A short tail slice up to this size is hashed in place: handing it over costs more.
const INLINE_MAX: usize = 1 << 20;

impl Digester {
    fn new() -> Self {
        Self {
            len: 0,
            buf: Vec::new(),
            slices: Vec::new(),
        }
    }

    fn update(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let room = (SLICE - self.len % SLICE) as usize;
            let (now, rest) = bytes.split_at(bytes.len().min(room));
            if self.buf.capacity() == 0 {
                self.buf = pool().buffer();
            }
            self.buf.extend_from_slice(now);
            self.len += now.len() as u64;
            if self.len.is_multiple_of(SLICE) {
                let full = std::mem::take(&mut self.buf);
                self.slices.push(Slice::Hashing(pool().hash(full)));
            }
            bytes = rest;
        }
    }

    /// A short tail is the last slice.
    fn finish(mut self) -> Digest {
        if !self.len.is_multiple_of(SLICE) {
            let tail = std::mem::take(&mut self.buf);
            self.slices.push(if tail.len() <= INLINE_MAX {
                let hash = blake3::hash(&tail);
                pool().give_back(tail);
                Slice::Ready(hash)
            } else {
                Slice::Hashing(pool().hash(tail))
            });
        } else if self.buf.capacity() > 0 {
            pool().give_back(std::mem::take(&mut self.buf));
        }
        let slices = self
            .slices
            .into_iter()
            .map(|s| match s {
                Slice::Ready(hash) => hash,
                // A worker only hashes and replies; it cannot fail.
                Slice::Hashing(rx) => rx.recv().expect("the hashing pool replies"),
            })
            .collect();
        Digest { slices }
    }
}

/// Threads that hash whole slices, one per core up to [`POOL_MAX`], fed through a bounded queue;
/// their buffers are reused. At most `3 × threads` slices exist at once (queued, being hashed,
/// spare): 192 MiB at 8 threads, which still hash ~19 GB/s, past any disk.
struct Pool {
    jobs: SyncSender<(Vec<u8>, SyncSender<blake3::Hash>)>,
    spare: Arc<Mutex<Vec<Vec<u8>>>>,
    keep: usize,
}

type Job = (Vec<u8>, SyncSender<blake3::Hash>);

/// The most hashing threads (see [`Pool`]): a bound on memory, on the PS5 most of all.
const POOL_MAX: usize = 8;

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(POOL_MAX);
        let (jobs, queue) = sync_channel::<Job>(threads);
        let queue = Arc::new(Mutex::new(queue));
        let spare = Arc::new(Mutex::new(Vec::new()));
        let keep = threads;
        for i in 0..threads {
            let (queue, spare) = (queue.clone(), spare.clone());
            // ponytail: workers live as long as the process (the pool is static). If none
            // starts, the queue's receiver is gone and the first `hash` fails loudly.
            let _ = std::thread::Builder::new()
                .name(format!("hash-{i}"))
                .spawn(move || {
                    loop {
                        let Ok((buf, reply)) = queue.lock().expect("queue lock").recv() else {
                            return;
                        };
                        let _ = reply.send(blake3::hash(&buf));
                        let mut spare = spare.lock().expect("spare lock");
                        if spare.len() < keep {
                            let mut buf = buf;
                            buf.clear();
                            spare.push(buf);
                        }
                    }
                });
        }
        Pool { jobs, spare, keep }
    })
}

impl Pool {
    /// An empty buffer that holds a whole slice.
    fn buffer(&self) -> Vec<u8> {
        self.spare
            .lock()
            .expect("spare lock")
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(SLICE as usize))
    }

    fn give_back(&self, mut buf: Vec<u8>) {
        let mut spare = self.spare.lock().expect("spare lock");
        if spare.len() < self.keep && buf.capacity() >= SLICE as usize {
            buf.clear();
            spare.push(buf);
        }
    }

    /// `buf`'s hash, as soon as a worker gets to it.
    fn hash(&self, buf: Vec<u8>) -> Receiver<blake3::Hash> {
        let (reply, rx) = sync_channel(1);
        self.jobs
            .send((buf, reply))
            .expect("the hashing pool is running");
        rx
    }
}

/// What the output must hold: the source's files with their digests, and its empty dirs.
pub(crate) struct Expected {
    pub files: Vec<SourceFile>,
    /// One per file, in the same order.
    pub digests: Vec<Digest>,
    pub empty_dirs: Vec<String>,
}

/// More files than this being read at once means the writer is not reading front to back;
/// their hashes are then dropped and computed by a second read instead.
const MAX_IN_FLIGHT: usize = 64;

/// A source that hashes each file as the writer reads it front to back, so the source is
/// usually read once, not twice. Files read any other way are hashed afterwards.
pub(crate) struct HashingTree<'a> {
    inner: &'a mut dyn SourceTree,
    sizes: HashMap<String, u64>,
    in_flight: HashMap<String, Digester>,
    done: HashMap<String, Digest>,
}

impl<'a> HashingTree<'a> {
    pub(crate) fn new(inner: &'a mut dyn SourceTree) -> Self {
        let sizes = inner
            .files()
            .iter()
            .map(|f| (f.path.clone(), f.size))
            .collect();
        Self {
            inner,
            sizes,
            in_flight: HashMap::new(),
            done: HashMap::new(),
        }
    }

    fn observe(&mut self, path: &str, offset: u64, bytes: &[u8]) {
        let Some(&size) = self.sizes.get(path) else {
            return;
        };
        if offset == 0 {
            if self.in_flight.len() >= MAX_IN_FLIGHT {
                self.in_flight.clear();
            }
            self.in_flight.insert(path.to_string(), Digester::new());
        }
        let Some(digester) = self.in_flight.get_mut(path) else {
            return;
        };
        if digester.len != offset {
            self.in_flight.remove(path);
            return;
        }
        digester.update(bytes);
        if digester.len >= size {
            let digester = self.in_flight.remove(path).expect("present above");
            // Only a file covered exactly, front to back, keeps its digest and slices.
            if digester.len == size {
                self.done.insert(path.to_string(), digester.finish());
            }
        }
    }

    /// The expected manifest: digests made during the write, the rest read again now.
    pub(crate) fn expected(self, ctx: &Ctx, mode: Mode) -> anyhow::Result<Expected> {
        expected_from(self.inner, &self.done, ctx, mode)
    }

    /// The digests of the files the writer read whole, front to back: the source's bytes.
    pub(crate) fn into_digests(self) -> HashMap<String, Digest> {
        self.done
    }
}

/// The expected manifest of `tree`: digests from `known` where it has them, the rest read now.
pub(crate) fn expected_from(
    tree: &mut dyn SourceTree,
    known: &HashMap<String, Digest>,
    ctx: &Ctx,
    mode: Mode,
) -> anyhow::Result<Expected> {
    let files = tree.files().to_vec();
    let empty_dirs = tree.empty_dirs().to_vec();
    let missing: Vec<&SourceFile> = files
        .iter()
        .filter(|f| f.size > 0 && !known.contains_key(&f.path))
        .collect();
    if !missing.is_empty() {
        ctx.log(format!(
            "hashing {} files not read whole, front to back, during the write",
            missing.len()
        ));
    }
    let total: u64 = missing.iter().map(|f| f.size).sum();
    // This re-hash, then the read-back.
    ctx.expect_rest(total.saturating_add(summary(&files, mode).checked_bytes));
    let mut progress = 0u64;
    let mut digests = HashMap::new();
    for f in missing {
        let mut digester = Digester::new();
        read_each(tree, &f.path, 0, f.size, ctx, &mut |bytes| {
            digester.update(bytes);
            progress += bytes.len() as u64;
            ctx.progress("verify", progress, total);
        })?;
        digests.insert(f.path.clone(), digester.finish());
    }
    let empty = Digester::new().finish();
    let digests = files
        .iter()
        .map(|f| {
            digests
                .get(&f.path)
                .or_else(|| known.get(&f.path))
                .unwrap_or(&empty)
                .clone()
        })
        .collect();
    Ok(Expected {
        files,
        digests,
        empty_dirs,
    })
}

impl SourceTree for HashingTree<'_> {
    fn files(&self) -> &[SourceFile] {
        self.inner.files()
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let bytes = self.inner.read(path)?;
        self.observe(path, 0, &bytes);
        Ok(bytes)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let bytes = self.inner.read_range(path, offset, len)?;
        self.observe(path, offset, &bytes);
        Ok(bytes)
    }

    fn empty_dirs(&self) -> &[String] {
        self.inner.empty_dirs()
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// Feeds `len` bytes of `path` from `offset` to `each`, read in bounded chunks: a short read
/// is continued, one that is empty (the file ended early) or longer than asked fails.
fn read_each(
    tree: &mut dyn SourceTree,
    path: &str,
    offset: u64,
    len: u64,
    ctx: &Ctx,
    each: &mut dyn FnMut(&[u8]),
) -> anyhow::Result<()> {
    let end = offset.saturating_add(len);
    let mut at = offset;
    while at < end {
        ctx.check()?;
        let want = (end - at).min(CHUNK) as usize;
        let buf = tree.read_range(path, at, want)?;
        if buf.is_empty() {
            bail!("{path}: ended at byte {at} of {end}");
        }
        if buf.len() > want {
            bail!(
                "{path}: {} bytes read at byte {at}, {want} asked for",
                buf.len()
            );
        }
        each(&buf);
        at += buf.len() as u64;
    }
    Ok(())
}

/// BLAKE3 of `len` bytes of `path` from `offset`. `step` gets the bytes of each chunk.
fn hash_range(
    tree: &mut dyn SourceTree,
    path: &str,
    offset: u64,
    len: u64,
    ctx: &Ctx,
    step: &mut dyn FnMut(u64),
) -> anyhow::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    read_each(tree, path, offset, len, ctx, &mut |bytes| {
        hasher.update(bytes);
        step(bytes.len() as u64);
    })?;
    Ok(hasher.finalize())
}

/// Hashes all of `f` in `output` slice by slice; the first slice that differs from `want`.
fn first_bad_slice(
    output: &mut dyn SourceTree,
    f: &SourceFile,
    want: &Digest,
    ctx: &Ctx,
    step: &mut dyn FnMut(u64),
) -> anyhow::Result<Option<usize>> {
    let mut digester = Digester::new();
    read_each(output, &f.path, 0, f.size, ctx, &mut |bytes| {
        digester.update(bytes);
        step(bytes.len() as u64);
    })?;
    let got = digester.finish().slices;
    let n = got.len().max(want.slices.len());
    Ok((0..n).find(|&i| got.get(i) != want.slices.get(i)))
}

/// One range fast mode compares: a whole file (`slice` None) or one of its slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    file: usize,
    slice: Option<usize>,
    offset: u64,
    len: u64,
}

/// Slice `s` of `files[file]`.
fn slice(files: &[SourceFile], file: usize, s: u64) -> Sample {
    let offset = s * SLICE;
    Sample {
        file,
        slice: Some(s as usize),
        offset,
        len: SLICE.min(files[file].size - offset),
    }
}

/// Fast mode's sample of `files` (policy 1), the same for the same files and seed:
/// - files of 16 MiB or less whole;
/// - of each larger file its first and last slice, the one holding [`UFS2_SEAM`] when `seam`
///   and the file reaches past it, and one random interior slice, no slice twice;
/// - then random slices of the rest, each as likely, without replacement: min(1 GiB, 1% of
///   the rest), rounded up to whole slices. Mandatory reads can be far more (many small
///   files); the budget is for the random part only.
///
/// Sorted by path and offset, so the output is read front to back.
fn plan(files: &[SourceFile], seed: u64, seam: bool) -> Vec<Sample> {
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|&a, &b| files[a].path.cmp(&files[b].path));
    let mut rng = SplitMix64(seed);
    let mut picked = Vec::new();
    // (file, slice) of every slice not picked yet.
    let mut pool: Vec<(usize, u64)> = Vec::new();
    for i in order {
        let size = files[i].size;
        if size == 0 {
            continue; // nothing to read; the size check covered it
        }
        if size <= WHOLE_MAX {
            picked.push(Sample {
                file: i,
                slice: None,
                offset: 0,
                len: size,
            });
            continue;
        }
        let last = (size - 1) / SLICE;
        let mut must = vec![0, last];
        if seam && size > UFS2_SEAM {
            must.push(UFS2_SEAM / SLICE);
        }
        // More than two slices, and the seam is slice 64: an interior one is always left.
        let start = pool.len();
        pool.extend((1..last).filter(|s| !must.contains(s)).map(|s| (i, s)));
        let one = start + rng.below(pool.len() - start);
        must.push(pool.swap_remove(one).1);
        must.sort_unstable();
        must.dedup();
        picked.extend(must.into_iter().map(|s| slice(files, i, s)));
    }
    let rest = pool.len() as u64 * SLICE; // interior slices are all whole
    let random = (BUDGET_MAX.min(rest / 100).div_ceil(SLICE) as usize).min(pool.len());
    for _ in 0..random {
        let (i, s) = pool.swap_remove(rng.below(pool.len()));
        picked.push(slice(files, i, s));
    }
    picked.sort_by(|a, b| (&files[a.file].path, a.offset).cmp(&(&files[b.file].path, b.offset)));
    picked
}

/// SplitMix64: small, and the same on every platform for the same seed.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`, `n > 0`.
    fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next()) * n as u128) >> 64) as usize
    }
}

/// What `mode` reads back of `files`.
pub(crate) fn summary(files: &[SourceFile], mode: Mode) -> VerifySummary {
    let total = files.iter().fold(0u64, |sum, f| sum.saturating_add(f.size));
    match mode {
        Mode::Full => VerifySummary {
            mode: VerifyMode::Full,
            checked_bytes: total,
            total_bytes: total,
            samples: 0,
            seed: 0,
        },
        Mode::Fast { seed, seam } => {
            let plan = plan(files, seed, seam);
            VerifySummary {
                mode: VerifyMode::Fast,
                checked_bytes: plan.iter().map(|s| s.len).sum(),
                total_bytes: total,
                samples: plan.len() as u64,
                seed,
            }
        }
    }
}

/// `verify: fast, 2.1 GiB of 80.9 GiB in 41 samples (seed 7), policy 1`.
fn summary_line(s: &VerifySummary) -> String {
    let (checked, total) = (shown(s.checked_bytes), shown(s.total_bytes));
    match s.mode {
        VerifyMode::Full => format!("verify: full, {checked} of {total}"),
        VerifyMode::Fast => format!(
            "verify: fast, {checked} of {total} in {} samples (seed {}), policy {POLICY}",
            s.samples, s.seed
        ),
    }
}

fn shown(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    match bytes {
        b if b >= 1024 * MIB => format!("{:.1} GiB", b as f64 / (1024 * MIB) as f64),
        b if b >= MIB => format!("{:.1} MiB", b as f64 / MIB as f64),
        b => format!("{b} bytes"),
    }
}

/// Compares `output` with `expected`; returns one line per check that passed, or fails
/// listing every difference. Every reader reports its empty directories. The structural
/// checks run in both modes; `mode` picks how much content is read back.
pub(crate) fn compare(
    expected: &Expected,
    output: &mut dyn SourceTree,
    ctx: &Ctx,
    mode: Mode,
) -> anyhow::Result<Vec<String>> {
    let mut checks = Vec::new();
    let got: HashMap<&str, u64> = output
        .files()
        .iter()
        .map(|f| (f.path.as_str(), f.size))
        .collect();
    let mut missing = Vec::new();
    let mut sizes = Vec::new();
    for f in &expected.files {
        match got.get(f.path.as_str()) {
            None => missing.push(f.path.clone()),
            Some(&size) if size != f.size => {
                sizes.push(format!("{} ({} bytes, expected {})", f.path, size, f.size));
            }
            Some(_) => {}
        }
    }
    let want: BTreeSet<&str> = expected.files.iter().map(|f| f.path.as_str()).collect();
    let mut extra: Vec<String> = got
        .keys()
        .filter(|p| !want.contains(*p))
        .map(|p| p.to_string())
        .collect();
    extra.sort();

    let want_dirs: BTreeSet<&str> = expected.empty_dirs.iter().map(String::as_str).collect();
    let got_dirs: BTreeSet<&str> = output.empty_dirs().iter().map(String::as_str).collect();
    let missing_dirs: Vec<String> = want_dirs
        .difference(&got_dirs)
        .map(|d| d.to_string())
        .collect();
    let extra_dirs: Vec<String> = got_dirs
        .difference(&want_dirs)
        .map(|d| d.to_string())
        .collect();

    let mut problems = listing("missing", &missing);
    problems.extend(listing("unexpected", &extra));
    problems.extend(listing("wrong size", &sizes));
    problems.extend(listing("missing empty dir", &missing_dirs));
    problems.extend(listing("unexpected empty dir", &extra_dirs));
    if !problems.is_empty() {
        bail!("verification failed:\n{}", problems.join("\n"));
    }
    checks.push(format!(
        "manifest: {} files, same paths as the source",
        expected.files.len()
    ));
    checks.push(format!("empty dirs: {} match", want_dirs.len()));
    let total = expected
        .files
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    checks.push(format!(
        "sizes: {total} bytes in {} files match",
        expected.files.len()
    ));

    let summary = summary(&expected.files, mode);
    ctx.log(summary_line(&summary));
    let read = summary.checked_bytes;
    ctx.expect_rest(read);
    let mut done = 0u64;
    let mut step = |n| {
        done += n;
        ctx.progress("verify", done, read);
    };
    let mut wrong = Vec::new();
    // Full mode reads every file whole, fast mode its sample; both compare slice hashes.
    let samples = match mode {
        Mode::Full => (0..expected.files.len())
            .map(|file| Sample {
                file,
                slice: None,
                offset: 0,
                len: expected.files[file].size,
            })
            .collect(),
        Mode::Fast { seed, seam } => plan(&expected.files, seed, seam),
    };
    for s in samples {
        let (f, digest) = (&expected.files[s.file], &expected.digests[s.file]);
        let bad = match s.slice {
            None => first_bad_slice(output, f, digest, ctx, &mut step)?
                .map(|i| slice(&expected.files, s.file, i as u64)),
            Some(i) => {
                let got = hash_range(output, &f.path, s.offset, s.len, ctx, &mut step)?;
                (digest.slices.get(i) != Some(&got)).then_some(s)
            }
        };
        if let Some(bad) = bad {
            wrong.push(if f.size <= SLICE {
                f.path.clone()
            } else {
                format!(
                    "{} (bytes {}..{})",
                    f.path,
                    bad.offset,
                    bad.offset + bad.len
                )
            });
        }
    }
    if !wrong.is_empty() {
        bail!(
            "verification failed:\n{}",
            listing("content differs (BLAKE3)", &wrong).join("\n")
        );
    }
    checks.push(match mode {
        Mode::Full => format!("blake3: {} files match the source", expected.files.len()),
        Mode::Fast { .. } => format!(
            "blake3: {} samples, {} of {total} bytes in {} files, match the source (fast, seed \
             {}, policy {POLICY})",
            summary.samples,
            summary.checked_bytes,
            expected.files.len(),
            summary.seed
        ),
    });
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::Event;

    /// An in-memory tree whose reads can come back short (`piece`), stop early (`cut`) or
    /// return a byte more than asked (`oversize`).
    struct Mem {
        files: Vec<SourceFile>,
        data: Vec<Vec<u8>>,
        piece: usize,
        cut: Option<usize>,
        oversize: bool,
    }

    impl Mem {
        fn new(files: &[(&str, Vec<u8>)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(p, d)| SourceFile {
                        path: p.to_string(),
                        size: d.len() as u64,
                    })
                    .collect(),
                data: files.iter().map(|(_, d)| d.clone()).collect(),
                piece: usize::MAX,
                cut: None,
                oversize: false,
            }
        }

        fn bytes(&mut self, path: &str) -> &mut Vec<u8> {
            let i = self.files.iter().position(|f| f.path == path).unwrap();
            &mut self.data[i]
        }
    }

    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.files
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(self.bytes(path).clone())
        }
        fn read_range(
            &mut self,
            path: &str,
            offset: u64,
            len: usize,
        ) -> ps5upload_fpkg::Result<Vec<u8>> {
            let (piece, cut, oversize) = (self.piece, self.cut, self.oversize);
            let data = self.bytes(path);
            let end = data.len().min(cut.unwrap_or(usize::MAX));
            let start = (offset as usize).min(end);
            let n = if oversize { len + 1 } else { len.min(piece) };
            Ok(data[start..(start + n).min(end)].to_vec())
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    fn with_ctx<T>(f: impl FnOnce(&Ctx) -> T) -> T {
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(|_| {});
        let cancel = AtomicBool::new(false);
        f(&Ctx::new(1, &emit, &cancel))
    }

    fn bytes(len: u64, salt: u8) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8 ^ salt).collect()
    }

    fn reference(data: &[u8]) -> Digest {
        Digest {
            slices: data.chunks(SLICE as usize).map(blake3::hash).collect(),
        }
    }

    /// Reads `path` through `tree` front to back in pieces of the sizes in `pieces`, cycled.
    fn feed(tree: &mut HashingTree, path: &str, size: u64, pieces: &[usize]) {
        let mut at = 0u64;
        for &n in pieces.iter().cycle() {
            if at >= size {
                break;
            }
            at += tree.read_range(path, at, n).unwrap().len() as u64;
        }
    }

    fn file(path: &str, size: u64) -> SourceFile {
        SourceFile {
            path: path.into(),
            size,
        }
    }

    #[test]
    fn slices_do_not_depend_on_read_sizes() {
        let (tail, exact) = (bytes(2 * SLICE + 5, 1), bytes(2 * SLICE, 2));
        let mut mem = Mem::new(&[
            ("tail", tail.clone()),
            ("exact", exact.clone()),
            ("tiny", b"abc".to_vec()),
        ]);
        let mut tree = HashingTree::new(&mut mem);
        // Across a slice boundary by a byte, then odd sizes.
        let odd = [SLICE as usize - 1, 2, (3 << 20) + 1];
        feed(&mut tree, "tail", tail.len() as u64, &odd);
        feed(&mut tree, "exact", exact.len() as u64, &[1 << 20]);
        tree.read("tiny").unwrap();
        let done = tree.into_digests();
        assert_eq!(done["tail"], reference(&tail));
        assert_eq!(done["tail"].slices.len(), 3, "a short last slice");
        assert_eq!(done["exact"], reference(&exact));
        assert_eq!(
            done["exact"].slices.len(),
            2,
            "no empty slice after an exact multiple"
        );
        assert_eq!(done["tiny"], reference(b"abc"));
    }

    #[test]
    fn files_not_read_front_to_back_are_hashed_again() {
        let (a, b) = (bytes(2 * SLICE + 5, 3), bytes(SLICE + 1, 4));
        let mut mem = Mem::new(&[("a", a.clone()), ("b", b.clone()), ("zero", Vec::new())]);
        let mut tree = HashingTree::new(&mut mem);
        // A gap invalidates `a`; `b` is read only in part.
        tree.read_range("a", 0, 1 << 20).unwrap();
        tree.read_range("a", 2 << 20, 1 << 20).unwrap();
        feed(&mut tree, "a", a.len() as u64, &[1 << 20]);
        tree.read_range("b", 0, SLICE as usize).unwrap();
        let expected = with_ctx(|ctx| tree.expected(ctx, Mode::Full)).unwrap();
        assert_eq!(
            expected.digests,
            [reference(&a), reference(&b), reference(&[])]
        );
    }

    #[test]
    fn the_plan_has_every_mandatory_slice_and_its_budget() {
        let big = 80 * 1024 * SLICE / 8; // 80 GiB: 10240 slices
        let mut files = vec![
            file("z/big", big),
            file("seam", (600 << 20) + 5),
            file("empty", 0),
        ];
        files.extend((0..300).map(|i| file(&format!("small/{i:03}"), 100 << 10)));
        let plan = plan(&files, 42, true);
        assert_eq!(plan, super::plan(&files, 42, true), "seeded");
        assert_ne!(plan, super::plan(&files, 43, true));
        let key = |s: &Sample| (files[s.file].path.clone(), s.offset);
        assert!(
            plan.windows(2).all(|w| key(&w[0]) < key(&w[1])),
            "sorted, no repeats"
        );
        let whole: Vec<&Sample> = plan.iter().filter(|s| s.slice.is_none()).collect();
        assert_eq!(whole.len(), 300);
        assert!(whole.iter().all(|s| s.len == 100 << 10));
        for (i, last) in [(0, 10239), (1, 75)] {
            let of: Vec<usize> = plan
                .iter()
                .filter(|s| s.file == i)
                .filter_map(|s| s.slice)
                .collect();
            for want in [0, last, 64] {
                assert!(of.contains(&want), "{want} not in {of:?}");
            }
            assert!(
                of.iter().any(|s| ![0, last, 64].contains(s)),
                "an interior slice"
            );
        }
        // Mandatory: 4 slices of each; the rest is 10236 + 72 slices, 1% of which rounds
        // up to 104 slices.
        assert_eq!(plan.len(), 300 + 8 + 104);
        let tail = plan
            .iter()
            .find(|s| s.file == 1 && s.slice == Some(75))
            .unwrap();
        assert_eq!(tail.len, 5);
    }

    #[test]
    fn mandatory_reads_can_exceed_the_budget() {
        let mut files: Vec<SourceFile> =
            (0..2000).map(|i| file(&format!("f{i}"), 1 << 20)).collect();
        files.push(file("big", 5 * SLICE)); // 0, 4 and an interior one; of 2 left, 1 at random
        let s = summary(
            &files,
            Mode::Fast {
                seed: 1,
                seam: false,
            },
        );
        assert_eq!(s.samples, 2000 + 4);
        assert_eq!(s.checked_bytes, (2000 << 20) + 4 * SLICE);
        assert!(s.checked_bytes > BUDGET_MAX);
        assert_eq!(s.total_bytes, (2000 << 20) + 5 * SLICE);
        let full = summary(&files, Mode::Full);
        assert_eq!(
            (full.checked_bytes, full.samples, full.seed),
            (s.total_bytes, 0, 0)
        );
    }

    #[test]
    fn fast_misses_only_what_it_does_not_sample() {
        let data = bytes(5 * SLICE + 3, 5); // 6 slices: 4 sampled, 2 not
        let small = bytes(1000, 6);
        let mut source = Mem::new(&[("big", data.clone()), ("small", small.clone())]);
        let mut tree = HashingTree::new(&mut source);
        feed(&mut tree, "big", data.len() as u64, &[(1 << 20) + 7]);
        tree.read("small").unwrap();
        let expected = with_ctx(|ctx| tree.expected(ctx, Mode::Full)).unwrap();
        let fast = Mode::Fast {
            seed: 7,
            seam: false,
        };
        let sampled: Vec<usize> = plan(&expected.files, 7, false)
            .iter()
            .filter_map(|s| s.slice)
            .collect();
        assert_eq!(sampled.len(), 4, "{sampled:?}");
        let unsampled = (0..6).find(|s| !sampled.contains(s)).unwrap();
        let damaged = |at: usize, path: &str| {
            let mut out = Mem::new(&[("big", data.clone()), ("small", small.clone())]);
            out.piece = (1 << 20) + 3; // short reads on the way back
            out.bytes(path)[at] ^= 1;
            out
        };
        let check = |mut out: Mem, mode| with_ctx(|ctx| compare(&expected, &mut out, ctx, mode));

        let lines = check(damaged(unsampled * SLICE as usize + 9, "big"), fast).unwrap();
        assert!(
            lines.last().unwrap().starts_with("blake3: 5 samples"),
            "{lines:?}"
        );
        let err = check(damaged(unsampled * SLICE as usize + 9, "big"), Mode::Full).unwrap_err();
        assert!(
            err.to_string().contains("content differs (BLAKE3): big"),
            "{err}"
        );

        let at = sampled[2] * SLICE as usize;
        let err = check(damaged(at + 1, "big"), fast).unwrap_err().to_string();
        let range = format!("big (bytes {at}..{})", at + SLICE as usize);
        assert!(err.contains(&range), "{err}");
        let err = check(damaged(999, "small"), fast).unwrap_err().to_string();
        assert!(err.contains("content differs (BLAKE3): small"), "{err}");
    }

    #[test]
    fn full_catches_a_flipped_byte_in_any_slice() {
        let data = bytes(3 * SLICE + 5, 8); // 4 slices, the last 5 bytes
        let small = bytes(SLICE, 9); // one whole slice
        let mut source = Mem::new(&[("big", data.clone()), ("small", small.clone())]);
        let mut tree = HashingTree::new(&mut source);
        feed(&mut tree, "big", data.len() as u64, &[(3 << 20) + 1]);
        feed(&mut tree, "small", small.len() as u64, &[1 << 20]);
        let expected = with_ctx(|ctx| tree.expected(ctx, Mode::Full)).unwrap();
        let check = |path: &str, at: usize| {
            let mut out = Mem::new(&[("big", data.clone()), ("small", small.clone())]);
            out.piece = (2 << 20) + 9;
            out.bytes(path)[at] ^= 0x80;
            with_ctx(|ctx| compare(&expected, &mut out, ctx, Mode::Full))
                .unwrap_err()
                .to_string()
        };
        let s = SLICE as usize;
        for (at, range) in [
            (0, (0, s)),
            (s - 1, (0, s)),
            (s, (s, 2 * s)),
            (2 * s + 12345, (2 * s, 3 * s)),
            (3 * s, (3 * s, 3 * s + 5)),
            (3 * s + 4, (3 * s, 3 * s + 5)),
        ] {
            let err = check("big", at);
            let want = format!(
                "content differs (BLAKE3): big (bytes {}..{})",
                range.0, range.1
            );
            assert!(err.contains(&want), "byte {at}: {err}");
        }
        let err = check("small", s - 1);
        assert!(err.contains("content differs (BLAKE3): small"), "{err}");
        let mut same = Mem::new(&[("big", data.clone()), ("small", small.clone())]);
        let lines = with_ctx(|ctx| compare(&expected, &mut same, ctx, Mode::Full)).unwrap();
        assert_eq!(lines.last().unwrap(), "blake3: 2 files match the source");
    }

    #[test]
    fn range_reads_stop_at_the_end_and_refuse_too_much() {
        let mut mem = Mem::new(&[("f", bytes(100, 0))]);
        mem.piece = 7;
        let read = |mem: &mut Mem, offset, len| {
            with_ctx(|ctx| hash_range(mem, "f", offset, len, ctx, &mut |_| {}))
        };
        assert_eq!(
            read(&mut mem, 10, 50).unwrap(),
            blake3::hash(&bytes(100, 0)[10..60])
        );
        mem.cut = Some(40);
        let err = read(&mut mem, 10, 50).unwrap_err().to_string();
        assert!(err.contains("f: ended at byte 40 of 60"), "{err}");
        mem.cut = None;
        mem.oversize = true;
        let err = read(&mut mem, 10, 50).unwrap_err().to_string();
        assert!(
            err.contains("51 bytes read at byte 10, 50 asked for"),
            "{err}"
        );
    }
}
