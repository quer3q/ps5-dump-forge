//! The Edit tab's preview of a host file or folder the user adds: what `Edited` would add.
//! A bounded walk (no links followed, junk filtered as a conversion filters it) stops at the
//! limits below; a folder within them is then listed by core's own folder scanner, so the
//! preview holds exactly what `Edited` scans, refusals included. Reads metadata only. A
//! paired device's file or folder (`flink://`) is walked the same way through its listing.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use ps5upload_fpkg::source::SourceTree;
use serde::Serialize;

use crate::preflight::listing;
use crate::remote::{FlinkPath, Remote};
use crate::scan::{ScannedFolder, is_junk};

/// Directory entries enumerated (junk and folders included); past it the walk stops and the
/// listing is truncated.
const MAX_ENTRIES: usize = 200_000;
/// The walk's and the scan's time budget; past it the listing is truncated.
const BUDGET: Duration = Duration::from_secs(15);

/// What a host path holds, as the Edit tab shows it before adding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostTree {
    pub kind: HostKind,
    /// A file's size, or the total of the listed files.
    pub size: u64,
    /// A folder's files, relative and `/`-separated, sorted; empty for a file.
    pub files: Vec<HostFile>,
    /// A folder's empty directories (one holding only junk counts), relative, sorted.
    pub empty_dirs: Vec<String>,
    /// The walk hit [`MAX_ENTRIES`] or the time budget; the listing is what it saw by then.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    File,
    Dir,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostFile {
    pub path: String,
    pub size: u64,
}

/// Lists `path`: a regular file or a folder. A link or special file at `path` is an error, as
/// is anything the scanner refuses inside a folder (links, special files, bad names).
pub fn host_tree(path: &Path, cancel: &AtomicBool) -> anyhow::Result<HostTree> {
    host_tree_with(path, cancel, MAX_ENTRIES, BUDGET)
}

fn host_tree_with(
    path: &Path,
    cancel: &AtomicBool,
    max_entries: usize,
    budget: Duration,
) -> anyhow::Result<HostTree> {
    // A paired device's only: Edit adds nothing from a NAS (SMB, NFS, FTP).
    if crate::remote::is_flink(path) {
        return peer_tree(path, cancel, max_entries, budget);
    }
    crate::net::refuse(path, "adding files")?;
    // Without trailing separators or `.` components: `lstat("link/")` describes the target.
    let path: PathBuf = path.components().collect();
    let meta = fs::symlink_metadata(&path).with_context(|| format!("{}", path.display()))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        bail!("{} is a link (not followed)", path.display());
    }
    if kind.is_file() {
        return Ok(HostTree {
            kind: HostKind::File,
            size: meta.len(),
            files: Vec::new(),
            empty_dirs: Vec::new(),
            truncated: false,
        });
    }
    if !kind.is_dir() {
        bail!("{} is not a file or a folder", path.display());
    }
    let start = Instant::now();
    let mut walk = Walk::new(cancel, start + budget, max_entries);
    walk.dir(&path, "")?;
    if !walk.truncated {
        let left = budget.saturating_sub(start.elapsed());
        if let Some(folder) = scan_within(&path, cancel, left)? {
            let files: Vec<HostFile> = folder
                .files()
                .iter()
                .map(|f| HostFile {
                    path: f.path.clone(),
                    size: f.size,
                })
                .collect();
            return Ok(HostTree {
                kind: HostKind::Dir,
                size: files.iter().map(|f| f.size).sum(),
                files,
                empty_dirs: folder.empty_dirs().to_vec(),
                truncated: false,
            });
        }
        // The scan ran out of time: the walk's listing stands.
        walk.truncated = true;
    }
    walked(&path, walk)
}

