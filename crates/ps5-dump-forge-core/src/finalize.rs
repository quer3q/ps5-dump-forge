//! The `.part` transaction: write to a unique sibling, verify, fsync, then
//! publish it with a rename that refuses to replace anything. `std::fs::rename` replaces
//! silently, so a check-then-rename would race with whatever appears in between; it is the
//! fallback only where no exclusive rename exists (macOS exFAT, see [`rename_no_replace`]).

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::JobId;

/// Windows `CreateFile` flag: open a reparse point itself, never what it points at.
#[cfg(windows)]
pub(crate) const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// `<output>.<job>-<pid>.part`, next to the output. The pid keeps two processes (the app
/// and a CLI run) from picking the same name; creation is exclusive either way.
pub(crate) fn part_path(output: &Path, job: JobId) -> PathBuf {
    let mut name = output.file_name().map(OsString::from).unwrap_or_default();
    name.push(format!(".{job}-{}.part", std::process::id()));
    output.with_file_name(name)
}

/// A `.part` this job created. Dropping it without [`Part::publish`] deletes it, so a
/// failure, a cancel or a panic (the guard drops while unwinding) never leaves it behind.
/// Before publishing and before deleting, the path is checked to still name what this job
/// created, so a `.part` swapped for something else is neither published nor deleted.
///
/// A file part keeps a duplicate of its creation handle and compares the handle's current
/// identity with the path's: macOS's exFAT and FAT drivers give a new, empty file a
/// placeholder inode number and a real one only once it has a cluster, so an identity
/// captured at creation goes stale with the first write. A directory gets its real inode
/// at `mkdir`, so a dir part keeps the identity it captured then.
pub(crate) struct Part {
    path: PathBuf,
    dir: bool,
    id: Option<FileId>,
    /// Unix only, and only ever used for `fstat`: it shares the caller's cursor.
    handle: Option<File>,
    published: bool,
}

impl Part {
    pub(crate) fn create_file(path: &Path) -> anyhow::Result<(Self, File)> {
        let file = create_new(path).with_context(|| format!("creating {}", path.display()))?;
        // The guard exists before the fallible clone, so a failed clone still cleans up.
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut part = Self::new(path, false, file_id(&file.metadata()?));
        #[cfg(unix)]
        {
            part.handle = Some(file.try_clone().context("duplicating the .part handle")?);
        }
        Ok((part, file))
    }

    pub(crate) fn create_dir(path: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir(path).with_context(|| format!("creating {}", path.display()))?;
        // ponytail: mkdir and this lstat are two steps; a swap in between (by a process
        // that can write the output folder) would be adopted. Every later step checks it.
        let meta = path.symlink_metadata()?;
        let part = Self::new(path, true, file_id(&meta));
        if !meta.is_dir() {
            anyhow::bail!("{} was replaced right after it was created", path.display());
        }
        Ok(part)
    }

