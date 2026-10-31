//! Deleting and moving what the user picked and confirmed ([`crate::Jobs::delete_path`],
//! [`crate::Jobs::move_path`]), both refused by one `check`: a file or a folder, never a link,
//! a special file, a filesystem root, a protected root or anything holding one, an interrupted
//! edit's undo journal while its image is there, nothing an unfinished job reads or writes,
//! and on the PS5 nothing at or in ShadowMount+'s mount folder, no image it has mounted and no
//! folder holding one (fails closed). Called with the job table locked, so no job is admitted
//! between the check and the change.

use std::io;
use std::path::{Component, Path, PathBuf};

use ps5upload_fpkg::source::SourceTree;

use crate::finalize::{FileId, followed_id, fsync_dir, path_id};

/// Why a delete did not happen, or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteError {
    /// Not something Forge deletes: an empty path, a link, a special file, a root.
    Refused(String),
    /// Nothing at the path.
    Missing(String),
    /// An unfinished job reads or writes it, or a peer is writing it (inside or around it too).
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

/// What a delete or move source names, once [`check`] passed it.
struct Checked {
    /// As typed, rebuilt from its components.
    path: PathBuf,
    /// Canonical.
    real: PathBuf,
    id: FileId,
    kind: std::fs::FileType,
    /// ShadowMount+'s mount points before its checks, for [`smp_unchanged`] (PS5).
    #[cfg(target_os = "freebsd")]
    smp: Vec<PathBuf>,
}

