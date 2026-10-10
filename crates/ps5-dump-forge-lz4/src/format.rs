//! Byte-level pieces of the AMPR pack formats: magics, sizes, flag bits, checked little-endian
//! access, the two path hashes and the journal's payload hash, the CRC ranges, logical path
//! rules and the chunk `location`/`descriptor` words. Nothing here maps a struct onto bytes.

use ps5upload_fpkg::{Error, Result};

pub const PAK_MAGIC: &[u8; 8] = b"AMPRPAK4";
pub const DAT_MAGIC: &[u8; 8] = b"AMPRDAT3";
pub const CFG_MAGIC: &[u8; 8] = b"AMPRCFG1";
pub const CRC_MAGIC: &[u8; 8] = b"AMPRCRC1";
pub const IDX_MAGIC: &[u8; 8] = b"AMPRIDX3";
pub const CMD_MAGIC: &[u8; 8] = b"AMPRCMD1";

pub const PAK_VERSION: u32 = 4;
pub const DAT_VERSION: u32 = 3;
pub const CFG_VERSION: u32 = 1;
pub const CRC_VERSION: u32 = 1;
pub const IDX_VERSION: u32 = 3;
pub const CMD_VERSION: u16 = 1;
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
pub const CMD_HEADER: usize = 96;

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