    fn new(path: &Path, dir: bool, id: Option<FileId>) -> Self {
        Self {
            path: path.to_path_buf(),
            dir,
            id,
            handle: None,
            published: false,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `meta` (of an open handle) is this part. Always true where identities are
    /// not available.
    pub(crate) fn is(&self, meta: &std::fs::Metadata) -> bool {
        match &self.handle {
            Some(handle) => handle
                .metadata()
                .is_ok_and(|held| file_id(&held) == file_id(meta)),
            None => self.id.is_none() || file_id(meta) == self.id,
        }
    }

    /// Whether the path still names the object this job created. Any metadata error means
    /// it does not.
    fn still_ours(&self) -> bool {
        match &self.handle {
            Some(handle) => match (handle.metadata(), self.path.symlink_metadata()) {
                (Ok(held), Ok(at)) => at.is_file() && file_id(&held) == file_id(&at),
                _ => false,
            },
            None => {
                self.id.is_none()
                    || self
                        .path
                        .symlink_metadata()
                        .is_ok_and(|m| file_id(&m) == self.id)
            }
        }
    }

    /// Renames the (already verified and fsynced) part to `output`, then fsyncs the folder
    /// so the new name survives a crash.
    pub(crate) fn publish(mut self, output: &Path) -> anyhow::Result<()> {
        // ponytail: identity is checked, then the path is renamed; a swap inside that
        // window needs renameat on held handles, which no OS offers for the source.
        if !self.still_ours() {
            anyhow::bail!(
                "{} was replaced after it was verified; not publishing it",
                self.path.display()
            );
        }
        rename_no_replace(&self.path, output)
            .with_context(|| format!("renaming {} to {}", self.path.display(), output.display()))?;
        self.published = true;
        if let Some(parent) = output.parent() {
            fsync_dir(parent).with_context(|| format!("syncing {}", parent.display()))?;
        }
        Ok(())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        // ponytail: as in `publish`, the check and the unlink are two steps.
        if self.published || !self.still_ours() {
            return;
        }
        // `remove_dir_all` does not follow links, so a link planted inside cannot widen it.
        let _ = if self.dir {
            std::fs::remove_dir_all(&self.path)
        } else {
            std::fs::remove_file(&self.path)
        };
    }
}

/// (device, inode): what a path names, independent of its spelling.
type FileId = (u64, u64);

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

// ponytail: Windows file ids need GetFileInformationByHandle (volume serial + file index,
// compared fresh as on Unix since FAT ids change there too); parts are trusted by path there.
#[cfg(not(unix))]
fn file_id(_: &std::fs::Metadata) -> Option<FileId> {
    None
}

/// Creates a file that must not exist yet, without following a link at `path`.
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    opts.open(path)
}

#[cfg(unix)]
pub(crate) fn fsync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

// ponytail: Windows has no directory fsync through std (it needs FILE_FLAG_BACKUP_SEMANTICS);
// NTFS journals the rename itself.
#[cfg(not(unix))]
pub(crate) fn fsync_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

/// Renames `from` to `to`, failing with `AlreadyExists` if `to` exists. Works for files
/// and directories on the same volume. Atomic everywhere except on volumes without an
/// exclusive rename (macOS exFAT), where it checks `to` and then renames.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename_no_replace(from, to)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let (c_from, c_to) = (c_path(from)?, c_path(to)?);
        // SAFETY: both are valid NUL-terminated paths.
        if unsafe { libc::renamex_np(c_from.as_ptr(), c_to.as_ptr(), libc::RENAME_EXCL) } == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ENOTSUP) {
            return checked_rename(from, to);
        }
        Err(err)
    }

    /// macOS's exFAT driver has no `RENAME_EXCL`. The kernel's own lookup still fails an
    /// existing `to` (in any case) with `EEXIST` before asking the driver, so this check only
    /// catches what appeared since: nothing at `to`, in any case, since the lookup follows the
    /// volume's case rules, then rename. Nothing is created at `to` first, so a crash leaves
    /// only the `.part`.
    // ponytail: an entry created at `to` by another process between the check and the rename
    // is replaced (a window of microseconds, only where RENAME_EXCL is missing). macOS offers
    // no way to close it on exFAT today: link(), RENAME_EXCL and clonefile are all unsupported.
    pub(super) fn checked_rename(from: &Path, to: &Path) -> io::Result<()> {
        match to.symlink_metadata() {
            Ok(_) => return Err(io::Error::from_raw_os_error(libc::EEXIST)),
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            Err(_) => {}
        }
        std::fs::rename(from, to)
    }

    fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path holds a NUL byte"))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;

    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (c_path(from)?, c_path(to)?);
        // The raw syscall, not glibc's wrapper: it also links against musl and old glibc.
        // SAFETY: both are valid NUL-terminated paths; the flags are a plain integer.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path holds a NUL byte"))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain([0]).collect() };
        let (from, to) = (wide(from), wide(to));
        // No MOVEFILE_REPLACE_EXISTING: the move fails if `to` exists.
        // SAFETY: both are valid NUL-terminated wide strings.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

// ponytail: other Unixes (BSDs) have no portable no-replace rename; refuse rather than race.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod imp {
    use super::*;

    pub(super) fn rename_no_replace(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no atomic no-replace rename on this OS",
        ))
    }
}

