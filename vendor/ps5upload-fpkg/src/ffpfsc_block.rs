//! One 64 KiB block of a `.ffpfsc`'s PFSC container, as it is stored.
//!
//! The console's kernel inflates each block on its own as the game reads, so a block is either a
//! complete zlib stream shorter than 64 KiB or the 64 KiB raw; a reader tells them apart by the
//! stored length alone. Within that, size can only improve through the encoder and the rule for
//! keeping a block raw, and time through not deflating what cannot shrink:
//!
//! - a block of zeros is never deflated: its stored form is worked out once per build;
//! - a block whose byte histogram says no deflate could reach the size it must beat (already
//!   compressed media, mostly) is stored raw without a full deflate ([`incompressible`]);
//! - the deflate writes into a buffer the size of what the block must beat;
//! - [`Level::Smallest`] tries a second, stronger pass (libdeflate level 12, near-optimal
//!   parsing) on every block level 9 shrank, and keeps whichever is shorter;
//! - every stream is walked for the one header construct the console's decoder misreads
//!   ([`crate::ffpfsc_compat`]); libdeflate writes it now and then (3 blocks in 273,001 at level
//!   12 on a real game), and such a stream is replaced by the next candidate, zlib-rs last;
//! - every block kept compressed is inflated again by a second implementation (zlib-rs) and
//!   compared with its source before it is handed back, so a container never carries a block
//!   that does not decode to its bytes.
//!
//! The encoder is libdeflate at every level: measured per 64 KiB block on a real game image
//! (007 First Light, a 1-in-128 sample), its level 1 is 13% smaller than zlib's level 1 and
//! faster, and its level 6 is 1.7% smaller than zlib's level 6 at 1.5 times the speed. Its
//! output is plain zlib, which the console's kernel inflates like any other.

use std::io;

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use crate::ffpfsc_compat::console_safe;

/// The PFSC block size.
pub const BLOCK: usize = 0x10000;

/// How hard a `.ffpfsc` is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// Level 1.
    Fast,
    /// Level 6.
    #[default]
    Balanced,
    /// Level 9, then level 12 where 9 shrank the block; every saved byte kept.
    Smallest,
}

impl Level {
    /// `fast`, `balanced` (the default, also when absent) or `smallest`.
    pub fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("balanced") => Ok(Self::Balanced),
            Some("fast") => Ok(Self::Fast),
            Some("smallest") => Ok(Self::Smallest),
            Some(other) => Err(format!(
                "unknown compression level {other:?}: use fast, balanced or smallest"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Smallest => "smallest",
        }
    }

    /// The block options this level stands for.
    pub fn options(self) -> BlockOptions {
        match self {
            Self::Fast => BlockOptions {
                level: 1,
                min_gain: 1,
                strong: false,
            },
            Self::Balanced => BlockOptions {
                level: 6,
                min_gain: 1,
                strong: false,
            },
            Self::Smallest => BlockOptions {
                level: 9,
                min_gain: 0,
                strong: true,
            },
        }
    }
}

/// How each block is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockOptions {
    /// Deflate level, 1–12 (libdeflate's scale; 1–9 mean what they mean in zlib).
    pub level: u32,
    /// Percent a block must shrink by to be kept compressed (at most 90). At 0 any block that
    /// comes out shorter than 64 KiB is kept: the console's inflate of a block is cheap next to
    /// reading it.
    pub min_gain: u8,
    /// Also try level 12 on blocks `level` shrank, keeping the shorter.
    pub strong: bool,
}

impl BlockOptions {
    pub fn check(&self) -> Result<(), String> {
        if !(1..=12).contains(&self.level) {
            return Err(format!(
                "the deflate level must be 1 through 12, not {}",
                self.level
            ));
        }
        if self.min_gain > 90 {
            return Err("the minimum block gain must be at most 90%".into());
        }
        Ok(())
    }

    /// What the encoders are, for a log line.
    pub fn encoder(&self) -> String {
        if self.strong && self.level < 12 {
            format!("libdeflate-{} + libdeflate-12", self.level)
        } else {
            format!("libdeflate-{}", self.level)
        }
    }

    /// The longest stored form a compressed block may have.
    fn cap(&self) -> usize {
        (BLOCK * (100 - usize::from(self.min_gain)) / 100).min(BLOCK - 1)
    }
}