/// Refuses `path` as something to `verb` ("delete", "move") unless it is a file or folder
/// that is no filesystem root, is not and holds none of `protected` and overlaps no path of
/// `guarded` (unfinished jobs' sources, outputs, `.part`s and inputs).
fn check<'a>(
    path: &Path,
    verb: &str,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<Checked, DeleteError> {
    if path.as_os_str().is_empty() {
        return Err(DeleteError::Refused(format!("no path to {verb}")));
    }
    // To the OS `link/` and `link/.` name the link's target, which the link check below would
    // never see; rebuilt from its components (no trailing `/` or `.`) the path names the link.
    let path = path.components().collect::<PathBuf>();
    let shown = path.display();
    let (id, kind) = match path_id(&path) {
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
    let failed = |e: io::Error| DeleteError::Failed(format!("resolving {shown}: {e}"));
    let real = path.canonicalize().map_err(failed)?;
    // Gone or elsewhere, the journal could never restore its image (or would be replayed into
    // another one); Edit's open settles it.
    if let Some(image) =
        journal_image(&real).filter(|i| kind.is_file() && i.symlink_metadata().is_ok())
    {
        return Err(DeleteError::Refused(format!(
            "{shown} is the undo journal of an interrupted edit of {}; open that image in Edit, \
             which restores it",
            image.display()
        )));
    }
    // Taken before the checks below: SMP mounts on its own, so what it mounts after this is
    // caught by `smp_unchanged` just before the change.
    #[cfg(target_os = "freebsd")]
    let smp = smp_snapshot(
        &mut crate::edit_job::smp_list,
        &mut crate::edit_job::smp_is_mount,
    )?;
    #[cfg(target_os = "freebsd")]
    if let Some(why) = smp_refusal(
        &path,
        &real,
        kind.is_dir(),
        verb,
        &mut crate::edit_job::smp_list,
        &mut crate::edit_job::smp_is_mount,
        &mut find_image,
    ) {
        return Err(why);
    }
    let chain = ids_up(&real).map_err(failed)?;
    // A volume's root, or a folder another volume is mounted on.
    let root = match real.parent() {
        None => true,
        Some(parent) => followed_id(parent).map_err(failed)?.0 != id.0,
    };
    if root {
        return Err(DeleteError::Refused(format!(
            "{shown} is the root of a drive or volume"
        )));
    }
    for keep in protected {
        if holds(id, keep) {
            return Err(DeleteError::Refused(format!(
                "{shown} is, or holds, {}, which is not {verb}d",
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
    Ok(Checked {
        path,
        real,
        id,
        kind,
        #[cfg(target_os = "freebsd")]
        smp,
    })
}

/// Deletes `path` unless it overlaps a path of `guarded` (unfinished jobs' sources, outputs,
/// `.part`s and inputs) or is, or holds, a filesystem root or one of `protected`.
pub(crate) fn delete<'a>(
    path: &Path,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    if crate::remote::is_remote(path) {
        return delete_remote(path, guarded);
    }
    remove(ready(path, protected, guarded)?).map_err(|(e, _)| e)
}

/// [`delete`]'s checks, all done before anything is removed.
fn ready<'a>(
    path: &Path,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<Checked, DeleteError> {
    let checked = check(path, "delete", protected, guarded)?;
    let Checked {
        path,
        real,
        id,
        kind,
        ..
    } = &checked;
    let (id, kind) = (*id, *kind);
    let shown = path.display();
    let failed = |what: &str, e: io::Error| DeleteError::Failed(format!("{what} {shown}: {e}"));
    // remove_dir_all would empty a volume mounted somewhere inside before failing on it.
    #[cfg(unix)]
    if kind.is_dir()
        && let Some(mount) = mount_inside(real, id.0).map_err(|e| failed("reading", e))?
    {
        return Err(DeleteError::Refused(format!(
            "{shown} holds {}, where another volume is mounted",
            mount.display()
        )));
    }
    // The checks above looked at what `path` named then; it must still be that.
    if !path_id(real).is_ok_and(|(now, k)| now == id && k.is_dir() == kind.is_dir()) {
        return Err(DeleteError::Failed(format!(
            "{shown} changed while it was checked; nothing deleted"
        )));
    }
    // ponytail: an image SMP mounts while remove_dir_all runs is not caught; SMP has no lock
    // to hold it off.
    #[cfg(target_os = "freebsd")]
    smp_unchanged(
        &checked.smp,
        &mut crate::edit_job::smp_list,
        &mut crate::edit_job::smp_is_mount,
        "deleted",
    )?;
    Ok(checked)
}

/// Removes what [`ready`] checked; a failure says what was gone by then (`None`: nothing).
fn remove(checked: Checked) -> Result<(), (DeleteError, Option<String>)> {
    let Checked {
        path, real, kind, ..
    } = checked;
    let shown = path.display();
    let gone = |e: String, what: String| (DeleteError::Failed(e), Some(what));
    if kind.is_dir() {
        // std's remove_dir_all removes links inside without following them.
        if let Err(e) = std::fs::remove_dir_all(&real) {
            if !real.exists() {
                return Err(gone(format!("deleting {shown}: {e}"), shown.to_string()));
            }
            return Err(gone(
                format!("deleting {shown}: {e}; some of it may already be gone"),
                format!("some of {shown}"),
            ));
        }
    } else {
        std::fs::remove_file(&real)
            .map_err(|e| (DeleteError::Failed(format!("deleting {shown}: {e}")), None))?;
        // An interrupted edit's undo journal goes with its image: left behind, it would be
        // replayed into a later image of the same name.
        let journal = ps5_dump_forge_patch::journal_path(&real);
        if std::fs::symlink_metadata(&journal).is_ok_and(|m| !m.is_dir()) {
            std::fs::remove_file(&journal).map_err(|e| {
                gone(
                    format!(
                        "deleted {shown}, but not its undo journal {}: {e}",
                        journal.display()
                    ),
                    shown.to_string(),
                )
            })?;
        }
    }
    if let Some(parent) = real.parent() {
        fsync_dir(parent).map_err(|e| {
            gone(
                format!(
                    "deleted {shown}, but syncing {} failed: {e}",
                    parent.display()
                ),
                shown.to_string(),
            )
        })?;
    }
    Ok(())
}

/// What a conversion read of its source, kept for [`delete_source`].
pub(crate) enum ReadSource {
    /// An image or an archive: its stamp, an archive's every part (as opened) with it.
    File(crate::convert::SourceStamp),
    /// A folder, here or on a peer, and what the job converted of it.
    Folder(PathBuf, FolderListing),
}

impl ReadSource {
    /// What [`delete_source`] deletes: the folder, the image, or an archive's every part
    /// stamped when the job opened it (the first one first).
    pub(crate) fn paths(&self) -> Vec<PathBuf> {
        match self {
            Self::File(stamp) => stamp.paths(),
            Self::Folder(path, _) => vec![path.clone()],
        }
    }
}

/// One file of a [`FolderListing`]: path, size, modification time and, here, its identity.
type Listed = (
    String,
    u64,
    Option<std::time::SystemTime>,
    Option<crate::finalize::FileId>,
);

/// A folder as the scanner kept it (junk left out): every file's (path, size, modification
/// time, identity), every empty folder and, here, the folder's own identity.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FolderListing {
    pub(crate) files: Vec<Listed>,
    pub(crate) empty_dirs: Vec<String>,
    /// The folder's identity, so an empty folder replaced by another reads as a change.
    pub(crate) root: Option<crate::finalize::FileId>,
}

/// The identity of the file (or, `dir`, the folder) at `path` (a link not followed), so a
/// same-size, same-time replacement reads as a change. An entry of another kind is an error.
#[cfg(any(unix, windows))]
fn listed_id(path: &Path, dir: bool) -> io::Result<Option<crate::finalize::FileId>> {
    let (id, kind) = crate::finalize::path_id(path)?;
    if (dir && !kind.is_dir()) || (!dir && !kind.is_file()) {
        let what = if dir { "folder" } else { "regular file" };
        return Err(io::Error::other(format!(
            "{} is no longer a {what}",
            path.display()
        )));
    }
    Ok(Some(id))
}

// ponytail: no file identities on this OS, so a replacement keeping size and time goes
// unseen.
#[cfg(not(any(unix, windows)))]
fn listed_id(_: &Path, _: bool) -> io::Result<Option<crate::finalize::FileId>> {
    Ok(None)
}

impl FolderListing {
    /// The folder at `path` (here or on another device) scanned as a source, with its listing.
    pub(crate) fn open(
        path: &Path,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> anyhow::Result<(Box<dyn SourceTree>, Self)> {
        if crate::net::is_network(path) {
            let at = crate::remote::FlinkPath::parse(path)?;
            let remote = crate::remote::get_for(&at)?;
            let tree = crate::remote::RemoteTree::open(remote, &at, cancel)?;
            // ponytail: a peer tells no file identities, so there a replacement keeping size
            // and time goes unseen.
            let files = (tree.files().iter())
                .map(|f| (f.path.clone(), f.size, tree.mtime(&f.path), None))
                .collect();
            let empty_dirs = tree.empty_dirs().to_vec();
            let tree = crate::scan::JunkFiltered::new(Box::new(tree));
            let listing = Self {
                files,
                empty_dirs,
                root: None,
            };
            return Ok((Box::new(tree), listing));
        }
        let tree = crate::scan::ScannedFolder::scan(path, cancel)?;
        // Each identity taken right after the scan, before the job reads a byte.
        let root = listed_id(tree.root(), true)?;
        let files = (tree.files().iter().zip(tree.stamps()))
            .map(|(f, s)| {
                let mut at = tree.root().to_path_buf();
                at.extend(f.path.split('/'));
                Ok((f.path.clone(), f.size, s.mtime(), listed_id(&at, false)?))
            })
            .collect::<io::Result<_>>()?;
        let empty_dirs = tree.empty_dirs().to_vec();
        let listing = Self {
            files,
            empty_dirs,
            root,
        };
        Ok((Box::new(tree), listing))
    }
}

/// Why [`delete_source`] stopped: nothing of the source deleted, or some of it gone (what,
/// then the error).
#[derive(Debug)]
pub(crate) enum SourceKept {
    Kept(DeleteError),
    Partly(String, DeleteError),
}

/// A finished job's source ([`crate::ConvertRequest::delete_source`]), deleted as [`delete`]
/// deletes it, only while it is what the job read: a folder scanned again (the job's rules)
/// must list exactly the files, sizes, times and empty folders it converted (skipped junk may
/// go with it); an image or an archive's every part must keep its stamp. An archive's parts
/// are the ones stamped when it was opened, each checked before the first goes.
// ponytail: a file added between the rescan and the removal (or on a peer, before the peer's
// delete) goes with the folder.
#[cfg(test)]
pub(crate) fn delete_source(
    read: &ReadSource,
    protected: &[PathBuf],
    guarded: &[&PathBuf],
) -> Result<(), SourceKept> {
    source_unchanged(read)?;
    remove_source(read, protected, guarded)
}

/// [`delete_source`]'s first half: refused unless `read` is still what the job read. A
/// source on a NAS or a server is refused at once (deleted there, never here), before a
/// rescan. Reads only, so the job table need not be locked (a folder's rescan may be a long
/// listing over the network).
pub(crate) fn source_unchanged(read: &ReadSource) -> Result<(), SourceKept> {
    use crate::remote::{FlinkPath, Scheme};
    for part in read.paths() {
        if let Ok(at) = FlinkPath::parse(&part)
            && at.scheme != Scheme::Flink
        {
            return Err(SourceKept::Kept(not_here(&at)));
        }
    }
    let changed = |path: &Path, why: String| {
        SourceKept::Kept(DeleteError::Refused(format!(
            "{} changed since it was read{why}",
            path.display()
        )))
    };
    match read {
        ReadSource::File(stamp) if !stamp.unchanged() => {
            return Err(changed(stamp.path(), String::new()));
        }
        ReadSource::File(_) => {}
        // A fresh flag: the job is done, so its cancel no longer applies.
        ReadSource::Folder(path, listing) => {
            match FolderListing::open(path, &std::sync::atomic::AtomicBool::new(false)) {
                Ok((_, now)) if now == *listing => {}
                Ok(_) => return Err(changed(path, String::new())),
                Err(e) => return Err(changed(path, format!(": {e:#}"))),
            }
        }
    }
    Ok(())
}

/// [`delete_source`]'s second half, run with the job table locked: every part checked against
/// `protected` and the `guarded` paths of unfinished jobs, then removed.
pub(crate) fn remove_source(
    read: &ReadSource,
    protected: &[PathBuf],
    guarded: &[&PathBuf],
) -> Result<(), SourceKept> {
    let parts = read.paths();
    if parts.len() > 1 {
        for part in parts.iter().filter(|p| !crate::remote::is_remote(p)) {
            check(part, "delete", protected, guarded.iter().copied()).map_err(SourceKept::Kept)?;
        }
    }
    let mut gone: Vec<String> = Vec::new();
    for part in &parts {
        let result = if crate::remote::is_remote(part) {
            source_on_peer(part, guarded)
        } else {
            (ready(part, protected, guarded.iter().copied()).map_err(|e| (e, None)))
                .and_then(remove)
        };
        if let Err((e, what)) = result {
            gone.extend(what);
            return Err(match gone.is_empty() {
                true => SourceKept::Kept(e),
                false => SourceKept::Partly(gone.join(", "), e),
            });
        }
        gone.push(part.display().to_string());
    }
    Ok(())
}

/// [`delete_remote`] for [`delete_source`]: a refusal or a busy path is the peer's answer
/// before it deleted anything; any other failure may come after part of it went (the peer
/// removing entries, or syncing its folder), so it is reported as partly gone. So is a missing
/// one: the rescan just found it, so a retried request whose first reply was lost reads as
/// missing after the peer deleted it.
fn source_on_peer(path: &Path, guarded: &[&PathBuf]) -> Result<(), (DeleteError, Option<String>)> {
    let at = remote_at(path).map_err(|e| (e, None))?;
    if in_use(path, guarded.iter().copied()) {
        return Err((busy(path), None));
    }
    let remote = remote_of(&at).map_err(|e| (e, None))?;
    match remote.delete_path(&at) {
        Err(e @ (DeleteError::Failed(_) | DeleteError::Missing(_))) => {
            Err((e, Some(format!("{} may be partly deleted", path.display()))))
        }
        other => other.map_err(|e| (e, None)),
    }
}

/// What [`crate::remote::Remote::delete_path`] and `move_path` answer by default: a NAS share
/// or a server is read only here.
pub(crate) fn not_here(at: &crate::remote::FlinkPath) -> DeleteError {
    use crate::remote::Scheme;
    DeleteError::Refused(match at.scheme {
        Scheme::Smb | Scheme::Nfs => format!(
            "{}: a NAS share is read only here; delete or move it on the NAS itself",
            at.shown()
        ),
        _ => format!(
            "{}: a file on another device is deleted or moved on that device itself",
            at.shown()
        ),
    })
}

/// A path on another device: refused while one of this device's unfinished jobs uses it, then
/// handed to that device ([`crate::remote::Remote::delete_path`]: a paired Forge deletes it
/// under its own rules, a NAS refuses).
// ponytail: called with the job table locked, so a slow device holds off this device's jobs
// until it answers (as a large local folder does).
fn delete_remote<'a>(
    path: &Path,
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    let at = remote_at(path)?;
    if in_use(path, guarded) {
        return Err(busy(path));
    }
    remote_of(&at)?.delete_path(&at)
}

/// [`delete_remote`] for a move: `from` and `to` on one device, neither used by an unfinished
/// job of this device; the device moves it.
fn move_remote<'a>(
    from: &Path,
    to: &Path,
    replace: bool,
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    let (a, b) = (remote_at(from)?, remote_at(to)?);
    if a.scheme != b.scheme || a.key_id != b.key_id {
        return Err(DeleteError::Refused(format!(
            "{} and {} are on different devices: a move stays on one device",
            from.display(),
            to.display()
        )));
    }
    let guarded: Vec<&PathBuf> = guarded.into_iter().collect();
    if let Some(p) = [from, to]
        .into_iter()
        .find(|p| in_use(p, guarded.iter().copied()))
    {
        return Err(busy(p));
    }
    remote_of(&a)?.move_path(&a, &b, replace)
}

fn remote_at(path: &Path) -> Result<crate::remote::FlinkPath, DeleteError> {
    if !crate::remote::is_remote(path) {
        return Err(DeleteError::Refused(format!(
            "{}: a move from or to another device is a transfer, not a move",
            path.display()
        )));
    }
    crate::remote::FlinkPath::parse(path).map_err(|e| DeleteError::Refused(e.to_string()))
}

fn remote_of(
    at: &crate::remote::FlinkPath,
) -> Result<std::sync::Arc<dyn crate::remote::Remote>, DeleteError> {
    crate::remote::get_for(at).map_err(|e| DeleteError::Failed(format!("{}: {e}", at.shown())))
}

fn busy(path: &Path) -> DeleteError {
    DeleteError::Busy(format!(
        "{} is in use by an unfinished job; wait for it or stop it first",
        path.display()
    ))
}

/// Why a job must not replace `path` with a file of its own (an extract's or transfer's
/// [`crate::ConvertRequest::replace`]; `dir`: a folder it merges into, which stays). A file is
/// deleted in effect, so it is refused as [`delete`] refuses it, and so is an image with an
/// interrupted edit's undo journal beside it (the journal would be replayed into the new
/// one). A folder is refused only at, in or above a path of `guarded`. On a peer or a NAS
/// only that is checked here, for a file too: the device judges the rest (a peer refuses, at
/// the rename, a link, a folder, a path its own jobs use and on the PS5 `/mnt/shadowmnt`).
// ponytail: checked at preflight; what changes between the check and the rename isn't seen.
pub(crate) fn replace_refusal<'a>(
    path: &Path,
    dir: bool,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    if dir || crate::remote::is_remote(path) {
        // A folder merged into takes new files: never ShadowMount+'s mount folder.
        #[cfg(any(target_os = "freebsd", test))]
        if dir && !crate::remote::is_remote(path) {
            let real = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if let Some(why) = smp_folder(path, &real) {
                return Err(why);
            }
        }
        if in_use(path, guarded) {
            return Err(busy(path));
        }
        return Ok(());
    }
    let checked = check(path, "replace", protected, guarded)?;
    let shown = checked.path.display();
    if checked.kind.is_dir() {
        return Err(DeleteError::Refused(format!(
            "{shown} is a folder: a file is not put in its place"
        )));
    }
    if ps5_dump_forge_patch::journal_path(&checked.real)
        .symlink_metadata()
        .is_ok()
    {
        return Err(DeleteError::Refused(format!(
            "an interrupted edit is pending on {shown}; open it in Edit first"
        )));
    }
    Ok(())
}

/// Moves or renames `from` to `to` (a new name in an existing folder, never replacing
/// anything unless `replace`), refused as [`delete`] refuses `from`, and for a `to` inside
/// `from`, on another volume or at, in or above a path of `guarded`. An image with an
/// interrupted edit's undo journal beside it stays where its journal is. With `replace` and
/// `to` there, a file goes over a file (refused as [`replace_refusal`] refuses it) and a
/// folder merges into a folder ([`plan_merge`]): every check first, then the moves.
pub(crate) fn move_<'a>(
    from: &Path,
    to: &Path,
    replace: bool,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), DeleteError> {
    if crate::remote::is_remote(from) || crate::remote::is_remote(to) {
        return move_remote(from, to, replace, guarded);
    }
    let guarded: Vec<&PathBuf> = guarded.into_iter().collect();
    let Checked {
        path,
        real,
        id,
        kind,
        #[cfg(target_os = "freebsd")]
        smp,
    } = check(from, "move", protected, guarded.iter().copied())?;
    let shown = path.display();
    // A volume mounted inside would move with it (or fail half way); delete refuses it too.
    #[cfg(unix)]
    if kind.is_dir() {
        let failed = |e: io::Error| DeleteError::Failed(format!("reading {shown}: {e}"));
        if let Some(mount) = mount_inside(&real, id.0).map_err(failed)? {
            return Err(DeleteError::Refused(format!(
                "{shown} holds {}, where another volume is mounted",
                mount.display()
            )));
        }
    }
    let to = to.components().collect::<PathBuf>();
    let shown_to = to.display();
    let refused = |m: String| Err(DeleteError::Refused(m));
    let Some(Component::Normal(name)) = to.components().next_back() else {
        return refused(format!("{shown_to} does not end in a file or folder name"));
    };
    if name.to_str().is_some_and(|n| n.contains(['/', '\\'])) {
        return refused(format!("{shown_to}: a name can't hold / or \\"));
    }
    let parent = match to.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let real_parent = match parent.canonicalize() {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(DeleteError::Missing(format!(
                "{} does not exist",
                parent.display()
            )));
        }
        Err(e) => {
            let m = format!("resolving {}: {e}", parent.display());
            return Err(DeleteError::Failed(m));
        }
    };
    // FreeBSD's realpath accepts a missing last component.
    match real_parent.metadata() {
        Ok(meta) if meta.is_dir() => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(DeleteError::Missing(format!(
                "{} does not exist",
                parent.display()
            )));
        }
        _ => return refused(format!("{} is not a folder", parent.display())),
    }
    #[cfg(target_os = "freebsd")]
    if let Some(why) = smp_folder(&to, &real_parent.join(name)) {
        return Err(why);
    }
    let failed = |e: io::Error| DeleteError::Failed(format!("resolving {shown_to}: {e}"));
    let chain = ids_up(&real_parent).map_err(failed)?;
    if chain.contains(&id) {
        return refused(format!("{shown} can't be moved into itself ({shown_to})"));
    }
    if chain[0].0 != id.0 {
        return refused(format!(
            "can't move between drives; {shown_to} is on another drive or volume than {shown}"
        ));
    }
    let target = real_parent.join(name);
    // A case-only rename on a case-insensitive volume finds `from` itself at `to`; on a
    // case-sensitive one `to` can be another name (a hard link) of the same file, which the
    // folder then lists as its own entry: that one already exists.
    let recased = path_id(&target).is_ok_and(|(t, _)| t == id)
        && real.parent() == Some(&*real_parent)
        && real.file_name().is_some_and(|old| {
            old != name
                && old.to_string_lossy().to_lowercase() == name.to_string_lossy().to_lowercase()
        })
        && std::fs::read_dir(&real_parent)
            .map_err(|e| DeleteError::Failed(format!("reading {}: {e}", real_parent.display())))?
            .all(|e| e.is_ok_and(|e| e.file_name() != name));
    let replacing = !recased && target.symlink_metadata().is_ok();
    if replacing && !replace {
        return refused(format!("{shown_to} already exists; nothing is replaced"));
    }
    for used in &guarded {
        let inside = followed_id(used).is_ok_and(|g| chain.contains(&g));
        if inside || resolved(used).starts_with(&target) {
            return Err(DeleteError::Busy(format!(
                "{shown_to} is in use by an unfinished job ({}); wait for it or stop it first",
                used.display()
            )));
        }
    }
    // Left behind, the journal would be replayed into a later image of the old name; found at
    // the new one, into this image.
    let journal = ps5_dump_forge_patch::journal_path(&real);
    let journal_to = ps5_dump_forge_patch::journal_path(&target);
    if kind.is_file() && journal.symlink_metadata().is_ok() {
        return refused(format!(
            "an interrupted edit is pending on {shown}; open it in Edit first"
        ));
    }
    if let Some(image) = journal_image(&target).filter(|i| i.symlink_metadata().is_ok()) {
        return refused(format!(
            "{shown_to} would be taken for the undo journal of {}; choose another name",
            image.display()
        ));
    }
    if kind.is_file() && journal_to.symlink_metadata().is_ok() {
        return refused(format!(
            "{} is left at the new name; delete or move it first",
            journal_to.display()
        ));
    }
    let merge = if replacing {
        Some(plan_merge(&real, kind, id, &target, protected, &guarded)?)
    } else {
        None
    };
    // The checks above looked at what `from` named then; it must still be that.
    if !path_id(&real).is_ok_and(|(now, k)| now == id && k.is_dir() == kind.is_dir()) {
        return Err(DeleteError::Failed(format!(
            "{shown} changed while it was checked; nothing moved"
        )));
    }
    #[cfg(target_os = "freebsd")]
    smp_unchanged(
        &smp,
        &mut crate::edit_job::smp_list,
        &mut crate::edit_job::smp_is_mount,
        "moved",
    )?;
    let merged = match &merge {
        Some(merge) => run_merge(merge, &shown, &shown_to),
        None => Ok(()),
    };
    let moved = if merge.is_some() {
        Ok(())
    } else if recased {
        std::fs::rename(&real, &target)
    } else {
        crate::finalize::rename_no_replace(&real, &target)
    };
    match moved {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return refused(format!("{shown_to} already exists; nothing is replaced"));
        }
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            return refused(format!(
                "can't move between drives; {shown_to} is on another drive or volume than {shown}"
            ));
        }
        Err(e) => {
            let m = format!("moving {shown} to {shown_to}: {e}");
            return Err(DeleteError::Failed(m));
        }
        Ok(()) => {}
    }
    let mut dirs = vec![real_parent.as_path()];
    dirs.extend(real.parent().filter(|p| *p != real_parent));
    dirs.extend(
        merge
            .iter()
            .flat_map(|m| m.into.iter().map(PathBuf::as_path)),
    );
    // A merge that failed part way: what moved before it is synced all the same (best
    // effort; the failure is the error).
    if let Err(e) = merged {
        // The source folders steps moved entries out of are still there: synced too.
        let left = merge
            .iter()
            .flat_map(|m| m.emptied.iter().map(PathBuf::as_path));
        for dir in dirs.into_iter().chain(left) {
            let _ = fsync_dir(dir);
        }
        return Err(e);
    }
    for dir in dirs {
        fsync_dir(dir).map_err(|e| {
            DeleteError::Failed(format!(
                "moved {shown} to {shown_to}, but syncing {} failed: {e}",
                dir.display()
            ))
        })?;
    }
    Ok(())
}

