//! A pack's (or compressed image's) size before the job runs: the source opened as a job opens
//! it, the files the built-in strategy packs, evenly spread chunks of them compressed with the
//! real encoder (about 64 MiB and 10 s at most), and the rest extrapolated by size. Reads only.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::{Duration, Instant};

use anyhow::bail;
use ps5upload_fpkg::kraken;
use ps5upload_fpkg::source::{SourceFile, SourceTree};
use serde::{Deserialize, Serialize};

use crate::convert::{Kind, SourceStamp, open_source, refuse_interrupted_edit};
use crate::jobs::Ctx;
use crate::{ArchivePassword, ConvertRequest, Lz4Mode};

/// zstd or LZ4 packs (the packed set), or a whole tree: Kraken (fpkg) or zlib (ffpfsc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EstimateKind {
    Zstd,
    Lz4,
    Kraken,
    Zlib,
}

/// [`pack_estimate`]'s answer, in bytes of file data (no image or pack metadata).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackEstimate {
    /// Every file of the source, packs decoded.
    pub logical_bytes: u64,
    /// What is compressed: the packed set, or every file for Kraken and zlib.
    pub packed_set_bytes: u64,
    pub loose_bytes: u64,
    /// The loose bytes plus the packed set as estimated once compressed.
    pub estimated_bytes: u64,
    /// The raw bytes actually compressed for the estimate.
    pub sampled_bytes: u64,
}

const BUDGET: u64 = 64 << 20;
const DEADLINE: Duration = Duration::from_secs(10);
/// Up to this many files (the largest) get at least one chunk each, within half the budget.
const MIN_ONE: usize = 2000;

/// `source` packed or compressed as `kind` at `level` (Kraken 0 stored, 1 fast, 2 balanced,
/// 3 smallest). `unpack`: Kraken or zlib of a packed game whose packs the build decodes.
pub fn pack_estimate(
    source: &Path,
    kind: EstimateKind,
    level: u8,
    password: Option<&ArchivePassword>,
    unpack: bool,
) -> anyhow::Result<PackEstimate> {
    let enc = Encoder::new(kind, level)?;
    let cancel = AtomicBool::new(false);
    let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
    let ctx = Ctx::new(0, &emit, &cancel);
    let mut tree = open(source, kind, password, unpack, &ctx)?;
    let files = tree.files().to_vec();
    let packed: Vec<SourceFile> = (files.iter())
        .filter(|f| f.size > 0 && enc.packs(&f.path, f.size))
        .cloned()
        .collect();
    let logical_bytes: u64 = files.iter().map(|f| f.size).sum();
    let packed_set_bytes: u64 = packed.iter().map(|f| f.size).sum();
    let loose_bytes = logical_bytes - packed_set_bytes;
    let (stored, sampled_bytes) = sample(tree.as_mut(), &packed, enc, &cancel)?;
    Ok(PackEstimate {
        logical_bytes,
        packed_set_bytes,
        loose_bytes,
        estimated_bytes: loose_bytes + stored,
        sampled_bytes,
    })
}

/// The source as a job reads it (a patch refused, packs decoded when the job would).
fn open(
    source: &Path,
    kind: EstimateKind,
    password: Option<&ArchivePassword>,
    unpack: bool,
    ctx: &Ctx,
) -> anyhow::Result<Box<dyn SourceTree>> {
    let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
        "source": source,
        "format": "folder",
        "output": "",
    }))?;
    req.archive_password = password.cloned();
    match kind {
        EstimateKind::Zstd => req.zstd_pack = true,
        EstimateKind::Lz4 => req.lz4 = Some(Lz4Mode::Pack),
        EstimateKind::Kraken | EstimateKind::Zlib if unpack => {
            req.lz4 = Some(Lz4Mode::Unpack);
            req.zstd_unpack = true;
        }
        EstimateKind::Kraken | EstimateKind::Zlib => {}
    }
    let source_kind = Kind::of(source)?;
    if source_kind == Kind::Pkg && ps5_dump_forge_fpkg::is_patch(source) {
        bail!(crate::preflight::PATCH_SOURCE);
    }
    refuse_interrupted_edit(source, source_kind)?;
    let tree = match source_kind {
        Kind::Archive(format) => {
            let stamp = SourceStamp::take(source, source_kind)?;
            crate::archive::open(&req, ctx, format, stamp)?.0
        }
        _ => open_source(source, source_kind, ctx.cancel)?,
    };
    Ok(crate::lz4::open(&req, tree, ctx)?.0)
}