/// What a block became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// All zeros: stored as [`BlockEncoder::zero_block`].
    Zero,
    /// Stored as it is (64 KiB). `prechecked`: the histogram ruled out a gain, so no deflate ran.
    Raw { prechecked: bool },
    /// A zlib stream shorter than 64 KiB that inflates back to the block. `strong`: the second
    /// pass's output was the shorter. `refused`: a libdeflate stream for this block was turned
    /// down as one the console misreads; `fallback`: so the stream is zlib-rs's.
    Zlib {
        bytes: Vec<u8>,
        strong: bool,
        refused: bool,
        fallback: bool,
    },
}

/// One worker's encoders and scratch space, reused block after block.
pub struct BlockEncoder {
    cap: usize,
    deflate: Compressor,
    strong: Option<Compressor>,
    /// The fallback for a stream the console would misread.
    zlib: Compress,
    inflate: Decompress,
    out: Vec<u8>,
    out2: Vec<u8>,
    back: Vec<u8>,
}

/// Which candidate a block's stream is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Main,
    Strong,
    Fallback,
}

/// Forge: the encoder is flate2's, not libdeflate's (no `libdeflater`: its C doesn't build for
/// the PS5 payload; Forge doesn't use this writer). Levels above 9 are taken as 9.
struct Compressor(Compress);

impl Compressor {
    /// The whole of `input` as one zlib stream in `out`, or `Err` when it doesn't fit.
    fn zlib_compress(&mut self, input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
        self.0.reset();
        match self.0.compress(input, out, FlushCompress::Finish) {
            Ok(Status::StreamEnd) => Ok(self.0.total_out() as usize),
            _ => Err(()),
        }
    }
}

fn compressor(level: u32) -> Compressor {
    Compressor(Compress::new(Compression::new(level.min(9)), true))
}

impl BlockEncoder {
    pub fn new(opts: BlockOptions) -> Self {
        Self {
            cap: opts.cap(),
            deflate: compressor(opts.level),
            strong: (opts.strong && opts.level < 12).then(|| compressor(12)),
            zlib: Compress::new(Compression::new(opts.level.min(9)), true),
            inflate: Decompress::new(true),
            out: vec![0u8; BLOCK],
            out2: vec![0u8; BLOCK],
            back: vec![0u8; BLOCK],
        }
    }

    /// The stored form of a block of zeros at these options, checked like any other.
    pub fn zero_block(&mut self) -> io::Result<Vec<u8>> {
        let zeros = vec![0u8; BLOCK];
        match self.compress(&zeros)? {
            Some(Stored::Zlib { bytes, .. }) => Ok(bytes),
            _ => Err(io::Error::other("a block of zeros did not compress")),
        }
    }

    /// `block` (exactly 64 KiB) as it is stored.
    pub fn encode(&mut self, block: &[u8]) -> io::Result<Stored> {
        if block.len() != BLOCK {
            return Err(io::Error::other(format!(
                "a PFSC block is {BLOCK} bytes, not {}",
                block.len()
            )));
        }
        if is_zero(block) {
            return Ok(Stored::Zero);
        }
        if incompressible(block, self.cap) {
            return Ok(Stored::Raw { prechecked: true });
        }
        Ok(self
            .compress(block)?
            .unwrap_or(Stored::Raw { prechecked: false }))
    }

    /// The shortest stream within the cap that the console reads right and that inflates back
    /// to `block` ([`Stored::Zlib`]); `None` when none fits.
    fn compress(&mut self, block: &[u8]) -> io::Result<Option<Stored>> {
        let cap = self.cap;
        // Out of space means longer than the cap: not worth keeping.
        let Ok(n) = self.deflate.zlib_compress(block, &mut self.out[..cap]) else {
            return Ok(None);
        };
        let mut refused = false;
        let mut pick = None;
        if console_safe(&self.out[..n]) {
            pick = Some((Pick::Main, n));
        } else {
            refused = true;
        }
        // Where the first pass paid: the strong one must beat it.
        let strong = self
            .strong
            .as_mut()
            .and_then(|c| c.zlib_compress(block, &mut self.out2[..n - 1]).ok());
        if let Some(m) = strong {
            if console_safe(&self.out2[..m]) {
                pick = Some((Pick::Strong, m));
            } else {
                refused = true;
            }
        }
        if pick.is_none() {
            self.zlib.reset();
            let status = self
                .zlib
                .compress(block, &mut self.out2[..cap], FlushCompress::Finish)
                .map_err(|e| io::Error::other(format!("zlib: {e}")))?;
            let m = self.zlib.total_out() as usize;
            if status == Status::StreamEnd && console_safe(&self.out2[..m]) {
                pick = Some((Pick::Fallback, m));
            }
        }
        let Some((from, len)) = pick else {
            return Ok(None);
        };
        let bytes = match from {
            Pick::Main => self.out[..len].to_vec(),
            Pick::Strong | Pick::Fallback => self.out2[..len].to_vec(),
        };
        self.check(&bytes, block)?;
        Ok(Some(Stored::Zlib {
            bytes,
            strong: from == Pick::Strong,
            refused,
            fallback: from == Pick::Fallback,
        }))
    }

