//! An in-place edit's tree: the source with its [`Edit`]s folded over it, served as one
//! `SourceTree`. Nothing is copied: a base file is read from the base at its original path, an
//! added one from this computer or a paired device (`flink://`), checked unchanged on every read.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::bail;
use ps5_dump_forge_patch::{Data, Op};
use ps5upload_fpkg::Error;
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::Edit;
use crate::finalize::{FileId, followed_id, handle_id, path_id};
use crate::remote::{FlinkPath, RemoteReader, RemoteTree};
use crate::scan::{JunkFiltered, ScannedFolder, Stamp, ancestors, open_nofollow};

/// Where an edited file's bytes come from.
#[derive(Debug, Clone)]
enum Origin {
    /// The base tree's file at this path.
    Base(String),
    /// A file an `Add` copies, as it was when the edit was resolved.
    Host(HostFile),
}

/// A file's size and where its bytes come from.
type Entry = (u64, Origin);

/// A file an `Add` copies (on this computer, or a `flink://` path on a paired device), as it
/// was when the edit was resolved; a format editor's [`Data::Source`] `id` is its index in
/// [`Edited::hosts`].
#[derive(Debug, Clone)]
pub(crate) struct HostFile {
    pub path: PathBuf,
    pub size: u64,
    pub stamp: Stamp,
    /// Which file the path named (so a file renamed over it later is caught); `None` on a
    /// peer, which has no identity to give: its size and mtime are checked.
    pub id: Option<FileId>,
}

impl HostFile {
    /// Opens the file for reading; [`Opened::same`] then says whether it is still this one.
    pub(crate) fn open(&self) -> io::Result<Opened> {
        if self.id.is_some() {
            return open_nofollow(&self.path).map(Opened::Local);
        }
        let at = FlinkPath::parse(&self.path).map_err(io::Error::other)?;
        let reader =
            RemoteReader::open(crate::remote::get_for(&at)?, &at).map_err(io::Error::other)?;
        Ok(Opened::Peer(reader))
    }
}

/// A [`HostFile`] open for reading: a local file by handle, a peer's through its reader.
pub(crate) enum Opened {
    Local(File),
    Peer(RemoteReader),
}

impl Opened {
    /// Whether this is still `host` as the edit was resolved with it: the path names the
    /// handle's file (local), and its size and modification time are unchanged.
    pub(crate) fn same(&self, host: &HostFile) -> io::Result<bool> {
        match self {
            Opened::Local(file) => {
                let named = Some(path_id(&host.path)?.0) == host.id;
                let meta = file.metadata()?;
                let held = Some(handle_id(file)?) == host.id;
                Ok(named && held && meta.is_file() && Stamp::of(&meta) == host.stamp)
            }
            Opened::Peer(reader) => Ok(reader
                .stamp_now()?
                .is_some_and(|s| !s.dir && Stamp::new(s.size, s.mtime) == host.stamp)),
        }
    }
}

impl Read for Opened {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Opened::Local(file) => file.read(buf),
            Opened::Peer(reader) => reader.read(buf),
        }
    }
}

impl Seek for Opened {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match self {
            Opened::Local(file) => file.seek(to),
            Opened::Peer(reader) => reader.seek(to),
        }
    }
}

/// The tree while the edits fold over it: every file by game path, and every directory (each
/// file's and each directory's parents included); and the editor's ops so far.
#[derive(Default)]
struct State {
    files: BTreeMap<String, Entry>,
    dirs: BTreeSet<String>,
    ops: Vec<Op>,
    hosts: Vec<HostFile>,
}

/// The base tree with the edits applied.
pub(crate) struct Edited {
    base: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    origins: Vec<Origin>,
    empty_dirs: Vec<String>,
    edits: usize,
    /// The edits as a format editor's ops, in order (a folder `Add` expanded into `Mkdir`s and
    /// `Put`s), and the host files their `Data::Source` ids name.
    ops: Vec<Op>,
    hosts: Vec<HostFile>,
    /// The last host file read, kept open: writers read one file in many chunks.
    open: Option<(usize, Opened)>,
}

