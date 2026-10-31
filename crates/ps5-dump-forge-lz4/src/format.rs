//! Byte-level pieces of the AMPR pack formats: magics, sizes, flag bits, checked little-endian
//! access, the two path hashes, the CRC ranges, logical path
//! rules and the chunk `location`/`descriptor` words. Nothing here maps a struct onto bytes.

use ps5upload_fpkg::{Error, Result};

pub const PAK_MAGIC: &[u8; 8] = b"AMPRPAK4";
pub const DAT_MAGIC: &[u8; 8] = b"AMPRDAT3";
pub const CFG_MAGIC: &[u8; 8] = b"AMPRCFG1";
pub const CRC_MAGIC: &[u8; 8] = b"AMPRCRC1";
pub const IDX_MAGIC: &[u8; 8] = b"AMPRIDX3";

pub const PAK_VERSION: u32 = 4;
pub const DAT_VERSION: u32 = 3;
pub const CFG_VERSION: u32 = 1;
pub const CRC_VERSION: u32 = 1;
pub const IDX_VERSION: u32 = 3;
pub const ENDIAN_MARKER: u32 = 0x0102_0304;

pub const PAK_HEADER: usize = 128;
pub const FILE_RECORD: usize = 48;
pub const CHUNK_RECORD: usize = 12;
pub const PACK_RECORD: usize = 32;
pub const DAT_HEADER: usize = 64;
pub const CFG_SIZE: usize = 64;
pub const CRC_HEADER: usize = 48;
pub const IDX_HEADER: usize = 48;
pub const IDX_RECORD: usize = 24;
pub const IDX_SLOT: usize = 16;

/// Offsets of the CRC fields inside their headers.
pub const PAK_PAYLOAD_CRC_AT: usize = 112;
pub const PAK_HEADER_CRC_AT: usize = 116;
pub const DAT_HEADER_CRC_AT: usize = 56;
pub const CFG_CRC_AT: usize = 56;
pub const CRC_PAYLOAD_CRC_AT: usize = 40;
pub const CRC_HEADER_CRC_AT: usize = 44;

// File record flags.
pub const FILE_PACKED: u32 = 1;
pub const FILE_STORE_ONLY: u32 = 2;
pub const FILE_STREAMING: u32 = 4;
pub const FILE_HOT: u32 = 8;
pub const FILE_RANDOM: u32 = 16;
pub const FILE_FLAGS_KNOWN: u32 = 0x1f;

// Chunk descriptor flags.
pub const CHUNK_SHARED: u8 = 1;
pub const CHUNK_STREAMING: u8 = 2;
pub const CHUNK_PAGE_CONTAINED: u8 = 4;
pub const CHUNK_PAGE_ALIGNED: u8 = 8;
pub const CHUNK_FLAGS_KNOWN: u8 = 0x0f;

// Pack record flags.
pub const PACK_STRIPED: u32 = 1;
pub const PACK_IO_PAGE_LAYOUT: u32 = 2;

pub const MIN_BLOCK_SHIFT: u8 = 14;
pub const MAX_BLOCK_SHIFT: u8 = 20;
/// Largest stored chunk the descriptor can hold (20 bits plus one).
pub const MAX_STORED: u32 = 1 << 20;
/// Location words carry the absolute volume offset in the low 48 bits, the pack id above.
pub const OFFSET_MASK: u64 = (1 << 48) - 1;

pub const MAX_PATH_BYTES: usize = 1023;
pub const MAX_COMPONENT_BYTES: usize = 255;
pub const APP0: &str = "/app0/";

fn bad(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

fn field<const N: usize>(b: &[u8], off: usize) -> Result<[u8; N]> {
    off.checked_add(N)
        .and_then(|end| b.get(off..end))
        .map(|s| s.try_into().expect("slice has N bytes"))
        .ok_or_else(|| {
            bad(format!(
                "{N} bytes at offset {off} lie past the end ({} bytes)",
                b.len()
            ))
        })
}

pub fn u16_at(b: &[u8], off: usize) -> Result<u16> {
    field(b, off).map(u16::from_le_bytes)
}
pub fn u32_at(b: &[u8], off: usize) -> Result<u32> {
    field(b, off).map(u32::from_le_bytes)
}
pub fn u64_at(b: &[u8], off: usize) -> Result<u64> {
    field(b, off).map(u64::from_le_bytes)
}
pub fn i64_at(b: &[u8], off: usize) -> Result<i64> {
    field(b, off).map(i64::from_le_bytes)
}

fn put(b: &mut [u8], off: usize, v: &[u8]) -> Result<()> {
    let len = b.len();
    off.checked_add(v.len())
        .and_then(|end| b.get_mut(off..end))
        .map(|s| s.copy_from_slice(v))
        .ok_or_else(|| {
            bad(format!(
                "{} bytes at offset {off} lie past the end ({len} bytes)",
                v.len()
            ))
        })
}

pub fn put_u32(b: &mut [u8], off: usize, v: u32) -> Result<()> {
    put(b, off, &v.to_le_bytes())
}
pub fn put_u64(b: &mut [u8], off: usize, v: u64) -> Result<()> {
    put(b, off, &v.to_le_bytes())
}
pub fn put_i64(b: &mut [u8], off: usize, v: i64) -> Result<()> {
    put(b, off, &v.to_le_bytes())
}
pub fn put_bytes(b: &mut [u8], off: usize, v: &[u8]) -> Result<()> {
    put(b, off, v)
}

/// The 8 magic bytes at the start of `b` must be `magic`.
pub fn check_magic(b: &[u8], magic: &[u8; 8], what: &str) -> Result<()> {
    match b.get(..8) {
        Some(m) if m == magic => Ok(()),
        _ => Err(bad(format!("not {what}: bad magic"))),
    }
}

const FNV_PRIME: u64 = 1099511628211;
/// AMPRPAK4 path hash: standard FNV-1a 64.
const PAK_BASIS: u64 = 14695981039346656037;
/// AMPRIDX3 slot hash: the same basis with its last digit dropped.
const IDX_BASIS: u64 = 1469598103934665603;

fn fold(b: u8) -> u8 {
    match b {
        b'\\' => b'/',
        b'A'..=b'Z' => b + 32,
        _ => b,
    }
}

fn fnv(basis: u64, bytes: impl Iterator<Item = u8>) -> u64 {
    let h = bytes.fold(basis, |h, b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME));
    if h == 0 { 1 } else { h }
}

