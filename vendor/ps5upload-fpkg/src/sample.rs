//! Fast verification: which parts of a finished output to read back.
//!
//! A full read-back re-reads every byte just written, which on a multi-hundred-gigabyte game
//! costs as long as the write. The fast mode keeps every structural check (file count, sizes,
//! empty folders, paths, block tables) and reads back only a seeded sample of the data: small
//! files whole, the first and last slice of every large file, one random interior slice per
//! file, the UFS2 seam, and a random draw of about 1% of the rest (at most 1 GiB). The draw is
//! deterministic for a seed, so a run can be repeated and a test can aim past it.

use std::collections::HashSet;

/// What one sample of a large file covers.
pub const SLICE: u64 = 8 << 20;
/// Files up to this size are read back whole.
pub const WHOLE_MAX: u64 = 16 << 20;
/// The most the random draw over the rest of the data reads.
pub const BUDGET_MAX: u64 = 1 << 30;
/// Where UFS2's first indirect block begins: the layout changes there, so a slice over it is
/// always read (12 direct blocks, then one indirect block of 8192 pointers, 64 KiB each).
pub const UFS2_SEAM: u64 = (12 + 8192) * 0x10000;
/// Bumped when the sampling rules change, so a logged run says which rules it followed.
pub const POLICY: u32 = 1;

/// How a read-back check treats the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMode {
    /// Every byte.
    Full,
    /// A sample drawn from `seed`; `seam` adds the UFS2 indirect-block seam.
    Fast { seed: u64, seam: bool },
}

impl VerifyMode {
    /// The mode a request's `full_verify` flag asks for; a fast run gets a fresh seed.
    pub fn from_full(full: bool, seam: bool) -> Self {
        if full {
            Self::Full
        } else {
            Self::Fast {
                seed: fresh_seed(),
                seam,
            }
        }
    }
}

/// A seed from OS randomness, kept to 53 bits so it survives a round trip through JSON.
pub fn fresh_seed() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    u64::from_le_bytes(b) & ((1 << 53) - 1)
}

/// SplitMix64: a small generator whose sequence is fixed by its seed, on every platform.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_D049);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n` > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }
}

/// One piece of one file to read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// Index into the file list given to [`plan`].
    pub file_idx: usize,
    /// A slice of [`SLICE`] bytes of a large file, or the whole file when `None`.
    pub slice: Option<u64>,
}

impl Sample {
    /// `(offset, length)` of this sample in a file of `size` bytes.
    pub fn range(&self, size: u64) -> (u64, u64) {
        match self.slice {
            None => (0, size),
            Some(s) => (s * SLICE, SLICE.min(size - s * SLICE)),
        }
    }
}

/// The samples for `files` (`(path, size)`), sorted by path then offset. The same files, seed
/// and `seam` always give the same samples.
pub fn plan(files: &[(String, u64)], seed: u64, seam: bool) -> Vec<Sample> {
    let mut rng = SplitMix64::new(seed);
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|a, b| files[*a].0.cmp(&files[*b].0));
    let mut out: Vec<Sample> = Vec::new();
    // Every slice of a large file that nothing has chosen yet, for the global draw.
    let mut pool: Vec<(usize, u64)> = Vec::new();
    for &i in &order {
        let size = files[i].1;
        if size == 0 {
            continue;
        }
        if size <= WHOLE_MAX {
            out.push(Sample {
                file_idx: i,
                slice: None,
            });
            continue;
        }
        let slices = size.div_ceil(SLICE);
        let last = slices - 1;
        let mut chosen: HashSet<u64> = HashSet::from([0, last]);
        if seam && size > UFS2_SEAM {
            chosen.insert(UFS2_SEAM / SLICE);
        }
        if last > 1 {
            chosen.insert(1 + rng.below(last - 1));
        }
        for s in 0..slices {
            if chosen.contains(&s) {
                out.push(Sample {
                    file_idx: i,
                    slice: Some(s),
                });
            } else {
                pool.push((i, s));
            }
        }
    }
    let rest: u64 = pool.iter().map(|(i, s)| slice_len(files[*i].1, *s)).sum();
    let want = (rest / 100).min(BUDGET_MAX).div_ceil(SLICE) as usize;
    // A partial shuffle: the first `want` entries are a draw without replacement.
    for k in 0..want.min(pool.len()) {
        let j = k + rng.below((pool.len() - k) as u64) as usize;
        pool.swap(k, j);
        let (i, s) = pool[k];
        out.push(Sample {
            file_idx: i,
            slice: Some(s),
        });
    }
    out.sort_by(|a, b| {
        files[a.file_idx]
            .0
            .cmp(&files[b.file_idx].0)
            .then(a.slice.cmp(&b.slice))
    });
    out
}

fn slice_len(size: u64, slice: u64) -> u64 {
    SLICE.min(size - slice * SLICE)
}

/// Indices in `0..count` for a sampled sweep over equal units: the first, the last, and a seeded
/// draw of about 1% (at most `cap`), ascending. Every unit when there are no more than the draw
/// plus the two ends.
pub fn pick(count: u64, seed: u64, cap: u64) -> Vec<u64> {
    let draw = count.div_ceil(100).min(cap);
    if count <= draw + 2 {
        return (0..count).collect();
    }
    let mut rng = SplitMix64::new(seed);
    let mut set: HashSet<u64> = HashSet::from([0, count - 1]);
    while (set.len() as u64) < draw + 2 {
        set.insert(rng.below(count));
    }
    let mut v: Vec<u64> = set.into_iter().collect();
    v.sort_unstable();
    v
}

/// What a read-back check covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifySummary {
    pub mode: VerifyMode,
    pub checked_bytes: u64,
    pub total_bytes: u64,
    pub samples: u64,
}

/// What `mode` reads of `files` (`(path, size)`).
pub fn summary(files: &[(String, u64)], mode: VerifyMode) -> VerifySummary {
    let total_bytes: u64 = files.iter().map(|f| f.1).sum();
    match mode {
        VerifyMode::Full => VerifySummary {
            mode,
            checked_bytes: total_bytes,
            total_bytes,
            samples: files.iter().filter(|f| f.1 > 0).count() as u64,
        },
        VerifyMode::Fast { seed, seam } => {
            let samples = plan(files, seed, seam);
            VerifySummary {
                mode,
                checked_bytes: samples.iter().map(|s| s.range(files[s.file_idx].1).1).sum(),
                total_bytes,
                samples: samples.len() as u64,
            }
        }
    }
}

impl VerifySummary {
    /// The line a stage detail and the log carry.
    pub fn describe(&self) -> String {
        match self.mode {
            VerifyMode::Full => "hash-checked".into(),
            VerifyMode::Fast { seed, .. } => format!(
                "fast-checked {} of {} in {} samples (seed {seed}, policy {POLICY})",
                human(self.checked_bytes),
                human(self.total_bytes),
                self.samples
            ),
        }
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_takes_the_ends_and_a_bounded_draw() {
        assert_eq!(pick(3, 1, 128), vec![0, 1, 2]);
        let five = pick(5, 1, 128);
        assert_eq!((five.len(), five[0], five[2]), (3, 0, 4));
        let v = pick(10_000, 1, 128);
        assert_eq!(v.len(), 100 + 2);
        assert_eq!((v[0], *v.last().unwrap()), (0, 9_999));
        assert!(v.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(pick(10_000_000, 1, 128).len(), 130);
        assert_eq!(pick(10_000, 1, 128), pick(10_000, 1, 128));
    }
}
