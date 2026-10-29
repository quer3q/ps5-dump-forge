//! Patch packages (`\x7FLIH`): a delta of a whole package, served as that package.
//!
//! Layout (measured on one debug patch, PPSA23566 01.006; no public spec): a 64 KiB header,
//! little-endian, with the file's length at `0x18` and three `{offset, size}` pairs: the
//! patched package's `\x7FFIH` header block (`0x20`), its `\x7FCNT` container (`0x30`) and a
//! stored zip of PlayGo and relocation tables (`0x40`). Between the header block and the
//! container sit the outer blocks the patch changed, 64 KiB each, in image order with gaps.
//! The FIH and the container describe the whole patched package (its `imagedigs` lists every
//! outer block), so each stored block is placed by matching its SHA3 against `imagedigs`; every
//! other block is the base game's, which this file does not hold.
//!
//! [`View`] is the package the header describes, as bytes: the FIH block at 0, the outer image
//! at the FIH's `pfs_offset`, the container at its `cnt_offset`. A read of a block the patch
//! does not carry fails with [`crate::IN_BASE`].
// ponytail: blocks placed by digest, so every open reads and hashes the whole patch once,
// uncancellable (472 MB: 0.2 s here; a 30 GB one takes minutes over USB); decoding the zip's
// relocation tables would place them without reading them

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};

use ps5upload_fpkg::crypto::sha3;
use ps5upload_fpkg::{BLOCK, ReadSeek, Result};

use crate::pfs::{Pkg, le64};
use crate::{CORRUPT, IN_BASE, err};

pub(crate) const MAGIC: &[u8; 4] = b"\x7FLIH";
const FIH_LEN: u64 = 0x100;
const CNT_HEADER: usize = 0x1000;
const CNT_ENTRY_LEN: usize = 0x20;
const MAX_CNT_ENTRIES: usize = 4096;
const IMAGE_DIGESTS: u32 = 0x040A;
/// `imagedigs` read per pass while blocks are placed.
const DIGEST_CHUNK: usize = 1 << 20;
/// Stored blocks read and hashed at once (16 MiB).
const HASH_BATCH: usize = 256;

/// What the patch holds.
pub(crate) struct Facts {
    /// The outer blocks it carries (a block of zeros it stores once may stand for many).
    pub present: HashSet<u64>,
    pub outer_blocks: u64,
}

/// The patched package, as bytes.
pub(crate) struct View {
    pkg: Pkg,
    /// The FIH block's offset in the patch.
    fih_at: u64,
    pfs_offset: u64,
    pfs_size: u64,
    cnt_offset: u64,
    /// The container's offset and size in the patch.
    cnt_at: u64,
    cnt_size: u64,
    /// Outer block index → its offset in the patch.
    blocks: HashMap<u64, u64>,
    len: u64,
    pos: u64,
}

pub(crate) fn is_lih(head: &[u8]) -> bool {
    head.starts_with(MAGIC)
}

/// Where the patch file holds the patched package's container, from its header alone (no
/// block placed): its entries' offsets are relative to it, so it reads as at that offset.
pub(crate) fn container_at(pkg: &mut Pkg) -> Result<u64> {
    if pkg.len < BLOCK {
        return Err(err(CORRUPT, "the patch header is truncated"));
    }
    let mut head = [0u8; 0x40];
    pkg.read_at(0, &mut head)?;
    let (off, size) = (le64(&head, 0x30), le64(&head, 0x38));
    if off < BLOCK || off.checked_add(size).is_none_or(|e| e > pkg.len) {
        return Err(err(
            CORRUPT,
            format!("the patch's container ({size} bytes at {off:#x}) is outside the file"),
        ));
    }
    Ok(off)
}

