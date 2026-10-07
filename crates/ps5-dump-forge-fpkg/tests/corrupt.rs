//! Damaged and foreign input is an error, never a panic, and each kind of wrong file is named.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{Rng, build, error_of, imagedigs_at, open_bytes};
use ps5_dump_forge_fpkg::{NOT_PS5, RETAIL};
use ps5upload_fpkg::crypto::sha3;
use ps5upload_fpkg::kraken_image;
use ps5upload_fpkg::source::SourceTree;

const BLOCK: usize = 0x10000;

#[test]
fn files_that_are_not_ps5_packages_are_named_so() {
    let ps4: Vec<u8> = b"\x7FCNT".iter().copied().chain([0u8; 0x2000]).collect();
    for bytes in [Vec::new(), vec![0u8; 10], vec![0u8; 1 << 20], ps4.clone()] {
        let e = error_of(open_bytes(bytes, None));
        assert!(e.starts_with(NOT_PS5), "{e}");
    }
    let e = error_of(open_bytes(ps4, None));
    assert!(e.contains("PS4"), "{e}");
}

#[test]
fn a_retail_header_is_named_so() {
    let built = build("retail", 0, |_| {});
    let mut pkg = built.package();
    pkg[5] = 0x80;
    let e = error_of(open_bytes(pkg, None));
    assert!(e.starts_with(RETAIL), "{e}");
}

/// Open the bytes and read every file; any outcome but a panic is fine. True when all of it
/// read without an error.
fn exercise(bytes: Vec<u8>) -> bool {
    let Ok(mut source) = open_bytes(bytes, None) else {
        return false;
    };
    let paths: Vec<String> = source.files().iter().map(|f| f.path.clone()).collect();
    let mut ok = true;
    for path in &paths {
        ok &= source.read(path).is_ok();
        ok &= source.read_range(path, 1, 70_000).is_ok();
    }
    let _ = source.details();
    ok
}

/// A built plaintext package and where its parts are.
struct Target {
    bytes: Vec<u8>,
    cnt_offset: usize,
    inner_blocks: usize,
    naps_len: usize,
    meta_base: usize,
    digests_at: usize,
    /// `(stored_at, stored_len)` of every block with an LZ half.
    lz: Vec<(usize, usize)>,
}

impl Target {
    fn new(bytes: Vec<u8>) -> Self {
        let le = |at: usize, n: usize| {
            let mut v = [0u8; 8];
            v[..n].copy_from_slice(&bytes[at..at + n]);
            u64::from_le_bytes(v) as usize
        };
        let inner_blocks = le(0x90, 4);
        let naps_len = le(0xA8, 8);
        let naps_at = BLOCK + inner_blocks * BLOCK;
        let lz = kraken_image::describe(&bytes[naps_at..naps_at + naps_len])
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.even_lz || b.odd_lz)
                    .map(|b| (b.stored_at as usize, b.stored_len as usize))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            cnt_offset: le(0x58, 8),
            inner_blocks,
            naps_len,
            meta_base: le(0x50, 4) * BLOCK,
            digests_at: imagedigs_at(&bytes),
            lz,
            bytes,
        }
    }

    /// Set image byte `at` (an offset into `pfs_image.dat`, or past it into the descriptor's
    /// blocks) and rewrite that block's `imagedigs` entry so the change gets past the digest.
    fn patch(&self, bytes: &mut [u8], at: usize, value: u8) {
        let block = at / BLOCK;
        bytes[BLOCK + at] = value;
        let base = BLOCK + block * BLOCK;
        let mut digest = sha3(&bytes[base..base + BLOCK]);
        digest.reverse();
        let d = self.digests_at + block * 32;
        bytes[d..d + 32].copy_from_slice(&digest);
    }
}

/// Mutate `t` `rounds` times, `pick` choosing the damage; returns (rounds that read cleanly,
/// rounds that failed). A panic in any round fails the test.
fn fuzz(
    t: &Target,
    rounds: u64,
    seed: u64,
    pick: impl Fn(&Target, &mut Rng, &mut Vec<u8>),
) -> (u32, u32) {
    let mut rng = Rng::new(seed);
    let mut panics = Vec::new();
    let (mut clean, mut failed) = (0, 0);
    for round in 0..rounds {
        let mut bytes = t.bytes.clone();
        pick(t, &mut rng, &mut bytes);
        match catch_unwind(AssertUnwindSafe(|| exercise(bytes))) {
            Ok(true) => clean += 1,
            Ok(false) => failed += 1,
            Err(_) => panics.push(round),
        }
    }
    assert!(
        panics.is_empty(),
        "seed {seed}: rounds that panicked: {panics:?}"
    );
    (clean, failed)
}

