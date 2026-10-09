//! `.ffpfsc`: an inner image (`.exfat`, `.ffpkg` or `.ffpfs`) as a zlib PFSC container, the one
//! file of a minimal PFS, built as the inner writer writes it, with no staged image.
//!
//! The inner writer writes into a [`Stream`] on the calling thread. The stream cuts 64 KiB
//! blocks and hands each, with a one-shot reply channel, to worker threads over a bounded
//! channel; it keeps the replies in submission order and, once `threads * 4` are pending,
//! waits on the oldest and writes it to the output itself. So memory is bounded by that depth,
//! nothing reorders, and an error anywhere (a worker's, the output's, the inner writer's)
//! comes back to the calling thread, which joins every worker before it returns.
//!
//! In the output, by byte: blocks 0..6 are the PFS (header, inodes, super-root, path table,
//! reserved, `uroot`), written last; at block 6 the PFSC header and its table of `n + 1`
//! block offsets at `0x400`, written once the stream is done; the stored blocks from `data_at`
//! (64 KiB, grown by whole blocks when the table outgrows `0x400..0x10000`). A block is kept
//! compressed when that saves [`WrapOptions::min_block_gain`] percent and is shorter than 64 KiB
//! (a reader tells the two apart by length alone), else stored raw. The container is kept even
//! if it does not shrink: a stream cannot be re-read to store the image raw instead.

use std::collections::VecDeque;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use ps5upload_fpkg::{Error, Result};

use crate::image::{self, Sink};
use crate::{BLOCK, check_cancel, err, now};

/// Where the inner image's container starts: after header, inodes, super-root, path table,
/// the reserved block and `uroot`.
const FILE_BLOCK: u64 = 6;
pub(crate) const PFSC_MAGIC: u32 = 0x4353_4650;
const PFSC_VERSION_WORD: u32 = 6;
pub(crate) const PFSC_HEADER_LEN: usize = 0x30;
pub(crate) const PFSC_TABLE_AT: u64 = 0x400;
/// Replies pending per worker before the stream waits on the oldest.
const DEPTH_PER_WORKER: usize = 4;

/// The app's default zlib level (see [`WrapOptions::level`]), zlib's own. On a 13 GB game
/// (14 cores) level 6 took 16.3 s for 76.25% stored; 7: 17.3 s, 76.23%; 9: 20.4 s, 76.21%;
/// 5: 13.7 s, 76.33%; 1: 5.7 s, 78.57%.
pub const DEFAULT_LEVEL: u32 = 6;

#[derive(Debug, Clone)]
pub struct WrapOptions {
    /// zlib level, 0–9 as zlib numbers them: 0 stores every block raw, 1 is fastest, 9 the
    /// smallest. Every level writes the same kind of zlib stream; only the size and time differ.
    pub level: u32,
    /// Percent a block must shrink by to be stored compressed (at most 90).
    pub min_block_gain: u8,
    /// Worker threads; 0 uses every core.
    pub threads: usize,
    /// The PFS timestamp (seconds since 1970); `None` is now.
    pub time: Option<i64>,
}

impl Default for WrapOptions {
    fn default() -> Self {
        Self {
            level: DEFAULT_LEVEL,
            min_block_gain: 5,
            threads: 0,
            time: None,
        }
    }
}

/// What was written.
#[derive(Debug, Clone)]
pub struct WrapReport {
    pub image_size: u64,
    /// The inner image's length.
    pub raw_size: u64,
    /// The container's length (header, table, stored blocks).
    pub stored_size: u64,
    /// 64 KiB blocks of the inner image.
    pub blocks: u64,
    /// Of those, the ones stored compressed.
    pub compressed_blocks: u64,
}

/// Bytes between the container's start and its first block: 64 KiB, grown by whole blocks
/// when the offset table outgrows `0x400..0x10000`.
fn data_at(raw_size: u64) -> Result<u64> {
    let table = raw_size
        .div_ceil(BLOCK)
        .checked_add(1)
        .and_then(|n| n.checked_mul(8))
        .ok_or_else(too_big)?;
    let room = BLOCK - PFSC_TABLE_AT;
    let extra = table.saturating_sub(room).div_ceil(BLOCK) * BLOCK;
    BLOCK.checked_add(extra).ok_or_else(too_big)
}

