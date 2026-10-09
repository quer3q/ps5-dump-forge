//! The inner image (`pfs_image.dat`) as the mount the console sees, read per range.
//!
//! Two layouts come out of the vendored builder, told apart by their layout descriptor
//! (`naps_pkg_layout.dat`):
//!
//! * **Kraken** (the default, and the stored-Kraken diagnostic): the mount is tiled by blocks of
//!   at most 256 KiB, each stored compressed or raw, back to back in the image. The descriptor
//!   (walked by `kraken_image::describe`) says where each one is stored; a read decodes only the
//!   blocks it touches, keeping the last few.
//! * **Flat** (`kraken: false`): file payloads stored raw at their own 64 KiB boundaries and the
//!   metadata region stored verbatim at the metadata base, so the image *is* the mount from
//!   there on. Where each file sits is derived from the afid order (see `lib.rs`).

use ps5upload_fpkg::kraken_image::{self, DescribedBlock};
use ps5upload_fpkg::{Result, naps};

use crate::pfs::{Outer, OuterFile, le64};
use crate::{CORRUPT, UNSUPPORTED_CODEC, err};

/// Decoded Kraken blocks kept (up to 256 KiB each). Sequential reads need one; the metadata walk
/// alternates between an inode table block and a directory block.
// ponytail: 4-slot ring (1 MiB), no LRU; widen if random access patterns show up
const DECODED_CACHE: usize = 4;
/// The largest layout descriptor read: nine bytes per block record, so 32 MiB covers ~3.7 M
/// stored blocks, about 900 GiB of mount (Spider-Man 2's 272 GB mount is a 10 MB descriptor).
/// Sparse blocks (zeros, no record) are not bounded by it: boundary offsets are 40-bit, so a
/// mount holds at most ~4.2 M of them (1 TiB / 256 KiB), ~7.9 M blocks in all.
// ponytail: 32 MiB descriptor, held whole with ~100 B per block while it is walked (~0.8 GB at
// the ~7.9 M-block worst case)
pub(crate) const MAX_DESCRIPTOR: u64 = 32 << 20;
/// The compression type every layout this reader decodes declares (Kraken, or no payload).
const COMP_KRAKEN: u64 = 2;

/// One Kraken block, kept compactly: a 100 GB mount is ~400 K of them.
struct KBlock {
    logical: u64,
    stored_at: u64,
    len: u32,
    stored_len: u32,
    even_len: u32,
    even_lz: bool,
    odd_lz: bool,
    mode_bits: u8,
}

pub(crate) struct Kraken {
    blocks: Vec<KBlock>,
    mount_size: u64,
    cache: Vec<(usize, Vec<u8>)>,
    next: usize,
    scratch: Vec<u8>,
}

pub(crate) enum Layout {
    Kraken(Kraken),
    /// The descriptor, for the file placements `lib.rs` derives.
    Flat(naps::Layout),
}

impl Layout {
    /// Which layout `blob` describes for an image of `image_size` stored bytes.
    pub fn parse(blob: &[u8], image_size: u64) -> Result<Self> {
        if blob.len() < 16 {
            return Err(err(
                CORRUPT,
                "the layout descriptor is shorter than its header",
            ));
        }
        let (w0, w1) = (le64(blob, 0), le64(blob, 8));
        let comp = (w0 >> 24) & 3;
        if comp != COMP_KRAKEN {
            let name = match comp {
                0 => "QuickZ",
                1 => "zlib (the diagnostic metadata codec the console rejects)",
                _ => "an unknown algorithm",
            };
            return Err(err(
                UNSUPPORTED_CODEC,
                format!(
                    "the inner image declares compression type {comp}, {name}; only Kraken and stored images are decoded"
                ),
            ));
        }
        if is_kraken(blob, w0, w1) {
            return Ok(Self::Kraken(Kraken::new(blob, image_size)?));
        }
        // `naps::parse` sizes its first table by the header before checking the blob; check
        // that table (and the shuffle one) fits first. Both counts are small fields.
        let outer = (w1 & 0xFF_FFFF) as usize;
        let shuffle = ((w0 >> 28) & 0xF) as usize;
        if blob.len() < 16 + (outer + shuffle) * 8 {
            return Err(err(
                CORRUPT,
                "the layout descriptor is shorter than its tables",
            ));
        }
        let layout = naps::parse(blob).map_err(|e| {
            err(
                CORRUPT,
                format!("the layout descriptor does not parse: {e}"),
            )
        })?;
        Ok(Self::Flat(layout))
    }
}