    /// `stored` inflates to exactly `block`.
    fn check(&mut self, stored: &[u8], block: &[u8]) -> io::Result<()> {
        self.inflate.reset(true);
        let status = self
            .inflate
            .decompress(stored, &mut self.back, FlushDecompress::Finish);
        let whole = matches!(status, Ok(Status::StreamEnd))
            && self.inflate.total_in() as usize == stored.len()
            && self.inflate.total_out() as usize == BLOCK;
        if whole && self.back[..] == block[..] {
            Ok(())
        } else {
            Err(io::Error::other(
                "a compressed block did not inflate back to its source bytes",
            ))
        }
    }
}

fn is_zero(block: &[u8]) -> bool {
    // Word at a time; the compiler vectorises the fold.
    block
        .as_chunks::<64>()
        .0
        .iter()
        .all(|c| c.iter().fold(0u8, |a, &b| a | b) == 0)
}

/// Whether the block's bytes are spread so evenly that no deflate could get it under `cap`, and
/// nothing in it repeats: already-compressed data, which is most of a media-heavy game. The
/// order-0 entropy of the bytes bounds what Huffman coding alone can reach; repeats are what
/// LZ77 adds on top, so a block with any is never ruled out here (a table of `i % 251` has a
/// flat histogram and deflates to almost nothing). Measured on a real media-heavy game (1 in 16
/// blocks of PPSA30883): at a 1% gain the check skips 12% of the blocks, and level 6 would have
/// saved 14 KB on the 18 of them it could shrink.
pub fn incompressible(block: &[u8], cap: usize) -> bool {
    // A quarter first: most blocks of most games are plainly compressible, and their sample
    // says so (an early "no" only means the block is deflated as usual).
    let quarter = &block[..block.len() / 4];
    if entropy_bits(quarter) * 4.0 / 8.0 < cap as f64 * SAMPLE_MARGIN {
        return false;
    }
    entropy_bits(block) / 8.0 > cap as f64 && !repeats(block)
}

/// How far under the cap a quarter's estimate must fall to settle it: a 16 KiB sample of a
/// block that is evenly random reads about 0.01 bit a byte low, and a block's quarters can
/// differ; 3% under is well clear of both.
const SAMPLE_MARGIN: f64 = 0.97;

/// The order-0 entropy of `bytes`, in bits for all of them.
fn entropy_bits(bytes: &[u8]) -> f64 {
    let mut counts = [[0u32; 256]; 4];
    for c in bytes.as_chunks::<4>().0 {
        counts[0][c[0] as usize] += 1;
        counts[1][c[1] as usize] += 1;
        counts[2][c[2] as usize] += 1;
        counts[3][c[3] as usize] += 1;
    }
    let [a, b, c, d] = &counts;
    let n = bytes.len() as f64;
    let mut bits = 0.0;
    for (((a, b), c), d) in a.iter().zip(b).zip(c).zip(d) {
        let k = a + b + c + d;
        if k > 0 {
            let k = f64::from(k);
            bits -= k * (k / n).log2();
        }
    }
    bits
}

/// Whether 4-byte sequences recur in the block more than chance allows: a hash table of the
/// last place each was seen, the way a fast LZ77 match finder starts. In noise two equal
/// 4-byte sequences meet about once per 2^32 tries, so a few dozen hits mean real repeats.
fn repeats(block: &[u8]) -> bool {
    const HITS: u32 = 32;
    let mut last = vec![0u32; 1 << 12];
    let mut hits = 0;
    for i in 0..block.len().saturating_sub(3) {
        let v = u32::from_le_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]);
        let h = (v.wrapping_mul(0x9E37_79B1) >> 20) as usize;
        let seen = last[h] as usize;
        if seen > 0 && block[seen - 1..seen + 3] == block[i..i + 4] {
            hits += 1;
            if hits >= HITS {
                return true;
            }
        }
        last[h] = i as u32 + 1;
    }
    false
}
