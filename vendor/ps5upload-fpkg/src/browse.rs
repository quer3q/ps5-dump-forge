//! Looking inside a game image or debug package on this computer (#479): what is in one folder
//! of it, one folder of it as a tree of its own (to save it to the computer), and the image's
//! tree with files added, replaced, renamed or deleted and folders made (to write an edited
//! copy).
//!
//! Nothing here writes to an image. An edit is a tree that a builder writes out as a NEW image,
//! read back and checked like any Convert output; the image it came from is only ever read.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::source::{SourceFile, SourceTree};
use crate::{format_err, Error, Result};

/// The filesystem an image holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    Exfat,
    /// `.ffpkg`: UFS2.
    Ufs2,
    /// `.ffpfs`: PFS.
    Pfs,
    /// A debug-key `.pkg`.
    Pkg,
}

impl Filesystem {
    pub fn name(self) -> &'static str {
        match self {
            Self::Exfat => "exfat",
            Self::Ufs2 => "ufs2",
            Self::Pfs => "pfs",
            Self::Pkg => "pkg",
        }
    }

    /// An edited copy can be written: an image writer exists for it. A package is not rebuilt
    /// from here (that is Convert's job, with its own checks), so it is read-only.
    pub fn editable(self) -> bool {
        !matches!(self, Self::Pkg)
    }
}

/// An image opened for browsing.
pub struct Opened {
    pub tree: Box<dyn SourceTree>,
    pub filesystem: Filesystem,
    /// A `.ffpfsc`: the image is inside a compressed PFS container.
    pub compressed: bool,
}

/// Opens a game image or debug package on this computer, by its extension (as
/// [`crate::source::open`] does), saying what it holds.
pub fn open(path: &Path) -> Result<Opened> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let plain = |filesystem| -> Result<Opened> {
        Ok(Opened {
            tree: crate::source::open(path)?,
            filesystem,
            compressed: false,
        })
    };
    match ext.as_str() {
        "exfat" => plain(Filesystem::Exfat),
        "ffpkg" | "ufs2" => plain(Filesystem::Ufs2),
        "ffpfs" => plain(Filesystem::Pfs),
        "pkg" => plain(Filesystem::Pkg),
        "ffpfsc" => {
            let open =
                || -> Result<Box<dyn crate::ReadSeek>> { Ok(Box::new(std::fs::File::open(path)?)) };
            let (tree, filesystem) =
                crate::source::ffpfsc_tree_of(&open, &format!("ffpfsc {}", path.display()))?;
            Ok(Opened {
                tree,
                filesystem,
                compressed: true,
            })
        }
        _ => format_err(format!(
            "{} is not a game image (.exfat, .ffpkg, .ffpfs, .ffpfsc) or debug package (.pkg)",
            path.display()
        )),
    }
}

/// A path inside an image as the tree spells it: `/`-separated, no leading or trailing `/`,
/// the root as "". `None` for one that names no place in a tree (`..`, `.`, a `\`, a NUL).
pub fn normalize(raw: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." || part.contains('\\') || part.contains('\0') {
            return None;
        }
        parts.push(part);
    }
    Some(parts.join("/"))
}

/// The folder `path` is in ("" at the root).
fn parent(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// `path` and every folder above it, innermost first.
fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut at = Some(path);
    std::iter::from_fn(move || {
        let here = at?;
        at = here.rfind('/').map(|i| &here[..i]);
        Some(here)
    })
}

/// One thing in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    /// A file's size; a folder's is the sum of every file under it.
    pub size: u64,
    /// Files under a folder (1 for a file).
    pub files: u64,
}

/// What a path in an image is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File(u64),
    Dir,
}

/// An image's file list, kept to answer folder listings without the image open.
#[derive(Debug, Clone, Default)]
pub struct Index {
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    dirs: BTreeSet<String>,
}