/// The Kraken descriptor's mark: after the boundary tables, padded to eight bytes, the ublock
/// size `0x040000` as three bytes, then the records (see `kraken_image::layout`). The flat
/// descriptor has records straight after its tables.
fn is_kraken(blob: &[u8], w0: u64, w1: u64) -> bool {
    // Every count is a 24-bit field, so none of this can overflow.
    let files = (w0 & 0xFF_FFFF) as usize + 1;
    let ublocks = ((w0 >> 32) & 0xFF_FFFF) as usize;
    let outer = (w1 & 0xFF_FFFF) as usize;
    let u2c_end = 16 + outer * 8 + files * 6 + (ublocks / 8 + 1) * 10;
    let rec_at = u2c_end.next_multiple_of(8) + 3;
    blob.get(rec_at - 3..rec_at) == Some(&[0, 0, 4][..])
}

impl Kraken {
    fn new(blob: &[u8], image_size: u64) -> Result<Self> {
        let described = kraken_image::describe(blob).map_err(|e| {
            err(
                CORRUPT,
                format!("the Kraken layout descriptor does not walk: {e}"),
            )
        })?;
        let mut blocks = Vec::with_capacity(described.len());
        let mut end = 0u64;
        for b in described {
            let fits = b
                .stored_at
                .checked_add(b.stored_len)
                .is_some_and(|e| e <= image_size);
            let small = |v: u64| u32::try_from(v).ok();
            let (Some(len), Some(stored_len), Some(even_len)) =
                (small(b.len), small(b.stored_len), small(b.even_len))
            else {
                return Err(err(CORRUPT, "a Kraken block's lengths are out of range"));
            };
            if b.logical != end || len == 0 || !fits {
                return Err(err(
                    CORRUPT,
                    format!(
                        "the Kraken block at {:#x} does not tile the mount or runs past the stored image",
                        b.logical
                    ),
                ));
            }
            end += b.len;
            blocks.push(KBlock {
                logical: b.logical,
                stored_at: b.stored_at,
                len,
                stored_len,
                even_len,
                even_lz: b.even_lz,
                odd_lz: b.odd_lz,
                mode_bits: b.mode_bits,
            });
        }
        Ok(Self {
            blocks,
            mount_size: end,
            cache: Vec::with_capacity(DECODED_CACHE),
            next: 0,
            scratch: Vec::new(),
        })
    }

    pub fn mount_size(&self) -> u64 {
        self.mount_size
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Blocks stored whole and raw (the stored-Kraken diagnostic stores every block so).
    pub fn raw_blocks(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| !b.even_lz && !b.odd_lz && b.stored_len == b.len)
            .count()
    }

    /// Exactly `buf.len()` mount bytes at `off`, decoding the blocks the range touches.
    pub fn read(
        &mut self,
        outer: &mut Outer,
        image: &OuterFile,
        off: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        if off
            .checked_add(buf.len() as u64)
            .is_none_or(|end| end > self.mount_size)
        {
            return Err(err(
                CORRUPT,
                format!(
                    "a read of {} bytes at {off:#x} runs past the mount's {:#x}",
                    buf.len(),
                    self.mount_size
                ),
            ));
        }
        let mut i = self
            .blocks
            .partition_point(|b| b.logical + u64::from(b.len) <= off);
        let end = off + buf.len() as u64;
        let last = self.blocks.partition_point(|b| b.logical < end);
        // The batch holds its blocks' stored bytes at once, so it is taken only when they are
        // no more than the decoded bytes (true of every real block: a half that does not
        // shrink is stored raw); a forged descriptor claiming more goes one block at a time.
        let span = &self.blocks[i..last];
        let stored: u64 = span.iter().map(|b| u64::from(b.stored_len)).sum();
        let logical: u64 = span.iter().map(|b| u64::from(b.len)).sum();
        if last - i > 2 && stored <= logical {
            // A sequential reader's previous read ended in this one's first block.
            let mut done = 0usize;
            if let Some((_, data)) = self.cache.iter().find(|(k, _)| *k == i) {
                let within = (off - self.blocks[i].logical) as usize;
                done = data.len() - within;
                buf[..done].copy_from_slice(&data[within..]);
                i += 1;
            }
            let at = off + done as u64;
            return self.read_many(outer, image, i..last, at, &mut buf[done..]);
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = off + done as u64;
            // The blocks tile the mount (checked in `new`), so block `i` holds `at`.
            let (logical, len) = (self.blocks[i].logical, self.blocks[i].len as usize);
            let within = (at - logical) as usize;
            let n = (len - within).min(buf.len() - done);
            let data = self.decoded(i, outer, image)?;
            buf[done..done + n].copy_from_slice(&data[within..within + n]);
            done += n;
            i += 1;
        }
        Ok(())
    }

