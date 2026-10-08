//! The destination probe (U1 in README.md, "Writing safely on the PS5"): what the output
//! folder's filesystem can do, asked of the folder itself once per job, before anything is
//! written, and never guessed from its path. A PS5 USB drive is never tested on hardware, so whatever the probe is not
//! sure of counts as the stricter answer (fail closed).
//!
//! Every step goes through one held descriptor of the folder (`openat`, `fstatat`, `linkat`),
//! which is also right under `nullfs` (`/data` can be a nullfs view). FreeBSD (the PS5) only;
//! it also builds on macOS for its tests.

use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, bail};

use crate::durable::{Synced, errno, sync_retry};
use crate::preflight::FAT_MAX_FILE;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// The probe file's size: big enough to need real blocks, small enough for a nearly full drive.
const PROBE_LEN: usize = 1 << 20;

/// What the probe saw.
#[derive(Debug)]
pub(crate) struct Dest {
    /// `st_dev` of the folder, through the view it was opened by.
    pub dev: u64,
    /// Bytes an unprivileged user may still write.
    pub free: u64,
    pub fstype: String,
    /// Where the filesystem is mounted (`f_mntonname`).
    pub mount: PathBuf,
    /// The largest file the folder can hold.
    pub max_file: u64,
    /// Names that differ only in case or Unicode normalization may name one file (or it is
    /// not known that they don't).
    pub folds_names: bool,
    /// Creating a non-ASCII name was refused.
    pub ascii_only: bool,
    pub hard_links: bool,
    /// Why an answer is a fail-closed guess, for the log.
    pub notes: Vec<String>,
}

impl Dest {
    /// One log line: `destination /mnt/usb0: exfatfs, 118 GiB free, 64-bit files, ...`.
    pub(crate) fn describe(&self) -> String {
        let free = if self.free >= GIB {
            format!("{} GiB", self.free / GIB)
        } else {
            format!("{} MiB", self.free / MIB)
        };
        let files = match self.max_file {
            m if m >= i64::MAX as u64 => "64-bit files".to_string(),
            FAT_MAX_FILE => "files under 4 GiB".to_string(),
            m => format!("{}-bit files", 64 - m.leading_zeros()),
        };
        let names = if self.folds_names {
            "folds names"
        } else {
            "case-sensitive names"
        };
        let ascii = if self.ascii_only {
            ", ASCII names only"
        } else {
            ""
        };
        let links = if self.hard_links {
            "hard links"
        } else {
            "no hard links"
        };
        format!(
            "destination {}: {}, {free} free, {files}, {names}{ascii}, {links}",
            self.mount.display(),
            self.fstype
        )
    }
}