impl Index {
    pub fn new(tree: &dyn SourceTree) -> Self {
        let files = tree.files().to_vec();
        let empty_dirs: Vec<String> = tree
            .empty_dirs()
            .iter()
            .filter_map(|d| normalize(d))
            .filter(|d| !d.is_empty())
            .collect();
        let mut dirs = BTreeSet::new();
        for f in &files {
            for a in ancestors(parent(&f.path)).filter(|a| !a.is_empty()) {
                if !dirs.insert(a.to_string()) {
                    break;
                }
            }
        }
        for d in &empty_dirs {
            for a in ancestors(d) {
                dirs.insert(a.to_string());
            }
        }
        Self {
            files,
            empty_dirs,
            dirs,
        }
    }

    pub fn file_count(&self) -> u64 {
        self.files.len() as u64
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// What `path` (normalized) is, or `None` when the image has nothing there. The root is a
    /// folder.
    pub fn kind(&self, path: &str) -> Option<Kind> {
        if path.is_empty() || self.dirs.contains(path) {
            return Some(Kind::Dir);
        }
        self.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| Kind::File(f.size))
    }

    /// What folder `dir` (normalized) holds, folders first, each group by name ignoring case.
    /// `None` when there is no such folder.
    pub fn list(&self, dir: &str) -> Option<Vec<Entry>> {
        if self.kind(dir) != Some(Kind::Dir) {
            return None;
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut folders: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
        let mut files = Vec::new();
        for f in &self.files {
            let Some(rest) = f.path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((name, _)) => {
                    let e = folders.entry(name).or_default();
                    e.0 += f.size;
                    e.1 += 1;
                }
                None => files.push(Entry {
                    name: rest.to_string(),
                    is_dir: false,
                    size: f.size,
                    files: 1,
                }),
            }
        }
        for d in &self.empty_dirs {
            if let Some(rest) = d.strip_prefix(&prefix) {
                let name = rest.split('/').next().unwrap_or(rest);
                folders.entry(name).or_default();
            }
        }
        let mut out: Vec<Entry> = folders
            .into_iter()
            .map(|(name, (size, files))| Entry {
                name: name.to_string(),
                is_dir: true,
                size,
                files,
            })
            .collect();
        let by_name = |a: &Entry, b: &Entry| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.name.cmp(&b.name))
        };
        out.sort_by(by_name);
        files.sort_by(by_name);
        out.extend(files);
        Some(out)
    }
}

/// One folder of a tree as a tree of its own: its files with the folder's path taken off, so
/// saving it writes the folder's contents and nothing above it.
pub struct Subtree {
    inner: Box<dyn SourceTree>,
    prefix: String,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
}

impl Subtree {
    /// The folder `dir` (normalized; "" is the whole tree) of `inner`. An error when `inner`
    /// has no such folder.
    pub fn new(inner: Box<dyn SourceTree>, dir: &str) -> Result<Self> {
        let index = Index::new(&*inner);
        if index.kind(dir) != Some(Kind::Dir) {
            return format_err(format!("{dir} is not a folder in {}", inner.describe()));
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let files = inner
            .files()
            .iter()
            .filter_map(|f| {
                f.path.strip_prefix(&prefix).map(|p| SourceFile {
                    path: p.to_string(),
                    size: f.size,
                })
            })
            .collect();
        let empty_dirs = index
            .empty_dirs
            .iter()
            .filter_map(|d| d.strip_prefix(&prefix).map(str::to_string))
            .collect();
        Ok(Self {
            inner,
            prefix,
            files,
            empty_dirs,
        })
    }
}

impl SourceTree for Subtree {
    fn release(&mut self) {
        self.inner.release();
    }

    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let full = format!("{}{path}", self.prefix);
        self.inner.read(&full)
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let full = format!("{}{path}", self.prefix);
        self.inner.read_range(&full, offset, len)
    }

    fn describe(&self) -> String {
        match self.prefix.strip_suffix('/') {
            Some(dir) => format!("{dir} of {}", self.inner.describe()),
            None => self.inner.describe(),
        }
    }

    fn notes(&self) -> Vec<String> {
        self.inner.notes()
    }
}

