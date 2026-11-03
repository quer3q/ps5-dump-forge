//! Our own folder walker. Upstream `scan_tree` follows symlinks, converts
//! names lossily and turns a literal `\` into `/`. A game opens its files by their exact
//! spelling, so each of those is an error here instead of a silent change.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5upload_fpkg::Error;
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::preflight::listing;

/// Skipped by name at any depth. Never deleted from the source.
pub(crate) fn is_junk(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with("._")
        || matches!(
            lower.as_str(),
            ".ds_store"
                | ".fseventsd"
                | ".spotlight-v100"
                | ".trashes"
                | "thumbs.db"
                | "desktop.ini"
                | "system volume information"
        )
}

/// What a file looked like when it was scanned. Checked again on every read, so a file
/// that changes mid-conversion fails the job instead of landing half old, half new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    size: u64,
    mtime: Option<SystemTime>,
}

impl Stamp {
    pub(crate) fn new(size: u64, mtime: Option<SystemTime>) -> Self {
        Self { size, mtime }
    }

    pub(crate) fn mtime(&self) -> Option<SystemTime> {
        self.mtime
    }

    pub(crate) fn of(meta: &Metadata) -> Self {
        Self {
            size: meta.len(),
            mtime: meta.modified().ok(),
        }
    }
}

/// A game folder, walked once with every special entry refused, as a `SourceTree`.
pub struct ScannedFolder {
    root: PathBuf,
    files: Vec<SourceFile>,
    stamps: Vec<Stamp>,
    empty_dirs: Vec<String>,
    /// The last file read, kept open: writers read one file in many chunks.
    open: Option<(usize, File)>,
    /// The next files opened ahead, once reads go through the files in order.
    ahead: Option<OpenAhead>,
}

/// Files a few ahead of the reader opened on [`OPENERS`] threads. With many small files the
/// open is most of a file's cost (~60 us each on a macOS SSD, 100k files 6 s of a folder
/// copy), and a reader opening them one at a time waits on each. Only the open moves:
/// [`ScannedFolder::checked`] still checks every file against its stamp as it reads it.
struct OpenAhead {
    ask: Option<mpsc::Sender<(usize, PathBuf)>>,
    got: mpsc::Receiver<(usize, io::Result<File>)>,
    workers: Vec<std::thread::JoinHandle<()>>,
    /// Files asked for: `None` while their open runs.
    asked: HashMap<usize, Option<io::Result<File>>>,
}

const OPENERS: usize = 4;
/// How many files past the one read are opened ahead (descriptors held at most).
const OPEN_AHEAD: usize = 16;

impl OpenAhead {
    fn start() -> io::Result<Self> {
        let (ask, asks) = mpsc::channel::<(usize, PathBuf)>();
        let (give, got) = mpsc::channel();
        let asks = Arc::new(Mutex::new(asks));
        let mut workers = Vec::new();
        for _ in 0..OPENERS {
            let (asks, give) = (Arc::clone(&asks), give.clone());
            workers.push(
                std::thread::Builder::new()
                    .name("open-ahead".into())
                    .spawn(move || {
                        while let Ok((i, path)) = { asks.lock().expect("not poisoned").recv() } {
                            if give.send((i, open_nofollow(&path))).is_err() {
                                return;
                            }
                        }
                    })?,
            );
        }
        Ok(Self {
            ask: Some(ask),
            got,
            workers,
            asked: HashMap::new(),
        })
    }

    /// File `i`'s open, if it was asked for (waiting for it if it still runs). When the reads
    /// go forward (`forward`), the files after it are asked for; every other one is forgotten.
    /// A reader in its own order (the pack writers) then asks for nothing.
    fn take(
        &mut self,
        i: usize,
        forward: bool,
        root: &Path,
        files: &[SourceFile],
    ) -> Option<io::Result<File>> {
        while let Ok((at, file)) = self.got.try_recv() {
            self.settle(at, file);
        }
        while matches!(self.asked.get(&i), Some(None)) {
            let Ok((at, file)) = self.got.recv() else {
                break;
            };
            self.settle(at, file);
        }
        let mine = self.asked.remove(&i).flatten();
        let next = if forward {
            i + 1..(i + 1 + OPEN_AHEAD).min(files.len())
        } else {
            0..0
        };
        self.asked.retain(|at, _| next.contains(at));
        for at in next {
            if !self.asked.contains_key(&at)
                && let Some(ask) = &self.ask
            {
                let mut full = root.to_path_buf();
                full.extend(files[at].path.split('/'));
                if ask.send((at, full)).is_ok() {
                    self.asked.insert(at, None);
                }
            }
        }
        mine
    }