/// [`host_tree`] of a `flink://` path: a paired device's file, or its folder by the walk alone.
// ponytail: no second scan as for a local folder (the job's `RemoteTree` walks as this does,
// junk skipped unread).
fn peer_tree(
    path: &Path,
    cancel: &AtomicBool,
    max_entries: usize,
    budget: Duration,
) -> anyhow::Result<HostTree> {
    let at = FlinkPath::parse(path)?;
    let remote = crate::remote::get_for(&at)?;
    let entry = crate::remote::entry(&*remote, &at).with_context(|| path.display().to_string())?;
    if entry.link {
        bail!("{} is a link (not followed)", path.display());
    }
    if entry.special {
        bail!("{} is not a file or folder", path.display());
    }
    if entry.bad_name {
        bail!("{}: name is not valid UTF-8", path.display());
    }
    if !entry.dir {
        return Ok(HostTree {
            kind: HostKind::File,
            size: entry.size,
            files: Vec::new(),
            empty_dirs: Vec::new(),
            truncated: false,
        });
    }
    let mut walk = Walk::new(cancel, Instant::now() + budget, max_entries);
    walk.peer_dir(&*remote, &at, "")?;
    walked(path, walk)
}

/// The walk's listing of the folder at `path`, unless it refused anything.
fn walked(path: &Path, mut walk: Walk) -> anyhow::Result<HostTree> {
    if !walk.refused.is_empty() {
        bail!(
            "{} holds entries that cannot be converted:\n{}",
            path.display(),
            listing("refused", &walk.refused).join("\n")
        );
    }
    walk.files.sort_by(|a, b| a.path.cmp(&b.path));
    walk.empty_dirs.sort();
    Ok(HostTree {
        kind: HostKind::Dir,
        size: walk.files.iter().map(|f| f.size).sum(),
        files: walk.files,
        empty_dirs: walk.empty_dirs,
        truncated: walk.truncated,
    })
}

/// The bounded walk: the scanner's junk filter and link rule, stopping at `max` entries or the
/// deadline. Refused entries are collected (the scanner reports them itself when it runs).
// ponytail: a truncated listing doesn't check hard links or unreadable files; the scanner
// does, for a folder within the limits and for the Edit job.
struct Walk<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
    max: usize,
    visited: usize,
    truncated: bool,
    files: Vec<HostFile>,
    empty_dirs: Vec<String>,
    refused: Vec<String>,
}

impl<'a> Walk<'a> {
    fn new(cancel: &'a AtomicBool, deadline: Instant, max: usize) -> Self {
        Self {
            cancel,
            deadline,
            max,
            visited: 0,
            truncated: false,
            files: Vec::new(),
            empty_dirs: Vec::new(),
            refused: Vec::new(),
        }
    }

