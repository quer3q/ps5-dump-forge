//! The PS5's package scan (each package's header and container only; reads, never writes),
//! the folders the desktop explorer refuses and the game folder inside an archive.

use std::collections::{HashSet, VecDeque};
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::finalize::{FileId, path_id};
#[cfg(target_os = "freebsd")]
use crate::library_roots::ps5_classify;
use crate::library_roots::{Os, home, refusal, shadowmnt, skip_dir_name, system_dir, system_drive};
use crate::preflight::{MAX_PARAM_JSON, parse_param};
use crate::scan;

// The walk's limits: depth below a root (0), directories and entries per root, entries per
// directory, package opens and packages per scan, and the time budget.
const MAX_DEPTH: usize = 6;
const MAX_DIRS: usize = 20_000;
const MAX_ENTRIES: usize = 200_000;
const MAX_ENTRIES_PER_DIR: usize = 10_000;
const MAX_IMAGE_PROBES: usize = 256;
const MAX_GAMES: usize = 2_000;
const SCAN_BUDGET: Duration = Duration::from_secs(15);
/// The largest `sce_sys/icon0.png` a package's `has_cover` counts, raw PNG bytes.
const MAX_COVER: u64 = 512 * 1024;

const LIMITS: Limits = Limits {
    depth: MAX_DEPTH,
    dirs: MAX_DIRS,
    entries: MAX_ENTRIES,
    per_dir: MAX_ENTRIES_PER_DIR,
    images: MAX_IMAGE_PROBES,
    games: MAX_GAMES,
    budget: SCAN_BUDGET,
};

/// The scan's limits, fixed in production and lowered by tests.
struct Limits {
    depth: usize,
    dirs: usize,
    entries: usize,
    per_dir: usize,
    images: usize,
    games: usize,
    budget: Duration,
}

/// Decides whether the walk may enter a path: `Err` says why not.
type Classify<'a> = &'a (dyn Fn(&Path) -> Result<(), String> + Sync);

/// Package scans run one at a time; a poisoned lock is still used.
static SCAN: Mutex<()> = Mutex::new(());

/// What `probe_all` reports for a candidate it never opened because the budget was spent.
const NOT_PROBED: &str = "not probed: the scan ran out of time";

/// A link (or, on Windows, any reparse point) is never followed.
fn is_link(ty: FileType, _meta: impl FnOnce() -> Option<fs::Metadata>) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if _meta().is_none_or(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0) {
            return true;
        }
    }
    ty.is_symlink()
}

fn is_pkg(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("pkg"))
}

struct Walk<'a> {
    limits: &'a Limits,
    classify: Classify<'a>,
    home: Option<PathBuf>,
    start: Instant,
    candidates: Vec<PathBuf>,
    truncated: bool,
    /// Directories already queued or accepted, and the identities of images already listed.
    seen_dirs: HashSet<FileId>,
    seen_images: HashSet<FileId>,
    /// Every accepted root: a walk from another root does not enter it.
    root_ids: HashSet<FileId>,
    image_probes: usize,
}