    /// An open that came back: kept while it is still wanted, else closed.
    fn settle(&mut self, at: usize, file: io::Result<File>) {
        if let Some(slot @ None) = self.asked.get_mut(&at) {
            *slot = Some(file);
        }
    }
}

impl Drop for OpenAhead {
    fn drop(&mut self) {
        self.ask = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct Walk {
    files: Vec<(SourceFile, Stamp)>,
    empty_dirs: Vec<String>,
    symlinks: Vec<String>,
    non_utf8: Vec<String>,
    backslash: Vec<String>,
    hard_links: Vec<String>,
    special: Vec<String>,
    unreadable: Vec<String>,
}

impl ScannedFolder {
    /// Walks `root`. Every offending entry is collected first, then all are reported at once.
    pub fn scan(root: &Path, cancel: &AtomicBool) -> anyhow::Result<Self> {
        // The root itself may be reached through a link (`/tmp` on macOS); only what is
        // inside it is held to the no-symlink rule. Plain `C:\x` on Windows, as `output_path`
        // spells the outputs it is compared with.
        let root = dunce::canonicalize(root).with_context(|| format!("{}", root.display()))?;
        if !root.is_dir() {
            bail!("{} is not a folder", root.display());
        }
        let mut walk = Walk::default();
        walk.dir(&root, "", cancel)?;
        let mut problems = Vec::new();
        problems.extend(listing("symlink (not followed)", &walk.symlinks));
        problems.extend(listing("name is not valid UTF-8", &walk.non_utf8));
        problems.extend(listing("name contains a backslash", &walk.backslash));
        problems.extend(listing("hard link (more than one name)", &walk.hard_links));
        problems.extend(listing("special file", &walk.special));
        problems.extend(listing("unreadable", &walk.unreadable));
        if !problems.is_empty() {
            bail!(
                "{} holds entries that cannot be converted:\n{}",
                root.display(),
                problems.join("\n")
            );
        }
        walk.files.sort_by(|a, b| a.0.path.cmp(&b.0.path));
        walk.empty_dirs.sort();
        let (files, stamps) = walk.files.into_iter().unzip();
        Ok(Self {
            root,
            files,
            stamps,
            empty_dirs: walk.empty_dirs,
            open: None,
            ahead: None,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Each file's size and modification time as scanned, in [`SourceTree::files`] order.
    pub(crate) fn stamps(&self) -> &[Stamp] {
        &self.stamps
    }

    fn index(&self, path: &str) -> ps5upload_fpkg::Result<usize> {
        self.files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .map_err(|_| Error::Format(format!("{path} is not in {}", self.root.display())))
    }

    /// The file at `i`, open and still exactly as scanned.
    fn checked(&mut self, i: usize) -> ps5upload_fpkg::Result<&mut File> {
        let path = &self.files[i].path;
        let with_path = |e: io::Error| Error::Io(io::Error::new(e.kind(), format!("{path}: {e}")));
        if self.open.as_ref().is_none_or(|(at, _)| *at != i) {
            // Reads moving to the next file in order start the openers; forward within the
            // window (empty files are never opened) they keep asking; a jump elsewhere finds
            // nothing opened for it and asks for nothing.
            let last = self.open.take().map(|(at, _)| at);
            let forward = last.is_some_and(|at| i > at && i <= at + OPEN_AHEAD);
            if last.is_some_and(|at| at + 1 == i) && self.ahead.is_none() {
                self.ahead = OpenAhead::start().ok();
            }
            let early = match &mut self.ahead {
                Some(ahead) => ahead.take(i, forward, &self.root, &self.files),
                None => None,
            };
            let file = match early {
                Some(file) => file,
                None => {
                    let mut full = self.root.clone();
                    full.extend(path.split('/'));
                    open_nofollow(&full)
                }
            };
            self.open = Some((i, file.map_err(with_path)?));
        }
        let file = &mut self.open.as_mut().expect("opened above").1;
        let meta = file.metadata().map_err(with_path)?;
        if !meta.is_file() || Stamp::of(&meta) != self.stamps[i] {
            return Err(Error::Format(format!(
                "{path} changed after it was scanned (size or modification time); \
                 run the conversion again once the folder is stable"
            )));
        }
        Ok(file)
    }
}

impl SourceTree for ScannedFolder {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let i = self.index(path)?;
        let size = self.files[i].size;
        let buf = self.read_range(path, 0, usize::try_from(size).unwrap_or(usize::MAX))?;
        if buf.len() as u64 != size {
            return Err(Error::Format(format!(
                "{path}: read {} of {size} bytes",
                buf.len()
            )));
        }
        Ok(buf)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let i = self.index(path)?;
        let size = self.stamps[i].size;
        let file = self.checked(i)?;
        // The allocation is bounded by the scanned size, never by the caller's `len` alone.
        let want = (len as u64).min(size.saturating_sub(offset));
        let mut buf = Vec::with_capacity(usize::try_from(want).unwrap_or(0));
        file.seek(SeekFrom::Start(offset))?;
        file.take(want).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!(
            "folder {} ({} files, {} empty dirs)",
            self.root.display(),
            self.files.len(),
            self.empty_dirs.len()
        )
    }
}

impl Walk {
    /// Walks one directory; returns whether anything under it was kept (or refused), so a
    /// directory holding only junk counts as empty.
    fn dir(&mut self, dir: &Path, rel: &str, cancel: &AtomicBool) -> anyhow::Result<bool> {
        let shown = |name: &str| {
            if rel.is_empty() {
                name.to_string()
            } else {
                format!("{rel}/{name}")
            }
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                self.unreadable.push(format!("{}: {e}", shown(".")));
                return Ok(true);
            }
        };
        let mut kept = false;
        for entry in entries {
            // Per entry, not per directory: a game folder can hold 100k files in one.
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.unreadable.push(format!("{}: {e}", shown(".")));
                    kept = true;
                    continue;
                }
            };
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(raw) => {
                    self.non_utf8.push(shown(&raw.to_string_lossy()));
                    kept = true;
                    continue;
                }
            };
            if is_junk(&name) {
                continue;
            }
            kept = true;
            let path = shown(&name);
            if name.contains('\\') {
                self.backslash.push(path);
                continue;
            }
            // `DirEntry::file_type`/`metadata` describe the entry itself, never a link target.
            let (kind, meta) = match entry.file_type().and_then(|t| Ok((t, entry.metadata()?))) {
                Ok(both) => both,
                Err(e) => {
                    self.unreadable.push(format!("{path}: {e}"));
                    continue;
                }
            };
            if kind.is_symlink() || is_reparse_point(&meta) {
                self.symlinks.push(path);
            } else if kind.is_dir() {
                if !self.dir(&entry.path(), &path, cancel)? {
                    self.empty_dirs.push(path);
                }
            } else if kind.is_file() {
                match links(&entry.path(), &meta) {
                    Err(e) => self.unreadable.push(format!("{path}: {e}")),
                    Ok(links) if links > 1 => self.hard_links.push(path),
                    Ok(_) => {
                        let stamp = Stamp::of(&meta);
                        self.files.push((
                            SourceFile {
                                path,
                                size: stamp.size,
                            },
                            stamp,
                        ));
                    }
                }
            } else {
                self.special
                    .push(format!("{path} ({})", special_kind(&kind)));
            }
        }
        Ok(kept)
    }
}

