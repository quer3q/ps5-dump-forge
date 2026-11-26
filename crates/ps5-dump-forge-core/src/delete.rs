//! Delete and move of what the user confirmed, both refused by one `check` (AGENTS.md, Never
//! touch the source, exception 1). Called with the job table locked, so no job is admitted
//! between the check and the change.

use std::io;
use std::path::{Component, Path, PathBuf};

use ps5upload_fpkg::source::SourceTree;

use crate::finalize::{FileId, followed_id, fsync_dir, path_id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteError {
    Refused(String),
    Missing(String),
    /// An unfinished job or a peer uses it (inside or around it too).
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

struct Checked {
    /// As typed, rebuilt from its components.
    path: PathBuf,
    real: PathBuf,
    id: FileId,
    kind: std::fs::FileType,
    #[cfg(target_os = "freebsd")]
    smp: Vec<PathBuf>,
}

/// Refuses `path` unless a file or folder, no filesystem root, none of `protected` (or holding
/// one) and overlapping no path of `guarded` (unfinished jobs' paths).
fn check<'a>(
    path: &Path,
    verb: &str,
    protected: &[PathBuf],
    guarded: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<Checked, DeleteError> {
    if path.as_os_str().is_empty() {
        return Err(DeleteError::Refused(format!("no path to {verb}")));
    }
    // `link/` and `link/.` name the target to the OS; rebuilt from components it names the link.
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
    // Gone or elsewhere, the journal could never restore its image; Edit's open settles it.
    if let Some(image) =
        journal_image(&real).filter(|i| kind.is_file() && i.symlink_metadata().is_ok())
    {
        return Err(DeleteError::Refused(format!(
            "{shown} is the undo journal of an interrupted edit of {}; open that image in Edit, \
             which restores it",
            image.display()
        )));
    }
    // Taken first: what SMP mounts after this is caught by `smp_unchanged` before the change.
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
    // remove_dir_all would empty a volume mounted somewhere inside before failing on it.
    #[cfg(unix)]
    if kind.is_dir()
        && let Some(mount) = mount_inside(real, id.0)
            .map_err(|e| DeleteError::Failed(format!("reading {shown}: {e}")))?
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
    // ponytail: an image SMP mounts while remove_dir_all runs is not caught (SMP has no lock).
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
        // The undo journal goes with its image: left, it would replay into a later one.
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
    /// An image or an archive (every part as opened).
    File(crate::convert::SourceStamp),
    Folder(PathBuf, FolderListing),
}

impl ReadSource {
    /// What [`delete_source`] deletes, an archive's first part first.
    pub(crate) fn paths(&self) -> Vec<PathBuf> {
        match self {
            Self::File(stamp) => stamp.paths(),
            Self::Folder(path, _) => vec![path.clone()],
        }
    }
}

/// (path, size, mtime, identity here).
type Listed = (
    String,
    u64,
    Option<std::time::SystemTime>,
    Option<crate::finalize::FileId>,
);

/// A folder as the scanner kept it (junk left out).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FolderListing {
    pub(crate) files: Vec<Listed>,
    pub(crate) empty_dirs: Vec<String>,
    /// So an empty folder replaced by another reads as a change.
    pub(crate) root: Option<crate::finalize::FileId>,
}

/// The identity at `path` (link not followed); an entry of another kind is an error.
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
    /// The folder at `path` (here or on another device) scanned as a source.
    pub(crate) fn open(
        path: &Path,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> anyhow::Result<(Box<dyn SourceTree>, Self)> {
        if crate::net::is_network(path) {
            let at = crate::remote::FlinkPath::parse(path)?;
            let remote = crate::remote::get_for(&at)?;
            let tree = crate::remote::RemoteTree::open(remote, &at, cancel)?;
            // ponytail: a peer tells no identities: a same-size, same-time replacement is unseen.
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

/// Why [`delete_source`] stopped: nothing deleted, or some of it gone (what, then the error).
#[derive(Debug)]
pub(crate) enum SourceKept {
    Kept(DeleteError),
    Partly(String, DeleteError),
}

/// A finished job's source, deleted as [`delete`] does, only while it is what the job read.
// ponytail: a file added between the rescan and the removal goes with the folder.
#[cfg(test)]
pub(crate) fn delete_source(
    read: &ReadSource,
    protected: &[PathBuf],
    guarded: &[&PathBuf],
) -> Result<(), SourceKept> {
    source_unchanged(read)?;
    remove_source(read, protected, guarded)
}

/// [`delete_source`]'s first half, reads only (no job-table lock: a rescan may be long). A NAS
/// or server source is kept at once.
pub(crate) fn source_unchanged(read: &ReadSource) -> Result<(), SourceKept> {
    use crate::remote::{FlinkPath, Scheme};
    for part in read.paths() {
        if let Ok(at) = FlinkPath::parse(&part)
            && at.scheme != Scheme::Flink
        {
            return Err(SourceKept::Kept(DeleteError::Refused(format!(
                "{}: a source on a NAS or server is never deleted after a job; delete it in the \
                 File explorer",
                at.shown()
            ))));
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
        // A fresh flag: the job's cancel no longer applies.
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

/// [`delete_source`]'s second half, with the job table locked: every part checked, then removed.
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

/// [`delete_remote`] for [`delete_source`]: refused or busy means nothing went; any other failure
/// is partly gone, a missing one too (the rescan just found it: a retry whose reply was lost).
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

/// [`crate::remote::Remote::delete_path`]'s and `move_path`'s default answer.
pub(crate) fn not_here(at: &crate::remote::FlinkPath) -> DeleteError {
    DeleteError::Refused(format!(
        "{}: a file on another device is deleted or moved on that device itself",
        at.shown()
    ))
}

/// A NAS share, export or server root: never deleted or moved.
pub(crate) fn nas_root(at: &crate::remote::FlinkPath) -> DeleteError {
    DeleteError::Refused(format!(
        "{}: a share, export or server itself is not deleted or moved",
        at.shown()
    ))
}

pub(crate) fn nas_other_share(
    from: &crate::remote::FlinkPath,
    to: &crate::remote::FlinkPath,
) -> DeleteError {
    DeleteError::Refused(format!(
        "{} and {} are on different shares: a move stays on one share",
        from.shown(),
        to.shown()
    ))
}

const NAS_DELETE_MAX: usize = 1_000_000;

/// Deletes `at` on a NAS (the adapter refused its roots): a folder listed all the way down first
/// (any failure refuses before anything goes; links never entered), then removed bottom-up.
// ponytail: a server presenting a link as a folder (Samba following it, FTP without MLSD types)
// has it entered; a change between the listing and the removal isn't seen.
pub(crate) fn nas_delete<R: crate::remote::Remote + ?Sized>(
    r: &R,
    at: &crate::remote::FlinkPath,
) -> Result<(), DeleteError> {
    let shown = at.shown();
    let (enter, is_dir) = nas_kind(r, at)?;
    // A filesystem boundary (an NFSv4 export root, a mount inside a share) is never crossed.
    let fs = r.fs_id(at).map_err(|e| nas_error(&shown, "reading", e))?;
    if let (Some(fs), Some(up)) = (fs, at.parent()) {
        let above = r
            .fs_id(&up)
            .map_err(|e| nas_error(&up.shown(), "reading", e))?;
        if above.is_some_and(|a| a != fs) {
            return Err(nas_boundary(at));
        }
    }
    // (path, entered, removed as a folder): an SMB junction is removed as a folder, never entered.
    let mut plan = vec![(at.clone(), enter, is_dir)];
    let mut i = 0;
    while i < plan.len() {
        let (dir, enter, _) = plan[i].clone();
        i += 1;
        if !enter {
            continue;
        }
        if i > 1
            && let Some(fs) = fs
            && r.fs_id(&dir)
                .map_err(|e| nas_error(&dir.shown(), "reading", e))?
                != Some(fs)
        {
            return Err(nas_boundary(&dir));
        }
        let entries = r.list(&dir).map_err(|e| {
            DeleteError::Failed(format!("listing {}: {e}; nothing deleted", dir.shown()))
        })?;
        for e in entries {
            if e.bad_name || !crate::remote::addressable(&e.name) {
                return Err(DeleteError::Refused(format!(
                    "{} holds {:?}, a name that can't be reached from here; nothing deleted",
                    dir.shown(),
                    e.name
                )));
            }
            plan.push((dir.join(&e.name), e.dir && !e.link && !e.special, e.dir));
            if plan.len() > NAS_DELETE_MAX {
                return Err(DeleteError::Refused(format!(
                    "{shown} holds more than {NAS_DELETE_MAX} entries; nothing deleted"
                )));
            }
        }
    }
    // Every entry comes after its folder: backwards, each folder is empty when it goes.
    let total = plan.len();
    for (n, (p, _, dir)) in plan.iter().rev().enumerate() {
        let done = if *dir { r.rmdir(p) } else { r.remove(p) };
        if let Err(e) = done {
            return Err(DeleteError::Failed(match n {
                0 => format!("deleting {shown}: {e}; nothing deleted"),
                _ => format!(
                    "deleting {shown}: {e}; {n} of {total} entries were deleted before it, so \
                     {shown} is partly deleted"
                ),
            }));
        }
    }
    Ok(())
}

/// (entered, removed as a folder), as `at`'s folder lists it (SMB and FTP stats follow links).
fn nas_kind<R: crate::remote::Remote + ?Sized>(
    r: &R,
    at: &crate::remote::FlinkPath,
) -> Result<(bool, bool), DeleteError> {
    let shown = at.shown();
    let s = r.stat(at).map_err(|e| nas_error(&shown, "reading", e))?;
    let (link, dir, special) = nas_listed(r, at, &s)?;
    Ok((dir && !link && !special, dir))
}

/// `at`'s (link, folder, special) as its folder lists it: its own spelling first, else an ASCII
/// case-folded one (SMB). Not listed: refused.
fn nas_listed<R: crate::remote::Remote + ?Sized>(
    r: &R,
    at: &crate::remote::FlinkPath,
    s: &crate::remote::RemoteStat,
) -> Result<(bool, bool, bool), DeleteError> {
    let Some(up) = at.parent() else {
        return Err(nas_root(at));
    };
    let leaf = at.path.rsplit('/').next().unwrap_or_default();
    let listed = r
        .list(&up)
        .map_err(|e| DeleteError::Failed(format!("listing {}: {e}; nothing done", up.shown())))?;
    let e = (listed.iter().find(|e| e.name == leaf))
        .or_else(|| listed.iter().find(|e| e.name.eq_ignore_ascii_case(leaf)))
        .ok_or_else(|| {
            DeleteError::Refused(format!(
                "{} is not listed in {}; nothing done",
                at.shown(),
                up.shown()
            ))
        })?;
    Ok((s.link || e.link, e.dir, e.special))
}

fn nas_boundary(at: &crate::remote::FlinkPath) -> DeleteError {
    DeleteError::Refused(format!(
        "{}: an export or a filesystem mounted there is not deleted or moved; nothing deleted",
        at.shown()
    ))
}

fn nas_error(shown: &str, doing: &str, e: io::Error) -> DeleteError {
    match e.kind() {
        io::ErrorKind::NotFound => DeleteError::Missing(format!("{shown} does not exist")),
        _ => DeleteError::Failed(format!("{doing} {shown}: {e}")),
    }
}

/// Moves `from` to `to` on one NAS share (the adapter refused roots and other shares): never
/// replacing unless `replace`, then only a file over a file. A case-only rename goes via a temp name.
pub(crate) fn nas_move<R: crate::remote::Remote + ?Sized>(
    r: &R,
    from: &crate::remote::FlinkPath,
    to: &crate::remote::FlinkPath,
    replace: bool,
) -> Result<(), DeleteError> {
    let (a, b) = (from.shown(), to.shown());
    let refused = |m: String| Err(DeleteError::Refused(m));
    let (Some(up), Some(up_to)) = (from.parent(), to.parent()) else {
        return Err(nas_root(if from.path.is_empty() { from } else { to }));
    };
    let name = to.path.rsplit('/').next().unwrap_or_default();
    let recased =
        up.path == up_to.path && from.path != to.path && from.path.eq_ignore_ascii_case(&to.path);
    if !recased && to.within(from) {
        return refused(format!("{a} can't be moved into itself ({b})"));
    }
    let s = r.stat(from).map_err(|e| nas_error(&a, "reading", e))?;
    if let Some(fs) = r.fs_id(from).map_err(|e| nas_error(&a, "reading", e))?
        && r.fs_id(&up)
            .map_err(|e| nas_error(&up.shown(), "reading", e))?
            .is_some_and(|u| u != fs)
    {
        return Err(nas_boundary(from));
    }
    match r.stat(&up_to) {
        Ok(p) if p.dir => {}
        Ok(_) => return refused(format!("{} is not a folder", up_to.shown())),
        Err(e) => return Err(nas_error(&up_to.shown(), "reading", e)),
    }
    let there = match r.stat(to) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(nas_error(&b, "reading", e)),
        Ok(t) => Some(t),
    };
    let renamed = |e: io::Error| match e.kind() {
        io::ErrorKind::NotFound => DeleteError::Missing(format!("{a} does not exist")),
        io::ErrorKind::AlreadyExists => {
            DeleteError::Refused(format!("{b} already exists; nothing is replaced"))
        }
        io::ErrorKind::CrossesDevices => nas_other_share(from, to),
        _ => DeleteError::Failed(format!("moving {a} to {b}: {e}")),
    };
    let Some(t) = there else {
        return r.rename_no_replace(from, to).map_err(renamed);
    };
    // A case-insensitive server finds `from` at `to`: unless listed apart, a rename of `from`.
    if recased {
        let listed = r
            .list(&up)
            .map_err(|e| nas_error(&up.shown(), "listing", e))?;
        if !listed.iter().any(|e| e.name == name) {
            return nas_recase(r, from, to).map_err(renamed);
        }
    }
    if !replace {
        return refused(format!("{b} already exists; nothing is replaced"));
    }
    // As the folder lists it: an SMB stat follows a link.
    let (t_link, t_dir, _) = nas_listed(r, to, &t)?;
    match (s.dir, t_dir, t_link) {
        (_, _, true) => refused(format!("{b} is a link: nothing is put in its place")),
        (false, false, _) => r.rename_replace(from, to).map_err(renamed),
        (true, true, _) => refused(format!(
            "{b} is a folder: folders are not merged on a NAS; delete it first or choose another name"
        )),
        (true, false, _) => refused(format!("{b} is a file: a folder is not put in its place")),
        (false, true, _) => refused(format!("{b} is a folder: a file is not put in its place")),
    }
}

/// A case-only rename through a temporary name; put back where the second rename fails.
fn nas_recase<R: crate::remote::Remote + ?Sized>(
    r: &R,
    from: &crate::remote::FlinkPath,
    to: &crate::remote::FlinkPath,
) -> io::Result<()> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let tmp = from
        .parent()
        .unwrap_or_else(|| from.clone())
        .join(&format!(".forge-move-{}-{n}", std::process::id()));
    r.rename_no_replace(from, &tmp)?;
    r.rename_no_replace(&tmp, to)
        .map_err(|e| match r.rename_no_replace(&tmp, from) {
            Ok(()) => e,
            Err(_) => io::Error::new(
                e.kind(),
                format!("{e}; {} is left as {}", from.shown(), tmp.shown()),
            ),
        })
}

/// A path on another device: refused while this device's jobs use it, else handed to it.
// ponytail: under the job-table lock, so a slow device holds off this device's jobs.
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

/// Why a job must not replace `path` (`dir`: a folder merged into). A file is deleted in effect,
/// so refused as [`delete`] refuses it, or with an undo journal beside it; a folder (and anything
/// remote: the device judges the rest) only by `guarded`.
// ponytail: checked at preflight; a change between the check and the rename isn't seen.
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

/// Moves `from` to `to` (a new name in an existing folder), refused as [`delete`] refuses `from`,
/// and for a `to` inside it, on another volume or used by a job. With `replace`, a file goes over
/// a file and a folder merges ([`plan_merge`]): every check first, then the moves.
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
    // A volume mounted inside would move with it (or fail half way).
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
    // A case-only rename finds `from` at `to`; on a case-sensitive volume `to` may be a hard
    // link of the same file, listed as its own entry: that one exists.
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
    // Left behind, the journal would be replayed into a later image of the old name.
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
    // A merge that failed part way: what moved is synced all the same (best effort).
    if let Err(e) = merged {
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

struct Step {
    from: PathBuf,
    to: PathBuf,
    over: bool,
}

#[derive(Default)]
struct Merge {
    steps: Vec<Step>,
    /// Source folders emptied by the steps, deepest first: removed after.
    emptied: Vec<PathBuf>,
    /// Target folders the steps add to: synced after.
    into: Vec<PathBuf>,
}

/// Plans moving `from` onto an existing `to`: a file over a file, a folder merged. Every
/// refusal comes before anything moves; `to`'s use by a job is [`move_`]'s check.
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

/// Only a file over a file and a folder into a folder, never a link or special file.
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

/// [`plan_merge`] for folders, recursing into folders both hold; links moved, never followed.
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
    // Each folder merged into, nested ones too, as a replace refuses it.
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

/// (PS5) Refuses a path at or inside ShadowMount+'s mount folder: a mounted game, not files.
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

#[cfg(any(target_os = "freebsd", test))]
type FindImage<'a> = dyn FnMut(&Path, &dyn Fn(&str) -> bool) -> io::Result<Option<PathBuf>> + 'a;

/// ShadowMount+'s part of [`check`] (PS5): nothing in its mount folder, no image it mounted and
/// no folder holding an image of a mounted stem. Fails closed.
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

/// ShadowMount+'s mount points now; a listing error refuses.
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

/// Refuses when ShadowMount+ mounted something since `before`; `done`: "deleted", "moved".
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

#[cfg(any(target_os = "freebsd", test))]
const FIND_MAX: usize = 200_000;

/// The first file under `dir` (links not followed) `wanted` accepts.
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

/// Whether `path` is at, in or above one of `guarded`, by identity (links on the typed path
/// included); a remote path only against that device's guarded paths ([`FlinkPath::within`]).
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
    let mut guarded = guarded.filter(|g| !crate::net::is_network(g));
    let chain = (path.canonicalize().and_then(|real| ids_up(&real))).unwrap_or_default();
    guarded.any(|used| followed_id(used).is_ok_and(|g| chain.contains(&g)) || holds(id, used))
}

/// [`in_use`] for a path not made yet: at, in or above a guarded path as spelled or resolved,
/// or inside one by its nearest existing folder's identity.
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

fn ids_up(real: &Path) -> io::Result<Vec<FileId>> {
    real.ancestors().map(followed_id).collect()
}

/// The first folder under `dir` (links not followed) on another volume.
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

/// Whether `id` is `path` or above it, as written (a link on the way counts) or resolved; a
/// path not made yet is judged by its existing folders.
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

    /// A kind mismatch at any level, or a replaced file a job uses, refuses before anything moves.
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

    /// Nothing a job uses is moved, nor anything around it, nor anything to where a job writes.
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

    /// An image stays with its undo journal; nothing is moved to a journal's name.
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

    /// A mount made between the checks and the change refuses; an unreadable mount folder too.
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

    /// A queued job's output doesn't exist yet: judged through its folders, links included.
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
            assert!(e.contains("never deleted after a job"), "{e}");
        }
    }

    /// NAS roots are never deleted; a NAS path a job uses is busy (SMB case-folded).
    #[test]
    fn nas_roots_and_paths_in_use_are_refused() {
        let none: [&PathBuf; 0] = [];
        for root in ["smb://nas", "smb://nas/share", "nfs://nas", "ftp://nas"] {
            let r = delete(Path::new(root), &[], none);
            assert!(matches!(r, Err(DeleteError::Refused(_))), "{root}: {r:?}");
        }
        let job = [PathBuf::from("smb://NAS/share/Game")];
        for used in ["smb://nas/Share/game/x.bin", "smb://nas/share", "smb://nas"] {
            let r = delete(Path::new(used), &[], &job);
            assert!(matches!(r, Err(DeleteError::Busy(_))), "{used}: {r:?}");
        }
        let r = move_(
            Path::new("smb://nas/share/x"),
            Path::new("smb://nas/share/game/x"),
            false,
            &[],
            &job,
        );
        assert!(matches!(r, Err(DeleteError::Busy(_))), "{r:?}");
    }
}
