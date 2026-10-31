//! Patch packages (`\x7FLIH`): a built package re-wrapped as a patch that carries some of its
//! outer blocks reads back as the files those blocks hold whole.

mod common;

use common::{BIG, build};
use ps5_dump_forge_fpkg::FpkgSource;
use ps5upload_fpkg::BLOCK;
use ps5upload_fpkg::source::SourceTree;

const B: usize = BLOCK as usize;

fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// `pkg` as a patch carrying every outer block but `omit`: a 64 KiB header, the FIH block, the
/// carried blocks in order, the container.
fn patch_of(pkg: &[u8], omit: &[usize]) -> Vec<u8> {
    let (pfs_offset, pfs_size, cnt_offset) = (
        le64(pkg, 0x10) as usize,
        le64(pkg, 0x18) as usize,
        le64(pkg, 0x58) as usize,
    );
    // The container runs to the image or to the package's end, whichever comes first after it.
    let cnt_end = if cnt_offset < pfs_offset {
        pfs_offset
    } else {
        pkg.len()
    };
    let mut out = vec![0u8; B];
    out.extend_from_slice(&pkg[..B]);
    for i in (0..pfs_size / B).filter(|i| !omit.contains(i)) {
        out.extend_from_slice(&pkg[pfs_offset + i * B..pfs_offset + (i + 1) * B]);
    }
    let cnt_at = out.len();
    out.extend_from_slice(&pkg[cnt_offset..cnt_end]);
    let len = out.len() as u64;
    out[..4].copy_from_slice(b"\x7FLIH");
    for (at, v) in [
        (0x18, len),
        (0x20, BLOCK),
        (0x28, BLOCK),
        (0x30, cnt_at as u64),
        (0x38, (cnt_end - cnt_offset) as u64),
    ] {
        out[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    out
}

#[test]
fn a_patch_lists_only_the_files_it_carries_whole() {
    // A random stretch of the big file is stored raw, so its bytes are found in the package.
    let built = build("patch-some", 0, |_| {});
    let pkg = built.package();
    let pfs_offset = le64(&pkg, 0x10) as usize;
    let needle = &built.bytes(BIG)[70_000 + 1000..70_000 + 1064];
    let at = pkg
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("a raw stretch of the big file");
    let omit = (at - pfs_offset) / B;
    let path = built.path.with_extension("patch.pkg");
    std::fs::write(&path, patch_of(&pkg, &[omit])).unwrap();

    // The block holds other files' bytes too (the image is small): each listed one reads back
    // whole, and every one left out is counted.
    let mut source = FpkgSource::open(&path, None).unwrap();
    let listed: Vec<String> = source.files().iter().map(|f| f.path.clone()).collect();
    assert!(!listed.iter().any(|p| p == BIG), "{listed:?}");
    assert!(
        listed.iter().any(|p| p == "sce_sys/icon0.png"),
        "the container's: {listed:?}"
    );
    for p in &listed {
        assert!(source.read(p).unwrap() == built.bytes(p), "{p}");
    }
    let details = source.details().join("\n");
    let left_out = built.expected.len() - listed.len();
    assert!(
        details.contains(&format!(
            "left out: {left_out} files partly the base game's"
        )),
        "{details}"
    );
}