/// One change to an image's tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Remove the file or the whole folder at `path`. The folder it was in stays, empty or not.
    Delete { path: String },
    /// Put the file `from` (on this computer) at `path`, replacing a file there; or, when
    /// `from` is a folder, everything in it under the folder `path`, merged with what is there.
    Put { path: String, from: PathBuf },
    /// Rename or move the file or folder `from` to `to`, which must not be there yet. A folder
    /// takes everything in it along; the folder it left stays, empty or not. (The same four
    /// changes as ps5-dump-forge's `Op` in `crates/ps5-dump-forge-patch/src/lib.rs`; there they
    /// patch an image in place, here they shape the tree a new image is written from.)
    Rename { from: String, to: String },
    /// A new empty folder at `path`, kept in the copy; the folders above it come with it.
    Mkdir { path: String },
}

/// Where a file of an edited tree comes from.
#[derive(Debug, Clone)]
enum Origin {
    Image,
    /// The image's file at this path, renamed.
    Moved(String),
    Local(PathBuf),
}

/// Why `name` (one part of a path a rename or a new folder asks for) cannot be stored in an
/// image holding `filesystem`, checked here so a bad name is refused before anything is
/// written; the image writers check every name again. `None` (the kind of image is not
/// known) applies only what every writer refuses.
fn storable_name(name: &str, filesystem: Option<Filesystem>) -> Result<()> {
    if filesystem == Some(Filesystem::Exfat) {
        return crate::exfat_write::check_name(name);
    }
    // UFS2 and PFS: a directory entry's name is at most 255 bytes.
    if name.len() > 255 {
        return format_err(format!(
            "\"{name}\" cannot be stored: a name is at most 255 bytes"
        ));
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return format_err(format!(
            "\"{name}\" cannot be stored: a control character in a name"
        ));
    }
    // As the writers refuse a decomposed name from a Mac: the game looks its files up by the
    // composed spelling.
    if !unicode_normalization::is_nfc(name) {
        return format_err(format!(
            "\"{name}\" cannot be stored: the name is decomposed (NFD, usually from a Mac), so \
             the game could not find the file"
        ));
    }
    Ok(())
}

/// An image's tree with [`Edit`]s applied: what an edited copy is written from.
pub struct Edited {
    base: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    local: BTreeMap<String, PathBuf>,
    /// Renamed files that are still the image's own: where each is in the image.
    moved: BTreeMap<String, String>,
    open: Option<(String, std::fs::File)>,
    changes: usize,
}

impl Edited {
    /// `base` with `edits` applied in order. Each edit is checked against the tree as the edits
    /// before it left it: a delete must name something there, a file cannot go where a folder
    /// is (or under a file), a rename or a new folder cannot take a name already there, and
    /// every path must be a plain relative one.
    pub fn new(base: Box<dyn SourceTree>, edits: &[Edit]) -> Result<Self> {
        Self::with_names(base, edits, None)
    }

    /// [`Self::new`] for an image holding `filesystem`: a rename's new name and a new folder's
    /// are also refused when that filesystem cannot hold them.
    pub fn for_filesystem(
        base: Box<dyn SourceTree>,
        edits: &[Edit],
        filesystem: Filesystem,
    ) -> Result<Self> {
        Self::with_names(base, edits, Some(filesystem))
    }