impl Walk<'_> {
    fn out_of_time(&mut self) -> bool {
        let over = self.start.elapsed() >= self.limits.budget;
        self.truncated |= over;
        over
    }

    /// Breadth first from `root` (depth 0), within the per-root budgets.
    fn root(&mut self, root: &Path, root_id: FileId) {
        let (mut dirs, mut entries) = (0, 0);
        let mut queue = VecDeque::from([(root.to_path_buf(), 0, root_id.0)]);
        while let Some((dir, depth, dev)) = queue.pop_front() {
            if self.out_of_time() {
                return;
            }
            if dirs >= self.limits.dirs {
                self.truncated = true;
                return;
            }
            dirs += 1;
            // An unreadable folder is left out.
            let Ok(mut names) = self.list(&dir, &mut entries) else {
                continue;
            };
            names.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, ty) in names {
                let path = dir.join(&name);
                if ty.is_file() {
                    self.image(path, &name, dev);
                } else if depth < self.limits.depth {
                    self.child_dir(&mut queue, path, depth + 1, dev);
                } else {
                    // A folder below the depth limit is left unread.
                    self.truncated = true;
                }
            }
            if entries >= self.limits.entries {
                self.truncated = true;
                return;
            }
        }
    }

    /// The first `per_dir` plain entries of `dir` worth a look: regular files and folders with
    /// UTF-8 names that are not junk, `.part` files or links.
    fn list(
        &mut self,
        dir: &Path,
        entries: &mut usize,
    ) -> std::io::Result<Vec<(String, FileType)>> {
        let mut out = Vec::new();
        for (n, entry) in fs::read_dir(dir)?.enumerate() {
            if n >= self.limits.per_dir || *entries >= self.limits.entries {
                self.truncated = true;
                break;
            }
            *entries += 1;
            let Ok(entry) = entry else { continue };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Ok(ty) = entry.file_type() else { continue };
            if is_link(ty, || entry.metadata().ok()) || scan::is_junk(&name) {
                continue;
            }
            let part = name
                .rsplit_once('.')
                .is_some_and(|(_, e)| e.eq_ignore_ascii_case("part"));
            if !part && (ty.is_file() || (ty.is_dir() && !skip_dir_name(&name))) {
                out.push((name, ty));
            }
        }
        Ok(out)
    }

    fn image(&mut self, path: PathBuf, name: &str, dev: u64) {
        if !is_pkg(name) {
            return;
        }
        // One on another filesystem than its folder is left out.
        match path_id(&path) {
            Ok((id, _)) if id.0 != dev || !self.seen_images.insert(id) => {}
            Ok(_) if self.image_probes >= self.limits.images => self.truncated = true,
            Ok(_) => {
                self.image_probes += 1;
                self.candidates.push(path);
            }
            Err(_) => {}
        }
    }

    fn child_dir(
        &mut self,
        queue: &mut VecDeque<(PathBuf, usize, u64)>,
        path: PathBuf,
        depth: usize,
        dev: u64,
    ) {
        if shadowmnt(&path) || system_dir(&path, self.home.as_deref()) {
            return;
        }
        let Ok((id, _)) = path_id(&path) else { return };
        if self.root_ids.contains(&id) || !self.seen_dirs.insert(id) {
            return;
        }
        if id.0 != dev && (self.classify)(&path).is_err() {
            return;
        }
        queue.push_back((path, depth, id.0));
    }
}

/// A root must not be a link, and both it and its canonical form must pass the classifier.
fn eligible(path: &Path, classify: Classify<'_>) -> Result<(), String> {
    classify(path)?;
    let meta = path.symlink_metadata().map_err(|e| e.to_string())?;
    if is_link(meta.file_type(), || Some(meta.clone())) {
        return Err("is a link".to_string());
    }
    let real = path.canonicalize().map_err(|e| e.to_string())?;
    classify(&real)
}

/// Probes every candidate on up to four threads, in candidate order; a panic skips one.
fn probe_all<T: Send>(
    candidates: &[PathBuf],
    limits: &Limits,
    start: Instant,
    probe: fn(&Path) -> Result<Option<T>, String>,
) -> Vec<Result<Option<T>, String>> {
    let next = AtomicUsize::new(0);
    let slots = Mutex::new((0..candidates.len()).map(|_| None).collect::<Vec<_>>());
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(candidate) = candidates.get(i) else {
                        break;
                    };
                    let result = if start.elapsed() >= limits.budget {
                        Err(NOT_PROBED.to_string())
                    } else {
                        std::panic::catch_unwind(|| probe(candidate))
                            .unwrap_or_else(|_| Err("skipped, the probe panicked".to_string()))
                    };
                    slots.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(result);
                }
            });
        }
    });
    let slots = slots.into_inner().unwrap_or_else(|e| e.into_inner());
    slots.into_iter().map(|r| r.unwrap_or(Ok(None))).collect()
}

/// Why `path` is a folder (or inside one) the desktop explorer refuses; `None` on the PS5.
/// A path not there yet (a move's target) by its folder's canonical path and its name.
pub(crate) fn folder_refusal(path: &Path) -> Option<String> {
    let os = Os::HOST?;
    let real = dunce::canonicalize(path)
        .ok()
        .or_else(|| {
            Some(
                dunce::canonicalize(path.parent()?)
                    .ok()?
                    .join(path.file_name()?),
            )
        })
        .unwrap_or_else(|| path.to_path_buf());
    let home = home().map(|h| dunce::canonicalize(&h).unwrap_or(h));
    refusal(&real, home.as_deref(), os, system_drive())
}

