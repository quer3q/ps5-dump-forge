//! The package's outer layers, read on demand: the FIH header, the CNT container (only its
//! header and entry table), and the outer PFS's 64 KiB blocks — decrypted in the native mode,
//! checked against `imagedigs` in both.
//!
//! The vendored `cnt::read` and `outer::open` hold the whole container and the whole outer image
//! and size their reads by on-disk counts, so this module re-reads the same structures with
//! every count and offset checked against the file before anything is allocated.

use std::io::SeekFrom;

use ps5upload_fpkg::crypto::sha3;
use ps5upload_fpkg::outer::{Dinode, PER_INDIRECT};
use ps5upload_fpkg::xts::{SIGNED_SECTOR_FLAG, Xts};
use ps5upload_fpkg::{BLOCK, ReadSeek, Result};

use crate::{CORRUPT, NOT_PS5, RETAIL, err};

/// The FIH header's length; everything we read from it sits inside.
const FIH_LEN: usize = 0x100;
/// The CNT header region.
const CNT_HEADER: usize = 0x1000;
const CNT_MAGIC: u32 = 0x7F43_4E54;
const CNT_ENTRY_LEN: u64 = 0x20;
/// Container entries we accept. Real packages carry a few dozen.
const MAX_CNT_ENTRIES: u32 = 4096;
/// Decrypted outer blocks kept. A Kraken block's stored bytes span at most nine outer blocks and
/// a sequential read moves forward, so a small ring decodes each block once.
// ponytail: 16-slot ring (1 MiB), no LRU; widen if random reads ever thrash it
const OUTER_CACHE: usize = 16;
const SUPERBLOCK_MAGIC: u64 = 20_130_315;
const ICV: std::ops::Range<usize> = 0x380..0x3A0;
const SIGNED_REGION: usize = 0x5A0;

pub(crate) fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

pub(crate) fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

/// The package file, read by offset with every read checked against its length first.
pub(crate) struct Pkg {
    file: Box<dyn ReadSeek>,
    pub len: u64,
}

impl Pkg {
    pub fn new(file: Box<dyn ReadSeek>, len: u64) -> Self {
        Self { file, len }
    }

    pub fn read_at(&mut self, off: u64, buf: &mut [u8]) -> Result<()> {
        let fits = off
            .checked_add(buf.len() as u64)
            .is_some_and(|end| end <= self.len);
        if !fits {
            return Err(err(
                CORRUPT,
                format!(
                    "a read of {} bytes at {off:#x} runs past the package's end {:#x} (truncated?)",
                    buf.len(),
                    self.len
                ),
            ));
        }
        self.file.seek(SeekFrom::Start(off))?;
        self.file.read_exact(buf)?;
        Ok(())
    }
}

/// What the FIH header says about where everything is.
pub(crate) struct Header {
    pub pfs_offset: u64,
    pub outer_blocks: u64,
    pub sb_index: u64,
    pub game_digest: [u8; 32],
    pub cnt_offset: u64,
    /// The inner mount's metadata base, in bytes.
    pub meta_base: u64,
    pub format_version: u16,
}