/// One rename of a move with replace: over a file at `to` when `over`, else to a new name.
struct Step {
    from: PathBuf,
    to: PathBuf,
    over: bool,
}

/// What a move with replace does, all decided before the first rename.
#[derive(Default)]
struct Merge {
    steps: Vec<Step>,
    /// Source folders emptied by the steps, deepest first: removed after.
    emptied: Vec<PathBuf>,
    /// Target folders the steps add to: synced after.
    into: Vec<PathBuf>,
}

/// Plans moving `from` (canonical, `kind`, `id`) onto `to`, which exists: a file over a file,
/// a folder merged into a folder. Every kind mismatch, replaced file [`replace_refusal`]
/// refuses and undo journal clash refuses before anything moves. `to`'s own use by a job is
/// [`move_`]'s check.
fn plan_merge(
    from: &Path,
    kind: std::fs::FileType,
    id: FileId,
    to: &Path,
    protected: &[PathBuf],
    guarded: &[&PathBuf],
) -> Result<Merge, DeleteError> {
    let failed =
        |p: &Path, e: io::Error| DeleteError::Failed(format!("reading {}: {e}", p.display()));
    let (to_id, to_kind) = path_id(to).map_err(|e| failed(to, e))?;
    if to_id == id {
        return Err(DeleteError::Refused(format!(
            "{} is {} itself; nothing to replace",
            to.display(),
            from.display()
        )));
    }
    if let Some(why) = clash(from, kind, to, to_kind) {
        return Err(why);
    }
    let mut merge = Merge::default();
    if kind.is_file() {
        replace_refusal(to, false, protected, guarded.iter().copied())?;
        let (from, to) = (from.to_path_buf(), to.to_path_buf());
        merge.steps.push(Step {
            from,
            to,
            over: true,
        });
        return Ok(merge);
    }
    if ids_up(from).map_err(|e| failed(from, e))?.contains(&to_id) {
        return Err(DeleteError::Refused(format!(
            "{} holds {}; it can't be merged into it",
            to.display(),
            from.display()
        )));
    }
    merge_dirs(from, to, id.0, protected, guarded, &mut merge)?;
    Ok(merge)
}

