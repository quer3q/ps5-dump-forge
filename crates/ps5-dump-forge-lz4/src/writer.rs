//! The one-lane pack writer: packed files cut into blocks, each kept as raw LZ4 or RAW,
//! compressed on worker threads and written in strict order into `ampr_assets-NNN.pak` volumes
//! (dense placement, rolled below 4 GiB for FAT32); then the volume headers rewritten with the
//! final build ID, the manifest, the CRC sidecar and, when given, the runtime profile.
//!
//! Loose files are not written here (core extracts them); their records still go in the manifest.

use std::collections::{HashSet, VecDeque};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TrySendError};
use std::sync::{Arc, Mutex};

use ps5upload_fpkg::source::SourceTree;
use ps5upload_fpkg::{Error, Result};

use crate::format::*;
use crate::reader::{MAX_CHUNKS, MAX_FILES, MAX_PACKS, open_manifest};
use crate::{CRC_SIDECAR, MANIFEST, PROFILE, PackSpec, RuntimeProfile, volume_name};

/// Largest volume: 4 GiB − 64 KiB, so every volume fits FAT32.
pub const VOLUME_CAP: u64 = 0xFFFF_0000;
/// Forge's I/O page and payload alignment.
pub const PAGE: u64 = 65536;
const ALIGN: u64 = 64;
/// Pending blocks per worker before the oldest is waited on.
pub(crate) const DEPTH_PER_WORKER: usize = 4;
const BUILD_ID_DOMAIN: &[u8] = b"PS5-FORGE-AMPRPAK4\0";
static ZEROS: [u8; PAGE as usize] = [0; PAGE as usize];

