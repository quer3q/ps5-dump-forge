//! The AMPRPAK4 manifest reader, which runs every check the runtime makes when it opens a
//! manifest (so verification of a written pack and any unpack go through the same rules), the
//! volume, CRC sidecar and runtime profile checks, and [`Unpacked`]: a source tree with the
//! packed files decoded back and the pack artifacts hidden.

use std::collections::{HashMap, HashSet};

use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::{Error, Result};

use crate::format::*;
use crate::{CRC_SIDECAR, MANIFEST, PROFILE, RuntimeProfile};

/// Runtime source defaults (not proof of the release blobs' build overrides).
pub const MAX_MANIFEST_BYTES: u64 = 512 << 20;
pub const MAX_FILES: u64 = 2_000_000;
pub const MAX_CHUNKS: u64 = 16_000_000;
pub const MAX_PACKS: u32 = 1024;
/// AMPRCFG1 cache sizes are multiples of this.
pub const CACHE_PAGE: u64 = 16384;
const MAX_WORKERS: u32 = 16;
const PHYSICAL_ALIGN: u64 = 64;
const MIN_IO_PAGE: u32 = 1 << 12;
const MAX_IO_PAGE: u32 = 1 << 20;

fn bad(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

/// One manifest file record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    /// Relative to the game root (`/app0/` stripped), spelling kept.
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub flags: u32,
    pub first_chunk: u32,
    pub chunk_count: u32,
    pub block_shift: u8,
    pub packing_class: u8,
}

impl FileRecord {
    pub fn packed(&self) -> bool {
        self.flags & FILE_PACKED != 0
    }
}

/// One chunk record, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub pack_id: u16,
    /// Absolute byte offset in the volume.
    pub offset: u64,
    pub stored: u32,
    pub codec: Codec,
    pub flags: u8,
}

/// One pack (volume) record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// Relative to the game root.
    pub name: String,
    pub payload_bytes: u64,
    pub file_size: u64,
    pub flags: u32,
    pub io_page_size: u32,
}

impl Pack {
    pub fn payload_offset(&self) -> u64 {
        self.file_size - self.payload_bytes
    }
}

/// A manifest that passed every runtime-contract check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub build_id: [u8; 16],
    pub files: Vec<FileRecord>,
    pub chunks: Vec<Chunk>,
    pub packs: Vec<Pack>,
}

/// The NUL-terminated string at `off..off+len` of the string table.
fn string(strings: &[u8], off: u32, len: u32, what: &str) -> Result<String> {
    let (off, end) = (off as usize, off as usize + len as usize);
    if end >= strings.len() || strings[end] != 0 || strings[off..end].contains(&0) {
        return Err(bad(format!(
            "{what} string at {off} (+{len}) is not NUL-terminated inside the string table"
        )));
    }
    String::from_utf8(strings[off..end].to_vec())
        .map_err(|_| bad(format!("{what} string at {off} is not UTF-8")))
}

/// Checks a manifest the way the runtime does at open, and parses it.
pub fn open_manifest(b: &[u8]) -> Result<Manifest> {
    if b.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(bad(format!(
            "manifest is {} bytes, over {MAX_MANIFEST_BYTES}",
            b.len()
        )));
    }
    check_magic(b, PAK_MAGIC, "an AMPRPAK4 manifest")?;
    if b.len() < PAK_HEADER {
        return Err(bad("manifest is shorter than its header"));
    }
    let fixed = [
        (u32_at(b, 8)?, PAK_VERSION, "version"),
        (u32_at(b, 12)?, PAK_HEADER as u32, "header size"),
        (u32_at(b, 16)?, 0, "flags"),
        (u32_at(b, 20)?, ENDIAN_MARKER, "endian marker"),
        (u32_at(b, 60)?, FILE_RECORD as u32, "file record size"),
        (u32_at(b, 64)?, CHUNK_RECORD as u32, "chunk record size"),
        (u32_at(b, 68)?, PACK_RECORD as u32, "pack record size"),
    ];
    for (got, want, what) in fixed {
        if got != want {
            return Err(bad(format!("manifest {what} is {got:#x}, not {want:#x}")));
        }
    }
    if u64_at(b, 120)? != 0 {
        return Err(bad("manifest reserved field is not zero"));
    }
    if pak_header_crc(b)? != u32_at(b, PAK_HEADER_CRC_AT)? {
        return Err(bad("manifest header CRC mismatch"));
    }
    if pak_payload_crc(b)? != u32_at(b, PAK_PAYLOAD_CRC_AT)? {
        return Err(bad("manifest payload CRC mismatch"));
    }
    let build_id: [u8; 16] = b[24..40].try_into().expect("16 bytes");
    let file_count = u64_at(b, 40)?;
    let chunk_count = u64_at(b, 48)?;
    let pack_count = u32_at(b, 56)?;
    if file_count > MAX_FILES || chunk_count > MAX_CHUNKS || pack_count > MAX_PACKS {
        return Err(bad(format!(
            "manifest counts {file_count} files, {chunk_count} chunks, {pack_count} packs \
             exceed {MAX_FILES}/{MAX_CHUNKS}/{MAX_PACKS}"
        )));
    }
    // The counts are bounded above, so none of this overflows.
    let files_at = PAK_HEADER as u64;
    let chunks_at = files_at + file_count * FILE_RECORD as u64;
    let packs_at = chunks_at + chunk_count * CHUNK_RECORD as u64;
    let strings_at = packs_at + u64::from(pack_count) * PACK_RECORD as u64;
    let strings_size = u64_at(b, 104)?;
    let sections = [
        (u64_at(b, 72)?, files_at, "files"),
        (u64_at(b, 80)?, chunks_at, "chunks"),
        (u64_at(b, 88)?, packs_at, "packs"),
        (u64_at(b, 96)?, strings_at, "strings"),
    ];
    for (got, want, what) in sections {
        if got != want {
            return Err(bad(format!(
                "manifest {what} section is at {got}, not the gapless {want}"
            )));
        }
    }
    if strings_at.checked_add(strings_size) != Some(b.len() as u64) {
        return Err(bad(format!(
            "manifest is {} bytes, its sections end at {strings_at} + {strings_size}",
            b.len()
        )));
    }
    let strings = &b[strings_at as usize..];

    let mut packs = Vec::with_capacity(pack_count as usize);
    for i in 0..pack_count as usize {
        let r = packs_at as usize + i * PACK_RECORD;
        packs.push(parse_pack(b, r, strings).map_err(|e| bad(format!("pack {i}: {e}")))?);
    }

    let mut chunks = Vec::with_capacity(chunk_count as usize);
    for i in 0..chunk_count as usize {
        let r = chunks_at as usize + i * CHUNK_RECORD;
        let (pack_id, offset) = unpack_location(u64_at(b, r)?);
        let d = unpack_descriptor(u32_at(b, r + 8)?).map_err(|e| bad(format!("chunk {i}: {e}")))?;
        chunks.push(Chunk {
            pack_id,
            offset,
            stored: d.stored,
            codec: d.codec,
            flags: d.flags,
        });
    }

    let mut files = Vec::with_capacity(file_count as usize);
    let mut seen = HashSet::with_capacity(file_count as usize);
    for i in 0..file_count as usize {
        let r = files_at as usize + i * FILE_RECORD;
        let f = parse_file(b, r, strings, &chunks, &packs)
            .map_err(|e| bad(format!("file record {}: {e}", i + 1)))?;
        if !seen.insert(f.path.to_ascii_lowercase()) {
            return Err(bad(format!("{} is in the manifest twice", f.path)));
        }
        files.push(f);
    }
    Ok(Manifest {
        build_id,
        files,
        chunks,
        packs,
    })
}