/// Walks the eligible `roots`, one per identity in the order given.
fn walk<'a>(roots: &[PathBuf], limits: &'a Limits, classify: Classify<'a>) -> Walk<'a> {
    let mut walk = Walk {
        limits,
        classify,
        home: home(),
        start: Instant::now(),
        candidates: Vec::new(),
        truncated: false,
        seen_dirs: HashSet::new(),
        seen_images: HashSet::new(),
        root_ids: HashSet::new(),
        image_probes: 0,
    };
    let mut accepted = Vec::new();
    for root in roots {
        // A root that isn't eligible, isn't UTF-8 or can't be read is left out.
        let id = match (eligible(root, classify), root.to_str(), path_id(root)) {
            (Ok(()), Some(_), Ok((id, _))) => Some(id),
            _ => None,
        };
        // Walk the canonical spelling so every check is canonical. Not on Windows (`\\?\C:\`
        // would leak into entry paths); ShadowMountPlus exists only on FreeBSD.
        #[cfg(not(windows))]
        let root = &root.canonicalize().unwrap_or_else(|_| root.clone());
        if let Some(id) = id
            && walk.root_ids.insert(id)
        {
            accepted.push((root.clone(), id));
        }
    }
    for (root, id) in &accepted {
        walk.seen_dirs.insert(*id);
        walk.root(root, *id);
    }
    walk
}

/// What a package installs, by its container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageContent {
    Game,
    /// A patch package (`\x7FLIH`).
    Patch,
    /// A DLC or another add-on (content type `0x21`).
    AdditionalContent,
    /// Unreadable, or a content type Forge doesn't know: the console judges it.
    Unknown,
}

/// One `.pkg` the package scan found; title fields from the container's plaintext `param.json`.
#[derive(Debug, Clone, Serialize)]
pub struct PackageEntry {
    pub path: PathBuf,
    pub size: u64,
    pub content_id: Option<String>,
    pub title_id: Option<String>,
    pub title_name: Option<String>,
    pub title_names: std::collections::BTreeMap<String, String>,
    pub version: Option<String>,
    pub content: PackageContent,
    /// The container carries a plaintext `icon0.png` of 1 B..512 KiB.
    pub has_cover: bool,
}

/// What a package scan found.
#[derive(Debug, Clone, Serialize)]
pub struct PackageScan {
    pub entries: Vec<PackageEntry>,
    pub truncated: bool,
}

/// A package's entry from its header and container alone (`fpkg::peek`, a few KiB read).
fn probe_package(path: &Path) -> Result<Option<PackageEntry>, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    let mut entry = PackageEntry {
        path: path.to_path_buf(),
        size,
        content_id: None,
        title_id: None,
        title_name: None,
        title_names: Default::default(),
        version: None,
        content: PackageContent::Unknown,
        has_cover: false,
    };
    // Not a package this reader knows: listed all the same, the console judges it.
    let Ok(peek) = ps5_dump_forge_fpkg::peek(path, MAX_PARAM_JSON) else {
        return Ok(Some(entry));
    };
    entry.content = match peek.content_type {
        _ if peek.patch => PackageContent::Patch,
        0x20 | 0x26 => PackageContent::Game,
        0x21 => PackageContent::AdditionalContent,
        _ => PackageContent::Unknown,
    };
    entry.content_id = Some(peek.content_id).filter(|id| !id.is_empty());
    entry.has_cover = peek
        .icon0_size
        .is_some_and(|s| (1..=MAX_COVER).contains(&s));
    let info = parse_param(peek.param_json.as_deref().unwrap_or_default());
    if let Some(json) = info.param_json.as_ref() {
        entry.title_name = crate::inspect::title_name(json);
        entry.title_names = crate::inspect::title_names(json);
        entry.version = (json.get("contentVersion").and_then(|v| v.as_str())).map(str::to_string);
    }
    entry.title_id = info.title_id;
    Ok(Some(entry))
}

/// Every `.pkg` under `roots` (game folders entered too).
fn packages(roots: &[PathBuf], limits: &Limits, classify: Classify<'_>) -> PackageScan {
    let mut walk = walk(roots, limits, classify);
    let results = probe_all(&walk.candidates, limits, walk.start, probe_package);
    let mut entries = Vec::new();
    for result in results {
        match result {
            Ok(Some(e)) => entries.push(e),
            Ok(None) => {}
            // Gone since the walk, or never reached: only the budget makes the list partial.
            Err(why) => walk.truncated |= why == NOT_PROBED,
        }
    }
    if entries.len() > limits.games {
        entries.truncate(limits.games);
        walk.truncated = true;
    }
    entries.sort_by_cached_key(|e| {
        let shown = (e.title_name.as_deref().or(e.title_id.as_deref())).map(str::to_lowercase);
        (shown.is_none(), shown, e.version.clone(), e.path.clone())
    });
    PackageScan {
        entries,
        truncated: walk.truncated,
    }
}

