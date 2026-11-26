//! The inner image (`pfs_image.dat`) as the mount the console sees, by its layout descriptor:
//! **Kraken** (the mount tiled by blocks of at most 256 KiB, each stored compressed or raw) or
//! **Flat** (`kraken: false`: files raw at 64 KiB boundaries, the metadata verbatim at its base).

use ps5upload_fpkg::kraken_image::{self, DescribedBlock};
use ps5upload_fpkg::{Result, naps};

use crate::pfs::{Outer, OuterFile, le64};
use crate::{CORRUPT, UNSUPPORTED_CODEC, err};

/// The metadata walk alternates between an inode table block and a directory block.
// ponytail: 4-slot ring (1 MiB), no LRU; widen if random access patterns show up
const DECODED_CACHE: usize = 4;
/// The most opening a descriptor may hold at once ([`open_cost`]): 3/4 of a job's 512 MiB.
/// Spider-Man 2's 272 GB mount is a 10 MB descriptor, issue #72's 398 GiB one 32 MB.
// ponytail: ~5.4 M ublocks when files are few (~1.4 TB of mount at 256 KiB blocks, each block
// counted once as a ublock and once as a record; ~0.9 TB for a 128 KiB build, two records per
// ublock), fewer with many small files
pub(crate) const MAX_OPEN: u64 = 384 << 20;
const COMP_KRAKEN: u64 = 2;

/// A 100 GB mount is ~400 K of them.
struct KBlock {
    logical: u64,
    stored_at: u64,
    len: u32,
    stored_len: u32,
    even_len: u32,
    even_lz: bool,
    odd_lz: bool,
    mode_bits: u8,
    /// `kraken_image::SHUFFLES`, undone after decoding.
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
    Flat(naps::Layout),
}

impl Layout {
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
        let cost = match shape {
            Shape::Kraken => open_cost(true, len, w0, w1),
            Shape::Flat => open_cost(false, len, w0, w1),
            // The walk holds a copy of the blob too.
            Shape::Either => (open_cost(true, len, w0, w1).saturating_add(len))
                .max(open_cost(false, len, w0, w1)),
        };
        if cost > MAX_OPEN {
            return Err(err(
                CORRUPT,
                format!(
                    "the layout descriptor ({} bytes) may need {} MiB to read, more than the {} MiB allowed",
                    blob.len(),
                    cost >> 20,
                    MAX_OPEN >> 20
                ),
            ));
        }
        let kraken_error = match shape {
            Shape::Kraken => return Ok(Self::Kraken(Kraken::new(blob, image_size)?)),
            Shape::Flat => None,
            // Walked from a copy: the flat parser needs the blob if the walk fails.
            Shape::Either => match Kraken::new(blob.clone(), image_size) {
                Ok(k) => return Ok(Self::Kraken(k)),
                Err(e) => Some(e),
            },
        };
        // `naps::parse` sizes its first tables by the header before checking the blob.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Kraken,
    Flat,
    /// Sized like both: walked as Kraken first, else parsed flat.
    Either,
}

/// A Kraken descriptor pads its tables to 8 bytes, then 3 bytes (`00 00 04` in plain builds),
/// then records; a flat one has records right after its tables and ends at the last. So
/// `00 00 04` there is Kraken, and so is any other value unless the blob is exactly the flat size.
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

/// The most opening a descriptor holds at once. Kraken: the blob, the walk's per-file tables and
/// one `KBlock` per block (at most ublocks + files + 1 + records). Flat: the blob and
/// `naps::Layout`'s tables.
fn open_cost(kraken: bool, len: u64, w0: u64, w1: u64) -> u64 {
    use std::mem::size_of;
    // Counts are 24-bit fields: only `len` and its terms can overflow; they saturate.
    let files = (w0 & 0xFF_FFFF) + 1;
    let ublocks = (w0 >> 32) & 0xFF_FFFF;
    if kraken {
        let records = ((w1 >> 24) & 0xFF_FFFF) + 2;
        let per_file = size_of::<(u64, u8)>() + size_of::<u64>() + size_of::<(u64, u64)>();
        let blocks = ublocks + files + 1 + records;
        let kept = blocks * size_of::<KBlock>() as u64;
        return len.saturating_add(files * per_file as u64 + 15 + kept);
    }
    let outer = (w1 & 0xFF_FFFF) + ((w0 >> 28) & 0xF);
    let cblocks = ((w1 >> 24) & 0xFF_FFFF) + 2;
    let tables = outer * 8
        + (ublocks / 8 + 2) * size_of::<(u32, [u8; 7])>() as u64
        + cblocks * size_of::<naps::Cblock>() as u64;
    len.saturating_add((len / 6).saturating_mul(size_of::<(u64, u8)>() as u64))
        .saturating_add(tables)
}