/// Opens an existing file without following a link planted in its place. `O_NONBLOCK`
/// keeps a FIFO swapped in for the file from blocking the open (and with it cancellation);
/// callers then refuse anything that is not a regular file. It changes nothing for reads
/// of a regular file.
pub(crate) fn open_nofollow(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(crate::finalize::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    opts.open(path)
}

/// How many names the regular file at `path` (listed with the lstat `meta`) has; an error
/// when it can't be read. Unix: the lstat already holds the link count, and the read
/// permission comes from its mode bits where they decide it alone (root, ours and
/// owner-readable, or group- and world-readable), else from access(2); no open (on macOS
/// an open per file was ~3.8 s of a 100k-file scan's 5.6 s, access(2) ~9 us each, the lstat
/// ~1 us). A file swapped after the listing is caught when it is read: every read opens it
/// with `O_NOFOLLOW` and checks its stamp.
// ponytail: an ACL or MAC policy denying a mode-readable file is found only when it is read.
#[cfg(unix)]
fn links(path: &Path, meta: &Metadata) -> io::Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid has no preconditions.
    let euid = unsafe { libc::geteuid() };
    // Only the one class that applies counts: the owner's when it is ours, else the group's
    // or the world's (which one needs our groups), so both of those must allow it.
    let mode = meta.mode();
    let readable = euid == 0
        || if meta.uid() == euid {
            mode & 0o400 != 0
        } else {
            mode & 0o044 == 0o044
        };
    if !readable {
        let c = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
        // SAFETY: a valid C string.
        if unsafe { libc::access(c.as_ptr(), libc::R_OK) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(meta.nlink())
}

/// Windows: opened for attributes only, since an open for reading has an on-access scanner
/// (Defender) check the file first; one at a time, ~95k assets kept the scan going for
/// minutes.
#[cfg(windows)]
fn links(path: &Path, _: &Metadata) -> io::Result<u64> {
    let f = crate::finalize::open_attributes(path, false)?;
    // The entry may have been swapped since it was listed.
    if !f.metadata()?.is_file() {
        return Err(io::Error::other("no longer a regular file"));
    }
    crate::win::link_count(&f)
}

// ponytail: other OSes have no link count wired; hard links are not detected there.
#[cfg(not(any(unix, windows)))]
fn links(path: &Path, _: &Metadata) -> io::Result<u64> {
    open_nofollow(path).map(|_| 1)
}

#[cfg(windows)]
fn is_reparse_point(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &Metadata) -> bool {
    false
}

#[cfg(unix)]
fn special_kind(kind: &std::fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    if kind.is_block_device() || kind.is_char_device() {
        "device"
    } else if kind.is_fifo() {
        "FIFO"
    } else if kind.is_socket() {
        "socket"
    } else {
        "unknown type"
    }
}

#[cfg(not(unix))]
fn special_kind(_: &std::fs::FileType) -> &'static str {
    "unknown type"
}

/// An image reader's tree without junk: every path with a junk component (by [`is_junk`],
/// the one list) is dropped, and the empty dirs are worked out again, so a directory that
/// held only junk counts as empty, as it does for a scanned folder. The vendored readers
/// filter by lists of their own; this makes every source agree with the folder scanner.
pub(crate) struct JunkFiltered {
    inner: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
}

fn has_junk(path: &str) -> bool {
    path.split('/').any(is_junk)
}

/// Every proper ancestor of `path` (`a`, `a/b` for `a/b/c`).
pub(crate) fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(|(i, _)| &path[..i])
}