#[derive(Debug, Clone, Copy)]
enum Encoder {
    Zstd(u8),
    Lz4(u8),
    Kraken(kraken::Level),
    Zlib(u32),
}

impl Encoder {
    fn new(kind: EstimateKind, level: u8) -> anyhow::Result<Self> {
        Ok(match kind {
            EstimateKind::Zstd if (1..=22).contains(&level) => Self::Zstd(level),
            EstimateKind::Zstd => bail!("zstd level {level} is not 1..=22"),
            EstimateKind::Lz4 => {
                ps5_dump_forge_lz4::writer::check_level(level)?;
                Self::Lz4(level)
            }
            EstimateKind::Kraken => Self::Kraken(match level {
                0 => kraken::Level::Stored,
                1 => kraken::Level::Fast,
                2 => kraken::Level::Balanced,
                3 => kraken::Level::Smallest,
                _ => bail!("Kraken level {level} is not 0..=3"),
            }),
            EstimateKind::Zlib if level <= 9 => Self::Zlib(level.into()),
            EstimateKind::Zlib => bail!("zlib level {level} is not 0..=9"),
        })
    }

    fn chunk(self) -> u64 {
        match self {
            Self::Zstd(_) => ps5_dump_forge_zstd::CHUNK_SIZE,
            Self::Lz4(_) | Self::Zlib(_) => 64 << 10,
            Self::Kraken(_) => kraken::BLOCK as u64,
        }
    }

    /// The built-in strategy for packs, every file for an image.
    fn packs(self, path: &str, size: u64) -> bool {
        match self {
            Self::Zstd(_) | Self::Lz4(_) => {
                !crate::zstd_pack::loose(path)
                    && ps5_dump_forge_lz4::select_everything(path, size, None).is_some()
            }
            Self::Kraken(_) | Self::Zlib(_) => true,
        }
    }

    fn stored(self, raw: Vec<u8>) -> anyhow::Result<u64> {
        let n = match self {
            Self::Zstd(level) => ps5_dump_forge_zstd::writer::stored_len(raw, level)?,
            Self::Lz4(level) => ps5_dump_forge_lz4::writer::stored_len(raw, level),
            Self::Kraken(kraken::Level::Stored) => raw.len(),
            Self::Kraken(level) => (kraken::encode_block_at(&raw, level).iter())
                .map(|h| h.bytes().len())
                .sum(),
            Self::Zlib(level) => ps5_dump_forge_pfs::block_stored_len(&raw, level)?,
        };
        Ok(n as u64)
    }
}

/// Chunks of each file to compress: all within [`BUDGET`]; else one each for the [`MIN_ONE`]
/// largest (within half the budget), the rest of it shared by size.
fn quotas(files: &[SourceFile], b: u64) -> Vec<u64> {
    let chunks: Vec<u64> = files.iter().map(|f| f.size.div_ceil(b)).collect();
    let total: u64 = files.iter().map(|f| f.size).sum();
    if total <= BUDGET {
        return chunks;
    }
    let mut q = vec![0u64; files.len()];
    let mut largest: Vec<usize> = (0..files.len()).collect();
    largest.sort_by_key(|&i| std::cmp::Reverse(files[i].size));
    let mut spent = 0u64;
    for &i in largest.iter().take(MIN_ONE) {
        let cost = files[i].size.min(b);
        if spent + cost > BUDGET / 2 {
            break;
        }
        q[i] = 1;
        spent += cost;
    }
    // Whole chunks of the budget left, shared by size.
    let left = BUDGET.saturating_sub(spent) / b;
    for (i, f) in files.iter().enumerate() {
        let share = (u128::from(left) * u128::from(f.size) / u128::from(total)) as u64;
        q[i] = (q[i] + share).min(chunks[i]);
    }
    q
}

/// `q` chunk indices spread evenly over `n` chunks, each at the middle of its stretch.
fn spread(q: u64, n: u64) -> impl Iterator<Item = u64> {
    (0..q).map(move |k| ((2 * u128::from(k) + 1) * u128::from(n) / (2 * u128::from(q))) as u64)
}

