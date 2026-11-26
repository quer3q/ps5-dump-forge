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
/// The most opening a layout descriptor may hold at once, the descriptor itself included:
/// three quarters of a job's 512 MiB. [`open_cost`] bounds the descriptor and the walk's tables
/// from the header's counts; a Kraken one is then walked once only to count its blocks, so the
/// list it keeps (a `KBlock`, 32 B, per block) is checked at its exact length before it is
/// taken. A descriptor's size says little by itself: Spider-Man 2's 272 GB mount is a 10 MB
/// descriptor, FF VII Rebirth's a 34 MB one.
// ponytail: a 1 TiB mount stored raw (the descriptor's 5-byte boundaries reach no further)
// opens with up to ~1 M files; a `KBlock` without its logical start (the lengths give it)
// would lift it
pub(crate) const MAX_OPEN: u64 = 384 << 20;
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
    /// Its byte shuffle (`kraken_image::SHUFFLES`), undone after decoding.
    shuffle: u8,
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
    pub fn parse(blob: Vec<u8>, image_size: u64) -> Result<Self> {
        if blob.len() < 16 {
            return Err(err(
                CORRUPT,
                "the layout descriptor is shorter than its header",
            ));
        }
        let (w0, w1) = (le64(&blob, 0), le64(&blob, 8));
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
        let shape = shape(&blob, w0, w1);
        let len = blob.len() as u64;
        let walk = open_cost(true, len, w0, w1);
        let cost = match shape {
            Shape::Kraken => walk,
            Shape::Flat => open_cost(false, len, w0, w1),
            Shape::Either => walk.max(open_cost(false, len, w0, w1)),
        };
        if cost > MAX_OPEN {
            return Err(too_big(blob.len(), cost));
        }
        let kraken_error = match shape {
            Shape::Kraken => return Ok(Self::Kraken(Kraken::new(&blob, image_size, walk)?)),
            Shape::Flat => None,
            // The flat parser needs the blob if the walk fails.
            Shape::Either => match Kraken::new(&blob, image_size, walk) {
                Ok(k) => return Ok(Self::Kraken(k)),
                Err(e) => Some(e),
            },
        };
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
        match naps::parse(&blob) {
            Ok(layout) => Ok(Self::Flat(layout)),
            Err(e) => Err(kraken_error.unwrap_or_else(|| {
                err(
                    CORRUPT,
                    format!("the layout descriptor does not parse: {e}"),
                )
            })),
        }
    }
}

/// Which reader a descriptor is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Kraken,
    Flat,
    /// Sized like both: walked as Kraken first, else parsed flat.
    Either,
}

/// How to tell the two descriptors apart. A Kraken one pads its boundary tables to eight
/// bytes, then three bytes (`00 00 04` in every plain build measured; other values from other
/// builders, see `kraken_image::describe`), then its records, then pads to eight bytes; the
/// flat one has its records straight after its tables (one `u2c` group more when the ublock
/// count isn't a multiple of eight) and ends at its last record. So `00 00 04` there is Kraken,
/// and so is any other value unless the blob is exactly the flat size.
fn shape(blob: &[u8], w0: u64, w1: u64) -> Shape {
    // Every count is a 24-bit field, so none of this can overflow.
    let files = (w0 & 0xFF_FFFF) as usize + 1;
    let shuffle = ((w0 >> 28) & 0xF) as usize;
    let ublocks = ((w0 >> 32) & 0xFF_FFFF) as usize;
    let outer = (w1 & 0xFF_FFFF) as usize;
    let records = ((w1 >> 24) & 0xFF_FFFF) as usize + 2;
    let fidx_end = 16 + (outer + shuffle) * 8 + files * 6;
    let rec_at = (fidx_end + (ublocks / 8 + 1) * 10).next_multiple_of(8) + 3;
    let flat_len = fidx_end + (ublocks.div_ceil(8) + 1) * 10 + records * 9;
    match blob.get(rec_at - 3..rec_at) {
        Some([0, 0, 4]) => Shape::Kraken,
        Some(_) if blob.len() >= rec_at + records * 9 => {
            if blob.len() == flat_len {
                Shape::Either
            } else {
                Shape::Kraken
            }
        }
        _ => Shape::Flat,
    }
}

/// The most opening a descriptor of `len` bytes with header words `w0`, `w1` holds at once,
/// before a Kraken one's blocks are counted.
///
/// Kraken: the blob and `describe_each`'s tables (per file: entries, sorted bounds, sparse
/// extents; the shuffle table's ids, at most 15 B); [`Kraken::new`] adds the block list once it
/// knows its length.
/// Flat: the blob and `naps::Layout`'s tables, the boundary one sized by what the blob has room
/// for.
fn open_cost(kraken: bool, len: u64, w0: u64, w1: u64) -> u64 {
    use std::mem::size_of;
    // Every count is a 24-bit field, so only `len` and its terms can overflow; they saturate.
    let files = (w0 & 0xFF_FFFF) + 1;
    if kraken {
        let per_file = size_of::<(u64, u8)>() + size_of::<u64>() + size_of::<(u64, u64)>();
        return len.saturating_add(files * per_file as u64 + 15);
    }
    let ublocks = (w0 >> 32) & 0xFF_FFFF;
    let outer = (w1 & 0xFF_FFFF) + ((w0 >> 28) & 0xF);
    let cblocks = ((w1 >> 24) & 0xFF_FFFF) + 2;
    let tables = outer * 8
        + (ublocks / 8 + 2) * size_of::<(u32, [u8; 7])>() as u64
        + cblocks * size_of::<naps::Cblock>() as u64;
    len.saturating_add((len / 6).saturating_mul(size_of::<(u64, u8)>() as u64))
        .saturating_add(tables)
}