fn bad(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

/// One manifest record, in final path index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    /// Relative to the game root.
    pub path: String,
    pub size: u64,
    /// `None`: a loose record (core writes the file itself).
    pub spec: Option<PackSpec>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackReport {
    /// Packed and loose records.
    pub packed: u64,
    pub loose: u64,
    pub volumes: u32,
    /// Raw bytes of the packed files, and what they take in the volumes (chunks only).
    pub raw_bytes: u64,
    pub stored_bytes: u64,
}

/// Where `pack` puts its files, all at the output root; core supplies safe creation and
/// durability. Each file is created once and finished; a volume is reopened once more, after
/// the build ID is known, to rewrite its 64-byte header, and finished again.
pub trait PackOutput {
    type W: Write + Seek;
    /// Creates `name`, which must not exist yet.
    fn create(&mut self, name: &str) -> Result<Self::W>;
    /// Opens `name`, created and finished by this run, for writing at offset 0.
    fn reopen(&mut self, name: &str) -> Result<Self::W>;
    /// Every byte of `name` is written: flush and sync it.
    fn finish(&mut self, name: &str, w: Self::W) -> Result<()>;
}

/// Refinement 3, upstream's thresholds: keep LZ4 only if it saves ≥ 64 B and ≥ 1%, or, when it
/// does not save a page, ≥ 8 KiB and ≥ 12.5%.
fn keep_lz4(raw: usize, compressed: usize) -> bool {
    if compressed >= raw {
        return false;
    }
    let (r, saved) = (raw as u64, (raw - compressed) as u64);
    if (compressed as u64).div_ceil(PAGE) >= r.div_ceil(PAGE) {
        saved >= 8192 && saved * 8 >= r
    } else {
        saved >= 64 && saved * 100 >= r
    }
}

/// Dense placement of `stored` bytes after `cur`: a chunk of a page or more starts on a page;
/// a smaller one is 64-aligned and moves to the next page only if it would cross one. Returns
/// the offset and the page flags of the actual placement.
fn place(cur: u64, stored: u64) -> (u64, u8) {
    let up = |v: u64, a: u64| v.div_ceil(a) * a;
    let off = if stored >= PAGE {
        up(cur, PAGE)
    } else {
        let o = up(cur, ALIGN);
        if o / PAGE != (o + stored - 1) / PAGE {
            up(o, PAGE)
        } else {
            o
        }
    };
    let mut flags = 0;
    if stored <= PAGE && off / PAGE == (off + stored - 1) / PAGE {
        flags |= CHUNK_PAGE_CONTAINED;
    }
    if off.is_multiple_of(PAGE) {
        flags |= CHUNK_PAGE_ALIGNED;
    }
    (off, flags)
}

/// Raw lengths of the blocks of a `size`-byte file.
fn blocks(size: u64, shift: u8) -> impl Iterator<Item = u64> {
    let b = 1u64 << shift;
    (0..size.div_ceil(b)).map(move |i| b.min(size - i * b))
}

/// Ruling 6: an upper bound of every byte `pack` writes plus every loose record's file. Each
/// chunk is charged its raw length in whole pages (a chunk adds at most that many pages to its
/// volume, whatever it compresses to), volumes are filled by that charge against the cap, each
/// adds its 64 KiB front region, and the metadata is counted exactly for that volume count.
/// `files` is the final path-index record set: the `ampr_emu.index` (AMPRIDX3) size for it is
/// included exactly. The caller adds only files outside it.
pub fn worst_case_bytes(files: &[PackFile], profile: bool) -> u64 {
    worst_case_with_cap(files, profile, VOLUME_CAP)
}

fn worst_case_with_cap(files: &[PackFile], profile: bool, cap: u64) -> u64 {
    let (mut total, mut chunks, mut volumes, mut strings) = (0u64, 0u64, 0u32, 0u64);
    let mut used: Option<u64> = None;
    for f in files {
        strings += (APP0.len() + f.path.len() + 1) as u64;
        let Some(spec) = f.spec else {
            total = total.saturating_add(f.size);
            continue;
        };
        for raw in blocks(f.size, spec.block_shift) {
            let charge = raw.div_ceil(PAGE) * PAGE;
            chunks += 1;
            if used.is_none_or(|u| u + charge > cap - PAGE) {
                volumes += 1;
                total = total.saturating_add(PAGE);
                used = Some(0);
            }
            used = used.map(|u| u + charge);
            total = total.saturating_add(charge);
        }
    }
    strings += (0..volumes)
        .map(|n| volume_name(n).len() as u64 + 1)
        .sum::<u64>();
    let manifest = PAK_HEADER as u64
        + files.len() as u64 * FILE_RECORD as u64
        + chunks * CHUNK_RECORD as u64
        + u64::from(volumes) * PACK_RECORD as u64
        + strings;
    let crc = CRC_HEADER as u64 + 4 * chunks;
    let cfg = if profile { CFG_SIZE as u64 } else { 0 };
    total
        .saturating_add(index_size(files))
        .saturating_add(manifest)
        .saturating_add(crc)
        .saturating_add(cfg)
}

/// Exact byte size of `ps5upload_fpkg::ampr_index::build` for these paths: header, 24-byte
/// records, NUL-terminated `/app0/` paths, padding to 16, then a power-of-two slot table.
fn index_size(files: &[PackFile]) -> u64 {
    let blob: u64 = files
        .iter()
        .map(|f| (APP0.len() + f.path.trim_start_matches('/').len() + 1) as u64)
        .sum();
    let n = files.len() as u64;
    let slots = (n.saturating_mul(2)).next_power_of_two().max(2);
    (48 + 24 * n + blob)
        .next_multiple_of(16)
        .saturating_add(slots.saturating_mul(16))
}

/// One block back from a worker: the CRC of its raw bytes and what is stored.
pub(crate) struct Encoded {
    pub(crate) raw_len: u64,
    pub(crate) crc: u32,
    pub(crate) codec: Codec,
    pub(crate) data: Vec<u8>,
}

/// What compresses one block (raw, store, LZ4 level); [`encode`] everywhere but in the tests
/// that inject another.
pub(crate) type Encoder = fn(Vec<u8>, bool, u8) -> Encoded;

type Job = (Vec<u8>, bool, u8, Sender<Encoded>);

/// The one encoding decision, used by `pack`, `measure`, `sample` and the regenerating tree
/// alike: LZ4 at `level` ([`crate::block`]) unless `store`, kept only when [`keep_lz4`] says
/// so, else RAW.
pub(crate) fn encode(raw: Vec<u8>, store: bool, level: u8) -> Encoded {
    let (raw_len, crc) = (raw.len() as u64, crc32(&raw));
    if !store
        && let Some(c) = crate::block::compress(&raw, level)
        && keep_lz4(raw.len(), c.len())
    {
        return Encoded {
            raw_len,
            crc,
            codec: Codec::Lz4,
            data: c,
        };
    }
    Encoded {
        raw_len,
        crc,
        codec: Codec::Raw,
        data: raw,
    }
}

/// Auto-loose's sample of `path` (`size` bytes, `block_shift` blocks): the blocks at `indices`
/// encoded exactly as `pack` would at `level` (LZ4 kept per [`keep_lz4`], else RAW).
/// `progress` gets the raw bytes sampled so far.
#[allow(clippy::too_many_arguments)]
pub fn sample<S: SourceTree + ?Sized>(
    src: &mut S,
    path: &str,
    size: u64,
    block_shift: u8,
    indices: &[u64],
    level: u8,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> Result<crate::Sample> {
    check_level(level)?;
    let b = 1u64 << block_shift;
    let mut s = crate::Sample::default();
    for &i in indices {
        check_cancel(cancel)?;
        let off = i * b;
        if off >= size {
            return Err(bad(format!("{path}: sample block {i} is past its end")));
        }
        let e = encode(read_block(src, path, off, b.min(size - off))?, false, level);
        s.raw_bytes += e.raw_len;
        s.stored_bytes += e.data.len() as u64;
        s.blocks += 1;
        s.raw_blocks += u64::from(e.codec == Codec::Raw);
        progress(s.raw_bytes);
    }
    Ok(s)
}

fn worker(queue: &Mutex<Receiver<Job>>) {
    loop {
        let job = queue.lock().unwrap_or_else(|p| p.into_inner()).recv();
        let Ok((raw, store, level, reply)) = job else {
            return;
        };
        // The reader may be gone after an error; nothing to do then.
        let _ = reply.send(encode(raw, store, level));
    }
}

/// An error unless `level` is one [`crate::block::valid_level`] accepts.
pub fn check_level(level: u8) -> Result<()> {
    if crate::block::valid_level(level) {
        return Ok(());
    }
    Err(bad(format!("LZ4 level {level} is not 0 or 3..=12")))
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(())
}

fn write_zeros(w: &mut impl Write, mut n: u64) -> Result<()> {
    while n > 0 {
        let k = n.min(PAGE);
        w.write_all(&ZEROS[..k as usize])?;
        n -= k;
    }
    Ok(())
}

fn write_file<O: PackOutput>(out: &mut O, name: &str, bytes: &[u8]) -> Result<()> {
    let mut w = out.create(name)?;
    w.write_all(bytes)?;
    out.finish(name, w)
}

/// Where the sink's volume bytes go: files (`pack`) or nowhere (`measure`). The sink decides
/// every offset; this only writes what it is told, in order.
trait Volumes {
    /// A new volume; its first page (header placeholder and front region) is zeros.
    fn open(&mut self, name: &str) -> Result<()>;
    /// `pad` zero bytes, then `data`.
    fn chunk(&mut self, pad: u64, data: &[u8]) -> Result<()>;
    /// `pad` zero bytes up to the volume's size, then it is finished.
    fn close(&mut self, name: &str, pad: u64) -> Result<()>;
}

/// `measure`: placement only.
struct Nowhere;

impl Volumes for Nowhere {
    fn open(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
    fn chunk(&mut self, _: u64, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn close(&mut self, _: &str, _: u64) -> Result<()> {
        Ok(())
    }
}

/// `pack`: the volumes as files of `out`, one open at a time.
struct Files<'a, O: PackOutput> {
    out: &'a mut O,
    open: Option<BufWriter<O::W>>,
}

impl<O: PackOutput> Volumes for Files<'_, O> {
    fn open(&mut self, name: &str) -> Result<()> {
        let mut w = BufWriter::with_capacity(1 << 20, self.out.create(name)?);
        w.write_all(&ZEROS)?;
        self.open = Some(w);
        Ok(())
    }
    fn chunk(&mut self, pad: u64, data: &[u8]) -> Result<()> {
        let w = self.open.as_mut().expect("a volume is open");
        write_zeros(w, pad)?;
        Ok(w.write_all(data)?)
    }
    fn close(&mut self, name: &str, pad: u64) -> Result<()> {
        let mut w = self.open.take().expect("a volume is open");
        write_zeros(&mut w, pad)?;
        let w = w.into_inner().map_err(|e| Error::Io(e.into_error()))?;
        self.out.finish(name, w)
    }
}

/// The open volume: its name, the end of its last chunk, and how many it holds.
struct OpenVolume {
    name: String,
    end: u64,
    chunks: u64,
}

/// The ordered half on the calling thread: placement, chunk records, CRCs, progress. The one
/// placement implementation: `pack` writes what it decides, `measure` only records it.
struct Sink<V: Volumes> {
    vols: V,
    cap: u64,
    open: Option<OpenVolume>,
    /// Name and size of every finished volume, by pack id.
    done: Vec<(String, u64)>,
    chunks: Vec<u8>,
    crcs: Vec<u8>,
    stored: u64,
    raw_done: u64,
    /// The LZ4 level every block is compressed at.
    level: u8,
}

impl<V: Volumes> Sink<V> {
    fn new(vols: V, cap: u64, chunk_count: u64, level: u8) -> Self {
        Self {
            vols,
            cap,
            level,
            open: None,
            done: Vec::new(),
            chunks: Vec::with_capacity(chunk_count as usize * CHUNK_RECORD),
            crcs: Vec::with_capacity(chunk_count as usize * 4),
            stored: 0,
            raw_done: 0,
        }
    }

    /// Places `e` after the last chunk, in a new volume if the current one would pass the cap.
    fn put(&mut self, e: Encoded, progress: &mut dyn FnMut(u64)) -> Result<()> {
        let s = e.data.len() as u64;
        let (off, flags) = loop {
            if self.open.is_none() {
                let id = self.done.len() as u32;
                if id >= MAX_PACKS {
                    return Err(bad(format!("the packs need more than {MAX_PACKS} volumes")));
                }
                let name = volume_name(id);
                self.vols.open(&name)?;
                self.open = Some(OpenVolume {
                    name,
                    end: PAGE,
                    chunks: 0,
                });
            }
            let v = self.open.as_mut().expect("opened above");
            let (off, flags) = place(v.end, s);
            if (off + s).div_ceil(PAGE) * PAGE <= self.cap {
                self.vols.chunk(off - v.end, &e.data)?;
                v.end = off + s;
                v.chunks += 1;
                break (off, flags);
            }
            if v.chunks == 0 {
                return Err(bad("a chunk does not fit an empty volume"));
            }
            self.close()?;
        };
        let loc = pack_location(self.done.len() as u16, off)?;
        self.chunks.extend_from_slice(&loc.to_le_bytes());
        self.chunks
            .extend_from_slice(&pack_descriptor(s as u32, e.codec, flags)?.to_le_bytes());
        self.crcs.extend_from_slice(&e.crc.to_le_bytes());
        self.stored += s;
        self.raw_done += e.raw_len;
        progress(self.raw_done);
        Ok(())
    }

    /// Pads the open volume to a whole page and finishes it.
    fn close(&mut self) -> Result<()> {
        let Some(v) = self.open.take() else {
            return Ok(());
        };
        let size = v.end.div_ceil(PAGE) * PAGE;
        self.vols.close(&v.name, size - v.end)?;
        self.done.push((v.name, size));
        Ok(())
    }
}

/// Reads every packed file front to back, block by block, hands the blocks to the workers and
/// puts the replies in order; at most `limit` blocks are pending.
fn feed<S: SourceTree + ?Sized, V: Volumes>(
    src: &mut S,
    files: &[PackFile],
    work: &mpsc::SyncSender<Job>,
    limit: usize,
    sink: &mut Sink<V>,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> Result<()> {
    let mut pending: VecDeque<Receiver<Encoded>> = VecDeque::with_capacity(limit);
    let mut oldest = |pending: &mut VecDeque<Receiver<Encoded>>, sink: &mut Sink<V>| {
        let r = pending.pop_front().expect("a block is pending");
        let e = r.recv().map_err(|_| bad("a compression worker failed"))?;
        check_cancel(cancel)?;
        sink.put(e, progress)
    };
    for f in files {
        let Some(spec) = f.spec else { continue };
        let mut off = 0;
        for raw in blocks(f.size, spec.block_shift) {
            check_cancel(cancel)?;
            while pending.len() >= limit {
                oldest(&mut pending, sink)?;
            }
            let data = read_block(src, &f.path, off, raw)?;
            off += raw;
            let (reply, result) = mpsc::channel();
            work.try_send((data, spec.store, sink.level, reply))
                .map_err(|e| match e {
                    TrySendError::Full(_) => bad("internal error: the compression queue is full"),
                    TrySendError::Disconnected(_) => bad("a compression worker failed"),
                })?;
            pending.push_back(result);
        }
    }
    while !pending.is_empty() {
        oldest(&mut pending, sink)?;
    }
    Ok(())
}

/// `raw` bytes of `path` at `off`, all of them.
pub(crate) fn read_block<S: SourceTree + ?Sized>(
    src: &mut S,
    path: &str,
    off: u64,
    raw: u64,
) -> Result<Vec<u8>> {
    let data = src.read_range(path, off, raw as usize)?;
    if data.len() as u64 != raw {
        return Err(bad(format!(
            "{path}: read {} of {raw} bytes at {off}; the file changed",
            data.len()
        )));
    }
    Ok(data)
}

/// Every packed block of `files` through `threads` workers into `sink`, in order.
fn compress<S: SourceTree + ?Sized, V: Volumes>(
    src: &mut S,
    files: &[PackFile],
    threads: usize,
    sink: &mut Sink<V>,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> Result<()> {
    let threads = threads.max(1);
    let limit = threads * DEPTH_PER_WORKER;
    let (fed, panicked) = std::thread::scope(|s| {
        let (work, queue) = mpsc::sync_channel::<Job>(limit);
        // Only the workers hold the queue: once they are all gone, a submit fails at once.
        let queue = Arc::new(Mutex::new(queue));
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                let queue = Arc::clone(&queue);
                s.spawn(move || worker(&queue))
            })
            .collect();
        drop(queue);
        let fed = feed(src, files, &work, limit, sink, progress, cancel);
        // Closes the queue: idle workers see it and exit.
        drop(work);
        // Every worker is joined, so none outlives the scope's borrows.
        let mut panicked = false;
        for w in workers {
            panicked |= w.join().is_err();
        }
        (fed, panicked)
    });
    if panicked {
        return Err(bad("a compression worker panicked"));
    }
    fed?;
    sink.close()?;
    check_cancel(cancel)
}

/// The logical path of every record and the chunk count, or why the list cannot be packed.
fn validate(files: &[PackFile]) -> Result<(Vec<String>, u64)> {
    let mut seen = HashSet::with_capacity(files.len());
    let mut logical = Vec::with_capacity(files.len());
    let mut chunk_count = 0u64;
    for f in files {
        let l = rel_to_logical(&f.path)?;
        if !seen.insert(l.to_ascii_lowercase()) {
            return Err(bad(format!("{} is listed twice", f.path)));
        }
        if let Some(s) = f.spec {
            if !(MIN_BLOCK_SHIFT..=MAX_BLOCK_SHIFT).contains(&s.block_shift) {
                return Err(bad(format!(
                    "{}: block shift {} is not 14..=20",
                    f.path, s.block_shift
                )));
            }
            chunk_count += f.size.div_ceil(1 << s.block_shift);
        }
        logical.push(l);
    }
    if files.len() as u64 > MAX_FILES || chunk_count > MAX_CHUNKS {
        return Err(bad(format!(
            "{} files in {chunk_count} chunks exceed the runtime's {MAX_FILES} files or \
             {MAX_CHUNKS} chunks",
            files.len()
        )));
    }
    Ok((logical, chunk_count))
}

/// Everything a pack run decides besides the chunk bytes: the volume names and sizes, the
/// build ID, and the manifest, CRC sidecar and runtime profile exactly as written. `pack`
/// writes it; `measure` returns it, and [`crate::tree::PackedTree`] serves the packs from it.
/// It holds no compressed data: the manifest's 12-byte chunk records and the sidecar's 4-byte
/// CRCs are all it keeps per chunk.
#[derive(Debug, Clone)]
pub struct Measured {
    pub build_id: [u8; 16],
    /// Name and size of every volume, by pack id.
    pub volumes: Vec<(String, u64)>,
    pub manifest: Vec<u8>,
    pub crc: Vec<u8>,
    pub profile: Option<Vec<u8>>,
    pub report: PackReport,
    /// The LZ4 level the chunks were compressed at; [`crate::tree::PackedTree`] uses it again.
    pub level: u8,
}

impl Measured {
    /// Every file `pack` writes, with its exact size: volumes, manifest, sidecar, profile.
    pub fn outputs(&self) -> Vec<(String, u64)> {
        let mut out = self.volumes.clone();
        out.push((MANIFEST.into(), self.manifest.len() as u64));
        out.push((CRC_SIDECAR.into(), self.crc.len() as u64));
        if let Some(p) = &self.profile {
            out.push((PROFILE.into(), p.len() as u64));
        }
        out
    }

    /// Bytes this keeps in memory (the metadata only).
    pub fn metadata_bytes(&self) -> usize {
        self.manifest.capacity()
            + self.crc.capacity()
            + self.profile.as_ref().map_or(0, Vec::capacity)
            + self
                .volumes
                .iter()
                .map(|v| v.0.capacity() + 32)
                .sum::<usize>()
    }

    /// The 64-byte AMPRDAT3 header of volume `id`, with the final build ID.
    pub fn volume_header(&self, id: usize) -> Result<[u8; DAT_HEADER]> {
        let size = self.volumes[id].1;
        let mut d = [0u8; DAT_HEADER];
        put_bytes(&mut d, 0, DAT_MAGIC)?;
        put_u32(&mut d, 8, DAT_VERSION)?;
        put_u32(&mut d, 12, DAT_HEADER as u32)?;
        put_u32(&mut d, 16, id as u32)?;
        put_u32(&mut d, 20, PACK_IO_PAGE_LAYOUT)?;
        put_bytes(&mut d, 24, &self.build_id)?;
        put_u64(&mut d, 40, PAGE)?;
        put_u64(&mut d, 48, size - PAGE)?;
        let crc = dat_header_crc(&d)?;
        put_u32(&mut d, DAT_HEADER_CRC_AT, crc)?;
        Ok(d)
    }
}

/// The metadata for what the sink placed: records exactly as on disk, the build ID over them
/// (ruling 8), then the manifest (checked as the runtime checks it), sidecar and profile.
fn metadata<V: Volumes>(
    files: &[PackFile],
    logical: &[String],
    chunk_count: u64,
    sink: Sink<V>,
    mtime: i64,
    runtime: Option<&RuntimeProfile>,
) -> Result<(V, Measured)> {
    let Sink {
        vols,
        done: volumes,
        chunks,
        crcs,
        stored,
        raw_done,
        level,
        ..
    } = sink;
    // The string table holds the paths, then the volume names.
    let mut strings = Vec::new();
    let mut add_string = |s: &str| -> Result<(u32, u32)> {
        let at = u32::try_from(strings.len()).map_err(|_| bad("the string table is too large"))?;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        Ok((at, s.len() as u32))
    };
    let mut file_recs = vec![0u8; files.len() * FILE_RECORD];
    let (mut first, mut packed) = (0u32, 0u64);
    for (i, (f, l)) in files.iter().zip(logical).enumerate() {
        let r = &mut file_recs[i * FILE_RECORD..(i + 1) * FILE_RECORD];
        put_u64(r, 0, pak_hash(l.as_bytes()))?;
        put_u64(r, 8, f.size)?;
        put_i64(r, 16, mtime)?;
        let (at, len) = add_string(l)?;
        put_u32(r, 32, at)?;
        put_u32(r, 36, len)?;
        if let Some(s) = f.spec {
            let n = f.size.div_ceil(1 << s.block_shift) as u32;
            let flags = FILE_PACKED
                | if s.store { FILE_STORE_ONLY } else { 0 }
                | if s.hot { FILE_HOT } else { 0 }
                | if s.random { FILE_RANDOM } else { 0 };
            put_u32(r, 24, first)?;
            put_u32(r, 28, n)?;
            put_u32(r, 40, flags)?;
            r[44] = s.block_shift;
            first += n;
            packed += 1;
        }
    }
    let mut pack_recs = vec![0u8; volumes.len() * PACK_RECORD];
    for (i, (name, size)) in volumes.iter().enumerate() {
        let r = &mut pack_recs[i * PACK_RECORD..(i + 1) * PACK_RECORD];
        put_u64(r, 0, size - PAGE)?;
        put_u64(r, 8, *size)?;
        let (at, len) = add_string(name)?;
        put_u32(r, 16, at)?;
        put_u32(r, 20, len)?;
        put_u32(r, 24, PACK_IO_PAGE_LAYOUT)?;
        put_u32(r, 28, PAGE as u32)?;
    }
    // Ruling 8.
    let mut h = blake3::Hasher::new();
    for part in [
        BUILD_ID_DOMAIN,
        &file_recs,
        &chunks,
        &pack_recs,
        &strings,
        &crcs,
    ] {
        h.update(part);
    }
    let build_id: [u8; 16] = h.finalize().as_bytes()[..16].try_into().expect("16 bytes");

    let mut m = Vec::with_capacity(
        PAK_HEADER + file_recs.len() + chunks.len() + pack_recs.len() + strings.len(),
    );
    m.resize(PAK_HEADER, 0);
    put_bytes(&mut m, 0, PAK_MAGIC)?;
    put_u32(&mut m, 8, PAK_VERSION)?;
    put_u32(&mut m, 12, PAK_HEADER as u32)?;
    put_u32(&mut m, 20, ENDIAN_MARKER)?;
    put_bytes(&mut m, 24, &build_id)?;
    put_u64(&mut m, 40, files.len() as u64)?;
    put_u64(&mut m, 48, chunk_count)?;
    put_u32(&mut m, 56, volumes.len() as u32)?;
    put_u32(&mut m, 60, FILE_RECORD as u32)?;
    put_u32(&mut m, 64, CHUNK_RECORD as u32)?;
    put_u32(&mut m, 68, PACK_RECORD as u32)?;
    let mut at = PAK_HEADER as u64;
    for (off, part) in [
        (72, &file_recs),
        (80, &chunks),
        (88, &pack_recs),
        (96, &strings),
    ] {
        put_u64(&mut m, off, at)?;
        at += part.len() as u64;
    }
    put_u64(&mut m, 104, strings.len() as u64)?;
    m.extend_from_slice(&file_recs);
    drop(file_recs);
    m.extend_from_slice(&chunks);
    drop(chunks);
    m.extend_from_slice(&pack_recs);
    m.extend_from_slice(&strings);
    drop(strings);
    let crc = pak_payload_crc(&m)?;
    put_u32(&mut m, PAK_PAYLOAD_CRC_AT, crc)?;
    let crc = pak_header_crc(&m)?;
    put_u32(&mut m, PAK_HEADER_CRC_AT, crc)?;
    // Our own output passes the runtime's checks, or nothing more is written.
    open_manifest(&m).map_err(|e| bad(format!("internal error: the new manifest fails: {e}")))?;

    let mut c = Vec::with_capacity(CRC_HEADER + crcs.len());
    c.resize(CRC_HEADER, 0);
    put_bytes(&mut c, 0, CRC_MAGIC)?;
    put_u32(&mut c, 8, CRC_VERSION)?;
    put_u32(&mut c, 12, CRC_HEADER as u32)?;
    put_bytes(&mut c, 16, &build_id)?;
    put_u64(&mut c, 32, chunk_count)?;
    c.extend_from_slice(&crcs);
    drop(crcs);
    let crc = crc1_payload_crc(&c)?;
    put_u32(&mut c, CRC_PAYLOAD_CRC_AT, crc)?;
    let crc = crc1_header_crc(&c)?;
    put_u32(&mut c, CRC_HEADER_CRC_AT, crc)?;
    let profile = runtime.map(|p| profile_bytes(p, &build_id)).transpose()?;
    let report = PackReport {
        packed,
        loose: files.len() as u64 - packed,
        volumes: volumes.len() as u32,
        raw_bytes: raw_done,
        stored_bytes: stored,
    };
    let measured = Measured {
        build_id,
        volumes,
        manifest: m,
        crc: c,
        profile,
        report,
        level,
    };
    Ok((vols, measured))
}

/// Packs `files` (final path index order) from `src` into `out`: volumes, then their headers
/// rewritten with the build ID, then `ampr_assets.index`, `.crc` and, with `runtime`, `.runtime`.
/// `mtime` (integer seconds) goes in every record. `threads` workers compress (0 counts as 1)
/// at LZ4 `level` ([`crate::block::valid_level`]). `progress` gets the raw bytes packed so far.
#[allow(clippy::too_many_arguments)]
pub fn pack<S: SourceTree + ?Sized, O: PackOutput>(
    src: &mut S,
    files: &[PackFile],
    mtime: i64,
    runtime: Option<&RuntimeProfile>,
    threads: usize,
    level: u8,
    out: &mut O,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> Result<PackReport> {
    pack_with_cap(
        src, files, mtime, runtime, threads, level, out, progress, cancel, VOLUME_CAP,
    )
}

#[allow(clippy::too_many_arguments)]
fn pack_with_cap<S: SourceTree + ?Sized, O: PackOutput>(
    src: &mut S,
    files: &[PackFile],
    mtime: i64,
    runtime: Option<&RuntimeProfile>,
    threads: usize,
    level: u8,
    out: &mut O,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
    cap: u64,
) -> Result<PackReport> {
    check_cancel(cancel)?;
    check_level(level)?;
    let (logical, chunk_count) = validate(files)?;
    let vols = Files {
        out: &mut *out,
        open: None,
    };
    let mut sink = Sink::new(vols, cap, chunk_count, level);
    compress(src, files, threads, &mut sink, progress, cancel)?;
    let (_, m) = metadata(files, &logical, chunk_count, sink, mtime, runtime)?;
    for (id, (name, _)) in m.volumes.iter().enumerate() {
        check_cancel(cancel)?;
        let d = m.volume_header(id)?;
        let mut w = out.reopen(name)?;
        w.seek(SeekFrom::Start(0))?;
        w.write_all(&d)?;
        out.finish(name, w)?;
    }
    check_cancel(cancel)?;
    write_file(out, MANIFEST, &m.manifest)?;
    check_cancel(cancel)?;
    write_file(out, CRC_SIDECAR, &m.crc)?;
    check_cancel(cancel)?;
    if let Some(p) = &m.profile {
        write_file(out, PROFILE, p)?;
        check_cancel(cancel)?;
    }
    Ok(m.report)
}

/// `pack`'s first half without writing: the same reads, compression, RAW/LZ4 decisions,
/// placement and rollover, keeping only the metadata ([`Measured`]). With the same `src`
/// bytes and arguments, [`crate::tree::PackedTree`] then serves exactly what `pack` writes.
#[allow(clippy::too_many_arguments)]
pub fn measure<S: SourceTree + ?Sized>(
    src: &mut S,
    files: &[PackFile],
    mtime: i64,
    runtime: Option<&RuntimeProfile>,
    threads: usize,
    level: u8,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> Result<Measured> {
    measure_with_cap(
        src, files, mtime, runtime, threads, level, progress, cancel, VOLUME_CAP,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn measure_with_cap<S: SourceTree + ?Sized>(
    src: &mut S,
    files: &[PackFile],
    mtime: i64,
    runtime: Option<&RuntimeProfile>,
    threads: usize,
    level: u8,
    progress: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
    cap: u64,
) -> Result<Measured> {
    check_cancel(cancel)?;
    check_level(level)?;
    let (logical, chunk_count) = validate(files)?;
    let mut sink = Sink::new(Nowhere, cap, chunk_count, level);
    compress(src, files, threads, &mut sink, progress, cancel)?;
    Ok(metadata(files, &logical, chunk_count, sink, mtime, runtime)?.1)
}

/// AMPRCFG1 for `p`; the values were checked by the rules.
fn profile_bytes(p: &RuntimeProfile, build_id: &[u8; 16]) -> Result<Vec<u8>> {
    let mut b = vec![0u8; CFG_SIZE];
    put_bytes(&mut b, 0, CFG_MAGIC)?;
    put_u32(&mut b, 8, CFG_VERSION)?;
    put_u32(&mut b, 12, CFG_SIZE as u32)?;
    put_bytes(&mut b, 16, build_id)?;
    put_u64(&mut b, 32, p.decoded_cache_bytes)?;
    put_u64(&mut b, 40, p.physical_cache_bytes)?;
    put_u32(&mut b, 48, p.workers)?;
    put_u32(&mut b, 52, p.latency_reserve_workers)?;
    let crc = cfg_crc(&b)?;
    put_u32(&mut b, CFG_CRC_AT, crc)?;
    Ok(b)
}

/// In-memory tree and output for this crate's tests.
#[cfg(test)]
pub(crate) mod testkit {
    use std::collections::BTreeMap;
    use std::io::Cursor;

    use ps5upload_fpkg::source::{SourceFile, SourceTree};

    use super::*;

    pub(crate) struct Mem {
        files: Vec<SourceFile>,
        data: BTreeMap<String, Vec<u8>>,
        pub(crate) dirs: Vec<String>,
        /// Every `read_range`: path, offset, length asked.
        pub(crate) reads: Vec<(String, u64, usize)>,
    }

    impl Mem {
        pub(crate) fn new(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
            let data: BTreeMap<String, Vec<u8>> = entries.into_iter().collect();
            let files = data
                .iter()
                .map(|(p, d)| SourceFile {
                    path: p.clone(),
                    size: d.len() as u64,
                })
                .collect();
            Self {
                files,
                data,
                dirs: Vec::new(),
                reads: Vec::new(),
            }
        }
    }

    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.files
        }
        fn read(&mut self, path: &str) -> Result<Vec<u8>> {
            self.data
                .get(path)
                .cloned()
                .ok_or_else(|| bad(format!("no {path}")))
        }
        fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
            self.reads.push((path.to_string(), offset, len));
            let d = self
                .data
                .get(path)
                .ok_or_else(|| bad(format!("no {path}")))?;
            let start = (offset as usize).min(d.len());
            Ok(d[start..start.saturating_add(len).min(d.len())].to_vec())
        }
        fn empty_dirs(&self) -> &[String] {
            &self.dirs
        }
        fn describe(&self) -> String {
            "memory".into()
        }
    }

    #[derive(Default)]
    pub(crate) struct MemOut {
        pub(crate) files: BTreeMap<String, Vec<u8>>,
        /// Every call, in order: `create x`, `reopen x`, `finish x`.
        pub(crate) log: Vec<String>,
    }

    impl PackOutput for MemOut {
        type W = Cursor<Vec<u8>>;
        fn create(&mut self, name: &str) -> Result<Self::W> {
            self.log.push(format!("create {name}"));
            if self.files.contains_key(name) {
                return Err(bad(format!("{name} exists")));
            }
            Ok(Cursor::new(Vec::new()))
        }
        fn reopen(&mut self, name: &str) -> Result<Self::W> {
            self.log.push(format!("reopen {name}"));
            self.files
                .remove(name)
                .map(Cursor::new)
                .ok_or_else(|| bad(format!("no {name}")))
        }
        fn finish(&mut self, name: &str, w: Self::W) -> Result<()> {
            self.log.push(format!("finish {name}"));
            self.files.insert(name.to_string(), w.into_inner());
            Ok(())
        }
    }

    pub(crate) fn spec(block_shift: u8) -> Option<PackSpec> {
        Some(PackSpec {
            block_shift,
            store: false,
            hot: false,
            random: false,
        })
    }

    /// Deterministic incompressible bytes.
    pub(crate) fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    /// Compressible bytes: short text runs.
    pub(crate) fn text(len: usize, seed: u64) -> Vec<u8> {
        (0..len)
            .map(|i| b"forge packs assets "[(i + seed as usize) % 19])
            .collect()
    }

    /// The source tree, the pack list and the packed output for `entries`.
    pub(crate) fn pack_mem(
        entries: &[(&str, Vec<u8>, Option<PackSpec>)],
        runtime: Option<&RuntimeProfile>,
        threads: usize,
        cap: u64,
    ) -> Result<(Mem, Vec<PackFile>, MemOut, PackReport)> {
        let mut src = Mem::new(entries.iter().map(|(p, d, _)| (p.to_string(), d.clone())));
        let files: Vec<PackFile> = entries
            .iter()
            .map(|(p, d, s)| PackFile {
                path: p.to_string(),
                size: d.len() as u64,
                spec: *s,
            })
            .collect();
        let mut out = MemOut::default();
        let cancel = AtomicBool::new(false);
        let report = pack_with_cap(
            &mut src,
            &files,
            1_700_000_000,
            runtime,
            threads,
            0,
            &mut out,
            &mut |_| {},
            &cancel,
            cap,
        )?;
        Ok((src, files, out, report))
    }

    /// The tree a packed folder would be: loose records' files plus everything `pack` wrote.
    pub(crate) fn deployed(entries: &[(&str, Vec<u8>, Option<PackSpec>)], out: &MemOut) -> Mem {
        let loose = entries.iter().filter(|e| e.2.is_none());
        Mem::new(
            loose
                .map(|(p, d, _)| (p.to_string(), d.clone()))
                .chain(out.files.iter().map(|(p, d)| (p.clone(), d.clone()))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::reader::{Unpacked, unpack};

    fn open(entries: &[(&str, Vec<u8>, Option<PackSpec>)], out: &MemOut) -> Unpacked<Mem> {
        unpack(Box::new(deployed(entries, out))).unwrap()
    }

    /// Every file reads back whole and in ranges that cross block edges.
    fn assert_round_trip(entries: &[(&str, Vec<u8>, Option<PackSpec>)], out: &MemOut) {
        let mut u = open(entries, out);
        let mut want: Vec<(String, u64)> = entries
            .iter()
            .map(|(p, d, _)| (p.to_string(), d.len() as u64))
            .collect();
        want.sort();
        let got: Vec<(String, u64)> = u.files().iter().map(|f| (f.path.clone(), f.size)).collect();
        assert_eq!(got, want);
        for (p, d, s) in entries {
            assert_eq!(&u.read(p).unwrap(), d, "{p}");
            let b = 1usize << s.map_or(16, |s| s.block_shift);
            for (off, len) in [
                (0, 1),
                (b - 1, 2),
                (b / 2, b + 7),
                (d.len().saturating_sub(3), 9),
            ] {
                let end = (off + len).min(d.len());
                let want = d.get(off.min(end)..end).unwrap_or(&[]);
                assert_eq!(
                    u.read_range(p, off as u64, len).unwrap(),
                    want,
                    "{p} at {off}"
                );
            }
        }
    }

    #[test]
    fn rollover_continues_files_across_volumes() {
        let cap = PAGE + (2 << 20);
        let entries = vec![
            ("r/a", noise(5 << 20, 1), spec(16)),
            ("r/b", text(3 << 20, 2), spec(20)),
            ("r/c", noise(1 << 20, 3), spec(20)),
        ];
        let (_, _, out, report) = pack_mem(&entries, None, 4, cap).unwrap();
        assert!(report.volumes >= 4, "{report:?}");
        let u = open(&entries, &out);
        let m = u.manifest();
        let a = &m.files[0];
        let ids: HashSet<u16> = m.chunks[..a.chunk_count as usize]
            .iter()
            .map(|c| c.pack_id)
            .collect();
        assert!(ids.len() > 1, "r/a spans volumes");
        for (i, p) in m.packs.iter().enumerate() {
            assert_eq!(p.name, volume_name(i as u32));
            assert!(p.file_size <= cap && p.file_size.is_multiple_of(PAGE));
            assert_eq!(p.payload_offset(), PAGE);
            assert_eq!(out.files[&p.name].len() as u64, p.file_size);
        }
        // Strictly ordered: offsets grow within a volume, pack ids never go back.
        for w in m.chunks.windows(2) {
            assert!((w[0].pack_id, w[0].offset) < (w[1].pack_id, w[1].offset));
        }
        assert_round_trip(&entries, &out);
    }

    /// Entries that cover every block shift class, STORE, a loose record and rollover.
    pub(super) fn golden_entries() -> Vec<(&'static str, Vec<u8>, Option<PackSpec>)> {
        let store = Some(PackSpec {
            block_shift: 15,
            store: true,
            hot: true,
            random: false,
        });
        vec![
            ("g/a", text(700_000, 1), spec(16)),
            ("g/b", noise(300_000, 2), spec(14)),
            ("g/c", noise(65, 3), spec(20)),
            ("g/d", text(1 << 20, 4), spec(20)),
            ("g/e", text(90_000, 5), store),
            ("g/loose", noise(1000, 6), None),
            ("g/f", noise(2 << 20, 7), spec(17)),
        ]
    }

    pub(super) fn golden_profile() -> RuntimeProfile {
        RuntimeProfile {
            decoded_cache_bytes: 1 << 20,
            physical_cache_bytes: 16384,
            workers: 4,
            latency_reserve_workers: 1,
        }
    }

    /// The writer's output is pinned: these hashes were taken from the one-pass writer before
    /// `measure` and the regenerating tree shared its code, so folder packs stay byte-identical.
    #[test]
    fn output_is_byte_identical_to_the_pinned_writer() {
        let p = golden_profile();
        for (cap, prof, want) in [
            (
                VOLUME_CAP,
                None,
                "0a2fe4d18437ed09b02dce119e8b73d26d9e68c3dcec77184b662709d440343b",
            ),
            (
                PAGE + (1 << 20),
                Some(&p),
                "a0c6102986eb0126daff228c45d1536fe2cad24e8c608ab76053428cfb5f0e4f",
            ),
        ] {
            let (_, _, out, _) = pack_mem(&golden_entries(), prof, 3, cap).unwrap();
            let mut h = blake3::Hasher::new();
            for (n, d) in &out.files {
                h.update(n.as_bytes());
                h.update(&(d.len() as u64).to_le_bytes());
                h.update(d);
            }
            assert_eq!(h.finalize().to_hex().as_str(), want, "cap {cap}");
        }
    }

    #[test]
    fn refuses_bad_lists_and_stops_on_cancel() {
        let mut bad_shift = spec(16);
        bad_shift.as_mut().unwrap().block_shift = 13;
        for entries in [
            vec![("a", vec![1], spec(16)), ("A", vec![2], spec(16))],
            vec![("a", vec![1], bad_shift)],
            vec![("../a", vec![1], spec(16))],
        ] {
            assert!(pack_mem(&entries, None, 1, VOLUME_CAP).is_err());
        }
        let mut src = Mem::new([("a".to_string(), vec![1; 10])]);
        let files = [PackFile {
            path: "a".into(),
            size: 10,
            spec: spec(16),
        }];
        let cancel = AtomicBool::new(true);
        let r = pack(
            &mut src,
            &files,
            0,
            None,
            1,
            0,
            &mut MemOut::default(),
            &mut |_| {},
            &cancel,
        );
        assert!(matches!(r, Err(Error::Cancelled)));
        // The source is shorter than listed.
        let files = [PackFile {
            path: "a".into(),
            size: 11,
            spec: spec(16),
        }];
        let cancel = AtomicBool::new(false);
        let r = pack(
            &mut src,
            &files,
            0,
            None,
            1,
            0,
            &mut MemOut::default(),
            &mut |_| {},
            &cancel,
        );
        assert!(matches!(r, Err(Error::Format(_))));
    }
}