/// Why `from` (`kind`) can't go over `to` (`to_kind`): only a file over a file and a folder
/// into a folder, never a link or special file on either side.
fn clash(
    from: &Path,
    kind: std::fs::FileType,
    to: &Path,
    to_kind: std::fs::FileType,
) -> Option<DeleteError> {
    let (shown, shown_to) = (from.display(), to.display());
    let why = if to_kind.is_symlink() {
        format!("{shown_to} is a link: nothing is put in its place")
    } else if !to_kind.is_file() && !to_kind.is_dir() {
        format!("{shown_to} is not a file or folder: nothing is put in its place")
    } else if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
        format!("{shown} is not a file or folder: it doesn't replace {shown_to}")
    } else if kind.is_file() && to_kind.is_dir() {
        format!("{shown_to} is a folder: a file is not put in its place")
    } else if kind.is_dir() && to_kind.is_file() {
        format!("{shown_to} is a file: a folder is not put in its place")
    } else {
        return None;
    };
    Some(DeleteError::Refused(why))
}

/// [`plan_merge`] for folder `from` into folder `to` (both on volume `dev`), recursing into
/// folders both hold. Links are moved as they are, never followed.
fn merge_dirs(
    from: &Path,
    to: &Path,
    dev: u64,
    protected: &[PathBuf],
    guarded: &[&PathBuf],
    merge: &mut Merge,
) -> Result<(), DeleteError> {
    let failed =
        |p: &Path, e: io::Error| DeleteError::Failed(format!("reading {}: {e}", p.display()));
    let refused = |m: String| Err(DeleteError::Refused(m));
    if path_id(to).map_err(|e| failed(to, e))?.0.0 != dev {
        return refused(format!(
            "can't move between drives; {} is on another drive or volume than {}",
            to.display(),
            from.display()
        ));
    }
    // Each folder merged into, nested ones too, as a replace refuses it (on the PS5 never
    // ShadowMount+'s mount folder found further down).
    replace_refusal(to, true, protected, guarded.iter().copied())?;
    merge.into.push(to.to_path_buf());
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(from).map_err(|e| failed(from, e))? {
        let entry = entry.map_err(|e| failed(from, e))?;
        let kind = entry.file_type().map_err(|e| failed(&entry.path(), e))?;
        entries.push((entry.file_name(), kind));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, kind) in entries {
        let (s, t) = (from.join(&name), to.join(&name));
        if let Some(why) = journal_clash(&s, kind, &t) {
            return refused(why);
        }
        let to_kind = match t.symlink_metadata() {
            Ok(meta) => meta.file_type(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                merge.steps.push(Step {
                    from: s,
                    to: t,
                    over: false,
                });
                continue;
            }
            Err(e) => return Err(failed(&t, e)),
        };
        if let Some(why) = clash(&s, kind, &t, to_kind) {
            return Err(why);
        }
        if kind.is_dir() {
            merge_dirs(&s, &t, dev, protected, guarded, merge)?;
            continue;
        }
        // Renaming a file over another name of itself changes nothing, and `s` would stay.
        if path_id(&s).is_ok_and(|(a, _)| path_id(&t).is_ok_and(|(b, _)| a == b)) {
            return refused(format!(
                "{} is {} itself; nothing to replace",
                t.display(),
                s.display()
            ));
        }
        replace_refusal(&t, false, protected, guarded.iter().copied())?;
        merge.steps.push(Step {
            from: s,
            to: t,
            over: true,
        });
    }
    merge.emptied.push(from.to_path_buf());
    Ok(())
}

