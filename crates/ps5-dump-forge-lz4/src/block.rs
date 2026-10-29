//! LZ4 block format through C liblz4 (`lz4-sys`): what ampr_emu decodes. No frame format.

use std::os::raw::{c_char, c_int};

/// Fast: `LZ4_compress_default` (acceleration 1).
pub const LEVEL_FAST: u8 = 0;
/// Medium: LZ4 HC level 9.
pub const LEVEL_MEDIUM: u8 = 9;
/// Max: LZ4 HC level 12.
pub const LEVEL_MAX: u8 = 12;

/// 0 (fast) or an HC level, 3..=12.
pub fn valid_level(level: u8) -> bool {
    level == LEVEL_FAST || (3..=LEVEL_MAX).contains(&level)
}

/// `raw` as one LZ4 block at `level` (see [`valid_level`]); `None` when the C library fails
/// (an input over `LZ4_MAX_INPUT_SIZE`, which a block never is).
pub fn compress(raw: &[u8], level: u8) -> Option<Vec<u8>> {
    let n = c_int::try_from(raw.len()).ok()?;
    // SAFETY: a pure function of its argument.
    let bound = unsafe { lz4_sys::LZ4_compressBound(n) };
    if bound <= 0 {
        return None;
    }
    let mut out = vec![0u8; bound as usize];
    let src = raw.as_ptr() as *const c_char;
    let dst = out.as_mut_ptr() as *mut c_char;
    // SAFETY: `src` holds `n` bytes and `dst` `bound` bytes, both live for the call; liblz4
    // writes at most `bound` bytes and keeps no pointer.
    let len = unsafe {
        if level == LEVEL_FAST {
            lz4_sys::LZ4_compress_default(src, dst, n, bound)
        } else {
            lz4_sys::LZ4_compress_HC(src, dst, n, bound, c_int::from(level))
        }
    };
    if len <= 0 {
        return None;
    }
    out.truncate(len as usize);
    Some(out)
}

/// Decodes the block `src` into `out`; the decoded length, or `None` when `src` is not a valid
/// block or decodes to more than `out` holds.
pub fn decompress_into(src: &[u8], out: &mut [u8]) -> Option<usize> {
    let n = c_int::try_from(src.len()).ok()?;
    let cap = c_int::try_from(out.len()).ok()?;
    // SAFETY: `LZ4_decompress_safe` reads at most `n` bytes of `src` and writes at most `cap`
    // bytes of `out`, whatever the input; both slices live for the call.
    let len = unsafe {
        lz4_sys::LZ4_decompress_safe(
            src.as_ptr() as *const c_char,
            out.as_mut_ptr() as *mut c_char,
            n,
            cap,
        )
    };
    usize::try_from(len).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_round_trips_and_higher_levels_are_no_bigger() {
        let raw: Vec<u8> = (0..8000u32)
            .flat_map(|i| {
                format!("row {i} = {}\n", i.wrapping_mul(2_654_435_761) % 977).into_bytes()
            })
            .collect();
        let mut sizes = Vec::new();
        for level in [LEVEL_FAST, 3, LEVEL_MEDIUM, LEVEL_MAX] {
            assert!(valid_level(level));
            let c = compress(&raw, level).unwrap();
            let mut out = vec![0; raw.len()];
            assert_eq!(decompress_into(&c, &mut out), Some(raw.len()));
            assert_eq!(out, raw);
            sizes.push(c.len());
        }
        assert!(sizes[2] <= sizes[0] && sizes[3] <= sizes[2], "{sizes:?}");
        assert!(!valid_level(1) && !valid_level(2) && !valid_level(13));
        let mut short = vec![0; 10];
        assert_eq!(
            decompress_into(&compress(&raw, 0).unwrap(), &mut short),
            None
        );
        assert_eq!(decompress_into(&[0xF0, 1, 2], &mut short), None);
    }
}