pub(crate) fn header(pkg: &mut Pkg) -> Result<Header> {
    if pkg.len < FIH_LEN as u64 {
        return Err(err(
            NOT_PS5,
            format!("{} bytes, shorter than a package header", pkg.len),
        ));
    }
    let mut head = [0u8; FIH_LEN];
    pkg.read_at(0, &mut head)?;
    match &head[0..4] {
        b"\x7FFIH" => {}
        b"\x7FCNT" => {
            return Err(err(NOT_PS5, "this is a PS4 package (\\x7FCNT at offset 0)"));
        }
        _ => return Err(err(NOT_PS5, "no \\x7FFIH magic at offset 0")),
    }
    let fih = ps5upload_fpkg::fih::parse(&head)?;
    // The vendored reader's own debug/retail test: the signed byte is 0x00 on a debug image.
    if !fih.is_debug() {
        return Err(err(
            RETAIL,
            format!(
                "the header's signed byte is {:#04x} (debug packages carry 0x00); retail \
                 packages are out of scope",
                fih.signed_byte
            ),
        ));
    }
    let bad = |what: String| Err(err(CORRUPT, what));
    if fih.pfs_offset < FIH_LEN as u64 || !fih.pfs_offset.is_multiple_of(BLOCK) {
        return bad(format!("outer image offset {:#x}", fih.pfs_offset));
    }
    if fih.pfs_size == 0 || !fih.pfs_size.is_multiple_of(BLOCK) {
        return bad(format!(
            "outer image size {:#x} is not a whole number of blocks",
            fih.pfs_size
        ));
    }
    if fih
        .pfs_offset
        .checked_add(fih.pfs_size)
        .is_none_or(|e| e > pkg.len)
    {
        return bad(format!(
            "the outer image ({:#x} bytes at {:#x}) runs past the package's end {:#x} (truncated?)",
            fih.pfs_size, fih.pfs_offset, pkg.len
        ));
    }
    let outer_blocks = fih.pfs_size / BLOCK;
    if outer_blocks > u64::from(u32::MAX) {
        return bad(format!("{outer_blocks} outer blocks"));
    }
    let sb_abs = le64(&head, 0x20);
    let sb_rel = sb_abs.checked_sub(fih.pfs_offset);
    let Some(sb_index) = sb_rel
        .filter(|r| r.is_multiple_of(BLOCK))
        .map(|r| r / BLOCK)
        .filter(|&i| i < outer_blocks)
    else {
        return bad(format!(
            "the header's superblock offset {sb_abs:#x} is outside the outer image"
        ));
    };
    if fih
        .cnt_offset
        .checked_add(CNT_HEADER as u64)
        .is_none_or(|e| e > pkg.len)
    {
        return bad(format!(
            "the container offset {:#x} is past the package's end {:#x} (truncated?)",
            fih.cnt_offset, pkg.len
        ));
    }
    if le64(&head, 0x60) != BLOCK {
        return bad(format!("header block size {:#x}", le64(&head, 0x60)));
    }
    Ok(Header {
        pfs_offset: fih.pfs_offset,
        outer_blocks,
        sb_index,
        game_digest: fih.game_digest,
        cnt_offset: fih.cnt_offset,
        meta_base: u64::from(le32(&head, 0x50)) * BLOCK,
        format_version: fih.format_version,
    })
}

/// One container entry, its payload located in the package.
#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub id: u32,
    pub flags1: u32,
    /// Absolute offset in the package.
    pub at: u64,
    pub size: u64,
}

pub(crate) struct Container {
    pub content_id: String,
    pub entries: Vec<Entry>,
}

impl Container {
    pub fn entry(&self, id: u32) -> Option<Entry> {
        self.entries.iter().copied().find(|e| e.id == id)
    }
}

/// The container's header and entry table; payloads stay on disk until asked for.
pub(crate) fn container(pkg: &mut Pkg, cnt_offset: u64) -> Result<Container> {
    let mut head = vec![0u8; CNT_HEADER];
    pkg.read_at(cnt_offset, &mut head)?;
    if be32(&head, 0) != CNT_MAGIC {
        return Err(err(
            CORRUPT,
            format!("no \\x7FCNT container at {cnt_offset:#x}"),
        ));
    }
    let count = be32(&head, 0x10);
    if count > MAX_CNT_ENTRIES {
        return Err(err(
            CORRUPT,
            format!("the container claims {count} entries (at most {MAX_CNT_ENTRIES} accepted)"),
        ));
    }
    let table = u64::from(be32(&head, 0x18));
    // Both factors are bounded (4096 × 32), so the table is at most 128 KiB.
    let mut rows = vec![0u8; count as usize * CNT_ENTRY_LEN as usize];
    let table_at = cnt_offset
        .checked_add(table)
        .ok_or_else(|| err(CORRUPT, "the container's entry table offset overflows"))?;
    pkg.read_at(table_at, &mut rows)?;
    let mut entries = Vec::with_capacity(count as usize);
    for row in rows.as_chunks::<{ CNT_ENTRY_LEN as usize }>().0 {
        let id = be32(row, 0);
        let offset = u64::from(be32(row, 16));
        let size = u64::from(be32(row, 20));
        let at = cnt_offset + offset; // u64 + u32 cannot overflow once cnt_offset fits the file
        if at.checked_add(size).is_none_or(|e| e > pkg.len) {
            return Err(err(
                CORRUPT,
                format!("container entry {id:#06x} runs past the package's end (truncated?)"),
            ));
        }
        entries.push(Entry {
            id,
            flags1: be32(row, 8),
            at,
            size,
        });
    }
    let content_id = String::from_utf8_lossy(&head[0x40..0x64])
        .trim_end_matches('\0')
        .to_string();
    Ok(Container {
        content_id,
        entries,
    })
}

