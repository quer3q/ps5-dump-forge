//! End-to-end checks of the public core API with the Folder target and the readers.
//! Image targets run through the real exFAT and UFS2 writers, `.pkg` through the FPKG builder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ps5_dump_forge_core::{
    ConvertRequest, DeleteError, Event, Format, JobReport, Jobs, KrakenLevel, ScannedFolder,
    VerifyMode, default_output, extraction_findings, inspect, rename_no_replace, stale_parts,
};
use ps5upload_fpkg::source::SourceTree;

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A minimal game: eboot, param.json, a file spanning several copy chunks, an empty dir
/// and some junk.
fn game(root: &Path) {
    write(root, "eboot.bin", b"\x7fELF fake eboot");
    write(
        root,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    let big: Vec<u8> = (0..9 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
    write(root, "data/big.bin", &big);
    write(root, "data/zero.bin", b"");
    std::fs::create_dir_all(root.join("data/empty")).unwrap();
    write(root, ".DS_Store", b"junk");
    write(root, "data/._big.bin", b"junk");
}

fn request(source: &Path, format: Format, output: &Path) -> ConvertRequest {
    ConvertRequest {
        source: source.to_path_buf(),
        format,
        output: output.to_path_buf(),
        compression_threads: None,
        inner: None,
        remove_backport: false,
        full_verify: false,
        kraken_level: KrakenLevel::Fast,
        ffpfsc_level: 6,
        lz4: None,
        zstd_pack: false,
        zstd_level: 5,
        zstd_unpack: false,
        lz4_level: 0,
        // The tests below expect the release runtime in packs; one test picks ampr-emu-ng.
        lz4_runtime: ps5_dump_forge_core::Lz4Runtime::AmprEmu,
        edits: Vec::new(),
        extract: Vec::new(),
        transfer: Vec::new(),
        replace: false,
        edit_rebuild: false,
        install: None,
        archive_password: None,
    }
}

/// Runs one job to completion; returns its events and result. Every job's stage list is
/// checked against what it reports ([`announced_stages`]).
fn run(req: ConvertRequest) -> (Vec<Event>, Result<JobReport, String>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let jobs = Jobs::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    jobs.start(req);
    let mut events = Vec::new();
    loop {
        let event = rx.recv().unwrap();
        if let Event::Done { result, .. } = &event {
            let result = result.clone();
            events.push(event);
            announced_stages(&events);
            return (events, result);
        }
        events.push(event);
    }
}

/// The one `Stages` event comes first and lists every `Progress` stage, which only ever moves
/// forward through it (a failed job stops part way). Returns the list.
fn announced_stages(events: &[Event]) -> Vec<String> {
    let lists: Vec<&Vec<String>> = events
        .iter()
        .filter_map(|e| match e {
            Event::Stages { stages, .. } => Some(stages),
            _ => None,
        })
        .collect();
    assert_eq!(lists.len(), 1, "one Stages event: {events:?}");
    assert!(matches!(events[0], Event::Stages { .. }), "Stages first");
    let list = lists[0];
    let line = serde_json::to_string(&events[0]).unwrap();
    assert!(line.starts_with(r#"{"kind":"stages","job":"#), "{line}");
    let mut at = 0;
    for e in events {
        if let Event::Progress { stage, .. } = e {
            let i = list.iter().position(|s| s == stage);
            let i = i.unwrap_or_else(|| panic!("{stage} is not in {list:?}"));
            assert!(i >= at, "{stage} after {} in {list:?}", list[at]);
            at = i;
        }
    }
    // A job that finishes reports every stage it announced.
    if let Some(Event::Done { result: Ok(_), .. }) = events.last() {
        assert_eq!(&stages(events), list, "reported vs announced");
    }
    list.clone()
}

fn tree_bytes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut tree = ScannedFolder::scan(root, &AtomicBool::new(false)).unwrap();
    let files = tree.files().to_vec();
    files
        .into_iter()
        .map(|f| {
            let bytes = tree.read(&f.path).unwrap();
            (f.path, bytes)
        })
        .collect()
}

/// Every entry under `root`, junk included: path, contents and modification time.
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = path.symlink_metadata().unwrap();
            let bytes = meta.is_file().then(|| std::fs::read(&path).unwrap());
            if meta.is_dir() {
                stack.push(path.clone());
            }
            out.push((path, bytes, meta.modified().unwrap()));
        }
    }
    out.sort();
    out
}