/// (Estimated stored bytes, raw bytes compressed). Chunks are read in the tree's file order on
/// this thread (an archive decodes front to back) and compressed on a pool.
fn sample(
    tree: &mut dyn SourceTree,
    files: &[SourceFile],
    enc: Encoder,
    cancel: &AtomicBool,
) -> anyhow::Result<(u64, u64)> {
    let b = enc.chunk();
    let q = quotas(files, b);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
    let started = Instant::now();
    let mut got = vec![(0u64, 0u64); files.len()];
    let (to_work, work) = sync_channel::<(usize, Vec<u8>)>(threads * 2);
    let work = Mutex::new(work);
    let (to_done, done) = std::sync::mpsc::channel::<anyhow::Result<(usize, u64, u64)>>();
    let read = std::thread::scope(|s| -> anyhow::Result<()> {
        for _ in 0..threads {
            let to_done = to_done.clone();
            let work = &work;
            s.spawn(move || worker(work, enc, &to_done, cancel));
        }
        drop(to_done);
        // A read that runs past the deadline (a solid archive decoding up to its offset) is cut
        // by the cancel flag the source checks; the watchdog ends as soon as feeding does.
        let (fed, watchdog) = std::sync::mpsc::channel::<()>();
        s.spawn(move || {
            let wait = watchdog.recv_timeout(DEADLINE + Duration::from_secs(2));
            if wait == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        let result = feed(tree, files, &q, b, started, cancel, &to_work);
        drop(fed);
        drop(to_work);
        result
    });
    read?;
    for r in done.iter() {
        let (i, raw, stored) = r?;
        got[i].0 += raw;
        got[i].1 += stored;
    }
    Ok(extrapolate(files, &got))
}

fn feed(
    tree: &mut dyn SourceTree,
    files: &[SourceFile],
    q: &[u64],
    b: u64,
    started: Instant,
    cancel: &AtomicBool,
    to_work: &std::sync::mpsc::SyncSender<(usize, Vec<u8>)>,
) -> anyhow::Result<()> {
    for (i, f) in files.iter().enumerate() {
        for j in spread(q[i], f.size.div_ceil(b)) {
            if started.elapsed() >= DEADLINE || cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }
            let off = j * b;
            let raw = match tree.read_range(&f.path, off, b.min(f.size - off) as usize) {
                Ok(raw) => raw,
                // Cut by the watchdog or a failed worker: sampling ends with what it has.
                Err(_) if cancel.load(std::sync::atomic::Ordering::Relaxed) => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            let _ = to_work.send((i, raw));
        }
    }
    Ok(())
}

fn worker(
    work: &Mutex<Receiver<(usize, Vec<u8>)>>,
    enc: Encoder,
    done: &std::sync::mpsc::Sender<anyhow::Result<(usize, u64, u64)>>,
    stop: &AtomicBool,
) {
    // Runs until the feeder drops its sender: a worker that quit early could leave the feeder
    // blocked on a full channel. A failure stops the feeder; its error is in the results.
    loop {
        // Its own statement: the lock is released before the chunk is compressed.
        let next = work.lock().unwrap_or_else(|p| p.into_inner()).recv();
        let Ok((i, raw)) = next else {
            return;
        };
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
        let len = raw.len() as u64;
        let r = enc.stored(raw).map(|stored| (i, len, stored));
        if r.is_err() {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let _ = done.send(r);
    }
}

/// (Estimated stored bytes, raw bytes sampled): each sampled file at its own ratio, the others
/// at the ratio of all samples (1 with none).
fn extrapolate(files: &[SourceFile], got: &[(u64, u64)]) -> (u64, u64) {
    let (raw, stored) = (got.iter()).fold((0u64, 0u64), |(r, s), g| (r + g.0, s + g.1));
    let at = |size: u64, r: u64, s: u64| (u128::from(size) * u128::from(s) / u128::from(r)) as u64;
    let mut estimate = 0u64;
    for (f, &(r, s)) in files.iter().zip(got) {
        estimate += match (r, raw) {
            (0, 0) => f.size,
            (0, _) => at(f.size, raw, stored),
            _ => at(f.size, r, s),
        };
    }
    (estimate, raw)
}