/// Manifest path hash of a complete `/app0/...` path (backslash to slash, ASCII A-Z lowercased).
pub fn pak_hash(path: &[u8]) -> u64 {
    fnv(PAK_BASIS, path.iter().copied().map(fold))
}

/// Path index slot hash of a complete `/app0/...` path, folded as `pak_hash` does.
pub fn idx_hash(path: &[u8]) -> u64 {
    fnv(IDX_BASIS, path.iter().copied().map(fold))
}

/// CRC-32/ISO-HDLC of `bytes`.
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

/// CRC of the first `len` bytes of `header` with the four bytes at `zero_at` read as zero.
fn crc_zeroed(header: &[u8], len: usize, zero_at: usize) -> Result<u32> {
    let h = header
        .get(..len)
        .ok_or_else(|| bad(format!("header is {} bytes, need {len}", header.len())))?;
    let mut c = crc32fast::Hasher::new();
    c.update(&h[..zero_at]);
    c.update(&[0; 4]);
    c.update(&h[zero_at + 4..]);
    Ok(c.finalize())
}

/// AMPRPAK4 `payloadCrc32` (offset 112): `[128, EOF)` of the whole file.
pub fn pak_payload_crc(file: &[u8]) -> Result<u32> {
    file.get(PAK_HEADER..)
        .map(crc32)
        .ok_or_else(|| bad("manifest is shorter than its header"))
}
/// AMPRPAK4 `headerCrc32` (offset 116): `[0,128)` with `[116,120)` zeroed; the payload CRC stays.
pub fn pak_header_crc(header: &[u8]) -> Result<u32> {
    crc_zeroed(header, PAK_HEADER, PAK_HEADER_CRC_AT)
}
/// AMPRDAT3 `headerCrc32` (offset 56): `[0,64)` with `[56,60)` zeroed.
pub fn dat_header_crc(header: &[u8]) -> Result<u32> {
    crc_zeroed(header, DAT_HEADER, DAT_HEADER_CRC_AT)
}
/// AMPRCFG1 `crc32` (offset 56): `[0,64)` with `[56,60)` zeroed.
pub fn cfg_crc(profile: &[u8]) -> Result<u32> {
    crc_zeroed(profile, CFG_SIZE, CFG_CRC_AT)
}
/// AMPRCRC1 `payloadCrc32` (offset 40): `[48, EOF)`.
pub fn crc1_payload_crc(file: &[u8]) -> Result<u32> {
    file.get(CRC_HEADER..)
        .map(crc32)
        .ok_or_else(|| bad("CRC sidecar is shorter than its header"))
}
/// AMPRCRC1 `headerCrc32` (offset 44): `[0,48)` with `[44,48)` zeroed; the payload CRC stays.
pub fn crc1_header_crc(header: &[u8]) -> Result<u32> {
    crc_zeroed(header, CRC_HEADER, CRC_HEADER_CRC_AT)
}

fn check_path_bytes(rel: &str) -> Result<()> {
    if rel.is_empty() {
        return Err(bad("empty path"));
    }
    if rel.contains(['\\', '\0']) {
        return Err(bad(format!("path {rel:?} has a backslash or NUL")));
    }
    for c in rel.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return Err(bad(format!(
                "path {rel:?} has an empty, `.` or `..` component"
            )));
        }
        if c.len() > MAX_COMPONENT_BYTES {
            return Err(bad(format!(
                "path {rel:?} has a component over {MAX_COMPONENT_BYTES} bytes"
            )));
        }
    }
    Ok(())
}

