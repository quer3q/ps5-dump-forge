//! Extraction into a folder. Names come from an image someone else made,
//! so every path is checked before the first byte is written, and every file is created
//! exclusively inside our own `.part` folder.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};

use anyhow::{Context, bail};
use ps5upload_fpkg::source::SourceTree;
use unicode_normalization::UnicodeNormalization;

use crate::convert::launch_order;
use crate::finalize::Part;
use crate::jobs::Ctx;
use crate::preflight::listing;

/// Bytes read and written per step; also how often cancel is checked.
pub(crate) const CHUNK: u64 = 4 * 1024 * 1024;

/// macOS (APFS/HFS+) and Windows (NTFS) folders are case-insensitive by default, and APFS
/// also ignores Unicode normalization.
// ponytail: assumed per OS, not probed. A case-insensitive Linux destination (vfat,
// casefold ext4) still cannot lose data: creation is exclusive, so a collision fails the job.
pub(crate) const CASE_INSENSITIVE: bool = cfg!(any(target_os = "macos", windows));

/// Why `name` (one path component) cannot be created safely on any desktop OS.
fn bad_component(name: &str) -> Option<&'static str> {
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    if name.is_empty() {
        return Some("empty component");
    }
    if name == "." || name == ".." {
        return Some("'.' or '..' component");
    }
    if name.contains('\0') {
        return Some("NUL in name");
    }
    if name.contains('\\') {
        return Some("backslash in name");
    }
    // A reader that decoded a name lossily put U+FFFD in it: the name on disk is another.
    if name.contains('\u{fffd}') {
        return Some("U+FFFD in name (a name the reader could not decode)");
    }
    if name.contains(':') {
        return Some("':' in name (drive or alternate data stream syntax)");
    }
    if name.chars().any(|c| c.is_control()) {
        return Some("control character in name");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Some("trailing dot or space");
    }
    if name.len() > 255 {
        return Some("component longer than 255 bytes");
    }
    // `CON`, `con.txt`, `COM1.dat`, `Lpt9 .x`: the device name wins whatever follows it.
    let base = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let upper = base.to_ascii_uppercase();
    let numbered = |prefix: &str| {
        upper
            .strip_prefix(prefix)
            .is_some_and(|n| n.len() == 1 && (b'1'..=b'9').contains(&n.as_bytes()[0]))
    };
    if RESERVED.contains(&upper.as_str()) || numbered("COM") || numbered("LPT") {
        return Some("Windows reserved device name");
    }
    None
}

/// Why `path` (`/`-separated, relative) is unsafe to create, if it is.
fn bad_path(path: &str) -> Option<&'static str> {
    if path.starts_with('/') {
        return Some("absolute path");
    }
    path.split('/').find_map(bad_component)
}

/// Every reason the tree cannot be extracted as is, one line per offending path. Empty
/// when it can. `case_insensitive` is whether the destination folds case and normalization.
pub fn extraction_findings(
    files: &[String],
    empty_dirs: &[String],
    case_insensitive: bool,
) -> Vec<String> {
    let mut unsafe_names = Vec::new();
    let mut clashes = Vec::new();
    let key = |p: &str| -> String {
        if case_insensitive {
            // ponytail: full Unicode uppercase stands in for the exFAT/NTFS upcase tables (σ
            // and ς both fold to Σ there); it over-folds a few (ß, as SS), refusing more.
            p.nfc().collect::<String>().to_uppercase()
        } else {
            p.to_string()
        }
    };
    // key -> (spelling as listed, is a directory)
    let mut seen: HashMap<String, (&str, bool)> = HashMap::new();
    let mut dirs: Vec<&str> = Vec::new();
    for path in files.iter().chain(empty_dirs) {
        if let Some(why) = bad_path(path) {
            unsafe_names.push(format!("{path} ({why})"));
            continue;
        }
        let mut at = 0;
        while let Some(i) = path[at..].find('/') {
            dirs.push(&path[..at + i]);
            at += i + 1;
        }
    }
    for path in files {
        if bad_path(path).is_some() {
            continue;
        }
        match seen.get(&key(path)) {
            Some((other, _)) if *other == path.as_str() => {
                clashes.push(format!("{path} is listed twice"));
            }
            Some((other, _)) => clashes.push(format!("{path} collides with {other}")),
            None => {
                seen.insert(key(path), (path.as_str(), false));
            }
        }
    }
    dirs.extend(
        empty_dirs
            .iter()
            .map(String::as_str)
            .filter(|d| bad_path(d).is_none()),
    );
    let mut reported = HashSet::new();
    for dir in dirs {
        match seen.get(&key(dir)) {
            Some((other, true)) if *other == dir => {}
            Some((other, is_dir)) => {
                if reported.insert((dir, *other)) {
                    if *is_dir {
                        clashes.push(format!("{dir}/ collides with {other}/"));
                    } else {
                        clashes.push(format!("{dir} is both a file and a directory ({other})"));
                    }
                }
            }
            None => {
                seen.insert(key(dir), (dir, true));
            }
        }
    }
    let mut out = listing("unsafe name", &unsafe_names);
    let kind = if case_insensitive {
        "conflict on a case-insensitive destination"
    } else {
        "conflict"
    };
    out.extend(listing(kind, &clashes));
    out
}

