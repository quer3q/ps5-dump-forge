//! UFS2 reader (`ps5upload_fpkg::ufs2_source`, over `ps5upload_pkg::ufs2`): superblock,
//! cylinder groups, inodes, directory walk, direct and indirect block maps.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fuzz::sparse::Sparse;
use ps5_dump_forge_fuzz::{MAX_FILES, READ_CAP};
use ps5upload_fpkg::source::SourceTree;
use ps5upload_fpkg::ufs2_source::Ufs2Source;

fuzz_target!(|data: &[u8]| {
    let Ok(mut tree) = Ufs2Source::from_reader(Box::new(Sparse::decode(data)), "fuzz".into())
    else {
        return;
    };
    let _ = tree.empty_dirs().len();
    let _ = tree.describe();
    let files: Vec<_> = tree.files().iter().take(MAX_FILES).cloned().collect();
    for f in files {
        let _ = tree.read_range(&f.path, 0, READ_CAP);
        let _ = tree.read_range(&f.path, f.size.saturating_sub(READ_CAP as u64), READ_CAP);
    }
});