#[cfg(unix)]
#[test]
fn scanner_refuses_links_and_special_files() {
    let root = dir("scan-special");
    game(&root);
    std::os::unix::fs::symlink("eboot.bin", root.join("link.bin")).unwrap();
    std::fs::hard_link(root.join("eboot.bin"), root.join("data/hard.bin")).unwrap();
    let fifo = std::ffi::CString::new(root.join("fifo").to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
    write(&root, "back\\slash", b"x");
    let err = ScannedFolder::scan(&root, &AtomicBool::new(false))
        .err()
        .unwrap()
        .to_string();
    for want in [
        "symlink (not followed): link.bin",
        "hard link (more than one name): data/hard.bin",
        "hard link (more than one name): eboot.bin",
        "special file: fifo (FIFO)",
        "name contains a backslash: back\\slash",
    ] {
        assert!(err.contains(want), "{want:?} not in:\n{err}");
    }
}

#[test]
fn scanner_catches_a_changed_file() {
    let root = dir("scan-stable");
    game(&root);
    let mut tree = ScannedFolder::scan(&root, &AtomicBool::new(false)).unwrap();
    assert_eq!(tree.read_range("eboot.bin", 0, 4).unwrap(), b"\x7fELF");
    write(&root, "eboot.bin", b"\x7fELF a different, longer eboot");
    let err = tree.read_range("eboot.bin", 0, 4).unwrap_err().to_string();
    assert!(err.contains("changed after it was scanned"), "{err}");

    // Same size, new modification time.
    let mut tree = ScannedFolder::scan(&root, &AtomicBool::new(false)).unwrap();
    let file = std::fs::File::options()
        .write(true)
        .open(root.join("data/zero.bin"))
        .unwrap();
    file.set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(tree.read("data/zero.bin").is_err());

    // A file swapped for a FIFO fails at once instead of blocking for a writer.
    #[cfg(unix)]
    {
        let mut tree = ScannedFolder::scan(&root, &AtomicBool::new(false)).unwrap();
        std::fs::remove_file(root.join("eboot.bin")).unwrap();
        let fifo = std::ffi::CString::new(root.join("eboot.bin").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        assert!(tree.read_range("eboot.bin", 0, 4).is_err());
    }
}

#[test]
fn extraction_refuses_unsafe_names() {
    let bad = [
        "/abs",
        "../up",
        "a/../b",
        "a//b",
        "a/./b",
        "trail/",
        "nul\0byte",
        "back\\slash",
        "CON",
        "dir/aux.txt",
        "com3.bin",
        "LPT1",
        "dot.",
        "space ",
        "ads:stream",
        "C:/x",
    ];
    for name in bad {
        let found = extraction_findings(&[name.to_string()], &[], false);
        assert_eq!(found.len(), 1, "{name:?}: {found:?}");
    }
    let ok = ["eboot.bin", "sce_sys/param.json", "COM10", "con_x.txt"];
    let ok: Vec<String> = ok.iter().map(|s| s.to_string()).collect();
    assert!(extraction_findings(&ok, &["data/empty".to_string()], true).is_empty());
    // Case and normalization collisions only matter on a folding destination.
    let pair = ["Eboot.bin".to_string(), "eboot.bin".to_string()];
    assert_eq!(extraction_findings(&pair, &[], true).len(), 1);
    assert!(extraction_findings(&pair, &[], false).is_empty());
    // File versus directory.
    let clash = ["a".to_string(), "a/b".to_string()];
    assert_eq!(extraction_findings(&clash, &[], false).len(), 1);
}

#[test]
fn rename_never_replaces() {
    let root = dir("rename");
    write(&root, "from", b"new");
    write(&root, "to", b"old");
    let err = rename_no_replace(&root.join("from"), &root.join("to")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(root.join("to")).unwrap(), b"old");
    assert!(root.join("from").exists());

    std::fs::create_dir(root.join("dir.part")).unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    assert!(rename_no_replace(&root.join("dir.part"), &root.join("dir")).is_err());
    std::fs::remove_dir(root.join("dir")).unwrap();
    rename_no_replace(&root.join("dir.part"), &root.join("dir")).unwrap();
    assert!(root.join("dir").is_dir() && !root.join("dir.part").exists());
}

#[test]
fn folder_round_trip_verifies_with_blake3() {
    let root = dir("round-trip");
    let src = root.join("src");
    game(&src);
    let out = root.join("out");
    let before = snapshot(&src);
    let (events, result) = run(request(&src, Format::Folder, &out));
    let report = result.unwrap();
    assert_eq!(
        snapshot(&src),
        before,
        "the source, junk included, is never touched"
    );
    assert_eq!(report.output, dunce::canonicalize(&out).unwrap());
    assert_eq!(report.files, 4);
    // Fast by default; files this small are all compared whole (the empty one by its size).
    let v = report.verify;
    assert_eq!(v.mode, VerifyMode::Fast);
    assert_eq!((v.samples, v.checked_bytes), (3, v.total_bytes), "{v:?}");
    assert!(v.seed < 1 << 53);
    let fast = format!(
        "blake3: 3 samples, {0} of {0} bytes in 4 files, match",
        v.total_bytes
    );
    assert!(
        report.checks.iter().any(|c| c.starts_with(&fast)),
        "{:?}",
        report.checks
    );
    let logged = |events: &[Event], want: &str| {
        events
            .iter()
            .any(|e| matches!(e, Event::Log { line, .. } if line.starts_with(want)))
    };
    assert!(logged(
        &events,
        "verify: fast, 9.0 MiB of 9.0 MiB in 3 samples (seed "
    ));
    assert!(report.checks.iter().any(|c| c == "empty dirs: 1 match"));
    assert_eq!(tree_bytes(&src), tree_bytes(&out));
    assert!(out.join("data/empty").is_dir());
    assert!(!out.join(".DS_Store").exists());
    assert!(stale_parts(&root).is_empty());
    let stages: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Progress { stage, .. } => Some(stage.as_str()),
            _ => None,
        })
        .collect();
    for stage in ["scan", "preflight", "write", "verify", "finalize"] {
        assert!(stages.contains(&stage), "no {stage} progress");
    }
    // Every event serializes the way the app forwards it.
    let line = serde_json::to_string(events.last().unwrap()).unwrap();
    assert!(line.starts_with(r#"{"kind":"done""#), "{line}");
    assert!(
        line.contains(r#""verify":{"mode":"fast","checked_bytes":"#),
        "{line}"
    );

    let out = root.join("out-full");
    let (events, result) = run(ConvertRequest {
        full_verify: true,
        kraken_level: KrakenLevel::Fast,
        ffpfsc_level: 6,
        ..request(&src, Format::Folder, &out)
    });
    let report = result.unwrap();
    assert!(
        report
            .checks
            .iter()
            .any(|c| c == "blake3: 4 files match the source")
    );
    let v = report.verify;
    assert_eq!(v.mode, VerifyMode::Full);
    assert_eq!((v.checked_bytes, v.samples, v.seed), (v.total_bytes, 0, 0));
    assert!(logged(&events, "verify: full, 9.0 MiB of 9.0 MiB"));
    assert_eq!(tree_bytes(&src), tree_bytes(&out));
}

#[test]
fn preflight_lists_every_problem() {
    let root = dir("preflight");
    let src = root.join("src");
    write(&src, "readme.txt", b"no game here");
    let (_, result) = run(request(&src, Format::Folder, &src.join("inside")));
    let err = result.unwrap_err();
    for want in [
        "eboot.bin is missing",
        "sce_sys/param.json is missing",
        "inside the source folder",
    ] {
        assert!(err.contains(want), "{want:?} not in:\n{err}");
    }

    game(&src);
    write(&root, "taken", b"");
    let (_, result) = run(request(&src, Format::Folder, &root.join("taken")));
    assert!(result.unwrap_err().contains("already exists"));
}

/// Progress stages in the order they first appear.
fn stages(events: &[Event]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for e in events {
        if let Event::Progress { stage, .. } = e
            && !out.contains(&stage.as_str())
        {
            out.push(stage);
        }
    }
    out
}

/// A game of many tiny files: the builder's per-file passes (check, plan) take a small share
/// of the bar, never most of it before compress begins.
#[test]
fn pkg_of_tiny_files_keeps_the_bar_for_compress() {
    let root = dir("pkg-tiny");
    let src = root.join("src");
    write(&src, "eboot.bin", b"\x7fELF fake eboot");
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    for i in 0..600 {
        write(&src, &format!("data/tiny/{i:03}.bin"), &[i as u8; 32]);
    }
    let (events, result) = run(request(&src, Format::Pkg, &root.join("tiny.pkg")));
    result.unwrap();
    let bar = one_bar(&events);
    let planned = bar.iter().rposition(|&(s, _, _)| s == "plan").unwrap();
    let most = bar[..=planned]
        .iter()
        .filter(|&&(_, _, t)| t > 0)
        .map(|&(_, d, t)| d as f64 / t as f64)
        .fold(0.0, f64::max);
    assert!(most < 0.10, "{most} after plan: {bar:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn folder_to_pkg_verifies_and_publishes() {
    let root = dir("pkg");
    let src = root.join("src");
    game(&src);
    // Artwork the package carries only as container entries, which the reader merges back.
    write(&src, "sce_sys/icon0.png", &[0x89; 300]);
    write(&src, "sce_sys/pic0.png", &[0x50; 200]);
    let pkg = root.join("PPSA01234.pkg");
    let before = snapshot(&src);
    let (events, result) = run(request(&src, Format::Pkg, &pkg));
    let report = result.unwrap();
    assert_eq!(snapshot(&src), before, "the source is never touched");
    // The builder's per-file reads move the one bar in check and plan.
    let bar = one_bar(&events);
    for stage in ["check", "plan"] {
        let done: Vec<u64> = bar
            .iter()
            .filter(|&&(s, _, _)| s == stage)
            .map(|&(_, d, _)| d)
            .collect();
        assert!(done.first() < done.last(), "{stage}: {bar:?}");
    }
    assert_eq!(report.output, dunce::canonicalize(&pkg).unwrap());
    assert_eq!(report.bytes, std::fs::metadata(&pkg).unwrap().len());
    let checks = report.checks.join("\n");
    // The builder's own read-back, through the job's handle.
    assert!(
        report
            .checks
            .iter()
            .filter(|c| c.starts_with("fpkg verify: "))
            .count()
            > 5,
        "{checks}"
    );
    assert!(checks.contains("source hashes: "), "{checks}");
    assert!(
        checks.contains("console: installing needs kstuff + fpkg-enable + ppr-patch"),
        "{checks}"
    );
    // Our own read-back through `FpkgSource`: param.json, eboot, two data files, the
    // generated keystone and the two container-only images.
    assert_eq!(report.files, 7, "{checks}");
    // Fast by default, the builder's own sweep sampled with the same seed.
    for want in [
        "in 7 files, match the source (fast, seed",
        "imagedigs entry 0 of 7 failed (a sample of 7, seed",
        "empty dirs: 1 match",
        "(2 as container entries)",
        "sce_sys/keystone (generated)",
    ] {
        assert!(checks.contains(want), "{want:?} not in:\n{checks}");
    }
    assert_eq!(
        stages(&events),
        [
            "scan",
            "preflight",
            "check",
            "plan",
            "compress",
            "write",
            "verify",
            "finalize"
        ]
    );
    let logs: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Log { line, .. } => Some(line.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        logs.iter().any(|l| l.starts_with("readiness [")),
        "{logs:?}"
    );
    assert!(stale_parts(&root).is_empty());
    let found = inspect(&pkg).unwrap();
    assert_eq!(found.kind, "pkg");
    assert_eq!(found.title_id.as_deref(), Some("PPSA01234"));
    assert_eq!(
        found.content_id.as_deref(),
        Some("UP0000-PPSA01234_00-TESTTESTTESTTEST")
    );
    assert_eq!(found.files.len(), 7);
    assert_eq!(found.empty_dirs, ["data/empty"]);
    assert!(found.details.iter().any(|d| d.starts_with("FIH: debug")));
    assert!(found.details.len() > 1, "{:?}", found.details);
}

#[test]
fn pkg_to_folder_round_trip() {
    let root = dir("pkg-back");
    let src = root.join("src");
    game(&src);
    let pkg = root.join("PPSA01234.pkg");
    let report = run(request(&src, Format::Pkg, &pkg)).1.unwrap();
    assert!(
        report.checks.iter().any(|c| c.starts_with("blake3: ")),
        "{:?}",
        report.checks
    );
    let back = root.join("back");
    run(request(&pkg, Format::Folder, &back)).1.unwrap();
    // The build rewrites param.json and adds a keystone; every other file is the source's.
    let got = tree_bytes(&back);
    let want: Vec<_> = tree_bytes(&src)
        .into_iter()
        .filter(|(p, _)| p != "sce_sys/param.json")
        .collect();
    for file in &want {
        assert!(got.contains(file), "{} differs or is missing", file.0);
    }
    assert!(got.iter().any(|(p, _)| p == "sce_sys/param.json"));
    assert!(back.join("data/empty").is_dir());
    // Straight into an image too, with no folder in between.
    let report = run(request(&pkg, Format::Exfat, &root.join("PPSA01234.exfat")))
        .1
        .unwrap();
    assert!(report.checks.iter().any(|c| c == "empty dirs: 1 match"));
    assert!(stale_parts(&root).is_empty());
}

/// `pkg` re-wrapped as a patch package (`\x7FLIH`) carrying every outer block: a 64 KiB header,
/// the FIH block, the blocks, the container (see `ps5-dump-forge-fpkg`'s `lih.rs`).
fn patch_of(pkg: &[u8]) -> Vec<u8> {
    const B: usize = 0x10000;
    let le = |at: usize| u64::from_le_bytes(pkg[at..at + 8].try_into().unwrap()) as usize;
    let (pfs_offset, pfs_size, cnt_offset) = (le(0x10), le(0x18), le(0x58));
    let cnt_end = if cnt_offset < pfs_offset {
        pfs_offset
    } else {
        pkg.len()
    };
    let mut out = vec![0u8; B];
    out.extend_from_slice(&pkg[..B]);
    out.extend_from_slice(&pkg[pfs_offset..pfs_offset + pfs_size]);
    let cnt_at = out.len();
    out.extend_from_slice(&pkg[cnt_offset..cnt_end]);
    let len = out.len();
    out[..4].copy_from_slice(b"\x7FLIH");
    for (at, v) in [
        (0x18, len),
        (0x20, B),
        (0x28, B),
        (0x30, cnt_at),
        (0x38, cnt_end - cnt_offset),
    ] {
        out[at..at + 8].copy_from_slice(&(v as u64).to_le_bytes());
    }
    out
}

/// A patch package is browsed and extracted from only: inspect flags it, Extract copies its
/// files, a conversion and an edit refuse it, and the Library leaves it out.
#[test]
fn a_patch_package_is_extracted_from_only() {
    let root = dir("patch-pkg");
    let src = root.join("src");
    game(&src);
    let pkg = root.join("PPSA01234.pkg");
    run(request(&src, Format::Pkg, &pkg)).1.unwrap();
    let patch = root.join("patches/PPSA01234-patch.pkg");
    write(
        &root,
        "patches/PPSA01234-patch.pkg",
        &patch_of(&std::fs::read(&pkg).unwrap()),
    );

    let ins = inspect(&patch).unwrap();
    assert!(ins.patch && ins.lz4.is_none(), "{:?}", ins.details);
    assert!(
        !ins.additional_content && ins.findings.is_empty(),
        "{:?}",
        ins.findings
    );
    assert_eq!(ins.title_id.as_deref(), Some("PPSA01234"));
    assert!(!inspect(&pkg).unwrap().patch);

    let out = root.join("out");
    std::fs::create_dir(&out).unwrap();
    run(ConvertRequest {
        extract: vec!["data".into()],
        ..request(&patch, Format::Folder, &out)
    })
    .1
    .unwrap();
    assert!(
        std::fs::read(out.join("data/big.bin")).unwrap()
            == std::fs::read(src.join("data/big.bin")).unwrap()
    );

    // Refused before the patch is read: no LZ4 step logs anything first.
    let (events, result) = run(request(&patch, Format::Exfat, &root.join("x.exfat")));
    let err = result.unwrap_err();
    assert!(
        !events.iter().any(|e| matches!(e, Event::Log { .. })),
        "{events:?}"
    );
    assert!(err.contains("a patch package (LIH)"), "{err}");
    assert!(!root.join("x.exfat").exists());
    let check = edit_check(&ConvertRequest {
        edits: vec![ps5_dump_forge_core::Edit::Delete {
            path: "data/zero.bin".into(),
        }],
        ..request(&patch, Format::Pkg, &patch)
    });
    for findings in [&check.in_place, &check.rebuild] {
        assert!(
            findings.iter().any(|f| f.contains("patch package")),
            "{check:?}"
        );
    }

    let scan = ps5_dump_forge_core::scan_library_folder(&root).unwrap();
    let listed: Vec<&Path> = scan.entries.iter().map(|e| e.path.as_path()).collect();
    assert!(
        listed.contains(&pkg.as_path()) && !listed.contains(&patch.as_path()),
        "{listed:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Cancels `format`'s job at the first progress of `at`, while its `.part` exists.
fn cancel_removes_only_this_jobs_part(format: Format, out: &str, at: &'static str) {
    cancel_removes_only_this_jobs_part_of(game, format, out, at);
}

/// The same for the game `make` writes.
fn cancel_removes_only_this_jobs_part_of(
    make: fn(&Path),
    format: Format,
    out: &str,
    at: &'static str,
) {
    cancel_at(make, |src, out| request(src, format, out), out, at, true);
}

/// Cancels the job `req` makes (from the source and output paths) at the first progress of
/// `at`; `part` says whether its `.part` exists by then. Nothing of it is left.
fn cancel_at(
    make: fn(&Path),
    req: impl FnOnce(&Path, &Path) -> ConvertRequest,
    out: &str,
    at: &'static str,
    part: bool,
) {
    let root = dir(&format!("cancel-{out}"));
    let src = root.join("src");
    make(&src);
    write(&root, "other.7-1.part", b"another job's leftover");
    let out = root.join(out);

    // The emit callback runs on the worker: holding it on the first progress of `at` lets
    // the test cancel at a known point.
    let (writing_tx, writing_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Mutex::new((Some(writing_tx), go_rx, done_tx));
    let jobs = Arc::new(Jobs::new(move |e| {
        let mut s = state.lock().unwrap();
        match e {
            Event::Progress { ref stage, .. } if stage == at => {
                if let Some(tx) = s.0.take() {
                    tx.send(()).unwrap();
                    s.1.recv().unwrap();
                }
            }
            Event::Done { result, .. } => s.2.send(result).unwrap(),
            _ => {}
        }
    }));
    let job = jobs.start(req(&src, &out));
    writing_rx.recv().unwrap();
    let parts = stale_parts(&root);
    assert_eq!(parts.len(), if part { 2 } else { 1 }, "{parts:?}");
    jobs.cancel(job);
    go_tx.send(()).unwrap();
    assert_eq!(done_rx.recv().unwrap().unwrap_err(), "cancelled");
    assert_eq!(stale_parts(&root), [root.join("other.7-1.part")]);
    assert!(!out.exists());
    jobs.cancel_all_and_wait();
    let _ = std::fs::remove_dir_all(root);
}

/// A `.ffpfs` whose flat path table is damaged after writing (one byte of its first hash, at
/// block 3) fails verification: the job errs and leaves no `.part` and no output.
#[test]
fn ffpfs_with_a_damaged_path_table_fails_verification() {
    let root = dir("ffpfs-fpt");
    let src = root.join("src");
    game(&src);
    let out = root.join("out.ffpfs");

    // Held on the first verify progress, after the image is written and synced.
    let (verify_tx, verify_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Mutex::new((Some(verify_tx), go_rx, done_tx));
    let jobs = Jobs::new(move |e| {
        let mut s = state.lock().unwrap();
        match e {
            Event::Progress { ref stage, .. } if stage == "verify" => {
                if let Some(tx) = s.0.take() {
                    tx.send(()).unwrap();
                    s.1.recv().unwrap();
                }
            }
            Event::Done { result, .. } => s.2.send(result).unwrap(),
            _ => {}
        }
    });
    jobs.start(request(&src, Format::Ffpfs, &out));
    verify_rx.recv().unwrap();
    let [part] = &stale_parts(&root)[..] else {
        panic!("one .part while verifying");
    };
    let mut img = std::fs::read(part).unwrap();
    img[0x30000] ^= 0xFF;
    std::fs::write(part, img).unwrap();
    go_tx.send(()).unwrap();
    let err = done_rx.recv().unwrap().unwrap_err();
    assert!(
        err.contains("the flat path table does not match the directory tree"),
        "{err}"
    );
    assert!(stale_parts(&root).is_empty());
    assert!(!out.exists());
    jobs.cancel_all_and_wait();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancel_ffpfsc_removes_only_this_jobs_part() {
    // Mid-stream: the inner exFAT writer is held at its first file data.
    cancel_removes_only_this_jobs_part(Format::Ffpfsc, "out.ffpfsc", "write");
}

/// `src` to `format` (with `inner` for `.ffpfsc`) and back to a folder, both verified; the
/// image is inspected in between. Returns the test's folder, its source and the image.
fn image_round_trip(
    format: Format,
    inner: Option<Format>,
    ext: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    let root = dir(&format!("image-{ext}-{inner:?}"));
    let src = root.join("src");
    game(&src);
    // Empty dirs the image readers must report back: a nested one, and one holding junk only.
    std::fs::create_dir_all(src.join("deep/a/b")).unwrap();
    write(&src, "only-junk/.DS_Store", b"junk");
    // Over 16 MiB, so fast verification compares it by 8 MiB slices; the last one is short.
    let large: Vec<u8> = (0..24 * 1024 * 1024 + 5)
        .map(|i| (i * 7 % 253) as u8)
        .collect();
    write(&src, "data/large.bin", &large);
    let image = root.join(format!("PPSA01234.{ext}"));
    let before = snapshot(&src);
    let report = run(ConvertRequest {
        inner,
        ..request(&src, format, &image)
    })
    .1
    .unwrap();
    assert_eq!(
        snapshot(&src),
        before,
        "the source, junk included, is never touched"
    );
    // Three small files whole, all four slices of the large one (its first, last, one
    // interior and one random).
    let v = report.verify;
    assert_eq!(v.mode, VerifyMode::Fast);
    assert_eq!((v.samples, v.checked_bytes), (7, v.total_bytes), "{v:?}");
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.starts_with("blake3: 7 samples")),
        "{:?}",
        report.checks
    );
    assert!(
        report.checks.iter().any(|c| c == "empty dirs: 3 match"),
        "{:?}",
        report.checks
    );
    if format == Format::Ffpkg {
        for want in [
            "geometry: UFS2 at byte 0",
            "last group: ",
            "root directory: mode 0777",
        ] {
            assert!(
                report.checks.iter().any(|c| c.starts_with(want)),
                "{want:?} not in {:?}",
                report.checks
            );
        }
    }
    let checks = report.checks.join("\n");
    match (format, inner) {
        (Format::Ffpfs, _) => assert!(checks.contains("geometry: PFS v2 at byte 0"), "{checks}"),
        (Format::Ffpfsc, inner) => {
            let (ext, geometry) = match inner.unwrap_or(Format::Ffpkg) {
                Format::Exfat => ("exfat", "geometry: raw volume at byte 0"),
                Format::Ffpkg => ("ffpkg", "geometry: UFS2 at byte 0"),
                _ => ("ffpfs", "geometry: PFS v2 at byte 0"),
            };
            let container = format!("container: PPSA01234.{ext}, ");
            for want in ["geometry: PFS v2 at byte 0", &container, geometry] {
                assert!(checks.contains(want), "{want:?} not in:\n{checks}");
            }
            // The 9 MiB file repeats every 251 bytes: most blocks compress.
            assert!(report.bytes < 9 * 1024 * 1024, "{checks}");
        }
        _ => {}
    }
    let found = inspect(&image).unwrap();
    assert_eq!(found.kind, ext);
    // The maker's mark: in an `.exfat`/`.ffpkg`, or the one inside a `.ffpfsc`; never PFS.
    let marked = match format {
        Format::Exfat | Format::Ffpkg => true,
        Format::Ffpfsc => inner != Some(Format::Ffpfs),
        _ => false,
    };
    let mark = marked.then_some(env!("CARGO_PKG_VERSION"));
    assert_eq!(found.forge_version.as_deref(), mark, "{format:?} {inner:?}");
    assert_eq!(found.title_id.as_deref(), Some("PPSA01234"));
    assert_eq!(found.empty_dirs, ["data/empty", "deep/a/b", "only-junk"]);
    // An `.exfat` notes its SMP geometry; nothing else is found in what we wrote.
    let notes = |f: &String| f.starts_with("already 512-byte sectors");
    assert!(found.findings.iter().all(notes), "{:?}", found.findings);
    if matches!(format, Format::Ffpfs | Format::Ffpfsc) {
        let details = found.details.join("\n");
        assert!(
            details.contains("PFS: version 2, 64 KiB blocks"),
            "{details}"
        );
        if format == Format::Ffpfsc {
            assert!(details.contains("container: PPSA01234."), "{details}");
        }
    }
    let back = root.join("back");
    let report = run(request(&image, Format::Folder, &back)).1.unwrap();
    assert!(report.checks.iter().any(|c| c == "empty dirs: 3 match"));
    assert_eq!(
        (report.verify.mode, report.verify.samples),
        (VerifyMode::Fast, 7)
    );
    assert_eq!(tree_bytes(&src), tree_bytes(&back));
    assert!(back.join("deep/a/b").is_dir() && back.join("only-junk").is_dir());
    assert!(stale_parts(&root).is_empty());
    (root, src, image)
}

#[test]
fn ffpkg_round_trip() {
    let (root, ..) = image_round_trip(Format::Ffpkg, None, "ffpkg");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ffpfsc_round_trip_with_each_inner_image() {
    for inner in [None, Some(Format::Ffpkg), Some(Format::Ffpfs)] {
        let (root, src, image) = image_round_trip(Format::Ffpfsc, inner, "ffpfsc");
        if inner.is_none() {
            // `.ffpfsc` (exFAT inside) straight to `.exfat`.
            let exfat = root.join("again.exfat");
            let report = run(request(&image, Format::Exfat, &exfat)).1.unwrap();
            assert!(report.checks.iter().any(|c| c == "empty dirs: 3 match"));
            let back = root.join("back-exfat");
            run(request(&exfat, Format::Folder, &back)).1.unwrap();
            assert_eq!(tree_bytes(&src), tree_bytes(&back));
        }
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn pfs_targets_refuse_what_smp_cannot_mount() {
    let root = dir("pfs-refuse");
    let src = root.join("src");
    game(&src);
    write(&src, "data/café.bin", b"x");
    write(&src, "data/naïve/x.bin", b"x");
    // PFS names are ASCII only: every offender is listed, for `.ffpfs` and a `.ffpfs` inside
    // a `.ffpfsc`; an exFAT inside takes them.
    let ffpfsc = |inner| ConvertRequest {
        inner,
        ..request(&src, Format::Ffpfsc, &root.join("a.ffpfsc"))
    };
    for req in [
        request(&src, Format::Ffpfs, &root.join("a.ffpfs")),
        ffpfsc(Some(Format::Ffpfs)),
    ] {
        let err = run(req).1.unwrap_err();
        for want in [
            "PFS layout: ",
            "data/café.bin: non-ASCII name",
            "data/naïve: non-ASCII name",
        ] {
            assert!(err.contains(want), "{want:?} not in:\n{err}");
        }
    }
    let found = inspect(&src).unwrap();
    assert_eq!(
        found.findings,
        [
            "name not ASCII (refused by .ffpfs): data/café.bin",
            "name not ASCII (refused by .ffpfs): data/naïve/x.bin"
        ]
    );
    // Only an `.exfat`, `.ffpkg` or `.ffpfs` goes inside.
    for (inner, what) in [
        (Format::Pkg, ".pkg"),
        (Format::Folder, "a folder"),
        (Format::Ffpfsc, ".ffpfsc"),
    ] {
        let err = run(ffpfsc(Some(inner))).1.unwrap_err();
        let want = format!("a .ffpfsc holds an .exfat, .ffpkg or .ffpfs image, not {what}");
        assert!(err.contains(&want), "{want:?} not in:\n{err}");
    }
    // An explicit name over ShadowMountPlus's limit (extension excluded) is refused: 63
    // bytes for an image, 58 for a .ffpfsc.
    for (format, ext, max) in [
        (Format::Exfat, "exfat", 63),
        (Format::Ffpkg, "ffpkg", 63),
        (Format::Ffpfs, "ffpfs", 63),
        (Format::Ffpfsc, "ffpfsc", 58),
    ] {
        let long = format!("{}.{ext}", "n".repeat(max + 1));
        let err = run(request(&src, format, &root.join(&long))).1.unwrap_err();
        let want = format!(
            "{long} is {} bytes long without its .{ext}: ShadowMountPlus fails to mount a \
             .{ext} whose name is over {max} bytes",
            max + 1
        );
        assert!(err.contains(&want), "{want:?} not in:\n{err}");
    }
    // A .pkg (not image-mounted) keeps to the same 63 bytes.
    let long = format!("{}.pkg", "n".repeat(64));
    let err = run(request(&src, Format::Pkg, &root.join(&long)))
        .1
        .unwrap_err();
    let want = format!("{long} is 64 bytes long without its .pkg: output names are at most 63");
    assert!(err.contains(&want), "{want:?} not in:\n{err}");
    std::fs::remove_file(src.join("data/café.bin")).unwrap();
    std::fs::remove_dir_all(src.join("data/naïve")).unwrap();
    let fits = format!("{}.ffpfs", "n".repeat(63));
    run(request(&src, Format::Ffpfs, &root.join(&fits)))
        .1
        .unwrap();
    assert!(stale_parts(&root).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn too_deep_for_the_image_readers_is_refused() {
    let root = dir("deep");
    let src = root.join("src");
    game(&src);
    let deep = |n: usize| vec!["d"; n].join("/");
    // 32 directory levels read back fine; 33 do not.
    write(&src, &format!("{}/ok.bin", deep(32)), b"x");
    let report = run(request(&src, Format::Exfat, &root.join("ok.exfat")))
        .1
        .unwrap();
    assert!(report.checks.iter().any(|c| c.starts_with("blake3:")));
    write(&src, &format!("{}/deep.bin", deep(33)), b"x");
    for (format, name) in [
        (Format::Exfat, "a.exfat"),
        (Format::Ffpkg, "a.ffpkg"),
        (Format::Ffpfs, "a.ffpfs"),
        (Format::Ffpfsc, "a.ffpfsc"),
    ] {
        let err = run(request(&src, format, &root.join(name))).1.unwrap_err();
        assert!(
            err.contains(&format!(
                "nested deeper than 32 directories (image readers' limit): {}/deep.bin",
                deep(33)
            )),
            "{err}"
        );
    }
    // A folder has no such limit.
    run(request(&src, Format::Folder, &root.join("out")))
        .1
        .unwrap();
    assert!(stale_parts(&root).is_empty());
}

/// A minimal ELF whose process param carries the PS5 SDK `sdk` (as `sdk.rs` reads it).
fn eboot(sdk: u32) -> Vec<u8> {
    let mut f = vec![0u8; 0x300];
    f[..4].copy_from_slice(b"\x7fELF");
    f[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes()); // e_phoff
    f[0x36..0x38].copy_from_slice(&0x38u16.to_le_bytes()); // e_phentsize
    f[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
    f[0x40..0x44].copy_from_slice(&0x6100_0001u32.to_le_bytes()); // PT_SCE_PROCPARAM
    f[0x48..0x50].copy_from_slice(&0x200u64.to_le_bytes());
    f[0x60..0x68].copy_from_slice(&0x40u64.to_le_bytes());
    f[0x200..0x204].copy_from_slice(&0x4942_524Fu32.to_le_bytes());
    f[0x20C..0x210].copy_from_slice(&sdk.to_le_bytes());
    f
}

#[test]
fn remove_backport_keeps_emulators_and_refuses_a_lowered_sdk() {
    let root = dir("remove-backport");
    let src = root.join("src");
    write(&src, "eboot.bin", &eboot(0x1200_0038));
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","sdkVersion":"0x1200000000000000"}"#,
    );
    write(&src, "fakelib/libSceAgc.sprx", b"\x7fELF a system library");
    write(
        &src,
        "fakelib/libSceAmpr.sprx",
        b"reads /app0/ampr_emu.index",
    );
    write(&src, "fakelib2/libSceGnm.sprx", b"\x7fELF another");
    write(&src, "ampr_emu.index", b"index");

    let found = inspect(&src).unwrap();
    assert_eq!(
        found.backport,
        ["fakelib/libSceAgc.sprx", "fakelib2/libSceGnm.sprx"]
    );
    assert_eq!(found.emulators.len(), 1);
    assert_eq!(found.emulators[0].name, "AMPR");
    assert_eq!(found.backport_blocked, None);

    let before = snapshot(&src);
    let out = root.join("out");
    let report = run(ConvertRequest {
        remove_backport: true,
        ..request(&src, Format::Folder, &out)
    })
    .1
    .unwrap();
    assert_eq!(snapshot(&src), before, "the source is never touched");
    let paths: Vec<String> = tree_bytes(&out).into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        paths,
        [
            "ampr_emu.index",
            "eboot.bin",
            "fakelib/libSceAmpr.sprx",
            "sce_sys/param.json"
        ]
    );
    assert!(
        !out.join("fakelib2").exists(),
        "an emptied fakelib2 goes too"
    );
    assert!(report.checks.iter().any(|c| c.starts_with("blake3:")));

    // eboot.bin lowered to 9.00 under a 12.00 param: refused, nothing written.
    write(&src, "eboot.bin", &eboot(0x0900_0040));
    assert!(
        inspect(&src)
            .unwrap()
            .backport_blocked
            .unwrap()
            .contains("lower than param.json's sdkVersion 12.00")
    );
    let err = run(ConvertRequest {
        remove_backport: true,
        ..request(&src, Format::Folder, &root.join("refused"))
    })
    .1
    .unwrap_err();
    assert!(
        err.contains("eboot.bin's SDK 9.00 (0x09000040) is lower"),
        "{err}"
    );
    assert!(!root.join("refused").exists());
}

// ---- LZ4 asset packs ----------------------------------------------------------------------------

use ps5_dump_forge_core::Lz4Mode;
use ps5_dump_forge_lz4::runtime::RELEASE;

/// Bytes that do not compress.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// [`game`] as a libSceAmpr title, with assets in a subfolder (the built-in guess packs
/// those, and `data/big.bin`; root files, `sce_sys` and empty files stay loose).
fn ampr_game(root: &Path) {
    game(root);
    write(
        root,
        "eboot.bin",
        b"\x7fELF fake eboot importing libSceAmpr.sprx",
    );
    let text: Vec<u8> = (0..300_000)
        .map(|i| b"compressible text "[i % 18])
        .collect();
    write(root, "data/assets/level1.bin", &text);
    write(root, "data/assets/noise.bin", &noise(200_000, 1));
    write(root, "readme.txt", b"stays loose at the root");
}

fn lz4_request(
    source: &Path,
    format: Format,
    output: &Path,
    mode: Option<Lz4Mode>,
) -> ConvertRequest {
    ConvertRequest {
        lz4: mode,
        ..request(source, format, output)
    }
}

/// The paths of `ampr_emu.index` in `root`, in record order.
fn index_paths(root: &Path) -> Vec<(String, u64)> {
    let bytes = std::fs::read(root.join("ampr_emu.index")).unwrap();
    ps5_dump_forge_lz4::index::read_index(&bytes)
        .unwrap()
        .records
}

fn logged(events: &[Event], want: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::Log { line, .. } if line.contains(want)))
}

/// `tree_bytes` without the runtime and the index Forge adds.
fn assets(root: &Path) -> Vec<(String, Vec<u8>)> {
    tree_bytes(root)
        .into_iter()
        .filter(|(p, _)| p != "ampr_emu.index" && p != "fakelib/libSceAmpr.sprx")
        .collect()
}

fn zstd_request(source: &Path, output: &Path) -> ConvertRequest {
    ConvertRequest {
        zstd_pack: true,
        ..request(source, Format::Folder, output)
    }
}

/// A zstd-packed folder read through its packs, without `ampr_emu.index` (its timestamp is
/// the job's).
fn zstd_logical(root: &Path) -> Vec<(String, Vec<u8>)> {
    let tree = ScannedFolder::scan(root, &AtomicBool::new(false)).unwrap();
    let mut tree = ps5_dump_forge_zstd::reader::unpack(Box::new(tree)).unwrap();
    let files = tree.files().to_vec();
    files
        .into_iter()
        .filter(|f| f.path != "ampr_emu.index")
        .map(|f| (f.path.clone(), tree.read(&f.path).unwrap()))
        .collect()
}

#[test]
fn zstd_pack_refusals_and_packed_sources() {
    let root = dir("zstd-pack");
    let src = root.join("src");
    ampr_game(&src);
    let refused = |req: ConvertRequest, want: &str| {
        let err = run(req).1.unwrap_err();
        assert!(err.contains(want), "{want}: {err}");
    };
    refused(
        ConvertRequest {
            zstd_pack: true,
            ..request(&src, Format::Pkg, &root.join("a.pkg"))
        },
        "zstd packs go into a folder or an exfat, ffpkg, ffpfs or ffpfsc image, not",
    );
    refused(
        ConvertRequest {
            lz4: Some(Lz4Mode::Unpack),
            ..zstd_request(&src, &root.join("b"))
        },
        "zstd packing takes no LZ4 option (lz4)",
    );
    refused(
        ConvertRequest {
            zstd_level: 23,
            ..zstd_request(&src, &root.join("c"))
        },
        "the zstd level is 1 through 22, not 23",
    );
    let plain = root.join("plain");
    game(&plain);
    refused(
        zstd_request(&plain, &root.join("d")),
        "does not import libSceAmpr",
    );

    let packed = root.join("packed");
    let (events, result) = run(zstd_request(&src, &packed));
    result.unwrap();
    assert!(logged(&events, "wrote zstd packs"), "{events:?}");
    assert!(!logged(&events, "not read whole"), "{events:?}");
    assert!(!packed.join("data/assets/level1.bin").exists());
    assert!(packed.join("data/empty").is_dir());
    // A plain conversion keeps the packs, the zstd runtime and the index byte for byte.
    let copy = root.join("copy");
    let (events, result) = run(request(&packed, Format::Folder, &copy));
    result.unwrap();
    assert!(
        logged(&events, "keeping ampr_emu.index as it is"),
        "{events:?}"
    );
    assert_eq!(tree_bytes(&copy), tree_bytes(&packed));
    // Packed again, the packs are decoded first: the same logical files in new packs.
    let again = root.join("again");
    let (events, result) = run(zstd_request(&packed, &again));
    result.unwrap();
    assert!(logged(&events, "with zstd packs unpacked"), "{events:?}");
    assert!(logged(
        &events,
        "the zstd runtime at fakelib/libSceAmpr.sprx is written again"
    ));
    assert_eq!(zstd_logical(&again), zstd_logical(&packed));
    let mut source = tree_bytes(&src);
    source.push((
        "fakelib/libSceAmpr.sprx".into(),
        ps5_dump_forge_lz4::runtime::ZSTD.to_vec(),
    ));
    source.sort();
    let mut logical = zstd_logical(&packed);
    logical.sort();
    assert_eq!(logical, source);
}

/// zstd packs straight into a `.ffpkg`: measured, written with no staging folder, verified as
/// written and through the packs; the image read back through the zstd reader gives the source's
/// files and the zstd runtime.
#[test]
fn zstd_pack_straight_into_an_ffpkg() {
    let root = dir("zstd-pack-image");
    let src = root.join("src");
    ampr_game(&src);
    let image = root.join("g.ffpkg");
    let (events, result) = run(ConvertRequest {
        zstd_pack: true,
        ..request(&src, Format::Ffpkg, &image)
    });
    let report = result.unwrap();
    one_bar(&events);
    assert_eq!(
        stages(&events),
        [
            "scan",
            "preflight",
            "measure",
            "write",
            "verify",
            "finalize"
        ]
    );
    assert!(logged(&events, "measured zstd packs"), "{events:?}");
    assert!(!logged(&events, "not read whole"), "{events:?}");
    assert!(report.checks.iter().any(|c| c.starts_with("zstd packs: ")));
    assert!(report.checks.iter().any(|c| c.starts_with("as written, ")));
    let file = std::fs::File::open(&image).unwrap();
    let tree = ps5upload_fpkg::ufs2_source::Ufs2Source::from_reader(Box::new(file), "ffpkg".into())
        .unwrap();
    assert!(tree.files().iter().any(|f| f.path == "ampr_zstd-000.pak"));
    let mut tree = ps5_dump_forge_zstd::reader::unpack(Box::new(tree)).unwrap();
    assert!(!tree.index().files.is_empty());
    let files = tree.files().to_vec();
    let mut logical: Vec<(String, Vec<u8>)> = (files.into_iter())
        .filter(|f| f.path != "ampr_emu.index")
        .map(|f| (f.path.clone(), tree.read(&f.path).unwrap()))
        .collect();
    logical.sort();
    let mut source = tree_bytes(&src);
    source.push((
        "fakelib/libSceAmpr.sprx".into(),
        ps5_dump_forge_lz4::runtime::ZSTD.to_vec(),
    ));
    source.sort();
    assert_eq!(logical, source);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn zstd_pack_from_lz4_packed_folder() {
    let root = dir("zstd-from-lz4");
    let src = root.join("src");
    ampr_game(&src);
    let lz4 = root.join("lz4");
    run(request(&src, Format::Lz4, &lz4)).1.unwrap();
    assert!(lz4.join("ampr_assets-000.pak").is_file());

    let forged = root.join("forged");
    let (events, result) = run(zstd_request(&lz4, &forged));
    result.unwrap();
    assert!(
        logged(&events, "wrote zstd packs: 3 files packed"),
        "{events:?}"
    );
    // Packed as LZ4 packed them: its three assets in the volume, its loose files on disk.
    for loose in [
        "readme.txt",
        "data/zero.bin",
        "eboot.bin",
        "sce_sys/param.json",
    ] {
        assert!(forged.join(loose).is_file(), "{loose}");
    }
    for packed in ["data/big.bin", "data/assets/noise.bin"] {
        assert!(!forged.join(packed).exists(), "{packed}");
    }
    // No LZ4 pack, manifest or LZ4 runtime survives; the logical files are the source's.
    for gone in [
        "ampr_assets.index",
        "ampr_assets.index.crc",
        "ampr_assets-000.pak",
        "data/assets/level1.bin",
    ] {
        assert!(!forged.join(gone).exists(), "{gone}");
    }
    let mut source = tree_bytes(&src);
    source.push((
        "fakelib/libSceAmpr.sprx".into(),
        ps5_dump_forge_lz4::runtime::ZSTD.to_vec(),
    ));
    source.sort();
    let mut logical = zstd_logical(&forged);
    logical.sort();
    assert_eq!(logical, source);
}

/// What a zstd pack holds apart from its random archive ID and its index's timestamp: the
/// decoded `ampr_zstd.index` (ID zeroed), each volume's payload, and every other file.
fn zstd_content(root: &Path) -> (ps5_dump_forge_zstd::Index, Vec<(String, Vec<u8>)>) {
    let tree = ScannedFolder::scan(root, &AtomicBool::new(false)).unwrap();
    let mut index = ps5_dump_forge_zstd::reader::unpack(Box::new(tree))
        .unwrap()
        .index()
        .clone();
    index.archive_id = [0; 16];
    let header = ps5_dump_forge_zstd::VOLUME_HEADER_SIZE;
    let files = tree_bytes(root)
        .into_iter()
        .filter(|(p, _)| p != "ampr_emu.index" && p != "ampr_zstd.index")
        .map(|(p, bytes)| {
            let pak = p.starts_with("ampr_zstd-") && p.ends_with(".pak");
            let bytes = if pak { bytes[header..].to_vec() } else { bytes };
            (p, bytes)
        })
        .collect();
    (index, files)
}

#[test]
fn zstd_pack_honours_compression_threads() {
    let root = dir("zstd-threads");
    let src = root.join("src");
    ampr_game(&src);
    let default = root.join("default");
    let (events, result) = run(zstd_request(&src, &default));
    result.unwrap();
    assert!(logged(&events, "MB/s on "), "{events:?}");
    let want = zstd_content(&default);
    assert!(!want.0.files.is_empty());
    for threads in [1, 2] {
        let out = root.join(format!("t{threads}"));
        let (events, result) = run(ConvertRequest {
            compression_threads: Some(threads),
            ..zstd_request(&src, &out)
        });
        result.unwrap();
        assert!(
            logged(&events, &format!("MB/s on {threads} threads")),
            "{threads}: {events:?}"
        );
        assert!(zstd_content(&out) == want, "{threads} threads");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// A zstd-packed folder converted plainly into an image or a `.pkg` keeps `ampr_emu.index`
/// (it lists the packed files), the packs and the zstd runtime byte for byte (the builder
/// keeps a zstd-packed source's index: vendor patch 0030). Inspect labels it.
#[test]
fn a_zstd_packed_source_keeps_its_index_in_an_image() {
    let root = dir("zstd-plain-image");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(zstd_request(&src, &packed)).1.unwrap();
    assert!(packed.join("ampr_zstd-000.pak").is_file());
    let image = root.join("PPSA01234.ffpkg");
    let (events, result) = run(request(&packed, Format::Ffpkg, &image));
    result.unwrap();
    assert!(
        logged(&events, "keeping ampr_emu.index as it is"),
        "{events:?}"
    );
    let back = root.join("back");
    run(request(&image, Format::Folder, &back)).1.unwrap();
    for same in [
        "ampr_emu.index",
        "ampr_zstd.index",
        "ampr_zstd-000.pak",
        "fakelib/libSceAmpr.sprx",
    ] {
        assert!(
            std::fs::read(back.join(same)).unwrap() == std::fs::read(packed.join(same)).unwrap(),
            "{same}"
        );
    }
    assert_eq!(zstd_logical(&back), zstd_logical(&packed));

    assert!(inspect(&packed).unwrap().zstd_packed);
    assert!(!inspect(&src).unwrap().zstd_packed);

    let pkg = root.join("game.pkg");
    run(request(&packed, Format::Pkg, &pkg)).1.unwrap();
    let from_pkg = root.join("from-pkg");
    run(request(&pkg, Format::Folder, &from_pkg)).1.unwrap();
    for same in [
        "ampr_emu.index",
        "ampr_zstd.index",
        "ampr_zstd-000.pak",
        "fakelib/libSceAmpr.sprx",
    ] {
        assert!(
            std::fs::read(from_pkg.join(same)).unwrap()
                == std::fs::read(packed.join(same)).unwrap(),
            "{same}"
        );
    }
    // The packed files read back through the packaged packs (the builder rewrites param.json
    // and adds its keystone beside them).
    let packed_only = |root: &Path| -> Vec<(String, Vec<u8>)> {
        (zstd_logical(root).into_iter())
            .filter(|(p, _)| !packed.join(p).exists())
            .collect()
    };
    let want = packed_only(&packed);
    let got: Vec<(String, Vec<u8>)> = (packed_only(&from_pkg).into_iter())
        .filter(|(p, _)| want.iter().any(|(w, _)| w == p))
        .collect();
    let names = |v: &[(String, Vec<u8>)]| v.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
    assert!(!want.is_empty());
    assert!(got == want, "{:?} vs {:?}", names(&got), names(&want));
    let _ = std::fs::remove_dir_all(root);
}

/// A zstd-packed source converted as it is ends with the zstd runtime (another one replaced),
/// and refuses `remove_backport`: its kept `ampr_emu.index` lists the backport.
#[test]
fn a_zstd_packed_source_gets_the_zstd_runtime_and_keeps_its_backport() {
    let root = dir("zstd-runtime");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(zstd_request(&src, &packed)).1.unwrap();
    write(&packed, "fakelib/libSceAmpr.sprx", RELEASE);
    let out = root.join("out");
    let (events, result) = run(request(&packed, Format::Folder, &out));
    result.unwrap();
    assert!(logged(&events, "with the zstd runtime"), "{events:?}");
    assert!(
        std::fs::read(out.join("fakelib/libSceAmpr.sprx")).unwrap()
            == ps5_dump_forge_lz4::runtime::ZSTD
    );

    write(
        &packed,
        "fakelib/libSceAgc.sprx",
        b"\x7fELF a system library",
    );
    let err = run(ConvertRequest {
        remove_backport: true,
        ..request(&packed, Format::Folder, &root.join("b"))
    })
    .1
    .unwrap_err();
    assert!(
        err.contains("lists its backport in its ampr_emu.index"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// zstd unpack writes the logical files with the release runtime in place of the zstd one
/// and a fresh index; it refuses a source without zstd packs and any LZ4 or pack option.
#[test]
fn zstd_unpack_restores_the_logical_files() {
    let root = dir("zstd-unpack");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(zstd_request(&src, &packed)).1.unwrap();
    let unpack = |source: &Path, format: Format, output: &Path| ConvertRequest {
        zstd_unpack: true,
        ..request(source, format, output)
    };
    let out = root.join("out");
    let (events, result) = run(unpack(&packed, Format::Folder, &out));
    result.unwrap();
    assert!(logged(&events, "with zstd packs unpacked"), "{events:?}");
    assert!(!out.join("ampr_zstd.index").exists() && !out.join("ampr_zstd-000.pak").exists());
    assert!(std::fs::read(out.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    assert_eq!(assets(&out), assets(&src));
    let mut listed = index_paths(&out);
    listed.sort();
    let mut files: Vec<(String, u64)> = (tree_bytes(&out).into_iter())
        .filter(|(p, _)| p != "ampr_emu.index")
        .map(|(p, b)| (p, b.len() as u64))
        .collect();
    files.sort();
    assert_eq!(listed, files);
    // Into an image and a package too.
    for (format, name) in [
        (Format::Ffpkg, "PPSA01234.ffpkg"),
        (Format::Pkg, "game.pkg"),
    ] {
        let image = root.join(name);
        run(unpack(&packed, format, &image)).1.unwrap();
        let back = root.join(format!("{name}-back"));
        run(request(&image, Format::Folder, &back)).1.unwrap();
        assert!(!back.join("ampr_zstd.index").exists(), "{name}");
        assert!(back.join("data/assets/level1.bin").is_file(), "{name}");
        assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    }

    let refused = |req: ConvertRequest, want: &str| {
        let err = run(req).1.unwrap_err();
        assert!(err.contains(want), "{want}: {err}");
    };
    refused(
        unpack(&src, Format::Folder, &root.join("a")),
        "the source holds no zstd packs",
    );
    refused(
        ConvertRequest {
            lz4: Some(Lz4Mode::Unpack),
            ..unpack(&packed, Format::Folder, &root.join("b"))
        },
        "zstd unpack takes no LZ4 option",
    );
    refused(
        ConvertRequest {
            zstd_pack: true,
            ..unpack(&packed, Format::Folder, &root.join("c"))
        },
        "zstd pack and zstd unpack can't go together",
    );
    let _ = std::fs::remove_dir_all(root);
}

/// LZ4 Pack of a zstd-packed game decodes the zstd packs first: the LZ4 packs hold the
/// logical files, with the release runtime in place of the zstd one.
#[test]
fn lz4_pack_of_a_zstd_packed_game_decodes_it_first() {
    let root = dir("zstd-to-lz4");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(zstd_request(&src, &packed)).1.unwrap();
    let lz4 = root.join("lz4");
    let (events, result) = run(request(&packed, Format::Lz4, &lz4));
    result.unwrap();
    assert!(logged(&events, "with zstd packs unpacked"), "{events:?}");
    assert!(lz4.join("ampr_assets-000.pak").is_file());
    assert!(!lz4.join("ampr_zstd.index").exists());
    let back = root.join("back");
    run(lz4_request(
        &lz4,
        Format::Folder,
        &back,
        Some(Lz4Mode::Unpack),
    ))
    .1
    .unwrap();
    assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    assert_eq!(assets(&back), assets(&src));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn folder_to_lz4_and_back() {
    let root = dir("lz4-round-trip");
    let src = root.join("src");
    ampr_game(&src);
    let before = snapshot(&src);
    // Folder rules: no extension, and the generated name is the folder's.
    let named = default_output(&src, Format::Lz4, &root).unwrap();
    assert_eq!(named, root.join("PPSA01234"));
    let packed = root.join("packed");
    let (events, result) = run(request(&src, Format::Lz4, &packed));
    let report = result.unwrap();
    assert_eq!(snapshot(&src), before, "the source is never touched");
    assert_eq!(
        stages(&events),
        ["scan", "preflight", "write", "pack", "verify", "finalize"]
    );
    assert!(logged(&events, ps5_dump_forge_lz4::runtime::WARNING));
    assert!(
        logged(&events, "LZ4 rules: the built-in guess"),
        "{events:?}"
    );
    assert!(
        logged(&events, "wrote LZ4 packs: 3 files packed into 1 volumes"),
        "{events:?}"
    );
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.starts_with("packs: the manifest"))
    );
    // One read of each file, front to back, gave every digest: nothing was hashed again.
    assert!(!logged(&events, "not read whole"), "{events:?}");
    assert!(report.checks.iter().any(|c| c == "empty dirs: 1 match"));
    // Packed assets are gone from the folder; loose files, packs, runtime and index are there.
    for gone in [
        "data/big.bin",
        "data/assets/level1.bin",
        "data/assets/noise.bin",
    ] {
        assert!(!packed.join(gone).exists(), "{gone}");
    }
    for kept in [
        "eboot.bin",
        "readme.txt",
        "data/zero.bin",
        "sce_sys/param.json",
        "ampr_assets.index",
        "ampr_assets.index.crc",
        "ampr_assets-000.pak",
    ] {
        assert!(packed.join(kept).is_file(), "{kept}");
    }
    assert!(!packed.join("ampr_assets.index.runtime").exists());
    assert!(packed.join("data/empty").is_dir());
    assert!(std::fs::read(packed.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    let mut want: Vec<(String, u64)> = tree_bytes(&src)
        .into_iter()
        .map(|(p, b)| (p, b.len() as u64))
        .chain([("fakelib/libSceAmpr.sprx".to_string(), RELEASE.len() as u64)])
        .collect();
    want.sort();
    let mut got = index_paths(&packed);
    got.sort();
    assert_eq!(got, want);
    assert!(stale_parts(&root).is_empty());

    // Unpacked, the assets are the source's, byte for byte; the runtime and index stay.
    let back = root.join("back");
    let (events, result) = run(lz4_request(
        &packed,
        Format::Folder,
        &back,
        Some(Lz4Mode::Unpack),
    ));
    result.unwrap();
    assert!(logged(
        &events,
        "with LZ4 packs unpacked (3 packed files in 1 volumes)"
    ));
    assert_eq!(assets(&back), tree_bytes(&src));
    assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    // Unpacking keeps the index byte for byte: it lists exactly the unpacked files.
    assert!(logged(&events, "keeping ampr_emu.index"), "{events:?}");
    let index = |dir: &Path| std::fs::read(dir.join("ampr_emu.index")).unwrap();
    assert!(index(&back) == index(&packed));
    assert!(!back.join("ampr_assets.index").exists());
    assert!(back.join("data/empty").is_dir());

    // Unpacking what holds no packs only says so.
    let again = root.join("again");
    let (events, result) = run(lz4_request(
        &back,
        Format::Folder,
        &again,
        Some(Lz4Mode::Unpack),
    ));
    result.unwrap();
    assert!(logged(&events, "nothing to unpack"));
    assert_eq!(tree_bytes(&again), tree_bytes(&back));

    // Without an option, a packed folder converts as it is: the packs are plain files.
    let image = root.join("packed.ffpkg");
    run(request(&packed, Format::Ffpkg, &image)).1.unwrap();
    let out = root.join("from-image");
    run(request(&image, Format::Folder, &out)).1.unwrap();
    assert_eq!(tree_bytes(&out), tree_bytes(&packed));
    let _ = std::fs::remove_dir_all(root);
}

/// A conversion without LZ4 options of an AMPR title (by user decision): the release runtime
/// installed where there is none and in place of an older or unknown one, another as new as
/// ours kept (by Pack only when it reads packs); the index kept only when it lists exactly the
/// files written, else a new one without the journal and logs. A non-AMPR title is untouched.
#[test]
fn a_plain_conversion_repairs_an_ampr_title() {
    let root = dir("lz4-plain-repair");
    let src = root.join("src");
    ampr_game(&src);
    let runtime = |root: &Path| std::fs::read(root.join("fakelib/libSceAmpr.sprx")).ok();

    // No runtime and no index: both written.
    let fixed = root.join("fixed");
    let (events, result) = run(request(&src, Format::Folder, &fixed));
    result.unwrap();
    assert!(
        logged(&events, "installing the release runtime"),
        "{events:?}"
    );
    assert!(logged(&events, "no ampr_emu.index yet"), "{events:?}");
    assert!(runtime(&fixed).unwrap() == RELEASE);
    assert_indexes_folder(&fixed);
    assert_eq!(assets(&fixed), tree_bytes(&src));
    write(&fixed, "ampr_commands.bin", b"a session's journal");
    write(&fixed, "ampr_emu.log", b"log");

    // An exact index beside Forge's release runtime: everything kept as it is.
    let exact = root.join("exact");
    let (events, result) = run(request(&fixed, Format::Folder, &exact));
    result.unwrap();
    assert!(!logged(&events, "LZ4"), "{events:?}");
    assert_eq!(tree_bytes(&exact), tree_bytes(&fixed));

    // A file added since: a new index, the journal and logs left out.
    write(&fixed, "data/added.bin", b"new");
    let image = root.join("PPSA01234.exfat");
    let (events, result) = run(request(&fixed, Format::Exfat, &image));
    result.unwrap();
    assert!(
        logged(&events, "does not list exactly the files written (stale)"),
        "{events:?}"
    );
    assert!(logged(&events, "leaving out ampr_commands.bin"));
    assert!(logged(&events, "leaving out ampr_emu.log"));
    let back = root.join("back");
    run(request(&image, Format::Folder, &back)).1.unwrap();
    assert!(!back.join("ampr_commands.bin").exists() && !back.join("ampr_emu.log").exists());
    assert_indexes_folder(&back);
    assert!(runtime(&back).unwrap() == RELEASE);

    // A damaged index is written anew too.
    write(&fixed, "ampr_emu.index", b"not an index");
    let out = root.join("damaged");
    let (events, result) = run(request(&fixed, Format::Folder, &out));
    result.unwrap();
    assert!(logged(&events, "ampr_emu.index does not read (damaged)"));
    assert_indexes_folder(&out);

    // Another ampr_emu as new as the bundled one: kept, its index repaired. Pack keeps it only
    // when it holds the pack format.
    let newer: &[u8] = b"\x7fELF AMPRPAK4 AMPRDAT3 9.9 (c) Drakmor";
    let no_packs: &[u8] = b"\x7fELF 0.4.2.1 (c) Drakmor";
    for (name, bytes, packs) in [("newer", newer, true), ("no-packs", no_packs, false)] {
        let other = root.join(name);
        copy_tree(&back, &other);
        write(&other, "fakelib/libSceAmpr.sprx", bytes);
        let out = root.join(format!("{name}-out"));
        let (events, result) = run(request(&other, Format::Folder, &out));
        result.unwrap();
        assert!(logged(&events, "keeping the game's ampr_emu"), "{events:?}");
        assert!(logged(&events, "(stale)"), "{events:?}");
        assert!(runtime(&out).unwrap() == bytes, "{name}");
        assert_indexes_folder(&out);
        let packed = root.join(format!("{name}-packed"));
        run(request(&other, Format::Lz4, &packed)).1.unwrap();
        let want = if packs { bytes } else { RELEASE };
        assert!(runtime(&packed).unwrap() == want, "{name}");
    }

    // An older one, or one naming no version: replaced.
    for (name, bytes) in [
        ("older", &b"\x7fELF AMPRPAK4 AMPRDAT3 0.4.2 (c) Drakmor"[..]),
        ("unknown", &b"\x7fELF someone else's AMPR"[..]),
    ] {
        let other = root.join(name);
        copy_tree(&back, &other);
        write(&other, "fakelib/libSceAmpr.sprx", bytes);
        let out = root.join(format!("{name}-out"));
        let (events, result) = run(request(&other, Format::Folder, &out));
        result.unwrap();
        assert!(
            logged(&events, "replacing the game's runtime"),
            "{events:?}"
        );
        assert!(runtime(&out).unwrap() == RELEASE, "{name}");
        assert_indexes_folder(&out);
    }

    // No runtime but an index (it lists the runtime): the runtime goes in, the index is kept.
    let none = root.join("none");
    copy_tree(&src, &none);
    std::fs::copy(exact.join("ampr_emu.index"), none.join("ampr_emu.index")).unwrap();
    let out = root.join("none-out");
    let (events, result) = run(request(&none, Format::Folder, &out));
    result.unwrap();
    assert!(logged(&events, "installing the release runtime"));
    assert!(runtime(&out).unwrap() == RELEASE);
    assert!(
        std::fs::read(out.join("ampr_emu.index")).unwrap()
            == std::fs::read(exact.join("ampr_emu.index")).unwrap()
    );

    // A title that does not use AMPR: nothing changes.
    let plain = root.join("plain");
    game(&plain);
    write(&plain, "ampr_emu.index", b"not an index");
    let out = root.join("plain-out");
    let (events, result) = run(request(&plain, Format::Folder, &out));
    result.unwrap();
    assert!(!logged(&events, "LZ4"), "{events:?}");
    assert_eq!(tree_bytes(&out), tree_bytes(&plain));
    let _ = std::fs::remove_dir_all(root);
}

/// A STORED `.zip` of `entries` at `path`.
fn stored_zip(path: &Path, entries: &[(String, Vec<u8>)]) {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in entries {
        let at = out.len() as u32;
        // version 20, no flags, stored, 1980-01-01 00:00, crc, sizes, name length, no extra
        let fields = |b: &mut Vec<u8>| {
            for v in [20u16, 0, 0, 0, 0x21] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in [crc32fast::hash(data), data.len() as u32, data.len() as u32] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&(name.len() as u16).to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes());
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        fields(&mut out);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        fields(&mut central);
        central.extend_from_slice(&[0; 10]); // comment, disk, attributes
        central.extend_from_slice(&at.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let (cd_at, cd_len, n) = (out.len() as u32, central.len() as u32, entries.len() as u16);
    out.append(&mut central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for v in [n, n] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [cd_len, cd_at] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0; 2]);
    std::fs::write(path, out).unwrap();
}

/// Into a `.pkg`, a plain conversion builds no `ampr_emu.index` of its own: the builder writes one after leaving out SDK leftovers, so names only they share in
/// case don't refuse the job.
#[test]
fn a_pkg_of_an_ampr_title_leaves_its_index_to_the_builder() {
    let root = dir("lz4-plain-pkg");
    let src = root.join("src");
    ampr_game(&src);
    write(&src, "ampr_commands.bin", b"a session's journal");
    write(&src, "ampr_emu.log", b"log");
    write(&src, "ampr_emu.index", b"a stale index");
    let mut entries = tree_bytes(&src);
    entries.push(("sce_sc/a".into(), b"an SDK leftover".to_vec()));
    entries.push(("sce_sc/A".into(), b"another one".to_vec()));
    let zip = root.join("game.zip");
    stored_zip(&zip, &entries);
    {
        let name = "plain";
        let pkg = root.join(format!("{name}.pkg"));
        let (events, result) = run(request(&zip, Format::Pkg, &pkg));
        result.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            logged(&events, "installing the release runtime"),
            "{name}: {events:?}"
        );
        assert!(logged(&events, "leaving out ampr_commands.bin"), "{name}");
        assert!(logged(&events, "leaving out ampr_emu.log"), "{name}");
        assert!(!logged(&events, "LZ4: writing a new"), "{name}: {events:?}");
        // The package as built, not a conversion of it (which would write its own index).
        let mut tree = ps5_dump_forge_fpkg::FpkgSource::open(&pkg, None).unwrap();
        let index = tree.read("ampr_emu.index").unwrap();
        let mut got = ps5_dump_forge_lz4::index::read_index(&index)
            .unwrap()
            .records;
        let mut want: Vec<(String, u64)> = tree
            .files()
            .iter()
            .filter(|f| f.path != "ampr_emu.index")
            .map(|f| (f.path.clone(), f.size))
            .collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{name}");
        assert!(
            got.contains(&("fakelib/libSceAmpr.sprx".to_string(), RELEASE.len() as u64)),
            "{name}"
        );
        assert!(
            tree.read("fakelib/libSceAmpr.sprx").unwrap() == RELEASE,
            "{name}"
        );
        assert!(
            !got.iter()
                .any(|(p, _)| p.starts_with("ampr_") || p.starts_with("sce_sc/")),
            "{name}: {got:?}"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

/// A packed source converted without LZ4 options: its index and packs byte for byte, the
/// runtime replaced by the release one; Forge still unpacks the result.
#[test]
fn a_packed_source_keeps_its_index_and_gets_the_release_runtime() {
    let root = dir("lz4-plain-packed");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(request(&src, Format::Lz4, &packed)).1.unwrap();
    write(
        &packed,
        "fakelib/libSceAmpr.sprx",
        b"\x7fELF AMPRPAK4 AMPRDAT3 0.4.2 (c) Drakmor",
    );
    write(&packed, "ampr_emu.log", b"log");
    let image = root.join("PPSA01234.ffpkg");
    let (events, result) = run(request(&packed, Format::Ffpkg, &image));
    result.unwrap();
    assert!(
        logged(&events, "replacing the game's runtime"),
        "{events:?}"
    );
    assert!(logged(&events, "keeping ampr_emu.index as it is"));
    let back = root.join("back");
    run(request(&image, Format::Folder, &back)).1.unwrap();
    assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    for same in [
        "ampr_emu.index",
        "ampr_emu.log",
        "ampr_assets.index",
        "ampr_assets-000.pak",
    ] {
        assert!(
            std::fs::read(back.join(same)).unwrap() == std::fs::read(packed.join(same)).unwrap(),
            "{same}"
        );
    }
    let unpacked = root.join("unpacked");
    run(lz4_request(
        &back,
        Format::Folder,
        &unpacked,
        Some(Lz4Mode::Unpack),
    ))
    .1
    .unwrap();
    assert_eq!(assets(&unpacked), tree_bytes(&src));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lz4_refusals() {
    let root = dir("lz4-refusals");
    let src = root.join("src");
    ampr_game(&src);
    let refused = |req: ConvertRequest, want: &str| {
        let out = req.output.clone();
        let err = run(req).1.unwrap_err();
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        assert!(!out.exists(), "{}", out.display());
    };
    refused(
        lz4_request(&src, Format::Lz4, &root.join("u"), Some(Lz4Mode::Unpack)),
        "redundant",
    );
    refused(
        ConvertRequest {
            lz4_level: 2,
            ..request(&src, Format::Lz4, &root.join("l"))
        },
        "LZ4 level 2 is not 0 (fast) or 3 through 12",
    );

    // A title that does not use AMPR.
    let plain = root.join("plain");
    game(&plain);
    let no_ampr = "does not import libSceAmpr";
    refused(request(&plain, Format::Lz4, &root.join("n")), no_ampr);
    refused(
        lz4_request(&plain, Format::Folder, &root.join("n"), Some(Lz4Mode::Pack)),
        no_ampr,
    );

    // A packed folder's backport is in its manifest: left out only from an unpacked view.
    let packed = root.join("packed");
    run(request(&src, Format::Lz4, &packed)).1.unwrap();
    write(
        &packed,
        "fakelib/libSceAgc.sprx",
        b"\x7fELF a system library",
    );
    refused(
        ConvertRequest {
            remove_backport: true,
            ..request(&packed, Format::Folder, &root.join("b"))
        },
        "lists its backport in its LZ4 manifest",
    );
    std::fs::remove_file(packed.join("fakelib/libSceAgc.sprx")).unwrap();

    // A file named like a pack the target writes.
    write(&src, "ampr_assets-000.PAK", b"someone else's");
    refused(
        request(&src, Format::Lz4, &root.join("c")),
        "named like an LZ4 pack file",
    );
    std::fs::remove_file(src.join("ampr_assets-000.PAK")).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// Runs an `Lz4` job and applies `damage` to its `.part` folder before verification; returns
/// the job's error.
fn damaged_lz4(name: &str, damage: fn(&Path)) -> String {
    let root = dir(name);
    let src = root.join("src");
    ampr_game(&src);
    let out = root.join("out");
    let (held_tx, held_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Mutex::new((Some(held_tx), go_rx, done_tx));
    let jobs = Jobs::new(move |e| {
        let mut s = state.lock().unwrap();
        match e {
            Event::Progress { ref stage, .. } if stage == "verify" => {
                if let Some(tx) = s.0.take() {
                    tx.send(()).unwrap();
                    s.1.recv().unwrap();
                }
            }
            Event::Done { result, .. } => s.2.send(result).unwrap(),
            _ => {}
        }
    });
    jobs.start(request(&src, Format::Lz4, &out));
    held_rx.recv().unwrap();
    damage(&stale_parts(&root).pop().unwrap());
    go_tx.send(()).unwrap();
    let err = done_rx.recv().unwrap().unwrap_err();
    jobs.cancel_all_and_wait();
    assert!(stale_parts(&root).is_empty());
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(root);
    err
}

/// Packs damaged after writing fail verification: no `.part`, no output. A missing sidecar
/// counts: the reader accepts packs without them, but this job wrote them.
#[test]
fn damaged_lz4_output_fails_verification() {
    let err = damaged_lz4("lz4-damaged", |part| {
        let manifest = part.join("ampr_assets.index");
        let mut bytes = std::fs::read(&manifest).unwrap();
        bytes[0] ^= 0xff;
        std::fs::write(&manifest, bytes).unwrap();
    });
    assert!(err.contains("reading the output back"), "{err}");
    let err = damaged_lz4("lz4-no-crc", |part| {
        std::fs::remove_file(part.join("ampr_assets.index.crc")).unwrap();
    });
    assert!(err.contains("ampr_assets.index.crc is missing"), "{err}");
}

// ---- LZ4 helpers -------------------------------------------------------------------------------

/// Every file of `from` (junk left out) written into `to`.
fn copy_tree(from: &Path, to: &Path) {
    for (path, bytes) in tree_bytes(from) {
        write(to, &path, &bytes);
    }
}

/// `index_paths` against the files of `root` (junk, the index and the journal left out).
fn assert_indexes_folder(root: &Path) {
    let mut want: Vec<(String, u64)> = tree_bytes(root)
        .into_iter()
        .filter(|(p, _)| p != "ampr_emu.index" && p != "ampr_commands.bin")
        .map(|(p, b)| (p, b.len() as u64))
        .collect();
    let mut got = index_paths(root);
    want.sort();
    got.sort();
    assert_eq!(got, want);
}

/// Inspect's `index`: the plain conversion's own comparison, journal and logs left out, a
/// runtime it installs or replaces counted at the release runtime's size, one it keeps at its
/// own; `None` where no conversion touches the index.
#[test]
fn inspect_reports_the_index_state() {
    let root = dir("lz4-facts-index");
    let src = root.join("src");
    ampr_game(&src);
    let index = |path: &Path| inspect(path).unwrap().lz4.unwrap().index;
    assert_eq!(
        index(&src),
        Some("missing"),
        "no runtime, no index: both written"
    );

    let game = root.join("game");
    run(request(&src, Format::Folder, &game)).1.unwrap();
    write(&game, "ampr_commands.bin", b"a session's journal");
    write(&game, "ampr_emu.log", b"log");
    assert_eq!(index(&game), Some("ok"));
    let image = root.join("PPSA01234.ffpkg");
    run(request(&game, Format::Ffpkg, &image)).1.unwrap();
    assert_eq!(index(&image), Some("ok"));
    write(
        &game,
        "fakelib/libSceAmpr.sprx",
        b"\x7fELF AMPRPAK4 AMPRDAT3 9.9 (c) Drakmor",
    );
    assert_eq!(index(&game), Some("stale"), "a newer runtime is kept");
    write(
        &game,
        "fakelib/libSceAmpr.sprx",
        b"\x7fELF AMPRPAK4 AMPRDAT3 0.4.2 (c) Drakmor",
    );
    assert_eq!(index(&game), Some("ok"), "an older one is replaced");

    let packed = root.join("packed");
    run(request(&game, Format::Lz4, &packed)).1.unwrap();
    assert_eq!(index(&packed), None, "kept byte for byte");

    write(&game, "data/added.bin", b"new");
    assert_eq!(index(&game), Some("stale"));
    write(&game, "ampr_emu.index", b"not an index");
    assert_eq!(index(&game), Some("damaged"));
    std::fs::remove_file(game.join("ampr_emu.index")).unwrap();
    assert_eq!(index(&game), Some("missing"));
    std::fs::remove_file(game.join("fakelib/libSceAmpr.sprx")).unwrap();
    assert_eq!(index(&game), Some("missing"), "no runtime: both written");
    let _ = std::fs::remove_dir_all(root);
}

/// Inspect's `runtime_action` (a plain conversion) and `pack_runtime_action` (Pack, or Unpack
/// of a packed source): `lz4::Runtime::action`, `None` for a non-AMPR title.
#[test]
fn inspect_reports_the_runtime_action() {
    let root = dir("lz4-facts-runtime");
    let src = root.join("src");
    ampr_game(&src);
    let actions = |path: &Path| {
        let f = inspect(path).unwrap().lz4.unwrap();
        (f.runtime_action, f.pack_runtime_action)
    };
    let both = |a| (Some(a), Some(a));
    assert_eq!(actions(&src), both("install"), "no runtime");

    let game = root.join("game");
    run(request(&src, Format::Folder, &game)).1.unwrap();
    assert_eq!(actions(&game), both("keep"), "Forge's release runtime");
    let packed = root.join("packed");
    run(request(&game, Format::Lz4, &packed)).1.unwrap();
    assert_eq!(
        actions(&packed),
        both("keep"),
        "packed, Unpack keeps it too"
    );

    let runtime = |bytes: &[u8]| write(&game, "fakelib/libSceAmpr.sprx", bytes);
    runtime(b"\x7fELF AMPRPAK4 AMPRDAT3 9.9 (c) Drakmor");
    assert_eq!(actions(&game), both("keep"), "newer, reads packs");
    let f = inspect(&game).unwrap().lz4.unwrap();
    assert_eq!(f.runtime_version.as_deref(), Some("9.9"));
    runtime(b"\x7fELF 9.9 (c) Drakmor");
    assert_eq!(
        actions(&game),
        (Some("keep"), Some("replace")),
        "newer without the pack format: Pack replaces it"
    );
    runtime(b"\x7fELF AMPRPAK4 AMPRDAT3 0.4.2 (c) Drakmor");
    assert_eq!(actions(&game), both("replace"), "older");
    runtime(b"\x7fELF AMPRPAK4 AMPRDAT3 no version");
    assert_eq!(actions(&game), both("replace"), "no version");

    let other = root.join("other");
    self::game(&other);
    write(&other, "ampr_emu.index", b"an index");
    let f = inspect(&other).unwrap().lz4.unwrap();
    assert!(!f.imports_ampr);
    assert_eq!((f.runtime_action, f.pack_runtime_action), (None, None));
    let _ = std::fs::remove_dir_all(root);
}

/// The pack files of a packed folder (or an image's files extracted as they are) without the
/// bytes that carry the job's time: every volume past its 64-byte header, the CRCs past the
/// sidecar's header, the manifest's chunk table only, and no `ampr_emu.index`. Loose files whole.
fn pack_payload(root: &Path) -> Vec<(String, Vec<u8>)> {
    tree_bytes(root)
        .into_iter()
        .filter(|(p, _)| p != "ampr_emu.index")
        .map(|(p, b)| {
            let b = if p.starts_with("ampr_assets-") {
                b[64..].to_vec()
            } else if p == "ampr_assets.index.crc" {
                b[48..].to_vec()
            } else if p == "ampr_assets.index" {
                let m = ps5_dump_forge_lz4::reader::open_manifest(&b).unwrap();
                format!("{:?}", m.chunks).into_bytes()
            } else {
                b
            };
            (p, b)
        })
        .collect()
}

/// `lz4: pack` into every image format, verified fast and in full: the image verifies (as written and through its packs), holds the same packs the folder target
/// writes, chunk for chunk, and unpacks to the source's files.
#[test]
fn lz4_pack_straight_into_each_image() {
    let root = dir("lz4-pack-image");
    let src = root.join("src");
    ampr_game(&src);
    let before = snapshot(&src);
    let images = [
        (Format::Exfat, None, "exfat"),
        (Format::Ffpkg, None, "ffpkg"),
        (Format::Ffpfs, None, "ffpfs"),
        (Format::Ffpfsc, Some(Format::Exfat), "ffpfsc"),
        (Format::Ffpfsc, Some(Format::Ffpkg), "ffpfsc"),
    ];
    for (rules, full) in [("fast", false), ("full", true)] {
        let folder = root.join(format!("folder-{rules}"));
        run(request(&src, Format::Lz4, &folder)).1.unwrap();
        let want = pack_payload(&folder);
        assert!(want.iter().any(|(p, _)| p == "ampr_assets-000.pak"));
        for (n, (format, inner, ext)) in images.into_iter().enumerate() {
            let image = root.join(format!("{rules}-{n}.{ext}"));
            let (events, result) = run(ConvertRequest {
                inner,
                lz4: Some(Lz4Mode::Pack),
                full_verify: full,
                ..request(&src, format, &image)
            });
            let report = result.unwrap_or_else(|e| panic!("{rules} {ext}: {e}"));
            one_bar(&events);
            // `run` holds this to the announced list; preflight is not reported again after
            // measure.
            assert_eq!(
                stages(&events),
                [
                    "scan",
                    "preflight",
                    "measure",
                    "write",
                    "verify",
                    "finalize"
                ]
            );
            assert!(logged(&events, "no temporary folder"), "{events:?}");
            assert!(logged(&events, "measured LZ4 packs"), "{events:?}");
            // Each logical file was read whole, front to back, through the packs.
            assert!(
                !logged(&events, "not read whole"),
                "{rules} {ext}: {events:?}"
            );
            let checks = &report.checks;
            assert!(checks.iter().any(|c| c.starts_with("packs: the manifest")));
            assert!(checks.iter().any(|c| c.starts_with("as written, manifest")));
            assert!(checks.iter().any(|c| c.ends_with("files match the source")) || !full);

            // As written: the folder target's packs and loose files.
            let plain = root.join(format!("{rules}-{n}-plain"));
            run(request(&image, Format::Folder, &plain)).1.unwrap();
            assert!(
                pack_payload(&plain) == want,
                "{rules} {ext}: not the folder's packs"
            );
            // Through the packs: the source's files, the release runtime and a fresh index.
            let back = root.join(format!("{rules}-{n}-back"));
            run(lz4_request(
                &image,
                Format::Folder,
                &back,
                Some(Lz4Mode::Unpack),
            ))
            .1
            .unwrap();
            assert_eq!(assets(&back), tree_bytes(&src), "{rules} {ext}");
            assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
            assert!(
                index_paths(&back)
                    .iter()
                    .any(|(p, _)| p == "data/assets/level1.bin")
            );
        }
    }
    assert_eq!(snapshot(&src), before, "the source is never touched");
    assert!(stale_parts(&root).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

/// An HC level reaches the measure pass and the image's re-encode, which must agree.
#[test]
fn lz4_pack_at_an_hc_level_into_an_image() {
    let root = dir("lz4-level");
    let src = root.join("src");
    ampr_game(&src);
    let image = root.join("g.ffpkg");
    let (events, result) = run(ConvertRequest {
        lz4: Some(Lz4Mode::Pack),
        lz4_level: 12,
        ..request(&src, Format::Ffpkg, &image)
    });
    result.unwrap();
    assert!(logged(&events, "level 12"), "{events:?}");
    let back = root.join("back");
    run(lz4_request(
        &image,
        Format::Folder,
        &back,
        Some(Lz4Mode::Unpack),
    ))
    .1
    .unwrap();
    assert_eq!(assets(&back), tree_bytes(&src));
    let _ = std::fs::remove_dir_all(root);
}

/// LZ4 Pack with ampr-emu-ng installs it, and a plain conversion of the packed folder keeps it
/// (it reads LZ4 packs).
#[test]
fn lz4_pack_with_ampr_emu_ng_keeps_it() {
    let root = dir("lz4-ng");
    let src = root.join("src");
    ampr_game(&src);
    let packed = root.join("packed");
    run(ConvertRequest {
        lz4_runtime: ps5_dump_forge_core::Lz4Runtime::Ng,
        ..lz4_request(&src, Format::Lz4, &packed, None)
    })
    .1
    .unwrap();
    let ng = ps5_dump_forge_lz4::runtime::ZSTD;
    assert!(std::fs::read(packed.join("fakelib/libSceAmpr.sprx")).unwrap() == ng);
    let copy = root.join("copy");
    run(request(&packed, Format::Folder, &copy)).1.unwrap();
    assert!(std::fs::read(copy.join("fakelib/libSceAmpr.sprx")).unwrap() == ng);
    let _ = std::fs::remove_dir_all(root);
}

/// Cancelled in the measure pass (before any `.part`) or while the image is written: nothing
/// of the job is left.
#[test]
fn cancel_lz4_into_an_image_leaves_nothing() {
    let pack = |format: Format| {
        move |src: &Path, out: &Path| lz4_request(src, format, out, Some(Lz4Mode::Pack))
    };
    cancel_at(
        ampr_game,
        pack(Format::Ffpkg),
        "lz4m.ffpkg",
        "measure",
        false,
    );
    cancel_at(ampr_game, pack(Format::Exfat), "lz4w.exfat", "write", true);
    cancel_at(
        ampr_game,
        pack(Format::Ffpfsc),
        "lz4w.ffpfsc",
        "verify",
        true,
    );
}

/// The job's one bar, as `(stage, done, total)` per progress event, checked the way the UI
/// shows it (the largest fraction yet): `done` never goes back, and the bar is full only once
/// no byte pass is left (nothing after it moves `done`). Returns the events.
fn one_bar(events: &[Event]) -> Vec<(&str, u64, u64)> {
    let bar: Vec<(&str, u64, u64)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Progress {
                stage, done, total, ..
            } => Some((stage.as_str(), *done, *total)),
            _ => None,
        })
        .collect();
    for pair in bar.windows(2) {
        assert!(
            pair[0].1 <= pair[1].1,
            "done went back: {pair:?} in {bar:?}"
        );
    }
    if let Some(full) = bar.iter().position(|&(_, d, t)| t > 0 && d >= t) {
        assert!(
            bar[full..].iter().all(|&(_, d, _)| d == bar[full].1),
            "full at {:?} with work left: {bar:?}",
            bar[full]
        );
    }
    bar
}

#[test]
fn an_interrupted_edit_blocks_other_jobs_and_goes_with_its_image() {
    let root = dir("interrupted-edit");
    let src = root.join("src");
    game(&src);
    let image = root.join("game.exfat");
    run(request(&src, Format::Exfat, &image)).1.unwrap();
    let journal = root.join("game.exfat.forge-undo");
    std::fs::write(&journal, b"x").unwrap();
    let interrupted = |req: ConvertRequest| {
        let err = run(req).1.unwrap_err();
        assert!(
            err.contains("an earlier edit of this image was interrupted"),
            "{err}"
        );
    };
    // Convert and Extract both refuse it as it is.
    interrupted(request(&image, Format::Folder, &root.join("out")));
    let out = root.join("extracted");
    std::fs::create_dir(&out).unwrap();
    interrupted(ConvertRequest {
        extract: vec!["eboot.bin".into()],
        ..request(&image, Format::Folder, &out)
    });
    assert!(!root.join("out").exists());
    assert_eq!(names_in(&out), Vec::<String>::new());
    assert!(journal.exists());
    // Deleting the image takes its journal too.
    let jobs = Jobs::new(|_| {});
    assert_eq!(deleted(&jobs, &image, &[]), "ok");
    assert!(!image.exists() && !journal.exists());
    let _ = std::fs::remove_dir_all(root);
}

/// What `delete_path` refused, or "ok".
fn deleted(jobs: &Jobs, path: &Path, protected: &[PathBuf]) -> &'static str {
    match jobs.delete_path(path, protected) {
        Ok(()) => "ok",
        Err(DeleteError::Refused(_)) => "refused",
        Err(DeleteError::Missing(_)) => "missing",
        Err(DeleteError::Busy(_)) => "busy",
        Err(DeleteError::Failed(e)) => panic!("{}: {e}", path.display()),
    }
}

#[test]
fn delete_removes_a_file_or_folder_but_not_through_links() {
    let root = dir("delete");
    let jobs = Jobs::new(|_| {});
    write(&root, "file.bin", b"x");
    assert_eq!(deleted(&jobs, &root.join("file.bin"), &[]), "ok");
    assert!(!root.join("file.bin").exists());
    game(&root.join("game"));
    write(&root, "outside/keep.bin", b"keep");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("outside"), root.join("game/data/link")).unwrap();
    // Another spelling of `game`.
    assert_eq!(deleted(&jobs, &root.join("game/data/.."), &[]), "ok");
    assert!(!root.join("game").exists());
    assert_eq!(
        std::fs::read(root.join("outside/keep.bin")).unwrap(),
        b"keep"
    );
    assert_eq!(deleted(&jobs, &root.join("game"), &[]), "missing");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn delete_refuses_roots_links_and_special_files() {
    let root = dir("delete-refused");
    let jobs = Jobs::new(|_| {});
    let games = root.join("games");
    write(&games, "a/eboot.bin", b"x");
    let keep = [games.clone()];
    assert_eq!(deleted(&jobs, Path::new(""), &[]), "refused");
    assert_eq!(
        deleted(&jobs, root.ancestors().last().unwrap(), &[]),
        "refused"
    );
    assert_eq!(deleted(&jobs, &games, &keep), "refused");
    assert_eq!(deleted(&jobs, &root, &keep), "refused");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&games, root.join("link")).unwrap();
        assert_eq!(deleted(&jobs, &root.join("link"), &[]), "refused");
        // To the OS `link/` and `link/.` name the target; still the link, still refused.
        let spelled = root.join("link").into_os_string().into_string().unwrap();
        for p in [format!("{spelled}/"), format!("{spelled}/.")] {
            assert_eq!(deleted(&jobs, Path::new(&p), &[]), "refused", "{p}");
        }
        let fifo = std::ffi::CString::new(root.join("fifo").to_str().unwrap()).unwrap();
        // SAFETY: a valid C string.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        assert_eq!(deleted(&jobs, &root.join("fifo"), &[]), "refused");
    }
    assert!(games.join("a/eboot.bin").exists());
    // Inside a protected root is what deleting is for.
    assert_eq!(deleted(&jobs, &games.join("a"), &keep), "ok");
    let _ = std::fs::remove_dir_all(root);
}

/// Nothing an unfinished job uses, running or queued, is deleted, nor anything around it; a
/// sibling with a similar name is. Once the jobs are done, their paths are free again.
#[test]
fn delete_leaves_what_unfinished_jobs_use() {
    let root = dir("delete-busy");
    let src = root.join("src");
    game(&src);
    let queued = root.join("queued");
    game(&queued);
    write(&root, "src2/file.bin", b"x");
    let out = root.join("out.ffpfs");

    // Held on the first write progress, with its `.part` open.
    let (at_tx, at_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Mutex::new((Some(at_tx), go_rx, done_tx));
    let jobs = Jobs::new(move |e| {
        let mut s = state.lock().unwrap();
        match e {
            Event::Progress { ref stage, .. } if stage == "write" => {
                if let Some(tx) = s.0.take() {
                    tx.send(()).unwrap();
                    s.1.recv().unwrap();
                }
            }
            Event::Done { result, .. } => s.2.send(result.map(|_| ())).unwrap(),
            _ => {}
        }
    });
    jobs.start(request(&src, Format::Ffpfs, &out));
    at_rx.recv().unwrap();
    // Queued through a link.
    let aliases = root.join("aliases");
    std::fs::create_dir_all(&aliases).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&queued, aliases.join("game")).unwrap();
    #[cfg(not(unix))]
    let aliases = queued.clone();
    let waiting = jobs.start(request(
        &aliases.join("game"),
        Format::Lz4,
        &root.join("queued-out"),
    ));
    let [part] = &stale_parts(&root)[..] else {
        panic!("one .part while writing");
    };
    for busy in [&src, &src.join("data"), part, &root, &queued, &aliases] {
        assert_eq!(deleted(&jobs, busy, &[]), "busy", "{}", busy.display());
    }
    assert_eq!(deleted(&jobs, &root.join("src2"), &[]), "ok");

    jobs.cancel(waiting);
    go_tx.send(()).unwrap();
    let mut results = [done_rx.recv().unwrap(), done_rx.recv().unwrap()];
    results.sort();
    assert_eq!(results, [Ok(()), Err("cancelled".to_string())]);
    for free in [&src, &queued, &out] {
        assert_eq!(deleted(&jobs, free, &[]), "ok", "{}", free.display());
    }
    jobs.cancel_all_and_wait();
    let _ = std::fs::remove_dir_all(root);
}

/// A request with `edits` is an edit job: its own stages, the image itself as the subject,
/// refusals as preflight findings. Applying the plan is not wired yet.
#[test]
fn an_edit_request_runs_the_edit_job() {
    use ps5_dump_forge_core::Edit;
    for (ext, format) in [("exfat", Format::Exfat), ("ffpkg", Format::Ffpkg)] {
        let root = dir(&format!("edit-dispatch-{ext}"));
        let src = root.join("src");
        game(&src);
        let image = root.join(format!("game.{ext}"));
        run(request(&src, format, &image)).1.unwrap();
        let edit = |edits: Vec<Edit>| ConvertRequest {
            edits,
            // Ignored for an edit.
            ..request(&image, Format::Pkg, &root.join("ignored"))
        };
        let (events, result) = run(edit(vec![Edit::Mkdir {
            path: "more".into(),
        }]));
        assert_eq!(
            announced_stages(&events),
            ["scan", "preflight", "write", "verify", "finalize"]
        );
        assert_eq!(stages(&events).last(), Some(&"finalize"));
        let report = result.unwrap();
        assert_eq!(report.output, image);
        assert_eq!(report.verify.mode, VerifyMode::Fast);
        let seen = inspect(&image).unwrap();
        assert!(seen.empty_dirs.contains(&"more".to_string()), "{ext}");
        assert!(!seen.interrupted_edit);

        // Add, replace, delete and rename; the image then converts to the edited folder.
        let host = root.join("host");
        write(&host, "added.bin", &[5u8; 300_000]);
        write(&host, "zero.bin", b"no longer empty");
        let mut want = tree_bytes(&src);
        let mut full = edit(vec![
            Edit::Add {
                from: host.join("added.bin"),
                to: "data/added.bin".into(),
            },
            Edit::Add {
                from: host.join("zero.bin"),
                to: "data/zero.bin".into(),
            },
            Edit::Rename {
                from: "data/big.bin".into(),
                to: "big.bin".into(),
            },
            Edit::Delete {
                path: "more".into(),
            },
        ]);
        full.full_verify = true;
        let report = run(full).1.unwrap();
        assert_eq!(report.verify.mode, VerifyMode::Full);
        for (path, bytes) in &mut want {
            match path.as_str() {
                "data/big.bin" => *path = "big.bin".into(),
                "data/zero.bin" => *bytes = b"no longer empty".to_vec(),
                _ => {}
            }
        }
        want.push(("data/added.bin".into(), vec![5u8; 300_000]));
        want.sort();
        let out = root.join("out");
        run(request(&image, Format::Folder, &out)).1.unwrap();
        assert_eq!(tree_bytes(&out), want, "{ext}");
        assert!(!root.join(format!("game.{ext}.forge-undo")).exists());

        // A refused edit writes nothing.
        let before = std::fs::read(&image).unwrap();
        let (_, result) = run(edit(vec![Edit::Delete {
            path: "nope".into(),
        }]));
        let err = result.unwrap_err();
        assert!(
            err.starts_with("preflight failed:") && err.contains("nope is not in the game"),
            "{err}"
        );
        assert_eq!(std::fs::read(&image).unwrap(), before, "nothing written");
        assert!(!root.join("ignored").exists());

        // A journal beside the image is an interrupted edit.
        let journal = root.join(format!("game.{ext}.forge-undo"));
        std::fs::write(&journal, b"x").unwrap();
        assert!(inspect(&image).unwrap().interrupted_edit);
        std::fs::remove_file(&journal).unwrap();
    }
}

#[test]
fn a_panic_during_an_edit_restores_the_image_before_the_job_ends() {
    let root = dir("edit-panic");
    let src = root.join("src");
    game(&src);
    let image = root.join("game.exfat");
    run(request(&src, Format::Exfat, &image)).1.unwrap();
    let before = inspect(&image).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let watched = image.clone();
    let jobs = Jobs::new(move |e| {
        if let Event::Progress { stage, .. } = &e
            && stage == "verify"
        {
            panic!("simulated panic in an event callback");
        }
        if let Event::Done { result, .. } = &e {
            // Seen as Done goes out: the image is already back.
            let seen = inspect(&watched).unwrap();
            let journal = watched.with_extension("exfat.forge-undo");
            let _ = tx
                .lock()
                .unwrap()
                .send((result.clone(), seen, journal.exists()));
        }
    });
    jobs.start(ConvertRequest {
        edits: vec![ps5_dump_forge_core::Edit::Delete {
            path: "data/big.bin".into(),
        }],
        ..request(&image, Format::Exfat, &image)
    });
    let (result, seen, journal) = rx.recv().unwrap();
    let err = result.unwrap_err();
    assert!(err.contains("simulated panic"), "{err}");
    assert!(!journal);
    let paths = |i: &ps5_dump_forge_core::Inspection| {
        (i.files.iter())
            .map(|f| (f.path.clone(), f.size))
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&seen), paths(&before));
    assert_eq!(seen.empty_dirs, before.empty_dirs);
}

#[test]
fn an_edit_refuses_what_it_cannot_edit_in_place() {
    let root = dir("edit-refusals");
    for (name, want) in [
        ("game.ffpfsc", "a .ffpfsc can only be rebuilt"),
        ("game.pkg", "a .pkg can only be rebuilt"),
        ("game.zip", "archives can be browsed but not edited"),
        ("game.rar", "archives can be browsed but not edited"),
    ] {
        let path = root.join(name);
        std::fs::write(&path, b"never read").unwrap();
        let req = ConvertRequest {
            edits: vec![ps5_dump_forge_core::Edit::Mkdir { path: "x".into() }],
            ..request(&path, Format::Exfat, &path)
        };
        let err = run(req).1.unwrap_err();
        assert!(
            err.starts_with("preflight failed:") && err.contains(want),
            "{name}: {err}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"never read");
    }
}

/// An edit request that rebuilds `image` (its `format` and `output` are ignored).
fn rebuild(image: &Path, edits: Vec<ps5_dump_forge_core::Edit>) -> ConvertRequest {
    ConvertRequest {
        edits,
        edit_rebuild: true,
        ..request(image, Format::Pkg, &image.with_file_name("ignored"))
    }
}

/// The images a rebuild is tested on: a name, the format, a `.ffpfsc`'s inner format.
const REBUILT: [(&str, Format, Option<Format>); 7] = [
    ("exfat", Format::Exfat, None),
    ("ffpkg", Format::Ffpkg, None),
    ("ffpfs", Format::Ffpfs, None),
    ("ffpfsc-exfat", Format::Ffpfsc, Some(Format::Exfat)),
    ("ffpfsc-ffpkg", Format::Ffpfsc, Some(Format::Ffpkg)),
    ("ffpfsc-ffpfs", Format::Ffpfsc, Some(Format::Ffpfs)),
    ("pkg", Format::Pkg, None),
];

/// `src` written as `name`'s image `game.<ext>` in `root`, as [`REBUILT`] names it.
fn rebuilt_image(
    root: &Path,
    src: &Path,
    (name, format, inner): (&str, Format, Option<Format>),
) -> PathBuf {
    let ext = name.split('-').next().unwrap();
    let image = root.join(format!("game.{ext}"));
    let mut req = request(src, format, &image);
    req.inner = inner;
    run(req).1.unwrap();
    image
}

/// The `.ffpfsc` container's inner image name.
fn inner_name(image: &Path) -> String {
    let file = std::fs::File::open(image).unwrap();
    let label = image.display().to_string();
    (ps5_dump_forge_pfs::open_ffpfsc(Box::new(file), &label)
        .unwrap()
        .1)
        .inner_name
}

#[test]
fn a_rebuild_replaces_the_image_with_its_edited_copy() {
    use ps5_dump_forge_core::Edit;
    for case in REBUILT {
        let (ext, format, inner) = case;
        let root = dir(&format!("edit-rebuild-{ext}"));
        let src = root.join("src");
        game(&src);
        let image = rebuilt_image(&root, &src, case);
        let name = image.file_name().unwrap().to_string_lossy().into_owned();
        // What the image holds before (a package adds files of its own).
        let held = root.join("held");
        run(request(&image, Format::Folder, &held)).1.unwrap();
        let host = root.join("host");
        write(&host, "added.bin", &[5u8; 300_000]);
        write(&host, "zero.bin", b"no longer empty");
        write(&host, "tree/a.txt", b"a");
        std::fs::create_dir_all(host.join("tree/hollow")).unwrap();
        let edits = vec![
            // A new path: a .ffpfs takes it only rebuilt.
            Edit::Add {
                from: host.join("added.bin"),
                to: "data/added.bin".into(),
            },
            Edit::Add {
                from: host.join("zero.bin"),
                to: "data/zero.bin".into(),
            },
            Edit::Add {
                from: host.join("tree"),
                to: "tree".into(),
            },
            Edit::Rename {
                from: "data/big.bin".into(),
                to: "big.bin".into(),
            },
            Edit::Delete {
                path: "data/empty".into(),
            },
            Edit::Mkdir {
                path: "more/deep".into(),
            },
        ];
        if format == Format::Ffpfs {
            // In place a .ffpfs takes no new path.
            let before = std::fs::read(&image).unwrap();
            let in_place = ConvertRequest {
                edit_rebuild: false,
                ..rebuild(&image, edits.clone())
            };
            assert!(run(in_place).1.is_err(), "{ext}");
            assert_eq!(std::fs::read(&image).unwrap(), before);
        }
        let mut req = rebuild(&image, edits);
        req.full_verify = true;
        // The source's own inner format and the request's level, whatever `inner` says.
        req.inner = Some(Format::Exfat);
        req.ffpfsc_level = 1;
        let (events, result) = run(req);
        // A conversion's stages into the image's own format.
        let middle: &[&str] = match format {
            Format::Pkg => &["check", "plan", "compress", "write"],
            _ => &["write"],
        };
        let want: Vec<&str> = (["scan", "preflight"].iter())
            .chain(middle)
            .chain(&["verify", "finalize"])
            .copied()
            .collect();
        assert_eq!(announced_stages(&events), want, "{ext}");
        let report = result.unwrap();
        assert_eq!(report.output, image);
        assert_eq!(report.verify.mode, VerifyMode::Full);
        assert!(
            report.checks.iter().any(|c| c.starts_with("replaced ")),
            "{:?}",
            report.checks
        );
        assert!(logged(&events, "rebuild: writing an edited copy beside"));
        assert_eq!(stale_parts(&root), Vec::<PathBuf>::new(), "{ext}");
        assert!(!root.join("ignored").exists());
        assert!(!root.join(format!("{name}.forge-undo")).exists());
        if let Some(inner) = inner {
            let ext = format!("{inner:?}").to_ascii_lowercase();
            assert_eq!(inner_name(&image), format!("PPSA01234.{ext}"));
        }

        let mut want = tree_bytes(&held);
        want.retain(|(p, _)| p != "data/big.bin");
        for (path, bytes) in &mut want {
            if path == "data/zero.bin" {
                *bytes = b"no longer empty".to_vec();
            }
        }
        let big: Vec<u8> = (0..9 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
        want.push(("big.bin".into(), big));
        want.push(("data/added.bin".into(), vec![5u8; 300_000]));
        want.push(("tree/a.txt".into(), b"a".to_vec()));
        want.sort();
        let out = root.join("out");
        run(request(&image, Format::Folder, &out)).1.unwrap();
        let got = tree_bytes(&out);
        let names = |t: &[(String, Vec<u8>)]| t.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
        assert_eq!(names(&got), names(&want), "{ext}");
        assert!(got == want, "{ext}");
        let seen = inspect(&image).unwrap();
        let mut dirs = seen.empty_dirs.clone();
        dirs.sort();
        assert_eq!(dirs, ["more/deep", "tree/hollow"], "{ext}");
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Runs `req`, calling `hook` on the job's first progress of `at` (at `verify` its `.part` is
/// written).
fn run_hooked(
    req: ConvertRequest,
    at: &'static str,
    hook: impl Fn(&Jobs, u64) + Send + Sync + 'static,
) -> Result<JobReport, String> {
    let (tx, rx) = mpsc::channel();
    let state = Arc::new(Mutex::new((Some(hook), tx, None::<std::sync::Weak<Jobs>>)));
    let hook = state.clone();
    let jobs = Arc::new(Jobs::new(move |e| {
        let mut s = hook.lock().unwrap();
        match e {
            Event::Progress { ref stage, job, .. } if stage == at => {
                if let (Some(f), Some(jobs)) = (s.0.take(), s.2.as_ref().and_then(|j| j.upgrade()))
                {
                    f(&jobs, job);
                }
            }
            Event::Done { result, .. } => s.1.send(result).unwrap(),
            _ => {}
        }
    }));
    state.lock().unwrap().2 = Some(Arc::downgrade(&jobs));
    jobs.start(req);
    let result = rx.recv().unwrap();
    jobs.cancel_all_and_wait();
    result
}

#[test]
fn a_refused_rebuild_leaves_the_image_as_it_was() {
    use ps5_dump_forge_core::Edit;
    for case in REBUILT {
        let (ext, format, _) = case;
        let root = dir(&format!("edit-rebuild-refused-{ext}"));
        let src = root.join("src");
        game(&src);
        let image = rebuilt_image(&root, &src, case);
        let before = std::fs::read(&image).unwrap();
        let unchanged = |what: &str| {
            assert_eq!(std::fs::read(&image).unwrap(), before, "{ext}: {what}");
            assert_eq!(stale_parts(&root), Vec::<PathBuf>::new(), "{ext}: {what}");
            assert!(!root.join("ignored").exists());
        };
        let delete = || {
            vec![Edit::Delete {
                path: "data/big.bin".into(),
            }]
        };

        // Preflight: an edit that does not apply, an LZ4 option, no game left.
        let err = run(rebuild(
            &image,
            vec![Edit::Delete {
                path: "nope".into(),
            }],
        ))
        .1
        .unwrap_err();
        assert!(err.contains("nope is not in the game"), "{err}");
        unchanged("missing path");
        let mut lz4 = rebuild(&image, delete());
        lz4.lz4 = Some(Lz4Mode::Unpack);
        let err = run(lz4).1.unwrap_err();
        assert!(err.contains("an edit takes no LZ4 option"), "{err}");
        unchanged("lz4");

        // Cancelled once the copy is written.
        let err = run_hooked(rebuild(&image, delete()), "verify", |jobs, job| {
            jobs.cancel(job)
        });
        assert_eq!(err.unwrap_err(), "cancelled");
        unchanged("cancel");

        // The image changed after it was read: its stamp fails at finalize.
        let touched = image.clone();
        let err = run_hooked(rebuild(&image, delete()), "verify", move |_, _| {
            let file = std::fs::File::options().write(true).open(&touched).unwrap();
            let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1 << 20);
            file.set_modified(t).unwrap();
        })
        .unwrap_err();
        assert!(err.contains("changed"), "{err}");
        unchanged("stamp");

        // A host file changed before the copy read it fails it too.
        let host = root.join("host.bin");
        std::fs::write(&host, [7u8; 1000]).unwrap();
        let add = vec![Edit::Add {
            from: host.clone(),
            to: "data/host.bin".into(),
        }];
        // A package's builder reads the files from its first stage on.
        let first = if format == Format::Pkg {
            "check"
        } else {
            "write"
        };
        let err = run_hooked(rebuild(&image, add), first, move |_, _| {
            std::fs::write(&host, [8u8; 1000]).unwrap();
            let file = std::fs::File::options().write(true).open(&host).unwrap();
            let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1 << 20);
            file.set_modified(t).unwrap();
        })
        .unwrap_err();
        assert!(err.contains("changed after it was added"), "{err}");
        unchanged("host file");
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn an_ampr_edit_refreshes_the_index_and_drops_the_journal_and_logs() {
    use ps5_dump_forge_core::Edit;
    let root = dir("edit-ampr");
    let src = root.join("src");
    ampr_game(&src);
    let fixed = root.join("fixed");
    run(request(&src, Format::Folder, &fixed)).1.unwrap();
    write(&fixed, "ampr_commands.bin", b"a session's journal");
    write(&fixed, "ampr_emu.log", b"a log");
    let host = root.join("host");
    write(&host, "added.bin", b"new");
    // An older ampr_emu, which a conversion would replace.
    let own: &[u8] = b"\x7fELF AMPRPAK4 AMPRDAT3 0.4.2 (c) Drakmor";
    write(&host, "own.sprx", own);
    let edits = || {
        vec![
            Edit::Add {
                from: host.join("added.bin"),
                to: "data/added.bin".into(),
            },
            // The user's own runtime stays: an edit never swaps it.
            Edit::Add {
                from: host.join("own.sprx"),
                to: "fakelib/libSceAmpr.sprx".into(),
            },
        ]
    };
    let read = |image: &Path| {
        let file = std::fs::File::open(image).unwrap();
        ps5upload_fpkg::ufs2_source::Ufs2Source::from_reader(Box::new(file), "ffpkg".into())
            .unwrap()
    };
    let traces = ["ampr_commands.bin", "ampr_emu.log"];
    for rebuilt in [true, false] {
        let image = root.join(format!("game-{rebuilt}.ffpkg"));
        run(request(&fixed, Format::Ffpkg, &image)).1.unwrap();
        assert!(traces.iter().all(|t| read(&image).read(t).is_ok()));
        let req = |edits| ConvertRequest {
            edit_rebuild: rebuilt,
            ..rebuild(&image, edits)
        };
        if rebuilt {
            // An undo journal beside the image is settled before anything reads the image;
            // one that can't be refuses the rebuild.
            let undo = image.with_file_name(format!("game-{rebuilt}.ffpkg.forge-undo"));
            std::fs::write(&undo, b"x").unwrap();
            let before = std::fs::read(&image).unwrap();
            let err = run(req(edits())).1.unwrap_err();
            assert!(err.contains("undo journal could not be settled"), "{err}");
            assert_eq!(std::fs::read(&image).unwrap(), before);
            std::fs::remove_file(&undo).unwrap();
        }
        // A new folder changes no file: the index stays, and so do the journal and logs.
        run(req(vec![Edit::Mkdir { path: "x".into() }])).1.unwrap();
        assert!(
            traces.iter().all(|t| read(&image).read(t).is_ok()),
            "{rebuilt}"
        );

        let (events, result) = run(req(edits()));
        result.unwrap();
        assert!(
            logged(&events, "writing a new ampr_emu.index"),
            "{events:?}"
        );
        assert!(
            logged(&events, "removing ampr_commands.bin, ampr_emu.log"),
            "{events:?}"
        );
        assert!(!logged(&events, "runtime"), "{events:?}");
        // Read back by the image's own reader: a conversion would swap the older runtime.
        let mut tree = read(&image);
        assert!(tree.read("fakelib/libSceAmpr.sprx").unwrap() == own);
        assert_eq!(tree.read("data/added.bin").unwrap(), b"new");
        for t in traces {
            assert!(tree.files().iter().all(|f| f.path != t), "{rebuilt}: {t}");
        }
        let index = ps5_dump_forge_lz4::index::read_index(&tree.read("ampr_emu.index").unwrap())
            .unwrap()
            .records;
        let mut got: Vec<(String, u64)> = index;
        let mut want: Vec<(String, u64)> = (tree.files().iter())
            .filter(|f| f.path != "ampr_emu.index")
            .map(|f| (f.path.clone(), f.size))
            .collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{rebuilt}");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// [`Jobs::edit_check`] on a table with no job.
fn edit_check(req: &ConvertRequest) -> ps5_dump_forge_core::EditCheck {
    Jobs::new(|_| {}).edit_check(req)
}

#[test]
fn a_ffpfs_is_edited_in_place_within_its_runs() {
    use ps5_dump_forge_core::Edit;
    let root = dir("edit-ffpfs");
    let src = root.join("src");
    game(&src);
    let image = rebuilt_image(&root, &src, ("ffpfs", Format::Ffpfs, None));
    let size = std::fs::metadata(&image).unwrap().len();
    let host = root.join("host");
    // As big as data/big.bin (served from the host file), and a shorter eboot.
    let big: Vec<u8> = (0..9 * 1024 * 1024 + 7).map(|i| (i % 241) as u8).collect();
    write(&host, "big.bin", &big);
    write(&host, "eboot.bin", b"\x7fELF eboot");
    let put = |from: &str, to: &str| Edit::Add {
        from: host.join(from),
        to: to.into(),
    };
    let rename = |from: &str, to: &str| Edit::Rename {
        from: from.into(),
        to: to.into(),
    };
    let in_place = |edits| ConvertRequest {
        edit_rebuild: false,
        ..rebuild(&image, edits)
    };
    let edits = vec![
        put("big.bin", "data/big.bin"),
        put("eboot.bin", "eboot.bin"),
        Edit::Delete {
            path: "data/empty".into(),
        },
        rename("data/zero.bin", "zero.bin"),
        rename("data", "assets"),
    ];
    let check = edit_check(&in_place(edits.clone()));
    assert_eq!(check.in_place, Vec::<String>::new());
    // PFS keeps no free count: no sizes inside.
    assert_eq!((check.needed_inside, check.free_inside), (None, None));
    let report = run(in_place(edits)).1.unwrap();
    assert_eq!(report.output, image);
    assert!(
        report.checks.iter().any(|c| c.starts_with("blake3:")),
        "{report:?}"
    );
    // A second edit, fully verified: a file moved into a renamed folder and back out.
    let mut req = in_place(vec![
        rename("zero.bin", "sce_sys/zero.bin"),
        rename("sce_sys/zero.bin", "zero.bin"),
    ]);
    req.full_verify = true;
    run(req).1.unwrap();
    assert_eq!(std::fs::metadata(&image).unwrap().len(), size);
    assert!(!ps5_dump_forge_patch::journal_path(&image).exists());
    let mut tree = ps5_dump_forge_pfs::PfsSource::open(&image).unwrap();
    let mut paths: Vec<String> = tree.files().iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "assets/big.bin",
            "eboot.bin",
            "sce_sys/param.json",
            "zero.bin"
        ]
    );
    assert!(tree.read("assets/big.bin").unwrap() == big);
    assert_eq!(tree.read("eboot.bin").unwrap(), b"\x7fELF eboot");
    assert_eq!(tree.read("zero.bin").unwrap(), b"");
    assert!(tree.empty_dirs().is_empty(), "{:?}", tree.empty_dirs());
    drop(tree);

    // A new file, a folder, a file past its run (eboot.bin holds one 64 KiB block), each on
    // its own and all together: refused in place (said once), a rebuild offered.
    write(&host, "run.bin", &[1u8; 65_536]);
    write(&host, "past.bin", &[1u8; 65_537]);
    let refusals = [
        vec![put("eboot.bin", "assets/new.bin")],
        vec![Edit::Mkdir {
            path: "more".into(),
        }],
        vec![put("past.bin", "eboot.bin")],
    ];
    let all = refusals.concat();
    for edits in refusals.into_iter().chain([all]) {
        let check = edit_check(&in_place(edits.clone()));
        assert_eq!(
            check.in_place,
            [ps5_dump_forge_pfs::edit::NO_ROOM],
            "{edits:?}"
        );
        assert_eq!(check.rebuild, Vec::<String>::new(), "{edits:?}");
        assert!(check.rebuild_needs.is_some(), "{edits:?}");
    }
    // The whole run still fits.
    let check = edit_check(&in_place(vec![put("run.bin", "eboot.bin")]));
    assert_eq!(check.in_place, Vec::<String>::new());
    let _ = std::fs::remove_dir_all(root);
}

// ponytail: one smoke test for folder edits (the MVP cut stopped Task 8 before its own tests).
#[test]
fn a_game_folder_is_edited_directly() {
    use ps5_dump_forge_core::Edit;
    let root = dir("edit-folder");
    let src = root.join("src");
    game(&src);
    let host = root.join("host");
    write(&host, "new.bin", b"new file");
    write(&host, "eboot.bin", b"\x7fELF new eboot");
    let req = |edits| ConvertRequest {
        edits,
        edit_rebuild: false,
        ..request(&src, Format::Folder, &root.join("ignored"))
    };
    let edits = vec![
        Edit::Add {
            from: host.join("new.bin"),
            to: "data/sub/new.bin".into(),
        },
        Edit::Add {
            from: host.join("eboot.bin"),
            to: "eboot.bin".into(),
        },
        Edit::Delete {
            path: "data/zero.bin".into(),
        },
        Edit::Rename {
            from: "data/empty".into(),
            to: "data/renamed".into(),
        },
        Edit::Mkdir {
            path: "extra".into(),
        },
    ];
    let check = edit_check(&req(edits.clone()));
    assert_eq!(check.in_place, Vec::<String>::new());
    run(req(edits)).1.unwrap();
    assert_eq!(
        std::fs::read(src.join("data/sub/new.bin")).unwrap(),
        b"new file"
    );
    assert_eq!(
        std::fs::read(src.join("eboot.bin")).unwrap(),
        b"\x7fELF new eboot"
    );
    assert!(!src.join("data/zero.bin").exists());
    assert!(!src.join("data/empty").exists());
    assert!(src.join("data/renamed").is_dir());
    assert!(src.join("extra").is_dir());
    assert_eq!(
        std::fs::read(src.join("data/big.bin")).unwrap().len(),
        9 * 1024 * 1024 + 7
    );
    // A refused edit changes nothing.
    let bad = vec![Edit::Delete {
        path: "missing.bin".into(),
    }];
    assert!(run(req(bad)).1.is_err());
    assert!(src.join("extra").is_dir());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn extract_copies_files_and_folders_out_of_a_folder_or_an_image() {
    let root = dir("extract");
    let src = root.join("src");
    game(&src);
    write(&src, "data/sub/deep.bin", b"deep");
    let image = rebuilt_image(&root, &src, ("exfat", Format::Exfat, None));
    let extract = |source: &Path, out: &Path, paths: &[&str], full: bool| {
        let mut req = request(source, Format::Folder, out);
        req.extract = paths.iter().map(|p| p.to_string()).collect();
        req.full_verify = full;
        run(req).1
    };
    for (label, source, full) in [("folder", &src, false), ("exfat", &image, true)] {
        let out = root.join(format!("out-{label}"));
        std::fs::create_dir(&out).unwrap();
        let report = extract(source, &out, &["eboot.bin", "data"], full)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(report.output, dunce::canonicalize(&out).unwrap(), "{label}");
        assert_eq!(report.files, 4, "{label}: eboot + big, zero, deep");
        let mode = if full {
            VerifyMode::Full
        } else {
            VerifyMode::Fast
        };
        assert_eq!(report.verify.mode, mode, "{label}");
        assert_eq!(
            std::fs::read(out.join("eboot.bin")).unwrap(),
            std::fs::read(src.join("eboot.bin")).unwrap(),
            "{label}"
        );
        assert_eq!(
            tree_bytes(&out.join("data")),
            tree_bytes(&src.join("data")),
            "{label}"
        );
        assert!(
            out.join("data/empty").is_dir(),
            "{label}: empty folder kept"
        );
        assert!(!out.join("data/._big.bin").exists(), "{label}: no junk");
        // Only the two selections and no temporary file are left in the destination.
        let mut names: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["data", "eboot.bin"], "{label}");

        // A target that exists is never replaced.
        let before = snapshot(&out);
        let err = extract(source, &out, &["data/big.bin", "eboot.bin"], false).unwrap_err();
        assert!(err.contains("preflight failed"), "{label}: {err}");
        assert!(err.contains("eboot.bin already exists"), "{label}: {err}");
        assert_eq!(snapshot(&out), before, "{label}: nothing written");
    }
    let out = root.join("out-refused");
    std::fs::create_dir(&out).unwrap();
    let cases: [(&[&str], &str); 5] = [
        (&["data/nope.bin"], "data/nope.bin is not in the game"),
        (
            &["data/big.bin", "data/big.bin"],
            "would both be named big.bin",
        ),
        (&["data", "data/sub"], "data/sub is inside data"),
        (&["data/../eboot.bin"], "'..' component"),
        (&[""], "an empty path"),
    ];
    for (paths, want) in cases {
        let err = extract(&src, &out, paths, false).unwrap_err();
        assert!(err.contains(want), "{paths:?}: {err}");
    }
    // Into a missing folder, or into the game folder itself.
    let err = extract(&src, &root.join("missing"), &["eboot.bin"], false).unwrap_err();
    assert!(err.contains("missing"), "{err}");
    let err = extract(&src, &src.join("data"), &["eboot.bin"], false).unwrap_err();
    assert!(err.contains("inside the game folder"), "{err}");
    assert_eq!(
        std::fs::read_dir(&out).unwrap().count(),
        0,
        "nothing written"
    );
}

/// Asked first ([`Jobs::existing_targets`]) and confirmed (`replace`), Extract puts a file over
/// a file and merges a folder into a folder; a kind mismatch at any level refuses it all.
#[test]
fn extract_replaces_what_exists_once_asked() {
    let root = dir("extract-replace");
    let src = root.join("src");
    game(&src);
    write(&src, "data/sub/deep.bin", b"deep");
    let out = root.join("out");
    write(&out, "eboot.bin", b"old eboot");
    write(&out, "data/zero.bin", b"old zero");
    write(&out, "data/mine.bin", b"keep");
    let req = |out: &Path, replace: bool| {
        let mut req = request(&src, Format::Folder, out);
        req.extract = vec!["eboot.bin".into(), "data".into(), "sce_sys".into()];
        req.replace = replace;
        req
    };
    let jobs = Jobs::new(|_| {});
    assert_eq!(
        jobs.existing_targets(&req(&out, false)).unwrap(),
        [out.join("eboot.bin"), out.join("data")]
    );
    let convert = request(&src, Format::Folder, &out);
    assert!(jobs.existing_targets(&convert).is_err(), "not an extract");
    assert!(
        jobs.existing_targets(&req(&root.join("missing"), false))
            .is_err()
    );

    // Without replace: refused as before, nothing written.
    let before = snapshot(&out);
    let err = run(req(&out, false)).1.unwrap_err();
    assert!(
        err.contains("eboot.bin already exists (never replaced)"),
        "{err}"
    );
    assert_eq!(snapshot(&out), before);

    let (events, result) = run(req(&out, true));
    let report = result.unwrap();
    assert_eq!(report.files, 5, "eboot, param.json, big, zero, deep");
    assert_eq!(
        std::fs::read(out.join("eboot.bin")).unwrap(),
        std::fs::read(src.join("eboot.bin")).unwrap()
    );
    assert_eq!(std::fs::read(out.join("data/zero.bin")).unwrap(), b"");
    assert_eq!(std::fs::read(out.join("data/mine.bin")).unwrap(), b"keep");
    assert_eq!(
        std::fs::read(out.join("data/sub/deep.bin")).unwrap(),
        b"deep"
    );
    assert!(out.join("data/empty").is_dir() && out.join("sce_sys/param.json").is_file());
    let logged = events.iter().any(
        |e| matches!(e, Event::Log { line, .. } if line.starts_with("replace: 2 files replaced: ")),
    );
    assert!(logged, "{events:?}");

    // A folder where a file goes, a file where a folder goes: refused, nothing written.
    let other = root.join("other");
    std::fs::create_dir_all(other.join("eboot.bin")).unwrap();
    write(&other, "data/sub", b"a file");
    let before = snapshot(&other);
    let err = run(req(&other, true)).1.unwrap_err();
    assert!(
        err.contains("eboot.bin is a folder: a file is not put in its place"),
        "{err}"
    );
    assert!(
        err.contains("sub is a file: a folder is not put in its place"),
        "{err}"
    );
    assert_eq!(snapshot(&other), before);
    let _ = std::fs::remove_dir_all(root);
}

/// Every name in `dir`, recursively, relative to it, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let rel = p.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
        if p.is_dir() {
            out.extend(names_in(&p).into_iter().map(|n| format!("{rel}/{n}")));
        }
        out.push(rel);
    }
    out.sort();
    out
}

// No eboot.bin but a param.json with a title id is additional content (a DLC, an update), by
// user decision: no finding, fpkg or a folder only, every edit allowed, listed by the Library.
#[test]
fn additional_content_converts_to_fpkg_or_a_folder_and_takes_edits() {
    use ps5_dump_forge_core::Edit;
    let root = dir("no-eboot");
    let src = root.join("src");
    game(&src);
    std::fs::remove_file(src.join("eboot.bin")).unwrap();
    let seen = inspect(&src).unwrap();
    assert!(seen.additional_content);
    assert!(
        !seen.findings.iter().any(|f| f.contains("eboot.bin")),
        "{:?}",
        seen.findings
    );
    run(request(&src, Format::Pkg, &root.join("dlc.pkg")))
        .1
        .unwrap();
    assert!(inspect(&root.join("dlc.pkg")).unwrap().additional_content);
    run(request(&src, Format::Folder, &root.join("copy")))
        .1
        .unwrap();
    for format in [Format::Exfat, Format::Ffpkg, Format::Lz4] {
        let err = run(request(&src, format, &root.join("refused")))
            .1
            .unwrap_err();
        assert!(
            err.contains("additional content (no eboot.bin) can only be packaged as fpkg"),
            "{format:?}: {err}"
        );
    }

    // An image of a game whose eboot.bin an edit deletes, then a second edit on it.
    let full = root.join("full");
    game(&full);
    let image = root.join("game.exfat");
    run(request(&full, Format::Exfat, &image)).1.unwrap();
    assert!(!inspect(&image).unwrap().additional_content);
    let edit = |edits| ConvertRequest {
        edits,
        ..request(&image, Format::Pkg, &root.join("ignored"))
    };
    let delete = edit(vec![Edit::Delete {
        path: "eboot.bin".into(),
    }]);
    let check = edit_check(&delete);
    assert!(
        check.in_place.is_empty() && check.rebuild.is_empty(),
        "{check:?}"
    );
    run(delete).1.unwrap();
    assert!(inspect(&image).unwrap().additional_content);
    let mkdir = edit(vec![Edit::Mkdir {
        path: "more".into(),
    }]);
    let check = edit_check(&mkdir);
    assert!(
        check.in_place.is_empty() && check.rebuild.is_empty(),
        "{check:?}"
    );
    run(mkdir).1.unwrap();
    run(rebuild(
        &image,
        vec![Edit::Delete {
            path: "more".into(),
        }],
    ))
    .1
    .unwrap();
    let seen = inspect(&image).unwrap();
    assert!(
        seen.additional_content && !seen.findings.iter().any(|f| f.contains("eboot.bin")),
        "{:?}",
        seen.findings
    );

    // The Library lists the folder and the image, flagged.
    std::fs::remove_dir_all(&full).unwrap();
    std::fs::remove_dir_all(root.join("copy")).unwrap();
    let scan = ps5_dump_forge_core::scan_library_folder(&root).unwrap();
    let mut listed: Vec<(String, bool)> = scan
        .entries
        .iter()
        .map(|e| {
            let name = e.path.file_name().unwrap().to_string_lossy().into_owned();
            (name, e.additional_content)
        })
        .collect();
    listed.sort();
    let want = [("dlc.pkg", true), ("game.exfat", true), ("src", true)];
    let want: Vec<(String, bool)> = want.iter().map(|(n, f)| (n.to_string(), *f)).collect();
    assert_eq!(listed, want, "{:?}", scan.warnings);
    let _ = std::fs::remove_dir_all(root);
}
