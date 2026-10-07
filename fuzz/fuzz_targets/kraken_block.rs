//! The Kraken block decoder (`kraken::decode_block`) on arbitrary halves.
//! Input: u8 kinds (bits 0-1 even, 2-3 odd: raw/LZ/LZ-delta), u32 LE logical length (taken
//! modulo 256 KiB, plus one), u32 LE length of the even half's bytes, then the halves' bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5upload_fpkg::kraken::{HALF, Half, decode_block};

fn half(kind: u8, bytes: &[u8]) -> Half {
    match kind % 3 {
        0 => Half::Raw(bytes.to_vec()),
        1 => Half::Lz(bytes.to_vec()),
        _ => Half::LzDelta(bytes.to_vec()),
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 9 {
        return;
    }
    let le32 = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let len = le32(1) % (2 * HALF) + 1;
    let rest = &data[9..];
    let (even, odd) = rest.split_at(le32(5).min(rest.len()));
    let mut halves = vec![half(data[0], even)];
    if len > HALF {
        halves.push(half(data[0] >> 2, odd));
    }
    if let Ok(out) = decode_block(&halves, len) {
        assert_eq!(out.len(), len);
    }
});
