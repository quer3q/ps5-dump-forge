//! The `.part` transaction: write to a unique sibling, verify, fsync, then
//! publish it with a rename that refuses to replace anything. `std::fs::rename` replaces
//! silently, so a check-then-rename would race with whatever appears in between; it is the
//! fallback only where no exclusive rename exists (macOS exFAT, see [`rename_no_replace`]).

use std::ffi::OsString;
use std::fs::{File, FileType, OpenOptions};
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
    /// Captured at creation; a file part uses it only until it holds `handle`.
    id: FileId,
    /// Only ever used for its identity: it shares the caller's cursor.
    handle: Option<File>,
    published: bool,
    /// From the destination probe: publish a file by hard link (U3).
    #[cfg(target_os = "freebsd")]
    hard_links: bool,
    /// The device the probe saw the output folder on.
    #[cfg(target_os = "freebsd")]
    dev: Option<u64>,
}

impl Part {
    pub(crate) fn create_file(path: &Path) -> anyhow::Result<(Self, File)> {
        let file = create_new(path).with_context(|| format!("creating {}", path.display()))?;
        let id = match handle_id(&file) {
            Ok(id) => id,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(e).with_context(|| format!("identifying {}", path.display()));
            }
        };
        // The guard exists before the fallible clone, so a failed clone still cleans up.
        let mut part = Self::new(path, false, id);
        part.handle = Some(file.try_clone().context("duplicating the .part handle")?);
        Ok((part, file))
    }

    pub(crate) fn create_dir(path: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir(path).with_context(|| format!("creating {}", path.display()))?;
        if let Err(e) = make_dir_open(path) {
            // Ours, just created and still empty.
            let _ = std::fs::remove_dir(path);
            return Err(e).with_context(|| format!("setting the mode of {}", path.display()));
        }
        // ponytail: mkdir and this lstat are two steps; a swap in between (by a process
        // that can write the output folder) would be adopted. Every later step checks it.
        let (id, kind) = path_id(path)?;
        let part = Self::new(path, true, id);
        if !kind.is_dir() {
            anyhow::bail!("{} was replaced right after it was created", path.display());
        }
        Ok(part)
    }

    fn new(path: &Path, dir: bool, id: FileId) -> Self {
        Self {
            path: path.to_path_buf(),
            dir,
            id,
            handle: None,
            published: false,
            #[cfg(target_os = "freebsd")]
            hard_links: false,
            #[cfg(target_os = "freebsd")]
            dev: None,
        }
    }

    /// What the job's destination probe saw, so publishing acts on it without probing again.
    /// In place: the caller's `file` must still drop before its part (U6).
    #[cfg(target_os = "freebsd")]
    pub(crate) fn set_dest(&mut self, dest: &crate::dest::Dest) {
        self.hard_links = dest.hard_links;
        self.dev = Some(dest.dev);
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// What this job created, as it is now.
    fn held(&self) -> io::Result<FileId> {
        match &self.handle {
            Some(handle) => handle_id(handle),
            None => Ok(self.id),
        }
    }

    /// Whether `id` (of an open handle) is this part.
    pub(crate) fn is(&self, id: FileId) -> bool {
        self.held().is_ok_and(|held| held == id)
    }

    /// Whether the path still names the object this job created. Any error means it does
    /// not.
    fn still_ours(&self) -> bool {
        match (self.held(), path_id(&self.path)) {
            (Ok(held), Ok((at, kind))) => held == at && (self.dir || kind.is_file()),
            _ => false,
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
        #[cfg(target_os = "freebsd")]
        return self.publish_here(output);
        #[cfg(not(target_os = "freebsd"))]
        {
            rename_no_replace(&self.path, output).with_context(|| {
                format!("renaming {} to {}", self.path.display(), output.display())
            })?;
            self.published = true;
            if let Some(parent) = output.parent() {
                fsync_dir(parent).with_context(|| format!("syncing {}", parent.display()))?;
            }
            Ok(())
        }
    }

    /// Puts the (already verified and fsynced) file part in place of `target`, the source image
    /// it replaces ([`crate::ConvertRequest::lz4_in_place`]), keeping the original recoverable
    /// until the new one is durable: the original gets a second name beside it (`*.orig.part`,
    /// a hard link, or a rename where the volume has no hard links: macOS exFAT/FAT), the folder
    /// is synced, the part renamed to `target`, the folder synced again, and only then the
    /// backup removed (folder synced once more). A failure before that last step puts the original back under `target`
    /// and leaves neither the part nor the backup; where even that fails, the error names the
    /// backup to rename back. Returns the backup's path when only its removal at the end
    /// failed. The caller has checked the source unchanged and closed its handles.
    // ponytail: the source's stamp is checked, then the original moves; a rewrite of the source
    // inside that window is lost. Without hard links `target` is briefly absent; a crash right
    // then leaves the original as `*.orig.part` (which `stale_parts` lists) for the user.
    pub(crate) fn replace(mut self, target: &Path) -> anyhow::Result<Option<PathBuf>> {
        if self.dir || !self.still_ours() {
            anyhow::bail!(
                "{} was replaced after it was verified; not putting it in place",
                self.path.display()
            );
        }
        #[cfg(target_os = "freebsd")]
        imp::same_device(&self.path, target, self.dev)
            .with_context(|| format!("replacing {}", target.display()))?;
        let parent = target.parent().unwrap_or(Path::new("."));
        // `<output>.<job>-<pid>.orig.part`: still a `.part`, so `stale_parts` lists a leftover.
        let part_name = self.path.file_name().unwrap_or_default().to_string_lossy();
        let stem = part_name.strip_suffix(".part").unwrap_or(&part_name);
        let backup = self.path.with_file_name(format!("{stem}.orig.part"));
        let (t, b) = (target.display(), backup.display());
        // Exclusive either way: the backup name is this attempt's and must not exist.
        let linked = match inject(Step::Link).and_then(|()| std::fs::hard_link(target, &backup)) {
            Ok(()) => true,
            Err(_) => {
                rename_no_replace(target, &backup)
                    .with_context(|| format!("moving {t} aside to {b}"))?;
                false
            }
        };
        // From here a failure puts the original back. The part's handle stays until its rename
        // lands, so `Drop` can still recognize (and delete) it before then.
        let restore = |e: anyhow::Error| -> anyhow::Error {
            let back = std::fs::rename(&backup, target).and_then(|()| fsync_dir(parent));
            match back {
                Ok(()) => e,
                Err(b2) => e.context(format!(
                    "putting the original back failed ({b2}): the original image is {b}; \
                     rename it to {t}"
                )),
            }
        };
        // Before the original can be replaced, its backup name is made durable: a crash after
        // the rename below then leaves the original reachable under it.
        let undo = |e: anyhow::Error| -> anyhow::Error {
            if linked {
                let _ = std::fs::remove_file(&backup);
                let _ = fsync_dir(parent);
                e
            } else {
                restore(e)
            }
        };
        let kept = inject(Step::BackupSync)
            .and_then(|()| fsync_dir(parent))
            .with_context(|| {
                format!(
                    "syncing {} after keeping the original as {b}",
                    parent.display()
                )
            });
        if let Err(e) = kept {
            return Err(undo(e));
        }
        let renamed = inject(Step::Rename)
            .and_then(|()| std::fs::rename(&self.path, target))
            .with_context(|| format!("renaming {} over {t}", self.path.display()));
        if let Err(e) = renamed {
            return Err(undo(e));
        }
        // The part's name is gone: nothing for `Drop` to delete.
        self.published = true;
        let synced = inject(Step::Sync)
            .and_then(|()| fsync_dir(parent))
            .with_context(|| format!("syncing {}", parent.display()));
        if let Err(e) = synced {
            // Renaming the backup over `target` also drops the new image.
            return Err(restore(e));
        }
        // The new image is in place and durable; a backup that can't be removed is left for the
        // caller to report (and `stale_parts` lists it).
        if std::fs::remove_file(&backup).is_err() {
            return Ok(Some(backup));
        }
        let _ = fsync_dir(parent);
        Ok(None)
    }

    /// U3: the device guard, then a hard link (a file, where the probe saw hard links) or a
    /// checked rename. Once the output name exists the job has succeeded: removing the
    /// part's own name and syncing the folder are best effort.
    #[cfg(target_os = "freebsd")]
    fn publish_here(&mut self, output: &Path) -> anyhow::Result<()> {
        let (from, to) = (self.path.display(), output.display());
        imp::same_device(&self.path, output, self.dev)
            .with_context(|| format!("publishing {from} as {to}"))?;
        // By file type, never by errno: UFS refuses to link a directory with EPERM.
        let link = !self.dir && self.hard_links;
        if link {
            imp::link_no_replace(&self.path, output)
                .with_context(|| format!("linking {from} to {to}"))?;
        } else {
            checked_rename(&self.path, output)
                .with_context(|| format!("renaming {from} to {to}"))?;
        }
        self.published = true;
        if link {
            // ponytail: a failed unlink leaves a second name of the output, which
            // `stale_parts` lists; deleting it leaves the output intact. Part has no log.
            self.handle = None;
            let _ = std::fs::remove_file(&self.path);
        }
        if let Some(parent) = output.parent() {
            let _ = fsync_dir(parent);
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
        // U6: an open handle can hold back the space a delete frees.
        #[cfg(target_os = "freebsd")]
        drop(self.handle.take());
        // `remove_dir_all` does not follow links, so a link planted inside cannot widen it.
        let _ = if self.dir {
            std::fs::remove_dir_all(&self.path)
        } else {
            std::fs::remove_file(&self.path)
        };
        // Sony's exFAT shows freed space as used until the folder is synced (U6).
        #[cfg(target_os = "freebsd")]
        if let Some(parent) = self.path.parent() {
            let _ = fsync_dir(parent);
        }
    }
}

/// The steps of [`Part::replace`] a test can make fail.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Step {
    /// The hard link to the original (as on a volume without them).
    Link,
    /// The folder sync that makes the backup name durable.
    BackupSync,
    Rename,
    Sync,
    /// `open_up` in [`create_new`].
    OpenUp,
}

#[cfg(test)]
thread_local! {
    /// The step that fails on this thread, in tests.
    pub(crate) static FAIL: std::cell::Cell<Option<Step>> = const { std::cell::Cell::new(None) };
    /// [`Step::Link`] fails too.
    pub(crate) static NO_LINKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// An injected failure of `step` (tests only; always `Ok` otherwise).
fn inject(step: Step) -> io::Result<()> {
    #[cfg(test)]
    if FAIL.with(|f| f.get()) == Some(step) || (step == Step::Link && NO_LINKS.with(|n| n.get())) {
        return Err(io::Error::other("injected failure"));
    }
    let _ = step;
    Ok(())
}

/// What a path names, independent of its spelling: (device, inode) on Unix, (volume
/// serial, file id) on Windows.
pub(crate) type FileId = (u64, u128);

#[cfg(unix)]
pub(crate) fn meta_id(meta: &std::fs::Metadata) -> FileId {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), u128::from(meta.ino()))
}