/// Where extracted entries are created, and which directories this job made there.
pub(crate) struct Dest {
    root: at::Dir,
    created: HashSet<String>,
    /// The folder the last file was created in, and its handle.
    last: Option<(String, Arc<at::Dir>)>,
}

impl Dest {
    pub(crate) fn open(part: &Part) -> anyhow::Result<Self> {
        let root = at::open_root(part.path())
            .with_context(|| format!("opening {}", part.path().display()))?;
        if !part.is(at::id(&root)?) {
            bail!("{} was replaced while extracting", part.path().display());
        }
        Ok(Self {
            root,
            created: HashSet::new(),
            last: None,
        })
    }

    /// The directory `rel` (`""` is the root), created with its parents as needed. A
    /// directory this job did not create already being there means two names folded
    /// together, or something planted: an error either way.
    fn dir(&mut self, rel: &str) -> anyhow::Result<at::Dir> {
        let mut cur = at::clone(&self.root)?;
        if rel.is_empty() {
            return Ok(cur);
        }
        let mut end = 0;
        for name in rel.split('/') {
            end += if end == 0 { name.len() } else { name.len() + 1 };
            let prefix = &rel[..end];
            if !self.created.contains(prefix) {
                match at::mkdir(&cur, name) {
                    Ok(()) => {
                        self.created.insert(prefix.to_string());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        bail!("{prefix}: another entry already took this name in the destination")
                    }
                    Err(e) => return Err(e).with_context(|| format!("creating {prefix}/")),
                }
            }
            cur = at::child(&cur, name).with_context(|| format!("opening {prefix}/"))?;
        }
        Ok(cur)
    }

    /// The folder `rel` goes into, created as needed, and its last component. Files come
    /// grouped by folder: the last one's handle is kept, not opened again component by
    /// component for every file.
    fn parent<'p>(&mut self, rel: &'p str) -> anyhow::Result<(Arc<at::Dir>, &'p str)> {
        let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
        if self.last.as_ref().is_none_or(|(at, _)| at != parent) {
            let dir = self.dir(parent)?;
            self.last = Some((parent.to_string(), Arc::new(dir)));
        }
        let (_, dir) = self.last.as_ref().expect("set above");
        Ok((Arc::clone(dir), name))
    }

    fn create(&mut self, rel: &str) -> anyhow::Result<File> {
        let (dir, name) = self.parent(rel)?;
        at::create(&dir, name).with_context(|| format!("creating {rel}"))
    }

    /// Moves the root file `staged` to `rel` (its folders made as needed), never over an entry.
    fn put_in_place(&mut self, staged: &str, rel: &str) -> anyhow::Result<()> {
        let (dir, name) = self.parent(rel)?;
        at::rename(&self.root, staged, &dir, name)
            .with_context(|| format!("putting {rel} in place"))
    }

    /// A new file at the root (the LZ4 pack files), exclusive and not through a link.
    pub(crate) fn create_root(&self, name: &str) -> std::io::Result<File> {
        at::create(&self.root, name)
    }

    /// A root file this job created, opened again for writing (an LZ4 volume's header).
    pub(crate) fn reopen_root(&self, name: &str) -> std::io::Result<File> {
        at::reopen(&self.root, name)
    }

    /// fsyncs every directory this job created, and the root. The root's sync comes last:
    /// on macOS its F_FULLFSYNC is the barrier that makes every [`sync_file`] durable.
    pub(crate) fn sync(&mut self, ctx: &Ctx) -> anyhow::Result<()> {
        let dirs: Vec<String> = self.created.iter().cloned().collect();
        for rel in dirs {
            ctx.check()?;
            at::sync(&self.dir(&rel)?, ctx.cancel).with_context(|| format!("syncing {rel}/"))?;
        }
        at::sync(&self.root, ctx.cancel).context("syncing the .part folder")
    }
}

