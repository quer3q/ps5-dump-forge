//! AMPRIDX3 path index parser (`ps5_dump_forge_lz4::index::read_index`) on arbitrary bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_lz4::index::read_index;

fuzz_target!(|data: &[u8]| {
    let _ = read_index(data);
});