/// The largest `.ffpfsc` an inner image of `raw_size` bytes can make (every block stored
/// raw), for preflight.
pub fn container_size_max(raw_size: u64) -> Result<u64> {
    let blocks = raw_size.div_ceil(BLOCK);
    (FILE_BLOCK * BLOCK)
        .checked_add(data_at(raw_size)?)
        .and_then(|n| n.checked_add(blocks.checked_mul(BLOCK)?))
        .ok_or_else(too_big)
}

/// One block as it is stored: padded to 64 KiB, then zlib when that saves `min_gain` percent.
fn encode_block(raw: &[u8], level: u32, min_gain: u8) -> io::Result<Vec<u8>> {
    let mut padded = raw.to_vec();
    padded.resize(BLOCK as usize, 0);
    // Level 0 stores: its zlib stream is always longer than the block, so the block stays raw.
    if level == 0 || (min_gain >= 5 && level > 1 && incompressible(&padded)?) {
        return Ok(padded);
    }
    let z = zlib(&padded, level)?;
    let limit = BLOCK as usize * (100 - usize::from(min_gain)) / 100;
    Ok(if z.len() < BLOCK as usize && z.len() <= limit {
        z
    } else {
        padded
    })
}

fn zlib(block: &[u8], level: u32) -> io::Result<Vec<u8>> {
    let mut enc = ZlibEncoder::new(Vec::with_capacity(BLOCK as usize), Compression::new(level));
    enc.write_all(block)?;
    enc.finish()
}

/// Whether a block is not worth the level's full search: its bytes are spread almost evenly
/// (over 7.9 bits each) and level 1 saves under 2%. Measured on ~24,000 blocks of three games
/// (compressed game data, a Kraken package, an executable), it changed no keep/raw decision
/// at levels 7 and 10 and saved 4–66% of the compression time, by how much data was
/// already compressed. A block it misjudges is only stored raw, still valid.
// ponytail: thresholds from that sample; widen the corpus if a game ever compresses worse
fn incompressible(block: &[u8]) -> io::Result<bool> {
    let mut counts = [0u32; 256];
    for &b in block {
        counts[usize::from(b)] += 1;
    }
    let n = block.len() as f64;
    let bits: f64 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / n;
            -p * p.log2()
        })
        .sum();
    Ok(bits > 7.9 && zlib(block, 1)?.len() * 100 > block.len() * 98)
}