/// An undo journal a merge would move away from its image or next to another one.
fn journal_clash(from: &Path, kind: std::fs::FileType, to: &Path) -> Option<String> {
    if !kind.is_file() {
        return None;
    }
    if ps5_dump_forge_patch::journal_path(from)
        .symlink_metadata()
        .is_ok()
    {
        let shown = from.display();
        return Some(format!(
            "an interrupted edit is pending on {shown}; open it in Edit first"
        ));
    }
    if let Some(image) = journal_image(to).filter(|i| i.symlink_metadata().is_ok()) {
        return Some(format!(
            "{} would be taken for the undo journal of {}; choose another name",
            to.display(),
            image.display()
        ));
    }
    let journal_to = ps5_dump_forge_patch::journal_path(to);
    journal_to.symlink_metadata().is_ok().then(|| {
        let shown = journal_to.display();
        format!("{shown} is left at the new name; delete or move it first")
    })
}

/// Runs a [`plan_merge`] plan; a failure part way says how much moved and stays.
// ponytail: what changes at a target between the plan and its rename isn't seen.
fn run_merge(
    merge: &Merge,
    shown: &dyn std::fmt::Display,
    shown_to: &dyn std::fmt::Display,
) -> Result<(), DeleteError> {
    let total = merge.steps.len();
    for (n, step) in merge.steps.iter().enumerate() {
        let done = if step.over {
            std::fs::rename(&step.from, &step.to)
        } else {
            crate::finalize::rename_no_replace(&step.from, &step.to)
        };
        if let Err(e) = done {
            let stays = match n {
                0 => "nothing was moved".to_string(),
                n => format!("{n} of {total} entries were moved before it and stay"),
            };
            return Err(DeleteError::Failed(format!(
                "moving {} to {}: {e}; {stays}",
                step.from.display(),
                step.to.display()
            )));
        }
    }
    for dir in &merge.emptied {
        std::fs::remove_dir(dir).map_err(|e| {
            DeleteError::Failed(format!(
                "moved everything in {shown} to {shown_to}, but removing {} failed: {e}",
                dir.display()
            ))
        })?;
    }
    Ok(())
}

/// Refuses `path` (as typed and resolved, `real`) at or inside ShadowMount+'s mount folder
/// (PS5 only): what is there is a mounted game, not files.
#[cfg(any(target_os = "freebsd", test))]
fn smp_folder(path: &Path, real: &Path) -> Option<DeleteError> {
    use crate::library_roots::shadowmnt;
    (shadowmnt(path) || shadowmnt(real)).then(|| {
        DeleteError::Refused(format!(
            "{} is at or inside ShadowMount+'s mount folder; nothing changed",
            path.display()
        ))
    })
}

/// Looks under a folder for a file whose name the filter accepts ([`find_image`]).
#[cfg(any(target_os = "freebsd", test))]
type FindImage<'a> = dyn FnMut(&Path, &dyn Fn(&str) -> bool) -> io::Result<Option<PathBuf>> + 'a;

/// ShadowMount+'s part of [`check`] (PS5 only): nothing at or inside its mount folder
/// ([`smp_folder`]), no image it has mounted (`list`/`is_mount`:
/// [`crate::edit_job::smp_mounts`]) and, while it has any mounted, no folder holding an image
/// of a mounted stem (`find` walks the folder for one). Fails closed: what can't be read
/// refuses.
#[cfg(any(target_os = "freebsd", test))]
fn smp_refusal(
    path: &Path,
    real: &Path,
    dir: bool,
    verb: &str,
    list: &mut dyn FnMut(&Path) -> io::Result<Vec<String>>,
    is_mount: &mut dyn FnMut(&Path) -> io::Result<bool>,
    find: &mut FindImage<'_>,
) -> Option<DeleteError> {
    use crate::edit_job::{smp_mounts, smp_stem};
    if let Some(why) = smp_folder(path, real) {
        return Some(why);
    }
    let shown = path.display();
    let refused = |m: String| Some(DeleteError::Refused(m));
    let mounted = |image: &Path, at: &Path| {
        refused(format!(
            "ShadowMount+ has {} mounted at {}: stop the game and turn ShadowMount+ off, then \
             {verb} it; nothing changed",
            image.display(),
            at.display()
        ))
    };
    if !dir {
        let name = real.file_name()?;
        let Some(name) = name.to_str() else {
            // A name SMP's mount entry can't be matched with (they are read as UTF-8).
            return smp_stem(&name.to_string_lossy()).and_then(|_| {
                refused(format!(
                    "can't tell whether ShadowMount+ has {shown} mounted (its name is not \
                     UTF-8); nothing changed"
                ))
            });
        };
        let stem = smp_stem(name)?;
        return match smp_mounts(list, is_mount, &|s| s == stem) {
            Ok(found) => found.first().and_then(|(_, at)| mounted(path, at)),
            Err((at, e)) => refused(format!(
                "can't tell whether ShadowMount+ has {shown} mounted ({}: {e}); nothing changed",
                at.display()
            )),
        };
    }
    let found = match smp_mounts(list, is_mount, &|_| true) {
        Ok(found) => found,
        Err((at, e)) => {
            return refused(format!(
                "can't tell whether ShadowMount+ has an image in {shown} mounted ({}: {e}); \
                 nothing changed",
                at.display()
            ));
        }
    };
    if found.is_empty() {
        return None;
    }
    let held = |name: &str| smp_stem(name).is_some_and(|s| found.iter().any(|(m, _)| m == s));
    match find(real, &held) {
        Ok(None) => None,
        Ok(Some(image)) => {
            let name = image.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let stem = smp_stem(name).unwrap_or(name);
            let at = found.iter().find(|(m, _)| m == stem).map(|(_, at)| at)?;
            mounted(&image, at)
        }
        Err(e) => refused(format!(
            "can't tell whether {shown} holds an image ShadowMount+ has mounted ({e}); nothing \
             changed"
        )),
    }
}

/// ShadowMount+'s mount points now (all stems); a listing error refuses (fails closed).
#[cfg(any(target_os = "freebsd", test))]
fn smp_snapshot(
    list: &mut dyn FnMut(&Path) -> io::Result<Vec<String>>,
    is_mount: &mut dyn FnMut(&Path) -> io::Result<bool>,
) -> Result<Vec<PathBuf>, DeleteError> {
    match crate::edit_job::smp_mounts(list, is_mount, &|_| true) {
        Ok(found) => Ok(found.into_iter().map(|(_, at)| at).collect()),
        Err((at, e)) => Err(DeleteError::Refused(format!(
            "can't tell what ShadowMount+ has mounted ({}: {e}); nothing changed",
            at.display()
        ))),
    }
}

/// Refuses when ShadowMount+ mounted something since `before` ([`smp_snapshot`]): the checks
/// in between could not have seen it. `done`: "deleted", "moved".
#[cfg(any(target_os = "freebsd", test))]
fn smp_unchanged(
    before: &[PathBuf],
    list: &mut dyn FnMut(&Path) -> io::Result<Vec<String>>,
    is_mount: &mut dyn FnMut(&Path) -> io::Result<bool>,
    done: &str,
) -> Result<(), DeleteError> {
    match smp_snapshot(list, is_mount)?
        .iter()
        .find(|m| !before.contains(m))
    {
        None => Ok(()),
        Some(m) => Err(DeleteError::Refused(format!(
            "ShadowMount+ mounted {} while this was checked; nothing {done}, try again",
            m.display()
        ))),
    }
}

/// At most this many entries [`find_image`] looks through before it gives up (an error).
#[cfg(any(target_os = "freebsd", test))]
const FIND_MAX: usize = 200_000;

/// The first file under `dir` (links not followed) whose name `wanted` accepts.
#[cfg(any(target_os = "freebsd", test))]
fn find_image(dir: &Path, wanted: &dyn Fn(&str) -> bool) -> io::Result<Option<PathBuf>> {
    let mut todo = vec![dir.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            seen += 1;
            if seen > FIND_MAX {
                return Err(io::Error::other(format!(
                    "more than {FIND_MAX} entries to look through"
                )));
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                todo.push(entry.path());
            } else if kind.is_file() {
                // Skipped, a mounted image with such a name would pass unseen.
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    let shown = entry.path().display().to_string();
                    let e = format!("{shown}: a name that is not UTF-8");
                    return Err(io::Error::new(io::ErrorKind::InvalidData, e));
                };
                if wanted(&name) {
                    return Ok(Some(entry.path()));
                }
            }
        }
    }
    Ok(None)
}

