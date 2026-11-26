//! Whether the PS5 kernel's PFSC decoder reads a zlib stream the way every other inflater does.
//!
//! Measured on FW 13.60 (Phat): a `.ffpfsc` whose blocks were encoded by libdeflate level 12
//! decoded on the console to the right bytes in all but 3 of 273,001 compressed blocks, and for
//! those 3 the kernel silently returned the previous block's bytes (no read error). The 3 copied
//! into a fresh container at other offsets failed the same way, so it is the streams, not where
//! they sit. All 3, and none of 200 sampled good blocks, have a code-length repeat (code 16) that
//! runs from the literal/length code lengths into the distance code lengths. RFC 1951 allows it
//! (the two lists are one sequence) and zlib, libdeflate and old zlib 1.0.4/1.1.4 all decode it,
//! but zlib's own encoder never writes it (it run-length codes the two lists separately), and
//! libdeflate does, now and then. The console's decoder evidently resets between the two lists.
//!
//! The same image read back in full also had 5 blocks whose distance lengths start with a code 16
//! copying the last literal length, and the console read those right: it carries the previous
//! length over, it only breaks on one run that spans both lists.
//!
//! [`console_safe`] walks a stream's block headers and refuses a run crossing the boundary (any
//! of codes 16, 17, 18: only 16 was seen to fail, zeros are refused to be safe), a missing
//! end-of-block code, and an over-subscribed or (beyond a lone distance code) incomplete code. A
//! refused stream is not stored: the encoder falls back to one that passes (zlib-rs, which never
//! writes a crossing run). The bytes themselves are checked separately, by inflating the block
//! back, so the walk stops at the last block's header.

/// Walks the deflate block headers of `zlib` (a zlib stream): `true` when none uses a construct
/// the console's decoder misreads. A stream that cannot be walked is refused too.
pub fn console_safe(zlib: &[u8]) -> bool {
    walk(zlib).is_some()
}

struct Bits<'a> {
    data: &'a [u8],
    /// Bit position from the start of `data`.
    pos: usize,
}

impl Bits<'_> {
    /// The next `n` (at most 32) bits, least significant first, without consuming them; bits
    /// past the end read as zeros.
    fn peek(&self, n: u32) -> u32 {
        let byte = self.pos / 8;
        let mut w = [0u8; 8];
        let avail = self.data.len().saturating_sub(byte).min(8);
        w[..avail].copy_from_slice(&self.data[byte..byte + avail]);
        let v = u64::from_le_bytes(w) >> (self.pos % 8);
        (v & ((1u64 << n) - 1)) as u32
    }

    fn take(&mut self, n: u32) -> Option<u32> {
        let v = self.peek(n);
        self.pos += n as usize;
        (self.pos <= self.data.len() * 8).then_some(v)
    }
}

/// A canonical Huffman code as a full lookup table indexed by the next `bits` bits; each entry
/// is `symbol << 4 | length`, 0 where no code lands.
struct Code {
    bits: u32,
    table: Vec<u16>,
}

impl Code {
    /// `None` for an over-subscribed code, or an incomplete one other than a single 1-bit code
    /// (which zlib accepts for distances).
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let bits = (1..16).rev().find(|&l| count[l] > 0).unwrap_or(0) as u32;
        if bits == 0 {
            return Some(Self {
                bits: 1,
                table: vec![0; 2],
            });
        }
        let mut left = 1i32;
        for c in &count[1..] {
            left = (left << 1) - i32::from(*c);
            if left < 0 {
                return None;
            }
        }
        let used: u16 = count.iter().sum();
        if left > 0 && !(used == 1 && count[1] == 1) {
            return None;
        }
        let mut next = [0u32; 16];
        let mut code = 0u32;
        for l in 1..16 {
            code = (code + u32::from(count[l - 1])) << 1;
            next[l] = code;
        }
        next[0] = 0;
        let mut table = vec![0u16; 1 << bits];
        for (sym, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            let r = c.reverse_bits() >> (32 - u32::from(l));
            let entry = (sym as u16) << 4 | u16::from(l);
            let mut k = r as usize;
            while k < table.len() {
                table[k] = entry;
                k += 1 << l;
            }
        }
        Some(Self { bits, table })
    }

    fn decode(&self, b: &mut Bits) -> Option<u16> {
        let e = self.table[b.peek(self.bits) as usize];
        if e == 0 {
            return None;
        }
        b.take(u32::from(e & 15))?;
        Some(e >> 4)
    }
}

const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Output bytes no PFSC block exceeds: past it the stream is not one of ours.
const MAX_OUT: usize = 1 << 17;