/// The outer superblock's fields this reader uses.
pub(crate) struct Superblock {
    pub dinode_count: u64,
    pub inode_table_block: u64,
    pub inode_table_digest: [u8; 32],
    pub seed: [u8; 16],
    pub plaintext: bool,
}

/// Parse and check the outer superblock (the vendored parser is crate-private).
pub(crate) fn superblock(sb: &[u8]) -> Result<Superblock> {
    if sb.len() < SIGNED_REGION || le64(sb, 0) != 2 || le64(sb, 8) != SUPERBLOCK_MAGIC {
        return Err(err(CORRUPT, "outer superblock version/magic mismatch"));
    }
    let mut zeroed = sb[..SIGNED_REGION].to_vec();
    zeroed[ICV].fill(0);
    if sha3(&zeroed) != sb[ICV] {
        return Err(err(CORRUPT, "outer superblock ICV mismatch"));
    }
    let mut seed = [0u8; 16];
    seed.copy_from_slice(&sb[0x370..0x380]);
    let mut inode_table_digest = [0u8; 32];
    inode_table_digest.copy_from_slice(&sb[0xB8..0xD8]);
    Ok(Superblock {
        dinode_count: le64(sb, 0x30),
        inode_table_block: u64::from(le32(sb, 0xD8)),
        inode_table_digest,
        seed,
        plaintext: seed == ps5upload_fpkg::PLAINTEXT_MARKER,
    })
}

/// The outer image as blocks: read, decrypted when native, checked against `imagedigs`.
pub(crate) struct Outer {
    pub pkg: Pkg,
    pfs_offset: u64,
    pub blocks: u64,
    sb_index: u64,
    /// Where `imagedigs` (32 bytes per block, byte-reversed) sits in the package.
    digests_at: u64,
    xts: Option<Xts>,
    cache: Vec<(u64, Vec<u8>)>,
    next: usize,
}

impl Outer {
    pub fn new(pkg: Pkg, h: &Header, digests_at: u64, xts: Option<Xts>) -> Self {
        Self {
            pkg,
            pfs_offset: h.pfs_offset,
            blocks: h.outer_blocks,
            sb_index: h.sb_index,
            digests_at,
            xts,
            cache: Vec::with_capacity(OUTER_CACHE),
            next: 0,
        }
    }

    pub fn set_xts(&mut self, xts: Option<Xts>) {
        self.xts = xts;
        self.cache.clear();
    }

    // ponytail: one SHA3 per 64 KiB read (and a 32-byte imagedigs read beside it), so a damaged
    // block is an error rather than wrong bytes; drop it for plaintext if throughput ever matters
    /// Block `index`'s plaintext, `None` when no reading of it matches its digest (the wrong key,
    /// or a damaged block). The superblock is never transformed; a native block is tried first
    /// with the sector this builder uses (its index before the superblock, bit 47 set after it)
    /// and then the other, since Sony's packages sign some blocks before the superblock too.
    pub fn try_load(&mut self, index: u64) -> Result<Option<Vec<u8>>> {
        if index >= self.blocks {
            return Err(err(
                CORRUPT,
                format!(
                    "outer block {index} is past the image's {} blocks",
                    self.blocks
                ),
            ));
        }
        let (raw, want) = self.load_raw(index)?;
        Ok(plaintext(
            self.xts.as_ref(),
            self.sb_index,
            index,
            raw,
            &want,
        ))
    }

