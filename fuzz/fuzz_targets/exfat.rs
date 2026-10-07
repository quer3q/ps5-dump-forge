//! exFAT reader (`ps5upload_fpkg::exfat`): boot sector, FAT, directory walk, file reads.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fuzz::sparse::Sparse;
use ps5_dump_forge_fuzz::{MAX_FILES, READ_CAP};
use ps5upload_fpkg::PkgFile;
use ps5upload_fpkg::exfat::ExFat;

fuzz_target!(|data: &[u8]| {
    let image = Sparse::decode(data);
    let len = image.len();
    let Ok(mut volume) = ExFat::from_file(PkgFile::from_reader(Box::new(image), len), "fuzz")
    else {
        return;
    };
    let _ = volume.geometry();
    let Ok((files, _empty_dirs)) = volume.walk_tree() else {
        return;
    };
    for f in files.iter().take(MAX_FILES) {
        let n = f.size.min(READ_CAP as u64) as usize;
        let _ = volume.read_file(f, 0, n);
        // The tail, which sits at the end of the cluster chain.
        let _ = volume.read_file(f, f.size.saturating_sub(n as u64), n);
    }
});
