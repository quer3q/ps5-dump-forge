//! The roots: where the file browser starts (`list_dir` with a `null` path). On a computer
//! they confine nothing; every path a request names goes to core as given, as in the Tauri
//! app. On the PS5 (by user request) listings and paired devices reach only them: `/data`
//! and each drive while it is mounted ([`Roots::confine`], [`Roots::reach`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::Platform;

/// The PS5's internal storage, whose free space the Library shows.
const DATA: &str = "/data";

/// Each root with the path it was named by (`/data` may resolve elsewhere).
type Named = Vec<(PathBuf, PathBuf)>;

pub(crate) struct Roots {
    list: Vec<Candidate>,
    ps5: bool,
    /// `statfs`; tests replace it.
    statfs: fn(&Path) -> Option<Mount>,
    /// [`Self::now`] kept this long for a paired device's calls ([`Self::reach`],
    /// [`Self::above`]): a transfer asks for every file; 0 in tests.
    ttl: Duration,
    cached: Mutex<Option<(Instant, Named)>>,
}

struct Candidate {
    path: PathBuf,
    /// The canonical path, pinned at startup. On the PS5 a candidate that doesn't exist then
    /// is resolved each time it is used, and so is every drive.
    pinned: Option<PathBuf>,
    /// A PS5 drive (`/mnt/usbN`, `/mnt/extN`, any `/mnt/...`): its folder exists with nothing
    /// plugged in, so it is listed only while a filesystem is mounted on it.
    drive: bool,
}

/// What `statfs` says of a path: where its filesystem is mounted, how many blocks it has, and
/// its size and free space in bytes.
#[cfg_attr(not(target_os = "freebsd"), allow(dead_code))] // built by FreeBSD's `statfs` only
pub(crate) struct Mount {
    on: PathBuf,
    blocks: u64,
    /// `f_blocks * f_bsize`.
    total: u64,
    /// `f_bavail * f_bsize`: what a user may still write (0 when the reserve is in use).
    free: u64,
}

/// A filesystem's free and total bytes, as the Library's scan reply carries them on the PS5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Storage {
    pub free: u64,
    pub total: u64,
}

impl Roots {
    /// Pins the roots; returns them with a line for the log about each one dropped (and,
    /// on the PS5, each one that resolves elsewhere: `/data` may be a symlink).
    pub fn new(platform: Platform, candidates: &[PathBuf]) -> (Self, Vec<String>) {
        let ps5 = platform == Platform::Ps5;
        let mut notes = Vec::new();
        let mut list = Vec::new();
        for path in candidates {
            let drive = ps5 && path.starts_with("/mnt");
            let candidate = |pinned| Candidate {
                path: path.clone(),
                pinned,
                drive,
            };
            match pin(path) {
                Ok(root) => {
                    if ps5 && root != *path {
                        notes.push(format!("root {} → {}", path.display(), root.display()));
                    }
                    list.push(candidate((!drive).then_some(root)));
                }
                // A PS5 drive that isn't plugged in is no news.
                Err(_) if ps5 && !path.exists() => list.push(candidate(None)),
                Err(why) => notes.push(format!("not serving {}: {why}", path.display())),
            }
        }
        (
            Self {
                list,
                ps5,
                statfs,
                ttl: Duration::from_secs(1),
                cached: Mutex::new(None),
            },
            notes,
        )
    }

    /// The roots that exist now (drives: that are mounted now), canonical, each once.
    pub fn current(&self) -> Vec<PathBuf> {
        self.now().into_iter().map(|(_, root)| root).collect()
    }

    /// The PS5's internal storage (`statfs` of `/data`): `None` on a computer, or when
    /// `statfs` fails. Asked for each Library scan, so Refresh renews it.
    pub fn storage(&self) -> Option<Storage> {
        if !self.ps5 {
            return None;
        }
        let fs = (self.statfs)(Path::new(DATA))?;
        Some(Storage {
            free: fs.free,
            total: fs.total,
        })
    }