/// The identity of an open handle.
#[cfg(unix)]
pub(crate) fn handle_id(file: &File) -> io::Result<FileId> {
    Ok(meta_id(&file.metadata()?))
}

/// What `path` names now, and its type; a link at `path` is not followed.
#[cfg(unix)]
pub(crate) fn path_id(path: &Path) -> io::Result<(FileId, FileType)> {
    let meta = path.symlink_metadata()?;
    Ok((meta_id(&meta), meta.file_type()))
}

/// What `path` names, following links.
#[cfg(unix)]
pub(crate) fn followed_id(path: &Path) -> io::Result<FileId> {
    Ok(meta_id(&path.metadata()?))
}

#[cfg(windows)]
pub(crate) fn handle_id(file: &File) -> io::Result<FileId> {
    crate::win::file_id(file)
}

#[cfg(windows)]
pub(crate) fn path_id(path: &Path) -> io::Result<(FileId, FileType)> {
    let file = open_attributes(path, false)?;
    Ok((handle_id(&file)?, file.metadata()?.file_type()))
}

#[cfg(windows)]
pub(crate) fn followed_id(path: &Path) -> io::Result<FileId> {
    handle_id(&open_attributes(path, true)?)
}

/// Opens a file or folder for its attributes only. It shares everything, so it never
/// stands in the way of a rename or a delete; `follow: false` opens a link itself.
#[cfg(windows)]
fn open_attributes(path: &Path, follow: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x7;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
    if !follow {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(flags)
        .open(path)
}

// Elsewhere there are no identities, so no path is ever recognised as a part: nothing is
// published or deleted (and `rename_no_replace` refuses there anyway).
#[cfg(not(any(unix, windows)))]
fn no_ids() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "no file identities on this OS")
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn handle_id(_: &File) -> io::Result<FileId> {
    Err(no_ids())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn path_id(_: &Path) -> io::Result<(FileId, FileType)> {
    Err(no_ids())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn followed_id(_: &Path) -> io::Result<FileId> {
    Err(no_ids())
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
    let file = opts.open(path)?;
    if let Err(e) = inject(Step::OpenUp).and_then(|()| open_up(&file)) {
        // Ours, just created: nothing else knows it yet.
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    Ok(file)
}

/// CE-107750-0: the game cannot read files made with the process default mode on the PS5, so
/// everything Forge creates in a game folder is 0777 (like the UFS2 writer's inodes), set
/// explicitly because the create mode is masked by the umask. A no-op off FreeBSD.
#[cfg(target_os = "freebsd")]
pub(crate) fn open_up(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: `file` is an open descriptor.
    match unsafe { libc::fchmod(file.as_raw_fd(), 0o777) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

#[cfg(not(target_os = "freebsd"))]
pub(crate) fn open_up(_: &File) -> io::Result<()> {
    Ok(())
}

/// `open_up` for a directory just made at `dir/name` (`dir` is a descriptor, or `AT_FDCWD`).
#[cfg(target_os = "freebsd")]
pub(crate) fn open_up_at(dir: libc::c_int, name: &std::ffi::CStr) -> io::Result<()> {
    // SAFETY: valid C string; `dir` is an open directory descriptor or AT_FDCWD.
    match unsafe { libc::fchmodat(dir, name.as_ptr(), 0o777, libc::AT_SYMLINK_NOFOLLOW) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

/// `open_up` for a directory made by path.
pub(crate) fn make_dir_open(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "freebsd")]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
        open_up_at(libc::AT_FDCWD, &c)?;
    }
    let _ = path;
    Ok(())
}

#[cfg(all(unix, not(target_os = "freebsd")))]
pub(crate) fn fsync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// With ps5upload's retries; a filesystem without directory fsync is fine (U4).
#[cfg(target_os = "freebsd")]
pub(crate) fn fsync_dir(dir: &Path) -> io::Result<()> {
    use std::sync::atomic::AtomicBool;
    // The retries take under a second; publish and cleanup are past a cancel.
    static NO_CANCEL: AtomicBool = AtomicBool::new(false);
    crate::durable::sync_retry(&File::open(dir)?, true, &NO_CANCEL).map(drop)
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

/// For volumes without an exclusive rename (macOS exFAT; FreeBSD has none at all). macOS's
/// kernel lookup still fails an existing `to` (in any case) with `EEXIST` before asking the
/// driver, so there this check only catches what appeared since: nothing at `to`, in any
/// case, since the lookup follows the volume's case rules, then rename. Nothing is created at
/// `to` first, so a crash leaves only the `.part`.
// ponytail: an entry created at `to` by another process between the check and the rename
// is replaced (a window of microseconds, the one the user accepted). macOS offers no way to
// close it on exFAT today: link(), RENAME_EXCL and clonefile are all unsupported; FreeBSD has
// no RENAME_EXCL, and a file there is published by hard link where the folder has them.
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn checked_rename(from: &Path, to: &Path) -> io::Result<()> {
    match to.symlink_metadata() {
        Ok(_) => return Err(io::Error::from_raw_os_error(libc::EEXIST)),
        Err(e) if !missing(&e) => return Err(e),
        Err(_) => {}
    }
    std::fs::rename(from, to)
}

#[cfg(target_os = "macos")]
fn missing(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
}

/// Sony's 0x8002xxxx form of ENOENT has no `ErrorKind`.
#[cfg(target_os = "freebsd")]
fn missing(e: &io::Error) -> bool {
    crate::durable::errno(e) == Some(libc::ENOENT)
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

/// FreeBSD (the PS5) has no no-replace rename, and a cross-device `rename` panics the PS5
/// kernel instead of failing with `EXDEV` (ps5upload `ftp_server.c:738`).
#[cfg(target_os = "freebsd")]
mod imp {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// The device guard, then the checked rename. No probe here, so no hard links.
    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        same_device(from, to, None)?;
        checked_rename(from, to)
    }

    /// Refuses unless `from` (not followed) and the folder `to` goes in are on one device,
    /// and on `dev` when the probe saw one. Any stat error refuses too.
    pub(super) fn same_device(from: &Path, to: &Path, dev: Option<u64>) -> io::Result<()> {
        let parent = match to.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let (a, b) = (from.symlink_metadata()?.dev(), parent.metadata()?.dev());
        if a != b || dev.is_some_and(|d| d != b) {
            return Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                format!(
                    "{} (device {a}) and {} (device {b}{}) are not on one device; a rename \
                     across devices would panic the console",
                    from.display(),
                    parent.display(),
                    dev.map_or(String::new(), |d| format!(", probed as {d}"))
                ),
            ));
        }
        Ok(())
    }

    /// The link is the publication: it fails if `to` exists, and nothing replaces it.
    pub(super) fn link_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to).map_err(|e| {
            // Sony's 0x8002xxxx form of EEXIST, as the plain one.
            if crate::durable::errno(&e) == Some(libc::EEXIST) {
                io::Error::from_raw_os_error(libc::EEXIST)
            } else {
                e
            }
        })
    }
}

// ponytail: other Unixes (BSDs) have no portable no-replace rename; refuse rather than race.
#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    windows,
    target_os = "freebsd"
)))]
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
    #[cfg(target_os = "freebsd")]
    #[test]
    fn created_entries_are_0777_whatever_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: umask only changes this process's mask.
        let old = unsafe { libc::umask(0o077) };
        let dir = std::env::temp_dir().join(format!("forge-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("f");
        let d = dir.join("d");
        super::create_new(&f).unwrap();
        let part = super::Part::create_dir(&d).unwrap();
        // SAFETY: restoring the previous mask.
        unsafe { libc::umask(old) };
        for p in [&f, &d] {
            assert_eq!(
                std::fs::metadata(p).unwrap().permissions().mode() & 0o777,
                0o777
            );
        }
        drop(part);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn stale(part: &mut Part) {
        part.id = (u64::MAX, u128::MAX - 6);
    }

    #[test]
    fn publish_after_write_fsync_and_close() {
        let dir = crate::test_dir("part-publish");
        let part = written(&dir, "a.part", b"image");
        part.publish(&dir.join("a.exfat")).unwrap();
        assert_eq!(std::fs::read(dir.join("a.exfat")).unwrap(), b"image");
        assert!(!dir.join("a.part").exists());
    }

    #[test]
    fn a_stale_creation_id_still_publishes_and_cleans_up() {
        let dir = crate::test_dir("part-stale");
        let mut part = written(&dir, "a.part", b"image");
        stale(&mut part);
        let open = File::open(dir.join("a.part")).unwrap();
        assert!(
            part.is(handle_id(&open).unwrap()),
            "an open handle on the part is recognised"
        );
        drop(open);
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

    #[test]
    fn a_file_planted_at_the_part_is_left_alone() {
        replaced_part_is_left_alone("part-plant-file", |at, _| {
            std::fs::write(at, b"theirs").unwrap()
        });
    }

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
        // FAT32 (FreeBSD's msdosfs) has no symlinks.
        let probe = crate::test_dir("part-plant-link-probe");
        if !crate::supported(
            std::os::unix::fs::symlink("nowhere", probe.join("link")),
            "symlink",
        ) {
            return;
        }
        // The link resolves to the very file this job wrote; it is still not the part.
        replaced_part_is_left_alone("part-plant-link", |at, original| {
            std::os::unix::fs::symlink(original, at).unwrap()
        });
    }

    /// The exFAT fallback's check, run here directly: on a volume with `RENAME_EXCL` the
    /// kernel answers first, and on exFAT only a race reaches it. FreeBSD's only rename.
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    #[test]
    fn the_checked_rename_never_replaces() {
        let dir = crate::test_dir("checked-rename");
        let from = dir.join("a.part");
        std::fs::write(&from, b"ours").unwrap();
        std::fs::write(dir.join("file"), b"theirs").unwrap();
        std::fs::create_dir(dir.join("dir")).unwrap();
        // FAT32 (FreeBSD's msdosfs) has no symlinks; anywhere else the case is required.
        let links = crate::supported(
            std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("dangling")),
            "symlink",
        );
        // A case-insensitive volume (the default on macOS) also catches the other case.
        let mut taken = vec!["file", "dir"];
        if links {
            taken.push("dangling");
        }
        if dir.join("FILE").exists() {
            taken.push("FILE");
        }
        for to in taken {
            let err = checked_rename(&from, &dir.join(to)).unwrap_err();
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
            !links
                || dir
                    .join("dangling")
                    .symlink_metadata()
                    .unwrap()
                    .is_symlink()
        );

        checked_rename(&from, &dir.join("free")).unwrap();
        assert_eq!(std::fs::read(dir.join("free")).unwrap(), b"ours");
        let err = checked_rename(&dir.join("free"), &dir.join("no/such/dir")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    /// U3 on FreeBSD: a file is published by hard link, the link refuses a taken name, and a
    /// part on another device than the probe saw is never renamed.
    #[cfg(target_os = "freebsd")]
    #[test]
    fn freebsd_publishes_by_link_behind_the_device_guard() {
        let dir = crate::test_dir("part-link");
        let dev = path_id(&dir).unwrap().0.0;
        let linked = |name: &str, dev: u64| {
            let mut part = written(&dir, name, b"ours");
            part.hard_links = true;
            part.dev = Some(dev);
            part
        };
        // FAT32 (FreeBSD's msdosfs) has no hard links, so the probe would never ask for one.
        std::fs::write(dir.join("t"), b"").unwrap();
        let links = crate::supported(
            std::fs::hard_link(dir.join("t"), dir.join("t2")),
            "hard_link",
        );
        for name in ["t", "t2"] {
            let _ = std::fs::remove_file(dir.join(name));
        }
        if links {
            linked("a.part", dev).publish(&dir.join("a.exfat")).unwrap();
            assert_eq!(std::fs::read(dir.join("a.exfat")).unwrap(), b"ours");
            assert!(
                !dir.join("a.part").exists(),
                "the part's own name is removed"
            );

            std::fs::write(dir.join("b.exfat"), b"theirs").unwrap();
            let err = linked("b.part", dev)
                .publish(&dir.join("b.exfat"))
                .unwrap_err();
            let io = err.downcast_ref::<io::Error>().expect("an io error");
            assert_eq!(io.kind(), io::ErrorKind::AlreadyExists, "{err:#}");
            assert_eq!(std::fs::read(dir.join("b.exfat")).unwrap(), b"theirs");
            assert!(
                !dir.join("b.part").exists(),
                "the failed publish drops its part"
            );
        }

        let err = linked("c.part", dev ^ 1)
            .publish(&dir.join("c.exfat"))
            .unwrap_err();
        assert!(format!("{err:#}").contains("not on one device"), "{err:#}");
        assert!(!dir.join("c.exfat").exists());

        let mut part = Part::create_dir(&dir.join("d.part")).unwrap();
        (part.hard_links, part.dev) = (true, Some(dev)); // a directory is renamed all the same
        part.publish(&dir.join("d")).unwrap();
        assert!(dir.join("d").is_dir() && !dir.join("d.part").exists());
    }

    #[test]
    fn a_dir_part_swapped_for_another_dir_is_left_alone() {
        let dir = crate::test_dir("part-dir-swap");
        let (path, aside) = (dir.join("a.part"), dir.join("a.aside"));
        let part = Part::create_dir(&path).unwrap();
        std::fs::rename(&path, &aside).unwrap();
        std::fs::create_dir(&path).unwrap();
        let err = part.publish(&dir.join("out")).unwrap_err().to_string();
        assert!(err.contains("was replaced after it was verified"), "{err}");
        assert!(path.is_dir() && aside.is_dir());
        assert!(!dir.join("out").exists());
    }

    #[test]
    fn identities_follow_the_file_not_its_name() {
        let dir = crate::test_dir("file-ids");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        // FAT32 (FreeBSD's msdosfs) has no hard links: no second name to compare.
        let linked = crate::supported(std::fs::hard_link(&a, dir.join("a2")), "hard_link");
        let id = |p: &Path| path_id(p).unwrap().0;
        assert_eq!(id(&a), id(&dir.join(".").join("a")), "another spelling");
        if linked {
            assert_eq!(id(&a), id(&dir.join("a2")), "another name");
        }
        assert_eq!(id(&a), handle_id(&File::open(&a).unwrap()).unwrap());
        assert_eq!(id(&a), followed_id(&a).unwrap());
        assert_ne!(id(&a), id(&b), "another file");
        assert!(path_id(&dir).unwrap().1.is_dir());
        assert!(path_id(&dir.join("missing")).is_err());
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

    /// Only `keep` is left in `dir`.
    fn only(dir: &Path, keep: &[&str]) {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, keep);
    }

    /// With a hard link to the original, and without (the original renamed aside).
    #[test]
    fn replace_puts_the_part_in_place_and_drops_the_backup() {
        for no_links in [false, true] {
            let dir = crate::test_dir("part-replace");
            let target = dir.join("game.exfat");
            std::fs::write(&target, b"original").unwrap();
            let part = written(&dir, "game.exfat.7-1.part", b"patched");
            NO_LINKS.with(|n| n.set(no_links));
            assert_eq!(part.replace(&target).unwrap(), None);
            NO_LINKS.with(|n| n.set(false));
            assert_eq!(std::fs::read(&target).unwrap(), b"patched");
            only(&dir, &["game.exfat"]);
        }
    }

    /// A failed rename of the part, or a failed folder sync after it, puts the original back
    /// byte for byte and leaves neither the part nor the backup, with or without hard links.
    #[test]
    fn a_failed_replace_puts_the_original_back() {
        for no_links in [false, true] {
            for step in [Step::BackupSync, Step::Rename, Step::Sync] {
                let dir = crate::test_dir("part-replace-fail");
                let target = dir.join("game.ffpkg");
                std::fs::write(&target, b"original").unwrap();
                let part = written(&dir, "game.ffpkg.7-1.part", b"patched");
                FAIL.with(|f| f.set(Some(step)));
                NO_LINKS.with(|n| n.set(no_links));
                let err = part.replace(&target).unwrap_err();
                FAIL.with(|f| f.set(None));
                NO_LINKS.with(|n| n.set(false));
                assert!(format!("{err:#}").contains("injected failure"), "{err:#}");
                assert_eq!(std::fs::read(&target).unwrap(), b"original");
                only(&dir, &["game.ffpkg"]);
            }
        }
    }

    /// A file whose mode can't be set (fchmod on the PS5) is removed again, as is a folder.
    #[test]
    fn create_new_removes_what_it_could_not_open_up() {
        let dir = crate::test_dir("create-open-up");
        FAIL.with(|f| f.set(Some(Step::OpenUp)));
        let err = create_new(&dir.join("f")).unwrap_err();
        FAIL.with(|f| f.set(None));
        assert!(err.to_string().contains("injected failure"), "{err}");
        only(&dir, &[]);
    }
}
