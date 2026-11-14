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

/// The mount points of the disk volumes in `mountinfo` (`/proc/self/mountinfo`), sorted: a
/// filesystem on a block device (not a loop, ram or zram one), or a ZFS or network one. Pseudo
/// filesystems, overlays and FUSE mounts are not volumes. Names are kept as the kernel prints
/// them, spaces as `\040`, unescaped here.
#[cfg(target_os = "linux")]
pub(crate) fn volume_mounts_of(mountinfo: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for line in mountinfo.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let Some(dash) = f.iter().position(|&s| s == "-") else {
            continue;
        };
        let (Some(&at), Some(&fstype), Some(&source)) =
            (f.get(4), f.get(dash + 1), f.get(dash + 2))
        else {
            continue;
        };
        let block = source.starts_with("/dev/")
            && !["/dev/loop", "/dev/ram", "/dev/zram"]
                .iter()
                .any(|virt| source.starts_with(virt));
        if block || ["zfs", "nfs", "nfs4", "cifs", "smb3"].contains(&fstype) {
            found.push(unescape_mount(at));
        }
    }
    found.sort();
    found.dedup();
    found
}

/// A mountinfo path: `\040` and the other `\NNN` octal escapes turned into their bytes.
#[cfg(target_os = "linux")]
fn unescape_mount(s: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let octal = b
            .get(i + 1..i + 4)
            .filter(|_| b[i] == b'\\')
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u8::from_str_radix(d, 8).ok());
        match octal {
            Some(c) => {
                out.push(c);
                i += 4;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    PathBuf::from(std::ffi::OsString::from_vec(out))
}

/// The disk volumes mounted now (Linux), from `/proc/self/mountinfo`; none if it can't be read.
#[cfg(target_os = "linux")]
pub(crate) fn volume_mounts() -> Vec<PathBuf> {
    std::fs::read("/proc/self/mountinfo")
        .map(|b| volume_mounts_of(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
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
const LINUX_SYSTEM: [&str; 16] = [
    "proc", "sys", "dev", "run", "bin", "sbin", "lib", "lib64", "usr", "etc", "var", "boot",
    "root", "snap", "opt", "sysroot",
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
        // `/run/media` and `/var/mnt` are where the disks are: on Fedora Atomic `/media` and
        // `/mnt` are links to them.
        Os::Linux => {
            LINUX_SYSTEM.contains(&first)
                && !(first == "run" && p.get(1) == Some(&"media"))
                && !(first == "var" && p.get(1) == Some(&"mnt"))
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
            // Fedora Atomic's `/mnt`.
            "/var/mnt",
            "/var/mnt/datos",
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
        ] {
            assert!(linux(c).is_some(), "{c}");
        }
        assert!(linux("/srv/games").is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mountinfo_lists_the_disk_volumes_only() {
        let info = "\
22 28 0:21 / /proc rw,nosuid - proc proc rw
29 1 259:2 / / rw,relatime - btrfs /dev/nvme1n1p3 rw,subvol=/root
35 29 259:1 / /var/mnt/datos rw,relatime shared:3 - ext4 /dev/nvme0n1p1 rw
40 29 7:0 / /snap/core rw - squashfs /dev/loop0 ro
41 29 0:50 / /run/user/1000 rw - tmpfs tmpfs rw
42 29 8:17 / /run/media/me/My\\040Stick rw - vfat /dev/sdb1 rw
43 29 0:55 / /srv/nas rw - nfs4 nas:/export rw
44 29 0:60 / /overlay rw - overlay overlay rw
";
        let found: Vec<_> = volume_mounts_of(info)
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            found,
            ["/", "/run/media/me/My Stick", "/srv/nas", "/var/mnt/datos"]
        );
    }
}
