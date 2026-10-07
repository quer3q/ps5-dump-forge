//! The streaming `.pkg` reader (`ps5_dump_forge_fpkg::FpkgSource`): FIH, container, outer PFS,
//! inner image, Kraken blocks, all through `SourceTree`. It opens a path, so each input is
//! written to a per-process sparse temp file.
#![no_main]

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fpkg::FpkgSource;
use ps5_dump_forge_fuzz::sparse::Sparse;
use ps5_dump_forge_fuzz::{MAX_FILES, READ_CAP};
use ps5upload_fpkg::source::SourceTree;

fn scratch() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| std::env::temp_dir().join(format!("forge-fuzz-{}.pkg", std::process::id())))
}

fuzz_target!(|data: &[u8]| {
    let path = scratch();
    let mut file: File = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .expect("scratch file");
    Sparse::decode(data)
        .write_to(&mut file)
        .expect("scratch write");
    drop(file);
    let Ok(mut tree) = FpkgSource::open(path, None) else {
        return;
    };
    let _ = tree.details();
    let _ = tree.describe();
    let _ = tree.empty_dirs().len();
    let files: Vec<_> = tree.files().iter().take(MAX_FILES).cloned().collect();
    for f in files {
        let _ = tree.read_range(&f.path, 0, READ_CAP);
        let _ = tree.read_range(&f.path, f.size.saturating_sub(READ_CAP as u64), READ_CAP);
    }
});