impl Edited {
    /// Folds `edits` over `base` in order. `source` is the source folder (`None` for an image):
    /// an `Add` from inside it is refused. The inner `Err` lists every offender; the outer one
    /// is a cancel.
    pub(crate) fn new(
        base: Box<dyn SourceTree>,
        edits: &[Edit],
        source: Option<&Path>,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Result<Edited, Vec<String>>> {
        let mut state = State::default();
        for f in base.files() {
            state.add_file(&f.path, f.size, Origin::Base(f.path.clone()));
        }
        for d in base.empty_dirs() {
            state.add_dir(d);
        }
        let source = source.and_then(|s| followed_id(s).ok());
        let mut findings = Vec::new();
        for edit in edits {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let why = state.apply(edit, source, cancel)?;
            findings.extend(why.into_iter().map(|w| format!("{}: {w}", label(edit))));
        }
        if !findings.is_empty() {
            return Ok(Err(findings));
        }
        let busy: HashSet<&str> = state
            .files
            .keys()
            .flat_map(|f| ancestors(f))
            .chain(state.dirs.iter().flat_map(|d| ancestors(d)))
            .collect();
        let empty_dirs = state
            .dirs
            .iter()
            .filter(|d| !busy.contains(d.as_str()))
            .cloned()
            .collect();
        let (files, origins) = state
            .files
            .into_iter()
            .map(|(path, (size, origin))| (SourceFile { path, size }, origin))
            .unzip();
        Ok(Ok(Edited {
            base,
            files,
            origins,
            empty_dirs,
            edits: edits.len(),
            ops: state.ops,
            hosts: state.hosts,
            open: None,
        }))
    }
}

impl Edited {
    /// The edits as a format editor's ops, in order.
    pub(crate) fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// The host files the ops' `Data::Source` ids name (by index).
    pub(crate) fn hosts(&self) -> &[HostFile] {
        &self.hosts
    }

    fn index(&self, path: &str) -> ps5upload_fpkg::Result<usize> {
        self.files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .map_err(|_| Error::Format(format!("{path} is not in the edited game")))
    }

    /// Host file `i`, open and still exactly as it was when the edit was resolved.
    fn host(&mut self, i: usize) -> ps5upload_fpkg::Result<&mut Opened> {
        let Origin::Host(host) = &self.origins[i] else {
            unreachable!("only called for host files");
        };
        let path = &self.files[i].path;
        let with_path = |e: io::Error| {
            Error::Io(io::Error::new(
                e.kind(),
                format!("{} ({path}): {e}", host.path.display()),
            ))
        };
        // A peer checks its stamp when opened; each fetch checks it again. A local path and
        // handle must still name the stamped file on every read.
        let opening = self.open.as_ref().is_none_or(|(at, _)| *at != i);
        if opening {
            self.open = None;
            self.open = Some((i, host.open().map_err(with_path)?));
        }
        let file = &mut self.open.as_mut().expect("opened above").1;
        if (opening || matches!(&*file, Opened::Local(_))) && !file.same(host).map_err(with_path)? {
            return Err(Error::Format(format!(
                "{} ({path}) changed after it was added (replaced, or its size or modification time); \
                 add it again",
                host.path.display()
            )));
        }
        Ok(file)
    }
}

impl SourceTree for Edited {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let i = self.index(path)?;
        if let Origin::Base(at) = &self.origins[i] {
            return self.base.read(at);
        }
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
        if let Origin::Base(at) = &self.origins[i] {
            return self.base.read_range(at, offset, len);
        }
        let size = self.files[i].size;
        let file = self.host(i)?;
        // The allocation is bounded by the recorded size, never by the caller's `len` alone.
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
        format!("{} with {} edits", self.base.describe(), self.edits)
    }
}

