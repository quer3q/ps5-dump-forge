//! `reader::unpack` over an in-memory tree of manifest + `.crc` + one volume (input shape in
//! `src/lz4_tree.rs`), then every file read back: whole when small, else its two ends.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fuzz::lz4_tree::tree;
use ps5_dump_forge_fuzz::{MAX_FILES, READ_CAP};
use ps5_dump_forge_lz4::reader::unpack;
use ps5upload_fpkg::source::SourceTree;

fuzz_target!(|data: &[u8]| {
    let Ok(mut u) = unpack(Box::new(tree(data))) else {
        return;
    };
    let _ = u.describe();
    let _ = u.empty_dirs().len();
    let files: Vec<_> = u.files().iter().take(MAX_FILES).cloned().collect();
    for f in files {
        if f.size <= READ_CAP as u64 {
            let _ = u.read(&f.path);
        }
        let _ = u.read_range(&f.path, 0, READ_CAP);
        let _ = u.read_range(&f.path, f.size.saturating_sub(READ_CAP as u64), READ_CAP);
    }
});