fn parse_pack(b: &[u8], r: usize, strings: &[u8]) -> Result<Pack> {
    let name = string(strings, u32_at(b, r + 16)?, u32_at(b, r + 20)?, "pack name")?;
    check_pack_name(&name)?;
    let p = Pack {
        name,
        payload_bytes: u64_at(b, r)?,
        file_size: u64_at(b, r + 8)?,
        flags: u32_at(b, r + 24)?,
        io_page_size: u32_at(b, r + 28)?,
    };
    let page = u64::from(p.io_page_size);
    if !p.io_page_size.is_power_of_two()
        || !(MIN_IO_PAGE..=MAX_IO_PAGE).contains(&p.io_page_size)
        || !page.is_multiple_of(PHYSICAL_ALIGN)
    {
        return Err(bad(format!(
            "I/O page size {} is not valid",
            p.io_page_size
        )));
    }
    if p.flags & !(PACK_STRIPED | PACK_IO_PAGE_LAYOUT) != 0 || p.flags & PACK_IO_PAGE_LAYOUT == 0 {
        return Err(bad(format!("flags {:#x} are not valid", p.flags)));
    }
    if p.file_size < DAT_HEADER as u64
        || p.payload_bytes > p.file_size
        || p.payload_offset() < DAT_HEADER as u64
        || !p.payload_offset().is_multiple_of(page)
        || !p.file_size.is_multiple_of(page)
    {
        return Err(bad(format!(
            "{}: file size {} and payload {} do not fit its page geometry",
            p.name, p.file_size, p.payload_bytes
        )));
    }
    Ok(p)
}

fn parse_file(
    b: &[u8],
    r: usize,
    strings: &[u8],
    chunks: &[Chunk],
    packs: &[Pack],
) -> Result<FileRecord> {
    let logical = string(strings, u32_at(b, r + 32)?, u32_at(b, r + 36)?, "path")?;
    let path = logical_to_rel(&logical)?.to_string();
    if pak_hash(logical.as_bytes()) != u64_at(b, r)? {
        return Err(bad(format!("{logical}: path hash mismatch")));
    }
    if u16_at(b, r + 46)? != 0 {
        return Err(bad(format!("{logical}: reserved field is not zero")));
    }
    let f = FileRecord {
        path,
        size: u64_at(b, r + 8)?,
        mtime: i64_at(b, r + 16)?,
        first_chunk: u32_at(b, r + 24)?,
        chunk_count: u32_at(b, r + 28)?,
        flags: u32_at(b, r + 40)?,
        block_shift: b[r + 44],
        packing_class: b[r + 45],
    };
    if f.flags & !FILE_FLAGS_KNOWN != 0 {
        return Err(bad(format!(
            "{logical}: flags {:#x} are not known",
            f.flags
        )));
    }
    if !f.packed() {
        if f.flags != 0 || f.first_chunk != 0 || f.chunk_count != 0 || f.block_shift != 0 {
            return Err(bad(format!(
                "{logical}: loose record has packed fields set"
            )));
        }
        if f.packing_class != 0 {
            return Err(bad(format!("{logical}: loose record has a packing class")));
        }
        return Ok(f);
    }
    if !(MIN_BLOCK_SHIFT..=MAX_BLOCK_SHIFT).contains(&f.block_shift) {
        return Err(bad(format!(
            "{logical}: block shift {} is not 14..=20",
            f.block_shift
        )));
    }
    if f.flags & FILE_STREAMING != 0 && f.flags & FILE_RANDOM != 0 {
        return Err(bad(format!(
            "{logical}: streaming and random access are exclusive"
        )));
    }
    let block = 1u64 << f.block_shift;
    if u64::from(f.chunk_count) != f.size.div_ceil(block) {
        return Err(bad(format!(
            "{logical}: {} chunks for {} bytes in {block}-byte blocks",
            f.chunk_count, f.size
        )));
    }
    if u64::from(f.first_chunk) + u64::from(f.chunk_count) > chunks.len() as u64 {
        return Err(bad(format!(
            "{logical}: chunk range lies past the chunk table"
        )));
    }
    let streaming = f.flags & FILE_STREAMING != 0;
    for local in 0..f.chunk_count {
        let c = chunks[(f.first_chunk + local) as usize];
        let derived = block.min(f.size - u64::from(local) * block);
        let pack = packs
            .get(c.pack_id as usize)
            .ok_or_else(|| bad(format!("{logical}: chunk {local} names pack {}", c.pack_id)))?;
        let stored = u64::from(c.stored);
        let ok = stored <= block
            && (c.codec != Codec::Raw || stored == derived)
            && (f.flags & FILE_STORE_ONLY == 0 || c.codec == Codec::Raw)
            && streaming == (c.flags & CHUNK_STREAMING != 0)
            && placement_ok(pack, &c);
        if !ok {
            return Err(bad(format!(
                "{logical}: chunk {local} ({c:?}, {derived} raw bytes) breaks the pack rules"
            )));
        }
    }
    Ok(f)
}