fn walk(zlib: &[u8]) -> Option<()> {
    if zlib.len() < 6 || zlib[0] & 0x0F != 8 || zlib[1] & 0x20 != 0 {
        return None;
    }
    if (u16::from(zlib[0]) << 8 | u16::from(zlib[1])) % 31 != 0 {
        return None;
    }
    let mut b = Bits {
        data: &zlib[2..],
        pos: 0,
    };
    let mut out = 0usize;
    loop {
        let last = b.take(1)?;
        match b.take(2)? {
            0 => {
                b.pos = b.pos.div_ceil(8) * 8;
                let len = b.take(16)?;
                let nlen = b.take(16)?;
                if len != !nlen & 0xFFFF {
                    return None;
                }
                b.pos += len as usize * 8;
                if b.pos > b.data.len() * 8 {
                    return None;
                }
                out += len as usize;
            }
            1 => {
                // The fixed codes: distances as 32 five-bit codes (30 and 31 never occur).
                let mut lens = [0u8; 288 + 32];
                lens[..144].fill(8);
                lens[144..256].fill(9);
                lens[256..280].fill(7);
                lens[280..288].fill(8);
                lens[288..].fill(5);
                let lit = Code::new(&lens[..288])?;
                let dist = Code::new(&lens[288..])?;
                out += symbols(&mut b, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = header(&mut b)?;
                // No header follows the last block: the rest is data, which the caller checks
                // by inflating it back.
                if last == 1 {
                    return Some(());
                }
                out += symbols(&mut b, &lit, &dist)?;
            }
            _ => return None,
        }
        if out > MAX_OUT {
            return None;
        }
        if last == 1 {
            break;
        }
    }
    // The Adler-32 trailer, whole.
    (b.pos.div_ceil(8) + 4 <= b.data.len()).then_some(())
}

/// A dynamic block's two codes, refusing what the console misreads.
fn header(b: &mut Bits) -> Option<(Code, Code)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = b.take(5)? as usize + 257;
    let hdist = b.take(5)? as usize + 1;
    let hclen = b.take(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return None;
    }
    let mut pre = [0u8; 19];
    for &i in &ORDER[..hclen] {
        pre[i] = b.take(3)? as u8;
    }
    let pre = Code::new(&pre)?;
    let total = hlit + hdist;
    let mut lens = [0u8; 286 + 30];
    let mut i = 0;
    while i < total {
        let sym = pre.decode(b)?;
        let (value, run) = match sym {
            0..=15 => (sym as u8, 1),
            16 => {
                // A run that starts at the first distance length, copying the last literal
                // length, reads fine on the console (measured); only one that crosses fails.
                if i == 0 {
                    return None;
                }
                (lens[i - 1], 3 + b.take(2)? as usize)
            }
            17 => (0, 3 + b.take(3)? as usize),
            _ => (0, 11 + b.take(7)? as usize),
        };
        // The one the console misreads: a run from the literal lengths into the distance ones.
        if i + run > total || (i < hlit && i + run > hlit) {
            return None;
        }
        lens[i..i + run].fill(value);
        i += run;
    }
    if lens[256] == 0 {
        return None;
    }
    Some((Code::new(&lens[..hlit])?, Code::new(&lens[hlit..total])?))
}