/// `*.part` entries in `dir`, sorted. Listing only: the user decides what to delete.
pub(crate) fn stale_parts(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut parts: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("part"))
        })
        .collect();
    parts.sort();
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_is_a_sibling() {
        let part = part_path(Path::new("/x/GAME.exfat"), 7);
        assert_eq!(part.parent(), Some(Path::new("/x")));
        let name = part.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("GAME.exfat.7-") && name.ends_with(".part"),
            "{name}"
        );
    }

    use std::io::Write;

    /// A file part with `bytes` written, fsynced and the caller's handle closed, the way
    /// the image jobs leave it before publishing.
    fn written(dir: &Path, name: &str, bytes: &[u8]) -> Part {
        let (part, mut file) = Part::create_file(&dir.join(name)).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        part
    }

    /// The identity captured at creation, gone stale as on macOS exFAT/FAT volumes.
    #[cfg(unix)]
    fn stale(part: &mut Part) {
        part.id = Some((u64::MAX, u64::MAX - 6));
    }

    #[test]
    fn publish_after_write_fsync_and_close() {
        let dir = crate::test_dir("part-publish");
        let part = written(&dir, "a.part", b"image");
        part.publish(&dir.join("a.exfat")).unwrap();
        assert_eq!(std::fs::read(dir.join("a.exfat")).unwrap(), b"image");
        assert!(!dir.join("a.part").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_creation_id_still_publishes_and_cleans_up() {
        let dir = crate::test_dir("part-stale");
        let mut part = written(&dir, "a.part", b"image");
        stale(&mut part);
        let meta = std::fs::metadata(dir.join("a.part")).unwrap();
        assert!(part.is(&meta), "an open handle on the part is recognised");
        part.publish(&dir.join("a.exfat")).unwrap();
        assert_eq!(std::fs::read(dir.join("a.exfat")).unwrap(), b"image");

        let mut part = written(&dir, "b.part", b"image");
        stale(&mut part);
        drop(part);
        assert!(!dir.join("b.part").exists());
    }

    #[test]
    fn drop_after_a_partial_write_removes_the_part() {
        let dir = crate::test_dir("part-partial");
        let (part, mut file) = Part::create_file(&dir.join("a.part")).unwrap();
        file.write_all(b"half").unwrap();
        drop(part);
        drop(file);
        assert!(!dir.join("a.part").exists());
    }

    #[test]
    fn a_panic_after_writing_removes_the_part() {
        let dir = crate::test_dir("part-panic");
        let path = dir.join("a.part");
        let result = std::panic::catch_unwind(|| {
            let (_part, mut file) = Part::create_file(&path).unwrap();
            file.write_all(b"half").unwrap();
            file.sync_all().unwrap();
            panic!("writer bug");
        });
        assert!(result.is_err());
        assert!(!path.exists());
    }

    /// Moves the part aside and plants `plant` at its path. Neither publish nor drop may
    /// touch the planted entry, the moved original or the output name.
    #[cfg(unix)]
    fn replaced_part_is_left_alone(name: &str, plant: impl Fn(&Path, &Path)) {
        let dir = crate::test_dir(name);
        for publish in [true, false] {
            let path = dir.join(format!("{publish}.part"));
            let aside = dir.join(format!("{publish}.aside"));
            let out = dir.join(format!("{publish}.exfat"));
            let mut part = written(&dir, &path.file_name().unwrap().to_string_lossy(), b"ours");
            stale(&mut part);
            std::fs::rename(&path, &aside).unwrap();
            plant(&path, &aside);
            if publish {
                let err = part.publish(&out).unwrap_err().to_string();
                assert!(err.contains("was replaced after it was verified"), "{err}");
            } else {
                drop(part);
            }
            assert!(path.symlink_metadata().is_ok(), "the planted entry is kept");
            assert_eq!(std::fs::read(&aside).unwrap(), b"ours");
            assert!(out.symlink_metadata().is_err(), "nothing is published");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_file_planted_at_the_part_is_left_alone() {
        replaced_part_is_left_alone("part-plant-file", |at, _| {
            std::fs::write(at, b"theirs").unwrap()
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_dir_planted_at_the_part_is_left_alone() {
        replaced_part_is_left_alone("part-plant-dir", |at, _| {
            std::fs::create_dir(at).unwrap();
            std::fs::write(at.join("keep"), b"theirs").unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_part_planted_at_its_path_is_left_alone() {
        // The link resolves to the very file this job wrote; it is still not the part.
        replaced_part_is_left_alone("part-plant-link", |at, original| {
            std::os::unix::fs::symlink(original, at).unwrap()
        });
    }

    /// The exFAT fallback's check, run here directly: on a volume with `RENAME_EXCL` the
    /// kernel answers first, and on exFAT only a race reaches it.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_checked_rename_never_replaces() {
        let dir = crate::test_dir("checked-rename");
        let from = dir.join("a.part");
        std::fs::write(&from, b"ours").unwrap();
        std::fs::write(dir.join("file"), b"theirs").unwrap();
        std::fs::create_dir(dir.join("dir")).unwrap();
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("dangling")).unwrap();
        // A case-insensitive volume (the default on macOS) also catches the other case.
        let mut taken = vec!["file", "dir", "dangling"];
        if dir.join("FILE").exists() {
            taken.push("FILE");
        }
        for to in taken {
            let err = imp::checked_rename(&from, &dir.join(to)).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{to}");
            assert_eq!(
                err.raw_os_error(),
                Some(libc::EEXIST),
                "as RENAME_EXCL says it"
            );
        }
        assert_eq!(std::fs::read(&from).unwrap(), b"ours");
        assert_eq!(std::fs::read(dir.join("file")).unwrap(), b"theirs");
        assert!(dir.join("dir").is_dir());
        assert!(
            dir.join("dangling")
                .symlink_metadata()
                .unwrap()
                .is_symlink()
        );

        imp::checked_rename(&from, &dir.join("free")).unwrap();
        assert_eq!(std::fs::read(dir.join("free")).unwrap(), b"ours");
        let err = imp::checked_rename(&dir.join("free"), &dir.join("no/such/dir")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn an_output_that_appears_before_publish_is_kept() {
        let dir = crate::test_dir("part-output-appears");
        let out = dir.join("a.exfat");
        let part = written(&dir, "a.part", b"ours");
        std::fs::write(&out, b"theirs").unwrap();
        let err = part.publish(&out).unwrap_err();
        assert!(format!("{err:#}").contains("renaming"), "{err:#}");
        assert_eq!(std::fs::read(&out).unwrap(), b"theirs");
        assert!(
            !dir.join("a.part").exists(),
            "the failed publish drops its part"
        );
    }
}
