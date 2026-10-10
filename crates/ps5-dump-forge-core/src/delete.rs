//! Deleting a source the user picked and confirmed ([`crate::Jobs::delete_path`]): a file or
//! a folder, never a link, a special file, a filesystem root, a protected root or anything
//! holding one, and nothing an unfinished job reads or writes. Called with the job table
//! locked, so no job is admitted between the check and the removal.

use std::io;
use std::path::{Path, PathBuf};

use crate::finalize::{FileId, followed_id, fsync_dir, path_id};

/// Why a delete did not happen, or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteError {
    /// Not something Forge deletes: an empty path, a link, a special file, a root.
    Refused(String),
    /// Nothing at the path.
    Missing(String),
    /// An unfinished job reads or writes it, or something inside or around it.
    Busy(String),
    /// The removal or the folder sync failed; a folder may be partly deleted.
    Failed(String),
}

impl std::fmt::Display for DeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Self::Refused(m) | Self::Missing(m) | Self::Busy(m) | Self::Failed(m)) = self;
        f.write_str(m)
    }
}

impl std::error::Error for DeleteError {}

/// Deletes `path` unless it overlaps a path of `guarded` (unfinished jobs' sources, outputs,
/// `.part`s and inputs) or is, or holds, a filesystem root or one of `protected`.
pub(crate) fn delete<'a>(
    path: &Path,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    if path.as_os_str().is_empty() {
        return Err(DeleteError::Refused("no path to delete".into()));
    }
    // To the OS `link/` and `link/.` name the link's target, which the link check below would
    // never see; rebuilt from its components (no trailing `/` or `.`) the path names the link.
    let path = &path.components().collect::<PathBuf>();
    let shown = path.display();
    let (id, kind) = match path_id(path) {
        Ok(found) => found,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(DeleteError::Missing(format!("{shown} does not exist")));
        }
        Err(e) => return Err(DeleteError::Failed(format!("reading {shown}: {e}"))),
    };
    if kind.is_symlink() {
        return Err(DeleteError::Refused(format!(
            "{shown} is a link; pick what it points at instead"
        )));
    }
    if !kind.is_file() && !kind.is_dir() {
        return Err(DeleteError::Refused(format!(
            "{shown} is not a file or folder"
        )));
    }
    let failed = |what: &str, e: io::Error| DeleteError::Failed(format!("{what} {shown}: {e}"));
    let real = path.canonicalize().map_err(|e| failed("resolving", e))?;
    let chain = ids_up(&real).map_err(|e| failed("resolving", e))?;
    // A volume's root, or a folder another volume is mounted on.
    let root = match real.parent() {
        None => true,
        Some(parent) => followed_id(parent).map_err(|e| failed("resolving", e))?.0 != id.0,
    };
    if root {
        return Err(DeleteError::Refused(format!(
            "{shown} is the root of a drive or volume"
        )));
    }
    for keep in protected {
        if holds(id, keep) {
            return Err(DeleteError::Refused(format!(
                "{shown} is, or holds, {}, which is not deleted",
                keep.display()
            )));
        }
    }
    for used in guarded {
        let inside = followed_id(used).is_ok_and(|g| chain.contains(&g));
        if inside || holds(id, used) {
            return Err(DeleteError::Busy(format!(
                "{shown} is in use by an unfinished job ({}); wait for it or stop it first",
                used.display()
            )));
        }
    }
    // remove_dir_all would empty a volume mounted somewhere inside before failing on it.
    #[cfg(unix)]
    if kind.is_dir()
        && let Some(mount) = mount_inside(&real, id.0).map_err(|e| failed("reading", e))?
    {
        return Err(DeleteError::Refused(format!(
            "{shown} holds {}, where another volume is mounted",
            mount.display()
        )));
    }
    // The checks above looked at what `path` named then; it must still be that.
    if !path_id(&real).is_ok_and(|(now, k)| now == id && k.is_dir() == kind.is_dir()) {
        return Err(DeleteError::Failed(format!(
            "{shown} changed while it was checked; nothing deleted"
        )));
    }
    if kind.is_dir() {
        // std's remove_dir_all removes links inside without following them.
        if let Err(e) = std::fs::remove_dir_all(&real) {
            let partly = if real.exists() {
                "; some of it may already be gone"
            } else {
                ""
            };
            return Err(DeleteError::Failed(format!(
                "deleting {shown}: {e}{partly}"
            )));
        }
    } else {
        std::fs::remove_file(&real).map_err(|e| failed("deleting", e))?;
    }
    if let Some(parent) = real.parent() {
        fsync_dir(parent).map_err(|e| {
            DeleteError::Failed(format!(
                "deleted {shown}, but syncing {} failed: {e}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

/// The identities of `real` (canonical) and every folder above it.
fn ids_up(real: &Path) -> io::Result<Vec<FileId>> {
    real.ancestors().map(followed_id).collect()
}

/// The first folder under `dir` (links not followed) on another volume than `dev`.
#[cfg(unix)]
fn mount_inside(dir: &Path, dev: u64) -> io::Result<Option<PathBuf>> {
    use std::os::unix::fs::MetadataExt;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if !meta.is_dir() {
            continue;
        }
        if meta.dev() != dev {
            return Ok(Some(entry.path()));
        }
        if let Some(mount) = mount_inside(&entry.path(), dev)? {
            return Ok(Some(mount));
        }
    }
    Ok(None)
}

/// Whether `id` is `path` or a folder above it, as written (a link on the way counts, not
/// only what it points at) or resolved. A path that does not exist yet (a job's output) is
/// judged by its existing folders.
fn holds(id: FileId, path: &Path) -> bool {
    let mut found = false;
    for real in path
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .filter_map(|p| p.canonicalize().ok())
    {
        found = true;
        if ids_up(&real).is_ok_and(|ids| ids.contains(&id)) {
            return true;
        }
    }
    !found
        && std::env::current_dir().is_ok_and(|cwd| ids_up(&cwd).is_ok_and(|ids| ids.contains(&id)))
}