/// Probes `dir` (the output's folder) and removes every probe file again. Any failure refuses
/// the job, quoting the step and errno.
pub(crate) fn probe(dir: &Path, cancel: &AtomicBool) -> anyhow::Result<Dest> {
    let step = |what: &str| format!("destination probe of {}: {what}", dir.display());
    let folder = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(dir)
        .with_context(|| step("opening the folder"))?;
    let fs = Statfs::of(&folder).with_context(|| step("fstatfs"))?;
    if fs.read_only {
        bail!(
            "{} is on a read-only filesystem ({} at {})",
            dir.display(),
            fs.fstype,
            fs.mount.display()
        );
    }
    let dev = folder
        .metadata()
        .with_context(|| step("fstat of the folder"))?
        .dev();
    let mut notes = Vec::new();

    let pid = std::process::id();
    let mut probes = Probes {
        folder: &folder,
        cancel,
        open: Vec::new(),
        made: Vec::new(),
    };
    let name = c_name(&format!(".forge-probe-{pid}.part"));
    let file = probes
        .create(&name)
        .with_context(|| step("creating the probe file"))?;
    let pattern: Vec<u8> = (0..PROBE_LEN).map(|i| (i % 251) as u8 ^ 0x5a).collect();
    probes.open[file]
        .write_all(&pattern)
        .with_context(|| step("writing the probe file"))?;
    if sync_retry(&probes.open[file], false, cancel).with_context(|| step("fsync of the probe"))?
        == Synced::Retried
    {
        notes.push(format!(
            "{}: the probe file's fsync only succeeded on a retry",
            dir.display()
        ));
    }
    let mut back = vec![0; PROBE_LEN];
    probes.open[file]
        .read_exact_at(&mut back, 0)
        .with_context(|| step("reading the probe file back"))?;
    if back != pattern {
        bail!("{}", step("the probe file read back different bytes"));
    }
    let meta = probes.open[file]
        .metadata()
        .with_context(|| step("fstat of the probe file"))?;
    if meta.dev() != dev {
        bail!(
            "{}",
            step(&format!(
                "the probe file is on device {}, the folder on {dev}",
                meta.dev()
            ))
        );
    }
    let id = (meta.dev(), meta.ino());

    let (max_file, note) =
        max_file(file_size_bits(&folder), &fs.fstype).with_context(|| step("fpathconf"))?;
    notes.extend(note);

    let upper = c_name(&format!(".FORGE-PROBE-{pid}.PART"));
    let case = fold(lookup(&folder, &upper), id);
    // The same name in NFC, looked up in NFD.
    let nfc = c_name(&format!(".forge-probe-{pid}-\u{e9}.part"));
    let nfd = c_name(&format!(".forge-probe-{pid}-e\u{301}.part"));
    let (normalization, ascii_only) = match probes.create(&nfc) {
        Ok(i) => {
            let meta = probes.open[i]
                .metadata()
                .with_context(|| step("fstat of the non-ASCII probe"))?;
            (fold(lookup(&folder, &nfd), (meta.dev(), meta.ino())), false)
        }
        Err(e) if matches!(errno(&e), Some(libc::EINVAL | libc::EILSEQ)) => {
            // No non-ASCII names at all: folder output lists them as findings instead.
            (Fold::Distinct, true)
        }
        Err(e) => return Err(e).with_context(|| step("creating the non-ASCII probe")),
    };
    for (what, answer) in [("case", &case), ("Unicode normalization", &normalization)] {
        if let Fold::Unknown(why) = answer {
            notes.push(format!(
                "{}: whether names differing in {what} are distinct is unknown ({why}); \
                 treated as folding",
                dir.display()
            ));
        }
    }
    let folds_names = case != Fold::Distinct || normalization != Fold::Distinct;

    let link = c_name(&format!(".forge-probe-{pid}.l.part"));
    let hard_links = loop {
        match linkat(&folder, &name, &link) {
            Ok(()) => {
                probes.made.push((link, file));
                break true;
            }
            Err(e) => match link_failure(&e) {
                LinkFailure::Unsupported => break false,
                LinkFailure::Retry => {}
                LinkFailure::Refuse => return Err(e).with_context(|| step("linkat")),
            },
        }
    };
    drop(probes);

    Ok(Dest {
        dev,
        free: fs.free,
        fstype: fs.fstype,
        mount: fs.mount,
        max_file,
        folds_names,
        ascii_only,
        hard_links,
        notes,
    })
}

/// The probe's files. Dropping it checks each name still names the file this probe made,
/// closes the handles, removes the names and syncs the folder (best effort, U6).
struct Probes<'a> {
    folder: &'a File,
    cancel: &'a AtomicBool,
    open: Vec<File>,
    /// Each name made, with the index in `open` of the file it must name.
    made: Vec<(CString, usize)>,
}

impl Probes<'_> {
    /// Creates `name` (exclusive, not following a link) and returns its index in `open`.
    fn create(&mut self, name: &CStr) -> io::Result<usize> {
        let flags =
            libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_RDWR | libc::O_CLOEXEC;
        // SAFETY: a valid descriptor and NUL-terminated name; the mode is a plain integer.
        let fd = unsafe {
            libc::openat(
                self.folder.as_raw_fd(),
                name.as_ptr(),
                flags,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a new descriptor nothing else owns.
        self.open.push(unsafe { File::from_raw_fd(fd) });
        let i = self.open.len() - 1;
        self.made.push((name.to_owned(), i));
        Ok(i)
    }
}

impl Drop for Probes<'_> {
    fn drop(&mut self) {
        // Identity first, while the handles are open; then close them before any unlink.
        let ids: Vec<Option<(u64, u64)>> = self
            .open
            .iter()
            .map(|f| f.metadata().ok().map(|m| (m.dev(), m.ino())))
            .collect();
        self.open.clear();
        for (name, i) in &self.made {
            if ids[*i].is_some_and(|id| lookup(self.folder, name).is_ok_and(|at| at == id)) {
                // SAFETY: a valid descriptor and NUL-terminated name.
                unsafe { libc::unlinkat(self.folder.as_raw_fd(), name.as_ptr(), 0) };
            }
        }
        let _ = sync_retry(self.folder, true, self.cancel);
    }
}

