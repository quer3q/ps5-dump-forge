//! Extraction into a folder. Names come from an image someone else made,
//! so every path is checked before the first byte is written, and every file is created
//! exclusively inside our own `.part` folder.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;

use anyhow::{Context, bail};
use ps5upload_fpkg::source::SourceTree;
use unicode_normalization::UnicodeNormalization;

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
            p.nfc().collect::<String>().to_lowercase()
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

    fn create(&mut self, rel: &str) -> anyhow::Result<File> {
        let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
        let dir = self.dir(parent)?;
        at::create(&dir, name).with_context(|| format!("creating {rel}"))
    }

    /// A new file at the root (the LZ4 pack files), exclusive and not through a link.
    pub(crate) fn create_root(&self, name: &str) -> std::io::Result<File> {
        at::create(&self.root, name)
    }

    /// A root file this job created, opened again for writing (an LZ4 volume's header).
    pub(crate) fn reopen_root(&self, name: &str) -> std::io::Result<File> {
        at::reopen(&self.root, name)
    }

    /// fsyncs every directory this job created, and the root.
    pub(crate) fn sync(&mut self, ctx: &Ctx) -> anyhow::Result<()> {
        let dirs: Vec<String> = self.created.iter().cloned().collect();
        for rel in dirs {
            ctx.check()?;
            at::sync(&self.dir(&rel)?, ctx.cancel).with_context(|| format!("syncing {rel}/"))?;
        }
        at::sync(&self.root, ctx.cancel).context("syncing the .part folder")
    }
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
pub(crate) fn write(tree: &mut dyn SourceTree, part: &Part, ctx: &Ctx) -> anyhow::Result<u64> {
    let files = tree.files().to_vec();
    let empty_dirs = tree.empty_dirs().to_vec();
    let total = files
        .iter()
        .try_fold(0u64, |sum, f| sum.checked_add(f.size))
        .context("total size overflows")?;
    let mut dest = Dest::open(part)?;
    let mut done = 0u64;
    // The job's one cadence (U5): what one file measured carries over to the next. Each file
    // is synced when it is complete, so no dirty bytes carry across files.
    #[cfg(target_os = "freebsd")]
    let mut cadence = crate::durable::Cadence::new();
    for file in &files {
        ctx.check()?;
        #[cfg_attr(target_os = "freebsd", allow(unused_mut))]
        let mut out = dest.create(&file.path)?;
        #[cfg(target_os = "freebsd")]
        let mut out = crate::durable::SyncEvery::new(out, &mut cadence, ctx.cancel);
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
            done += buf.len() as u64;
            ctx.progress("write", done, total);
        }
        #[cfg(not(target_os = "freebsd"))]
        out.sync_all()
            .with_context(|| format!("syncing {}", file.path))?;
        #[cfg(target_os = "freebsd")]
        {
            out.finish()
                .with_context(|| format!("syncing {}", file.path))?;
            crate::convert::log_cadence(&mut cadence, ctx);
        }
    }
    for dir in &empty_dirs {
        ctx.check()?;
        dest.dir(dir)?;
    }
    dest.sync(ctx)?;
    ctx.progress("write", total, total);
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(files: &[&str], dirs: &[&str], ci: bool) -> Vec<String> {
        let f: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let d: Vec<String> = dirs.iter().map(|s| s.to_string()).collect();
        extraction_findings(&f, &d, ci)
    }

    #[test]
    fn components() {
        for bad in [
            "",
            ".",
            "..",
            "a\0b",
            "a\\b",
            "a:b",
            "CON",
            "con.txt",
            "Com1.dat",
            "LPT9",
            "nul .x",
            "x.",
            "x ",
            "a\u{1}",
            "a\u{fffd}b",
        ] {
            assert!(bad_component(bad).is_some(), "{bad:?}");
        }
        for good in ["eboot.bin", "COM10", "console.txt", "LPT", "a.b.c", "日本"] {
            assert_eq!(bad_component(good), None, "{good:?}");
        }
        assert!(bad_component(&"x".repeat(256)).is_some());
    }

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
        let err = write(&mut Greedy(vec![file]), &part, &ctx).unwrap_err();
        assert!(err.to_string().contains("gave 11 bytes"), "{err}");
    }

    #[test]
    fn clean_tree_passes() {
        assert!(findings(&["eboot.bin", "sce_sys/param.json"], &["data/empty"], true).is_empty());
    }

    #[test]
    fn collisions() {
        assert_eq!(findings(&["A.bin", "a.bin"], &[], true).len(), 1);
        assert!(findings(&["A.bin", "a.bin"], &[], false).is_empty());
        // NFC vs NFD spelling of the same name.
        assert_eq!(findings(&["caf\u{e9}", "cafe\u{301}"], &[], true).len(), 1);
        assert_eq!(findings(&["Data/x", "data/y"], &[], true).len(), 1);
        assert_eq!(findings(&["a", "a/b"], &[], false).len(), 1);
        assert_eq!(findings(&["a"], &["a"], false).len(), 1);
        assert_eq!(findings(&["a", "a"], &[], false).len(), 1);
    }
}
