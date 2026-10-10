//! End-to-end checks of the public core API with the Folder target and the readers.
//! Image targets run through the real exFAT and UFS2 writers, `.pkg` through the FPKG builder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ps5_dump_forge_core::{
    ConvertRequest, DataDirs, DeleteError, Event, Format, JobReport, Jobs, KrakenLevel,
    ScannedFolder, VerifyMode, default_output, extraction_findings, inspect, rename_no_replace,
    stale_parts,
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
        lz4_profile: None,
        lz4_traces: None,
        lz4_trace_space_mib: 256,
        lz4_in_place: false,
    }
}

/// Runs one job to completion; returns its events and result.
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
            return (events, result);
        }
        events.push(event);
    }
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

#[test]
fn scanner_filters_junk_and_keeps_empty_dirs() {
    let root = dir("scan-junk");
    game(&root);
    std::fs::create_dir_all(root.join("only-junk")).unwrap();
    write(&root, "only-junk/Thumbs.db", b"x");
    let tree = ScannedFolder::scan(&root, &AtomicBool::new(false)).unwrap();
    let paths: Vec<&str> = tree.files().iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "data/big.bin",
            "data/zero.bin",
            "eboot.bin",
            "sce_sys/param.json"
        ]
    );
    assert_eq!(tree.empty_dirs(), ["data/empty", "only-junk"]);
    assert!(root.join(".DS_Store").exists(), "junk is never deleted");
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

#[cfg(unix)]
#[test]
fn scanner_refuses_non_utf8_names() {
    use std::os::unix::ffi::OsStrExt;
    let root = dir("scan-utf8");
    game(&root);
    let name = std::ffi::OsStr::from_bytes(b"bad\xff.bin");
    if std::fs::write(root.join(name), b"x").is_err() {
        return; // APFS refuses such names itself
    }
    let err = ScannedFolder::scan(&root, &AtomicBool::new(false))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("not valid UTF-8"), "{err}");
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
fn stale_parts_are_listed_not_deleted() {
    let root = dir("stale");
    write(&root, "GAME.exfat.3-99.part", b"x");
    std::fs::create_dir(root.join("GAME.1-99.part")).unwrap();
    write(&root, "GAME.exfat", b"x");
    let parts = stale_parts(&root);
    assert_eq!(
        parts,
        [
            root.join("GAME.1-99.part"),
            root.join("GAME.exfat.3-99.part")
        ]
    );
    assert!(parts.iter().all(|p| p.exists()));
    assert!(stale_parts(&root.join("missing")).is_empty());
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
    assert_eq!(report.output, out.canonicalize().unwrap());
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
    assert_eq!(report.output, pkg.canonicalize().unwrap());
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
fn pkg_refuses_patches_dlc_and_bad_ids() {
    let root = dir("pkg-refuse");
    let src = root.join("src");
    game(&src);
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST","applicationCategoryType":33554432}"#,
    );
    let (_, result) = run(request(&src, Format::Pkg, &root.join("a.pkg")));
    let err = result.unwrap_err();
    assert!(err.contains("patches and DLC are not supported"), "{err}");

    std::fs::remove_file(src.join("eboot.bin")).unwrap();
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-SHORT"}"#,
    );
    let (_, result) = run(request(&src, Format::Pkg, &root.join("b.pkg")));
    let err = result.unwrap_err();
    for want in ["additional content (DLC)", "content id:"] {
        assert!(err.contains(want), "{want:?} not in:\n{err}");
    }
    assert!(stale_parts(&root).is_empty());
    assert!(!root.join("a.pkg").exists() && !root.join("b.pkg").exists());
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