    /// Block `index` as stored, and the digest its plaintext must have.
    fn load_raw(&mut self, index: u64) -> Result<(Vec<u8>, [u8; 32])> {
        let mut raw = vec![0u8; BLOCK as usize];
        self.pkg
            .read_at(self.pfs_offset + index * BLOCK, &mut raw)?;
        let mut want = [0u8; 32];
        self.pkg.read_at(self.digests_at + index * 32, &mut want)?;
        want.reverse();
        Ok((raw, want))
    }

    /// [`Outer::block`] for many blocks at once, uncached: read in order, then checked (and
    /// decrypted) on every core.
    pub fn blocks(&mut self, indices: &[u64]) -> Result<Vec<Vec<u8>>> {
        let mut loaded = Vec::with_capacity(indices.len());
        for &index in indices {
            if index >= self.blocks {
                return Err(err(
                    CORRUPT,
                    format!(
                        "outer block {index} is past the image's {} blocks",
                        self.blocks
                    ),
                ));
            }
            let (raw, want) = self.load_raw(index)?;
            loaded.push((index, raw, want));
        }
        let (xts, sb_index) = (self.xts.as_ref(), self.sb_index);
        crate::par_map(loaded, |(index, raw, want)| {
            plaintext(xts, sb_index, index, raw, &want).ok_or_else(|| {
                err(
                    CORRUPT,
                    format!("outer block {index} does not match its imagedigs entry"),
                )
            })
        })
        .into_iter()
        .collect()
    }

    /// Block `index`'s checked plaintext, from the ring when it is there.
    pub fn block(&mut self, index: u64) -> Result<&[u8]> {
        if let Some(pos) = self.cache.iter().position(|(i, _)| *i == index) {
            return Ok(&self.cache[pos].1);
        }
        let Some(plain) = self.try_load(index)? else {
            return Err(err(
                CORRUPT,
                format!("outer block {index} does not match its imagedigs entry"),
            ));
        };
        let slot = if self.cache.len() < OUTER_CACHE {
            self.cache.push((index, plain));
            self.cache.len() - 1
        } else {
            let slot = self.next;
            self.cache[slot] = (index, plain);
            self.next = (slot + 1) % OUTER_CACHE;
            slot
        };
        Ok(&self.cache[slot].1)
    }
}

/// Outer block `index`'s plaintext from its stored bytes, `None` when no reading of it matches
/// `want` (see [`Outer::try_load`]).
fn plaintext(
    xts: Option<&Xts>,
    sb_index: u64,
    index: u64,
    raw: Vec<u8>,
    want: &[u8; 32],
) -> Option<Vec<u8>> {
    let Some(xts) = xts.filter(|_| index != sb_index) else {
        return (sha3(&raw) == *want).then_some(raw);
    };
    let (first, second) = if index < sb_index {
        (index, SIGNED_SECTOR_FLAG | index)
    } else {
        (SIGNED_SECTOR_FLAG | index, index)
    };
    for sector in [first, second] {
        let mut pt = raw.clone();
        xts.decrypt(sector, &mut pt);
        if sha3(&pt) == *want {
            return Some(pt);
        }
    }
    None
}

/// One file of the outer PFS (`pfs_image.dat`, `naps_pkg_layout.dat`): its size and the outer
/// block behind each of its 64 KiB. Four bytes per block — 6.4 MB for a 100 GB image.
pub(crate) struct OuterFile {
    pub size: u64,
    map: Vec<u32>,
}