/// Build a `.ffpfsc` holding `inner_name` (whose name tells SMP the filesystem) into `out`, an
/// empty file (or a wrapper around one) the caller owns, fsyncs and renames. Every byte of the
/// output is written through `out`, zeros included. `fill` writes the inner image into the
/// [`Stream`]: exactly `raw_size` bytes, forward only. Returns what `fill` returned.
pub fn wrap<T, W: Write + Seek>(
    inner_name: &str,
    raw_size: u64,
    out: &mut W,
    opts: &WrapOptions,
    cancel: &AtomicBool,
    fill: impl FnOnce(&mut Stream<'_>) -> Result<T>,
) -> Result<(T, WrapReport)> {
    let (level, gain) = (opts.level, opts.min_block_gain);
    let encode = move |raw: &[u8]| encode_block(raw, level, gain);
    wrap_with(inner_name, raw_size, out, opts, cancel, &encode, fill)
}

type Encode = dyn Fn(&[u8]) -> io::Result<Vec<u8>> + Sync;

fn wrap_with<T, W: Write + Seek>(
    inner_name: &str,
    raw_size: u64,
    out: &mut W,
    opts: &WrapOptions,
    cancel: &AtomicBool,
    encode: &Encode,
    fill: impl FnOnce(&mut Stream<'_>) -> Result<T>,
) -> Result<(T, WrapReport)> {
    if opts.level > 9 {
        return Err(err(format!(
            "zlib level must be 0 through 9, not {}",
            opts.level
        )));
    }
    if opts.min_block_gain > 90 {
        return Err(err("the minimum block gain must be at most 90%"));
    }
    if raw_size == 0 {
        return Err(err("the image inside a .ffpfsc cannot be empty"));
    }
    let time = opts.time.unwrap_or_else(now);
    // The worst case must fit the PFS's 32-bit pointers before anything is written.
    let data_at = data_at(raw_size)?;
    let max = container_size_max(raw_size)? - FILE_BLOCK * BLOCK;
    image::single(inner_name, raw_size, max, time)?;
    if out.seek(SeekFrom::End(0))? != 0 {
        return Err(err("the .ffpfsc output is not empty"));
    }
    check_cancel(cancel)?;
    let base = FILE_BLOCK * BLOCK;
    out.seek(SeekFrom::Start(base + data_at))?;
    let threads = match opts.threads {
        0 => std::thread::available_parallelism().map_or(4, |n| n.get()),
        n => n,
    };
    let limit = threads * DEPTH_PER_WORKER;

    let (filled, panicked) = std::thread::scope(|s| {
        let (work, queue) = mpsc::sync_channel::<Job>(limit);
        // Only the workers hold the queue: once they are all gone, a submit fails at once.
        let queue = Arc::new(Mutex::new(queue));
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                let queue = Arc::clone(&queue);
                s.spawn(move || worker(&queue, encode))
            })
            .collect();
        drop(queue);
        let mut stream = Stream {
            out: BufWriter::with_capacity(8 << 20, &mut *out as &mut dyn Write),
            work,
            pending: VecDeque::with_capacity(limit),
            limit,
            raw_size,
            pos: 0,
            block: Vec::with_capacity(BLOCK as usize),
            offsets: vec![data_at],
            compressed_blocks: 0,
            cancel,
        };
        let filled = fill(&mut stream).and_then(|t| Ok((t, stream.finish()?)));
        // Closes the queue: idle workers see it and exit.
        drop(stream);
        let mut panicked = None;
        for w in workers {
            if let Err(payload) = w.join() {
                panicked.get_or_insert(payload);
            }
        }
        (filled, panicked)
    });
    // The root error wins: a worker's panic over the closed channel it left behind, a cancel
    // over the error it made the inner writer return.
    if let Some(payload) = panicked {
        let why = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        return Err(err(format!("a compression worker panicked: {why}")));
    }
    let (value, (offsets, compressed_blocks)) = match filled {
        Err(_) if cancel.load(Ordering::Relaxed) => return Err(Error::Cancelled),
        r => r?,
    };
    check_cancel(cancel)?;

    // The container's header and table, then the PFS around it.
    let blocks = raw_size.div_ceil(BLOCK);
    let stored = offsets.last().copied().unwrap_or(data_at);
    out.seek(SeekFrom::Start(base))?;
    let mut sink = Sink::new(&mut *out, cancel);
    sink.write(&pfsc_header(blocks, data_at))?;
    sink.zeros_to(PFSC_TABLE_AT)?;
    for o in &offsets {
        sink.write(&o.to_le_bytes())?;
    }
    sink.zeros_to(data_at)?;
    sink.w.flush()?;
    drop(sink);

    let img = image::single(inner_name, raw_size, stored, time)?;
    out.seek(SeekFrom::Start(0))?;
    let mut sink = Sink::new(&mut *out, cancel);
    image::write_meta(&mut sink, &img)?;
    sink.expect(base)?;
    sink.w.flush()?;
    drop(sink);
    let image_size = img.ndblock * BLOCK;
    // The zero tail of the container's last block, written rather than `set_len`, so an output
    // that bounds its unsynced data sees every byte.
    let end = base + stored;
    if end > image_size {
        return Err(err(format!(
            "internal error: the container ends at byte {end} of a {image_size}-byte image"
        )));
    }
    if end < image_size {
        out.seek(SeekFrom::Start(end))?;
        let mut sink = Sink::new(&mut *out, cancel);
        sink.zeros_to(image_size - end)?;
        sink.w.flush()?;
    }
    Ok((
        value,
        WrapReport {
            image_size,
            raw_size,
            stored_size: stored,
            blocks,
            compressed_blocks,
        },
    ))
}