    fn with_names(
        base: Box<dyn SourceTree>,
        edits: &[Edit],
        filesystem: Option<Filesystem>,
    ) -> Result<Self> {
        let mut files: BTreeMap<String, (u64, Origin)> = base
            .files()
            .iter()
            .map(|f| (f.path.clone(), (f.size, Origin::Image)))
            .collect();
        let mut dirs: BTreeSet<String> = base
            .empty_dirs()
            .iter()
            .filter_map(|d| normalize(d))
            .filter(|d| !d.is_empty())
            .collect();
        let under = |prefix: &str, path: &str| {
            path.len() > prefix.len() + 1
                && path.starts_with(prefix)
                && path.as_bytes()[prefix.len()] == b'/'
        };
        // Sorted keys: everything under `p/` follows `p/` directly.
        let is_dir = |files: &BTreeMap<String, (u64, Origin)>, dirs: &BTreeSet<String>, p: &str| {
            let slash = format!("{p}/");
            dirs.contains(p)
                || dirs
                    .range(slash.clone()..)
                    .next()
                    .is_some_and(|d| d.starts_with(&slash))
                || files
                    .range(slash.clone()..)
                    .next()
                    .is_some_and(|(f, _)| f.starts_with(&slash))
        };
        let checked = |raw: &str| -> Result<String> {
            match normalize(raw) {
                Some(p) if !p.is_empty() => Ok(p),
                _ => format_err(format!("{raw:?} is not a path inside the image")),
            }
        };
        // A path that is about to exist: every part a name the image can hold, and no file
        // where one of its folders has to be.
        let placeable = |files: &BTreeMap<String, (u64, Origin)>, p: &str| -> Result<()> {
            for part in p.split('/') {
                storable_name(part, filesystem)?;
            }
            match ancestors(parent(p)).find(|a| !a.is_empty() && files.contains_key(*a)) {
                Some(file) => format_err(format!("{file} is a file in the image")),
                None => Ok(()),
            }
        };
        for edit in edits {
            match edit {
                Edit::Delete { path } => {
                    let path = checked(path)?;
                    if files.remove(&path).is_none() {
                        if !is_dir(&files, &dirs, &path) {
                            return format_err(format!("{path} is not in the image"));
                        }
                        files.retain(|f, _| !under(&path, f));
                        dirs.retain(|d| *d != path && !under(&path, d));
                    }
                    // The folder it was in stays, as on a drive.
                    let up = parent(&path);
                    if !up.is_empty() {
                        dirs.insert(up.to_string());
                    }
                }
                Edit::Put { path, from } => {
                    let path = checked(path)?;
                    let meta = std::fs::metadata(from)
                        .map_err(|e| Error::Format(format!("{}: {e}", from.display())))?;
                    let mut put: Vec<(String, u64, PathBuf)> = Vec::new();
                    let mut put_dirs: Vec<String> = Vec::new();
                    if meta.is_dir() {
                        if files.contains_key(&path) {
                            return format_err(format!("{path} is a file in the image"));
                        }
                        let (found, empty) = crate::source::scan_tree(from)?;
                        if found.is_empty() && empty.is_empty() {
                            put_dirs.push(path.clone());
                        }
                        for f in found {
                            put.push((format!("{path}/{}", f.path), f.size, from.join(&f.path)));
                        }
                        put_dirs.extend(empty.iter().map(|d| format!("{path}/{d}")));
                    } else {
                        if is_dir(&files, &dirs, &path) {
                            return format_err(format!("{path} is a folder in the image"));
                        }
                        put.push((path.clone(), meta.len(), from.clone()));
                    }
                    for (p, _, _) in &put {
                        if normalize(p).as_deref() != Some(p.as_str()) {
                            return format_err(format!("{p:?} is not a path inside the image"));
                        }
                        if let Some(file) =
                            ancestors(parent(p)).find(|a| !a.is_empty() && files.contains_key(*a))
                        {
                            return format_err(format!("{file} is a file in the image"));
                        }
                        if is_dir(&files, &dirs, p) {
                            return format_err(format!("{p} is a folder in the image"));
                        }
                    }
                    for (p, size, local) in put {
                        files.insert(p, (size, Origin::Local(local)));
                    }
                    dirs.extend(put_dirs);
                }
                Edit::Rename { from, to } => {
                    let (from, to) = (checked(from)?, checked(to)?);
                    let file = files.contains_key(&from);
                    if !file && !is_dir(&files, &dirs, &from) {
                        return format_err(format!("{from} is not in the image"));
                    }
                    if under(&from, &to) {
                        return format_err(format!("{from} cannot be moved into itself ({to})"));
                    }
                    if files.contains_key(&to) || is_dir(&files, &dirs, &to) {
                        return format_err(format!("{to} is already in the image"));
                    }
                    placeable(&files, &to)?;
                    // A file that was the image's own remembers where it is in the image; one
                    // put from this computer, or renamed before, keeps what it had.
                    let carry = |old: String, origin: Origin| match origin {
                        Origin::Image => Origin::Moved(old),
                        other => other,
                    };
                    if file {
                        if let Some((size, origin)) = files.remove(&from) {
                            files.insert(to.clone(), (size, carry(from.clone(), origin)));
                        }
                    } else {
                        let inside: Vec<String> = files
                            .range(format!("{from}/")..)
                            .take_while(|(f, _)| under(&from, f))
                            .map(|(f, _)| f.clone())
                            .collect();
                        for old in inside {
                            if let Some((size, origin)) = files.remove(&old) {
                                let new = format!("{to}{}", &old[from.len()..]);
                                files.insert(new, (size, carry(old, origin)));
                            }
                        }
                        let (gone, kept): (Vec<String>, Vec<String>) = std::mem::take(&mut dirs)
                            .into_iter()
                            .partition(|d| *d == from || under(&from, d));
                        dirs = kept.into_iter().collect();
                        dirs.extend(gone.iter().map(|d| format!("{to}{}", &d[from.len()..])));
                        // A folder known only by its files is still a folder once they move.
                        dirs.insert(to.clone());
                    }
                    // The folder it left stays, as on a drive.
                    let up = parent(&from);
                    if !up.is_empty() {
                        dirs.insert(up.to_string());
                    }
                }
                Edit::Mkdir { path } => {
                    let path = checked(path)?;
                    if files.contains_key(&path) || is_dir(&files, &dirs, &path) {
                        return format_err(format!("{path} is already in the image"));
                    }
                    placeable(&files, &path)?;
                    dirs.insert(path);
                }
            }
        }
        // A folder is listed as empty only when nothing at all is under it.
        let empty_dirs: Vec<String> = dirs
            .iter()
            .filter(|d| {
                let slash = format!("{d}/");
                !files
                    .range(slash.clone()..)
                    .next()
                    .is_some_and(|(f, _)| f.starts_with(&slash))
                    && !dirs
                        .range(slash.clone()..)
                        .next()
                        .is_some_and(|o| o.starts_with(&slash))
            })
            .cloned()
            .collect();
        let mut local = BTreeMap::new();
        let mut moved = BTreeMap::new();
        let files = files
            .into_iter()
            .map(|(path, (size, origin))| {
                match origin {
                    Origin::Image => {}
                    Origin::Moved(old) => {
                        moved.insert(path.clone(), old);
                    }
                    Origin::Local(from) => {
                        local.insert(path.clone(), from);
                    }
                }
                SourceFile { path, size }
            })
            .collect();
        Ok(Self {
            base,
            files,
            empty_dirs,
            local,
            moved,
            open: None,
            changes: edits.len(),
        })
    }