impl OuterFile {
    /// Map `node`'s blocks: the direct slots, then each indirect level, every table checked
    /// against the digest its parent records for it. Every table read adds at least one block
    /// or fails, so the walk is bounded by the file's own block count.
    pub fn map(outer: &mut Outer, node: &Dinode, what: &str) -> Result<Self> {
        let need = node.size.div_ceil(BLOCK);
        if need > outer.blocks {
            return Err(err(
                CORRUPT,
                format!(
                    "{what} claims {} bytes, more than the outer image holds",
                    node.size
                ),
            ));
        }
        let mut map = Vec::with_capacity(need as usize);
        for d in node.direct.iter().take(need.min(12) as usize) {
            push_block(outer, &mut map, d.block, what)?;
        }
        for (level, table) in node.indirect.iter().enumerate() {
            if map.len() as u64 >= need {
                break;
            }
            if table.block == 0 {
                break;
            }
            walk_table(
                outer,
                table.block,
                level,
                &table.digest,
                need,
                &mut map,
                what,
            )?;
        }
        if (map.len() as u64) < need {
            return Err(err(
                CORRUPT,
                format!(
                    "{what}'s block map covers {} of its {need} blocks",
                    map.len()
                ),
            ));
        }
        Ok(Self {
            size: node.size,
            map,
        })
    }

    /// Exactly `buf.len()` bytes at `off`.
    pub fn read(&self, outer: &mut Outer, off: u64, buf: &mut [u8]) -> Result<()> {
        if off
            .checked_add(buf.len() as u64)
            .is_none_or(|end| end > self.size)
        {
            return Err(err(
                CORRUPT,
                format!(
                    "a read of {} bytes at {off:#x} runs past an outer file of {:#x} bytes",
                    buf.len(),
                    self.size
                ),
            ));
        }
        let first = (off / BLOCK) as usize;
        let last = (off + buf.len() as u64).div_ceil(BLOCK) as usize;
        // A long read checks its blocks on every core; the ring would only hold its tail.
        // ponytail: the edge blocks of a long read may be loaded twice (once more by the next read)
        if last - first > 2 {
            let indices: Vec<u64> = self.map[first..last]
                .iter()
                .map(|&i| u64::from(i))
                .collect();
            let blocks = outer.blocks(&indices)?;
            let within = (off % BLOCK) as usize;
            let mut done = 0usize;
            for (k, block) in blocks.iter().enumerate() {
                let from = if k == 0 { within } else { 0 };
                let n = (BLOCK as usize - from).min(buf.len() - done);
                buf[done..done + n].copy_from_slice(&block[from..from + n]);
                done += n;
            }
            return Ok(());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = off + done as u64;
            let index = self.map[(at / BLOCK) as usize];
            let within = (at % BLOCK) as usize;
            let n = (BLOCK as usize - within).min(buf.len() - done);
            let block = outer.block(u64::from(index))?;
            buf[done..done + n].copy_from_slice(&block[within..within + n]);
            done += n;
        }
        Ok(())
    }
}

fn push_block(outer: &Outer, map: &mut Vec<u32>, block: u32, what: &str) -> Result<()> {
    if u64::from(block) >= outer.blocks {
        return Err(err(
            CORRUPT,
            format!("{what} points at outer block {block}, past the image"),
        ));
    }
    map.push(block);
    Ok(())
}

fn walk_table(
    outer: &mut Outer,
    block: u32,
    level: usize,
    digest: &[u8; 32],
    need: u64,
    map: &mut Vec<u32>,
    what: &str,
) -> Result<()> {
    if u64::from(block) >= outer.blocks {
        return Err(err(
            CORRUPT,
            format!("{what}'s indirect table at block {block} is past the image"),
        ));
    }
    let table = outer.block(u64::from(block))?.to_vec();
    if sha3(&table) != *digest {
        return Err(err(
            CORRUPT,
            format!("{what}'s indirect table at block {block} does not match its record"),
        ));
    }
    for slot in 0..PER_INDIRECT {
        if map.len() as u64 >= need {
            break;
        }
        let at = slot * 36;
        let child = le32(&table, at + 32);
        if child == 0 {
            // Tables fill from the front; an empty slot means the table ends short of the file.
            return Err(err(
                CORRUPT,
                format!("{what}'s indirect table at block {block} ends early"),
            ));
        }
        if level == 0 {
            push_block(outer, map, child, what)?;
        } else {
            let mut record = [0u8; 32];
            record.copy_from_slice(&table[at..at + 32]);
            walk_table(outer, child, level - 1, &record, need, map, what)?;
        }
    }
    Ok(())
}