    /// [`Kraken::read`] over blocks `range`, decoded on every core. Their stored bytes are read
    /// one run of back-to-back blocks at a time, so the outer checks spread across cores too.
    fn read_many(
        &mut self,
        outer: &mut Outer,
        image: &OuterFile,
        range: std::ops::Range<usize>,
        off: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let blocks = &self.blocks[range.clone()];
        // (run, offset in it) per block; a sparse block stores nothing and joins any run.
        let mut runs: Vec<Vec<u8>> = Vec::new();
        let mut at = Vec::with_capacity(blocks.len());
        let mut k = 0;
        while k < blocks.len() {
            let start = blocks[k].stored_at;
            let mut stored = start;
            let mut j = k;
            while j < blocks.len() && (blocks[j].stored_len == 0 || blocks[j].stored_at == stored) {
                let len = u64::from(blocks[j].stored_len);
                at.push((runs.len(), if len == 0 { 0 } else { stored - start }));
                stored += len;
                j += 1;
            }
            let mut run = vec![0u8; (stored - start) as usize];
            image.read(outer, start, &mut run)?;
            runs.push(run);
            k = j;
        }
        let work: Vec<_> = range
            .clone()
            .zip(&at)
            .map(|(i, &(run, from))| {
                let len = self.blocks[i].stored_len as usize;
                (i, &runs[run][from as usize..from as usize + len])
            })
            .collect();
        let mut decoded = crate::par_map(work, |(i, stored)| self.decode(i, stored))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut done = 0usize;
        for (i, data) in range.clone().zip(&decoded) {
            let within = (off + done as u64 - self.blocks[i].logical) as usize;
            let n = (data.len() - within).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&data[within..within + n]);
            done += n;
        }
        // A sequential reader's next read starts in the last block.
        let tail = decoded.pop().unwrap_or_default();
        self.remember(range.end - 1, tail);
        Ok(())
    }

    fn decoded(&mut self, i: usize, outer: &mut Outer, image: &OuterFile) -> Result<&[u8]> {
        if let Some(pos) = self.cache.iter().position(|(k, _)| *k == i) {
            return Ok(&self.cache[pos].1);
        }
        let b = &self.blocks[i];
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.resize(b.stored_len as usize, 0);
        image.read(outer, b.stored_at, &mut scratch)?;
        let data = self.decode(i, &scratch);
        self.scratch = scratch;
        Ok(self.remember(i, data?))
    }

    /// Block `i` from its stored bytes.
    fn decode(&self, i: usize, stored: &[u8]) -> Result<Vec<u8>> {
        let b = &self.blocks[i];
        let described = DescribedBlock {
            logical: b.logical,
            len: u64::from(b.len),
            stored_at: 0,
            even_len: u64::from(b.even_len),
            stored_len: u64::from(b.stored_len),
            even_lz: b.even_lz,
            odd_lz: b.odd_lz,
            mode_bits: b.mode_bits,
        };
        // The stored bytes passed their outer digest, so a block that does not decode uses a
        // mode this decoder does not handle rather than being damaged.
        let data = kraken_image::decode_described(stored, &described).map_err(|e| {
            err(
                UNSUPPORTED_CODEC,
                format!(
                    "the Kraken block at mount offset {:#x} does not decode: {e}",
                    b.logical
                ),
            )
        })?;
        if data.len() != b.len as usize {
            return Err(err(
                UNSUPPORTED_CODEC,
                format!(
                    "the Kraken block at mount offset {:#x} decodes to {} bytes, not {}",
                    b.logical,
                    data.len(),
                    b.len
                ),
            ));
        }
        Ok(data)
    }

    /// Keep block `i`'s decoded bytes in the ring.
    fn remember(&mut self, i: usize, data: Vec<u8>) -> &[u8] {
        let slot = if self.cache.len() < DECODED_CACHE {
            self.cache.push((i, data));
            self.cache.len() - 1
        } else {
            let slot = self.next;
            self.cache[slot] = (i, data);
            self.next = (slot + 1) % DECODED_CACHE;
            slot
        };
        &self.cache[slot].1
    }
}
