//! Packages built by the vendored builder read back, byte for byte, as their effective manifest.

mod common;

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use common::{BIG, BIG_LEN, Rng, assert_matches, build, error_of, imagedigs_at, open_bytes};
use ps5_dump_forge_fpkg::{FpkgSource, NATIVE_UNDECODABLE, UNSUPPORTED_CODEC, WRONG_PASSCODE};
use ps5upload_fpkg::ImageMode;
use ps5upload_fpkg::crypto::sha3;
use ps5upload_fpkg::inner::MetaCodec;
use ps5upload_fpkg::source::SourceTree;

#[test]
fn a_kraken_package_reads_back_as_its_manifest() {
    let built = build("kraken", 0, |_| {});
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert_matches(&mut source, &built);
    // The artwork the builder moved into the container is served from there.
    assert_eq!(
        built.container_only,
        [
            "sce_sys/icon0.png",
            "sce_sys/pic0.png",
            "sce_sys/pic1.png",
            "sce_sys/pic2.png",
            "sce_sys/snd0.at9"
        ]
    );
    assert!(built.empty_dirs.contains(&"data/empty".to_string()));
    let details = source.details().join("\n");
    assert!(details.contains(common::CONTENT_ID), "{details}");
    assert!(details.contains("layout: Kraken"), "{details}");
    assert!(details.contains("plaintext"), "{details}");
    assert!(source.describe().contains(common::CONTENT_ID));
}

#[test]
fn our_packages_take_param_json_and_np_files_from_the_image() {
    let built = build("np", 0, |_| {});
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert_matches(&mut source, &built);
    let details = source.details().join("\n");
    assert!(details.contains(" 0 conflicts"), "{details}");
    assert!(
        !built
            .container_only
            .contains(&"sce_sys/param.json".to_string())
    );
    for path in [
        "sce_sys/param.json",
        "sce_sys/nptitle.dat",
        "sce_sys/uds/npbind.dat",
        "sce_sys/trophy2/npbind.dat",
    ] {
        let n = source.files().iter().filter(|f| f.path == path).count();
        assert_eq!(n, 1, "{path}");
    }
}

#[test]
fn unaligned_ranges_of_a_multi_block_file_match() {
    let built = build("ranges", 0, |_| {});
    let want = built.bytes(BIG);
    assert_eq!(want.len(), BIG_LEN);
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    let mut rng = Rng::new(99);
    for _ in 0..300 {
        let offset = rng.below(BIG_LEN as u64 + 100) as usize;
        let len = rng.below(700_000) as usize;
        let got = source.read_range(BIG, offset as u64, len).unwrap();
        let start = offset.min(BIG_LEN);
        let end = offset.saturating_add(len).min(BIG_LEN);
        assert!(got == want[start..end], "{offset} + {len}");
    }
    // Sequential reads of more than two blocks, each starting in the block the last one ended in.
    let mut got = Vec::new();
    while got.len() < BIG_LEN {
        got.extend(source.read_range(BIG, got.len() as u64, 600_001).unwrap());
    }
    assert!(got == want, "sequential");
    // Block boundaries of the 256 KiB Kraken blocks, from either side.
    for k in 1..5u64 {
        let at = k * 256 * 1024;
        let got = source.read_range(BIG, at - 3, 7).unwrap();
        assert_eq!(got, want[at as usize - 3..at as usize + 4]);
    }
    assert!(source.read_range(BIG, u64::MAX, 10).unwrap().is_empty());
    assert!(source.read_range("no/such/file", 0, 10).is_err());
}

#[test]
fn a_flat_package_reads_back_as_its_manifest() {
    let built = build("flat", 0, |r| r.kraken = false);
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert!(source.details().join("\n").contains("layout: flat"));
    assert_matches(&mut source, &built);
}

#[test]
fn a_zlib_metadata_region_is_an_unsupported_codec() {
    let built = build("zlib", 0, |r| {
        r.kraken = false;
        r.metadata_codec = MetaCodec::Zlib;
    });
    let e = error_of(FpkgSource::open(&built.path, None));
    assert!(e.starts_with(UNSUPPORTED_CODEC), "{e}");
}

