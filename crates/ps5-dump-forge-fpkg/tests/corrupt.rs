//! Damaged input is an error, never a panic.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{Rng, build, imagedigs_at, open_bytes};
use ps5upload_fpkg::crypto::sha3;
use ps5upload_fpkg::kraken_image;
use ps5upload_fpkg::source::SourceTree;

const BLOCK: usize = 0x10000;

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