fn pfsc_header(blocks: u64, data_at: u64) -> [u8; PFSC_HEADER_LEN] {
    let mut h = [0u8; PFSC_HEADER_LEN];
    h[0x00..0x04].copy_from_slice(&PFSC_MAGIC.to_le_bytes());
    h[0x08..0x0C].copy_from_slice(&PFSC_VERSION_WORD.to_le_bytes());
    h[0x0C..0x10].copy_from_slice(&(BLOCK as u32).to_le_bytes());
    h[0x10..0x18].copy_from_slice(&BLOCK.to_le_bytes());
    h[0x18..0x20].copy_from_slice(&PFSC_TABLE_AT.to_le_bytes());
    h[0x20..0x28].copy_from_slice(&data_at.to_le_bytes());
    h[0x28..0x30].copy_from_slice(&(blocks * BLOCK).to_le_bytes());
    h
}

/// A raw block and where its stored form goes.
type Job = (Vec<u8>, Sender<io::Result<Vec<u8>>>);

/// Takes blocks until the queue closes. A panic drops the reply sender, which the stream
/// sees as a failed worker; the queue lock is never held across the encode.
fn worker(queue: &Mutex<Receiver<Job>>, encode: &Encode) {
    loop {
        let next = match queue.lock() {
            Ok(q) => q.recv(),
            Err(_) => return,
        };
        let Ok((raw, reply)) = next else {
            return;
        };
        // The stream may be gone already (an error elsewhere); nothing to tell it then.
        let _ = reply.send(encode(&raw));
    }
}

/// The inner image as it is written: forward-only `Write + Seek` on the calling thread.
/// Seeking forward writes zeros (cancellable per block); `End(0)` is the current position;
/// going back is an error.
pub struct Stream<'a> {
    out: BufWriter<&'a mut dyn Write>,
    work: SyncSender<Job>,
    /// Replies in submission order.
    pending: VecDeque<Receiver<io::Result<Vec<u8>>>>,
    limit: usize,
    raw_size: u64,
    /// Inner-image bytes taken.
    pos: u64,
    /// The block being cut.
    block: Vec<u8>,
    /// Every block's start in the container, then the end.
    /// ponytail: 8 bytes per 64 KiB in memory (19 MB for a 150 GB image); spill to the output
    /// past that.
    offsets: Vec<u64>,
    compressed_blocks: u64,
    cancel: &'a AtomicBool,
}