/// Damage nothing digests on the way in: truncations, and bytes of the header and of the
/// container's header and entry table; plus random flips anywhere.
#[test]
fn damaged_headers_and_truncations_fail_without_panicking() {
    let t = Target::new(build("fuzz-head", 0, |_| {}).package());
    assert!(exercise(t.bytes.clone()), "the pristine package reads");
    let (_, failed) = fuzz(&t, 150, 1, |t, rng, bytes| match rng.below(4) {
        0 => bytes.truncate(rng.below(bytes.len() as u64) as usize),
        1 => {
            let at = rng.below(0x100) as usize;
            bytes[at] = rng.next() as u8;
        }
        2 => {
            let at = t.cnt_offset + (rng.below(0x1400) as usize & !3);
            let value: u32 =
                [0, 1, 0x7FFF_FFFF, u32::MAX, rng.next() as u32][rng.below(5) as usize];
            bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
        _ => {
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(bytes.len() as u64) as usize;
                bytes[at] ^= 1 << rng.below(8);
            }
        }
    });
    assert!(failed > 50, "only {failed} of 150 failed");
}

/// Damage behind a rewritten digest, so it reaches the descriptor walk and the Kraken decoder:
/// bytes of the layout descriptor, and of the stored LZ halves.
#[test]
fn damaged_descriptors_and_kraken_blocks_fail_without_panicking() {
    let t = Target::new(build("fuzz-kraken", 0, |_| {}).package());
    assert!(!t.lz.is_empty(), "the package has compressed blocks");
    let descriptor = fuzz(&t, 120, 2, |t, rng, bytes| {
        for _ in 0..1 + rng.below(3) {
            let at = t.inner_blocks * BLOCK + rng.below(t.naps_len as u64) as usize;
            t.patch(bytes, at, rng.next() as u8);
        }
    });
    let blocks = fuzz(&t, 150, 3, |t, rng, bytes| {
        let (stored_at, stored_len) = t.lz[rng.below(t.lz.len() as u64) as usize];
        for _ in 0..1 + rng.below(4) {
            let at = stored_at + rng.below(stored_len as u64) as usize;
            t.patch(bytes, at, rng.next() as u8);
        }
    });
    assert!(
        descriptor.1 > 0 && blocks.1 > 0,
        "{descriptor:?} {blocks:?}"
    );
}

/// Damage to a flat image's metadata region, which it stores verbatim: the superblock, the
/// inode table and the dirent streams, so it reaches the tree walk.
#[test]
fn damaged_inner_metadata_fails_without_panicking() {
    let t = Target::new(build("fuzz-flat", 0, |r| r.kraken = false).package());
    assert!(exercise(t.bytes.clone()), "the pristine package reads");
    let meta_blocks = t.inner_blocks - t.meta_base / BLOCK;
    let (clean, failed) = fuzz(&t, 240, 4, |t, rng, bytes| {
        if rng.below(2) == 0 {
            // A field the walk reads, of an inode the tree uses (the table is the block after
            // the superblock): mode, size, logical offset or afid, set to an extreme.
            let inode = rng.below(80) as usize;
            let field = [0usize, 1, 8, 12, 0x60, 0x64, 0x68][rng.below(7) as usize];
            let at = t.meta_base + BLOCK + inode * 0xA8 + field;
            let value = [0u8, 0xFF, 0x7F, 0x40, rng.next() as u8][rng.below(5) as usize];
            t.patch(bytes, at, value);
            return;
        }
        let block = t.meta_base / BLOCK + rng.below(meta_blocks as u64) as usize;
        for _ in 0..1 + rng.below(4) {
            // The structures start each block; most of a block is zero padding.
            let at = block * BLOCK + rng.below(0x1800) as usize;
            let value = match rng.below(3) {
                0 => 0,
                1 => 0xFF,
                _ => rng.next() as u8,
            };
            t.patch(bytes, at, value);
        }
    });
    eprintln!("inner metadata: {clean} clean, {failed} failed");
    assert!(failed > 20 && clean > 0, "{clean} clean, {failed} failed");
}