/// The refusal of a descriptor that would hold `cost` bytes, more than [`MAX_OPEN`].
fn too_big(len: usize, cost: u64) -> ps5upload_fpkg::Error {
    err(
        CORRUPT,
        format!(
            "the layout descriptor ({len} bytes) may need {} MiB to read, more than the {} MiB allowed",
            cost >> 20,
            MAX_OPEN >> 20
        ),
    )
}

impl Kraken {
    /// The blocks `blob` describes, with `held` bytes ([`open_cost`]'s) already counted
    /// against [`MAX_OPEN`].
    fn new(blob: &[u8], image_size: u64, held: u64) -> Result<Self> {
        // Counted first, so the list is taken at its length and only once it fits.
        let mut count = 0u64;
        kraken_image::describe_each(blob, |_| {
            count += 1;
            Ok(())
        })
        .map_err(|e| {
            err(
                CORRUPT,
                format!("the Kraken layout descriptor does not walk: {e}"),
            )
        })?;
        let cost = held.saturating_add(count * std::mem::size_of::<KBlock>() as u64);
        if cost > MAX_OPEN {
            return Err(too_big(blob.len(), cost));
        }
        let mut blocks = Vec::with_capacity(count as usize);
        let mut end = 0u64;
        // The same walk again, so only these checks can fail it.
        kraken_image::describe_each(blob, |b| {
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
                shuffle: b.shuffle,
            });
            Ok(())
        })?;
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

    /// The stored byte ranges behind mount bytes `off..off + len`.
    pub fn stored(&self, off: u64, len: u64) -> impl Iterator<Item = std::ops::Range<u64>> + '_ {
        let first = self
            .blocks
            .partition_point(|b| b.logical + u64::from(b.len) <= off);
        let last = self.blocks.partition_point(|b| b.logical < off + len);
        self.blocks[first..last.max(first)]
            .iter()
            .map(|b| b.stored_at..b.stored_at + u64::from(b.stored_len))
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
            shuffle: b.shuffle,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn header(files: u64, ublocks: u64) -> u64 {
        (files - 1) | COMP_KRAKEN << 24 | ublocks << 32
    }

    #[test]
    fn open_cost_admits_real_titles_and_refuses_what_the_fields_allow() {
        // FF VII Rebirth's 34,188,936-byte descriptor, at a guess of ~640 K files (the count
        // its size implies).
        assert!(open_cost(true, 34_188_936, header(640_000, 610_000), 0) <= MAX_OPEN);
        assert!(open_cost(true, 1 << 20, header(1 << 24, 0xFF_FFFF), 0) > MAX_OPEN);
        assert!(open_cost(false, u64::MAX, 0, 0) > MAX_OPEN);
        assert!(open_cost(false, 64 << 20, 0xFF_FFFF << 32, u64::MAX) > MAX_OPEN);
        // What `MAX_OPEN`'s note says a kept block costs, its shuffle id included.
        assert_eq!(std::mem::size_of::<KBlock>(), 32);
    }

    /// The descriptor the builder writes for a 419 GB mount stored raw (1.6 M blocks, a 68 MB
    /// descriptor) opens. Estimated from the header's counts, as a ublock and a record each,
    /// it was refused at 403 MiB after the whole package had been written.
    #[test]
    fn a_large_package_opens_within_the_budget() {
        use ps5upload_fpkg::kraken_image::{KrakenImage, Owner, StoredBlock};
        const UBLOCK: u64 = 256 << 10;
        const N: u64 = 1_600_000;
        let half = (UBLOCK / 2) as u32;
        let blocks = (0..N)
            .map(|i| StoredBlock {
                logical: i * UBLOCK,
                len: UBLOCK as u32,
                stored_at: i * UBLOCK,
                halves: vec![(half, false), (half, false)],
                delta: [false; 2],
                owner: Owner::File(0),
            })
            .collect();
        let mount = N * UBLOCK;
        let image = KrakenImage {
            blocks,
            image_len: mount,
            file_digests: Vec::new(),
            file_stored_at: Vec::new(),
            mount_size: mount,
            block_digests: None,
            data_end: mount,
            meta_base: mount,
        };
        let blob = kraken_image::layout(&image, &[0]).unwrap();
        drop(image);
        match Layout::parse(blob, mount).unwrap() {
            Layout::Kraken(k) => {
                assert_eq!(k.block_count(), N as usize);
                assert_eq!(k.mount_size(), mount);
            }
            Layout::Flat(_) => panic!("read as a flat layout"),
        }
    }
}