/// Syncs a file written into the `.part` folder; it is durable once [`Dest::sync`] has run.
///
/// macOS: plain fsync(2), which per Apple's man page moves the file's data and metadata to
/// the drive but may leave them in the drive's volatile cache. F_FULLFSYNC (what `sync_all`
/// does there) is fsync(2) and then a flush of that whole cache (fcntl(2)): ~5 ms each on an
/// APFS SSD, 26 s of a 41 s extraction of 5,549 files. `Dest::sync` ends with one
/// F_FULLFSYNC on the root, after every file here, so one flush gives the same guarantee
/// before the rename. Elsewhere `sync_all`: Linux fsync and Windows FlushFileBuffers are
/// already full.
#[cfg(not(target_os = "freebsd"))]
pub(crate) fn sync_file(file: &File) -> std::io::Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::fd::AsRawFd;
        loop {
            // SAFETY: a valid descriptor.
            if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
                return Ok(());
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(err);
            }
        }
    }
    #[cfg(not(target_vendor = "apple"))]
    file.sync_all()
}

/// Every step relative to a directory handle opened with `O_NOFOLLOW`, so a directory
/// swapped for a link mid-extraction cannot redirect a write outside the `.part` folder.
#[cfg(unix)]
mod at {
    use std::ffi::CString;
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    pub(super) type Dir = OwnedFd;

    const DIR_FLAGS: libc::c_int =
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