    /// [`Self::now`], at most `ttl` old.
    fn recent(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, roots)) = cached.as_ref().filter(|(at, _)| at.elapsed() < self.ttl) {
            return roots.clone();
        }
        let roots = self.now();
        *cached = Some((Instant::now(), roots.clone()));
        roots
    }

    /// [`Self::current`], each with the path it was named by (`/data` may resolve elsewhere).
    fn now(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut roots: Vec<(PathBuf, PathBuf)> = Vec::new();
        for c in &self.list {
            let root = match &c.pinned {
                Some(root) => root.is_dir().then(|| root.clone()),
                None if self.ps5 => pin(&c.path)
                    .ok()
                    .filter(|r| !c.drive || mounted(r, (self.statfs)(r))),
                None => None,
            };
            if let Some(root) = root.filter(|r| roots.iter().all(|(_, x)| x != r)) {
                roots.push((c.path.clone(), root));
            }
        }
        roots
    }

    /// The PS5's rule for a path a request names (`list_dir`, `host_tree`, a Library folder):
    /// at or inside a root now, resolved as far as it exists (the rest as typed, never `..`).
    /// A computer, and another device's path (`smb://`, `flink://`, ...), pass. `Err`: the
    /// one line a 400 carries.
    pub fn confine(&self, path: &Path) -> Result<(), String> {
        if !self.ps5 || ps5_dump_forge_core::net::is_network(path) {
            return Ok(());
        }
        let now = self.now();
        match resolve(path) {
            Some(real) if now.iter().any(|(_, root)| real.starts_with(root)) => Ok(()),
            _ => Err(outside(path)),
        }
    }

    /// The PS5's rule for a paired device's path `p` (its folder resolved, its own name not):
    /// at or inside a root now, a root's own path as named giving the canonical one (so a
    /// linked `/data` reads as the folder); with `browse` (stat, list) also a folder above a
    /// root (`/`; `/mnt` while a drive is mounted). A computer: `p` as it is.
    pub fn reach(&self, p: &Path, browse: bool) -> Result<PathBuf, String> {
        if !self.ps5 {
            return Ok(p.to_path_buf());
        }
        let now = self.recent();
        // The snapshot may be a second old: a write under a drive asks that drive again, so
        // nothing lands in the mount point of one just unplugged (browsing may be stale).
        let still = |named: &PathBuf, root: &Path| {
            browse
                || !(self.list.iter()).any(|c| c.drive && c.path == *named)
                || mounted(root, (self.statfs)(root))
        };
        if let Some((named, root)) = now.iter().find(|(named, _)| named == p) {
            return match still(named, root) {
                true => Ok(root.clone()),
                false => Err(outside(p)),
            };
        }
        if let Some((named, root)) = now.iter().find(|(_, root)| p.starts_with(root)) {
            return match still(named, root) {
                true => Ok(p.to_path_buf()),
                false => Err(outside(p)),
            };
        }
        match browse && now.iter().any(|(named, _)| named.starts_with(p)) {
            true => Ok(p.to_path_buf()),
            false => Err(outside(p)),
        }
    }

    /// On the PS5, for a folder above the roots, the names in it that lead to one: all a
    /// paired device's listing of it keeps. `None`: list everything.
    pub fn above(&self, dir: &Path) -> Option<Vec<OsString>> {
        if !self.ps5 {
            return None;
        }
        let now = self.recent();
        if now.iter().any(|(_, root)| dir.starts_with(root)) {
            return None;
        }
        let mut names: Vec<OsString> = Vec::new();
        for (named, _) in &now {
            let next = named.strip_prefix(dir).ok().and_then(|r| r.iter().next());
            if let Some(name) = next.filter(|n| !names.iter().any(|x| x == n)) {
                names.push(name.to_owned());
            }
        }
        Some(names)
    }
}

#[cfg(all(test, unix))]
impl Roots {
    /// PS5 roots for tests, `(path, drive)` each, resolved on each use, with a fake `statfs`.
    pub(crate) fn fake(list: &[(PathBuf, bool)], statfs: fn(&Path) -> Option<Mount>) -> Roots {
        let list = (list.iter())
            .map(|(path, drive)| Candidate {
                path: path.clone(),
                pinned: None,
                drive: *drive,
            })
            .collect();
        Roots {
            list,
            ps5: true,
            statfs,
            ttl: Duration::ZERO,
            cached: Mutex::new(None),
        }
    }
}

/// A fake `statfs` for tests: a `mounted` file in a folder makes it a mount point.
#[cfg(all(test, unix))]
pub(crate) fn fake_statfs(p: &Path) -> Option<Mount> {
    let on = match p.join("mounted").exists() {
        true => p.to_path_buf(),
        false => p.parent()?.to_path_buf(),
    };
    Some(Mount {
        on,
        blocks: 1,
        total: 0,
        free: 0,
    })
}