    /// Walks one directory; returns whether anything under it was kept (or refused), so a
    /// directory holding only junk counts as empty. A cut-short directory counts as kept.
    fn dir(&mut self, dir: &Path, rel: &str) -> anyhow::Result<bool> {
        let shown = |name: &str| match rel {
            "" => name.to_string(),
            _ => format!("{rel}/{name}"),
        };
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if rel.is_empty() => return Err(e).context(dir.display().to_string()),
            Err(e) => {
                self.refused.push(format!("{rel}: unreadable ({e})"));
                return Ok(true);
            }
        };
        let mut kept = false;
        for entry in entries {
            if self.cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            if self.visited >= self.max || Instant::now() >= self.deadline {
                self.truncated = true;
                return Ok(true);
            }
            self.visited += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.refused
                        .push(format!("{}: unreadable ({e})", shown(".")));
                    kept = true;
                    continue;
                }
            };
            let Ok(name) = entry.file_name().into_string() else {
                let raw = entry.file_name();
                let path = shown(&raw.to_string_lossy());
                self.refused
                    .push(format!("{path} (name is not valid UTF-8)"));
                kept = true;
                continue;
            };
            if is_junk(&name) {
                continue;
            }
            kept = true;
            let path = shown(&name);
            if name.contains('\\') {
                self.refused
                    .push(format!("{path} (name contains a backslash)"));
                continue;
            }
            match entry.file_type().and_then(|t| Ok((t, entry.metadata()?))) {
                Err(e) => self.refused.push(format!("{path}: unreadable ({e})")),
                Ok((t, _)) if t.is_symlink() => {
                    self.refused.push(format!("{path} (symlink, not followed)"));
                }
                Ok((t, _)) if t.is_dir() => {
                    if !self.dir(&entry.path(), &path)? {
                        self.empty_dirs.push(path);
                    }
                }
                Ok((t, meta)) if t.is_file() => self.files.push(HostFile {
                    path,
                    size: meta.len(),
                }),
                Ok(_) => self.refused.push(format!("{path} (special file)")),
            }
        }
        Ok(kept)
    }

    /// [`Self::dir`] of a folder on a paired device, by its listing.
    fn peer_dir(&mut self, remote: &dyn Remote, at: &FlinkPath, rel: &str) -> anyhow::Result<bool> {
        let shown = |name: &str| match rel {
            "" => name.to_string(),
            _ => format!("{rel}/{name}"),
        };
        let entries = match remote.list(at) {
            Ok(entries) => entries,
            Err(e) if rel.is_empty() => return Err(e).context(at.url().display().to_string()),
            Err(e) => {
                self.refused.push(format!("{rel}: unreadable ({e})"));
                return Ok(true);
            }
        };
        let mut kept = false;
        for entry in entries {
            if self.cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            if self.visited >= self.max || Instant::now() >= self.deadline {
                self.truncated = true;
                return Ok(true);
            }
            self.visited += 1;
            if is_junk(&entry.name) {
                continue;
            }
            kept = true;
            let path = shown(&entry.name);
            if entry.bad_name {
                self.refused
                    .push(format!("{path} (name is not valid UTF-8)"));
            } else if !crate::remote::addressable(&entry.name) {
                self.refused
                    .push(format!("{path} (a name a flink:// path can't spell)"));
            } else if entry.link {
                self.refused.push(format!("{path} (symlink, not followed)"));
            } else if entry.special {
                self.refused.push(format!("{path} (special file)"));
            } else if entry.dir {
                if !self.peer_dir(remote, &at.join(&entry.name), &path)? {
                    self.empty_dirs.push(path);
                }
            } else {
                self.files.push(HostFile {
                    path,
                    size: entry.size,
                });
            }
        }
        Ok(kept)
    }
}