    fn c(bytes: &[u8]) -> io::Result<CString> {
        CString::new(bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in name"))
    }

    fn fd(rc: libc::c_int) -> io::Result<OwnedFd> {
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a non-negative return from open/openat is a new descriptor we own.
        Ok(unsafe { OwnedFd::from_raw_fd(rc) })
    }

    pub(super) fn open_root(path: &Path) -> io::Result<Dir> {
        let path = c(path.as_os_str().as_bytes())?;
        // SAFETY: valid C string; the flags open nothing but a directory.
        fd(unsafe { libc::open(path.as_ptr(), DIR_FLAGS) })
    }

    pub(super) fn child(dir: &Dir, name: &str) -> io::Result<Dir> {
        let name = c(name.as_bytes())?;
        // SAFETY: `dir` is an open directory descriptor and `name` a valid C string.
        fd(unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), DIR_FLAGS) })
    }

    pub(super) fn mkdir(dir: &Dir, name: &str) -> io::Result<()> {
        let name = c(name.as_bytes())?;
        // SAFETY: as in `child`.
        if unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o777) } != 0 {
            return Err(io::Error::last_os_error());
        }
        #[cfg(target_os = "freebsd")]
        crate::finalize::open_up_at(dir.as_raw_fd(), &name)?; // CE-107750-0
        Ok(())
    }

    pub(super) fn create(dir: &Dir, name: &str) -> io::Result<File> {
        let name = c(name.as_bytes())?;
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: as in `child`; the mode is the variadic third argument of openat.
        let rc =
            unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags, 0o666 as libc::c_uint) };
        let file = File::from(fd(rc)?);
        crate::finalize::open_up(&file)?; // CE-107750-0
        Ok(file)
    }

    pub(super) fn reopen(dir: &Dir, name: &str) -> io::Result<File> {
        let name = c(name.as_bytes())?;
        let flags = libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: as in `child`.
        let rc = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        Ok(File::from(fd(rc)?))
    }

    /// Renames `from` in `from_dir` to `to` in `to_dir`, never over an entry there. The
    /// target is in this job's own folder, which nothing else writes: a check, then a rename.
    pub(super) fn rename(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()> {
        let (from, to) = (c(from.as_bytes())?, c(to.as_bytes())?);
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        let flag = libc::AT_SYMLINK_NOFOLLOW;
        // SAFETY: open directory descriptors, valid C strings; `st` is only written.
        if unsafe { libc::fstatat(to_dir.as_raw_fd(), to.as_ptr(), st.as_mut_ptr(), flag) } == 0 {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::ENOENT) {
            return Err(e);
        }
        let (fd, td) = (from_dir.as_raw_fd(), to_dir.as_raw_fd());
        // SAFETY: as above.
        if unsafe { libc::renameat(fd, from.as_ptr(), td, to.as_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn clone(dir: &Dir) -> io::Result<Dir> {
        dir.try_clone()
    }

    pub(super) fn id(dir: &Dir) -> io::Result<crate::finalize::FileId> {
        crate::finalize::handle_id(&File::from(dir.try_clone()?))
    }

    #[cfg(not(target_os = "freebsd"))]
    pub(super) fn sync(dir: &Dir, _: &AtomicBool) -> io::Result<()> {
        File::from(dir.try_clone()?).sync_all()
    }

    /// With ps5upload's retries; a filesystem without directory fsync is fine (U4).
    #[cfg(target_os = "freebsd")]
    pub(super) fn sync(dir: &Dir, cancel: &AtomicBool) -> io::Result<()> {
        crate::durable::sync_retry(&File::from(dir.try_clone()?), true, cancel).map(drop)
    }
}

// ponytail: elsewhere (Windows) entries are created by path; only the final component is
// protected (CREATE_NEW + FILE_FLAG_OPEN_REPARSE_POINT). Handle-relative creation there
// needs NtCreateFile with a root directory handle.
#[cfg(not(unix))]
mod at {
    use std::fs::File;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    pub(super) type Dir = PathBuf;

    pub(super) fn open_root(path: &Path) -> io::Result<Dir> {
        Ok(path.to_path_buf())
    }

    pub(super) fn child(dir: &Dir, name: &str) -> io::Result<Dir> {
        Ok(dir.join(name))
    }

    pub(super) fn mkdir(dir: &Dir, name: &str) -> io::Result<()> {
        std::fs::create_dir(dir.join(name))
    }

    pub(super) fn create(dir: &Dir, name: &str) -> io::Result<File> {
        crate::finalize::create_new(&dir.join(name))
    }

    pub(super) fn reopen(dir: &Dir, name: &str) -> io::Result<File> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.custom_flags(crate::finalize::FILE_FLAG_OPEN_REPARSE_POINT);
        }
        opts.open(dir.join(name))
    }

    pub(super) fn rename(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()> {
        crate::finalize::rename_no_replace(&from_dir.join(from), &to_dir.join(to))
    }

    pub(super) fn clone(dir: &Dir) -> io::Result<Dir> {
        Ok(dir.clone())
    }

    pub(super) fn id(dir: &Dir) -> io::Result<crate::finalize::FileId> {
        crate::finalize::path_id(dir).map(|(id, _)| id)
    }

    pub(super) fn sync(_: &Dir, _: &AtomicBool) -> io::Result<()> {
        Ok(())
    }
}

/// Copies every file and empty directory of `tree` into the `part` folder. Returns the
/// bytes written. Names must have passed [`extraction_findings`].
pub(crate) fn write(
    tree: &mut dyn SourceTree,
    part: &Part,
    stage: &str,
    ctx: &Ctx,
) -> anyhow::Result<u64> {
    let files = tree.files().to_vec();
    let empty_dirs = tree.empty_dirs().to_vec();
    let total = files
        .iter()
        .try_fold(0u64, |sum, f| sum.checked_add(f.size))
        .context("total size overflows")?;
    let mut dest = Dest::open(part)?;
    let (launch_dirs, dirs): (Vec<&String>, Vec<&String>) =
        empty_dirs.iter().partition(|d| launch_order(d) != 0);
    for dir in dirs {
        ctx.check()?;
        dest.dir(dir)?;
    }
    let mut staged = Vec::new();
    let failed = Mutex::new(None);
    let (send, recv) = mpsc::sync_channel::<Small>(WRITERS * 2);
    // Held by the writers only: once they all end (a panic too), a send fails, not blocks.
    let recv = Arc::new(Mutex::new(recv));
    let (failed_ref, cancel) = (&failed, ctx.cancel);
    let fed = std::thread::scope(|s| {
        for _ in 0..WRITERS {
            let recv = Arc::clone(&recv);
            s.spawn(move || {
                // The lock is held only while waiting for the next file.
                while let Ok(small) = { recv.lock().expect("not poisoned").recv() } {
                    // After a failure the rest are drained, not written.
                    if failed_ref.lock().expect("not poisoned").is_some() {
                        continue;
                    }
                    if let Err(e) = write_small(small, cancel) {
                        failed_ref.lock().expect("not poisoned").get_or_insert(e);
                    }
                }
            });
        }
        drop(recv);
        let fed = feed(
            tree,
            &files,
            &mut dest,
            &send,
            failed_ref,
            &mut staged,
            stage,
            ctx,
        );
        drop(send);
        fed
    });
    fed?;
    if let Some(e) = failed.into_inner().expect("not poisoned") {
        return Err(e);
    }
    // Every other file is written and synced and the writers joined: only now do the launch
    // files get their names, `sce_sys/` first, then `eboot.bin` (`launch_order`).
    // The sync first, so the other files' directory entries are durable before any launch
    // name appears.
    if !staged.is_empty() || !launch_dirs.is_empty() {
        dest.sync(ctx)?;
    }
    staged.sort_by_key(|(_, rel): &(String, String)| launch_order(rel));
    for (tmp, rel) in &staged {
        ctx.check()?;
        dest.put_in_place(tmp, rel)?;
    }
    for dir in launch_dirs {
        ctx.check()?;
        dest.dir(dir)?;
    }
    dest.sync(ctx)?;
    ctx.progress(stage, total, total);
    Ok(total)
}

/// Files up to this size are read whole by [`write`] and created, written, synced and closed
/// by [`WRITERS`] threads, each file still synced before the folder's final sync. For a few
/// KiB the create and the sync cost far more than the bytes (macOS APFS ~80 us and ~25 us,
/// and on the PS5 a fsync per file too), and one at a time a 100k-file game spent its write
/// stage waiting on them. At most `WRITERS * 3 + 1` such files are held at once (52 MiB).
const SMALL: u64 = CHUNK;
const WRITERS: usize = 4;

/// A small file for the writers: its folder's handle, its name there, its game path.
struct Small {
    dir: Arc<at::Dir>,
    name: String,
    path: String,
    bytes: Vec<u8>,
}

/// One writer's file: created exclusively, written, synced as [`write`] syncs any file (on
/// FreeBSD through U4's retries, a retried sync a failure, as `SyncEvery::finish` does).
fn write_small(small: Small, cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        bail!("cancelled");
    }
    let path = &small.path;
    let mut out =
        at::create(&small.dir, &small.name).with_context(|| format!("creating {path}"))?;
    out.write_all(&small.bytes)
        .with_context(|| format!("writing {path}"))?;
    #[cfg(not(target_os = "freebsd"))]
    sync_file(&out).with_context(|| format!("syncing {path}"))?;
    #[cfg(target_os = "freebsd")]
    {
        let synced = crate::durable::sync_retry(&out, false, cancel);
        if synced.with_context(|| format!("syncing {path}"))? == crate::durable::Synced::Retried {
            return Err(crate::durable::retried_sync()).with_context(|| format!("syncing {path}"));
        }
    }
    Ok(())
}

/// [`write`]'s reading side: every file in the tree's order, a small one read whole and handed
/// to the writers, a larger one streamed into its file here. A launch file (`launch_order`)
/// goes under a temporary name at the root, pushed to `staged` with its path, for [`write`]
/// to put in place last. Stops at the first error, here or in a writer (`failed`).
#[allow(clippy::too_many_arguments)]
fn feed(
    tree: &mut dyn SourceTree,
    files: &[ps5upload_fpkg::source::SourceFile],
    dest: &mut Dest,
    send: &mpsc::SyncSender<Small>,
    failed: &Mutex<Option<anyhow::Error>>,
    staged: &mut Vec<(String, String)>,
    stage: &str,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let total = files.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    // The tree's own root names, so a temporary one never takes a name it writes.
    let roots: HashSet<&str> = (files.iter().map(|f| f.path.as_str()))
        .map(|p| p.split('/').next().unwrap_or(p))
        .collect();
    let mut target = |file: &ps5upload_fpkg::source::SourceFile| {
        if launch_order(&file.path) == 0 {
            return file.path.clone();
        }
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        let tmp = std::iter::repeat_with(|| crate::temp_file::temp_name(name))
            .find(|t| !roots.contains(t.as_str()))
            .expect("an endless supply");
        staged.push((tmp.clone(), file.path.clone()));
        tmp
    };
    // The job's one cadence (U5): what one file measured carries over to the next. Each file
    // is synced when it is complete, so no dirty bytes carry across files.
    #[cfg(target_os = "freebsd")]
    let mut cadence = crate::durable::Cadence::new();
    for file in files {
        ctx.check()?;
        if failed.lock().expect("not poisoned").is_some() {
            return Ok(());
        }
        if file.size <= SMALL {
            let mut bytes = Vec::with_capacity(file.size as usize);
            copy(tree, file, &mut bytes, &mut done, total, stage, ctx)?;
            let rel = target(file);
            let (dir, name) = dest.parent(&rel)?;
            let name = name.to_string();
            let path = file.path.clone();
            if send
                .send(Small {
                    dir,
                    name,
                    path,
                    bytes,
                })
                .is_err()
            {
                bail!("the file writers stopped");
            }
            continue;
        }
        #[cfg_attr(target_os = "freebsd", allow(unused_mut))]
        let mut out = dest.create(&target(file))?;
        #[cfg(target_os = "freebsd")]
        let mut out = crate::durable::SyncEvery::new(out, &mut cadence, ctx.cancel);
        copy(tree, file, &mut out, &mut done, total, stage, ctx)?;
        #[cfg(not(target_os = "freebsd"))]
        sync_file(&out).with_context(|| format!("syncing {}", file.path))?;
        #[cfg(target_os = "freebsd")]
        {
            out.finish()
                .with_context(|| format!("syncing {}", file.path))?;
            crate::convert::log_cadence(&mut cadence, ctx);
        }
    }
    Ok(())
}

/// [`write`] into a folder on a peer: each file through a temporary name, sent, synced,
/// closed and renamed (a failure removes the one being written).
pub(crate) fn write_remote(
    tree: &mut dyn SourceTree,
    dest: &mut crate::remote::RemoteDir,
    stage: &str,
    ctx: &Ctx,
) -> anyhow::Result<u64> {
    let mut files = tree.files().to_vec();
    // Stable: every other file keeps the tree's order, `sce_sys/` comes before `eboot.bin`.
    files.sort_by_key(|f| launch_order(&f.path));
    let empty_dirs = tree.empty_dirs().to_vec();
    let total = files
        .iter()
        .try_fold(0u64, |sum, f| sum.checked_add(f.size))
        .context("total size overflows")?;
    let mut done = 0u64;
    for dir in &empty_dirs {
        ctx.check()?;
        dest.dir(dir)?;
    }
    for file in &files {
        ctx.check()?;
        dest.write_new(&file.path, &mut |out| {
            copy(tree, file, out, &mut done, total, stage, ctx)
                .map_err(|e| std::io::Error::other(format!("{e:#}")))
        })?;
    }
    ctx.progress(stage, total, total);
    Ok(total)
}

/// Streams `file` of `tree` into `out` in chunks, checking cancel between them.
fn copy(
    tree: &mut dyn SourceTree,
    file: &ps5upload_fpkg::source::SourceFile,
    out: &mut dyn Write,
    done: &mut u64,
    total: u64,
    stage: &str,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let mut offset = 0u64;
    while offset < file.size {
        ctx.check()?;
        let want = (file.size - offset).min(CHUNK) as usize;
        let buf = tree.read_range(&file.path, offset, want)?;
        if buf.is_empty() {
            bail!(
                "{}: the source ended at byte {offset} of {}",
                file.path,
                file.size
            );
        }
        if buf.len() > want {
            bail!(
                "{}: the source gave {} bytes at {offset} where {want} were asked",
                file.path,
                buf.len()
            );
        }
        out.write_all(&buf)
            .with_context(|| format!("writing {}", file.path))?;
        offset += buf.len() as u64;
        *done += buf.len() as u64;
        ctx.progress(stage, *done, total);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that returns more than it was asked for.
    struct Greedy(Vec<ps5upload_fpkg::source::SourceFile>);

    impl SourceTree for Greedy {
        fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
            &self.0
        }
        fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(vec![0; 10])
        }
        fn read_range(&mut self, _: &str, _: u64, len: usize) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(vec![0; len + 1])
        }
        fn describe(&self) -> String {
            "greedy".into()
        }
    }

    #[test]
    fn over_long_reads_are_refused() {
        let dir = crate::test_dir("greedy");
        let part = Part::create_dir(&dir.join("out.part")).unwrap();
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let ctx = Ctx::new(1, &emit, &cancel);
        let file = ps5upload_fpkg::source::SourceFile {
            path: "a.bin".into(),
            size: 10,
        };
        let err = write(&mut Greedy(vec![file]), &part, "write", &ctx).unwrap_err();
        assert!(err.to_string().contains("gave 11 bytes"), "{err}");
        drop(part);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// ShadowMount+ takes a folder with `sce_sys/` and `eboot.bin` for a game.
    /// The source is read in its own order, the launch files first here, and neither name
    /// may be in the folder while any file is still being read.
    #[test]
    fn launch_files_appear_after_every_other_file() {
        use ps5upload_fpkg::source::SourceFile;
        struct Watch(
            Vec<SourceFile>,
            Vec<String>,
            std::path::PathBuf,
            Vec<String>,
        );
        impl SourceTree for Watch {
            fn files(&self) -> &[SourceFile] {
                &self.0
            }
            fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
                unreachable!()
            }
            fn read_range(
                &mut self,
                p: &str,
                _: u64,
                len: usize,
            ) -> ps5upload_fpkg::Result<Vec<u8>> {
                for seen in ["eboot.bin", "sce_sys"] {
                    if self.2.join(seen).symlink_metadata().is_ok() {
                        self.3.push(format!("{seen} while reading {p}"));
                    }
                }
                Ok(vec![p.len() as u8; len])
            }
            fn empty_dirs(&self) -> &[String] {
                &self.1
            }
            fn describe(&self) -> String {
                "watch".into()
            }
        }
        assert_eq!(crate::convert::launch_order("a/eboot.bin"), 0);
        assert_eq!(crate::convert::launch_order("sce_sysx"), 0);
        let dir = crate::test_dir("launch-last");
        let part = Part::create_dir(&dir.join("out.part")).unwrap();
        // eboot.bin is larger than a writer's file, so it would be written here at once.
        let sizes = [
            ("eboot.bin", SMALL + 1),
            ("sce_sys/param.json", 10),
            ("a.bin", 10),
            ("b/c", SMALL + 1),
            ("sce_sys/icon0.png", 10),
            ("d", 10),
        ];
        let files = sizes.map(|(path, size)| SourceFile {
            path: path.into(),
            size,
        });
        let dirs = vec!["e".into(), "sce_sys/about".into()];
        let mut tree = Watch(files.to_vec(), dirs, part.path().to_path_buf(), Vec::new());
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let ctx = Ctx::new(1, &emit, &cancel);
        write(&mut tree, &part, "write", &ctx).unwrap();
        assert_eq!(tree.3, Vec::<String>::new());
        for (path, size) in sizes {
            let bytes = std::fs::read(part.path().join(path)).unwrap();
            assert_eq!(bytes, vec![path.len() as u8; size as usize], "{path}");
        }
        assert!(part.path().join("sce_sys/about").is_dir());
        let names = std::fs::read_dir(part.path()).unwrap();
        let names = names.map(|e| e.unwrap().file_name().into_string().unwrap());
        let mut root: Vec<String> = names.collect();
        root.sort();
        assert_eq!(root, ["a.bin", "b", "d", "e", "eboot.bin", "sce_sys"]);
        drop(part);
        let _ = std::fs::remove_dir_all(dir);
    }
}
