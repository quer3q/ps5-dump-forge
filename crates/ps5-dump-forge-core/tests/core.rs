//! End-to-end checks of the public core API with the Folder target and the readers.
//! Image targets run through the real exFAT and UFS2 writers, `.pkg` through the FPKG builder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ps5_dump_forge_core::{
    ConvertRequest, DataDirs, Event, Format, JobReport, Jobs, ScannedFolder, default_output,
    extraction_findings, inspect, rename_no_replace, stale_parts,
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
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.starts_with("blake3: 4 files"))
    );
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
    for want in [
        "blake3: 7 files match",
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
    let root = dir(&format!("cancel-{out}"));
    let src = root.join("src");
    game(&src);
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
    let job = jobs.start(request(&src, format, &out));
    writing_rx.recv().unwrap();
    let parts = stale_parts(&root);
    assert_eq!(parts.len(), 2, "{parts:?}");
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
    assert!(report.checks.iter().any(|c| c.starts_with("blake3:")));
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
    // An explicit name over 63 bytes is refused for the PFS formats only.
    let long = format!("{}.ffpfs", "n".repeat(58));
    let err = run(request(&src, Format::Ffpfs, &root.join(&long)))
        .1
        .unwrap_err();
    assert!(
        err.contains(&format!(
            "{long} is 64 bytes long: ShadowMountPlus fails to mount a .ffpfs"
        )),
        "{err}"
    );
    std::fs::remove_file(src.join("data/café.bin")).unwrap();
    std::fs::remove_dir_all(src.join("data/naïve")).unwrap();
    let fits = format!("{}.ffpfs", "n".repeat(57));
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
    let opts = ps5_dump_forge_ufs2::Options { free_bytes: 0 };
    let layout = ps5_dump_forge_ufs2::plan(&tree, &opts, &cancel).unwrap();
    let image = root.join("junk.ffpkg");
    let mut out = std::fs::File::create_new(&image).unwrap();
    ps5_dump_forge_ufs2::write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {}).unwrap();
    drop(out);

    let found = inspect(&image).unwrap();
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