/// Decodes one block's symbols up to end-of-block: the bytes it stands for.
fn symbols(b: &mut Bits, lit: &Code, dist: &Code) -> Option<usize> {
    let mut out = 0usize;
    loop {
        let sym = lit.decode(b)? as usize;
        match sym {
            0..=255 => out += 1,
            256 => return Some(out),
            257..=285 => {
                let i = sym - 257;
                let len = usize::from(LEN_BASE[i]) + b.take(u32::from(LEN_EXTRA[i]))? as usize;
                let d = dist.decode(b)? as usize;
                if d >= 30 {
                    return None;
                }
                b.take(u32::from(DIST_EXTRA[d]))?;
                out += len;
            }
            _ => return None,
        }
        if out > MAX_OUT {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zlib_rs(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn sample() -> Vec<u8> {
        let mut x = 7u32;
        (0..1 << 16)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                if i % 3 == 0 {
                    x as u8 & 0x3F
                } else {
                    b"deflate "[i % 8]
                }
            })
            .collect()
    }

    #[test]
    fn zlib_streams_are_safe() {
        let data = sample();
        for level in [0, 1, 6, 9] {
            assert!(console_safe(&zlib_rs(&data, level)), "zlib-rs {level}");
        }
        assert!(console_safe(&zlib_rs(&[0u8; 1 << 16], 9)));
    }

    #[test]
    fn damaged_streams_are_refused() {
        // Only headers are walked (the data is checked by inflating it back): a stream cut in
        // its header is refused.
        let z = zlib_rs(&sample(), 6);
        assert!(!console_safe(&z[..10]));
        let mut bad = z.clone();
        bad[2] |= 0b110; // block type 3
        assert!(!console_safe(&bad));
        assert!(!console_safe(&[0x78, 0x9c]));
    }

    /// A hand-made block whose code-length repeat crosses into the distance lengths is refused;
    /// the same block with the two lists coded apart is not. zlib reads both.
    #[test]
    fn a_repeat_into_the_distance_lengths_is_refused() {
        use std::io::Read;
        let crossing = three_as(Run::Across);
        let separate = three_as(Run::Apart);
        let at_boundary = three_as(Run::FromBoundary);
        for z in [&crossing, &separate, &at_boundary] {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&z[..])
                .read_to_end(&mut out)
                .unwrap();
            assert_eq!(out, b"aaa");
        }
        assert!(console_safe(&separate));
        assert!(!console_safe(&crossing));
        // A run starting at the first distance length, copying the last literal length: the
        // console reads it (measured), so it is not refused.
        assert!(console_safe(&at_boundary));
    }

    enum Run {
        /// 254, then one repeat for 255, 256 and distance 0.
        Across,
        /// 254, 255, 256 and distance 0 written out, then a repeat for distances 1-3.
        Apart,
        /// 254, 255, 256 written out, then a repeat for distances 0-2, then distance 3.
        FromBoundary,
    }

    /// "aaa" as one dynamic block. Literal/length lengths: 97, 254, 255 and 256 (end of block)
    /// 2 bits each; distance lengths: codes 0-3, 2 bits each (HLIT 257, HDIST 4), the lengths
    /// from 254 on written as `run` says.
    fn three_as(run: Run) -> Vec<u8> {
        struct W {
            out: Vec<u8>,
            acc: u64,
            n: u32,
        }
        impl W {
            fn put(&mut self, v: u32, bits: u32) {
                self.acc |= u64::from(v) << self.n;
                self.n += bits;
                while self.n >= 8 {
                    self.out.push(self.acc as u8);
                    self.acc >>= 8;
                    self.n -= 8;
                }
            }
            /// A Huffman code, most significant bit first.
            fn code(&mut self, c: u32, len: u32) {
                for i in (0..len).rev() {
                    self.put((c >> i) & 1, 1);
                }
            }
        }
        let mut w = W {
            out: vec![0x78, 0x01],
            acc: 0,
            n: 0,
        };
        // Precode: symbols 0, 2, 16, 18 at 2 bits: codes 00, 01, 10, 11 in symbol order.
        let pre = |w: &mut W, sym: u32| {
            let c = match sym {
                0 => 0,
                2 => 1,
                16 => 2,
                _ => 3,
            };
            w.code(c, 2);
        };
        w.put(1, 1); // last block
        w.put(2, 2); // dynamic
        w.put(0, 5); // HLIT 257
        w.put(3, 5); // HDIST 4
        w.put(12, 4); // HCLEN 16: up to symbol 2 in the precode order
        const ORDER: [u32; 16] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2];
        for s in ORDER {
            w.put(if matches!(s, 0 | 2 | 16 | 18) { 2 } else { 0 }, 3);
        }
        pre(&mut w, 18); // 0..=96: 97 zeros
        w.put(86, 7);
        pre(&mut w, 2); // 97
        pre(&mut w, 18); // 98..=253: 156 zeros
        w.put(127, 7);
        pre(&mut w, 18);
        w.put(7, 7);
        pre(&mut w, 2); // 254
        match run {
            Run::Across => {
                pre(&mut w, 16); // 255, 256, distance 0
                w.put(0, 2);
                pre(&mut w, 16); // distances 1-3
                w.put(0, 2);
            }
            Run::Apart => {
                pre(&mut w, 2); // 255
                pre(&mut w, 2); // 256
                pre(&mut w, 2); // distance 0
                pre(&mut w, 16); // distances 1-3
                w.put(0, 2);
            }
            Run::FromBoundary => {
                pre(&mut w, 2); // 255
                pre(&mut w, 2); // 256
                pre(&mut w, 16); // distances 0-2
                w.put(0, 2);
                pre(&mut w, 2); // distance 3
            }
        }
        // Literal/length codes in symbol order: 97 = 00, 254 = 01, 255 = 10, 256 = 11.
        for _ in 0..3 {
            w.code(0b00, 2);
        }
        w.code(0b11, 2);
        w.put(0, 7); // to a byte boundary
        let mut out = w.out;
        out.extend_from_slice(&adler(b"aaa").to_be_bytes());
        out
    }

    fn adler(d: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &x in d {
            a = (a + u32::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        b << 16 | a
    }
}
