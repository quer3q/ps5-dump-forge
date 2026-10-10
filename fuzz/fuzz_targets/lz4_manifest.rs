//! AMPRPAK4 manifest parser (`ps5_dump_forge_lz4::reader::open_manifest`) on arbitrary bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_lz4::reader::open_manifest;

fuzz_target!(|data: &[u8]| {
    let _ = open_manifest(data);
});