/// The runtime's per-chunk geometry rules for page `P` of the chunk's pack.
fn placement_ok(pack: &Pack, c: &Chunk) -> bool {
    let (p, s, off) = (u64::from(pack.io_page_size), u64::from(c.stored), c.offset);
    let Some(end) = off.checked_add(s) else {
        return false;
    };
    let contained = c.flags & CHUNK_PAGE_CONTAINED != 0;
    let aligned = c.flags & CHUNK_PAGE_ALIGNED != 0;
    let streaming = c.flags & CHUNK_STREAMING != 0;
    if off < pack.payload_offset() || end > pack.file_size || !off.is_multiple_of(PHYSICAL_ALIGN) {
        return false;
    }
    if contained && (s > p || off / p != (end - 1) / p) {
        return false;
    }
    if aligned && (!off.is_multiple_of(p) || (s <= p && !contained)) {
        return false;
    }
    if !streaming
        && ((s <= p && !contained)
            || (s > p && !aligned)
            || (c.codec == Codec::Raw && s >= p && !aligned))
    {
        return false;
    }
    // Page-rounded range; the pack's payload start and end are page-aligned.
    let io_begin = off - off % p;
    let io_end = end.div_ceil(p).saturating_mul(p);
    io_begin >= pack.payload_offset() && io_end <= pack.file_size
}

/// Checks the AMPRDAT3 header of volume `pack_id` (its first 64 bytes) against the manifest and
/// the volume's actual size.
pub fn check_volume_header(h: &[u8], pack_id: usize, m: &Manifest, actual_size: u64) -> Result<()> {
    let pack = m
        .packs
        .get(pack_id)
        .ok_or_else(|| bad(format!("no pack {pack_id} in the manifest")))?;
    let name = &pack.name;
    check_magic(h, DAT_MAGIC, &format!("an AMPRDAT3 volume ({name})"))?;
    if h.len() < DAT_HEADER {
        return Err(bad(format!("{name}: header is cut short")));
    }
    let payload_offset = u64_at(h, 40)?;
    let payload_bytes = u64_at(h, 48)?;
    let ok = u32_at(h, 8)? == DAT_VERSION
        && u32_at(h, 12)? == DAT_HEADER as u32
        && u32_at(h, 16)? as usize == pack_id
        && u32_at(h, 20)? == pack.flags
        && u32_at(h, 60)? == 0
        && h[24..40] == m.build_id;
    if !ok {
        return Err(bad(format!(
            "{name}: volume header does not match pack {pack_id} of the manifest"
        )));
    }
    if dat_header_crc(h)? != u32_at(h, DAT_HEADER_CRC_AT)? {
        return Err(bad(format!("{name}: volume header CRC mismatch")));
    }
    let page = u64::from(pack.io_page_size);
    if payload_offset < DAT_HEADER as u64
        || !payload_offset.is_multiple_of(page)
        || payload_offset.checked_add(payload_bytes) != Some(pack.file_size)
        || payload_bytes != pack.payload_bytes
        || actual_size != pack.file_size
    {
        return Err(bad(format!(
            "{name}: {actual_size} bytes with payload {payload_bytes} at {payload_offset}, \
             the manifest says {} bytes with payload {}",
            pack.file_size, pack.payload_bytes
        )));
    }
    Ok(())
}

/// Checks an AMPRCRC1 sidecar; returns the decoded-chunk CRCs in chunk-table order.
pub fn read_crc_sidecar(b: &[u8], m: &Manifest) -> Result<Vec<u32>> {
    check_magic(b, CRC_MAGIC, "an AMPRCRC1 sidecar")?;
    let count = m.chunks.len();
    if b.len() != CRC_HEADER + 4 * count {
        return Err(bad(format!(
            "CRC sidecar is {} bytes, not {} for {count} chunks",
            b.len(),
            CRC_HEADER + 4 * count
        )));
    }
    let ok = u32_at(b, 8)? == CRC_VERSION
        && u32_at(b, 12)? == CRC_HEADER as u32
        && b[16..32] == m.build_id
        && u64_at(b, 32)? == count as u64;
    if !ok {
        return Err(bad("CRC sidecar header does not match the manifest"));
    }
    if crc1_payload_crc(b)? != u32_at(b, CRC_PAYLOAD_CRC_AT)?
        || crc1_header_crc(b)? != u32_at(b, CRC_HEADER_CRC_AT)?
    {
        return Err(bad("CRC sidecar CRC mismatch"));
    }
    (0..count).map(|i| u32_at(b, CRC_HEADER + 4 * i)).collect()
}

/// Checks an AMPRCFG1 runtime profile as the runtime loads it.
pub fn read_profile(b: &[u8], m: &Manifest) -> Result<RuntimeProfile> {
    check_magic(b, CFG_MAGIC, "an AMPRCFG1 runtime profile")?;
    if b.len() != CFG_SIZE {
        return Err(bad(format!(
            "runtime profile is {} bytes, not {CFG_SIZE}",
            b.len()
        )));
    }
    let p = RuntimeProfile {
        decoded_cache_bytes: u64_at(b, 32)?,
        physical_cache_bytes: u64_at(b, 40)?,
        workers: u32_at(b, 48)?,
        latency_reserve_workers: u32_at(b, 52)?,
    };
    let ok = u32_at(b, 8)? == CFG_VERSION
        && u32_at(b, 12)? == CFG_SIZE as u32
        && b[16..32] == m.build_id
        && u32_at(b, 60)? == 0
        && cfg_crc(b)? == u32_at(b, CFG_CRC_AT)?;
    if !ok {
        return Err(bad(
            "runtime profile header or CRC does not match the manifest",
        ));
    }
    if !(1..=MAX_WORKERS).contains(&p.workers)
        || p.latency_reserve_workers >= p.workers
        || !p.decoded_cache_bytes.is_multiple_of(CACHE_PAGE)
        || !p.physical_cache_bytes.is_multiple_of(CACHE_PAGE)
    {
        return Err(bad(format!(
            "runtime profile values are not valid: {p:?} (workers 1..=16, reserve below \
             workers, caches multiples of {CACHE_PAGE})"
        )));
    }
    Ok(p)
}