/// The refusal of a path off the PS5's storage.
fn outside(p: &Path) -> String {
    format!(
        "{} is not on this console's storage or a plugged-in drive",
        p.display()
    )
}

/// `path` canonical as far as it exists, the rest appended as named; `None` for a relative
/// path or one whose missing part holds `..`.
fn resolve(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        if let Ok(mut real) = dunce::canonicalize(at) {
            real.extend(rest.iter().rev());
            return Some(real);
        }
        rest.push(at.file_name()?);
        at = at.parent()?;
    }
}

/// Whether a filesystem is mounted on `root` itself: an empty mount-point folder is on the
/// filesystem above it (`f_mntonname` names that one), and a drive pulled out may leave a
/// mount with no blocks.
fn mounted(root: &Path, fs: Option<Mount>) -> bool {
    fs.is_some_and(|fs| fs.on == root && fs.blocks > 0)
}

/// On the PS5 this is `statfs@FBSD_1.0`, forwarded by ps5/compat.c (the FreeBSD 11 layout:
/// the CLI asserts its size).
#[cfg(target_os = "freebsd")]
#[allow(clippy::unnecessary_cast)] // f_blocks is u64 on FreeBSD 12+, i64 on 11
fn statfs(path: &Path) -> Option<Mount> {
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: a valid C string; `st` is a properly sized out-parameter.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let on: Vec<u8> = st
        .f_mntonname
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    let bsize = st.f_bsize as u64;
    Some(Mount {
        on: PathBuf::from(OsString::from_vec(on)),
        blocks: st.f_blocks as u64,
        total: (st.f_blocks as u64).saturating_mul(bsize),
        free: u64::try_from(st.f_bavail as i64)
            .unwrap_or(0)
            .saturating_mul(bsize),
    })
}

/// Only FreeBSD (and the PS5) serves drives.
#[cfg(not(target_os = "freebsd"))]
fn statfs(_: &Path) -> Option<Mount> {
    None
}

/// A root as listed: canonical (Windows: `C:\x`, as core's `list_dir` spells it), not `/`, a
/// folder. It may resolve elsewhere (`/data` can be a symlink on the PS5, `/tmp` is
/// `/private/tmp` on macOS).
fn pin(path: &Path) -> Result<PathBuf, String> {
    let canon = dunce::canonicalize(path).map_err(|e| e.to_string())?;
    if canon.parent().is_none() {
        return Err("it is the filesystem root".into());
    }
    if !canon.is_dir() {
        return Err("not a folder".into());
    }
    Ok(canon)
}

