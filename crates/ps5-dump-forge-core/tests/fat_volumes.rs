//! Every target written onto real exFAT and FAT32 volumes (macOS `hdiutil`, no root needed).
//! macOS's exFAT and FAT drivers give a new, empty file a placeholder inode and renumber it
//! at its first write, which once made every image job refuse to publish its own `.part`;
//! macOS's exFAT driver also has no `RENAME_EXCL`, so every publish there takes the checked
//! rename.
//!
//! Ignored by default (it attaches disk images); CI's macOS leg runs it:
//! `cargo test --release -p ps5-dump-forge-core --test fat_volumes -- --ignored`.
//! When run, a setup or teardown failure fails the test rather than skipping it.
#![cfg(target_os = "macos")]

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use ps5upload_fpkg::source::SourceTree;

use ps5_dump_forge_core::{
    ConvertRequest, Event, Format, JobId, JobReport, Jobs, ScannedFolder, rename_no_replace,
    stale_parts,
};

/// An `hdiutil` volume attached at `base/mnt`. [`Volume::detach`] is the checked teardown;
/// drop is the best-effort one for a failing test.
struct Volume {
    base: PathBuf,
    mnt: PathBuf,
    /// The whole-disk device (`/dev/diskN`) while attached.
    device: Option<String>,
}

impl Volume {
    fn attach(name: &str, fs: &str, volname: &str, fstype: &str) -> Self {
        let base =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let mut vol = Self {
            mnt: base.join("mnt"),
            base,
            device: None,
        };
        let image = vol.base.join("vol.sparseimage");
        let _ = std::fs::remove_file(&image);
        // Sparse: room for every output at once without writing 600 MB up front.
        hdiutil(
            Command::new("hdiutil")
                .args(["create", "-size", "600m", "-type", "SPARSE", "-fs", fs])
                .args(["-volname", volname])
                .arg(&image),
        );
        std::fs::create_dir_all(&vol.mnt).unwrap();
        let plist = hdiutil(
            Command::new("hdiutil")
                .args(["attach", "-plist", "-nobrowse", "-mountpoint"])
                .arg(&vol.mnt)
                .arg(&image),
        );
        // Every `dev-entry`: the whole disk and its slices; the shortest is the disk.
        vol.device = plist
            .lines()
            .filter_map(|l| l.trim().strip_prefix("<string>/dev/disk"))
            .filter_map(|l| l.strip_suffix("</string>"))
            .map(|d| format!("/dev/disk{d}"))
            .min_by_key(String::len);
        assert!(
            vol.device.is_some(),
            "no device in hdiutil attach output:\n{plist}"
        );
        assert_eq!(
            fs_type(&vol.mnt),
            fstype,
            "{} is not {fs}",
            vol.mnt.display()
        );
        vol
    }

    /// Detaches the disk (retrying with `-force`), then deletes `base`. Never deletes
    /// through a disk it could not detach.
    fn detach(&mut self) -> Result<(), String> {
        if let Some(device) = &self.device {
            let detach = |force: bool| {
                let mut cmd = Command::new("hdiutil");
                cmd.arg("detach").arg(device);
                if force {
                    cmd.arg("-force");
                }
                cmd.output()
                    .map_err(|e| e.to_string())
                    .and_then(|o| match o.status.success() {
                        true => Ok(()),
                        false => Err(String::from_utf8_lossy(&o.stderr).into_owned()),
                    })
            };
            detach(false)
                .or_else(|_| detach(true))
                .map_err(|e| format!("detaching {device}: {e}"))?;
            self.device = None;
        }
        match std::fs::remove_dir_all(&self.base) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("removing {}: {e}", self.base.display()))
            }
            _ => Ok(()),
        }
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        if let Err(e) = self.detach() {
            eprintln!("{e}");
        }
    }
}