/// `ScannedFolder::scan` stopped by the caller's `cancel` (an error) or past `budget` (`None`).
fn scan_within(
    path: &Path,
    cancel: &AtomicBool,
    budget: Duration,
) -> anyhow::Result<Option<ScannedFolder>> {
    let stop = AtomicBool::new(cancel.load(Ordering::Relaxed) || budget.is_zero());
    let done = AtomicBool::new(false);
    let scanned = std::thread::scope(|s| {
        s.spawn(|| {
            let start = Instant::now();
            while !done.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) || start.elapsed() >= budget {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let scanned = ScannedFolder::scan(path, &stop);
        done.store(true, Ordering::Relaxed);
        scanned
    });
    match scanned {
        // Only the scanner's own "cancelled" means the budget ran out.
        Err(e)
            if stop.load(Ordering::Relaxed)
                && !cancel.load(Ordering::Relaxed)
                && e.to_string() == "cancelled" =>
        {
            Ok(None)
        }
        other => other.map(Some),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("forge-host-tree-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn folder(name: &str) -> PathBuf {
        let dir = temp(name);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::create_dir_all(dir.join("empty")).unwrap();
        fs::create_dir_all(dir.join("only-junk")).unwrap();
        fs::write(dir.join("top.bin"), b"12345").unwrap();
        fs::write(dir.join("a/b/deep.txt"), b"xyz").unwrap();
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        fs::write(dir.join("a/._deep.txt"), b"junk").unwrap();
        fs::write(dir.join("only-junk/Thumbs.db"), b"junk").unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn host_tree_refuses_links_and_special_files() {
        let dir = folder("links");
        let no = AtomicBool::new(false);
        let link = dir.with_extension("link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        let err = host_tree(&link, &no).unwrap_err().to_string();
        assert!(err.contains("link"), "{err}");
        // Trailing separators or `.` don't reach the target.
        for spelled in [
            format!("{}/", link.display()),
            format!("{}/.", link.display()),
        ] {
            let err = host_tree(Path::new(&spelled), &no).unwrap_err().to_string();
            assert!(err.contains("is a link"), "{spelled}: {err}");
        }
        let file_link = dir.join("a/file-link");
        std::os::unix::fs::symlink(dir.join("top.bin"), &file_link).unwrap();
        let err = format!("{:#}", host_tree(&file_link, &no).unwrap_err());
        assert!(err.contains("link"), "{err}");
        // A link inside the folder: the scanner's refusal, as `Edited` would see it.
        let err = format!("{:#}", host_tree(&dir, &no).unwrap_err());
        assert!(err.contains("a/file-link"), "{err}");
        fs::remove_file(&file_link).unwrap();
        let fifo = dir.join("fifo");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o644) }, 0);
        let err = host_tree(&fifo, &no).unwrap_err().to_string();
        assert!(err.contains("not a file or a folder"), "{err}");
        fs::remove_file(&link).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_peer_file_or_folder_is_listed_through_it() {
        let dir = temp("peer");
        fs::write(dir.join("top.bin"), b"top").unwrap();
        fs::create_dir_all(dir.join("a/b/void")).unwrap();
        fs::create_dir_all(dir.join("a/junk")).unwrap();
        fs::write(dir.join("a/b/c.bin"), b"see").unwrap();
        fs::write(dir.join("a/junk/.DS_Store"), b"j").unwrap();
        fs::write(dir.join("a/.DS_Store"), b"j").unwrap();
        let (root, fake) = crate::remote::tests::peer(&dir);
        let no = AtomicBool::new(false);
        let file = host_tree(&root.join("top.bin").url(), &no).unwrap();
        assert_eq!((file.kind, file.size), (HostKind::File, 3));
        let tree = host_tree(&root.join("a").url(), &no).unwrap();
        assert_eq!(tree.kind, HostKind::Dir);
        let files: Vec<&str> = tree.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["b/c.bin"]);
        assert_eq!(tree.empty_dirs, ["b/void", "junk"]);
        assert_eq!((tree.size, tree.truncated), (3, false));
        // Cut short at the limit, as a local walk is.
        let cut = host_tree_with(&root.join("a").url(), &no, 1, BUDGET).unwrap();
        assert!(cut.truncated);
        // A link is refused, at the path or inside it; a missing path is an error.
        fake.links.lock().unwrap().push("a/b/c.bin".into());
        let err = format!("{:#}", host_tree(&root.join("a").url(), &no).unwrap_err());
        assert!(err.contains("b/c.bin (symlink"), "{err}");
        let err = format!(
            "{:#}",
            host_tree(&root.join("a/b/c.bin").url(), &no).unwrap_err()
        );
        assert!(err.contains("is a link"), "{err}");
        fake.links.lock().unwrap().clear();
        // So are special files and names that aren't UTF-8, as the edit job refuses them.
        fake.specials.lock().unwrap().push("a/b/c.bin".into());
        let err = format!("{:#}", host_tree(&root.join("a").url(), &no).unwrap_err());
        assert!(err.contains("b/c.bin (special file)"), "{err}");
        let err = format!(
            "{:#}",
            host_tree(&root.join("a/b/c.bin").url(), &no).unwrap_err()
        );
        assert!(err.contains("not a file or folder"), "{err}");
        fake.specials.lock().unwrap().clear();
        fake.bad_names.lock().unwrap().push("a/b/c.bin".into());
        let err = format!("{:#}", host_tree(&root.join("a").url(), &no).unwrap_err());
        assert!(err.contains("b/c.bin (name is not valid UTF-8)"), "{err}");
        fake.bad_names.lock().unwrap().clear();
        assert!(host_tree(&root.join("gone").url(), &no).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