impl View {
    pub fn open(file: Box<dyn ReadSeek>, len: u64) -> Result<(Self, Facts)> {
        let mut pkg = Pkg::new(file, len);
        let mut head = vec![0u8; BLOCK as usize];
        if len < BLOCK {
            return Err(err(CORRUPT, "the patch header is truncated"));
        }
        pkg.read_at(0, &mut head)?;
        if le64(&head, 0x18) != len {
            return Err(err(
                CORRUPT,
                format!(
                    "the patch header says {} bytes, the file has {len} (truncated?)",
                    le64(&head, 0x18)
                ),
            ));
        }
        let pair = |at: usize, what: &str| -> Result<(u64, u64)> {
            let (off, size) = (le64(&head, at), le64(&head, at + 8));
            if off < BLOCK || off.checked_add(size).is_none_or(|e| e > len) {
                return Err(err(
                    CORRUPT,
                    format!("the patch's {what} ({size} bytes at {off:#x}) is outside the file"),
                ));
            }
            Ok((off, size))
        };
        let (fih_at, fih_len) = pair(0x20, "package header")?;
        let (cnt_at, cnt_size) = pair(0x30, "container")?;
        if fih_len != BLOCK || !fih_at.is_multiple_of(BLOCK) || !cnt_at.is_multiple_of(BLOCK) {
            return Err(err(CORRUPT, "the patch's regions are not whole blocks"));
        }
        let data = fih_at + BLOCK..cnt_at;
        if data.is_empty() {
            return Err(err(CORRUPT, "the patch holds no outer blocks"));
        }

        let mut fih = vec![0u8; FIH_LEN as usize];
        pkg.read_at(fih_at, &mut fih)?;
        if &fih[0..4] != b"\x7FFIH" {
            return Err(err(CORRUPT, "the patch carries no \\x7FFIH package header"));
        }
        let parsed = ps5upload_fpkg::fih::parse(&fih)?;
        let (pfs_offset, pfs_size, cnt_offset) =
            (parsed.pfs_offset, parsed.pfs_size, parsed.cnt_offset);
        let outer_blocks = pfs_size / BLOCK;
        let image_end = pfs_offset.checked_add(pfs_size);
        let cnt_end = cnt_offset.checked_add(cnt_size);
        let (Some(image_end), Some(cnt_end)) = (image_end, cnt_end) else {
            return Err(err(CORRUPT, "the patched package's regions overflow"));
        };
        let apart = image_end <= cnt_offset || cnt_end <= pfs_offset;
        if pfs_offset < BLOCK || cnt_offset < BLOCK || pfs_size == 0 || !apart {
            return Err(err(
                CORRUPT,
                format!(
                    "the patched package's layout (image {pfs_size:#x} at {pfs_offset:#x}, \
                     container at {cnt_offset:#x}) is not one this reader knows"
                ),
            ));
        }

        let digests = image_digests(&mut pkg, cnt_at, cnt_size)?;
        if digests.1 != outer_blocks * 32 {
            return Err(err(
                CORRUPT,
                format!(
                    "imagedigs holds {} bytes for {outer_blocks} outer blocks",
                    digests.1
                ),
            ));
        }

        // Each stored block's digest, then one pass over `imagedigs` places them. A digest that
        // `imagedigs` lists more than once (a block of zeros) serves every one of its blocks.
        // ponytail: two maps of ~50 B per stored block (40 MB for a 50 GB patch)
        let offsets: Vec<u64> = data.step_by(BLOCK as usize).collect();
        let mut stored: HashMap<[u8; 32], u64> = HashMap::new();
        for batch in offsets.chunks(HASH_BATCH) {
            let mut raw = Vec::with_capacity(batch.len());
            for &off in batch {
                let mut b = vec![0u8; BLOCK as usize];
                pkg.read_at(off, &mut b)?;
                raw.push((off, b));
            }
            stored.extend(crate::par_map(raw, |(off, b)| (sha3(&b), off)));
        }
        let mut blocks = HashMap::new();
        let mut chunk = vec![0u8; DIGEST_CHUNK];
        let mut index = 0u64;
        while index < outer_blocks {
            let n = ((outer_blocks - index) * 32).min(DIGEST_CHUNK as u64) as usize;
            pkg.read_at(digests.0 + index * 32, &mut chunk[..n])?;
            for d in chunk[..n].as_chunks::<32>().0 {
                let mut want = *d;
                want.reverse();
                if let Some(&off) = stored.get(&want) {
                    blocks.insert(index, off);
                }
                index += 1;
            }
        }
        // Identical stored blocks share one digest (and one offset): count digests, not blocks.
        let placed: HashSet<u64> = blocks.values().copied().collect();
        if placed.len() != stored.len() {
            return Err(err(
                CORRUPT,
                format!(
                    "{} of the patch's {} distinct blocks match no outer block of the package it \
                     patches",
                    stored.len() - placed.len(),
                    stored.len()
                ),
            ));
        }
        let facts = Facts {
            present: blocks.keys().copied().collect(),
            outer_blocks,
        };
        Ok((
            Self {
                pkg,
                fih_at,
                pfs_offset,
                pfs_size,
                cnt_offset,
                cnt_at,
                cnt_size,
                blocks,
                len: image_end.max(cnt_end),
                pos: 0,
            },
            facts,
        ))
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// Where in the patch the view's byte `pos` is, and how many bytes from there are contiguous.
    fn locate(&self, pos: u64) -> io::Result<(u64, u64)> {
        if pos < BLOCK {
            return Ok((self.fih_at + pos, BLOCK - pos));
        }
        if pos >= self.pfs_offset && pos < self.pfs_offset + self.pfs_size {
            let rel = pos - self.pfs_offset;
            let index = rel / BLOCK;
            let within = rel % BLOCK;
            return match self.blocks.get(&index) {
                Some(&off) => Ok((off + within, BLOCK - within)),
                None => Err(io::Error::other(format!(
                    "{IN_BASE}: outer block {index} is the base game's; this patch holds only \
                     the blocks it changed"
                ))),
            };
        }
        if pos >= self.cnt_offset && pos < self.cnt_offset + self.cnt_size {
            let rel = pos - self.cnt_offset;
            return Ok((self.cnt_at + rel, self.cnt_size - rel));
        }
        Err(io::Error::other(format!(
            "{CORRUPT}: offset {pos:#x} is outside the patched package"
        )))
    }
}

impl Read for View {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let (at, run) = self.locate(self.pos)?;
        let n = (buf.len() as u64).min(run) as usize;
        self.pkg
            .read_at(at, &mut buf[..n])
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for View {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::other("seek out of range"))?;
        Ok(self.pos)
    }
}

