//! The Edit tab's preview of a host file or folder the user adds: what `Edited` would add.
//! A bounded walk (no links followed, junk filtered as a conversion filters it) stops at the
//! limits below; a folder within them is then listed by core's own folder scanner, so the
//! preview holds exactly what `Edited` scans, refusals included. Reads metadata only.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use ps5upload_fpkg::source::SourceTree;
use serde::Serialize;

use crate::preflight::listing;
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
        truncated: true,
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

    #[test]
    fn host_tree_stops_the_walk_at_the_entry_cap() {
        let dir = temp("cap");
        for i in 0..50 {
            fs::write(dir.join(format!("f{i:02}")), b"ab").unwrap();
        }
        let no = AtomicBool::new(false);
        let never = Instant::now() + Duration::from_secs(60);
        // The walk itself stops at the cap: 10 of the 50 entries enumerated.
        let mut walk = Walk::new(&no, never, 10);
        assert!(walk.dir(&dir, "").unwrap());
        assert_eq!(
            (walk.visited, walk.files.len(), walk.truncated),
            (10, 10, true)
        );
        let tree = host_tree_with(&dir, &no, 10, Duration::from_secs(60)).unwrap();
        assert_eq!(
            (tree.files.len(), tree.size, tree.truncated),
            (10, 20, true)
        );
        let tree = host_tree_with(&dir, &no, 50, Duration::from_secs(60)).unwrap();
        assert_eq!(
            (tree.files.len(), tree.size, tree.truncated),
            (50, 100, false)
        );
        // Junk counts toward the cap but is never listed.
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        let tree = host_tree_with(&dir, &no, 51, Duration::from_secs(60)).unwrap();
        assert_eq!((tree.files.len(), tree.truncated), (50, false));
        // Out of time before the first entry: truncated, nothing listed.
        let tree = host_tree_with(&dir, &no, 100, Duration::ZERO).unwrap();
        assert_eq!((tree.files.len(), tree.truncated), (0, true));
        // Cancelled by the caller: an error, not a truncated listing.
        let cancelled = AtomicBool::new(true);
        assert!(host_tree_with(&dir, &cancelled, 100, Duration::from_secs(60)).is_err());
        // A truncated walk still refuses what it saw: any 2 of these 3 hold a link.
        #[cfg(unix)]
        {
            let refused = temp("cap-refused");
            fs::write(refused.join("f"), b"x").unwrap();
            std::os::unix::fs::symlink("f", refused.join("l1")).unwrap();
            std::os::unix::fs::symlink("f", refused.join("l2")).unwrap();
            let err = host_tree_with(&refused, &no, 2, Duration::from_secs(60)).unwrap_err();
            assert!(err.to_string().contains("symlink, not followed"), "{err}");
            fs::remove_dir_all(&refused).unwrap();
        }
        fs::remove_dir_all(&dir).unwrap();
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
}
