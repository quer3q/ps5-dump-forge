//! Per-job verification: read the output back through its own reader and
//! compare paths, empty dirs, sizes and BLAKE3 hashes against what the source held.

use std::collections::{BTreeSet, HashMap};

use anyhow::bail;
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::extract::CHUNK;
use crate::jobs::Ctx;
use crate::preflight::listing;

/// What the output must hold: the source's files with their hashes, and its empty dirs.
pub(crate) struct Expected {
    pub files: Vec<(SourceFile, blake3::Hash)>,
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
    in_flight: HashMap<String, (u64, blake3::Hasher)>,
    done: HashMap<String, blake3::Hash>,
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
            self.in_flight
                .insert(path.to_string(), (0, blake3::Hasher::new()));
        }
        let Some((next, hasher)) = self.in_flight.get_mut(path) else {
            return;
        };
        if *next != offset {
            self.in_flight.remove(path);
            return;
        }
        hasher.update(bytes);
        *next += bytes.len() as u64;
        if *next >= size {
            let (next, hasher) = self.in_flight.remove(path).expect("present above");
            if next == size {
                self.done.insert(path.to_string(), hasher.finalize());
            }
        }
    }

    /// The expected manifest: hashes seen during the write, the rest read again now.
    pub(crate) fn expected(self, ctx: &Ctx) -> anyhow::Result<Expected> {
        expected_from(self.inner, &self.done, ctx)
    }

    /// The hashes of the files the writer read whole, front to back: the source's bytes.
    pub(crate) fn into_hashes(self) -> HashMap<String, blake3::Hash> {
        self.done
    }
}

/// The expected manifest of `tree`: hashes from `known` where it has them, the rest read now.
pub(crate) fn expected_from(
    tree: &mut dyn SourceTree,
    known: &HashMap<String, blake3::Hash>,
    ctx: &Ctx,
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
    // This re-hash, then the read-back of every file.
    let all = files.iter().fold(0u64, |sum, f| sum.saturating_add(f.size));
    ctx.expect_rest(total.saturating_add(all));
    let mut progress = 0u64;
    let mut hashes = HashMap::new();
    for f in missing {
        let hash = hash_file(tree, f, ctx, &mut |n| {
            progress += n;
            ctx.progress("verify", progress, total);
        })?;
        hashes.insert(f.path.clone(), hash);
    }
    let empty = blake3::hash(&[]);
    let files = files
        .into_iter()
        .map(|f| {
            let h = hashes
                .get(&f.path)
                .or_else(|| known.get(&f.path))
                .copied()
                .unwrap_or(empty);
            (f, h)
        })
        .collect();
    Ok(Expected { files, empty_dirs })
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

/// BLAKE3 of one file, read in bounded chunks. `step` gets the bytes of each chunk.
fn hash_file(
    tree: &mut dyn SourceTree,
    file: &SourceFile,
    ctx: &Ctx,
    step: &mut dyn FnMut(u64),
) -> anyhow::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    let mut offset = 0u64;
    while offset < file.size {
        ctx.check()?;
        let want = (file.size - offset).min(CHUNK) as usize;
        let buf = tree.read_range(&file.path, offset, want)?;
        if buf.is_empty() {
            bail!("{}: ended at byte {offset} of {}", file.path, file.size);
        }
        hasher.update(&buf);
        offset += buf.len() as u64;
        step(buf.len() as u64);
    }
    Ok(hasher.finalize())
}

/// Compares `output` with `expected`; returns one line per check that passed, or fails
/// listing every difference. Every reader reports its empty directories.
pub(crate) fn compare(
    expected: &Expected,
    output: &mut dyn SourceTree,
    ctx: &Ctx,
) -> anyhow::Result<Vec<String>> {
    let mut checks = Vec::new();
    let got: HashMap<&str, u64> = output
        .files()
        .iter()
        .map(|f| (f.path.as_str(), f.size))
        .collect();
    let mut missing = Vec::new();
    let mut sizes = Vec::new();
    for (f, _) in &expected.files {
        match got.get(f.path.as_str()) {
            None => missing.push(f.path.clone()),
            Some(&size) if size != f.size => {
                sizes.push(format!("{} ({} bytes, expected {})", f.path, size, f.size));
            }
            Some(_) => {}
        }
    }
    let want: BTreeSet<&str> = expected
        .files
        .iter()
        .map(|(f, _)| f.path.as_str())
        .collect();
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
        .fold(0u64, |sum, (f, _)| sum.saturating_add(f.size));
    checks.push(format!(
        "sizes: {total} bytes in {} files match",
        expected.files.len()
    ));

    ctx.expect_rest(total);
    let mut done = 0u64;
    let mut wrong = Vec::new();
    for (f, hash) in &expected.files {
        let got = hash_file(output, f, ctx, &mut |n| {
            done += n;
            ctx.progress("verify", done, total);
        })?;
        if got != *hash {
            wrong.push(f.path.clone());
        }
    }
    if !wrong.is_empty() {
        bail!(
            "verification failed:\n{}",
            listing("content differs (BLAKE3)", &wrong).join("\n")
        );
    }
    checks.push(format!(
        "blake3: {} files match the source",
        expected.files.len()
    ));
    Ok(checks)
}
