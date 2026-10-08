//! The roots: where the file browser starts (`list_dir` with a `null` path). They confine
//! nothing; every path a request names goes to core as given, as in the Tauri app.

use std::path::{Path, PathBuf};

use crate::Platform;

pub(crate) struct Roots {
    list: Vec<Candidate>,
    ps5: bool,
    /// `statfs`; tests replace it.
    statfs: fn(&Path) -> Option<Mount>,
}

struct Candidate {
    path: PathBuf,
    /// The canonical path, pinned at startup. On the PS5 a candidate that doesn't exist then
    /// is resolved each time it is used, and so is every drive.
    pinned: Option<PathBuf>,
    /// A PS5 drive (`/mnt/usbN`, `/mnt/extN`): its folder exists with nothing plugged in, so
    /// it is listed only while a filesystem is mounted on it.
    drive: bool,
}

/// What `statfs` says of a path: where its filesystem is mounted and how many blocks it has.
#[cfg_attr(not(target_os = "freebsd"), allow(dead_code))] // built by FreeBSD's `statfs` only
pub(crate) struct Mount {
    on: PathBuf,
    blocks: u64,
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
        (Self { list, ps5, statfs }, notes)
    }

    /// The roots that exist now (drives: that are mounted now), canonical, each once.
    pub fn current(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for c in &self.list {
            let root = match &c.pinned {
                Some(root) => root.is_dir().then(|| root.clone()),
                None if self.ps5 => pin(&c.path)
                    .ok()
                    .filter(|r| !c.drive || mounted(r, (self.statfs)(r))),
                None => None,
            };
            if let Some(root) = root.filter(|r| !roots.contains(r)) {
                roots.push(root);
            }
        }
        roots
    }
}

/// Whether a filesystem is mounted on `root` itself: an empty mount-point folder is on the
/// filesystem above it (`f_mntonname` names that one), and a drive pulled out may leave a
/// mount with no blocks.
fn mounted(root: &Path, fs: Option<Mount>) -> bool {
    fs.is_some_and(|fs| fs.on == root && fs.blocks > 0)
}

/// On the PS5 this is `statfs@FBSD_1.0`, forwarded by ps5/compat.c.
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
    Some(Mount {
        on: PathBuf::from(OsString::from_vec(on)),
        blocks: st.f_blocks as u64,
    })
}

/// Only FreeBSD (and the PS5) serves drives.
#[cfg(not(target_os = "freebsd"))]
fn statfs(_: &Path) -> Option<Mount> {
    None
}

/// A root as listed: canonical, not `/`, a folder. It may resolve elsewhere (`/data` can
/// be a symlink on the PS5, `/tmp` is `/private/tmp` on macOS).
fn pin(path: &Path) -> Result<PathBuf, String> {
    let canon = path.canonicalize().map_err(|e| e.to_string())?;
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

    #[test]
    fn drive_is_listed_only_while_mounted() {
        let fs = |on: &str, blocks| {
            Some(Mount {
                on: on.into(),
                blocks,
            })
        };
        let usb = Path::new("/mnt/usb0");
        assert!(mounted(usb, fs("/mnt/usb0", 1)));
        // The empty folder, on the filesystem above it.
        assert!(!mounted(usb, fs("/", 1_000)));
        assert!(!mounted(usb, fs("/mnt", 1_000)));
        // A mount left behind by a pulled drive; `statfs` failing.
        assert!(!mounted(usb, fs("/mnt/usb0", 0)));
        assert!(!mounted(usb, None));

        // In `Roots`: checked on each use, so plugging in and pulling out show.
        let tmp = std::env::temp_dir().join(format!("forge-drives-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let usb = tmp.join("usb0");
        std::fs::create_dir_all(&usb).unwrap();
        let usb = usb.canonicalize().unwrap();
        // A fake `statfs`: a `mounted` file in the folder makes it a mount point.
        let fake = |p: &Path| {
            let on = match p.join("mounted").exists() {
                true => p.to_path_buf(),
                false => p.parent()?.to_path_buf(),
            };
            Some(Mount { on, blocks: 1 })
        };
        let drive = Candidate {
            path: usb.clone(),
            pinned: None,
            drive: true,
        };
        let roots = Roots {
            list: vec![drive],
            ps5: true,
            statfs: fake,
        };
        assert!(roots.current().is_empty());
        std::fs::write(usb.join("mounted"), b"").unwrap();
        assert_eq!(roots.current(), std::slice::from_ref(&usb));
        std::fs::remove_file(usb.join("mounted")).unwrap();
        assert!(roots.current().is_empty());
        std::fs::remove_dir_all(&tmp).unwrap();

        // `/mnt/...` candidates are drives on the PS5 only, and never pinned.
        let mnt = [PathBuf::from("/mnt/usb0")];
        let (roots, _) = Roots::new(Platform::Ps5, &mnt);
        assert!(roots.list[0].drive && roots.list[0].pinned.is_none());
        let (roots, _) = Roots::new(Platform::Host, &mnt);
        assert!(roots.list.iter().all(|c| !c.drive));
    }
}