/// `path` with its longest existing ancestor resolved and the rest as written.
fn resolved(path: &Path) -> PathBuf {
    for above in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(real) = above.canonicalize() {
            return real.join(path.strip_prefix(above).unwrap_or(Path::new("")));
        }
    }
    path.to_path_buf()
}

/// Whether `path` is at, in or above one of `guarded`, compared as [`delete`] compares them
/// (by identity, links on the typed path included); a path that doesn't exist yet as
/// [`new_in_use`] judges it. A path on a peer or a NAS is compared with the guarded paths on
/// that device as [`FlinkPath::within`] compares them (the device judges identity there),
/// never with local ones.
pub(crate) fn in_use<'a>(path: &Path, guarded: impl IntoIterator<Item = &'a PathBuf>) -> bool {
    use crate::remote::FlinkPath;
    if crate::remote::is_remote(path) {
        let Ok(at) = FlinkPath::parse(path) else {
            return false;
        };
        // Scheme and device checked, an SMB share's paths case-folded.
        return guarded
            .into_iter()
            .any(|g| FlinkPath::parse(g).is_ok_and(|g| at.within(&g) || g.within(&at)));
    }
    let path = &path.components().collect::<PathBuf>();
    let guarded = guarded.into_iter();
    let Ok((id, _)) = path_id(path) else {
        return new_in_use(path, guarded);
    };
    // A peer's or a NAS's path is never this computer's.
    let mut guarded = guarded.filter(|g| !crate::net::is_network(g));
    let chain = (path.canonicalize().and_then(|real| ids_up(&real))).unwrap_or_default();
    guarded.any(|used| followed_id(used).is_ok_and(|g| chain.contains(&g)) || holds(id, used))
}

/// [`in_use`] for a path that doesn't exist yet (a file or folder about to be made): at, in or
/// above a guarded path as spelled or with existing folders resolved (a link to a queued job's
/// output folder), or inside one by the identity of its nearest existing folder (a new file
/// in a job's source folder or `.part`).
fn new_in_use<'a>(path: &Path, mut guarded: impl Iterator<Item = &'a PathBuf>) -> bool {
    let real = resolved(path);
    let chain = (path.ancestors().skip(1))
        .filter(|p| !p.as_os_str().is_empty())
        .find_map(|p| p.canonicalize().ok())
        .and_then(|real| ids_up(&real).ok())
        .unwrap_or_default();
    guarded.any(|g| {
        let g: PathBuf = g.components().collect();
        if path.starts_with(&g) || g.starts_with(path) {
            return true;
        }
        if crate::net::is_network(&g) {
            return false;
        }
        let at = resolved(&g);
        real.starts_with(&at)
            || at.starts_with(&real)
            || followed_id(&g).is_ok_and(|g| chain.contains(&g))
    })
}