/// Runs `cmd`, failing the test unless it succeeds; returns its stdout.
fn hdiutil(cmd: &mut Command) -> String {
    let out = cmd.output().expect("running hdiutil");
    assert!(
        out.status.success(),
        "{cmd:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn c_path(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

/// The mounted filesystem's type name (`exfat`, `msdos`, `apfs`, ...).
fn fs_type(path: &Path) -> String {
    let c = c_path(path);
    // SAFETY: a valid NUL-terminated path and a zeroed out-struct of the right type.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::statfs(c.as_ptr(), &mut st) }, 0);
    // SAFETY: the kernel NUL-terminates f_fstypename.
    unsafe { std::ffi::CStr::from_ptr(st.f_fstypename.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

const XATTR: &std::ffi::CStr = c"com.quer3q.forge-test";

/// Sets an xattr on `path`; exFAT and FAT keep it in an AppleDouble `._` sidecar.
fn set_xattr(path: &Path) {
    let c = c_path(path);
    // SAFETY: valid NUL-terminated path and name, a 4-byte value.
    let rc =
        unsafe { libc::setxattr(c.as_ptr(), XATTR.as_ptr(), b"mark".as_ptr().cast(), 4, 0, 0) };
    assert_eq!(
        rc,
        0,
        "setxattr {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
}

fn has_xattr(path: &Path) -> bool {
    let c = c_path(path);
    let mut buf = [0u8; 16];
    // SAFETY: valid NUL-terminated path and name, a buffer of the length given.
    let n = unsafe {
        libc::getxattr(
            c.as_ptr(),
            XATTR.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            0,
        )
    };
    n == 4 && &buf[..4] == b"mark"
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from("._");
    name.push(path.file_name().unwrap());
    path.with_file_name(name)
}

/// The job's `.part` in `dir`, given an xattr so its sidecar exists for sure.
fn mark_part(dir: &Path) -> PathBuf {
    let parts: Vec<PathBuf> = stale_parts(dir)
        .into_iter()
        .filter(|p| !p.file_name().unwrap().as_bytes().starts_with(b"._"))
        .collect();
    assert_eq!(parts.len(), 1, "{parts:?}");
    set_xattr(&parts[0]);
    assert!(
        sidecar(&parts[0]).exists(),
        "no sidecar for {}",
        parts[0].display()
    );
    parts[0].clone()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A small game.
fn game(root: &Path) {
    write(root, "eboot.bin", b"\x7fELF fake eboot");
    write(
        root,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    let big: Vec<u8> = (0..3 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
    write(root, "data/big.bin", &big);
    write(root, "data/zero.bin", b"");
    std::fs::create_dir_all(root.join("data/empty")).unwrap();
}

fn request(source: &Path, format: Format, output: &Path) -> ConvertRequest {
    ConvertRequest {
        source: source.to_path_buf(),
        format,
        output: output.to_path_buf(),
        compression_threads: None,
        inner: None,
        remove_backport: false,
    }
}

/// Runs on the test thread while the job is held; may cancel it.
type AtPause<'a> = dyn Fn(&Jobs, JobId) + 'a;

enum Msg {
    Paused,
    Done(Result<JobReport, String>),
}

/// Runs one job to completion. With `pause`, the job is held at that stage's first progress
/// while the closure runs (it may cancel the job), then released.
fn run(req: ConvertRequest, pause: Option<(&'static str, &AtPause)>) -> Result<JobReport, String> {
    const WAIT: Duration = Duration::from_secs(300);
    let (tx, rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    // The callback runs on the worker: blocking it holds the job at a known point.
    let state = Mutex::new((tx, go_rx, pause.map(|p| p.0)));
    let jobs = Jobs::new(move |e| {
        let mut s = state.lock().unwrap();
        match e {
            Event::Progress { ref stage, .. } if s.2 == Some(stage.as_str()) => {
                s.2 = None;
                let _ = s.0.send(Msg::Paused);
                // Returns early when the test side drops `go_tx`.
                let _ = s.1.recv();
            }
            Event::Done { result, .. } => {
                let _ = s.0.send(Msg::Done(result));
            }
            _ => {}
        }
    });
    let job = jobs.start(req);
    let mut msg = rx.recv_timeout(WAIT);
    if let (Ok(Msg::Paused), Some((_, at_pause))) = (&msg, pause) {
        // A panic in `at_pause` unwinds after `go_tx` is dropped and the job cancelled below.
        let released =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| at_pause(&jobs, job)));
        let _ = go_tx.send(());
        if let Err(panic) = released {
            jobs.cancel_all_and_wait();
            std::panic::resume_unwind(panic);
        }
        msg = rx.recv_timeout(WAIT);
    }
    drop(go_tx);
    match msg {
        Ok(Msg::Done(result)) => result,
        Ok(Msg::Paused) => unreachable!("a job pauses once"),
        Err(e) => {
            jobs.cancel_all_and_wait();
            panic!("the job did not finish: {e}");
        }
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

/// No `.part` and no `._` sidecar without its file in `dir`.
fn assert_clean(dir: &Path, what: &str) {
    assert_eq!(stale_parts(dir), Vec::<PathBuf>::new(), "{what}");
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name();
        if let Some(main) = name.as_bytes().strip_prefix(b"._") {
            let main = dir.join(std::ffi::OsStr::from_bytes(main));
            assert!(main.symlink_metadata().is_ok(), "{what}: orphan {name:?}");
        }
    }
}

/// The publish rename on this volume, called directly: an existing name, in any case, is
/// never replaced, and a free one is taken.
fn renames_never_replace(mnt: &Path) {
    let root = mnt.join("rename");
    std::fs::create_dir(&root).unwrap();
    write(&root, "a.part", b"ours");
    write(&root, "a.exfat", b"theirs");
    std::fs::create_dir(root.join("d.part")).unwrap();
    write(&root, "d.part/f", b"ours");
    std::fs::create_dir(root.join("d")).unwrap();
    for (from, to) in [
        ("a.part", "a.exfat"),
        ("a.part", "A.EXFAT"),
        ("d.part", "d"),
        ("d.part", "D"),
        ("d.part", "a.exfat"),
    ] {
        let err = rename_no_replace(&root.join(from), &root.join(to)).unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::AlreadyExists,
            "{from} -> {to}"
        );
    }
    assert_eq!(std::fs::read(root.join("a.part")).unwrap(), b"ours");
    assert_eq!(std::fs::read(root.join("a.exfat")).unwrap(), b"theirs");
    assert_eq!(std::fs::read(root.join("d.part/f")).unwrap(), b"ours");
    assert_eq!(std::fs::read_dir(root.join("d")).unwrap().count(), 0);

    rename_no_replace(&root.join("a.part"), &root.join("b.exfat")).unwrap();
    rename_no_replace(&root.join("d.part"), &root.join("e")).unwrap();
    assert_eq!(std::fs::read(root.join("b.exfat")).unwrap(), b"ours");
    assert_eq!(std::fs::read(root.join("e/f")).unwrap(), b"ours");
    assert_clean(&root, "direct renames");
}

/// Something appears at the output (`taken`, created in `dir`) while the job holds at
/// `finalize`: the publish fails, that entry is untouched and the job's part is gone.
fn output_taken_at_publish(
    src: &Path,
    format: Format,
    dir: &Path,
    out: &str,
    taken: &str,
    as_dir: bool,
) {
    std::fs::create_dir(dir).unwrap();
    let what = format!(
        "{} {out} vs {taken}{}",
        dir.display(),
        if as_dir { "/" } else { "" }
    );
    let at_pause = |_: &Jobs, _: JobId| {
        mark_part(dir);
        if as_dir {
            write(dir, &format!("{taken}/keep"), b"theirs");
        } else {
            write(dir, taken, b"theirs");
        }
    };
    let err = run(
        request(src, format, &dir.join(out)),
        Some(("finalize", &at_pause)),
    )
    .expect_err(&what);
    assert!(err.contains("File exists"), "{what}: {err}");
    let names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| !n.as_bytes().starts_with(b"._"))
        .collect();
    assert_eq!(names, [taken], "{what}");
    let kept = if as_dir {
        dir.join(taken).join("keep")
    } else {
        dir.join(taken)
    };
    assert_eq!(std::fs::read(kept).unwrap(), b"theirs", "{what}");
    assert_clean(dir, &what);
}

fn every_target_publishes_on(name: &str, fs: &str, volname: &str, fstype: &str) {
    let mut vol = Volume::attach(name, fs, volname, fstype);
    // The source sits on the system disk, next to the image the volume's teardown deletes.
    let src = vol.base.join("src");
    game(&src);
    let mnt = vol.mnt.clone();
    renames_never_replace(&mnt);

    // Cancelled once its bytes are written and fsynced: the part and its sidecar go,
    // nothing is published.
    for (format, ext) in [
        (Format::Exfat, "exfat"),
        (Format::Ffpfsc, "ffpfsc"),
        (Format::Pkg, "pkg"),
    ] {
        let out = mnt.join(format!("cancelled.{ext}"));
        let cancel = |jobs: &Jobs, job: JobId| {
            mark_part(&mnt);
            jobs.cancel(job);
        };
        let err = run(request(&src, format, &out), Some(("verify", &cancel))).unwrap_err();
        assert_eq!(err, "cancelled", "{ext}");
        assert!(!out.exists(), "{ext}");
        assert_clean(&mnt, &format!("cancelled {ext}"));
    }

    // Each image, then each image back to a folder. The part carries an xattr into the
    // publish: its sidecar must follow it to the output name.
    let publish = |req: ConvertRequest, what: &str| {
        let out = req.output.clone();
        let mark = |_: &Jobs, _: JobId| {
            mark_part(&mnt);
        };
        let report =
            run(req, Some(("finalize", &mark))).unwrap_or_else(|e| panic!("{what} on {fs}: {e}"));
        assert!(
            has_xattr(&out),
            "{what}: the xattr did not follow the rename"
        );
        assert!(sidecar(&out).exists(), "{what}");
        assert_clean(&mnt, what);
        report
    };
    for (format, ext) in [
        (Format::Exfat, "exfat"),
        (Format::Ffpkg, "ffpkg"),
        (Format::Ffpfs, "ffpfs"),
        (Format::Ffpfsc, "ffpfsc"),
        (Format::Pkg, "pkg"),
    ] {
        let image = mnt.join(format!("PPSA01234.{ext}"));
        let report = publish(request(&src, format, &image), &format!("--to {ext}"));
        assert_eq!(report.bytes, std::fs::metadata(&image).unwrap().len());

        let back = mnt.join(format!("back-{ext}"));
        publish(
            request(&image, Format::Folder, &back),
            &format!("{ext} --to folder"),
        );
        // A package rewrites param.json and adds a keystone; every other file is the source's.
        let got = tree_bytes(&back);
        for file in tree_bytes(&src) {
            if format != Format::Pkg || file.0 != "sce_sys/param.json" {
                assert!(
                    got.contains(&file),
                    "{ext}: {} differs or is missing",
                    file.0
                );
            }
        }
        assert!(back.join("data/empty").is_dir());
    }

    // The output name taken while the job holds at `finalize`: same name, another case and
    // a directory. The kernel's lookup refuses each before the driver's rename (exFAT too);
    // finalize.rs's unit tests cover the checked rename's own check.
    let image = mnt.join("PPSA01234.exfat");
    let taken = mnt.join("taken");
    std::fs::create_dir(&taken).unwrap();
    let cases = [
        ("file", "out.exfat", false),
        ("case", "OUT.EXFAT", false),
        ("dir", "out.exfat", true),
    ];
    for (case, taken_name, as_dir) in cases {
        let dir = taken.join(format!("image-{case}"));
        output_taken_at_publish(&src, Format::Exfat, &dir, "out.exfat", taken_name, as_dir);
        let dir = taken.join(format!("folder-{case}"));
        let taken_name = taken_name.replace(".exfat", "").replace(".EXFAT", "");
        output_taken_at_publish(&image, Format::Folder, &dir, "out", &taken_name, as_dir);
    }

    vol.detach().unwrap();
}

#[test]
#[ignore = "attaches an hdiutil volume; run with --ignored"]
fn every_target_publishes_on_exfat() {
    every_target_publishes_on("vol-exfat", "ExFAT", "FORGEEXFAT", "exfat");
}

#[test]
#[ignore = "attaches an hdiutil volume; run with --ignored"]
fn every_target_publishes_on_fat32() {
    every_target_publishes_on("vol-fat32", "MS-DOS FAT32", "FORGEFAT32", "msdos");
}
