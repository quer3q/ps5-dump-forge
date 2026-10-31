//! Round trips through the writer, checked by the vendored reader and by the mini fsck in
//! `common`. Images land in `$CARGO_TARGET_TMPDIR/ufs2/` so `scripts/fsck-ufs.sh` can be
//! pointed at them afterwards.

mod common;

use std::sync::atomic::AtomicBool;

use common::{Synth, build, compare_with_reader, fsck, scratch};
use ps5_dump_forge_ufs2::Options;
use ps5upload_fpkg::source::{FolderSource, SourceTree};

const B: u64 = 65536;

fn assert_clean(image: &std::path::Path) -> common::Fsck {
    common::script_last_group_check(image);
    let r = fsck(image);
    assert!(r.errors.is_empty(), "{}: {:#?}", image.display(), r.errors);
    r
}

/// A game-shaped folder through `FolderSource`: nested dirs, an empty dir, a zero-length
/// file, files at the direct -> single-indirect edge, non-ASCII (NFC) names.
#[test]
fn game_folder_round_trip() {
    let root = scratch("game-src");
    let _ = std::fs::remove_dir_all(&root);
    let files: &[(&str, u64)] = &[
        ("eboot.bin", 3 * B + 17),
        ("sce_sys/param.json", 812),
        ("sce_sys/icon0.png", 70_000),
        ("sce_module/libc.prx", 12 * B), // exactly the 12 direct blocks
        ("sce_module/libSceFios2.prx", 12 * B + 1), // first single-indirect block
        ("data/empty.bin", 0),
        ("data/deep/er/still/x.dat", 5),
        ("data/caf\u{e9}/\u{65e5}\u{672c}.txt", 4096),
    ];
    for (i, (p, size)) in files.iter().enumerate() {
        let path = root.join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, common::pattern(i, 0, *size as usize)).unwrap();
    }
    std::fs::create_dir_all(root.join("data/shaders")).unwrap();

    let mut src = FolderSource::open(&root).unwrap();
    assert_eq!(src.empty_dirs(), ["data/shaders"]);
    let opts = Options {
        maker: Some("PS5-FORGE-v0.0.1-pre4".into()),
        ..Options::default()
    };
    let (img, layout, report) = build("game", &mut src, &opts);
    assert_eq!(report.files, files.len() as u64);
    assert_eq!(report.image_size % B, 0);
    assert_eq!(std::fs::metadata(&img).unwrap().len(), layout.image_size);
    assert!(layout.last_group_blocks > layout.metadata_blocks_per_group);
    let r = assert_clean(&img);
    assert!(
        r.dirs.contains(&"data/shaders".to_string()),
        "empty dir kept: {:?}",
        r.dirs
    );
    assert_eq!(r.files.len(), files.len());
    compare_with_reader(&img, &mut src);

    // The maker's mark is fs_volname in the primary and every backup superblock.
    let first = std::fs::read(&img).unwrap();
    let volname = |at: usize| &first[at + 680..at + 712];
    let mut want = [0u8; 32];
    want[..21].copy_from_slice(b"PS5-FORGE-v0.0.1-pre4");
    assert_eq!(volname(65536), want);
    let fpg = layout.blocks_per_group as usize * B as usize;
    for c in 0..layout.cylinder_groups as usize {
        assert_eq!(
            volname(c * fpg + 2 * B as usize),
            want,
            "backup in group {c}"
        );
    }
    let mut f = std::fs::File::open(&img).unwrap();
    let got = ps5_dump_forge_ufs2::read_volname(&mut f).unwrap();
    assert_eq!(got.as_deref(), Some("PS5-FORGE-v0.0.1-pre4"));

    // Same tree, same bytes.
    let (img2, _, _) = build("game-again", &mut src, &opts);
    assert!(
        first == std::fs::read(&img2).unwrap(),
        "output is not deterministic"
    );
    std::fs::remove_file(img2).unwrap();
}

/// Every bad name is reported, in one error, before anything is written.
#[test]
fn rejected_names_listed_together() {
    let long = "x".repeat(256);
    let ok255 = "y".repeat(255);
    let bad: Vec<String> = vec![
        "nfd/e\u{301}.txt".into(),
        format!("long/{long}"),
        "nul/a\0b".into(),
        "empty//comp".into(),
        "dot/../x".into(),
        "lossy/\u{FFFD}.bin".into(),
        "clash".into(),
        "clash/inner".into(),
        format!("deep/{}", "d/".repeat(600) + "f"),
    ];
    let mut list: Vec<(&str, u64)> = bad.iter().map(|s| (s.as_str(), 1)).collect();
    list.push((&ok255, 1));
    list.push(("fine.bin", 1));
    let src = Synth::new(&list);
    let cancel = AtomicBool::new(false);
    let e = ps5_dump_forge_ufs2::plan(&src, &Options::default(), &cancel)
        .unwrap_err()
        .to_string();
    for needle in [
        "nfd/",
        "long/",
        "nul/",
        "empty//",
        "dot/",
        "lossy/",
        "\"clash\"",
    ] {
        assert!(e.contains(needle), "{needle} missing from: {e}");
    }
    assert!(!e.contains("fine.bin") && !e.contains(&ok255[..20]), "{e}");
}

/// The layout belongs to the tree it was planned from, and the output must be empty.
#[test]
fn write_refuses_mismatches() {
    let src = Synth::new(&[("eboot.bin", 10)]);
    let cancel = AtomicBool::new(false);
    let layout = ps5_dump_forge_ufs2::plan(&src, &Options::default(), &cancel).unwrap();
    let path = scratch("mismatch.ffpkg");
    let _ = std::fs::remove_file(&path);
    let mut out = std::fs::File::create_new(&path).unwrap();
    // Resized, and renamed at the same size: both refused.
    for other in [&[("eboot.bin", 11)], &[("eboot.bim", 10)]] {
        let mut other = Synth::new(other);
        let e = ps5_dump_forge_ufs2::write(&mut other, &layout, &mut out, &cancel, &mut |_, _| {});
        assert!(e.unwrap_err().to_string().contains("source changed"));
    }
    let mut src = src;
    std::io::Write::write_all(&mut out, b"x").unwrap();
    assert!(
        ps5_dump_forge_ufs2::write(&mut src, &layout, &mut out, &cancel, &mut |_, _| {}).is_err()
    );
    std::fs::remove_file(path).unwrap();
}