/// Whether `tree` holds packs: a root `ampr_assets.index` that starts with `AMPRPAK4`.
pub fn detect<T: SourceTree + ?Sized>(tree: &mut T) -> Result<bool> {
    if !tree.files().iter().any(|f| f.path == MANIFEST) {
        return Ok(false);
    }
    Ok(tree.read_range(MANIFEST, 0, 8)? == PAK_MAGIC)
}

/// A source tree with its packs decoded back: packed records become plain files, and the
/// manifest, the volumes it names and its two sidecars are hidden by exact name. Everything
/// else (loose records, an unrelated `foo.pak`, diagnostics) passes through unchanged.
pub struct Unpacked<T: SourceTree + ?Sized> {
    inner: Box<T>,
    manifest: Manifest,
    crcs: Option<Vec<u32>>,
    profile: Option<RuntimeProfile>,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    /// Packed records by path: index into `manifest.files`.
    packed: HashMap<String, usize>,
    hidden: HashSet<String>,
    /// The last decoded chunk: (chunk index, decoded length, bytes).
    cache: Option<(usize, usize, Vec<u8>)>,
}

/// Reads all of `path`, which the listing says is `size` bytes.
fn read_exact<T: SourceTree + ?Sized>(tree: &mut T, path: &str, size: u64) -> Result<Vec<u8>> {
    let b = tree.read_range(path, 0, usize::try_from(size).unwrap_or(usize::MAX))?;
    if b.len() as u64 != size {
        return Err(bad(format!("{path}: read {} of {size} bytes", b.len())));
    }
    Ok(b)
}