/// One edit, as a finding names it.
fn label(edit: &Edit) -> String {
    match edit {
        Edit::Add { from, to } => format!("add {} as {to}", from.display()),
        Edit::Delete { path } => format!("delete {path}"),
        Edit::Rename { from, to } => format!("rename {from} to {to}"),
        Edit::Mkdir { path } => format!("new folder {path}"),
    }
}

/// Relative, `/`-separated, no empty, `.` or `..` component, no `\` (and no NUL).
fn valid(path: &str) -> bool {
    !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// `rel` under game folder `dir`.
fn join(dir: &str, rel: &str) -> String {
    format!("{dir}/{rel}")
}

impl State {
    fn add_dir(&mut self, dir: &str) {
        self.dirs.extend(ancestors(dir).map(str::to_string));
        self.dirs.insert(dir.to_string());
    }

    fn add_file(&mut self, path: &str, size: u64, origin: Origin) {
        self.dirs.extend(ancestors(path).map(str::to_string));
        self.files.insert(path.to_string(), (size, origin));
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path) || self.dirs.contains(path)
    }

    /// Why `path` cannot be a folder or hold a new entry: it or a parent is a file.
    fn blocked(&self, path: &str, itself: bool) -> Option<String> {
        let mut at = ancestors(path).chain(itself.then_some(path));
        at.find(|a| self.files.contains_key(*a))
            .map(|a| format!("{a} is a file"))
    }

    /// Everything under folder `dir`, taken out of the tree (`dir` itself too).
    fn take_dir(&mut self, dir: &str) -> (Vec<(String, Entry)>, Vec<String>) {
        let pre = join(dir, "");
        let under = |p: &String| p.starts_with(&pre);
        let keys: Vec<String> = self.files.keys().filter(|k| under(k)).cloned().collect();
        let files = keys
            .into_iter()
            .map(|k| {
                let v = self.files.remove(&k).expect("listed above");
                (k[pre.len()..].to_string(), v)
            })
            .collect();
        let dirs: Vec<String> = self.dirs.iter().filter(|d| under(d)).cloned().collect();
        self.dirs.retain(|d| d != dir && !under(d));
        let dirs = dirs
            .into_iter()
            .map(|d| d[pre.len()..].to_string())
            .collect();
        (files, dirs)
    }

    /// Applies one edit; the reasons it does not apply, if any (then nothing changed).
    fn apply(
        &mut self,
        edit: &Edit,
        source: Option<FileId>,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Vec<String>> {
        let paths: Vec<&str> = match edit {
            Edit::Add { to, .. } => vec![to],
            Edit::Delete { path } | Edit::Mkdir { path } => vec![path],
            Edit::Rename { from, to } => vec![from, to],
        };
        let bad: Vec<String> = paths
            .into_iter()
            .filter(|p| !valid(p))
            .map(|p| format!("{p:?} is not a valid game path"))
            .collect();
        if !bad.is_empty() {
            return Ok(bad);
        }
        let why = match edit {
            Edit::Delete { path } => {
                if self.files.remove(path).is_none() {
                    if !self.dirs.contains(path) {
                        return Ok(vec![format!("{path} is not in the game")]);
                    }
                    self.take_dir(path);
                }
                self.ops.push(Op::Delete { path: path.clone() });
                None
            }
            Edit::Mkdir { path } => {
                let why = self.blocked(path, true);
                if why.is_none() {
                    self.add_dir(path);
                    self.ops.push(Op::Mkdir { path: path.clone() });
                }
                why
            }
            Edit::Rename { from, to } => {
                let why = self.rename(from, to);
                if why.is_none() {
                    self.ops.push(Op::Rename {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
                why
            }
            Edit::Add { from, to } => return self.add(from, to, source, cancel),
        };
        Ok(why.into_iter().collect())
    }

    fn rename(&mut self, from: &str, to: &str) -> Option<String> {
        if !self.exists(from) {
            return Some(format!("{from} is not in the game"));
        }
        if self.exists(to) {
            return Some(format!("{to} already exists"));
        }
        if let Some(why) = self.blocked(to, false) {
            return Some(why);
        }
        if let Some(file) = self.files.remove(from) {
            self.add_file(to, file.0, file.1);
            return None;
        }
        if to.starts_with(&join(from, "")) {
            return Some(format!("{from} cannot move into itself"));
        }
        let (files, dirs) = self.take_dir(from);
        self.add_dir(to);
        for d in dirs {
            self.add_dir(&join(to, &d));
        }
        for (rel, (size, origin)) in files {
            self.add_file(&join(to, &rel), size, origin);
        }
        None
    }

    fn add(
        &mut self,
        from: &Path,
        to: &str,
        source: Option<FileId>,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Vec<String>> {
        if crate::remote::is_flink(from) {
            return self.add_peer(from, to, cancel);
        }
        if crate::remote::is_remote(from) {
            return Ok(vec![format!(
                "{}: files are added from this device or a paired one, not a NAS",
                from.display()
            )]);
        }
        let shown = from.display();
        // `link/`, `link/.` or `link/..` would follow a link at `link`.
        let raw = from.as_os_str().as_encoded_bytes();
        let last = raw
            .rsplit(|&b| std::path::is_separator(b as char))
            .next()
            .unwrap_or(raw);
        if matches!(last, b"" | b"." | b"..") {
            return Ok(vec![format!(
                "{shown} does not end in a file or folder name"
            )]);
        }
        let meta = match std::fs::symlink_metadata(from) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(vec![format!("{shown} does not exist")]);
            }
            Err(e) => return Ok(vec![format!("{shown}: {e}")]),
        };
        if meta.file_type().is_symlink() {
            return Ok(vec![format!("{shown} is a link (not followed)")]);
        }
        if !meta.is_file() && !meta.is_dir() {
            return Ok(vec![format!("{shown} is not a file or folder")]);
        }
        // By identity, not spelling: a case-insensitive volume's other spelling is caught too.
        let inside = |src: FileId| {
            from.canonicalize().is_ok_and(|f| {
                f.ancestors()
                    .any(|a| followed_id(a).is_ok_and(|id| id == src))
            })
        };
        if source.is_some_and(inside) {
            return Ok(vec![format!("{shown} is inside the game folder")]);
        }
        if let Some(why) = self.blocked(to, false) {
            return Ok(vec![why]);
        }
        if meta.is_file() {
            if self.dirs.contains(to) {
                return Ok(vec![format!("{to} is a folder")]);
            }
            let id = match path_id(from) {
                Ok((id, _)) => id,
                Err(e) => return Ok(vec![format!("{shown}: {e}")]),
            };
            let host = HostFile {
                path: from.to_path_buf(),
                size: meta.len(),
                stamp: Stamp::of(&meta),
                id: Some(id),
            };
            self.put(to, host);
            return Ok(Vec::new());
        }
        if self.files.contains_key(to) {
            return Ok(vec![format!("{to} is a file")]);
        }
        let folder = match ScannedFolder::scan(from, cancel) {
            Ok(folder) => folder,
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) => return Ok(vec![format!("{e:#}")]),
        };
        let mut why = Vec::new();
        let mut hosts = Vec::with_capacity(folder.files().len());
        for (f, stamp) in folder.files().iter().zip(folder.stamps()) {
            let mut host = folder.root().to_path_buf();
            host.extend(f.path.split('/'));
            match path_id(&host) {
                Ok((id, _)) => hosts.push(HostFile {
                    path: host,
                    size: f.size,
                    stamp: *stamp,
                    id: Some(id),
                }),
                Err(e) => why.push(format!("{}: {e}", host.display())),
            }
        }
        Ok(self.add_folder(to, folder.files(), folder.empty_dirs(), hosts, why))
    }

    /// [`Self::add`] of a `flink://` path: a file or folder on a paired device, as it lists
    /// it (links refused, junk left out as a conversion leaves it out).
    fn add_peer(
        &mut self,
        from: &Path,
        to: &str,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Vec<String>> {
        let shown = from.display();
        let at = match FlinkPath::parse(from) {
            Ok(at) => at,
            Err(e) => return Ok(vec![format!("{e:#}")]),
        };
        let remote = match crate::remote::get_for(&at) {
            Ok(remote) => remote,
            Err(e) => return Ok(vec![format!("{shown}: {e}")]),
        };
        let entry = match crate::remote::entry(&*remote, &at) {
            Ok(entry) => entry,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(vec![format!("{shown} does not exist")]);
            }
            Err(e) => return Ok(vec![format!("{shown}: {e}")]),
        };
        if entry.link {
            return Ok(vec![format!("{shown} is a link (not followed)")]);
        }
        if entry.special {
            return Ok(vec![format!("{shown} is not a file or folder")]);
        }
        if entry.bad_name {
            return Ok(vec![format!("{shown}: name is not valid UTF-8")]);
        }
        if let Some(why) = self.blocked(to, false) {
            return Ok(vec![why]);
        }
        if !entry.dir {
            if self.dirs.contains(to) {
                return Ok(vec![format!("{to} is a folder")]);
            }
            let host = HostFile {
                path: at.url(),
                size: entry.size,
                stamp: Stamp::new(entry.size, entry.mtime),
                id: None,
            };
            self.put(to, host);
            return Ok(Vec::new());
        }
        if self.files.contains_key(to) {
            return Ok(vec![format!("{to} is a file")]);
        }
        let tree = match RemoteTree::open(remote, &at, cancel) {
            Ok(tree) => tree,
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) => return Ok(vec![format!("{e:#}")]),
        };
        let stamps: HashMap<String, Stamp> = (tree.files().iter())
            .filter_map(|f| {
                let s = tree.stamp(&f.path)?;
                Some((f.path.clone(), Stamp::new(s.size, s.mtime)))
            })
            .collect();
        let folder = JunkFiltered::new(Box::new(tree));
        let hosts = (folder.files().iter())
            .map(|f| HostFile {
                path: (f.path.split('/').fold(at.clone(), |a, p| a.join(p))).url(),
                size: f.size,
                stamp: stamps[&f.path],
                id: None,
            })
            .collect();
        Ok(self.add_folder(to, folder.files(), folder.empty_dirs(), hosts, Vec::new()))
    }

    /// Adds a folder's `files` (`hosts` holds each one's origin, in order) and `empty_dirs`
    /// at `to`, unless `why` (what was found already) or the tree refuses them.
    fn add_folder(
        &mut self,
        to: &str,
        files: &[SourceFile],
        empty_dirs: &[String],
        hosts: Vec<HostFile>,
        mut why: Vec<String>,
    ) -> Vec<String> {
        let targets: Vec<String> = files.iter().map(|f| join(to, &f.path)).collect();
        let empty: Vec<String> = empty_dirs.iter().map(|d| join(to, d)).collect();
        why.extend(
            targets
                .iter()
                .filter(|t| self.dirs.contains(*t))
                .map(|t| format!("{t} is a folder")),
        );
        why.extend(
            targets
                .iter()
                .chain(&empty)
                .filter_map(|t| self.blocked(t, false)),
        );
        why.extend(empty.iter().filter_map(|d| self.blocked(d, true)));
        why.sort();
        why.dedup();
        if !why.is_empty() {
            return why;
        }
        for d in std::iter::once(to).chain(empty.iter().map(String::as_str)) {
            self.add_dir(d);
            self.ops.push(Op::Mkdir { path: d.into() });
        }
        for (target, host) in targets.iter().zip(hosts) {
            self.put(target, host);
        }
        Vec::new()
    }

    /// Adds `host` at `path`, and its `Put`.
    fn put(&mut self, path: &str, host: HostFile) {
        let size = host.size;
        let source = u32::try_from(self.hosts.len()).expect("fewer than 2^32 host files");
        self.hosts.push(host.clone());
        self.ops.push(Op::Put {
            path: path.into(),
            len: size,
            data: Data::Source {
                id: source,
                offset: 0,
                len: size,
            },
        });
        self.add_file(path, size, Origin::Host(host));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A base whose every file holds its own path.
    struct Mem(Vec<SourceFile>, Vec<String>);

    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            assert!(self.0.iter().any(|f| f.path == path), "{path}");
            Ok(path.as_bytes().to_vec())
        }
        fn empty_dirs(&self) -> &[String] {
            &self.1
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    fn base() -> Box<dyn SourceTree> {
        let paths = [
            "data/a.bin",
            "data/sub/b.bin",
            "eboot.bin",
            "sce_sys/param.json",
        ];
        let files = paths
            .iter()
            .map(|p| SourceFile {
                path: p.to_string(),
                size: p.len() as u64,
            })
            .collect();
        Box::new(Mem(files, vec!["empty".into()]))
    }

    fn edited(edits: &[Edit], source: Option<&Path>) -> Result<Edited, Vec<String>> {
        Edited::new(base(), edits, source, &AtomicBool::new(false)).unwrap()
    }

    fn paths(tree: &Edited) -> Vec<&str> {
        tree.files().iter().map(|f| f.path.as_str()).collect()
    }

    fn delete(path: &str) -> Edit {
        Edit::Delete { path: path.into() }
    }

    fn rename(from: &str, to: &str) -> Edit {
        Edit::Rename {
            from: from.into(),
            to: to.into(),
        }
    }

    fn mkdir(path: &str) -> Edit {
        Edit::Mkdir { path: path.into() }
    }

    fn add(from: &Path, to: &str) -> Edit {
        Edit::Add {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn delete_rename_and_mkdir_fold_in_order() {
        let mut tree = edited(
            &[
                rename("data/a.bin", "data/c.bin"),
                delete("eboot.bin"),
                mkdir("x/y"),
                rename("data", "stuff"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            paths(&tree),
            ["sce_sys/param.json", "stuff/c.bin", "stuff/sub/b.bin"]
        );
        assert_eq!(tree.empty_dirs(), ["empty", "x/y"]);
        assert_eq!(tree.read("stuff/c.bin").unwrap(), b"data/a.bin");
        assert_eq!(tree.read_range("stuff/sub/b.bin", 5, 3).unwrap(), b"sub");
        assert!(tree.read("data/a.bin").is_err());
        assert_eq!(tree.describe(), "mem with 4 edits");
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_host_file_renamed_over_after_a_read_fails_the_next_read() {
        let dir = crate::test_dir("edit-replaced");
        let x = put(&dir, "x.bin", b"before");
        let mut tree = edited(&[add(&x, "x.bin")], None).unwrap();
        assert_eq!(tree.read("x.bin").unwrap(), b"before");
        // Same length and modification time: only the file's identity tells them apart.
        let other = put(&dir, "other.bin", b"after!");
        let mtime = std::fs::metadata(&x).unwrap().modified().unwrap();
        File::options()
            .write(true)
            .open(&other)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        std::fs::rename(&other, &x).unwrap();
        let err = tree.read_range("x.bin", 0, 6).unwrap_err().to_string();
        assert!(err.contains("changed after it was added"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_link_inside_an_added_folder_is_a_finding() {
        let dir = crate::test_dir("edit-folder-link");
        let host = dir.join("host");
        let a = put(&host, "a.bin", b"a");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&a, host.join("l.bin")).unwrap();
        #[cfg(not(unix))]
        drop(a);
        let result = edited(&[add(&host, "data")], None);
        #[cfg(unix)]
        {
            let findings = result.err().unwrap();
            assert!(
                findings[0].contains("symlink (not followed): l.bin"),
                "{findings:?}"
            );
        }
        #[cfg(not(unix))]
        assert!(result.is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