impl Stream<'_> {
    fn take(&mut self, mut buf: &[u8]) -> io::Result<()> {
        if self
            .pos
            .checked_add(buf.len() as u64)
            .is_none_or(|e| e > self.raw_size)
        {
            return Err(io::Error::other(format!(
                "the inner image runs past its planned {} bytes",
                self.raw_size
            )));
        }
        while !buf.is_empty() {
            let n = (BLOCK as usize - self.block.len()).min(buf.len());
            self.block.extend_from_slice(&buf[..n]);
            buf = &buf[n..];
            self.pos += n as u64;
            if self.block.len() == BLOCK as usize {
                self.submit()?;
            }
        }
        Ok(())
    }

    fn zeros(&mut self, mut n: u64) -> io::Result<()> {
        static ZEROS: [u8; BLOCK as usize] = [0; BLOCK as usize];
        while n > 0 {
            let k = (BLOCK - self.block.len() as u64).min(n);
            self.take(&ZEROS[..k as usize])?;
            n -= k;
        }
        Ok(())
    }

    /// Hands the cut block to the workers, first writing the oldest reply if the queue is at
    /// its depth. The work channel holds at most what is pending, so the send never blocks.
    fn submit(&mut self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("the .ffpfsc build was cancelled"));
        }
        while self.pending.len() >= self.limit {
            self.write_oldest()?;
        }
        let raw = std::mem::replace(&mut self.block, Vec::with_capacity(BLOCK as usize));
        let (reply, result) = mpsc::channel();
        self.work.try_send((raw, reply)).map_err(|e| match e {
            TrySendError::Full(_) => {
                io::Error::other("internal error: the compression queue is full")
            }
            TrySendError::Disconnected(_) => io::Error::other("compression worker failed"),
        })?;
        self.pending.push_back(result);
        Ok(())
    }

    fn write_oldest(&mut self) -> io::Result<()> {
        let Some(result) = self.pending.pop_front() else {
            return Ok(());
        };
        let stored = result
            .recv()
            .map_err(|_| io::Error::other("compression worker failed"))??;
        if stored.len() < BLOCK as usize {
            self.compressed_blocks += 1;
        }
        self.out.write_all(&stored)?;
        let end = self.offsets.last().copied().unwrap_or(0) + stored.len() as u64;
        self.offsets.push(end);
        Ok(())
    }

    /// The last (padded) block, every pending reply, then the offsets and the count of
    /// compressed blocks.
    fn finish(&mut self) -> Result<(Vec<u64>, u64)> {
        if self.pos != self.raw_size {
            return Err(err(format!(
                "the inner image is {} bytes, not the planned {}",
                self.pos, self.raw_size
            )));
        }
        if !self.block.is_empty() {
            self.submit()?;
        }
        while !self.pending.is_empty() {
            self.write_oldest()?;
        }
        self.out.flush()?;
        Ok((std::mem::take(&mut self.offsets), self.compressed_blocks))
    }
}

impl Write for Stream<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.take(buf)?;
        Ok(buf.len())
    }

    /// Blocks are written as their replies come; `wrap` drains the rest.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for Stream<'_> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(0) => Some(self.pos),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(_) => None,
        };
        match target {
            Some(t) if t >= self.pos => {
                self.zeros(t - self.pos)?;
                Ok(t)
            }
            _ => Err(io::Error::other("the .ffpfsc stream only moves forward")),
        }
    }
}