#[test]
fn a_native_package_needs_its_passcode() {
    let built = build("native", 0, |r| {
        r.image_mode = ImageMode::Native;
        r.seed = Some([0x5A; 16]);
    });
    let mut source = FpkgSource::open(&built.path, None).unwrap();
    assert!(source.details().join("\n").contains("native AES-XTS"));
    assert_matches(&mut source, &built);
    let e = error_of(FpkgSource::open(
        &built.path,
        Some("11111111111111111111111111111111"),
    ));
    assert!(e.starts_with(WRONG_PASSCODE), "{e}");
}

#[test]
fn a_native_package_built_with_its_own_passcode() {
    let passcode = "0123456789ABCDEF0123456789ABCDEF";
    let built = build("native-own", 0, |r| {
        r.image_mode = ImageMode::Native;
        r.passcode = passcode.to_string();
    });
    let e = error_of(FpkgSource::open(&built.path, None));
    assert!(e.starts_with(WRONG_PASSCODE), "{e}");
    let mut source = FpkgSource::open(&built.path, Some(passcode)).unwrap();
    assert_matches(&mut source, &built);
}

/// The container's key check passes but the blocks do not decrypt: the superblock's seed is
/// changed and every digest over it rewritten, so only the decryption can notice.
#[test]
fn a_native_image_that_does_not_decrypt_is_named_so() {
    let built = build("native-seed", 0, |r| r.image_mode = ImageMode::Native);
    let mut pkg = built.package();
    let le64 = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let sb_at = le64(&pkg, 0x20) as usize;
    let sb_index = (sb_at - 0x10000) / 0x10000;
    let mut sb = pkg[sb_at..sb_at + 0x10000].to_vec();
    sb[0x370] ^= 0xFF;
    let icv = ps5upload_fpkg::outer_write::superblock_icv(&sb);
    sb[0x380..0x3A0].copy_from_slice(&icv);
    let digest = sha3(&sb);
    pkg[sb_at..sb_at + 0x10000].copy_from_slice(&sb);
    pkg[0x30..0x50].copy_from_slice(&digest);
    let digests_at = imagedigs_at(&pkg);
    let mut reversed = digest;
    reversed.reverse();
    pkg[digests_at + sb_index * 32..digests_at + sb_index * 32 + 32].copy_from_slice(&reversed);
    let e = error_of(open_bytes(pkg, None));
    assert!(e.starts_with(NATIVE_UNDECODABLE), "{e}");
}

/// A reader that counts what is read through it.
struct Counting {
    inner: std::io::Cursor<Vec<u8>>,
    total: Arc<AtomicU64>,
    largest: Arc<AtomicU64>,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.total.fetch_add(n as u64, Ordering::Relaxed);
        self.largest.fetch_max(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl Seek for Counting {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Opening reads the headers and metadata, not the image; a small range reads a few blocks.
#[test]
fn opening_and_a_small_read_do_not_read_the_image() {
    let built = build("memory", 6 << 20, |_| {});
    let pkg = built.package();
    let len = pkg.len() as u64;
    assert!(len > 6 << 20);
    let total = Arc::new(AtomicU64::new(0));
    let largest = Arc::new(AtomicU64::new(0));
    let reader = Counting {
        inner: std::io::Cursor::new(pkg),
        total: total.clone(),
        largest: largest.clone(),
    };
    let mut source = FpkgSource::from_reader(Box::new(reader), len, "counting", None).unwrap();
    let opened = total.load(Ordering::Relaxed);
    assert!(opened < 2 << 20, "open read {opened} of {len} bytes");
    assert!(
        largest.load(Ordering::Relaxed) <= 128 << 10,
        "open made a read of {} bytes",
        largest.load(Ordering::Relaxed)
    );
    let mid = source.read_range("data/random.bin", 3 << 20, 16).unwrap();
    assert_eq!(mid, built.bytes("data/random.bin")[3 << 20..(3 << 20) + 16]);
    let ranged = total.load(Ordering::Relaxed) - opened;
    eprintln!("open read {opened} of {len} bytes; a 16-byte range read {ranged}");
    assert!(ranged < 1 << 20, "a 16-byte read read {ranged} bytes");
    assert!(largest.load(Ordering::Relaxed) <= 128 << 10);
}