#[test]
fn bad_pkg_source_is_refused() {
    let root = dir("pkg-bad");
    write(&root, "in.pkg", b"not a package");
    let (_, result) = run(request(
        &root.join("in.pkg"),
        Format::Folder,
        &root.join("o"),
    ));
    let err = result.unwrap_err();
    assert!(err.contains("in.pkg: not a PS5 package"), "{err}");
    assert!(stale_parts(&root).is_empty());
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
fn cancel_folder_removes_only_this_jobs_part() {
    cancel_removes_only_this_jobs_part(Format::Folder, "out", "write");
}

#[test]
fn cancel_ffpfsc_removes_only_this_jobs_part() {
    // Mid-stream: the inner exFAT writer is held at its first file data.
    cancel_removes_only_this_jobs_part(Format::Ffpfsc, "out.ffpfsc", "write");
}

#[test]
fn cancel_pkg_removes_only_this_jobs_part() {
    cancel_removes_only_this_jobs_part(Format::Pkg, "out.pkg", "compress");
}

#[test]
fn jobs_run_one_at_a_time_in_order() {
    let root = dir("queue");
    let src = root.join("src");
    game(&src);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let jobs = Jobs::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let a = jobs.start(request(&src, Format::Folder, &root.join("a")));
    let b = jobs.start(request(&src, Format::Folder, &root.join("b")));
    let mut order = Vec::new();
    let mut done = 0;
    while done < 2 {
        match rx.recv().unwrap() {
            Event::Progress { job, .. } if order.last() != Some(&(job, false)) => {
                order.push((job, false))
            }
            Event::Done { job, result } => {
                result.unwrap();
                order.push((job, true));
                done += 1;
            }
            _ => {}
        }
    }
    assert_eq!(order, [(a, false), (a, true), (b, false), (b, true)]);
    jobs.cancel_all_and_wait();
}

#[test]
fn cancel_all_while_queued() {
    let root = dir("cancel-all");
    let src = root.join("src");
    game(&src);
    let results = Arc::new(Mutex::new(Vec::new()));
    let sink = results.clone();
    let jobs = Jobs::new(move |e| {
        if let Event::Done { result, .. } = e {
            sink.lock().unwrap().push(result.map(|_| ()));
        }
    });
    for name in ["a", "b", "c"] {
        jobs.start(request(&src, Format::Folder, &root.join(name)));
    }
    jobs.cancel_all_and_wait();
    let results = results.lock().unwrap();
    assert_eq!(results.len(), 3);
    assert!(stale_parts(&root).is_empty());
}

#[test]
fn inspect_and_name_a_folder() {
    let root = dir("inspect");
    let src = root.join("src");
    game(&src);
    let found = inspect(&src).unwrap();
    assert_eq!(found.kind, "folder");
    assert_eq!(found.title_id.as_deref(), Some("PPSA01234"));
    assert_eq!(found.files.len(), 4);
    assert_eq!(found.empty_dirs, ["data/empty"]);
    assert!(found.findings.is_empty(), "{:?}", found.findings);
    assert_eq!(
        default_output(&src, Format::Exfat, &root).unwrap(),
        root.join("PPSA01234.exfat")
    );
    assert_eq!(
        default_output(&src, Format::Folder, &root).unwrap(),
        root.join("PPSA01234")
    );
    let plain = root.join("plain");
    write(&plain, "eboot.bin", b"x");
    assert_eq!(
        default_output(&plain, Format::Ffpkg, &root).unwrap(),
        root.join("plain.ffpkg")
    );
}

#[test]
fn data_dirs_for_an_app_bundle() {
    let root = dir("portable");
    let exe = root.join("Forge.app/Contents/MacOS/forge");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    let dirs = DataDirs::resolve(&exe);
    assert_eq!(dirs.root, root);
    assert_eq!(dirs.webview(), root.join("data/webview"));
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
            let (ext, geometry) = match inner.unwrap_or(Format::Exfat) {
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
fn exfat_round_trip() {
    let (root, ..) = image_round_trip(Format::Exfat, None, "exfat");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ffpkg_round_trip() {
    let (root, ..) = image_round_trip(Format::Ffpkg, None, "ffpkg");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ffpfs_round_trip_and_to_ffpkg() {
    let (root, src, image) = image_round_trip(Format::Ffpfs, None, "ffpfs");
    // Straight from one image to another.
    let ffpkg = root.join("again.ffpkg");
    let report = run(request(&image, Format::Ffpkg, &ffpkg)).1.unwrap();
    assert!(report.checks.iter().any(|c| c == "empty dirs: 3 match"));
    let back = root.join("back-ffpkg");
    run(request(&ffpkg, Format::Folder, &back)).1.unwrap();
    assert_eq!(tree_bytes(&src), tree_bytes(&back));
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

/// An in-memory tree, for images whose names a folder on this disk cannot hold.
struct Mem(Vec<ps5upload_fpkg::source::SourceFile>);

impl SourceTree for Mem {
    fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
        &self.0
    }
    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        if path == "sce_sys/param.json" {
            return Ok(
                br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#
                    .to_vec(),
            );
        }
        let size = self.0.iter().find(|f| f.path == path).map_or(0, |f| f.size);
        Ok(vec![0x5a; size as usize])
    }
    fn describe(&self) -> String {
        "mem".into()
    }
}

#[test]
fn junk_inside_an_image_is_dropped_everywhere() {
    let root = dir("image-junk");
    // The UFS2 writer filters nothing itself, so this image carries macOS junk, as one made
    // by another tool can. `.Trashes` is not on the vendored readers' own junk list.
    let file = |p: &str, size| ps5upload_fpkg::source::SourceFile {
        path: p.into(),
        size,
    };
    let param = br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#;
    let mut tree = Mem(vec![
        file("eboot.bin", 64),
        file("sce_sys/param.json", param.len() as u64),
        file(".Trashes/501/old.bin", 100),
        file("only-junk/.Trashes", 3),
        file("data/keep.bin", 5),
        file("data/Thumbs.db", 5),
    ]);
    let cancel = AtomicBool::new(false);
    let opts = ps5_dump_forge_ufs2::Options::default();
    let layout = ps5_dump_forge_ufs2::plan(&tree, &opts, &cancel).unwrap();
    let image = root.join("junk.ffpkg");
    let mut out = std::fs::File::create_new(&image).unwrap();
    ps5_dump_forge_ufs2::write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {}).unwrap();
    drop(out);

    let found = inspect(&image).unwrap();
    assert_eq!(found.forge_version, None, "written without a maker's mark");
    let paths: Vec<&str> = found.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["data/keep.bin", "eboot.bin", "sce_sys/param.json"]);
    assert_eq!(found.empty_dirs, ["only-junk"]);
    for (format, name) in [
        (Format::Folder, "out"),
        (Format::Exfat, "out.exfat"),
        (Format::Ffpkg, "out.ffpkg"),
    ] {
        let report = run(request(&image, format, &root.join(name))).1.unwrap();
        assert_eq!(report.files, 3, "{format:?}");
        assert!(report.checks.iter().any(|c| c == "empty dirs: 1 match"));
        let back = inspect(&root.join(name)).unwrap();
        let mark = (format != Format::Folder).then(|| env!("CARGO_PKG_VERSION").to_string());
        assert_eq!(back.forge_version, mark, "{format:?}");
        assert!(
            back.files.iter().all(|f| !f.path.contains(".Trashes")),
            "{format:?} carried junk"
        );
    }
    assert!(!root.join("out/.Trashes").exists());
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
use ps5_dump_forge_lz4::runtime::{RELEASE, TRACE};

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
    assert!(logged(&events, "LZ4 rules: a built-in guess"), "{events:?}");
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

/// One AMPRCMD1 record (domain APR) of one ReadFile packet per id in `ids`.
fn journal_record(seq: u64, ids: &[u32]) -> Vec<u8> {
    let payload: Vec<u8> = ids
        .iter()
        .flat_map(|&id| [40 | 4 << 8, 4095, id, 0, 0])
        .flat_map(|w: u32| w.to_le_bytes())
        .collect();
    let mut h = vec![0u8; 96];
    h[..8].copy_from_slice(b"AMPRCMD1");
    h[8..10].copy_from_slice(&1u16.to_le_bytes());
    h[10..12].copy_from_slice(&96u16.to_le_bytes());
    h[12..16].copy_from_slice(&(96 + payload.len() as u32).to_le_bytes());
    h[16..24].copy_from_slice(&seq.to_le_bytes());
    let hash = ps5_dump_forge_lz4::format::cmd_hash(&payload);
    h[56..64].copy_from_slice(&hash.to_le_bytes());
    h[64..68].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    h[80..84].copy_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&payload);
    h
}

/// A traced dump of [`ampr_game`] in `root/traced`, after a "session" that read `read`.
fn traced(root: &Path, read: &[&str]) -> PathBuf {
    traced_from(root, ampr_game, read)
}

/// [`traced`] for the game `make` writes.
fn traced_from(root: &Path, make: fn(&Path), read: &[&str]) -> PathBuf {
    let src = root.join("src");
    make(&src);
    let traced = root.join("traced");
    let (events, result) = run(lz4_request(
        &src,
        Format::Folder,
        &traced,
        Some(Lz4Mode::Trace),
    ));
    result.unwrap();
    assert!(logged(&events, ps5_dump_forge_lz4::runtime::WARNING));
    assert!(logged(&events, "the journal and logs grow"));
    assert!(std::fs::read(traced.join("fakelib/libSceAmpr.sprx")).unwrap() == TRACE);
    let records = index_paths(&traced);
    assert!(records.contains(&("fakelib/libSceAmpr.sprx".to_string(), TRACE.len() as u64)));
    assert_eq!(assets(&traced), tree_bytes(&src));
    let ids: Vec<u32> = read
        .iter()
        .map(|p| records.iter().position(|(r, _)| r == p).unwrap() as u32 + 1)
        .collect();
    let mut journal = journal_record(1, &ids);
    journal.extend(journal_record(2, &[]));
    write(&traced, "ampr_commands.bin", &journal);
    write(&traced, "ampr_emu.log", b"session log");
    traced
}

#[test]
fn traces_pick_what_lz4_packs() {
    let root = dir("lz4-traces");
    let traced = traced(&root, &["data/assets/level1.bin", "eboot.bin"]);
    let packed = root.join("packed");
    let (events, result) = run(ConvertRequest {
        full_verify: true,
        ..request(&traced, Format::Lz4, &packed)
    });
    let report = result.unwrap();
    assert!(
        logged(&events, "this dump's traces (2 files the game read)"),
        "{events:?}"
    );
    assert!(
        logged(&events, "LZ4 traces: 2 records read, 0 skipped"),
        "{events:?}"
    );
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.ends_with("files match the source"))
    );
    // Observed and eligible: packed. Unobserved: loose. eboot.bin: loose whatever was read.
    assert!(!packed.join("data/assets/level1.bin").exists());
    for loose in ["data/big.bin", "data/assets/noise.bin", "eboot.bin"] {
        assert!(packed.join(loose).is_file(), "{loose}");
    }
    assert!(!packed.join("ampr_commands.bin").exists() && !packed.join("ampr_emu.log").exists());
    assert!(std::fs::read(packed.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    let records = index_paths(&packed);
    assert!(records.contains(&("fakelib/libSceAmpr.sprx".to_string(), RELEASE.len() as u64)));
    assert!(!records.iter().any(|(p, _)| p == "ampr_commands.bin"));
    let _ = std::fs::remove_dir_all(root);
}

/// A traced dump's index lists its backport too: the traces are checked against the whole
/// source, then the backport is left out of the packed folder.
#[test]
fn traces_survive_leaving_the_backport_out() {
    let root = dir("lz4-traces-backport");
    let traced = traced_from(
        &root,
        |src| {
            ampr_game(src);
            let mut exe = eboot(0x1200_0038);
            exe.extend_from_slice(b"libSceAmpr");
            write(src, "eboot.bin", &exe);
            write(
                src,
                "sce_sys/param.json",
                br#"{"titleId":"PPSA01234","sdkVersion":"0x1200000000000000"}"#,
            );
            write(src, "fakelib/libSceAgc.sprx", b"\x7fELF a system library");
        },
        &["data/assets/level1.bin"],
    );
    assert!(
        index_paths(&traced)
            .iter()
            .any(|(p, _)| p == "fakelib/libSceAgc.sprx")
    );
    let packed = root.join("packed");
    let (events, result) = run(ConvertRequest {
        remove_backport: true,
        ..request(&traced, Format::Lz4, &packed)
    });
    result.unwrap();
    assert!(
        logged(&events, "this dump's traces (1 files the game read)"),
        "{events:?}"
    );
    assert!(logged(
        &events,
        "remove backport: leaving out fakelib/libSceAgc.sprx"
    ));
    assert!(!packed.join("fakelib/libSceAgc.sprx").exists());
    assert!(!packed.join("data/assets/level1.bin").exists());
    assert!(packed.join("data/big.bin").is_file());
    let records = index_paths(&packed);
    assert!(!records.iter().any(|(p, _)| p == "fakelib/libSceAgc.sprx"));
    assert!(records.iter().any(|(p, _)| p == "data/assets/level1.bin"));
    let _ = std::fs::remove_dir_all(root);
}

/// The LZ4 target always writes its own `ampr_emu.index` (the space bound counts exactly that),
/// even when the source's lists the same files.
#[test]
fn lz4_rebuilds_a_matching_index() {
    let root = dir("lz4-own-index");
    let src = root.join("src");
    ampr_game(&src);
    write(&src, "fakelib/libSceAmpr.sprx", RELEASE);
    let rows: Vec<(String, u64)> = tree_bytes(&src)
        .into_iter()
        .map(|(p, b)| (p, b.len() as u64))
        .collect();
    let old = ps5upload_fpkg::ampr_index::build(&rows, 1).unwrap();
    write(&src, "ampr_emu.index", &old);
    let packed = root.join("packed");
    let (events, result) = run(request(&src, Format::Lz4, &packed));
    result.unwrap();
    assert!(
        logged(&events, "writing a new ampr_emu.index"),
        "{events:?}"
    );
    assert!(!logged(&events, "keeping ampr_emu.index"));
    assert!(std::fs::read(packed.join("ampr_emu.index")).unwrap() != old);
    assert_eq!(index_paths(&packed), index_paths(&src));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_plain_conversion_swaps_the_trace_runtime() {
    let root = dir("lz4-swap");
    let traced = traced(&root, &["data/big.bin"]);
    let image = root.join("PPSA01234.exfat");
    let (events, result) = run(request(&traced, Format::Exfat, &image));
    result.unwrap();
    assert!(
        logged(&events, "swapping the tracing runtime"),
        "{events:?}"
    );
    assert!(logged(&events, "leaving out ampr_commands.bin"));
    let back = root.join("back");
    run(request(&image, Format::Folder, &back)).1.unwrap();
    assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    assert!(!back.join("ampr_commands.bin").exists() && !back.join("ampr_emu.log").exists());
    let records = index_paths(&back);
    assert!(records.contains(&("fakelib/libSceAmpr.sprx".to_string(), RELEASE.len() as u64)));
    assert_eq!(assets(&back), assets(&root.join("src")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_traced_image_gets_the_requested_room() {
    let root = dir("lz4-trace-room");
    let src = root.join("src");
    ampr_game(&src);
    for ext in ["exfat", "ffpkg"] {
        let format = if ext == "exfat" {
            Format::Exfat
        } else {
            Format::Ffpkg
        };
        let plain = root.join(format!("plain.{ext}"));
        let plain = run(request(&src, format, &plain)).1.unwrap().bytes;
        let image = root.join(format!("traced.{ext}"));
        let (events, result) = run(ConvertRequest {
            lz4_trace_space_mib: 256,
            ..lz4_request(&src, format, &image, Some(Lz4Mode::Trace))
        });
        let traced = result.unwrap().bytes;
        assert!(
            logged(&events, "256 MiB of free space in the image"),
            "{events:?}"
        );
        // Room for the files (the tracing runtime included) and 256 MiB more, which a small
        // plain image's own spare and rounding may partly overlap.
        let files: u64 = tree_bytes(&src).iter().map(|(_, b)| b.len() as u64).sum();
        let files = files + TRACE.len() as u64;
        assert!(traced >= files + (256 << 20), "{ext}: {traced} for {files}");
        assert!(
            traced > plain && traced < plain + (256 << 20) + (64 << 20),
            "{ext}: {traced} vs {plain}"
        );
        // The image holds the tracing runtime (a plain conversion back out would swap it).
        let found = inspect(&image).unwrap();
        let runtime = found
            .files
            .iter()
            .find(|f| f.path == "fakelib/libSceAmpr.sprx");
        assert_eq!(runtime.map(|f| f.size), Some(TRACE.len() as u64));
        std::fs::remove_file(&image).unwrap();
    }
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
    let trace = Some(Lz4Mode::Trace);
    let only = "LZ4 traces are recorded only in a folder, .exfat or .ffpkg";
    refused(
        lz4_request(&src, Format::Ffpfs, &root.join("t.ffpfs"), trace),
        only,
    );
    refused(
        lz4_request(&src, Format::Pkg, &root.join("t.pkg"), trace),
        only,
    );
    refused(
        lz4_request(&src, Format::Lz4, &root.join("u"), Some(Lz4Mode::Unpack)),
        "redundant",
    );
    refused(
        ConvertRequest {
            lz4_trace_space_mib: 100,
            ..lz4_request(&src, Format::Exfat, &root.join("t.exfat"), trace)
        },
        "64 MiB to 1 GiB, in 64 MiB steps, not 100 MiB",
    );
    let toml = root.join("profile.toml");
    std::fs::write(
        &toml,
        "[pack]\ndefault_action = \"compress\"\nbogus_key = 1\n",
    )
    .unwrap();
    refused(
        ConvertRequest {
            lz4_profile: Some(toml.clone()),
            ..request(&src, Format::Folder, &root.join("p"))
        },
        "an LZ4 profile only goes with packing (LZ4 Pack)",
    );
    refused(
        ConvertRequest {
            lz4_profile: Some(toml),
            ..request(&src, Format::Lz4, &root.join("p"))
        },
        "bogus_key",
    );

    // A title that does not use AMPR.
    let plain = root.join("plain");
    game(&plain);
    let no_ampr = "does not import libSceAmpr";
    refused(request(&plain, Format::Lz4, &root.join("n")), no_ampr);
    refused(
        lz4_request(&plain, Format::Folder, &root.join("n"), trace),
        no_ampr,
    );

    // Traces from another file set: an indexed file the dump lacks; the journal's ids would
    // name the wrong files.
    let lz4 = root.join("lz4");
    std::fs::create_dir_all(&lz4).unwrap();
    let traced = traced(&lz4, &["data/big.bin"]);
    let saved = std::fs::read(traced.join("readme.txt")).unwrap();
    std::fs::remove_file(traced.join("readme.txt")).unwrap();
    refused(
        request(&traced, Format::Lz4, &root.join("m")),
        "do not belong to this dump",
    );
    write(&traced, "readme.txt", &saved);

    // A packed folder's backport is in its manifest: left out only from an unpacked view.
    let packed = root.join("packed");
    run(request(&traced, Format::Lz4, &packed)).1.unwrap();
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

    // A packed folder that carries the tracing runtime cannot be converted as it is.
    write(&packed, "fakelib/libSceAmpr.sprx", TRACE);
    refused(
        request(&packed, Format::Folder, &root.join("k")),
        "carries Forge's LZ4 trace runtime",
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancel_lz4_removes_only_this_jobs_part() {
    cancel_removes_only_this_jobs_part_of(ampr_game, Format::Lz4, "out-lz4", "pack");
}

/// Runs an `Lz4` job (with the TOML `profile`, if any) and applies `damage` to its `.part`
/// folder before verification; returns the job's error.
fn damaged_lz4(name: &str, profile: Option<&str>, damage: fn(&Path)) -> String {
    let root = dir(name);
    let src = root.join("src");
    ampr_game(&src);
    let out = root.join("out");
    let lz4_profile = profile.map(|text| {
        let path = root.join("profile.toml");
        std::fs::write(&path, text).unwrap();
        path
    });
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
    jobs.start(ConvertRequest {
        lz4_profile,
        ..request(&src, Format::Lz4, &out)
    });
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

const RUNTIME_PROFILE: &str = "[pack]\ndefault_action = \"compress\"\n[runtime]\n\
    decoded_cache_bytes = 67108864\nphysical_cache_bytes = 33554432\nworkers = 4\n\
    latency_reserve_workers = 1\n";

/// Packs damaged after writing fail verification: no `.part`, no output. A missing sidecar
/// counts: the reader accepts packs without them, but this job wrote them.
#[test]
fn damaged_lz4_output_fails_verification() {
    let err = damaged_lz4("lz4-damaged", None, |part| {
        let manifest = part.join("ampr_assets.index");
        let mut bytes = std::fs::read(&manifest).unwrap();
        bytes[0] ^= 0xff;
        std::fs::write(&manifest, bytes).unwrap();
    });
    assert!(err.contains("reading the output back"), "{err}");
    let err = damaged_lz4("lz4-no-crc", None, |part| {
        std::fs::remove_file(part.join("ampr_assets.index.crc")).unwrap();
    });
    assert!(err.contains("ampr_assets.index.crc is missing"), "{err}");
    let err = damaged_lz4("lz4-no-runtime", Some(RUNTIME_PROFILE), |part| {
        std::fs::remove_file(part.join("ampr_assets.index.runtime")).unwrap();
    });
    assert!(
        err.contains("ampr_assets.index.runtime is missing"),
        "{err}"
    );
}

/// A TOML `[runtime]` table becomes `ampr_assets.index.runtime`, verified as part of the packs.
#[test]
fn lz4_profile_with_a_runtime_table() {
    let root = dir("lz4-runtime-profile");
    let src = root.join("src");
    ampr_game(&src);
    let toml = root.join("profile.toml");
    std::fs::write(&toml, RUNTIME_PROFILE).unwrap();
    let out = root.join("out");
    let (events, result) = run(ConvertRequest {
        lz4_profile: Some(toml),
        ..request(&src, Format::Lz4, &out)
    });
    let report = result.unwrap();
    assert!(logged(&events, "LZ4 rules: the profile"), "{events:?}");
    assert!(out.join("ampr_assets.index.runtime").is_file());
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.starts_with("packs: the manifest, its CRC sidecar, its runtime profile")),
        "{:?}",
        report.checks
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Upstream's auto-loose over `ampr_game` (the profile lowers the 64 MiB minimum to 100 KiB):
/// the incompressible `noise.bin` stays loose, compressible files pack, hot files are exempt
/// unless `auto_loose_hot_files`, and `auto_loose_large_files = false` turns it off. The
/// keep-loose list beats the profile (`readme.txt`).
#[test]
fn lz4_auto_loose_and_the_keep_loose_list() {
    let root = dir("lz4-auto-loose");
    let src = root.join("src");
    ampr_game(&src);
    let pack = |tag: &str, extra: &str| {
        let toml = root.join(format!("{tag}.toml"));
        let body = format!(
            "[pack]\ndefault_action = \"compress\"\nauto_loose_min_file_size = \"100KiB\"\n{extra}"
        );
        std::fs::write(&toml, body).unwrap();
        let out = root.join(tag);
        let (events, result) = run(ConvertRequest {
            lz4_profile: Some(toml),
            ..request(&src, Format::Lz4, &out)
        });
        result.unwrap();
        let loose = |p: &str| out.join(p).is_file();
        (
            events,
            loose("data/assets/noise.bin"),
            loose("data/assets/level1.bin") || loose("data/big.bin"),
        )
    };
    let (events, noise_loose, other_loose) = pack("on", "");
    assert!(noise_loose && !other_loose, "{events:?}");
    assert!(
        logged(
            &events,
            "auto-loose: 1 large files kept loose (incompressible samples): data/assets/noise.bin"
        ),
        "{events:?}"
    );
    assert!(logged(&events, "2 sampled stay packed"), "{events:?}");
    assert!(
        logged(
            &events,
            "LZ4 profile: 1 files the profile packs stay loose (the keep-loose list"
        ) && logged(&events, "AMPR starts): readme.txt"),
        "{events:?}"
    );
    assert!(root.join("on/readme.txt").is_file());

    let (events, noise_loose, _) = pack("off", "auto_loose_large_files = false\n");
    assert!(
        !noise_loose && !logged(&events, "auto-loose: "),
        "{events:?}"
    );

    let hot = "[[rule]]\ninclude = \"data/assets/noise.bin\"\nhot = true\n";
    let (events, noise_loose, _) = pack("hot", hot);
    assert!(!noise_loose, "hot files are exempt by default: {events:?}");
    let (events, noise_loose, _) = pack("hot-on", &format!("auto_loose_hot_files = true\n{hot}"));
    assert!(noise_loose, "{events:?}");

    // A bad value is a finding.
    let toml = root.join("bad.toml");
    std::fs::write(&toml, "[pack]\nauto_loose_max_raw_ratio = 2\n").unwrap();
    let err = run(ConvertRequest {
        lz4_profile: Some(toml),
        ..request(&src, Format::Lz4, &root.join("bad"))
    })
    .1
    .unwrap_err();
    assert!(err.contains("pack.auto_loose_max_raw_ratio"), "{err}");
    let _ = std::fs::remove_dir_all(root);
}

/// Save as profile: the resolved plan as a TOML that, loaded back, packs the same files; nothing
/// is written.
#[test]
fn lz4_plan_profile_round_trips() {
    let root = dir("lz4-plan-profile");
    let src = root.join("src");
    ampr_game(&src);
    let before = snapshot(&src);
    let req = request(&src, Format::Lz4, &root.join("never"));
    let saved = ps5_dump_forge_core::lz4_plan_profile(&req).unwrap();
    assert_eq!(snapshot(&src), before);
    assert!(!root.join("never").exists());
    assert_eq!(saved.file_name, "[PPSA01234]-lz4profile.toml");
    assert_eq!((saved.packed, saved.loose), (3, 5), "{}", saved.toml);
    assert!(
        saved.toml.contains("# Rules from: a built-in guess"),
        "{}",
        saved.toml
    );
    assert!(
        saved
            .log
            .iter()
            .any(|l| l.starts_with("LZ4 rules: a built-in guess"))
    );
    let toml = root.join(&saved.file_name);
    std::fs::write(&toml, &saved.toml).unwrap();
    let out = root.join("out");
    let (events, result) = run(ConvertRequest {
        lz4_profile: Some(toml.clone()),
        ..request(&src, Format::Lz4, &out)
    });
    result.unwrap();
    assert!(logged(&events, "3 files to pack, 5 loose"), "{events:?}");
    for gone in [
        "data/big.bin",
        "data/assets/level1.bin",
        "data/assets/noise.bin",
    ] {
        assert!(!out.join(gone).exists(), "{gone}");
    }
    // Saved from that profile again: the same rules.
    let again = ps5_dump_forge_core::lz4_plan_profile(&ConvertRequest {
        lz4_profile: Some(toml),
        ..req.clone()
    })
    .unwrap();
    let rules = |t: &str| {
        t.lines()
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(rules(&again.toml), rules(&saved.toml));
    // Not a pack request; LZ4 findings are the error.
    let err = ps5_dump_forge_core::lz4_plan_profile(&request(&src, Format::Folder, &out))
        .unwrap_err()
        .to_string();
    assert!(err.contains("LZ4 Pack"), "{err}");
    let mut plain = request(&root.join("plain"), Format::Lz4, &out);
    game(&plain.source);
    plain.output = root.join("o");
    let err = ps5_dump_forge_core::lz4_plan_profile(&plain)
        .unwrap_err()
        .to_string();
    assert!(err.contains("libSceAmpr"), "{err}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lz4_request_fields_default_for_old_requests() {
    let old: ConvertRequest =
        serde_json::from_str(r#"{"source":"/s","format":"exfat","output":"/o.exfat"}"#).unwrap();
    assert_eq!(
        (
            old.lz4,
            old.lz4_profile,
            old.lz4_traces,
            old.lz4_trace_space_mib,
            old.lz4_in_place
        ),
        (None, None, None, 256, false)
    );
    let unpatch: ConvertRequest = serde_json::from_str(
        r#"{"source":"/s.ffpkg","format":"ffpkg","output":"","lz4":"unpatch","lz4_in_place":true}"#,
    )
    .unwrap();
    assert_eq!(
        (unpatch.lz4, unpatch.lz4_in_place),
        (Some(Lz4Mode::Unpatch), true)
    );
    let new: ConvertRequest = serde_json::from_str(
        r#"{"source":"/s","format":"lz4","output":"/o","lz4":"trace","lz4_profile":"/p.toml",
            "lz4_trace_space_mib":192,"lz4_traces":"/t/ampr_commands.bin"}"#,
    )
    .unwrap();
    assert_eq!(
        new.lz4_traces.as_deref(),
        Some(Path::new("/t/ampr_commands.bin"))
    );
    assert_eq!(new.format, Format::Lz4);
    assert_eq!(new.lz4, Some(Lz4Mode::Trace));
    assert_eq!(new.lz4_profile.as_deref(), Some(Path::new("/p.toml")));
    assert_eq!(new.lz4_trace_space_mib, 192);
}

#[test]
fn inspect_reports_lz4_facts() {
    let root = dir("inspect-lz4");
    // A plain title without libSceAmpr: no LZ4 block.
    let plain = root.join("plain");
    game(&plain);
    assert!(inspect(&plain).unwrap().lz4.is_none());

    // Imports libSceAmpr, nothing else: plain, no runtime.
    let src = root.join("src");
    ampr_game(&src);
    let facts = inspect(&src).unwrap().lz4.unwrap();
    assert!(facts.imports_ampr);
    assert!(facts.packed.is_none() && facts.manifest_error.is_none());
    assert_eq!(
        (facts.runtime.as_str(), facts.journal_bytes),
        ("none", None)
    );

    // Other runtime, trace runtime with a journal, release runtime.
    write(&src, "fakelib/libSceAmpr.sprx", b"some other build");
    assert_eq!(inspect(&src).unwrap().lz4.unwrap().runtime, "other");
    write(&src, "fakelib/libSceAmpr.sprx", TRACE);
    write(&src, "ampr_commands.bin", &[0u8; 1234]);
    let facts = inspect(&src).unwrap().lz4.unwrap();
    assert_eq!(facts.runtime, "forge_trace");
    assert_eq!(facts.journal_bytes, Some(1234));
    write(&src, "fakelib/libSceAmpr.sprx", RELEASE);
    assert_eq!(inspect(&src).unwrap().lz4.unwrap().runtime, "forge_release");

    // Artifacts without the import still report (a stray Forge runtime).
    let stray = root.join("stray");
    game(&stray);
    write(&stray, "fakelib/libSceAmpr.sprx", RELEASE);
    let facts = inspect(&stray).unwrap().lz4.unwrap();
    assert!(!facts.imports_ampr);
    assert_eq!(facts.runtime, "forge_release");

    // A packed folder: counts from the manifest, and in the image that holds it too.
    let clean = root.join("clean");
    ampr_game(&clean);
    let packed = root.join("packed");
    let (_, result) = run(request(&clean, Format::Lz4, &packed));
    result.unwrap();
    let found = inspect(&packed).unwrap();
    let facts = found.lz4.clone().unwrap();
    let p = facts.packed.unwrap();
    assert_eq!((p.packed_files, p.volumes), (Some(3), 1), "{p:?}");
    assert!(p.files >= p.packed_files.unwrap());
    assert!(matches!(p.stored_percent, Some(1..=99)), "{p:?}");
    assert_eq!(facts.runtime, "forge_release");
    assert!(!found.findings.iter().any(|f| f.contains("LZ4")));
    let json = serde_json::to_value(&found).unwrap();
    assert_eq!(json["lz4"]["packed"]["volumes"], 1);
    assert_eq!(json["lz4"]["runtime"], "forge_release");
    assert!(json["lz4"]["journal_bytes"].is_null());
    let image = root.join("packed.exfat");
    let (_, result) = run(request(&packed, Format::Exfat, &image));
    result.unwrap();
    assert_eq!(
        inspect(&image)
            .unwrap()
            .lz4
            .unwrap()
            .packed
            .unwrap()
            .volumes,
        1
    );

    // Magic present, rest damaged: a finding, not a failure.
    let mut manifest = std::fs::read(packed.join("ampr_assets.index")).unwrap();
    manifest.truncate(40);
    std::fs::write(packed.join("ampr_assets.index"), manifest).unwrap();
    let found = inspect(&packed).unwrap();
    let facts = found.lz4.unwrap();
    assert!(facts.packed.is_none() && facts.manifest_error.is_some());
    assert!(found.findings.iter().any(|f| f.contains("LZ4 packs")));
}

// ---- LZ4: patching a folder in place, traces copied from it ------------------------------------

use ps5_dump_forge_core::lz4_patch;

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

#[test]
fn lz4_patch_changes_the_folder_in_place() {
    let root = dir("lz4-patch");
    let game = root.join("game");
    ampr_game(&game);
    write(&game, "ampr_commands.bin", b"the last session's journal");
    write(&game, "ampr_emu.log", b"log");
    write(&game, "apr_emu.log", b"log");
    write(&game, "ampr_emu.index", b"an index of other files");
    let done = lz4_patch(&game).unwrap();
    assert!(std::fs::read(game.join("fakelib/libSceAmpr.sprx")).unwrap() == TRACE);
    assert_eq!(
        done.removed,
        ["ampr_commands.bin", "ampr_emu.log", "apr_emu.log"]
    );
    assert_eq!(done.warning, ps5_dump_forge_lz4::runtime::WARNING);
    for gone in done.removed.iter().map(|p| game.join(p)) {
        assert!(!gone.exists(), "{}", gone.display());
    }
    assert_indexes_folder(&game);
    let records = index_paths(&game);
    assert_eq!(done.indexed, records.len());
    assert!(records.contains(&("fakelib/libSceAmpr.sprx".to_string(), TRACE.len() as u64)));
    // Junk stays (never touched) and is not indexed; no temporary file is left.
    assert!(game.join(".DS_Store").is_file());
    assert!(
        !records
            .iter()
            .any(|(p, _)| p.contains("DS_Store") || p.contains("._"))
    );
    let fakelib = std::fs::read_dir(game.join("fakelib")).unwrap().count();
    assert_eq!(fakelib, 1);
    let names = std::fs::read_dir(&game).unwrap();
    assert!(
        !names
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains(".forge-"))
    );
    // As inspect sees it now: traced, no journal yet.
    let facts = inspect(&game).unwrap().lz4.unwrap();
    assert_eq!(
        (facts.runtime.as_str(), facts.journal_bytes),
        ("forge_trace", None)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Whatever runtime is there is replaced by the trace build: another one, Forge's release
/// build or the trace build itself (patched twice). Nothing is kept beside it.
#[test]
fn lz4_patch_replaces_any_runtime() {
    let root = dir("lz4-patch-runtime");
    let game = root.join("game");
    ampr_game(&game);
    for before in [&b"\x7fELF someone else's libSceAmpr"[..], RELEASE, TRACE] {
        write(&game, "fakelib/libSceAmpr.sprx", before);
        let done = lz4_patch(&game).unwrap();
        assert!(done.removed.is_empty());
        assert!(std::fs::read(game.join("fakelib/libSceAmpr.sprx")).unwrap() == TRACE);
        assert_eq!(std::fs::read_dir(game.join("fakelib")).unwrap().count(), 1);
        assert_indexes_folder(&game);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lz4_patch_refusals() {
    let root = dir("lz4-patch-refusals");
    let refused = |path: &Path, want: &str| {
        let before = snapshot(path.parent().unwrap());
        let err = format!("{:#}", lz4_patch(path).unwrap_err());
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        assert_eq!(
            snapshot(path.parent().unwrap()),
            before,
            "{}",
            path.display()
        );
    };
    let src = root.join("src");
    ampr_game(&src);
    let image = root.join("images/PPSA01234.exfat");
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    run(request(&src, Format::Exfat, &image)).1.unwrap();
    refused(&image, "only a game folder is patched here");
    let packed = root.join("packed/PPSA01234");
    std::fs::create_dir_all(packed.parent().unwrap()).unwrap();
    run(request(&src, Format::Lz4, &packed)).1.unwrap();
    refused(&packed, "unpack it first");
    let plain = root.join("plain/game");
    game(&plain);
    refused(&plain, "does not import libSceAmpr");
    let _ = std::fs::remove_dir_all(root);
}

/// The console workflow: a copy of the dump patched in place and played; its journal and
/// index copied to the computer; the ORIGINAL dump (no runtime, no index) packed with them.
/// Returns the copied journal's path.
fn copied_traces(root: &Path, src: &Path, read: &[&str]) -> PathBuf {
    let console = root.join("console");
    copy_tree(src, &console);
    lz4_patch(&console).unwrap();
    let records = index_paths(&console);
    let ids: Vec<u32> = read
        .iter()
        .map(|p| records.iter().position(|(r, _)| r == p).unwrap() as u32 + 1)
        .collect();
    write(&console, "ampr_commands.bin", &journal_record(1, &ids));
    write(&console, "ampr_emu.log", b"session log");
    let copied = root.join("copied");
    for name in ["ampr_commands.bin", "ampr_emu.index"] {
        write(&copied, name, &std::fs::read(console.join(name)).unwrap());
    }
    copied.join("ampr_commands.bin")
}

#[test]
fn copied_traces_pick_what_lz4_packs() {
    let root = dir("lz4-copied-traces");
    let src = root.join("src");
    ampr_game(&src);
    let journal = copied_traces(&root, &src, &["data/assets/level1.bin"]);
    let packed = root.join("packed");
    let (events, result) = run(ConvertRequest {
        lz4_traces: Some(journal.clone()),
        ..request(&src, Format::Lz4, &packed)
    });
    result.unwrap();
    let rules = format!("the traces copied to {} (1 files", journal.display());
    assert!(logged(&events, &rules), "{events:?}");
    assert!(!packed.join("data/assets/level1.bin").exists());
    for loose in ["data/big.bin", "data/assets/noise.bin", "eboot.bin"] {
        assert!(packed.join(loose).is_file(), "{loose}");
    }
    assert!(std::fs::read(packed.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    // The source's own files are untouched.
    assert!(!src.join("fakelib").exists() && !src.join("ampr_emu.index").exists());
    let _ = std::fs::remove_dir_all(root);
}

/// The traces as Download traces hands them over (one zip, from the console's folder), as a folder
/// of both files, or as the journal with its index beside it: the same packed result. A
/// damaged zip is refused with what is wrong.
#[test]
fn copied_traces_as_zip_folder_or_journal() {
    let root = dir("lz4-traces-zip");
    let src = root.join("src");
    ampr_game(&src);
    let journal = copied_traces(&root, &src, &["data/assets/level1.bin"]);
    let mut traces = ps5_dump_forge_core::lz4_traces(&root.join("console"))
        .unwrap()
        .unwrap();
    let zip = root.join(traces.zip_name());
    assert!(zip.to_string_lossy().ends_with("[PPSA01234]-amprtrace.zip"));
    let mut file = std::fs::File::create(&zip).unwrap();
    traces.write_zip(&mut file).unwrap();
    assert_eq!(file.metadata().unwrap().len(), traces.zip_len());
    drop(file);
    let pack = |traces: &Path, out: &str| {
        let packed = root.join(out);
        run(ConvertRequest {
            lz4_traces: Some(traces.to_path_buf()),
            ..request(&src, Format::Lz4, &packed)
        })
        .1
        // Every path, and the bytes of all but the generated index, manifest and volumes,
        // which carry the job's time.
        .map(|_| {
            tree_bytes(&packed)
                .into_iter()
                .map(|(p, b)| match p.starts_with("ampr_") {
                    true => (p, Vec::new()),
                    false => (p, b),
                })
                .collect::<Vec<_>>()
        })
    };
    let by_journal = pack(&journal, "by-journal").unwrap();
    assert!(!root.join("by-journal/data/assets/level1.bin").exists());
    assert!(pack(&zip, "by-zip").unwrap() == by_journal);
    assert!(pack(journal.parent().unwrap(), "by-folder").unwrap() == by_journal);

    let bytes = std::fs::read(&zip).unwrap();
    let damaged = |bytes: &[u8], out: &str, want: &str| {
        let bad = root.join(format!("{out}.zip"));
        std::fs::write(&bad, bytes).unwrap();
        let err = pack(&bad, out).unwrap_err();
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        assert!(!root.join(out).exists());
    };
    let mut flipped = bytes.clone();
    flipped[30 + 17] ^= 0xff; // the journal's first byte
    damaged(
        &flipped,
        "crc",
        "the traces zip is damaged: ampr_commands.bin fails its CRC-32",
    );
    damaged(
        &bytes[..bytes.len() / 2],
        "cut",
        "the traces zip is damaged",
    );
    let folder = root.join("loose");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(&journal, folder.join("ampr_commands.bin")).unwrap();
    let err = pack(&folder, "no-index").unwrap_err();
    assert!(err.contains("ampr_emu.index"), "{err}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn copied_traces_refusals() {
    let root = dir("lz4-copied-refusals");
    let src = root.join("src");
    ampr_game(&src);
    let journal = copied_traces(&root, &src, &["data/assets/level1.bin"]);
    let refused = |req: ConvertRequest, want: &str| {
        let out = req.output.clone();
        let err = run(req).1.unwrap_err();
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        assert!(!out.exists(), "{}", out.display());
    };
    let traces = |format, out: &str| ConvertRequest {
        lz4_traces: Some(journal.clone()),
        ..request(&src, format, &root.join(out))
    };
    // A file the console's index lists but this dump lacks: the journal's ids would name the
    // wrong files.
    let saved = std::fs::read(src.join("readme.txt")).unwrap();
    std::fs::remove_file(src.join("readme.txt")).unwrap();
    let err = run(traces(Format::Lz4, "m")).1.unwrap_err();
    assert!(err.contains("do not belong to this dump"), "{err}");
    assert!(
        err.contains("only in the index") && err.contains("readme.txt"),
        "{err}"
    );
    write(&src, "readme.txt", &saved);
    // Not with a profile; only for the LZ4 target.
    let toml = root.join("profile.toml");
    std::fs::write(&toml, "[pack]\ndefault_action = \"compress\"\n").unwrap();
    refused(
        ConvertRequest {
            lz4_profile: Some(toml),
            ..traces(Format::Lz4, "p")
        },
        "choose a profile or traces, not both",
    );
    refused(
        traces(Format::Folder, "f"),
        "LZ4 traces only go with packing (LZ4 Pack)",
    );
    // The index must sit beside the journal.
    let alone = root.join("alone/ampr_commands.bin");
    write(
        &root,
        "alone/ampr_commands.bin",
        &std::fs::read(&journal).unwrap(),
    );
    refused(
        ConvertRequest {
            lz4_traces: Some(alone),
            ..request(&src, Format::Lz4, &root.join("a"))
        },
        "with ampr_emu.index beside it",
    );
    // A session that read nothing is no rule source.
    std::fs::write(&journal, journal_record(1, &[])).unwrap();
    refused(traces(Format::Lz4, "z"), "name no file the game read");
    let _ = std::fs::remove_dir_all(root);
}

/// A killed patch's temporary files are left alone (they may be another attempt's), never
/// indexed, and do not break the traces' membership check of the patched folder.
#[test]
fn lz4_patch_leaves_and_ignores_leftover_temp_files() {
    let root = dir("lz4-patch-leftovers");
    let game = root.join("game");
    ampr_game(&game);
    let leftovers = [
        ".ampr_emu.index.forge-1-0.tmp",
        "fakelib/.libSceAmpr.sprx.forge-1-0.tmp",
    ];
    for rel in leftovers {
        write(&game, rel, b"half written");
    }
    lz4_patch(&game).unwrap();
    for rel in leftovers {
        assert_eq!(std::fs::read(game.join(rel)).unwrap(), b"half written");
    }
    let records = index_paths(&game);
    assert!(
        !records.iter().any(|(p, _)| p.contains(".forge-")),
        "{records:?}"
    );
    assert!(std::fs::read(game.join("fakelib/libSceAmpr.sprx")).unwrap() == TRACE);
    // The patched folder, played, packs with its own traces.
    let id = records
        .iter()
        .position(|(p, _)| p == "data/assets/level1.bin")
        .unwrap() as u32
        + 1;
    write(&game, "ampr_commands.bin", &journal_record(1, &[id]));
    let packed = root.join("packed");
    let (events, result) = run(request(&game, Format::Lz4, &packed));
    result.unwrap();
    assert!(logged(&events, "this dump's traces (1 files"), "{events:?}");
    assert!(!packed.join("data/assets/level1.bin").exists());
    let _ = std::fs::remove_dir_all(root);
}

/// A folder where the patch writes or deletes a file (or a file where `fakelib` goes) is
/// refused before anything changes.
#[test]
fn lz4_patch_refuses_folders_in_its_places() {
    let root = dir("lz4-patch-types");
    let game = root.join("game");
    ampr_game(&game);
    write(&game, "ampr_emu.log", b"last log");
    for rel in [
        "ampr_emu.index",
        "ampr_commands.bin",
        "ampr_emu.log",
        "apr_emu.log",
        "fakelib/libSceAmpr.sprx",
    ] {
        let path = game.join(rel);
        let saved = path.is_file().then(|| std::fs::read(&path).unwrap());
        if saved.is_some() {
            std::fs::remove_file(&path).unwrap();
        }
        write(&path, "inside", b"x");
        let before = snapshot(&game);
        let err = format!("{:#}", lz4_patch(&game).unwrap_err());
        assert!(err.contains("is not a regular file"), "{rel}: {err}");
        assert_eq!(snapshot(&game), before, "{rel}");
        std::fs::remove_dir_all(&path).unwrap();
        if let Some(bytes) = saved {
            std::fs::write(&path, bytes).unwrap();
        }
    }
    let _ = std::fs::remove_dir_all(game.join("fakelib"));
    write(&game, "fakelib", b"a file");
    let before = snapshot(&game);
    let err = format!("{:#}", lz4_patch(&game).unwrap_err());
    assert!(err.contains("is not a folder"), "{err}");
    assert_eq!(snapshot(&game), before);
    let _ = std::fs::remove_dir_all(root);
}

use ps5_dump_forge_core::{JobId, lz4_unpatch};

/// A request that patches (`Trace`) or unpatches `image` in place; its `output` names
/// something that must stay absent.
fn in_place(image: &Path, mode: Lz4Mode) -> ConvertRequest {
    let format = match image.extension().and_then(|e| e.to_str()) {
        Some("exfat") => Format::Exfat,
        Some("ffpkg") => Format::Ffpkg,
        Some("ffpfs") => Format::Ffpfs,
        Some("ffpfsc") => Format::Ffpfsc,
        Some("pkg") => Format::Pkg,
        _ => Format::Folder,
    };
    ConvertRequest {
        lz4_in_place: true,
        ..lz4_request(
            image,
            format,
            &image.with_file_name("ignored-output"),
            Some(mode),
        )
    }
}

/// Runs `req`, holding its worker at the first progress of `at` while `act` runs here.
fn run_pausing(
    req: ConvertRequest,
    at: &'static str,
    act: impl FnOnce(&Jobs, JobId),
) -> Result<JobReport, String> {
    let (at_tx, at_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Mutex::new((Some(at_tx), go_rx, done_tx));
    let jobs = Jobs::new(move |e| {
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
    });
    let job = jobs.start(req);
    at_rx.recv().unwrap();
    act(&jobs, job);
    go_tx.send(()).unwrap();
    let result = done_rx.recv().unwrap();
    jobs.cancel_all_and_wait();
    result
}

fn facts_runtime(image: &Path) -> (String, Option<u64>) {
    let facts = inspect(image).unwrap().lz4.unwrap();
    (facts.runtime, facts.journal_bytes)
}

/// Patch then unpatch an `.exfat` and an `.ffpkg` in place: each time the image is replaced
/// by a verified copy with the requested runtime, under its own name, and nothing else stays.
#[test]
fn lz4_patch_and_unpatch_an_image_in_place() {
    let root = dir("lz4-in-place");
    let src = root.join("src");
    ampr_game(&src);
    for ext in ["exfat", "ffpkg"] {
        let images = root.join(ext);
        std::fs::create_dir_all(&images).unwrap();
        let image = images.join(format!("game.{ext}"));
        let format = if ext == "exfat" {
            Format::Exfat
        } else {
            Format::Ffpkg
        };
        let plain = run(request(&src, format, &image)).1.unwrap().bytes;
        assert_eq!(facts_runtime(&image), ("none".to_string(), None));

        let (events, result) = run(in_place(&image, Lz4Mode::Trace));
        let report = result.unwrap();
        assert_eq!(report.output, image.canonicalize().unwrap());
        assert!(stages(&events).contains(&"verify"), "{events:?}");
        assert!(logged(&events, "in place: writing a patched copy"));
        assert!(logged(&events, "256 MiB of free space in the image"));
        assert!(report.checks.last().unwrap().starts_with("replaced "));
        assert!(!images.join("ignored-output").exists());
        assert!(stale_parts(&images).is_empty());
        assert_eq!(std::fs::read_dir(&images).unwrap().count(), 1);
        assert_eq!(std::fs::metadata(&image).unwrap().len(), report.bytes);
        assert!(report.bytes >= plain + (256 << 20) - (64 << 20), "{ext}");
        assert_eq!(facts_runtime(&image), ("forge_trace".to_string(), None));

        let (events, result) = run(in_place(&image, Lz4Mode::Unpatch));
        let report = result.unwrap();
        assert!(logged(&events, "in place: writing an unpatched copy"));
        assert!(logged(
            &events,
            "LZ4 unpatch: replacing the tracing runtime"
        ));
        assert!(report.checks.last().unwrap().starts_with("replaced "));
        assert!(stale_parts(&images).is_empty());
        assert_eq!(std::fs::read_dir(&images).unwrap().count(), 1);
        assert_eq!(facts_runtime(&image), ("forge_release".to_string(), None));
        let back = root.join(format!("back-{ext}"));
        run(request(&image, Format::Folder, &back)).1.unwrap();
        assert!(std::fs::read(back.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
        assert_indexes_folder(&back);
        assert_eq!(assets(&back), tree_bytes(&src));
    }
    let _ = std::fs::remove_dir_all(root);
}

/// A cancel mid-write, or a source that changes before the rename, fails the job: the
/// source keeps every byte and no `.part` is left.
#[test]
fn a_failed_in_place_job_leaves_the_source() {
    let root = dir("lz4-in-place-fail");
    let src = root.join("src");
    ampr_game(&src);
    let images = root.join("images");
    std::fs::create_dir_all(&images).unwrap();
    for ext in ["exfat", "ffpkg"] {
        let format = if ext == "exfat" {
            Format::Exfat
        } else {
            Format::Ffpkg
        };
        let image = images.join(format!("game.{ext}"));
        run(request(&src, format, &image)).1.unwrap();
        let before = std::fs::read(&image).unwrap();

        let err = run_pausing(in_place(&image, Lz4Mode::Unpatch), "write", |jobs, job| {
            assert_eq!(stale_parts(&images).len(), 1);
            jobs.cancel(job);
        })
        .unwrap_err();
        assert_eq!(err, "cancelled");
        assert!(std::fs::read(&image).unwrap() == before, "{ext}");
        assert!(stale_parts(&images).is_empty());

        // Same bytes, another modification time: the stamp says it changed.
        let err = run_pausing(in_place(&image, Lz4Mode::Trace), "finalize", |_, _| {
            let file = std::fs::File::options().write(true).open(&image).unwrap();
            let mtime = file.metadata().unwrap().modified().unwrap();
            file.set_modified(mtime - std::time::Duration::from_secs(10))
                .unwrap();
        })
        .unwrap_err();
        assert!(
            err.contains("changed while it was being converted"),
            "{err}"
        );
        assert!(std::fs::read(&image).unwrap() == before, "{ext}");
        assert!(stale_parts(&images).is_empty());
        assert!(!images.join("ignored-output").exists());
    }
    let _ = std::fs::remove_dir_all(root);
}

/// What in place refuses, before anything is written: a read-only or folder source, another
/// format, a job that neither patches nor unpatches, a packed image.
#[test]
fn in_place_refusals() {
    let root = dir("lz4-in-place-refusals");
    let src = root.join("src");
    ampr_game(&src);
    let images = root.join("images");
    std::fs::create_dir_all(&images).unwrap();
    let refused = |req: ConvertRequest, want: &str| {
        let source = req.source.clone();
        let before = source.is_file().then(|| std::fs::read(&source).unwrap());
        let err = run(req).1.unwrap_err();
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        if let Some(before) = before {
            assert!(std::fs::read(&source).unwrap() == before);
        }
        assert!(stale_parts(&images).is_empty());
        assert!(!images.join("ignored-output").exists());
    };
    let read_only = "can't be patched in place: the game's /app0 is read-only";
    for (ext, format) in [
        ("ffpfs", Format::Ffpfs),
        ("ffpfsc", Format::Ffpfsc),
        ("pkg", Format::Pkg),
    ] {
        let image = images.join(format!("game.{ext}"));
        run(request(&src, format, &image)).1.unwrap();
        refused(in_place(&image, Lz4Mode::Trace), read_only);
        refused(in_place(&image, Lz4Mode::Unpatch), read_only);
        std::fs::remove_file(&image).unwrap();
    }
    refused(
        in_place(&src, Lz4Mode::Trace),
        "a game folder is patched in place directly",
    );
    let exfat = images.join("game.exfat");
    run(request(&src, Format::Exfat, &exfat)).1.unwrap();
    refused(
        ConvertRequest {
            format: Format::Ffpkg,
            ..in_place(&exfat, Lz4Mode::Trace)
        },
        "the target must be .exfat, not .ffpkg",
    );
    for mode in [None, Some(Lz4Mode::Unpack)] {
        refused(
            ConvertRequest {
                lz4: mode,
                ..in_place(&exfat, Lz4Mode::Trace)
            },
            "in place goes only with an LZ4 patch",
        );
    }
    std::fs::remove_file(&exfat).unwrap();

    // A packed dump in an image: in place refuses; Unpatch refuses it anywhere.
    let packed = root.join("packed");
    run(request(&src, Format::Lz4, &packed)).1.unwrap();
    let image = images.join("packed.exfat");
    run(request(&packed, Format::Exfat, &image)).1.unwrap();
    for mode in [Lz4Mode::Trace, Lz4Mode::Unpatch] {
        refused(
            in_place(&image, mode),
            "holds LZ4 packs (ampr_assets.index): unpack it first",
        );
    }
    std::fs::remove_file(&image).unwrap();
    refused(
        lz4_request(
            &packed,
            Format::Folder,
            &images.join("u"),
            Some(Lz4Mode::Unpatch),
        ),
        "unpack it first (Unpack LZ4), then unpatch the unpacked copy",
    );
    refused(
        lz4_request(&src, Format::Lz4, &images.join("u"), Some(Lz4Mode::Unpatch)),
        "unpatching LZ4 is redundant",
    );
    let plain = root.join("plain");
    game(&plain);
    refused(
        lz4_request(
            &plain,
            Format::Folder,
            &images.join("u"),
            Some(Lz4Mode::Unpatch),
        ),
        "does not import libSceAmpr",
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Unpatch as a conversion: the release runtime in place of any runtime, the journal and
/// logs left out, a fresh index.
#[test]
fn an_unpatch_conversion_installs_the_release_runtime() {
    let root = dir("lz4-unpatch-convert");
    let traced = traced(&root, &["data/big.bin"]);
    let out = root.join("out");
    let (events, result) = run(lz4_request(
        &traced,
        Format::Folder,
        &out,
        Some(Lz4Mode::Unpatch),
    ));
    result.unwrap();
    assert!(logged(
        &events,
        "LZ4 unpatch: replacing the tracing runtime"
    ));
    assert!(logged(&events, "leaving out ampr_commands.bin"));
    assert!(logged(&events, "writing a new ampr_emu.index"));
    assert!(std::fs::read(out.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    assert!(!out.join("ampr_commands.bin").exists() && !out.join("ampr_emu.log").exists());
    assert_indexes_folder(&out);
    assert_eq!(assets(&out), assets(&root.join("src")));

    // Another runtime, and none at all.
    let other = root.join("other");
    copy_tree(&root.join("src"), &other);
    write(
        &other,
        "fakelib/libSceAmpr.sprx",
        b"\x7fELF someone else's libSceAmpr",
    );
    let out = root.join("out-other.ffpkg");
    let (events, result) = run(lz4_request(
        &other,
        Format::Ffpkg,
        &out,
        Some(Lz4Mode::Unpatch),
    ));
    result.unwrap();
    assert!(logged(&events, "replacing another runtime"));
    assert_eq!(facts_runtime(&out).0, "forge_release");
    let out = root.join("out-none.exfat");
    let (events, result) = run(lz4_request(
        &root.join("src"),
        Format::Exfat,
        &out,
        Some(Lz4Mode::Unpatch),
    ));
    result.unwrap();
    assert!(logged(&events, "installing the release runtime"));
    assert_eq!(facts_runtime(&out).0, "forge_release");

    // The other targets, with a backport left out: the source is untouched, the output has
    // the release runtime, no backport and an index of what it holds.
    let mut exe = eboot(0x1200_0038);
    exe.extend_from_slice(b"libSceAmpr");
    write(&other, "eboot.bin", &exe);
    write(
        &other,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST","sdkVersion":"0x1200000000000000"}"#,
    );
    write(
        &other,
        "fakelib/libSceAgc.sprx",
        b"\x7fELF a system library",
    );
    let facts = inspect(&other).unwrap().lz4.unwrap();
    assert_eq!(
        (facts.runtime.as_str(), facts.shipped_runtime_version),
        ("other", ps5_dump_forge_lz4::runtime::VERSION)
    );
    let before = snapshot(&other);
    for (format, ext) in [
        (Format::Ffpfs, "ffpfs"),
        (Format::Ffpfsc, "ffpfsc"),
        (Format::Pkg, "pkg"),
    ] {
        let out = root.join(format!("out-backport.{ext}"));
        run(ConvertRequest {
            remove_backport: true,
            ..lz4_request(&other, format, &out, Some(Lz4Mode::Unpatch))
        })
        .1
        .unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!(
            snapshot(&other),
            before,
            "{ext}: the source is never touched"
        );
        let found = inspect(&out).unwrap();
        assert!(found.backport.is_empty(), "{ext}: {:?}", found.backport);
        assert_eq!(found.lz4.unwrap().runtime, "forge_release", "{ext}");
        let back = root.join(format!("back-{ext}"));
        run(request(&out, Format::Folder, &back)).1.unwrap();
        assert!(!back.join("fakelib/libSceAgc.sprx").exists(), "{ext}");
        // The `.pkg` builder adds its keystone after the index is written.
        if format == Format::Pkg {
            std::fs::remove_file(back.join("sce_sys/keystone")).unwrap();
        }
        assert_indexes_folder(&back);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lz4_unpatch_changes_the_folder_in_place() {
    let root = dir("lz4-unpatch");
    let folder = root.join("folder");
    ampr_game(&folder);
    lz4_patch(&folder).unwrap();
    write(&folder, "ampr_commands.bin", b"the session's journal");
    write(&folder, "ampr_emu.log", b"log");
    let done = lz4_unpatch(&folder).unwrap();
    assert_eq!(done.removed, ["ampr_commands.bin", "ampr_emu.log"]);
    assert_eq!(done.warning, ps5_dump_forge_lz4::runtime::WARNING);
    assert!(std::fs::read(folder.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
    assert_eq!(
        std::fs::read_dir(folder.join("fakelib")).unwrap().count(),
        1
    );
    assert_indexes_folder(&folder);
    assert_eq!(done.indexed, index_paths(&folder).len());
    assert!(
        index_paths(&folder)
            .contains(&("fakelib/libSceAmpr.sprx".to_string(), RELEASE.len() as u64))
    );
    assert_eq!(facts_runtime(&folder), ("forge_release".to_string(), None));
    // Twice, and over another runtime: still the release build.
    for before in [RELEASE, &b"\x7fELF someone else's libSceAmpr"[..]] {
        write(&folder, "fakelib/libSceAmpr.sprx", before);
        assert!(lz4_unpatch(&folder).unwrap().removed.is_empty());
        assert!(std::fs::read(folder.join("fakelib/libSceAmpr.sprx")).unwrap() == RELEASE);
        assert_indexes_folder(&folder);
    }

    let refused = |path: &Path, want: &str| {
        let before = snapshot(path.parent().unwrap());
        let err = format!("{:#}", lz4_unpatch(path).unwrap_err());
        assert!(err.contains(want), "{want:?} not in:\n{err}");
        assert_eq!(snapshot(path.parent().unwrap()), before);
    };
    let image = root.join("images/PPSA01234.ffpkg");
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    run(request(&folder, Format::Ffpkg, &image)).1.unwrap();
    refused(&image, "only a game folder is unpatched here");
    let packed = root.join("packed/PPSA01234");
    std::fs::create_dir_all(packed.parent().unwrap()).unwrap();
    run(request(&folder, Format::Lz4, &packed)).1.unwrap();
    refused(&packed, "unpack it first, then unpatch the unpacked folder");
    let plain = root.join("plain/game");
    game(&plain);
    refused(&plain, "does not import libSceAmpr");
    let _ = std::fs::remove_dir_all(root);
}

/// A dump with files the traced index does not list (a scene `.nfo`, say) still packs by the
/// traces, copied or its own: those files were never read, so they stay loose, and one log
/// line names them (the first 10, then a count).
#[test]
fn traces_allow_files_the_index_does_not_list() {
    let root = dir("lz4-traces-extra");
    let src = root.join("src");
    ampr_game(&src);
    let journal = copied_traces(&root, &src, &["data/assets/level1.bin"]);
    write(&src, "_DUPLEX_/duplex.nfo", b"scene info");
    for n in 0..11 {
        write(&src, &format!("extra/{n:02}.bin"), &noise(70_000, n));
    }
    let packed = root.join("packed");
    let (events, result) = run(ConvertRequest {
        lz4_traces: Some(journal),
        ..request(&src, Format::Lz4, &packed)
    });
    result.unwrap();
    assert!(
        logged(
            &events,
            "LZ4 traces: 12 files of this dump are not in ampr_emu.index, so the game can't have \
             read them; they stay loose: _DUPLEX_/duplex.nfo, extra/00.bin"
        ),
        "{events:?}"
    );
    assert!(logged(&events, "extra/08.bin and 2 more"), "{events:?}");
    assert!(!packed.join("data/assets/level1.bin").exists());
    assert_eq!(
        std::fs::read(packed.join("_DUPLEX_/duplex.nfo")).unwrap(),
        b"scene info"
    );
    assert!(packed.join("extra/10.bin").is_file());

    // The dump's own traces, with an extra file beside them.
    let lz4 = root.join("own");
    std::fs::create_dir_all(&lz4).unwrap();
    let traced = traced(&lz4, &["data/big.bin"]);
    write(&traced, "_DUPLEX_/duplex.nfo", b"scene info");
    let packed = root.join("packed-own");
    let (events, result) = run(request(&traced, Format::Lz4, &packed));
    result.unwrap();
    assert!(
        logged(&events, "1 files of this dump are not in ampr_emu.index"),
        "{events:?}"
    );
    assert!(packed.join("_DUPLEX_/duplex.nfo").is_file());
    assert!(!packed.join("data/big.bin").exists());
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

/// `lz4: pack` into every image format, with the built-in guess and with a traces zip: the
/// image verifies (as written and through its packs), holds the same packs the folder target
/// writes, chunk for chunk, and unpacks to the source's files.
#[test]
fn lz4_pack_straight_into_each_image() {
    let root = dir("lz4-pack-image");
    let src = root.join("src");
    ampr_game(&src);
    let before = snapshot(&src);
    copied_traces(&root, &src, &["data/assets/level1.bin", "data/big.bin"]);
    let mut traces = ps5_dump_forge_core::lz4_traces(&root.join("console"))
        .unwrap()
        .unwrap();
    let zip = root.join("traces.zip");
    traces
        .write_zip(&mut std::fs::File::create(&zip).unwrap())
        .unwrap();
    let images = [
        (Format::Exfat, None, "exfat"),
        (Format::Ffpkg, None, "ffpkg"),
        (Format::Ffpfs, None, "ffpfs"),
        (Format::Ffpfsc, Some(Format::Exfat), "ffpfsc"),
        (Format::Ffpfsc, Some(Format::Ffpkg), "ffpfsc"),
    ];
    for (rules, traces) in [("guess", None), ("traces", Some(zip.clone()))] {
        let folder = root.join(format!("folder-{rules}"));
        run(ConvertRequest {
            lz4_traces: traces.clone(),
            ..request(&src, Format::Lz4, &folder)
        })
        .1
        .unwrap();
        let want = pack_payload(&folder);
        assert!(want.iter().any(|(p, _)| p == "ampr_assets-000.pak"));
        for (n, (format, inner, ext)) in images.into_iter().enumerate() {
            let image = root.join(format!("{rules}-{n}.{ext}"));
            let (events, result) = run(ConvertRequest {
                inner,
                lz4: Some(Lz4Mode::Pack),
                lz4_traces: traces.clone(),
                full_verify: rules == "traces",
                ..request(&src, format, &image)
            });
            let report = result.unwrap_or_else(|e| panic!("{rules} {ext}: {e}"));
            one_bar(&events);
            let order = stages(&events);
            let at = |s: &str| order.iter().position(|x| *x == s).unwrap();
            assert!(
                at("measure") < at("write") && at("write") < at("verify"),
                "{order:?}"
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
            assert!(
                checks.iter().any(|c| c.ends_with("files match the source")) || rules == "guess"
            );

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

#[test]
fn lz4_pack_refusals_and_old_requests() {
    let root = dir("lz4-pack-refusals");
    let src = root.join("src");
    ampr_game(&src);
    let out = root.join("x.pkg");
    let err = run(lz4_request(&src, Format::Pkg, &out, Some(Lz4Mode::Pack)))
        .1
        .unwrap_err();
    assert!(
        err.contains("packing into a .pkg is not supported yet"),
        "{err}"
    );
    assert!(!out.exists());
    // A plain game is not an AMPR title, whatever the target.
    let plain = root.join("plain");
    game(&plain);
    let err = run(lz4_request(
        &plain,
        Format::Exfat,
        &root.join("p.exfat"),
        Some(Lz4Mode::Pack),
    ))
    .1
    .unwrap_err();
    assert!(err.contains("does not import libSceAmpr"), "{err}");
    // `lz4: pack` with `format: folder` is the `lz4` target.
    let (events, result) = run(lz4_request(
        &src,
        Format::Folder,
        &root.join("f"),
        Some(Lz4Mode::Pack),
    ));
    result.unwrap();
    assert!(logged(&events, "wrote LZ4 packs"));
    assert!(root.join("f/ampr_assets.index").is_file());
    let new: ConvertRequest = serde_json::from_str(
        r#"{"source":"/s","format":"ffpfsc","output":"/o.ffpfsc","lz4":"pack","inner":"ffpkg"}"#,
    )
    .unwrap();
    assert_eq!(
        (new.lz4, new.inner),
        (Some(Lz4Mode::Pack), Some(Format::Ffpkg))
    );
    assert!(stale_parts(&root).is_empty());
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

/// LZ4 packs into an `.ffpfs` (measure, write, verify as written and through the packs):
/// measuring and auto-loose sampling reserve the passes after them, so the write moves the
/// bar on from where measuring left it. Compressible and RAW-heavy packs, fast and full.
#[test]
fn lz4_pack_into_an_image_is_one_bar() {
    let root = dir("lz4-pack-bar");
    let src = root.join("src");
    ampr_game(&src);
    // Sampled (noise.bin kept loose), or noise.bin packed as RAW chunks.
    for (tag, extra) in [("sampled", ""), ("raw", "auto_loose_large_files = false\n")] {
        let toml = root.join(format!("{tag}.toml"));
        std::fs::write(
            &toml,
            format!(
                "[pack]\ndefault_action = \"compress\"\nauto_loose_min_file_size = \"100KiB\"\n{extra}"
            ),
        )
        .unwrap();
        for full_verify in [false, true] {
            let out = root.join(format!("{tag}-{full_verify}.ffpfs"));
            let (events, result) = run(ConvertRequest {
                lz4_profile: Some(toml.clone()),
                full_verify,
                ..lz4_request(&src, Format::Ffpfs, &out, Some(Lz4Mode::Pack))
            });
            result.unwrap_or_else(|e| panic!("{tag} {full_verify}: {e}"));
            assert_eq!(
                logged(&events, "auto-loose: 1 large files kept loose"),
                tag == "sampled"
            );
            let bar = one_bar(&events);
            let fraction = |&(_, d, t): &(&str, u64, u64)| d as f64 / t.max(1) as f64;
            let shown = |upto: usize| bar[..upto].iter().map(fraction).fold(0.0, f64::max);
            let after = |stage: &str| bar.iter().rposition(|e| e.0 == stage).unwrap() + 1;
            let (measured, written) = (shown(after("measure")), shown(after("write")));
            assert!(
                measured < written && written < 1.0,
                "{tag} {full_verify}: measured {measured}, written {written}: {bar:?}"
            );
            // Auto-loose sampling, in preflight before the measure pass, is a sliver of it.
            let sampled = shown(bar.iter().position(|e| e.0 == "measure").unwrap());
            assert!(
                (sampled > 0.0) == (tag == "sampled") && sampled < measured,
                "{bar:?}"
            );
            assert_eq!(
                bar.last().map(|e| (e.0, e.1 == e.2)),
                Some(("finalize", true))
            );
        }
    }
    assert!(stale_parts(&root).is_empty());
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
    // Queued through a link, with a journal whose index beside it it reads too.
    let aliases = root.join("aliases");
    std::fs::create_dir_all(&aliases).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&queued, aliases.join("game")).unwrap();
    #[cfg(not(unix))]
    let aliases = queued.clone();
    write(&root, "traces/ampr_commands.bin", b"j");
    write(&root, "traces/ampr_emu.index", b"i");
    let mut waiting = request(&aliases.join("game"), Format::Lz4, &root.join("queued-out"));
    waiting.lz4_traces = Some(root.join("traces/ampr_commands.bin"));
    let waiting = jobs.start(waiting);
    let [part] = &stale_parts(&root)[..] else {
        panic!("one .part while writing");
    };
    let index = root.join("traces/ampr_emu.index");
    for busy in [
        &src,
        &src.join("data"),
        part,
        &root,
        &queued,
        &aliases,
        &index,
    ] {
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
