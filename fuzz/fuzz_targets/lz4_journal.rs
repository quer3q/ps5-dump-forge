//! AMPRCMD1 journal scanner (`ps5_dump_forge_lz4::journal::scan`). The first four input bytes
//! are the pack's file count (LE), the rest is the journal.
#![no_main]

use std::io::Cursor;
use std::sync::atomic::AtomicBool;

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_lz4::journal::scan;

fuzz_target!(|data: &[u8]| {
    let (head, rest) = data.split_at(data.len().min(4));
    let mut n = [0u8; 4];
    n[..head.len()].copy_from_slice(head);
    let _ = scan(
        Cursor::new(rest),
        u32::from_le_bytes(n),
        &AtomicBool::new(false),
    );
});