// Unix: symlinks.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn ps5_roots() {
        let tmp = std::env::temp_dir().join(format!("forge-roots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let real = tmp.join("user/data");
        std::fs::create_dir_all(&real).unwrap();
        let real = real.canonicalize().unwrap();
        let (data, slash, usb, file) = (
            tmp.join("data"),
            tmp.join("slash"),
            tmp.join("usb0"),
            tmp.join("file"),
        );
        std::os::unix::fs::symlink(&real, &data).unwrap();
        std::os::unix::fs::symlink("/", &slash).unwrap();
        std::fs::write(&file, b"").unwrap();
        let candidates = [data.clone(), slash.clone(), usb.clone(), file.clone()];

        let (roots, notes) = Roots::new(Platform::Ps5, &candidates);
        // A symlinked `/data` is kept, under its canonical path, and logged.
        assert_eq!(roots.current(), std::slice::from_ref(&real));
        assert!(notes.contains(&format!("root {} → {}", data.display(), real.display())));
        // `/` and a non-folder are dropped and logged; a missing drive is silent.
        let dropped = |p: &Path| {
            let line = format!("not serving {}:", p.display());
            notes.iter().any(|n| n.starts_with(&line))
        };
        assert!(dropped(&slash) && dropped(&file));
        assert_eq!(notes.len(), 3);
        // A candidate that appears later shows up (a `/mnt` drive must also be mounted:
        // `drive_is_listed_only_while_mounted`).
        std::fs::create_dir(&usb).unwrap();
        assert_eq!(roots.current(), [real.clone(), usb.canonicalize().unwrap()]);

        // The host keeps its rule (resolving elsewhere is fine, not logged; `/` dropped).
        let (roots, notes) = Roots::new(Platform::Host, &[data, slash]);
        assert_eq!(roots.current(), [real]);
        assert_eq!(notes.len(), 1);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The cached roots serve browsing, but a write under a drive unplugged since asks it again.
    #[test]
    fn a_write_never_reaches_a_drive_unplugged_since_the_cache() {
        let tmp = std::env::temp_dir().join(format!("forge-reach-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let usb = tmp.join("usb0");
        std::fs::create_dir_all(&usb).unwrap();
        let usb = usb.canonicalize().unwrap();
        std::fs::write(usb.join("mounted"), b"").unwrap();
        let mut roots = Roots::fake(&[(usb.clone(), true)], fake_statfs);
        roots.ttl = Duration::from_secs(600);
        let file = usb.join("f.bin");
        assert_eq!(roots.reach(&file, false), Ok(file.clone()));
        std::fs::remove_file(usb.join("mounted")).unwrap();
        assert!(roots.reach(&file, false).is_err());
        assert!(roots.reach(&usb, false).is_err());
        assert_eq!(roots.reach(&file, true), Ok(file.clone()));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The PS5's reach: `/data` (a link here) and a drive while mounted; nothing else, also not
    /// through a link inside a root. A computer reaches everything.
    #[test]
    fn ps5_reaches_only_data_and_mounted_drives() {
        let tmp = std::env::temp_dir().join(format!("forge-reach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("user/data/game")).unwrap();
        std::fs::create_dir_all(tmp.join("mnt/usb0")).unwrap();
        std::fs::create_dir_all(tmp.join("mnt/usb1")).unwrap();
        std::fs::create_dir_all(tmp.join("system")).unwrap();
        let tmp = tmp.canonicalize().unwrap();
        let (data, real) = (tmp.join("data"), tmp.join("user/data"));
        std::os::unix::fs::symlink(&real, &data).unwrap();
        std::os::unix::fs::symlink(tmp.join("system"), real.join("out")).unwrap();
        let (usb0, usb1) = (tmp.join("mnt/usb0"), tmp.join("mnt/usb1"));
        let roots = Roots::fake(
            &[(data.clone(), false), (usb0.clone(), true), (usb1, true)],
            fake_statfs,
        );
        let ok = |p: &Path| roots.confine(p).is_ok();
        assert!(ok(&data) && ok(&real.join("game")) && ok(&data.join("game/new/file")));
        assert!(!ok(&tmp) && !ok(&tmp.join("system")) && !ok(&tmp.join("mnt")));
        // A link inside a root that leads out, a `..` past what exists, a relative path.
        assert!(!ok(&real.join("out")) && !ok(&data.join("out/x")));
        assert!(!ok(&data.join("new/../../../system")) && !ok(Path::new("data")));
        assert!(!ok(&usb0) && ok(Path::new("flink://0123/anything")));
        let err = roots.confine(&tmp.join("system")).unwrap_err();
        assert!(err.ends_with("is not on this console's storage or a plugged-in drive"));
        std::fs::write(usb0.join("mounted"), b"").unwrap();
        assert!(ok(&usb0.join("games")));

        // A paired device: `/data` as named reads as its folder; above the roots only browsed,
        // listing just the way to them (`mnt` only while a drive is mounted).
        assert_eq!(roots.reach(&data, false).unwrap(), real);
        assert!(roots.reach(&tmp, true).is_ok() && roots.reach(&tmp, false).is_err());
        assert!(roots.reach(&tmp.join("system"), true).is_err());
        assert!(roots.reach(&tmp.join("mnt/usb1"), true).is_err());
        let names = |d: &Path| roots.above(d).map(|v| v.into_iter().collect::<Vec<_>>());
        assert_eq!(names(&tmp), Some(vec!["data".into(), "mnt".into()]));
        assert_eq!(names(&tmp.join("mnt")), Some(vec!["usb0".into()]));
        assert_eq!(names(&real), None);
        std::fs::remove_file(usb0.join("mounted")).unwrap();
        assert_eq!(names(&tmp), Some(vec!["data".into()]));
        assert!(roots.reach(&tmp.join("mnt"), true).is_err());

        let (host, _) = Roots::new(Platform::Host, std::slice::from_ref(&real));
        assert!(host.confine(&tmp.join("system")).is_ok() && host.above(&tmp).is_none());
        assert_eq!(host.reach(&tmp, false).unwrap(), tmp);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