    /// The files whose bytes or place the edits change: those put from this computer and the
    /// image's own files that were renamed or moved. What an in-place edit reads back in full.
    pub fn changed_files(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .local
            .keys()
            .chain(self.moved.keys())
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn size_of(&self, path: &str) -> Result<u64> {
        self.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.size)
            .ok_or_else(|| Error::Format(format!("{path} is not in the edited image")))
    }
}

impl SourceTree for Edited {
    fn release(&mut self) {
        self.base.release();
    }

    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let size = self.size_of(path)?;
        self.read_range(path, 0, usize::try_from(size).unwrap_or(usize::MAX))
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let Some(from) = self.local.get(path) else {
            // The image's own file, under the name the image has it by.
            let in_image = self.moved.get(path).map_or(path, String::as_str);
            return self.base.read_range(in_image, offset, len);
        };
        if self.open.as_ref().is_none_or(|(p, _)| p != path) {
            let f = std::fs::File::open(from)
                .map_err(|e| Error::Format(format!("{}: {e}", from.display())))?;
            self.open = Some((path.to_string(), f));
        }
        // A file that changed size since the edit was planned would leave the image's sizes
        // wrong: refuse it rather than write it.
        let size = self.size_of(path)?;
        let Some((_, f)) = self.open.as_mut() else {
            unreachable!("opened above")
        };
        if f.metadata()?.len() != size {
            return format_err(format!(
                "{} changed while the image was being written",
                from.display()
            ));
        }
        let end = offset.saturating_add(len as u64).min(size);
        let mut buf = Vec::with_capacity(end.saturating_sub(offset) as usize);
        if offset < end {
            f.seek(SeekFrom::Start(offset))?;
            f.by_ref().take(end - offset).read_to_end(&mut buf)?;
        }
        Ok(buf)
    }

    fn describe(&self) -> String {
        format!("{} with {} change(s)", self.base.describe(), self.changes)
    }
}