/// The image whose undo journal ([`ps5_dump_forge_patch::journal_path`]) `path` would be.
fn journal_image(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?.strip_suffix(".forge-undo")?;
    (!name.is_empty()).then(|| path.with_file_name(name))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_folder_changed_since_it_was_read_is_kept() {
        let dir = crate::test_dir("delete-source-changed");
        let game = dir.join("game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        std::fs::write(game.join("eboot.bin"), b"eboot").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let read = |path: &Path| {
            let (_, listing) = FolderListing::open(path, &cancel).unwrap();
            ReadSource::Folder(path.to_path_buf(), listing)
        };
        let gone = |read: &ReadSource| delete_source(read, &[], &[]);
        // A file added after the job's scan: never read, so the folder stays.
        let before = read(&game);
        std::fs::write(game.join("added.bin"), b"new").unwrap();
        let Err(SourceKept::Kept(DeleteError::Refused(e))) = gone(&before) else {
            panic!("an added file must keep the source");
        };
        assert!(e.ends_with("changed since it was read"), "{e}");
        // A file grown, and one removed, the same.
        let before = read(&game);
        std::fs::write(game.join("added.bin"), b"newer").unwrap();
        assert!(matches!(gone(&before), Err(SourceKept::Kept(_))));
        let before = read(&game);
        std::fs::remove_file(game.join("added.bin")).unwrap();
        assert!(matches!(gone(&before), Err(SourceKept::Kept(_))));
        assert!(game.join("eboot.bin").is_file());
        // A file replaced by another of the same size and time: its identity tells.
        #[cfg(unix)]
        {
            let eboot = game.join("eboot.bin");
            let before = read(&game);
            let mtime = std::fs::metadata(&eboot).unwrap().modified().unwrap();
            let swap = dir.join("swap.bin");
            std::fs::write(&swap, b"EBOOT").unwrap();
            (std::fs::File::options().write(true).open(&swap).unwrap())
                .set_modified(mtime)
                .unwrap();
            std::fs::rename(&swap, &eboot).unwrap();
            let now = std::fs::metadata(&eboot).unwrap();
            assert_eq!((now.len(), now.modified().unwrap()), (5, mtime));
            let Err(SourceKept::Kept(DeleteError::Refused(e))) = gone(&before) else {
                panic!("a replaced file must keep the source");
            };
            assert!(e.ends_with("changed since it was read"), "{e}");
            assert_eq!(std::fs::read(&eboot).unwrap(), b"EBOOT");
        }
        // An empty folder replaced by another empty one: the folder's identity tells.
        #[cfg(unix)]
        {
            let empty = dir.join("empty");
            std::fs::create_dir(&empty).unwrap();
            let before = read(&empty);
            // Made while the old one still exists, so it can't reuse its inode number.
            let held = dir.join("held");
            std::fs::create_dir(&held).unwrap();
            std::fs::remove_dir(&empty).unwrap();
            std::fs::rename(&held, &empty).unwrap();
            let Err(SourceKept::Kept(DeleteError::Refused(e))) = gone(&before) else {
                panic!("a replaced empty folder must keep the source");
            };
            assert!(e.ends_with("changed since it was read"), "{e}");
            assert!(empty.is_dir());
            // Unchanged, it goes.
            gone(&read(&empty)).unwrap();
            assert!(!empty.exists());
        }
        // Junk the scanner skips may go with it.
        let before = read(&game);
        std::fs::write(game.join(".DS_Store"), b"junk").unwrap();
        gone(&before).unwrap();
        assert!(!game.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What `move_` refused, or "ok".
    fn moved(from: &Path, to: &Path, protected: &[PathBuf], guarded: &[PathBuf]) -> &'static str {
        match move_(from, to, false, protected, guarded) {
            Ok(()) => "ok",
            Err(DeleteError::Refused(_)) => "refused",
            Err(DeleteError::Missing(_)) => "missing",
            Err(DeleteError::Busy(_)) => "busy",
            Err(DeleteError::Failed(e)) => panic!("{} -> {}: {e}", from.display(), to.display()),
        }
    }

    #[test]
    fn move_refuses_what_it_must_not_move_or_replace() {
        let dir = crate::test_dir("move-refused");
        std::fs::create_dir_all(dir.join("game/data")).unwrap();
        std::fs::create_dir_all(dir.join("games/a")).unwrap();
        std::fs::write(dir.join("x.bin"), b"x").unwrap();
        std::fs::write(dir.join("y.bin"), b"y").unwrap();
        let x = dir.join("x.bin");
        // Never replaced, never into itself.
        assert_eq!(moved(&x, &dir.join("y.bin"), &[], &[]), "refused");
        assert_eq!(std::fs::read(dir.join("y.bin")).unwrap(), b"y");
        assert_eq!(moved(&x, &x, &[], &[]), "refused");
        let game = dir.join("game");
        assert_eq!(moved(&game, &game.join("data/game"), &[], &[]), "refused");
        assert_eq!(moved(&game, &game.join("inner"), &[], &[]), "refused");
        // Not there, or nowhere to go.
        assert_eq!(
            moved(&dir.join("none"), &dir.join("z"), &[], &[]),
            "missing"
        );
        assert_eq!(moved(&x, &dir.join("none/x.bin"), &[], &[]), "missing");
        assert_eq!(moved(&x, &dir.join("y.bin/x.bin"), &[], &[]), "refused");
        // Bad target names: none, `..`, a backslash.
        for to in [PathBuf::new(), dir.join("game/.."), dir.join("a\\b")] {
            #[cfg(windows)]
            if to.ends_with("a\\b") {
                continue;
            }
            assert_eq!(moved(&x, &to, &[], &[]), "refused", "{}", to.display());
        }
        assert_eq!(moved(Path::new(""), &dir.join("z"), &[], &[]), "refused");
        // A root, a protected root and what holds it.
        let top = dir.ancestors().last().unwrap();
        assert_eq!(moved(top, &dir.join("z"), &[], &[]), "refused");
        let keep = [dir.join("games")];
        assert_eq!(
            moved(&dir.join("games"), &dir.join("g2"), &keep, &[]),
            "refused"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&game, dir.join("link")).unwrap();
            assert_eq!(
                moved(&dir.join("link"), &dir.join("l2"), &[], &[]),
                "refused"
            );
            let fifo = std::ffi::CString::new(dir.join("fifo").to_str().unwrap()).unwrap();
            // SAFETY: a valid C string.
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
            assert_eq!(
                moved(&dir.join("fifo"), &dir.join("f2"), &[], &[]),
                "refused"
            );
        }
        assert!(x.is_file() && game.join("data").is_dir() && dir.join("games/a").is_dir());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What `move_` with replace refused, or "ok".
    fn replaced(from: &Path, to: &Path, guarded: &[PathBuf]) -> &'static str {
        match move_(from, to, true, &[], guarded) {
            Ok(()) => "ok",
            Err(DeleteError::Refused(_)) => "refused",
            Err(DeleteError::Missing(_)) => "missing",
            Err(DeleteError::Busy(_)) => "busy",
            Err(DeleteError::Failed(e)) => panic!("{} -> {}: {e}", from.display(), to.display()),
        }
    }

    #[test]
    fn move_with_replace_merges_a_folder_into_a_folder() {
        let dir = crate::test_dir("move-replace-merge");
        for (path, data) in [
            ("src/same.bin", "new"),
            ("src/only-new.bin", "n"),
            ("src/data/deep/same.bin", "new deep"),
            ("src/data/fresh/f.bin", "f"),
            ("dst/same.bin", "old"),
            ("dst/only-old.bin", "o"),
            ("dst/data/keep.bin", "k"),
            ("dst/data/deep/same.bin", "old deep"),
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
        }
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        assert_eq!(moved(&src, &dst, &[], &[]), "refused");
        assert_eq!(replaced(&src, &dst, &[]), "ok");
        assert!(!src.exists());
        let read = |p: &str| std::fs::read_to_string(dst.join(p)).unwrap();
        assert_eq!(read("same.bin"), "new");
        assert_eq!(read("only-new.bin"), "n");
        assert_eq!(read("only-old.bin"), "o");
        assert_eq!(read("data/keep.bin"), "k");
        assert_eq!(read("data/deep/same.bin"), "new deep");
        assert_eq!(read("data/fresh/f.bin"), "f");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A kind mismatch at any level, or a replaced file a job uses, refuses before anything
    /// moves.
    #[test]
    fn move_with_replace_checks_everything_first() {
        let dir = crate::test_dir("move-replace-refused");
        for path in [
            "src/a.bin",
            "src/sub/b.bin",
            "src/sub/clash",
            "dst/a.bin",
            "dst/sub/b.bin",
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        // A file in src where dst has a folder, deep down.
        std::fs::create_dir_all(dir.join("dst/sub/clash")).unwrap();
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        match move_(&src, &dst, true, &[], &[]) {
            Err(DeleteError::Refused(m)) => {
                assert!(
                    m.ends_with("is a folder: a file is not put in its place"),
                    "{m}"
                )
            }
            other => panic!("{other:?}"),
        }
        let untouched = || {
            [
                "src/a.bin",
                "src/sub/b.bin",
                "src/sub/clash",
                "dst/a.bin",
                "dst/sub/b.bin",
            ]
            .iter()
            .all(|p| dir.join(p).is_file())
        };
        assert!(untouched());
        // A folder over a file, a file over a folder.
        assert_eq!(
            replaced(&dir.join("src/sub"), &dir.join("dst/a.bin"), &[]),
            "refused"
        );
        assert_eq!(
            replaced(&dir.join("src/a.bin"), &dir.join("dst/sub"), &[]),
            "refused"
        );
        std::fs::remove_dir(dir.join("dst/sub/clash")).unwrap();
        // A replaced file a job uses, at the top or deep in a merge.
        let used = [dir.join("dst/sub/b.bin")];
        assert_eq!(replaced(&src, &dst, &used), "busy");
        let used = [dir.join("dst/a.bin")];
        assert_eq!(
            replaced(&dir.join("src/a.bin"), &dir.join("dst/a.bin"), &used),
            "busy"
        );
        assert!(untouched());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("src"), dir.join("dst/link")).unwrap();
            std::fs::create_dir_all(dir.join("src/sub/link")).unwrap();
            let link = dir.join("dst/link");
            assert_eq!(replaced(&dir.join("src/sub/link"), &link, &[]), "refused");
            assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        }
        assert_eq!(replaced(&src, &dst, &[]), "ok");
        assert!(!src.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Nothing an unfinished job uses is moved, nor anything around it, and nothing is moved
    /// to where a job reads or writes.
    #[test]
    fn move_leaves_what_unfinished_jobs_use() {
        let dir = crate::test_dir("move-busy");
        std::fs::create_dir_all(dir.join("src/data")).unwrap();
        std::fs::create_dir_all(dir.join("free")).unwrap();
        std::fs::write(dir.join("loose.bin"), b"l").unwrap();
        // A job's source and its output, not written yet.
        let guarded = [dir.join("src"), dir.join("out/game.ffpkg")];
        let loose = dir.join("loose.bin");
        for from in [dir.join("src"), dir.join("src/data"), dir.clone()] {
            let to = dir.join("free/moved");
            assert_eq!(
                moved(&from, &to, &[], &guarded),
                "busy",
                "{}",
                from.display()
            );
        }
        for to in [
            dir.join("src/loose.bin"),
            dir.join("src/data/loose.bin"),
            dir.join("out"),
            dir.join("out/game.ffpkg"),
        ] {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            assert_eq!(
                moved(&loose, &to, &[], &guarded),
                "busy",
                "{}",
                to.display()
            );
        }
        assert!(loose.is_file());
        assert_eq!(
            moved(&loose, &dir.join("free/loose.bin"), &[], &guarded),
            "ok"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An image with an interrupted edit's journal stays with it, and no image is moved to a
    /// name a journal is left at.
    #[test]
    fn move_keeps_an_image_with_its_undo_journal() {
        let dir = crate::test_dir("move-journal");
        let image = dir.join("game.ffpkg");
        std::fs::write(&image, b"img").unwrap();
        let journal = ps5_dump_forge_patch::journal_path(&image);
        std::fs::write(&journal, b"FORGEUNDO1").unwrap();
        assert_eq!(moved(&image, &dir.join("new.ffpkg"), &[], &[]), "refused");
        assert!(image.is_file());
        // Nor the journal, while its image is there: not moved, renamed or deleted.
        assert_eq!(
            moved(&journal, &dir.join("other.ffpkg.forge-undo"), &[], &[]),
            "refused"
        );
        assert_eq!(
            moved(&journal, &dir.join("sub.forge-undo"), &[], &[]),
            "refused"
        );
        let none: [PathBuf; 0] = [];
        assert!(matches!(
            delete(&journal, &[], &none),
            Err(DeleteError::Refused(_))
        ));
        assert!(journal.is_file());
        // Nothing takes a journal's name beside an image.
        let other = dir.join("other.ffpkg");
        std::fs::write(&other, b"img2").unwrap();
        std::fs::write(dir.join("loose.bin"), b"x").unwrap();
        let taken = dir.join("other.ffpkg.forge-undo");
        assert_eq!(moved(&dir.join("loose.bin"), &taken, &[], &[]), "refused");
        std::fs::remove_file(&other).unwrap();
        std::fs::rename(&journal, dir.join("left.ffpkg.forge-undo")).unwrap();
        assert_eq!(moved(&image, &dir.join("left.ffpkg"), &[], &[]), "refused");
        assert_eq!(moved(&image, &dir.join("new.ffpkg"), &[], &[]), "ok");
        // A journal without its image is a plain file.
        let stray = dir.join("left.ffpkg.forge-undo");
        assert!(delete(&stray, &[], &none).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What ShadowMount+ mounts between the checks and the change refuses; what was already
    /// mounted (and passed the checks) does not, and an unreadable mount folder fails closed.
    #[test]
    fn a_mount_made_while_checking_refuses() {
        let listing = |names: &'static [&'static str]| {
            move |d: &Path| -> io::Result<Vec<String>> {
                match d.to_str() {
                    Some("/mnt/shadowmnt") => Ok(names.iter().map(|n| n.to_string()).collect()),
                    _ => Err(io::ErrorKind::NotFound.into()),
                }
            }
        };
        let is_mount = &mut |_: &Path| -> io::Result<bool> { Ok(true) };
        let before = smp_snapshot(&mut listing(&["A_0a1b2c3d"]), is_mount).unwrap();
        assert_eq!(before.len(), 1);
        let same = &mut listing(&["A_0a1b2c3d"]);
        assert!(smp_unchanged(&before, same, is_mount, "deleted").is_ok());
        let gone = &mut listing(&[]);
        assert!(smp_unchanged(&before, gone, is_mount, "deleted").is_ok());
        let more = &mut listing(&["A_0a1b2c3d", "B_00000000"]);
        match smp_unchanged(&before, more, is_mount, "moved") {
            Err(DeleteError::Refused(m)) => {
                let m = m.replace('\\', "/");
                assert!(m.contains("mounted /mnt/shadowmnt/B_00000000 while"), "{m}");
                assert!(m.contains("nothing moved"), "{m}");
            }
            other => panic!("{other:?}"),
        }
        let denied = &mut |_: &Path| -> io::Result<Vec<String>> {
            Err(io::ErrorKind::PermissionDenied.into())
        };
        assert!(matches!(
            smp_unchanged(&before, denied, is_mount, "deleted"),
            Err(DeleteError::Refused(_))
        ));
    }

    #[test]
    fn shadowmount_paths_and_mounted_images_are_refused() {
        let dir = crate::test_dir("delete-smp");
        std::fs::create_dir_all(dir.join("games/deep")).unwrap();
        std::fs::write(dir.join("games/deep/My Game.FFPKG"), b"img").unwrap();
        std::fs::write(dir.join("games/Other.exfat"), b"img").unwrap();
        let real = dir.canonicalize().unwrap();
        let mounted = |names: &'static [&'static str]| {
            move |d: &Path| -> io::Result<Vec<String>> {
                match d.to_str() {
                    Some("/mnt/shadowmnt") => Ok(names.iter().map(|n| n.to_string()).collect()),
                    _ => Err(io::ErrorKind::NotFound.into()),
                }
            }
        };
        let is_mount = &mut |_: &Path| -> io::Result<bool> { Ok(true) };
        let walks = std::cell::Cell::new(0);
        let find = &mut |d: &Path, w: &dyn Fn(&str) -> bool| {
            walks.set(walks.get() + 1);
            find_image(d, w)
        };
        // Joined with the OS separator: `\` on Windows.
        let message = |r: Option<DeleteError>| match r {
            Some(DeleteError::Refused(m)) => m.replace('\\', "/"),
            other => panic!("{other:?}"),
        };
        let image = real.join("games/deep/My Game.FFPKG");
        let got = message(smp_refusal(
            &image,
            &image,
            false,
            "delete",
            &mut mounted(&["My Game_0a1b2c3d"]),
            is_mount,
            find,
        ));
        assert!(got.starts_with("ShadowMount+ has "), "{got}");
        assert!(
            got.contains("My Game.FFPKG mounted at /mnt/shadowmnt/My Game_0a1b2c3d: stop the game"),
            "{got}"
        );
        assert!(got.ends_with("then delete it; nothing changed"), "{got}");
        let other = real.join("games/Other.exfat");
        let list = &mut mounted(&["My Game_0a1b2c3d"]);
        assert_eq!(
            smp_refusal(&other, &other, false, "move", list, is_mount, find),
            None
        );
        // A folder holding one, however deep.
        let games = real.join("games");
        let got = message(smp_refusal(
            &games, &games, true, "move", list, is_mount, find,
        ));
        assert!(got.contains("deep/My Game.FFPKG mounted at"), "{got}");
        assert!(got.ends_with("then move it; nothing changed"), "{got}");
        assert_eq!(walks.get(), 1);
        // Mounts of other stems: walked, nothing found.
        let list = &mut mounted(&["Third_0a1b2c3d"]);
        assert_eq!(
            smp_refusal(&games, &games, true, "delete", list, is_mount, find),
            None
        );
        assert_eq!(walks.get(), 2);
        // No mounts: no walk.
        let none = &mut mounted(&[]);
        assert_eq!(
            smp_refusal(&games, &games, true, "delete", none, is_mount, find),
            None
        );
        let unmounted = &mut |_: &Path| -> io::Result<bool> { Ok(false) };
        let list = &mut mounted(&["My Game_0a1b2c3d"]);
        assert_eq!(
            smp_refusal(&games, &games, true, "delete", list, unmounted, find),
            None
        );
        assert_eq!(
            smp_refusal(&image, &image, false, "delete", list, unmounted, find),
            None
        );
        assert_eq!(walks.get(), 2);
        // Fails closed: mounts that can't be listed or stat'ed, a walk that fails.
        let denied = &mut |_: &Path| -> io::Result<Vec<String>> {
            Err(io::ErrorKind::PermissionDenied.into())
        };
        let got = message(smp_refusal(
            &image, &image, false, "delete", denied, is_mount, find,
        ));
        assert!(
            got.starts_with("can't tell whether ShadowMount+ has "),
            "{got}"
        );
        let got = message(smp_refusal(
            &games, &games, true, "delete", denied, is_mount, find,
        ));
        assert!(
            got.starts_with("can't tell whether ShadowMount+ has an image in "),
            "{got}"
        );
        let stat =
            &mut |_: &Path| -> io::Result<bool> { Err(io::ErrorKind::PermissionDenied.into()) };
        assert!(smp_refusal(&image, &image, false, "delete", list, stat, find).is_some());
        let broken = &mut |_: &Path, _: &dyn Fn(&str) -> bool| -> io::Result<Option<PathBuf>> {
            Err(io::Error::other("walk failed"))
        };
        let got = message(smp_refusal(
            &games, &games, true, "delete", list, is_mount, broken,
        ));
        assert!(
            got.starts_with("can't tell whether ") && got.contains("walk failed"),
            "{got}"
        );
        // ShadowMount+'s folder itself, as typed or resolved, source or target.
        for at in [
            "/mnt/shadowmnt",
            "/mnt/shadowmnt/My Game_0a1b2c3d",
            "/mnt/shadowmnt/pfsc/x",
        ] {
            let got = message(smp_refusal(
                Path::new(at),
                &games,
                true,
                "delete",
                none,
                is_mount,
                find,
            ));
            assert!(
                got.ends_with("is at or inside ShadowMount+'s mount folder; nothing changed"),
                "{got}"
            );
            assert!(smp_folder(&games, Path::new(at)).is_some());
        }
        assert_eq!(smp_folder(&games, &games), None);
        assert_eq!(smp_folder(Path::new("/mnt/shadowmnt2"), &games), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A queued job's output doesn't exist yet: [`new_in_use`] judges it through its folders,
    /// links included.
    #[cfg(unix)]
    #[test]
    fn a_new_path_is_judged_by_its_folders() {
        let dir = crate::test_dir("in-use-new");
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("alias")).unwrap();
        let guarded = [dir.join("alias/new.exfat"), dir.join("game")];
        assert!(in_use(&dir.join("real/new.exfat"), &guarded));
        assert!(in_use(&dir.join("game/new/file"), &guarded));
        assert!(!in_use(&dir.join("real/other.exfat"), &guarded));
        assert!(!in_use(&dir.join("gamer"), &guarded));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A NAS or server source is refused before any rescan (no network reached here).
    #[test]
    fn a_nas_source_is_kept_without_a_rescan() {
        let empty = || FolderListing {
            files: Vec::new(),
            empty_dirs: Vec::new(),
            root: None,
        };
        for at in [
            "smb://nas/share/game",
            "nfs://nas/srv/game",
            "ftp://nas/game",
        ] {
            let read = ReadSource::Folder(PathBuf::from(at), empty());
            let Err(SourceKept::Kept(DeleteError::Refused(e))) = source_unchanged(&read) else {
                panic!("{at}: not refused");
            };
            assert!(
                e.contains("deleted or moved on") || e.contains("read only here"),
                "{e}"
            );
        }
    }
}
