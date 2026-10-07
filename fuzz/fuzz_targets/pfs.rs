//! PFS reader (`ps5_dump_forge_pfs`): `.ffpfs` through `PfsSource` (header, inode table,
//! directory walk, compressed files), and `.ffpfsc` through `open_ffpfsc` (the outer PFS, the
//! PFSC window over its one file, and the nested image's reader).
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fuzz::sparse::Sparse;
use ps5_dump_forge_fuzz::{MAX_FILES, READ_CAP};
use ps5_dump_forge_pfs::{PfsSource, open_ffpfsc};
use ps5upload_fpkg::source::SourceTree;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut tree) = PfsSource::from_reader(Box::new(Sparse::decode(data)), "fuzz".into()) {
        let _ = tree.header();
        read_back(&mut tree);
    }
    if let Ok((mut tree, _)) = open_ffpfsc(Box::new(Sparse::decode(data)), "fuzz") {
        read_back(tree.as_mut());
    }
});

fn read_back(tree: &mut dyn SourceTree) {
    let _ = tree.empty_dirs().len();
    let _ = tree.describe();
    let files: Vec<_> = tree.files().iter().take(MAX_FILES).cloned().collect();
    for f in files {
        let _ = tree.read_range(&f.path, 0, READ_CAP);
        let _ = tree.read_range(&f.path, f.size.saturating_sub(READ_CAP as u64), READ_CAP);
    }
}