fn too_big() -> Error {
    err("the image is too large for a .ffpfsc")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pfs-wrap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Runs `f` on its own thread and fails the test if it does not return in time: every
    /// failure path must join its workers and come back, never hang.
    fn bounded<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(30))
            .expect("wrap did not return: a thread is stuck")
    }

    fn opts(threads: usize) -> WrapOptions {
        WrapOptions {
            threads,
            time: Some(1_700_000_000),
            ..WrapOptions::default()
        }
    }

    /// Writes `n` bytes of compressible data in odd-sized pieces.
    fn feed(s: &mut Stream<'_>, n: u64) -> Result<()> {
        let piece: Vec<u8> = (0..10_007u32).map(|i| (i % 7) as u8).collect();
        let mut left = n;
        while left > 0 {
            let k = left.min(piece.len() as u64) as usize;
            s.write_all(&piece[..k])?;
            left -= k as u64;
        }
        Ok(())
    }

    fn plain(raw: &[u8]) -> io::Result<Vec<u8>> {
        encode_block(raw, 6, 5)
    }

    /// Every level from 1 writes a block zlib reads back exactly; 0 keeps it raw. 10 is
    /// refused before anything is written.
    #[test]
    fn every_level_round_trips() {
        let raw: Vec<u8> = (0..BLOCK as u32)
            .map(|i| ((i % 251) ^ (i / 997)) as u8)
            .collect();
        let mut sizes = Vec::new();
        assert_eq!(encode_block(&raw, 0, 5).unwrap(), raw);
        for level in 1..=9 {
            let z = encode_block(&raw, level, 5).unwrap();
            assert!(z.len() < raw.len(), "level {level} kept the block raw");
            let mut back = Vec::new();
            flate2::read::ZlibDecoder::new(&z[..])
                .read_to_end(&mut back)
                .unwrap();
            assert_eq!(back, raw, "level {level}");
            sizes.push(z.len());
        }
        assert!(sizes[8] <= sizes[0], "{sizes:?}");
        let d = temp("level");
        for level in [10, 11] {
            let mut out = File::create(d.join(format!("{level}.ffpfsc"))).unwrap();
            let o = WrapOptions { level, ..opts(1) };
            let e = wrap("x.exfat", 100, &mut out, &o, &AtomicBool::new(false), |s| {
                feed(s, 100)
            })
            .unwrap_err();
            assert!(e.to_string().contains("0 through 9"), "{e}");
        }
    }

    /// Noise skips the level's search and is stored raw; patterned data still compresses.
    #[test]
    fn noise_is_found_without_the_full_search() {
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        let noise: Vec<u8> = (0..BLOCK)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        assert!(incompressible(&noise).unwrap());
        assert_eq!(encode_block(&noise, 7, 5).unwrap(), noise);
        let pattern: Vec<u8> = (0..BLOCK as u32).map(|i| (i % 97) as u8).collect();
        assert!(!incompressible(&pattern).unwrap());
        assert!(encode_block(&pattern, 7, 5).unwrap().len() < BLOCK as usize / 10);
    }

    #[test]
    fn data_at_grows_with_the_table() {
        let room_blocks = (BLOCK - PFSC_TABLE_AT) / 8 - 1;
        assert_eq!(data_at(1).unwrap(), BLOCK);
        assert_eq!(data_at(room_blocks * BLOCK).unwrap(), BLOCK);
        assert_eq!(data_at(room_blocks * BLOCK + 1).unwrap(), 2 * BLOCK);
        assert_eq!(
            container_size_max(room_blocks * BLOCK + 1).unwrap(),
            6 * BLOCK + 2 * BLOCK + (room_blocks + 1) * BLOCK
        );
        assert!(container_size_max(u64::MAX).is_err());
    }

    #[test]
    fn fill_of_the_wrong_length_fails() {
        let d = temp("len");
        for (wrote, planned) in [(100_000u64, 200_000u64), (300_000, 200_000)] {
            let path = d.join(format!("{wrote}.ffpfsc"));
            let mut out = File::create(&path).unwrap();
            let cancel = AtomicBool::new(false);
            let e = wrap("x.exfat", planned, &mut out, &opts(2), &cancel, |s| {
                feed(s, wrote)
            })
            .unwrap_err();
            let e = e.to_string();
            assert!(e.contains("planned"), "{e}");
        }
    }

    #[test]
    fn a_worker_error_is_the_error() {
        let d = temp("werr");
        let r = bounded(move || {
            let mut out = File::create(d.join("e.ffpfsc")).unwrap();
            let calls = AtomicUsize::new(0);
            let encode = move |raw: &[u8]| {
                if calls.fetch_add(1, Ordering::Relaxed) == 5 {
                    return Err(io::Error::other("zlib broke"));
                }
                plain(raw)
            };
            let cancel = AtomicBool::new(false);
            wrap_with(
                "x.exfat",
                64 * BLOCK,
                &mut out,
                &opts(2),
                &cancel,
                &encode,
                |s| feed(s, 64 * BLOCK),
            )
            .map(|_| ())
        });
        let e = r.unwrap_err().to_string();
        assert!(e.contains("zlib broke"), "{e}");
    }

    #[test]
    fn a_worker_panic_is_the_error() {
        let d = temp("panic");
        let r = bounded(move || {
            let mut out = File::create(d.join("p.ffpfsc")).unwrap();
            let calls = AtomicUsize::new(0);
            let encode = move |raw: &[u8]| {
                if calls.fetch_add(1, Ordering::Relaxed) == 3 {
                    panic!("encoder exploded");
                }
                plain(raw)
            };
            let cancel = AtomicBool::new(false);
            wrap_with(
                "x.exfat",
                64 * BLOCK,
                &mut out,
                &opts(3),
                &cancel,
                &encode,
                |s| feed(s, 64 * BLOCK),
            )
            .map(|_| ())
        });
        let e = r.unwrap_err().to_string();
        assert!(e.contains("panicked: encoder exploded"), "{e}");
    }

    /// Every worker gone while the queue is at its depth: the next submit or the oldest
    /// reply fails at once instead of blocking.
    #[test]
    fn all_workers_gone_fails_fast() {
        let d = temp("gone");
        let r = bounded(move || {
            let mut out = File::create(d.join("g.ffpfsc")).unwrap();
            let encode = |_: &[u8]| -> io::Result<Vec<u8>> { panic!("every worker dies") };
            let cancel = AtomicBool::new(false);
            let mut fill_err = None;
            let r = wrap_with(
                "x.exfat",
                256 * BLOCK,
                &mut out,
                &opts(2),
                &cancel,
                &encode,
                |s| {
                    let r = feed(s, 256 * BLOCK);
                    fill_err = r.as_ref().err().map(|e| e.to_string());
                    r
                },
            );
            (r.map(|_| ()), fill_err)
        });
        let (r, fill_err) = r;
        let fill_err = fill_err.expect("the stream itself reported the failure");
        assert!(fill_err.contains("compression worker failed"), "{fill_err}");
        let e = r.unwrap_err().to_string();
        assert!(e.contains("panicked: every worker dies"), "{e}");
    }

    /// A write error on the output comes straight back from the stream: here a read-only
    /// handle, full deque and full buffer.
    #[test]
    fn an_output_error_is_the_error() {
        let d = temp("out");
        let path = d.join("ro.ffpfsc");
        File::create(&path).unwrap();
        let r = bounded(move || {
            let mut out = File::open(&path).unwrap();
            let cancel = AtomicBool::new(false);
            // Incompressible-sized writes fill the 8 MiB buffer quickly.
            let raw = 512 * BLOCK;
            wrap("x.exfat", raw, &mut out, &opts(1), &cancel, |s| {
                let mut x = 0x2545_F491u32;
                let block: Vec<u8> = (0..BLOCK)
                    .map(|_| {
                        x ^= x << 13;
                        x ^= x >> 17;
                        x ^= x << 5;
                        x as u8
                    })
                    .collect();
                for _ in 0..512 {
                    s.write_all(&block)?;
                }
                Ok(())
            })
            .map(|_| ())
        });
        let e = r.unwrap_err();
        assert!(matches!(e, Error::Io(_)), "{e}");
    }

    #[test]
    fn cancel_mid_stream_is_cancelled() {
        let d = temp("cancel");
        let r = bounded(move || {
            let mut out = File::create(d.join("c.ffpfsc")).unwrap();
            let cancel = AtomicBool::new(false);
            wrap("x.exfat", 64 * BLOCK, &mut out, &opts(2), &cancel, |s| {
                feed(s, 10 * BLOCK)?;
                cancel.store(true, Ordering::Relaxed);
                // A forward seek is zero fill, and cancellable like any write.
                s.seek(SeekFrom::Start(64 * BLOCK - 1))?;
                s.write_all(&[0])?;
                Ok(())
            })
            .map(|_| ())
        });
        assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    }

    #[test]
    fn the_stream_only_moves_forward() {
        let d = temp("seek");
        let mut out = File::create(d.join("s.ffpfsc")).unwrap();
        let cancel = AtomicBool::new(false);
        let ((), report) = wrap("x.exfat", 3 * BLOCK, &mut out, &opts(2), &cancel, |s| {
            assert_eq!(s.seek(SeekFrom::End(0))?, 0);
            s.write_all(b"head")?;
            assert_eq!(s.stream_position()?, 4);
            assert!(s.seek(SeekFrom::Start(1)).is_err());
            assert!(s.seek(SeekFrom::End(1)).is_err());
            s.seek(SeekFrom::Start(3 * BLOCK - 1))?;
            s.write_all(&[1])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(report.blocks, 3);
        assert_eq!(report.compressed_blocks, 3);
        assert_eq!(out.metadata().unwrap().len(), report.image_size);
    }
}
