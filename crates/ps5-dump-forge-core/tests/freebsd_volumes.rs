//! End to end on the FreeBSD volume under `CARGO_TARGET_TMPDIR` (UFS, FAT32 or nullfs in
//! `scripts/test-freebsd.sh`): a small game folder, with nothing FAT32 cannot hold (no
//! symlinks, hard links or FIFOs), converted to each target (and each `.ffpfsc` inner image)
//! through the public Jobs API, as the PS5 payload does. Each job must probe the volume as
//! what it is, publish, pass the BLAKE3 read-back, and leave no `.part` or probe file behind;
//! a job that fails late publishes nothing. FAT32 runs no other integration test, so this is
//! its coverage.
#![cfg(target_os = "freebsd")]

use std::ffi::CString;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};

use ps5_dump_forge_core::{ConvertRequest, Event, Format, JobReport, Jobs, stale_parts};

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

/// The filesystem type `path` is on, asked of the kernel (never guessed from the path).
fn fstype(path: &Path) -> String {
    let c = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: a valid C string and a properly sized out-parameter.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::statfs(c.as_ptr(), &mut st) }, 0);
    let name: Vec<u8> = st
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(name).unwrap()
}

/// Runs one job; returns its log lines and result. `hook` sees every event first, on the
/// job's worker thread.
fn run_with(
    req: ConvertRequest,
    hook: impl Fn(&Event) + Send + Sync + 'static,
) -> (Vec<String>, Result<JobReport, String>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let jobs = Jobs::new(move |e| {
        hook(&e);
        let _ = tx.lock().unwrap().send(e);
    });
    jobs.start(req);
    let mut lines = Vec::new();
    loop {
        match rx.recv().unwrap() {
            Event::Log { line, .. } => lines.push(line),
            Event::Done { result, .. } => return (lines, result),
            Event::Progress { .. } => {}
        }
    }
}

fn request(source: &Path, format: Format, output: &Path, inner: Option<Format>) -> ConvertRequest {
    ConvertRequest {
        source: source.to_path_buf(),
        format,
        output: output.to_path_buf(),
        compression_threads: Some(2),
        inner,
        remove_backport: false,
        full_verify: false,
    }
}

/// Every name under `root`, recursively.
fn names(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            out.push(entry.file_name().to_string_lossy().into_owned());
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            }
        }
    }
    out
}

/// Nothing a job leaves behind: no `.part`, no probe file.
fn assert_clean(out: &Path) {
    assert!(stale_parts(out).is_empty(), "{:?}", stale_parts(out));
    let left: Vec<String> = names(out)
        .into_iter()
        .filter(|n| n.ends_with(".part") || n.starts_with(".forge-probe"))
        .collect();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn every_target_publishes_on_this_volume() {
    let base = dir("freebsd-volume");
    let src = base.join("game");
    write(&src, "eboot.bin", b"\x7fELF fake eboot");
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    let data: Vec<u8> = (0..300 * 1024 + 3).map(|i| (i % 251) as u8).collect();
    write(&src, "data/big.bin", &data);
    write(&src, "data/zero.bin", b"");
    std::fs::create_dir_all(src.join("data/empty")).unwrap();
    let out = base.join("out");
    std::fs::create_dir(&out).unwrap();

    let fs = fstype(&out);
    // What the probe must have seen, by the kernel's name for the volume.
    let expect: &[&str] = match fs.as_str() {
        "msdosfs" => &[
            "msdosfs",
            "files under 4 GiB",
            "folds names",
            "no hard links",
        ],
        "ufs" | "nullfs" => &["64-bit files", "case-sensitive names", ", hard links"],
        other => panic!("no expectation for a {other} volume"),
    };

    for (format, inner, name) in [
        (Format::Exfat, None, "game.exfat"),
        (Format::Ffpkg, None, "game.ffpkg"),
        (Format::Ffpfs, None, "game.ffpfs"),
        (Format::Ffpfsc, None, "game.ffpfsc"),
        (Format::Ffpfsc, Some(Format::Ffpkg), "game-ufs.ffpfsc"),
        (Format::Ffpfsc, Some(Format::Ffpfs), "game-pfs.ffpfsc"),
        (Format::Pkg, None, "game.pkg"),
        (Format::Folder, None, "game-folder"),
    ] {
        let output = out.join(name);
        let (lines, result) = run_with(request(&src, format, &output, inner), |_| {});
        let report = result.unwrap_or_else(|e| panic!("{name} on {fs}: {e}\n{lines:#?}"));
        assert_eq!(report.output, output);
        assert!(output.exists(), "{name} was not published");
        assert_eq!(
            report.checks.last().map(String::as_str),
            Some(format!("published {}", output.display()).as_str())
        );
        // The verify pass's own BLAKE3 line (`verify::compare`): the files were read back. The
        // fixture's files are all small, so even the default fast mode reads every byte.
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.starts_with("blake3: ") && c.contains("match the source")),
            "{name}: no BLAKE3 check in {:#?}",
            report.checks
        );
        if format != Format::Pkg {
            assert_eq!(
                report.verify.mode,
                ps5_dump_forge_core::VerifyMode::Fast,
                "{name}"
            );
        }
        assert_eq!(
            report.verify.checked_bytes, report.verify.total_bytes,
            "{name}: small files are checked whole"
        );
        let dest = lines
            .iter()
            .find(|l| l.starts_with("destination "))
            .unwrap_or_else(|| panic!("{name}: no destination line in {lines:#?}"));
        for want in expect {
            assert!(
                dest.contains(want),
                "{name} on {fs}: {dest:?} lacks {want:?}"
            );
        }
        if format == Format::Folder {
            assert_eq!(std::fs::read(output.join("data/big.bin")).unwrap(), data);
            assert!(output.join("data/empty").is_dir());
        }
        assert_clean(&out);
    }

    // A job that fails late: its image source grows while it is written, which the stamp
    // check before publishing catches. Nothing is published and nothing is left behind.
    let source = out.join("game.ffpfs");
    let late = out.join("late.exfat");
    let grown = AtomicBool::new(false);
    let changed = source.clone();
    let (lines, result) = run_with(request(&source, Format::Exfat, &late, None), move |e| {
        if let Event::Progress { stage, .. } = e
            && stage == "write"
            && !grown.swap(true, Ordering::Relaxed)
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&changed)
                .unwrap();
            f.write_all(b"grown").unwrap();
        }
    });
    let err = result.expect_err("a source that changed mid-job must fail it");
    assert!(
        err.contains("changed while it was being converted"),
        "{err}\n{lines:#?}"
    );
    assert!(!late.exists(), "a failed job published its output");
    assert_clean(&out);
    let _ = std::fs::remove_dir_all(&base);
}
