//! Which folders the Library may enter: the PS5's `statfs` classifier (network, FUSE and
//! unknown filesystems are refused, failing closed), the walker's name and app-data
//! exclusions, and the refusal of system folders as a chosen Library folder.
//! Reads only.

use std::path::{Component, Path, PathBuf};

/// Directory names the walker never enters, at any depth (ASCII case-insensitive). The
/// walker also skips `scan::is_junk` names and `.part` entries.
const SKIP_DIRS: [&str; 12] = [
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "target",
    "__pycache__",
    ".cache",
    ".npm",
    ".cargo",
    ".rustup",
    ".gradle",
    "$RECYCLE.BIN",
];

/// Whether the walker skips a directory named `name` (the build, VCS and cache folders above).
pub(crate) fn skip_dir_name(name: &str) -> bool {
    SKIP_DIRS.iter().any(|skip| skip.eq_ignore_ascii_case(name))
}

/// ShadowMountPlus's mounts of images the Library already lists by their files.
const SHADOWMNT: &str = "/mnt/shadowmnt";

/// Whether `path` is `/mnt/shadowmnt` or inside it: never walked, supplied or discovered.
pub(crate) fn shadowmnt(path: &Path) -> bool {
    path.starts_with(SHADOWMNT)
}

/// Whether the walker leaves out the folder `path`: the user's per-user application data
/// (macOS `~/Library`, Windows `~/AppData`), so choosing a home does not spend the budget
/// there. Never applied to a root itself.
pub(crate) fn system_dir(path: &Path, home: Option<&Path>) -> bool {
    let name = match Os::HOST {
        Some(Os::Mac) => "Library",
        Some(Os::Windows) => "AppData",
        _ => return false,
    };
    home.is_some_and(|h| {
        path.parent() == Some(h)
            && path
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
    })
}

/// The user's home folder, when the OS names an absolute one.
pub(crate) fn home() -> Option<PathBuf> {
    std::env::home_dir().filter(|h| h.is_absolute())
}

/// Filesystems the PS5 Library may enter; anything else (network, FUSE, pseudo, unknown) is
/// refused.
#[cfg(target_os = "freebsd")]
const FREEBSD_FS: [&str; 9] = [
    "ufs", "msdosfs", "exfat", "exfatfs", "bfs", "nullfs", "cd9660", "udf", "zfs",
];

/// `Ok` when the PS5 Library may enumerate `path`: an absolute path without `..`, outside
/// `/mnt/shadowmnt`, on a local disk filesystem. `Err` says why not, for a warning. Fails
/// closed.
#[cfg(target_os = "freebsd")]
pub(crate) fn ps5_classify(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err("not an absolute path without `..`".into());
    }
    if shadowmnt(path) {
        return Err("ShadowMountPlus's mounts are not scanned".into());
    }
    os::check(path)
}

#[cfg(target_os = "freebsd")]
mod os {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::FREEBSD_FS;

    /// `statfs` of the folder itself, so a root and a mount point are classified alike. On
    /// the PS5 this is `statfs@FBSD_1.0`, forwarded by ps5/compat.c (as in `dest.rs`).
    #[allow(clippy::unnecessary_cast)] // f_flags' width differs between FreeBSD versions
    pub(super) fn check(path: &Path) -> Result<(), String> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| "NUL in path")?;
        // SAFETY: a valid C string; `st` is a properly sized out-parameter.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return Err(format!("statfs: {}", std::io::Error::last_os_error()));
        }
        let name: Vec<u8> = st
            .f_fstypename
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        let fstype = String::from_utf8_lossy(&name).into_owned();
        let local = st.f_flags as u64 & libc::MNT_LOCAL as u64 != 0;
        if local && FREEBSD_FS.contains(&fstype.as_str()) {
            Ok(())
        } else {
            Err(format!("{fstype}, not a local disk filesystem"))
        }
    }
}

/// The OS families whose system folders differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    pub(crate) const HOST: Option<Os> = if cfg!(target_os = "macos") {
        Some(Os::Mac)
    } else if cfg!(windows) {
        Some(Os::Windows)
    } else if cfg!(target_os = "linux") {
        Some(Os::Linux)
    } else {
        None
    };
}

/// System folders by OS, lower-case, matched at the top of a path (Windows: at a drive's root).
const MAC_SYSTEM: [&str; 12] = [
    "system",
    "library",
    "applications",
    "private",
    "usr",
    "bin",
    "sbin",
    "etc",
    "var",
    "dev",
    "cores",
    "opt",
];
const LINUX_SYSTEM: [&str; 18] = [
    "proc", "sys", "dev", "run", "bin", "sbin", "lib", "lib64", "usr", "etc", "var", "boot",
    "root", "snap", "opt", "sysroot", "efi", "nix",
];
const WINDOWS_SYSTEM: [&str; 7] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "$recycle.bin",
    "system volume information",
];
/// A path as (Windows drive letter or UNC `\\server\share`, normal components) for comparison;
/// lower-cased when `lower`.
fn key(path: &Path, lower: bool) -> (Option<String>, Vec<String>) {
    let mut drive = None;
    let mut parts = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => {
                use std::path::Prefix::{Disk, UNC, VerbatimDisk, VerbatimUNC};
                drive = match p.kind() {
                    Disk(l) | VerbatimDisk(l) => {
                        Some(char::from(l.to_ascii_uppercase()).to_string())
                    }
                    UNC(s, h) | VerbatimUNC(s, h) => Some(format!(
                        "\\\\{}\\{}",
                        s.to_string_lossy().to_lowercase(),
                        h.to_string_lossy().to_lowercase()
                    )),
                    _ => Some(p.as_os_str().to_string_lossy().to_lowercase()),
                };
            }
            Component::Normal(n) => {
                let n = n.to_string_lossy();
                parts.push(if lower {
                    n.to_lowercase()
                } else {
                    n.into_owned()
                });
            }
            _ => {}
        }
    }
    (drive, parts)
}

