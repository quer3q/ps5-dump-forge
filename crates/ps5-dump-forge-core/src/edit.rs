//! An in-place edit's tree: the source with its [`Edit`]s folded over it, served as one
//! `SourceTree`. Nothing is copied: a base file is read from the base at its original path, an
//! added one from this computer, checked unchanged on every read.

use std::collections::{BTreeMap, BTreeSet, HashSet};
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
use crate::scan::{ScannedFolder, Stamp, ancestors, open_nofollow};

/// Where an edited file's bytes come from.
#[derive(Debug, Clone)]
enum Origin {
    /// The base tree's file at this path.
    Base(String),
    /// A file on this computer, as it was when the edit was resolved: its stamp, and which
    /// file the path named (so a file renamed over it later is caught).
    Host(PathBuf, Stamp, FileId),
}

/// A file's size and where its bytes come from.
type Entry = (u64, Origin);

/// A file on this computer an `Add` copies, as it was when the edit was resolved; a format
/// editor's [`Data::Source`] `id` is its index in [`Edited::hosts`].
#[derive(Debug, Clone)]
pub(crate) struct HostFile {
    pub path: PathBuf,
    pub size: u64,
    pub stamp: Stamp,
    pub id: FileId,
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
    open: Option<(usize, File)>,
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
    fn host(&mut self, i: usize) -> ps5upload_fpkg::Result<&mut File> {
        let Origin::Host(host, stamp, id) = &self.origins[i] else {
            unreachable!("only called for host files");
        };
        let path = &self.files[i].path;
        let with_path = |e: io::Error| {
            Error::Io(io::Error::new(
                e.kind(),
                format!("{} ({path}): {e}", host.display()),
            ))
        };
        // The path must still name the stamped file, and the handle (kept open across reads)
        // must be that file: a file renamed over the path is caught on the next read.
        let named = path_id(host).map_err(with_path)?.0 == *id;
        if self.open.as_ref().is_none_or(|(at, _)| *at != i) {
            self.open = None;
            self.open = Some((i, open_nofollow(host).map_err(with_path)?));
        }
        let file = &mut self.open.as_mut().expect("opened above").1;
        let meta = file.metadata().map_err(with_path)?;
        let held = handle_id(file).map_err(with_path)? == *id;
        if !named || !held || !meta.is_file() || Stamp::of(&meta) != *stamp {
            return Err(Error::Format(format!(
                "{} ({path}) changed after it was added (replaced, or its size or modification time); \
                 add it again",
                host.display()
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
            let origin = Origin::Host(from.to_path_buf(), Stamp::of(&meta), id);
            self.put(to, meta.len(), origin);
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
        let targets: Vec<String> = folder.files().iter().map(|f| join(to, &f.path)).collect();
        let empty: Vec<String> = folder.empty_dirs().iter().map(|d| join(to, d)).collect();
        let mut why: Vec<String> = targets
            .iter()
            .filter(|t| self.dirs.contains(*t))
            .map(|t| format!("{t} is a folder"))
            .collect();
        why.extend(
            targets
                .iter()
                .chain(&empty)
                .filter_map(|t| self.blocked(t, false)),
        );
        why.extend(empty.iter().filter_map(|d| self.blocked(d, true)));
        let mut hosts = Vec::with_capacity(targets.len());
        for (f, stamp) in folder.files().iter().zip(folder.stamps()) {
            let mut host = folder.root().to_path_buf();
            host.extend(f.path.split('/'));
            match path_id(&host) {
                Ok((id, _)) => hosts.push(Origin::Host(host, *stamp, id)),
                Err(e) => why.push(format!("{}: {e}", host.display())),
            }
        }
        why.sort();
        why.dedup();
        if !why.is_empty() {
            return Ok(why);
        }
        for d in std::iter::once(to).chain(empty.iter().map(String::as_str)) {
            self.add_dir(d);
            self.ops.push(Op::Mkdir { path: d.into() });
        }
        for ((target, f), origin) in targets.iter().zip(folder.files()).zip(hosts) {
            self.put(target, f.size, origin);
        }
        Ok(Vec::new())
    }

    /// Adds host file `origin` at `path`, and its `Put`.
    fn put(&mut self, path: &str, size: u64, origin: Origin) {
        let Origin::Host(host, stamp, id) = &origin else {
            unreachable!("only host files are put");
        };
        let source = u32::try_from(self.hosts.len()).expect("fewer than 2^32 host files");
        self.hosts.push(HostFile {
            path: host.clone(),
            size,
            stamp: *stamp,
            id: *id,
        });
        self.ops.push(Op::Put {
            path: path.into(),
            len: size,
            data: Data::Source {
                id: source,
                offset: 0,
                len: size,
            },
        });
        self.add_file(path, size, origin);
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
    fn a_folder_onto_a_folder_merges() {
        let dir = crate::test_dir("edit-add-folder");
        let host = dir.join("host");
        put(&host, "a.bin", b"replaced");
        put(&host, "new.bin", b"new");
        put(&host, "deep/c.bin", b"c");
        put(&host, ".DS_Store", b"junk");
        std::fs::create_dir_all(host.join("e")).unwrap();
        let mut tree = edited(&[add(&host, "data"), add(&host, "copy")], None).unwrap();
        assert_eq!(
            paths(&tree),
            [
                "copy/a.bin",
                "copy/deep/c.bin",
                "copy/new.bin",
                "data/a.bin",
                "data/deep/c.bin",
                "data/new.bin",
                "data/sub/b.bin",
                "eboot.bin",
                "sce_sys/param.json",
            ]
        );
        assert_eq!(tree.empty_dirs(), ["copy/e", "data/e", "empty"]);
        assert_eq!(tree.read("data/a.bin").unwrap(), b"replaced");
        assert_eq!(tree.read("data/sub/b.bin").unwrap(), b"data/sub/b.bin");
        assert_eq!(tree.read("copy/deep/c.bin").unwrap(), b"c");
        std::fs::remove_dir_all(dir).unwrap();
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
    fn every_offender_is_listed() {
        let dir = crate::test_dir("edit-refusals");
        let game = dir.join("game");
        let inside = put(&game, "sce_sys/icon0.png", b"png");
        let file = put(&dir, "f.bin", b"f");
        let folder = dir.join("folder");
        put(&folder, "a.bin", b"a");
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut edits = vec![
            delete(""),
            delete("/data"),
            mkdir("a//b"),
            mkdir("./a"),
            rename("data/..", "x"),
            rename("data", "a\\b"),
            delete("missing"),
            rename("missing", "x"),
            rename("data", "eboot.bin"),
            rename("data", "data/sub/inner"),
            add(&file, "data"),
            add(&folder, "eboot.bin"),
            mkdir("eboot.bin/x"),
            add(&file, "eboot.bin/x"),
            add(&dir.join("nope.bin"), "nope.bin"),
            add(&inside, "icon.png"),
            add(&game, "game"),
        ];
        #[cfg(unix)]
        {
            let link = dir.join("link.bin");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            edits.push(add(&link, "link.bin"));
        }
        let findings = edited(&edits, Some(&game)).err().unwrap();
        let wanted = [
            "delete : \"\" is not a valid game path",
            "delete /data: \"/data\" is not a valid game path",
            "new folder a//b: \"a//b\" is not a valid game path",
            "new folder ./a: \"./a\" is not a valid game path",
            "rename data/.. to x: \"data/..\" is not a valid game path",
            "rename data to a\\b: \"a\\\\b\" is not a valid game path",
            "delete missing: missing is not in the game",
            "rename missing to x: missing is not in the game",
            "rename data to eboot.bin: eboot.bin already exists",
            "rename data to data/sub/inner: data cannot move into itself",
            "data is a folder",
            "eboot.bin is a file",
            "new folder eboot.bin/x: eboot.bin is a file",
            "as eboot.bin/x: eboot.bin is a file",
            "nope.bin does not exist",
            "icon0.png is inside the game folder",
            "game is inside the game folder",
        ];
        for (finding, want) in findings.iter().zip(wanted) {
            assert!(finding.ends_with(want), "{finding:?} vs {want:?}");
        }
        #[cfg(unix)]
        assert!(findings[wanted.len()].ends_with("link.bin is a link (not followed)"));
        assert_eq!(findings.len(), edits.len(), "{findings:#?}");
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