fn c_name(name: &str) -> CString {
    CString::new(name).expect("probe names hold no NUL")
}

/// The (dev, ino) `name` in `folder` names, not following a link.
#[allow(clippy::unnecessary_cast)] // field types differ between FreeBSD and macOS
fn lookup(folder: &File, name: &CStr) -> io::Result<(u64, u64)> {
    // SAFETY: valid descriptor and name; `st` is a properly sized out-parameter.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::fstatat(
            folder.as_raw_fd(),
            name.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((st.st_dev as u64, st.st_ino as u64))
}

fn linkat(folder: &File, from: &CStr, to: &CStr) -> io::Result<()> {
    let fd = folder.as_raw_fd();
    // SAFETY: valid descriptor and NUL-terminated names; no flags (a link is not followed).
    if unsafe { libc::linkat(fd, from.as_ptr(), fd, to.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// What one name lookup says about folding.
#[derive(Debug, PartialEq, Eq)]
enum Fold {
    /// Not found: the filesystem keeps the two spellings apart.
    Distinct,
    /// Found the probe itself.
    Folds,
    Unknown(String),
}

/// `found`: the other spelling's lookup; `probe`: the (dev, ino) of the file it was made from.
/// Only "not found" proves the spellings distinct.
fn fold(found: io::Result<(u64, u64)>, probe: (u64, u64)) -> Fold {
    match found {
        Ok(id) if id == probe => Fold::Folds,
        Ok(_) => Fold::Unknown("the other spelling names another file".into()),
        Err(e) if errno(&e) == Some(libc::ENOENT) => Fold::Distinct,
        Err(e) => Fold::Unknown(e.to_string()),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LinkFailure {
    /// No hard links here (FreeBSD's msdosfs, perhaps Sony's exfatfs).
    Unsupported,
    Retry,
    /// Anything else is a broken folder, never "unsupported".
    Refuse,
}

/// Why `linkat` on the fresh probe failed. EPERM counts as unsupported only because the probe
/// was just created by us, with no flags, in a folder we just wrote to. Comparisons, not a
/// match: ENOTSUP == EOPNOTSUPP on FreeBSD.
fn link_failure(e: &io::Error) -> LinkFailure {
    match errno(e) {
        Some(c) if c == libc::EOPNOTSUPP || c == libc::ENOTSUP || c == libc::EPERM => {
            LinkFailure::Unsupported
        }
        Some(libc::EINTR) => LinkFailure::Retry,
        _ => LinkFailure::Refuse,
    }
}

/// `fpathconf(_PC_FILESIZEBITS)`: Ok(None) when it reports no value (-1 with errno untouched).
fn file_size_bits(folder: &File) -> io::Result<Option<libc::c_long>> {
    // SAFETY: `__error` is this thread's errno; `fpathconf` takes a valid descriptor.
    unsafe { *libc::__error() = 0 };
    let bits = unsafe { libc::fpathconf(folder.as_raw_fd(), libc::_PC_FILESIZEBITS) };
    if bits != -1 {
        return Ok(Some(bits));
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(0) {
        return Ok(None);
    }
    Err(e)
}

/// The largest file: from the reported bit count when it is believable, else from the
/// filesystem's name. Anything unknown gets FAT32's limit, with a note. Only EINVAL (the
/// question is unsupported) falls back; any other error (EIO, ENODEV, ...) refuses the job.
fn max_file(
    bits: io::Result<Option<libc::c_long>>,
    fstype: &str,
) -> io::Result<(u64, Option<String>)> {
    // FreeBSD's msdosfs reports 32, meaning FAT32's 4 GiB - 1 (the count is unsigned there).
    let why = match bits {
        Ok(Some(64)) => return Ok((i64::MAX as u64, None)),
        Ok(Some(b @ 1..=63)) => return Ok(((1u64 << b) - 1, None)),
        Ok(Some(b)) => format!("it reported {b} bits"),
        Ok(None) => "it reported no value".to_string(),
        Err(e) if errno(&e) == Some(libc::EINVAL) => e.to_string(),
        Err(e) => return Err(e),
    };
    Ok(match fstype {
        "msdosfs" => (FAT_MAX_FILE, None),
        "exfatfs" | "ufs" | "bfs" => (i64::MAX as u64, None),
        _ => (
            FAT_MAX_FILE,
            Some(format!(
                "{fstype}: file size limit unknown (_PC_FILESIZEBITS: {why}); files are capped \
                 at 4 GiB - 1"
            )),
        ),
    })
}

/// The mount point `path` is on (`statfs`), for U7's drive-removal check.
pub(crate) fn mount_of(path: &Path) -> io::Result<PathBuf> {
    Ok(Statfs::at(path)?.mount)
}

/// A mount point's (or a folder's) `(st_dev, f_blocks)` now: a removed drive fails the stat,
/// shows another device (the folder under it) or no blocks (U7).
pub(crate) fn mount_state(mount: &Path) -> io::Result<(u64, u64)> {
    Ok((std::fs::metadata(mount)?.dev(), Statfs::at(mount)?.blocks))
}

/// The filesystem type `path` is on, for tests that depend on it.
#[cfg(all(test, target_os = "freebsd"))]
pub(crate) fn fstype_of(path: &Path) -> io::Result<String> {
    Ok(Statfs::at(path)?.fstype)
}

/// What `fstatfs` says about the folder's filesystem.
struct Statfs {
    free: u64,
    blocks: u64,
    fstype: String,
    mount: PathBuf,
    read_only: bool,
}

impl Statfs {
    fn of(folder: &File) -> io::Result<Self> {
        // SAFETY: a valid descriptor; `st` is a properly sized out-parameter. On the PS5 this
        // is `fstatfs@FBSD_1.0`, forwarded by ps5/compat.c.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatfs(folder.as_raw_fd(), &mut st) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self::from(&st))
    }

    fn at(path: &Path) -> io::Result<Self> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
        // SAFETY: a valid C string; `st` as in `of`. On the PS5 this is `statfs@FBSD_1.0`.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(path.as_ptr(), &mut st) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self::from(&st))
    }

    #[allow(clippy::unnecessary_cast)] // field types differ between FreeBSD and macOS
    fn from(st: &libc::statfs) -> Self {
        let text = |chars: &[libc::c_char]| -> Vec<u8> {
            chars
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect()
        };
        Self {
            free: u64::try_from(st.f_bavail as i64)
                .unwrap_or(0)
                .saturating_mul(st.f_bsize as u64),
            blocks: st.f_blocks as u64,
            fstype: String::from_utf8_lossy(&text(&st.f_fstypename)).into_owned(),
            mount: PathBuf::from(OsStr::from_bytes(&text(&st.f_mntonname))),
            read_only: st.f_flags as u64 & libc::MNT_RDONLY as u64 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    #[test]
    fn link_errors_are_classified() {
        for code in [libc::EOPNOTSUPP, libc::ENOTSUP, libc::EPERM] {
            assert_eq!(link_failure(&os(code)), LinkFailure::Unsupported, "{code}");
        }
        assert_eq!(link_failure(&os(libc::EINTR)), LinkFailure::Retry);
        let sony = (0x8002_0000_u32 | libc::EINTR as u32) as i32;
        assert_eq!(link_failure(&os(sony)), LinkFailure::Retry);
        for code in [
            libc::EACCES,
            libc::ENOSPC,
            libc::EIO,
            libc::EXDEV,
            libc::EROFS,
            libc::EEXIST,
        ] {
            assert_eq!(link_failure(&os(code)), LinkFailure::Refuse, "{code}");
        }
        assert_eq!(
            link_failure(&io::Error::other("no errno")),
            LinkFailure::Refuse
        );
    }

    #[test]
    fn only_not_found_means_distinct() {
        let probe = (1, 2);
        assert_eq!(fold(Err(os(libc::ENOENT)), probe), Fold::Distinct);
        assert_eq!(fold(Ok(probe), probe), Fold::Folds);
        assert!(matches!(fold(Ok((1, 3)), probe), Fold::Unknown(_)));
        assert!(matches!(fold(Err(os(libc::EIO)), probe), Fold::Unknown(_)));
    }

    #[test]
    fn file_size_limits_fail_closed() {
        let max = |bits, fstype| max_file(bits, fstype).unwrap();
        assert_eq!(max(Ok(Some(64)), "x"), (i64::MAX as u64, None));
        assert_eq!(max(Ok(Some(32)), "x"), (FAT_MAX_FILE, None));
        assert_eq!(max(Ok(Some(40)), "x").0, (1 << 40) - 1);
        let unsupported = || Err(os(libc::EINVAL));
        assert_eq!(max(unsupported(), "msdosfs"), (FAT_MAX_FILE, None));
        let sony_einval = (0x8002_0000_u32 | libc::EINVAL as u32) as i32;
        for name in ["exfatfs", "ufs", "bfs"] {
            assert_eq!(max(unsupported(), name), (i64::MAX as u64, None));
            assert_eq!(max(Err(os(sony_einval)), name), (i64::MAX as u64, None));
            assert_eq!(max(Ok(None), name), (i64::MAX as u64, None));
            // A broken drive is not "unsupported": no name lets it through.
            for code in [libc::EIO, libc::ENODEV, libc::EBADF] {
                let e = max_file(Err(os(code)), name).unwrap_err();
                assert_eq!(e.raw_os_error(), Some(code));
            }
        }
        for bits in [
            unsupported(),
            Ok(None),
            Ok(Some(0)),
            Ok(Some(65)),
            Ok(Some(-1)),
        ] {
            let (limit, note) = max(bits, "nullfs");
            assert_eq!(limit, FAT_MAX_FILE);
            assert!(note.unwrap().contains("nullfs"));
        }
    }

    #[test]
    fn describe_is_one_line() {
        let dest = Dest {
            dev: 1,
            free: 118 * GIB + 5,
            fstype: "exfatfs".into(),
            mount: "/mnt/usb0".into(),
            max_file: i64::MAX as u64,
            folds_names: true,
            ascii_only: false,
            hard_links: false,
            notes: Vec::new(),
        };
        assert_eq!(
            dest.describe(),
            "destination /mnt/usb0: exfatfs, 118 GiB free, 64-bit files, folds names, \
             no hard links"
        );
        let fat = Dest {
            free: 3 * MIB,
            fstype: "msdosfs".into(),
            max_file: FAT_MAX_FILE,
            folds_names: false,
            ascii_only: true,
            hard_links: true,
            ..dest
        };
        assert_eq!(
            fat.describe(),
            "destination /mnt/usb0: msdosfs, 3 MiB free, files under 4 GiB, \
             case-sensitive names, ASCII names only, hard links"
        );
        let apfs = Dest {
            max_file: (1 << 56) - 1,
            ..fat
        };
        assert!(apfs.describe().contains(", 56-bit files,"));
    }

    /// On this Mac the temp folder is APFS: it folds case (and normalization), has hard links
    /// and no 4 GiB limit. In the FreeBSD VM it is whatever volume `TMPDIR` names.
    #[test]
    fn probes_the_temp_folder_and_leaves_nothing() {
        let dir = crate::test_dir("dest-probe");
        let dest = probe(&dir, &AtomicBool::new(false)).unwrap();
        assert!(dest.free > 0, "{dest:?}");
        assert!(dest.max_file >= FAT_MAX_FILE, "{dest:?}");
        assert_eq!(dest.dev, std::fs::metadata(&dir).unwrap().dev());
        assert!(dest.mount.is_absolute(), "{dest:?}");
        assert!(dest.describe().contains(&dest.fstype));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "probe files left"
        );
        if cfg!(target_os = "macos") {
            assert_eq!(dest.fstype, "apfs", "{dest:?}");
            assert!(dest.max_file > FAT_MAX_FILE, "{dest:?}"); // APFS reports 56 bits
            assert!(dest.folds_names && !dest.ascii_only && dest.hard_links);
            assert!(dest.notes.is_empty(), "{:?}", dest.notes);
        }
        let _ = std::fs::remove_dir(&dir);
    }

    /// What U7 records and re-checks, on the temp folder's real mount.
    #[test]
    fn the_temp_folder_has_a_live_mount() {
        let dir = crate::test_dir("dest-mount");
        let mount = mount_of(&dir).unwrap();
        assert!(mount.is_absolute(), "{}", mount.display()); // macOS firmlinks: no prefix
        let (dev, blocks) = mount_state(&mount).unwrap();
        assert_eq!(dev, std::fs::metadata(&dir).unwrap().dev());
        assert!(blocks > 0);
        assert!(mount_state(&dir.join("nope")).is_err());
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn a_missing_folder_names_the_step() {
        let dir = crate::test_dir("dest-missing").join("nope");
        let e = probe(&dir, &AtomicBool::new(false)).unwrap_err();
        let msg = format!("{e:#}");
        assert!(
            msg.contains("opening the folder") && msg.contains("os error"),
            "{msg}"
        );
    }
}