/// Checks a complete logical path (`/app0/<relative>`) and returns the relative part, spelling kept.
pub fn logical_to_rel(path: &str) -> Result<&str> {
    let b = path.as_bytes();
    if b.len() < APP0.len()
        || b[0] != b'/'
        || !b[1..5].eq_ignore_ascii_case(b"app0")
        || b[5] != b'/'
    {
        return Err(bad(format!("path {path:?} does not start with /app0/")));
    }
    // The first six bytes are ASCII, so this is a char boundary.
    let rel = &path[APP0.len()..];
    if path.len() > MAX_PATH_BYTES {
        return Err(bad(format!("path {path:?} is over {MAX_PATH_BYTES} bytes")));
    }
    check_path_bytes(rel)?;
    Ok(rel)
}

/// `/app0/` + a relative tree path, checked by the same rules.
pub fn rel_to_logical(rel: &str) -> Result<String> {
    let full = format!("{APP0}{rel}");
    logical_to_rel(&full)?;
    Ok(full)
}

/// A pack (volume) name: relative to `/app0`, safe components, at most 1017 bytes.
pub fn check_pack_name(name: &str) -> Result<()> {
    if name.len() > MAX_PATH_BYTES - 6 {
        return Err(bad(format!(
            "pack name {name:?} is over {} bytes",
            MAX_PATH_BYTES - 6
        )));
    }
    check_path_bytes(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Raw = 0,
    Lz4 = 1,
}

/// A decoded chunk descriptor word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// 1..=1 MiB.
    pub stored: u32,
    pub codec: Codec,
    pub flags: u8,
}

/// `(stored - 1) | codec << 20 | flags << 22`.
pub fn pack_descriptor(stored: u32, codec: Codec, flags: u8) -> Result<u32> {
    if !(1..=MAX_STORED).contains(&stored) {
        return Err(bad(format!(
            "chunk stored size {stored} is outside 1..={MAX_STORED}"
        )));
    }
    if flags & !CHUNK_FLAGS_KNOWN != 0 {
        return Err(bad(format!("chunk flags {flags:#x} are not known")));
    }
    Ok((stored - 1) | (codec as u32) << 20 | u32::from(flags) << 22)
}

pub fn unpack_descriptor(d: u32) -> Result<Descriptor> {
    if d >> 30 != 0 {
        return Err(bad(format!(
            "chunk descriptor {d:#010x} sets reserved bits"
        )));
    }
    let codec = match (d >> 20) & 3 {
        0 => Codec::Raw,
        1 => Codec::Lz4,
        c => return Err(bad(format!("chunk codec {c} is not RAW or LZ4"))),
    };
    let flags = (d >> 22) as u8;
    if flags & !CHUNK_FLAGS_KNOWN != 0 {
        return Err(bad(format!("chunk flags {flags:#x} are not known")));
    }
    Ok(Descriptor {
        stored: (d & 0xf_ffff) + 1,
        codec,
        flags,
    })
}

/// `offset | pack_id << 48`.
pub fn pack_location(pack_id: u16, offset: u64) -> Result<u64> {
    if offset > OFFSET_MASK {
        return Err(bad(format!("volume offset {offset} does not fit 48 bits")));
    }
    Ok(offset | u64::from(pack_id) << 48)
}

/// `(pack_id, offset)` of a location word.
pub fn unpack_location(loc: u64) -> (u16, u64) {
    ((loc >> 48) as u16, loc & OFFSET_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_hash_vectors() {
        for (p, h) in [
            ("/app0/toc", 0x348925d202b64400u64),
            ("/app0/d/materialgraph", 0xedd2eedc1022d002),
            ("/app0/commandline.txt", 0x7fcfbabf58b3482b),
            ("/app0/sce_sys/trophy2/trophy00.ucp", 0xf487434ede06e42f),
            ("", 1469598103934665603),
            ("a", 0x44bd8ad473cd9906),
        ] {
            assert_eq!(idx_hash(p.as_bytes()), h, "{p}");
        }
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + 3) as u8).collect()
    }

    #[test]
    fn header_crcs_zero_their_own_field_only() {
        let mut h = sample(160);
        type Case = (fn(&[u8]) -> Result<u32>, usize, usize);
        let cases: [Case; 4] = [
            (pak_header_crc, 116, 128),
            (dat_header_crc, 56, 64),
            (cfg_crc, 56, 64),
            (crc1_header_crc, 44, 48),
        ];
        for (f, at, len) in cases {
            let mut z = h[..len].to_vec();
            z[at..at + 4].fill(0);
            let want = crc32(&z);
            assert_eq!(f(&h).unwrap(), want);
            // The field's current value does not matter; any other byte inside does.
            h[at] ^= 0xff;
            assert_eq!(f(&h).unwrap(), want);
            h[at] ^= 0xff;
            h[0] ^= 1;
            assert_ne!(f(&h).unwrap(), want);
            h[0] ^= 1;
            // Bytes past the range do not matter.
            h[len] ^= 1;
            assert_eq!(f(&h).unwrap(), want);
            h[len] ^= 1;
            assert!(f(&h[..len - 1]).is_err());
        }
    }
}