pub fn put_u16(b: &mut [u8], off: usize, v: u16) -> Result<()> {
    put(b, off, &v.to_le_bytes())
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
/// AMPRIDX3 slot hash and AMPRCMD1 payload hash: the same basis with its last digit dropped.
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

/// Journal payload hash: the index basis over the raw bytes, no folding.
pub fn cmd_hash(payload: &[u8]) -> u64 {
    fnv(IDX_BASIS, payload.iter().copied())
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

    #[test]
    fn pak_hash_is_fnv1a_64() {
        assert_eq!(pak_hash(b""), 0xcbf29ce484222325);
        assert_eq!(pak_hash(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(pak_hash(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn hashes_fold_paths_but_cmd_hash_does_not() {
        assert_eq!(pak_hash(b"/APP0\\Dir\\X.Bin"), pak_hash(b"/app0/dir/x.bin"));
        assert_eq!(idx_hash(b"/APP0\\Dir\\X.Bin"), idx_hash(b"/app0/dir/x.bin"));
        // Only ASCII A-Z fold.
        assert_ne!(pak_hash("É".as_bytes()), pak_hash("é".as_bytes()));
        assert_ne!(cmd_hash(b"A"), cmd_hash(b"a"));
        assert_ne!(cmd_hash(b"\\"), cmd_hash(b"/"));
        assert_eq!(cmd_hash(b""), 1469598103934665603);
        assert_eq!(cmd_hash(b"a"), idx_hash(b"a"));
    }

    #[test]
    fn crc_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
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

    #[test]
    fn pak_header_crc_keeps_payload_crc_and_payload_ranges() {
        let mut f = sample(300);
        assert_eq!(pak_payload_crc(&f).unwrap(), crc32(&f[128..]));
        assert_eq!(crc1_payload_crc(&f).unwrap(), crc32(&f[48..]));
        let before = pak_header_crc(&f).unwrap();
        f[PAK_PAYLOAD_CRC_AT] ^= 1;
        assert_ne!(pak_header_crc(&f).unwrap(), before);
        assert!(pak_payload_crc(&f[..127]).is_err());
        assert!(crc1_payload_crc(&f[..47]).is_err());
    }

    #[test]
    fn le_helpers_check_bounds() {
        let mut b = [0u8; 16];
        put_u16(&mut b, 0, 0x0102).unwrap();
        put_u32(&mut b, 2, 0x03040506).unwrap();
        put_u64(&mut b, 8, 0x1122334455667788).unwrap();
        assert_eq!(&b[..6], &[2, 1, 6, 5, 4, 3]);
        assert_eq!(u16_at(&b, 0).unwrap(), 0x0102);
        assert_eq!(u32_at(&b, 2).unwrap(), 0x03040506);
        assert_eq!(u64_at(&b, 8).unwrap(), 0x1122334455667788);
        put_i64(&mut b, 8, -2).unwrap();
        assert_eq!(i64_at(&b, 8).unwrap(), -2);
        assert!(u64_at(&b, 9).is_err());
        assert!(u32_at(&b, usize::MAX).is_err());
        assert!(put_u32(&mut b, 14, 1).is_err());
        assert!(put_bytes(&mut b, usize::MAX, &[1, 2]).is_err());
        assert!(check_magic(b"AMPRPAK4x", PAK_MAGIC, "a manifest").is_ok());
        assert!(check_magic(b"AMPRPAK", PAK_MAGIC, "a manifest").is_err());
        assert!(check_magic(b"AMPRPAK3", PAK_MAGIC, "a manifest").is_err());
    }

    #[test]
    fn logical_paths() {
        assert_eq!(logical_to_rel("/app0/a/B.bin").unwrap(), "a/B.bin");
        assert_eq!(logical_to_rel("/APP0/x").unwrap(), "x");
        assert_eq!(
            rel_to_logical("sce_sys/param.json").unwrap(),
            "/app0/sce_sys/param.json"
        );
        for bad in [
            "",
            "/app0",
            "/app0/",
            "app0/x",
            "/app1/x",
            "/app0x/x",
            "/app0//x",
            "/app0/x/",
            "/app0/./x",
            "/app0/x/..",
            "/app0/a\\b",
            "/app0/a\0b",
            "//app0/x",
            "/é/x",
        ] {
            assert!(logical_to_rel(bad).is_err(), "{bad:?}");
        }
        // Multi-byte chars across the prefix are an error, never a slice panic.
        for bad in [
            "/appé/foo",
            "éabcd/x",
            "/app0é",
            "/é",
            "é",
            "/ap\u{1F600}/x",
        ] {
            assert!(logical_to_rel(bad).is_err(), "{bad:?}");
        }
        assert!(rel_to_logical("").is_err());
        assert!(rel_to_logical("a//b").is_err());
        assert!(rel_to_logical("/a").is_err());
        let comp = "a".repeat(255);
        assert!(rel_to_logical(&comp).is_ok());
        assert!(rel_to_logical(&"a".repeat(256)).is_err());
        // 1023 bytes in all is the limit.
        let long = format!(
            "{}/{}/{}/{}",
            comp,
            comp,
            comp,
            "b".repeat(1023 - 6 - 3 * 256)
        );
        assert_eq!(long.len() + 6, 1023);
        assert!(rel_to_logical(&long).is_ok());
        assert!(rel_to_logical(&format!("{long}b")).is_err());
    }

    #[test]
    fn pack_names() {
        assert!(check_pack_name("ampr_assets-000.pak").is_ok());
        assert!(check_pack_name("sub/dir.pak").is_ok());
        for bad in ["", "/abs.pak", "a/../b", "a\\b", "a//b", "a/"] {
            assert!(check_pack_name(bad).is_err(), "{bad:?}");
        }
        assert!(
            check_pack_name(&format!(
                "{}/{}/{}/{}",
                "a".repeat(255),
                "a".repeat(255),
                "a".repeat(255),
                "a".repeat(249)
            ))
            .is_ok()
        );
        assert!(
            check_pack_name(&format!(
                "{}/{}/{}/{}",
                "a".repeat(255),
                "a".repeat(255),
                "a".repeat(255),
                "a".repeat(250)
            ))
            .is_err()
        );
    }

    #[test]
    fn descriptors_round_trip() {
        for stored in [1, 2, 4096, 65536, MAX_STORED] {
            for codec in [Codec::Raw, Codec::Lz4] {
                for flags in 0..=CHUNK_FLAGS_KNOWN {
                    let d = pack_descriptor(stored, codec, flags).unwrap();
                    assert_eq!(d >> 30, 0);
                    assert_eq!(
                        unpack_descriptor(d).unwrap(),
                        Descriptor {
                            stored,
                            codec,
                            flags
                        }
                    );
                }
            }
        }
        assert_eq!(pack_descriptor(1, Codec::Raw, 0).unwrap(), 0);
        assert_eq!(
            pack_descriptor(65536, Codec::Lz4, CHUNK_SHARED).unwrap(),
            0xffff | 1 << 20 | 1 << 22
        );
    }

    #[test]
    fn descriptors_reject_invalid() {
        assert!(pack_descriptor(0, Codec::Raw, 0).is_err());
        assert!(pack_descriptor(MAX_STORED + 1, Codec::Raw, 0).is_err());
        assert!(pack_descriptor(1, Codec::Raw, 0x10).is_err());
        assert!(unpack_descriptor(2 << 20).is_err());
        assert!(unpack_descriptor(3 << 20).is_err());
        assert!(unpack_descriptor(1 << 30).is_err());
        assert!(unpack_descriptor(1 << 31).is_err());
        assert!(unpack_descriptor(0x10 << 22).is_err());
    }

    #[test]
    fn locations() {
        let l = pack_location(7, 0x1234_5678_9abc).unwrap();
        assert_eq!(unpack_location(l), (7, 0x1234_5678_9abc));
        assert_eq!(
            unpack_location(pack_location(u16::MAX, OFFSET_MASK).unwrap()),
            (u16::MAX, OFFSET_MASK)
        );
        assert!(pack_location(0, OFFSET_MASK + 1).is_err());
    }
}