/// Opens the packs of `tree`: the manifest with every runtime check, every volume header, and
/// the CRC sidecar and runtime profile when present. Fails if a loose record's file is missing
/// or has the wrong size, or a packed path is also a physical file.
pub fn unpack<T: SourceTree + ?Sized>(mut inner: Box<T>) -> Result<Unpacked<T>> {
    let sizes: HashMap<String, u64> = inner
        .files()
        .iter()
        .map(|f| (f.path.clone(), f.size))
        .collect();
    let size = *sizes
        .get(MANIFEST)
        .ok_or_else(|| bad(format!("no {MANIFEST} at the root")))?;
    if size > MAX_MANIFEST_BYTES {
        return Err(bad(format!(
            "{MANIFEST} is {size} bytes, over {MAX_MANIFEST_BYTES}"
        )));
    }
    let manifest = open_manifest(&read_exact(&mut *inner, MANIFEST, size)?)?;
    for (id, pack) in manifest.packs.iter().enumerate() {
        let size = *sizes.get(&pack.name).ok_or_else(|| {
            bad(format!(
                "volume {} named in {MANIFEST} is missing",
                pack.name
            ))
        })?;
        let header = inner.read_range(&pack.name, 0, DAT_HEADER)?;
        check_volume_header(&header, id, &manifest, size)?;
    }
    let crcs = match sizes.get(CRC_SIDECAR) {
        Some(&n) if n != (CRC_HEADER + 4 * manifest.chunks.len()) as u64 => {
            return Err(bad(format!(
                "{CRC_SIDECAR} is {n} bytes, not what the manifest needs"
            )));
        }
        Some(&n) => Some(read_crc_sidecar(
            &read_exact(&mut *inner, CRC_SIDECAR, n)?,
            &manifest,
        )?),
        None => None,
    };
    let profile = match sizes.get(PROFILE) {
        Some(&n) if n != CFG_SIZE as u64 => {
            return Err(bad(format!("{PROFILE} is {n} bytes, not {CFG_SIZE}")));
        }
        Some(&n) => Some(read_profile(
            &read_exact(&mut *inner, PROFILE, n)?,
            &manifest,
        )?),
        None => None,
    };

    let mut hidden: HashSet<String> = [MANIFEST, CRC_SIDECAR, PROFILE].map(String::from).into();
    hidden.extend(manifest.packs.iter().map(|p| p.name.clone()));
    let physical: HashSet<String> = sizes.keys().map(|p| p.to_ascii_lowercase()).collect();
    let mut files: Vec<SourceFile> = inner
        .files()
        .iter()
        .filter(|f| !hidden.contains(&f.path))
        .cloned()
        .collect();
    let mut packed = HashMap::new();
    for (i, f) in manifest.files.iter().enumerate() {
        if hidden.contains(&f.path) {
            return Err(bad(format!(
                "{} is both a manifest record and a pack artifact",
                f.path
            )));
        }
        if !f.packed() {
            match sizes.get(&f.path) {
                Some(&n) if n == f.size => {}
                Some(&n) => {
                    return Err(bad(format!(
                        "loose {} is {n} bytes, the manifest says {}",
                        f.path, f.size
                    )));
                }
                None => return Err(bad(format!("loose {} is missing", f.path))),
            }
            continue;
        }
        if physical.contains(&f.path.to_ascii_lowercase()) {
            return Err(bad(format!(
                "{} is both packed and a physical file",
                f.path
            )));
        }
        packed.insert(f.path.clone(), i);
        files.push(SourceFile {
            path: f.path.clone(),
            size: f.size,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    // A path cannot be a file and a folder at once; an underlying empty folder that now holds
    // a packed file is not empty any more.
    let folded: HashSet<String> = files.iter().map(|f| f.path.to_ascii_lowercase()).collect();
    let mut dirs = HashSet::new();
    for f in &folded {
        let mut at = 0;
        while let Some(i) = f[at..].find('/') {
            dirs.insert(f[..at + i].to_string());
            at += i + 1;
        }
    }
    if let Some(p) = folded.iter().find(|p| dirs.contains(*p)) {
        return Err(bad(format!("{p} is both a file and a folder")));
    }
    let mut empty_dirs = Vec::new();
    for d in inner.empty_dirs() {
        let key = d.to_ascii_lowercase();
        if folded.contains(&key) {
            return Err(bad(format!(
                "{d} is both a packed file and an empty folder"
            )));
        }
        // No ancestor of an empty folder may be a file either.
        let mut at = 0;
        while let Some(i) = key[at..].find('/') {
            if folded.contains(&key[..at + i]) {
                return Err(bad(format!("{d} sits under a packed file")));
            }
            at += i + 1;
        }
        if !dirs.contains(&key) {
            empty_dirs.push(d.clone());
        }
    }
    Ok(Unpacked {
        inner,
        manifest,
        crcs,
        profile,
        files,
        empty_dirs,
        packed,
        hidden,
        cache: None,
    })
}

impl<T: SourceTree + ?Sized> Unpacked<T> {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The validated runtime profile, if the tree has one.
    pub fn profile(&self) -> Option<RuntimeProfile> {
        self.profile
    }

    /// Whether decoded chunks are checked against a CRC sidecar.
    pub fn has_crc(&self) -> bool {
        self.crcs.is_some()
    }

    /// Decodes chunk `local` of packed record `i` into the cache.
    fn decode(&mut self, i: usize, local: u64) -> Result<&[u8]> {
        let f = &self.manifest.files[i];
        let block = 1u64 << f.block_shift;
        let idx = f.first_chunk as usize + local as usize;
        let derived = block.min(f.size - local * block) as usize;
        if !matches!(&self.cache, Some((c, n, _)) if *c == idx && *n == derived) {
            let c = self.manifest.chunks[idx];
            let pack = &self.manifest.packs[c.pack_id as usize];
            let stored = self
                .inner
                .read_range(&pack.name, c.offset, c.stored as usize)?;
            let what = || format!("{} block {local} ({} at {})", f.path, pack.name, c.offset);
            if stored.len() != c.stored as usize {
                return Err(bad(format!("{}: volume is cut short", what())));
            }
            let data = match c.codec {
                Codec::Raw => stored,
                Codec::Lz4 => {
                    let mut out = vec![0; derived];
                    match lz4_flex::block::decompress_into(&stored, &mut out) {
                        Ok(n) if n == derived => out,
                        _ => {
                            return Err(bad(format!(
                                "{}: LZ4 data does not decode to {derived} bytes",
                                what()
                            )));
                        }
                    }
                }
            };
            if let Some(crcs) = &self.crcs
                && crc32(&data) != crcs[idx]
            {
                return Err(bad(format!("{}: decoded CRC mismatch", what())));
            }
            self.cache = Some((idx, derived, data));
        }
        Ok(&self.cache.as_ref().expect("filled above").2)
    }

    fn read_packed(&mut self, i: usize, offset: u64, len: usize) -> Result<Vec<u8>> {
        let (size, shift) = (
            self.manifest.files[i].size,
            self.manifest.files[i].block_shift,
        );
        let end = offset.saturating_add(len as u64).min(size);
        if offset >= end {
            return Ok(Vec::new());
        }
        let block = 1u64 << shift;
        let mut out = Vec::with_capacity((end - offset) as usize);
        for local in offset / block..=(end - 1) / block {
            let start = local * block;
            let data = self.decode(i, local)?;
            let to = (end - start).min(data.len() as u64) as usize;
            out.extend_from_slice(&data[(offset.max(start) - start) as usize..to]);
        }
        Ok(out)
    }
}

impl<T: SourceTree + ?Sized> SourceTree for Unpacked<T> {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let size = self
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .map(|i| self.files[i].size)
            .map_err(|_| bad(format!("{path} is not in the unpacked tree")))?;
        read_exact(self, path, size)
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(&i) = self.packed.get(path) {
            return self.read_packed(i, offset, len);
        }
        if self.hidden.contains(path) {
            return Err(bad(format!("{path} is not in the unpacked tree")));
        }
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!(
            "{} with LZ4 packs unpacked ({} packed files in {} volumes)",
            self.inner.describe(),
            self.packed.len(),
            self.manifest.packs.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume_name;
    use crate::writer::testkit::*;
    use crate::writer::{PAGE, VOLUME_CAP};
    use crate::{JOURNAL, PackSpec};

    type Entries = Vec<(&'static str, Vec<u8>, Option<PackSpec>)>;

    fn sample() -> (Entries, MemOut) {
        let entries: Entries = vec![
            ("data/a.bin", text(200_000, 1), spec(16)),
            ("data/b.bin", noise(70_000, 2), spec(16)),
            ("eboot.bin", noise(300, 3), None),
        ];
        let (_, _, out, _) = pack_mem(&entries, None, 2, VOLUME_CAP).unwrap();
        (entries, out)
    }

    /// The deployed tree with `edit` applied to its file map.
    fn tree(edit: impl FnOnce(&mut std::collections::BTreeMap<String, Vec<u8>>)) -> Mem {
        let (entries, out) = sample();
        let mut files: std::collections::BTreeMap<String, Vec<u8>> = entries
            .iter()
            .filter(|e| e.2.is_none())
            .map(|(p, d, _)| (p.to_string(), d.clone()))
            .chain(out.files)
            .collect();
        edit(&mut files);
        Mem::new(files)
    }

    fn err(t: Mem) -> String {
        match unpack(Box::new(t)) {
            Err(Error::Format(m)) => m,
            Err(e) => panic!("{e}"),
            Ok(_) => panic!("accepted"),
        }
    }

    #[test]
    fn detects_by_root_magic() {
        assert!(detect(&mut tree(|_| {})).unwrap());
        assert!(!detect(&mut Mem::new([("x".to_string(), vec![1])])).unwrap());
        let mut other = Mem::new([(MANIFEST.to_string(), b"AMPRPAK3........".to_vec())]);
        assert!(!detect(&mut other).unwrap());
        let mut nested = Mem::new([(format!("d/{MANIFEST}"), PAK_MAGIC.to_vec())]);
        assert!(!detect(&mut nested).unwrap());
    }

    #[test]
    fn hides_exactly_the_pack_artifacts() {
        let t = tree(|f| {
            f.insert("foo.pak".into(), vec![1]);
            f.insert("ampr_assets-001.pak".into(), vec![2]);
            f.insert(format!("d/{CRC_SIDECAR}"), vec![3]);
            f.insert(JOURNAL.into(), vec![4]);
        });
        let mut u = unpack(Box::new(t)).unwrap();
        let names: Vec<&str> = u.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            names,
            [
                "ampr_assets-001.pak",
                JOURNAL,
                "d/ampr_assets.index.crc",
                "data/a.bin",
                "data/b.bin",
                "eboot.bin",
                "foo.pak"
            ]
        );
        assert!(u.read(MANIFEST).is_err());
        assert!(u.read_range(&volume_name(0), 0, 4).is_err());
        assert_eq!(u.read("foo.pak").unwrap(), [1]);
        assert!(u.has_crc());
        assert!(u.describe().contains("2 packed files in 1 volumes"));
    }

    #[test]
    fn loose_records_must_match_and_packed_paths_must_not_exist() {
        assert!(err(tree(|f| drop(f.remove("eboot.bin")))).contains("missing"));
        assert!(err(tree(|f| drop(f.insert("eboot.bin".into(), vec![0; 301])))).contains("301"));
        let both = err(tree(|f| drop(f.insert("DATA/A.bin".into(), vec![0]))));
        assert!(both.contains("both packed and a physical"), "{both}");
        let dir = err(tree(|f| drop(f.insert("data/a.bin/x".into(), vec![0]))));
        assert!(dir.contains("file and a folder"), "{dir}");
    }

    #[test]
    fn an_empty_folder_under_a_file_is_refused() {
        let mut t = tree(|_| {});
        t.dirs = vec!["data/a.bin/empty".into()];
        assert!(err(t).contains("under a packed file"));
    }

    #[test]
    fn empty_dirs_are_kept_unless_they_now_hold_files() {
        let mut t = tree(|_| {});
        t.dirs = vec!["data".into(), "keep/me".into()];
        let u = unpack(Box::new(t)).unwrap();
        assert_eq!(u.empty_dirs(), ["keep/me"]);
    }

    #[test]
    fn volume_problems_fail_at_open() {
        let v = volume_name(0);
        assert!(err(tree(|f| drop(f.remove(&v)))).contains("missing"));
        assert!(err(tree(|f| f.get_mut(&v).unwrap().truncate(PAGE as usize))).contains(&v));
        assert!(err(tree(|f| f.get_mut(&v).unwrap()[30] ^= 1)).contains(&v));
        let crc = err(tree(|f| f.get_mut(&v).unwrap()[61] ^= 1));
        assert!(crc.contains(&v), "{crc}");
    }

    #[test]
    fn sidecars_are_checked_when_present() {
        // Absent is fine.
        let mut u = unpack(Box::new(tree(|f| drop(f.remove(CRC_SIDECAR))))).unwrap();
        assert!(!u.has_crc());
        assert_eq!(u.read("data/a.bin").unwrap(), text(200_000, 1));
        assert!(err(tree(|f| f.get_mut(CRC_SIDECAR).unwrap()[50] ^= 1)).contains("CRC"));
        assert!(err(tree(|f| f.get_mut(CRC_SIDECAR).unwrap().push(0))).contains("bytes"));
        // A profile is checked whole, CRC first, then its values.
        let (_, out) = sample();
        let id = &out.files[MANIFEST][24..40];
        let profile = |workers: u32, reserve: u32, cache: u64| {
            let mut b = vec![0u8; CFG_SIZE];
            put_bytes(&mut b, 0, CFG_MAGIC).unwrap();
            put_u32(&mut b, 8, 1).unwrap();
            put_u32(&mut b, 12, 64).unwrap();
            put_bytes(&mut b, 16, id).unwrap();
            put_u64(&mut b, 32, cache).unwrap();
            put_u32(&mut b, 48, workers).unwrap();
            put_u32(&mut b, 52, reserve).unwrap();
            let crc = cfg_crc(&b).unwrap();
            put_u32(&mut b, CFG_CRC_AT, crc).unwrap();
            b
        };
        let with = |b: Vec<u8>| tree(move |f| drop(f.insert(PROFILE.into(), b)));
        let ok = unpack(Box::new(with(profile(16, 15, 32768)))).unwrap();
        assert_eq!(ok.profile().unwrap().workers, 16);
        for (w, r, c) in [(0, 0, 0), (17, 0, 0), (4, 4, 0), (4, 0, 16383)] {
            assert!(
                err(with(profile(w, r, c))).contains("not valid"),
                "{w} {r} {c}"
            );
        }
        let mut b = profile(4, 0, 0);
        b[40] = 1;
        assert!(err(with(b)).contains("CRC"));
        assert!(err(with(vec![0; 65])).contains("bytes"));
    }

    #[test]
    fn decoded_crc_mismatch_fails_the_read() {
        let v = volume_name(0);
        // The noise file is RAW: flip one of its stored bytes.
        let (_, out) = sample();
        let m = open_manifest(&out.files[MANIFEST]).unwrap();
        let b = &m.files[1];
        let c = m.chunks[b.first_chunk as usize];
        assert_eq!(c.codec, Codec::Raw);
        let at = c.offset as usize + 10;
        let mut u = unpack(Box::new(tree(|f| f.get_mut(&v).unwrap()[at] ^= 1))).unwrap();
        assert!(u.read_range("data/a.bin", 0, 10).is_ok());
        let e = u.read("data/b.bin").unwrap_err().to_string();
        assert!(e.contains("CRC mismatch"), "{e}");
        // Without the sidecar the flipped byte reads through.
        let mut u = unpack(Box::new(tree(|f| {
            f.get_mut(&v).unwrap()[at] ^= 1;
            f.remove(CRC_SIDECAR);
        })))
        .unwrap();
        assert_ne!(u.read("data/b.bin").unwrap(), noise(70_000, 2));
        // A cut-short volume is caught at open (size), never read past.
        let short = err(tree(|f| f.get_mut(&v).unwrap().truncate(c.offset as usize)));
        assert!(short.contains(&v));
    }

    /// Recomputes both manifest CRCs after a mutation.
    fn fix(m: &mut [u8]) {
        let c = pak_payload_crc(m).unwrap();
        put_u32(m, PAK_PAYLOAD_CRC_AT, c).unwrap();
        let c = pak_header_crc(m).unwrap();
        put_u32(m, PAK_HEADER_CRC_AT, c).unwrap();
    }

    /// The sample tree with its manifest mutated (CRCs fixed when `refix`).
    fn mutated(refix: bool, edit: impl FnOnce(&mut Vec<u8>)) -> String {
        err(tree(|f| {
            let m = f.get_mut(MANIFEST).unwrap();
            edit(m);
            if refix {
                fix(m);
            }
        }))
    }

    #[test]
    fn malformed_manifests_are_rejected() {
        let file = |i: usize| 128 + 48 * i;
        let chunks_at = |m: &[u8]| u64_at(m, 80).unwrap() as usize;
        let strings_at = |m: &[u8]| u64_at(m, 96).unwrap() as usize;
        assert!(mutated(false, |m| m[3] = b'X').contains("magic"));
        assert!(mutated(false, |m| m[130] ^= 1).contains("CRC"));
        assert!(mutated(false, |m| m[41] ^= 1).contains("CRC"));
        assert!(mutated(true, |m| m[8] = 3).contains("version"));
        assert!(mutated(true, |m| m[16] = 1).contains("flags"));
        assert!(mutated(true, |m| m[120] = 1).contains("reserved"));
        let gap = mutated(true, |m| {
            let at = strings_at(m);
            m.insert(at, 0);
            put_u64(m, 96, at as u64 + 1).unwrap();
        });
        assert!(gap.contains("gapless"), "{gap}");
        assert!(mutated(true, |m| m.push(0)).contains("sections end"));
        assert!(mutated(true, |m| put_u64(m, 40, MAX_FILES + 1).unwrap()).contains("exceed"));
        assert!(mutated(true, |m| m[file(0) + 40] |= 0x20).contains("not known"));
        assert!(mutated(true, |m| m[file(2) + 44] = 16).contains("loose record"));
        assert!(mutated(true, |m| m[file(0) + 44] = 13).contains("block shift"));
        assert!(mutated(true, |m| m[file(0) + 46] = 1).contains("reserved"));
        assert!(mutated(true, |m| m[file(0)] ^= 1).contains("hash"));
        let range = mutated(true, |m| put_u32(m, file(1) + 24, 1000).unwrap());
        assert!(range.contains("past the chunk table"), "{range}");
        assert!(mutated(true, |m| m[file(0) + 28] += 1).contains("chunks for"));
        let far = mutated(true, |m| {
            let at = chunks_at(m);
            put_u64(m, at, 1 << 40).unwrap();
        });
        assert!(far.contains("breaks the pack rules"), "{far}");
        let pack = mutated(true, |m| {
            let at = chunks_at(m);
            m[at + 6] = 1;
        });
        assert!(pack.contains("names pack 1"), "{pack}");
        let nul = mutated(true, |m| {
            let at = strings_at(m) + 7;
            m[at] = 0;
        });
        assert!(nul.contains("NUL"), "{nul}");
    }

    struct Hand {
        /// Logical path, size, flags, first chunk, chunk count, block shift.
        files: Vec<(&'static str, u64, u32, u32, u32, u8)>,
        /// Pack id, offset, stored, codec, flags.
        chunks: Vec<(u16, u64, u32, Codec, u8)>,
        /// Name, file size, payload offset, page; flags are IoPageLayout.
        packs: Vec<(&'static str, u64, u64, u32)>,
        /// Volume contents: pack id, offset, bytes.
        data: Vec<(usize, u64, Vec<u8>)>,
    }

    impl Hand {
        fn tree(&self) -> Mem {
            let id = [7u8; 16];
            let mut strings = Vec::new();
            let mut s = |v: &str| {
                let at = strings.len() as u32;
                strings.extend_from_slice(v.as_bytes());
                strings.push(0);
                (at, v.len() as u32)
            };
            let mut recs = Vec::new();
            for &(p, size, flags, first, count, shift) in &self.files {
                let mut r = [0u8; FILE_RECORD];
                put_u64(&mut r, 0, pak_hash(p.as_bytes())).unwrap();
                put_u64(&mut r, 8, size).unwrap();
                put_u32(&mut r, 24, first).unwrap();
                put_u32(&mut r, 28, count).unwrap();
                let (at, len) = s(p);
                put_u32(&mut r, 32, at).unwrap();
                put_u32(&mut r, 36, len).unwrap();
                put_u32(&mut r, 40, flags).unwrap();
                r[44] = shift;
                recs.extend_from_slice(&r);
            }
            let mut chunks = Vec::new();
            for &(pack, off, stored, codec, flags) in &self.chunks {
                chunks.extend_from_slice(&pack_location(pack, off).unwrap().to_le_bytes());
                let d = pack_descriptor(stored, codec, flags).unwrap();
                chunks.extend_from_slice(&d.to_le_bytes());
            }
            let mut packs = Vec::new();
            let mut vols = Vec::new();
            for (i, &(name, size, payload_at, page)) in self.packs.iter().enumerate() {
                let mut r = [0u8; PACK_RECORD];
                put_u64(&mut r, 0, size - payload_at).unwrap();
                put_u64(&mut r, 8, size).unwrap();
                let (at, len) = s(name);
                put_u32(&mut r, 16, at).unwrap();
                put_u32(&mut r, 20, len).unwrap();
                put_u32(&mut r, 24, PACK_IO_PAGE_LAYOUT).unwrap();
                put_u32(&mut r, 28, page).unwrap();
                packs.extend_from_slice(&r);
                let mut v = vec![0u8; size as usize];
                put_bytes(&mut v, 0, DAT_MAGIC).unwrap();
                put_u32(&mut v, 8, 3).unwrap();
                put_u32(&mut v, 12, 64).unwrap();
                put_u32(&mut v, 16, i as u32).unwrap();
                put_u32(&mut v, 20, PACK_IO_PAGE_LAYOUT).unwrap();
                put_bytes(&mut v, 24, &id).unwrap();
                put_u64(&mut v, 40, payload_at).unwrap();
                put_u64(&mut v, 48, size - payload_at).unwrap();
                let c = dat_header_crc(&v).unwrap();
                put_u32(&mut v, DAT_HEADER_CRC_AT, c).unwrap();
                for (_, off, d) in self.data.iter().filter(|d| d.0 == i) {
                    put_bytes(&mut v, *off as usize, d).unwrap();
                }
                vols.push((name.to_string(), v));
            }
            let mut m = vec![0u8; PAK_HEADER];
            put_bytes(&mut m, 0, PAK_MAGIC).unwrap();
            for (at, v) in [
                (8, 4),
                (12, 128),
                (20, ENDIAN_MARKER),
                (60, 48),
                (64, 12),
                (68, 32),
            ] {
                put_u32(&mut m, at, v).unwrap();
            }
            put_bytes(&mut m, 24, &id).unwrap();
            put_u64(&mut m, 40, self.files.len() as u64).unwrap();
            put_u64(&mut m, 48, self.chunks.len() as u64).unwrap();
            put_u32(&mut m, 56, self.packs.len() as u32).unwrap();
            let mut at = 128u64;
            for (field, part) in [(72, &recs), (80, &chunks), (88, &packs), (96, &strings)] {
                put_u64(&mut m, field, at).unwrap();
                at += part.len() as u64;
                m.extend_from_slice(part);
            }
            put_u64(&mut m, 104, strings.len() as u64).unwrap();
            fix(&mut m);
            Mem::new(vols.into_iter().chain([(MANIFEST.to_string(), m)]))
        }
    }

    const P: u64 = PAGE;
    const PACKED: u32 = FILE_PACKED;
    const BOTH: u8 = CHUNK_PAGE_CONTAINED | CHUNK_PAGE_ALIGNED;

    /// One file of one chunk of exactly a page, in a 64 KiB-page volume.
    fn one_page(codec: Codec, flags: u8, off: u64) -> Hand {
        Hand {
            files: vec![("/app0/x", P, PACKED, 0, 1, 17)],
            chunks: vec![(0, off, P as u32, codec, flags)],
            packs: vec![("v.pak", 4 * P, P, P as u32)],
            data: vec![],
        }
    }

    #[test]
    fn raw_of_exactly_one_page_must_be_aligned() {
        let c = CHUNK_PAGE_CONTAINED;
        assert!(err(one_page(Codec::Raw, c, P).tree()).contains("breaks the pack rules"));
        assert!(unpack(Box::new(one_page(Codec::Raw, BOTH, P).tree())).is_ok());
        // LZ4 of a page needs containment only.
        assert!(unpack(Box::new(one_page(Codec::Lz4, c, P).tree())).is_ok());
        // Crossing a page, or a flag the placement does not have.
        assert!(err(one_page(Codec::Lz4, c, P + 64).tree()).contains("breaks"));
        assert!(err(one_page(Codec::Raw, 0, P).tree()).contains("breaks"));
        assert!(err(one_page(Codec::Raw, BOTH, P + 64).tree()).contains("breaks"));
        // 64-byte alignment, and the payload range.
        let mut h = one_page(Codec::Raw, BOTH, P);
        h.files[0] = ("/app0/x", 100, PACKED, 0, 1, 17);
        h.chunks[0] = (0, P + 32, 100, Codec::Raw, CHUNK_PAGE_CONTAINED);
        assert!(err(h.tree()).contains("breaks"));
        h.chunks[0] = (0, 4 * P - 64, 100, Codec::Raw, CHUNK_PAGE_CONTAINED);
        assert!(err(h.tree()).contains("breaks"));
        h.chunks[0] = (0, 0, 100, Codec::Raw, BOTH);
        assert!(err(h.tree()).contains("breaks"));
    }

    #[test]
    fn bad_paths_are_rejected() {
        for p in [
            "/app0/../x",
            "/app0/a//b",
            "/app1/x",
            "/app0/a\\b",
            "/app0/x/",
        ] {
            let mut h = one_page(Codec::Raw, BOTH, P);
            h.files[0].0 = p;
            assert!(unpack(Box::new(h.tree())).is_err(), "{p}");
        }
        let mut h = one_page(Codec::Raw, BOTH, P);
        h.packs[0].0 = "../v.pak";
        assert!(err(h.tree()).contains("pack 0"));
        let mut h = one_page(Codec::Raw, BOTH, P);
        h.files.push(("/APP0/X", 0, 0, 0, 0, 0));
        assert!(err(h.tree()).contains("twice"));
    }

    #[test]
    fn accepts_upstream_geometry_forge_does_not_emit() {
        let page = 4096u64;
        let raw = noise(1000, 1);
        let lz = lz4_flex::block::compress(&raw);
        assert!(lz.len() > raw.len(), "LZ4 of noise is not smaller");
        let tail = text(500, 2);
        let shared = BOTH | CHUNK_SHARED;
        let h = Hand {
            files: vec![
                ("/app0/s1", 1000, PACKED, 0, 1, 14),
                ("/app0/s2", 1000, PACKED | FILE_HOT, 0, 1, 14),
                ("/app0/st", 500, PACKED | FILE_STREAMING, 1, 1, 14),
                ("/app0/empty", 0, PACKED, 0, 0, 14),
            ],
            chunks: vec![
                (0, page, lz.len() as u32, Codec::Lz4, shared),
                (0, 2 * page + 64, 500, Codec::Raw, CHUNK_STREAMING),
            ],
            packs: vec![("sub/v.pak", 3 * page, page, page as u32)],
            data: vec![(0, page, lz.clone()), (0, 2 * page + 64, tail.clone())],
        };
        let mut u = unpack(Box::new(h.tree())).unwrap();
        assert_eq!(u.read("s1").unwrap(), raw);
        assert_eq!(u.read("s2").unwrap(), raw);
        assert_eq!(u.read_range("s2", 990, 100).unwrap(), &raw[990..]);
        assert_eq!(u.read("st").unwrap(), tail);
        assert_eq!(u.read("empty").unwrap(), Vec::<u8>::new());
        assert!(!u.files().iter().any(|f| f.path == "sub/v.pak"));
        assert!(u.empty_dirs().is_empty());

        // Streaming must agree between file and chunk.
        let mut bad = Hand {
            files: h.files.clone(),
            ..h
        };
        bad.files[2].2 = PACKED;
        assert!(err(bad.tree()).contains("breaks"));
        bad.files[2].2 = PACKED | FILE_STREAMING | FILE_RANDOM;
        assert!(err(bad.tree()).contains("exclusive"));
        // LZ4 that decodes to another length opens, then fails the read.
        bad.files[2].2 = PACKED | FILE_STREAMING;
        bad.files[0].1 = 999;
        let mut u = unpack(Box::new(bad.tree())).unwrap();
        let e = u.read("s1").unwrap_err().to_string();
        assert!(e.contains("does not decode to 999"), "{e}");
    }
}