pub(crate) fn scan_packages(roots: &[PathBuf]) -> anyhow::Result<PackageScan> {
    let _guard = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    if roots.is_empty() {
        return Ok(packages(roots, &LIMITS, &|_| Ok(())));
    }
    #[cfg(not(target_os = "freebsd"))]
    anyhow::bail!("package discovery runs only on the PS5");
    #[cfg(target_os = "freebsd")]
    Ok(packages(roots, &LIMITS, &ps5_classify))
}

/// How deep inside an archive its game folder may sit (the archive's root is 0).
const ARCHIVE_DEPTH: usize = 4;
/// Candidate game folders whose `param.json` are read; more are refused unread.
// ponytail: real archives hold one or two games; 64 bounds a crafted one.
const ARCHIVE_CANDIDATES: usize = 64;
/// The candidates' `param.json` bytes read in all.
// ponytail: `read_files` returns the buffers together, so this is also the peak.
const ARCHIVE_PARAM_BUDGET: u64 = 8 << 20;

/// The "more than one game" error for these roots: the first few, then a count.
fn several_games(roots: &[&str]) -> anyhow::Error {
    const SHOWN: usize = 5;
    let mut names: Vec<&str> = roots
        .iter()
        .take(SHOWN)
        .map(|r| r.trim_end_matches('/'))
        .collect();
    let more = roots.len().saturating_sub(SHOWN);
    let more = format!("and {more} more");
    if roots.len() > SHOWN {
        names.push(&more);
    }
    anyhow::anyhow!(
        "More than one PS5 game inside this archive: {}",
        names.join(", ")
    )
}