/// The Windows system drive letter (`%SystemDrive%`, default `C`).
pub(crate) fn system_drive() -> char {
    std::env::var("SystemDrive")
        .ok()
        .and_then(|d| d.chars().next())
        .unwrap_or('C')
}

fn is_drive(drive: &Option<String>, letter: char) -> bool {
    drive.as_deref() == Some(letter.to_ascii_uppercase().to_string().as_str())
}

/// Why `path` (canonical) cannot be a chosen Library folder, or `None`. Pure: `home` and the
/// Windows system drive (`sys_drive`, default `C`) are passed in. ASCII case-insensitive on
/// macOS and Windows; exact on Linux.
pub(crate) fn refusal(path: &Path, home: Option<&Path>, os: Os, sys_drive: char) -> Option<String> {
    // Linux compares exactly; the other two ignore case (the lists are lower-case).
    let fold = |p: &Path| key(p, os != Os::Linux);
    let (drive, parts) = fold(path);
    let p: Vec<&str> = parts.iter().map(String::as_str).collect();
    let system = || {
        Some(format!(
            "{} is a system folder; choose the folder that holds your games",
            path.display()
        ))
    };
    let on_system_drive = os != Os::Windows || is_drive(&drive, sys_drive);
    match (os, p.as_slice()) {
        // The filesystem root; Windows: the system drive's root only.
        (_, []) if on_system_drive => return system(),
        _ => {}
    }
    if let Some(home) = home {
        let (hd, hp) = fold(home);
        // Per-user application data is refused. Anything else at or inside the home is allowed
        // before the system list (Linux homes under `/var/home` or `/root`).
        let app_data = match os {
            Os::Mac => "library",
            Os::Windows => "appdata",
            Os::Linux => "",
        };
        if hd == drive && !hp.is_empty() && p.len() >= hp.len() && p[..hp.len()] == hp[..] {
            return (p.len() > hp.len() && p[hp.len()] == app_data)
                .then(system)
                .flatten();
        }
    }
    let first = p.first().copied().unwrap_or("");
    let in_system = match os {
        // `/tmp` resolves to `/private/tmp`: the one part of `/private` that is allowed.
        Os::Mac => MAC_SYSTEM.contains(&first) && !(first == "private" && p.get(1) == Some(&"tmp")),
        // `/run/media` holds removable drives; `/var/mnt` is Fedora Atomic's `/mnt` (issue #52).
        Os::Linux => {
            LINUX_SYSTEM.contains(&first)
                && !matches!(
                    (first, p.get(1)),
                    ("run", Some(&"media")) | ("var", Some(&"mnt"))
                )
        }
        Os::Windows => on_system_drive && WINDOWS_SYSTEM.contains(&first),
    };
    if in_system {
        return system();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(p: &str) -> Option<String> {
        refusal(Path::new(p), Some(Path::new("/Users/me")), Os::Mac, 'C')
    }
    fn linux(p: &str) -> Option<String> {
        refusal(Path::new(p), Some(Path::new("/home/me")), Os::Linux, 'C')
    }

    #[test]
    fn refuses_root_and_allows_mount_containers() {
        assert!(mac("/").is_some() && linux("/").is_some());
        for c in ["/Volumes", "/Volumes/Games"] {
            assert!(mac(c).is_none(), "{c}");
        }
        for c in [
            "/media",
            "/mnt",
            "/run/media",
            "/run/media/me",
            "/media/me/Games",
            "/run/media/me/Games",
            "/mnt/games",
        ] {
            assert!(linux(c).is_none(), "{c}");
        }
    }

    #[test]
    fn refuses_system_folders_and_inside() {
        for c in [
            "/System/Library",
            "/Applications",
            "/private/var/x",
            "/usr/local",
            "/opt",
        ] {
            assert!(mac(c).is_some(), "{c}");
        }
        for c in [
            "/proc",
            "/etc/x",
            "/var/games",
            "/boot",
            "/run/user/1",
            "/root",
            "/snap",
            "/sysroot",
            "/efi",
            "/nix/store",
        ] {
            assert!(linux(c).is_some(), "{c}");
        }
        assert!(linux("/srv/games").is_none());
        assert!(linux("/var/mnt/datos").is_none());
    }
}