/// `b`, checked to tile the mount from `end` and to lie within the stored image.
fn kblock(b: &DescribedBlock, end: u64, image_size: u64) -> Result<KBlock> {
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
    Ok(KBlock {
        logical: b.logical,
        stored_at: b.stored_at,
        len,
        stored_len,
        even_len,
        even_lz: b.even_lz,
        odd_lz: b.odd_lz,
        mode_bits: b.mode_bits,
        shuffle: b.shuffle,
    })
}

impl Kraken {
    fn new(blob: Vec<u8>, image_size: u64) -> Result<Self> {
        let walk_err = |e| {
            err(
                CORRUPT,
                format!("the Kraken layout descriptor does not walk: {e}"),
            )
        };
        // A counting walk first: a growing Vec holds up to three times the list while it moves.
        let mut count = 0usize;
        kraken_image::describe_each(&blob, |_| {
            count += 1;
            Ok(())
        })
        .map_err(walk_err)?;
        let mut blocks = Vec::with_capacity(count);
        let (mut end, mut bad) = (0u64, None);
        let walked = kraken_image::describe_each(&blob, |b| {
            let block = kblock(&b, end, image_size).map_err(|e| {
                bad = Some(e);
                ps5upload_fpkg::Error::Format(String::new())
            })?;
            end += b.len;
            blocks.push(block);
            Ok(())
        });
        if let Err(e) = walked {
            return Err(bad.unwrap_or_else(|| walk_err(e)));
        }
        drop(blob);
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

    /// Blocks stored whole and raw.
    pub fn raw_blocks(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| !b.even_lz && !b.odd_lz && b.stored_len == b.len)
            .count()
    }

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
        // Batched only when stored bytes don't exceed decoded ones (true of every real block):
        // a forged descriptor claiming more goes one block at a time.
        let span = &self.blocks[i..last];
        let stored: u64 = span.iter().map(|b| u64::from(b.stored_len)).sum();
        let logical: u64 = span.iter().map(|b| u64::from(b.len)).sum();
        if last - i > 2 && stored <= logical {
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
            // The blocks tile the mount (checked in `new`).
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

    /// [`Kraken::read`] over blocks `range`, read in back-to-back runs and decoded on every core.
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
        // The stored bytes passed their digest, so a failure is an unhandled mode, not damage.
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
        // FF VII Rebirth's 34 MB descriptor (~640 K files, guessed), and a 1 TiB mount of few files.
        assert!(open_cost(true, 34_188_936, header(640_000, 610_000), 0) <= MAX_OPEN);
        assert!(open_cost(true, 10 << 20, header(1_000, 2_400_000), 0) <= MAX_OPEN);
        // Issue #72: ~398 GiB of mount in 88 files, ~137 MB.
        let records = |n: u64| (n - 2) << 24;
        let issue = open_cost(true, 32_269_600, header(88, 1_630_208), records(1_630_300));
        assert!(issue <= MAX_OPEN, "{issue}");
        // A 1 TiB mount of few files at 256 KiB blocks, its descriptor taken large: ~353 MB.
        let tib = open_cost(true, 85 << 20, header(100, 4 << 20), records(4_200_000));
        assert!(tib <= MAX_OPEN, "{tib}");
        // The most the fields allow: millions of records more than the ublocks need.
        assert!(open_cost(true, 64 << 20, header(1_000, 4_200_000), records(0xFF_FFFF)) > MAX_OPEN);
        assert!(open_cost(true, 1 << 20, header(1 << 24, 0xFF_FFFF), 0) > MAX_OPEN);
        assert!(open_cost(false, u64::MAX, 0, 0) > MAX_OPEN);
        assert!(open_cost(false, 64 << 20, 0xFF_FFFF << 32, u64::MAX) > MAX_OPEN);
        assert_eq!(std::mem::size_of::<KBlock>(), 32);
    }
}
