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
        lz4_profile: None,
        lz4_traces: None,
        lz4_trace_space_mib: 256,
        lz4_in_place: false,
        edits: Vec::new(),
        extract: Vec::new(),
        edit_rebuild: false,
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

/// Into a `.pkg`, neither a plain conversion nor an unpatch builds an `ampr_emu.index` of its
/// own: the builder writes one after leaving out SDK leftovers, so names only they share in
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
    for (name, mode) in [("plain", None), ("unpatch", Some(Lz4Mode::Unpatch))] {
        let pkg = root.join(format!("{name}.pkg"));
        let (events, result) = run(lz4_request(&zip, Format::Pkg, &pkg, mode));
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
    write(&packed, "fakelib/libSceAmpr.sprx", TRACE);
    write(&packed, "ampr_emu.log", b"log");
    let image = root.join("PPSA01234.ffpkg");
    let (events, result) = run(request(&packed, Format::Ffpkg, &image));
    result.unwrap();
    assert!(
        logged(&events, "swapping the tracing runtime"),
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
    let _ = std::fs::remove_dir_all(root);
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
    run(lz4_request(
        &src,
        Format::Folder,
        &game,
        Some(Lz4Mode::Unpatch),
    ))
    .1
    .unwrap();
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
    run(lz4_request(
        &src,
        Format::Folder,
        &game,
        Some(Lz4Mode::Unpatch),
    ))
    .1
    .unwrap();
    assert_eq!(actions(&game), both("keep"), "Forge's release runtime");
    let packed = root.join("packed");
    run(request(&game, Format::Lz4, &packed)).1.unwrap();
    assert_eq!(
        actions(&packed),
        both("keep"),
        "packed, Unpack keeps it too"
    );

    let traced = root.join("traced");
    run(lz4_request(
        &src,
        Format::Folder,
        &traced,
        Some(Lz4Mode::Trace),
    ))
    .1
    .unwrap();
    assert_eq!(actions(&traced), both("replace"), "Forge's trace runtime");

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
        // A plain conversion of an AMPR title installs the release runtime.
        assert_eq!(facts_runtime(&image), ("forge_release".to_string(), None));

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
        // The `.pkg` builder writes the index anew, its keystone and param.json included.
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
    // Convert, Extract and an LZ4 job in place all refuse it as it is.
    interrupted(request(&image, Format::Folder, &root.join("out")));
    let out = root.join("extracted");
    std::fs::create_dir(&out).unwrap();
    interrupted(ConvertRequest {
        extract: vec!["eboot.bin".into()],
        ..request(&image, Format::Folder, &out)
    });
    interrupted(ConvertRequest {
        lz4: Some(ps5_dump_forge_core::Lz4Mode::Trace),
        lz4_in_place: true,
        ..request(&image, Format::Exfat, &root.join("ignored"))
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
        lz4.lz4_in_place = true;
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
    write(&host, "trace.sprx", TRACE);
    let edits = || {
        vec![
            Edit::Add {
                from: host.join("added.bin"),
                to: "data/added.bin".into(),
            },
            // The user's own runtime stays: an edit never swaps it.
            Edit::Add {
                from: host.join("trace.sprx"),
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
        // Read back by the image's own reader: a conversion would swap the trace runtime.
        let mut tree = read(&image);
        assert!(tree.read("fakelib/libSceAmpr.sprx").unwrap() == TRACE);
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
        assert_eq!(report.output, out.canonicalize().unwrap(), "{label}");
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