/// The container's `imagedigs` entry: its offset in the patch and its size.
fn image_digests(pkg: &mut Pkg, cnt_at: u64, cnt_size: u64) -> Result<(u64, u64)> {
    let mut head = vec![0u8; CNT_HEADER];
    pkg.read_at(cnt_at, &mut head)?;
    if &head[0..4] != b"\x7FCNT" {
        return Err(err(CORRUPT, "the patch's container has no \\x7FCNT magic"));
    }
    let be32 = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
    let count = be32(&head, 0x10) as usize;
    let table = u64::from(be32(&head, 0x18));
    if count > MAX_CNT_ENTRIES {
        return Err(err(
            CORRUPT,
            format!("the container claims {count} entries"),
        ));
    }
    let mut rows = vec![0u8; count * CNT_ENTRY_LEN];
    pkg.read_at(cnt_at + table, &mut rows)?;
    for row in rows.as_chunks::<CNT_ENTRY_LEN>().0 {
        if be32(row, 0) == IMAGE_DIGESTS {
            let (off, size) = (u64::from(be32(row, 16)), u64::from(be32(row, 20)));
            if off + size > cnt_size {
                return Err(err(CORRUPT, "imagedigs runs past the patch's container"));
            }
            return Ok((cnt_at + off, size));
        }
    }
    Err(err(CORRUPT, "the patch's container has no imagedigs entry"))
}