/// The game folder inside an archive: `""` or `"Game/"`, the outermost `eboot.bin` beside a
/// `sce_sys/param.json` with a title ID (`__MACOSX` and junk skipped); with several candidates
/// their `param.json` are read.
pub(crate) fn archive_game_root(
    path: &Path,
    password: Option<&str>,
    listing: &ps5_dump_forge_archive::Listing,
    cancel: &AtomicBool,
) -> anyhow::Result<String> {
    let files: std::collections::HashMap<&str, u64> = listing
        .entries
        .iter()
        .filter(|e| {
            !e.dir
                && !e
                    .path
                    .split('/')
                    .any(|c| c == "__MACOSX" || scan::is_junk(c))
        })
        .map(|e| (e.path.as_str(), e.size))
        .collect();
    let param = |root: &str| format!("{root}sce_sys/param.json");
    let candidates = |marker: &str| -> Vec<&str> {
        files
            .keys()
            .filter_map(|p| p.strip_suffix(marker))
            .filter(|r| {
                (r.is_empty() || r.ends_with('/')) && r.matches('/').count() <= ARCHIVE_DEPTH
            })
            .filter(|r| {
                files
                    .get(param(r).as_str())
                    .is_some_and(|s| (1..=MAX_PARAM_JSON).contains(s))
            })
            .collect()
    };
    let mut roots = candidates("eboot.bin");
    // No game: additional content (param.json without eboot.bin).
    if roots.is_empty() {
        roots = candidates("sce_sys/param.json");
    }
    roots.sort_unstable();
    if roots.len() > ARCHIVE_CANDIDATES {
        return Err(several_games(&roots));
    }
    if roots.len() > 1 {
        let wanted: HashSet<String> = roots.iter().map(|r| param(r)).collect();
        let budget: u64 = wanted.iter().map(|p| files[p.as_str()]).sum();
        if budget > ARCHIVE_PARAM_BUDGET {
            return Err(several_games(&roots));
        }
        let params = ps5_dump_forge_archive::read_files(
            path,
            password,
            &|p| wanted.contains(p),
            budget,
            cancel,
        )?;
        // Each buffer is parsed and dropped in turn.
        let games: HashSet<String> = params
            .into_iter()
            .filter(|(_, b)| parse_param(b).title_id.is_some())
            .map(|(p, _)| p)
            .collect();
        roots.retain(|r| games.contains(&param(r)));
    }
    let mut outer: Vec<&str> = Vec::new();
    for r in roots {
        if !outer.iter().any(|o| r.starts_with(o)) {
            outer.push(r);
        }
    }
    match outer[..] {
        [] => anyhow::bail!("No PS5 game inside this archive"),
        [root] => Ok(root.to_string()),
        _ => Err(several_games(&outer)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh temp folder, removed when dropped.
    struct Tmp(PathBuf);
    impl std::ops::Deref for Tmp {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }
    impl AsRef<Path> for Tmp {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("forge-library-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir.canonicalize().unwrap())
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// A game folder at `root/rel` with this `param.json`.
    fn game(root: &Path, rel: &str, param: &str) -> PathBuf {
        let dir = root.join(rel);
        put(&dir, "eboot.bin", b"\x7fELF");
        put(&dir, "sce_sys/param.json", param.as_bytes());
        dir
    }

    /// A small game's `.pkg` at `out`, built as a conversion builds one.
    fn package(src: &Path, out: &Path) {
        use ps5upload_fpkg::build::{self, BuildControl, BuildRequest};
        let mut request = BuildRequest::production(src, out.parent().unwrap());
        request.time = Some((1_700_000_000, 0));
        request.threads = Some(2);
        request.level = ps5upload_fpkg::kraken::Level::Fast;
        let mut tree = ps5upload_fpkg::source::FolderSource::open(src).unwrap();
        let prepared = build::prepare(&mut tree, &request, &mut BuildControl::default()).unwrap();
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(out)
            .unwrap();
        let mut control = BuildControl::default();
        build::write_package(
            &prepared,
            &mut tree,
            &mut file,
            out,
            &mut |_| {},
            &mut control,
        )
        .unwrap();
    }

    #[test]
    fn scan_packages_lists_every_pkg_from_its_container() {
        let (src, root) = (tmp("pkg-src"), tmp("pkg-root"));
        game(
            &src,
            "",
            r#"{"contentId":"UP0000-PPSA01234_00-TESTGAME00000000","titleId":"PPSA01234","contentVersion":"01.001.000","localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"Pkg Game"}}}"#,
        );
        put(&src, "sce_sys/icon0.png", b"\x89PNG\r\n");
        put(&src, "data/a.bin", &[7u8; 70_000]);
        // Inside a game folder too: a package scan enters every folder.
        let inside = game(&root, "Game", r#"{"titleId":"PPSA00009"}"#);
        let pkg = inside.join("Game.PKG");
        package(&src, &pkg);

        // A patch: its 64 KiB header points at the patched package's container.
        let bytes = fs::read(&pkg).unwrap();
        let le64 = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize;
        let (pfs_offset, cnt_offset) = (le64(0x10), le64(0x58));
        let cnt_end = if cnt_offset < pfs_offset {
            pfs_offset
        } else {
            bytes.len()
        };
        let mut patch = vec![0u8; 0x10000];
        patch.extend_from_slice(&bytes[cnt_offset..cnt_end]);
        patch[..4].copy_from_slice(b"\x7FLIH");
        let len = patch.len() as u64;
        for (at, v) in [(0x18, len), (0x30, 0x10000), (0x38, len - 0x10000)] {
            patch[at..at + 8].copy_from_slice(&v.to_le_bytes());
        }
        put(&root, "deep/er/update.pkg", &patch);
        put(&root, "junk.pkg", b"not a package");
        put(&root, "Other.exfat", b"not listed by a package scan");

        let roots = [root.to_path_buf()];
        let scan = packages(&roots, &LIMITS, &|_| Ok(()));
        assert!(!scan.truncated);
        let content: Vec<_> = scan
            .entries
            .iter()
            .map(|e| (e.path.clone(), e.content))
            .collect();
        assert_eq!(
            content,
            [
                (pkg.clone(), PackageContent::Game),
                (root.join("deep/er/update.pkg"), PackageContent::Patch),
                (root.join("junk.pkg"), PackageContent::Unknown),
            ]
        );
        let (game, patch) = (&scan.entries[0], &scan.entries[1]);
        assert_eq!(game.size, fs::metadata(&pkg).unwrap().len());
        assert_eq!(
            (game.title_id.as_deref(), game.title_name.as_deref()),
            (Some("PPSA01234"), Some("Pkg Game"))
        );
        assert_eq!(game.title_names["en-US"], "Pkg Game");
        assert_eq!(game.version.as_deref(), Some("01.001.000"));
        assert_eq!(
            game.content_id.as_deref(),
            Some("UP0000-PPSA01234_00-TESTGAME00000000")
        );
        assert!(game.has_cover);
        assert_eq!(
            (patch.title_id.as_deref(), patch.has_cover),
            (Some("PPSA01234"), true)
        );
        let junk = &scan.entries[2];
        assert_eq!(
            (junk.size, junk.title_id.as_deref(), junk.has_cover),
            (13, None, false)
        );
    }
}