impl JunkFiltered {
    pub(crate) fn new(inner: Box<dyn SourceTree>) -> Self {
        let files: Vec<SourceFile> = inner
            .files()
            .iter()
            .filter(|f| !has_junk(&f.path))
            .cloned()
            .collect();
        // Every directory the reader knows of, then the ones something is still under.
        let all = inner.files().iter().map(|f| f.path.as_str());
        let mut dirs: BTreeSet<&str> = all.flat_map(ancestors).collect();
        for dir in inner.empty_dirs() {
            dirs.extend(ancestors(dir));
            dirs.insert(dir);
        }
        dirs.retain(|d| !has_junk(d));
        let mut busy: HashSet<&str> = files.iter().flat_map(|f| ancestors(&f.path)).collect();
        busy.extend(dirs.iter().flat_map(|d| ancestors(d)));
        let empty_dirs = dirs
            .into_iter()
            .filter(|d| !busy.contains(d))
            .map(str::to_string)
            .collect();
        Self {
            inner,
            files,
            empty_dirs,
        }
    }
}

impl SourceTree for JunkFiltered {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Listed(Vec<SourceFile>, Vec<String>);

    impl SourceTree for Listed {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn empty_dirs(&self) -> &[String] {
            &self.1
        }
        fn describe(&self) -> String {
            "listed".into()
        }
    }

    #[test]
    fn junk_filtered_recomputes_empty_dirs() {
        let file = |p: &str| SourceFile {
            path: p.into(),
            size: 1,
        };
        let inner = Listed(
            vec![
                file("eboot.bin"),
                file(".Trashes/501/old.bin"),
                file("a/.DS_Store"),
                file("b/c/._x"),
                file("b/keep.bin"),
                file("d/Thumbs.db"),
                file("d/e/f.bin"),
            ],
            vec!["g/h".into(), ".fseventsd/x".into()],
        );
        let tree = JunkFiltered::new(Box::new(inner));
        let paths: Vec<&str> = tree.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["eboot.bin", "b/keep.bin", "d/e/f.bin"]);
        assert_eq!(tree.empty_dirs(), ["a", "b/c", "g/h"]);
    }
}
