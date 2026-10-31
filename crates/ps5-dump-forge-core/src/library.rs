//! The Library: finds game dumps (folders and images) under a chosen folder or the PS5's roots
//! and reads only what a list needs (title, version, cover presence). Reads, never writes; never the conversion
//! folder scanner or a full `inspect`. Bounded by the limits below.

use std::collections::{HashSet, VecDeque};
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::convert::Kind;
use crate::finalize::{FileId, handle_id, path_id};
#[cfg(target_os = "freebsd")]
use crate::library_roots::ps5_classify;
use crate::library_roots::{Os, home, refusal, shadowmnt, skip_dir_name, system_dir, system_drive};
use crate::preflight::{MAX_PARAM_JSON, parse_param};
use crate::{Format, scan};

/// Folders deeper than this below a root are not entered (the root is depth 0).
const MAX_DEPTH: usize = 6;
/// Directory probes per root.
const MAX_DIRS: usize = 20_000;
/// Enumerated directory entries per root.
const MAX_ENTRIES: usize = 200_000;
/// Entries read from one directory; the rest are left out and the scan is truncated.
const MAX_ENTRIES_PER_DIR: usize = 10_000;
/// Image opens attempted per scan.
const MAX_IMAGE_PROBES: usize = 256;
/// Games listed per scan.
const MAX_GAMES: usize = 2_000;
/// The scan's cooperative time budget.
const SCAN_BUDGET: Duration = Duration::from_secs(15);
/// Detailed warnings; one more line counts the rest.
const MAX_WARNINGS: usize = 20;
/// The largest `sce_sys/icon0.png` the Library shows, raw PNG bytes.
pub(crate) const MAX_LIBRARY_COVER: u64 = 512 * 1024;

/// One game the Library found.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub kind: Format,
    pub title_id: String,
    pub title_name: Option<String>,
    /// Every `localizedParameters` title by language tag, as `Inspection::title_names`.
    pub title_names: std::collections::BTreeMap<String, String>,
    pub version: Option<String>,
    pub has_cover: bool,
    /// LZ4 packed (ampr_emu asset packs): `ampr_assets.index` at the root starts with the
    /// manifest magic, the rule of `reader::detect` (and of inspect's Packed).
    pub lz4_packed: bool,
    /// zstd packed: `ampr_zstd.index` at the root starts with `AMPRZPK1` (inspect's rule).
    pub zstd_packed: bool,
    /// No `eboot.bin` at the root (a DLC, an update or another package), as
    /// `Inspection::additional_content`.
    pub additional_content: bool,
}

/// What a Library scan found.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryScan {
    pub roots: Vec<PathBuf>,
    pub entries: Vec<LibraryEntry>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

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

/// Decides whether the Library may enter a path: `Err` says why not.
type Classify<'a> = &'a (dyn Fn(&Path) -> Result<(), String> + Sync);

/// Library scans run one at a time; a panicking scan must not wedge the next, so a poisoned
/// lock is still used.
static SCAN: Mutex<()> = Mutex::new(());

/// Detailed warnings up to the cap, then a count.
#[derive(Default)]
struct Warnings {
    list: Vec<String>,
    omitted: usize,
}

impl Warnings {
    fn add(&mut self, text: String) {
        if self.list.len() < MAX_WARNINGS {
            self.list.push(text);
        } else {
            self.omitted += 1;
        }
    }

    fn finish(mut self) -> Vec<String> {
        if self.omitted > 0 {
            self.list
                .push(format!("{} more warnings not shown", self.omitted));
        }
        self.list
    }
}

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

fn image_kind(name: &str) -> Option<Kind> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "exfat" => Kind::Exfat,
        "ffpkg" => Kind::Ffpkg,
        "ffpfs" => Kind::Ffpfs,
        "ffpfsc" => Kind::Ffpfsc,
        "pkg" => Kind::Pkg,
        _ => return None,
    })
}

struct Walk<'a> {
    limits: &'a Limits,
    classify: Classify<'a>,
    home: Option<PathBuf>,
    start: Instant,
    candidates: Vec<(PathBuf, Kind)>,
    /// Accepted game folders, probed inline: only an accepted one is not entered.
    folders: Vec<LibraryEntry>,
    warnings: Warnings,
    truncated: bool,
    /// Why a scan root itself could not be listed (a warning too; a chosen folder fails on it).
    root_error: Option<String>,
    /// Directories already queued or accepted, and the identities of images already listed.
    seen_dirs: HashSet<FileId>,
    seen_images: HashSet<FileId>,
    /// Every accepted root: a walk from another root does not enter it.
    root_ids: HashSet<FileId>,
    image_probes: usize,
    /// A package scan: only `.pkg` files are candidates and game folders are entered too.
    packages: bool,
}

impl Walk<'_> {
    fn out_of_time(&mut self) -> bool {
        let over = self.start.elapsed() >= self.limits.budget;
        if over && !self.truncated {
            self.warnings
                .add("the scan ran out of time; the list is partial".into());
        }
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
            let mut names = match self.list(&dir, &mut entries, depth == 0) {
                Ok(names) => names,
                Err(e) => {
                    if depth == 0 {
                        self.root_error
                            .get_or_insert(format!("{}: {e}", dir.display()));
                    }
                    self.warnings.add(format!("{}: {e}", dir.display()));
                    continue;
                }
            };
            names.sort_by(|a, b| a.0.cmp(&b.0));
            let has = |n: &str| names.iter().any(|(name, _)| name == n);
            // Without `eboot.bin` too: additional content (a DLC, an update) is listed.
            if !self.packages && has("sce_sys") {
                // Folder probes are cheap (metadata and one small read), so they run here:
                // an accepted game is never entered, a rejected folder is searched on.
                if let Some(found) = probe_folder(&dir) {
                    self.folders.push(found);
                    continue;
                }
            }
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

    /// The first `per_dir` plain entries of `dir` worth a look: regular files and folders
    /// with UTF-8 names that are not junk, `.part` files or links.
    fn list(
        &mut self,
        dir: &Path,
        entries: &mut usize,
        root_dir: bool,
    ) -> std::io::Result<Vec<(String, FileType)>> {
        let mut out = Vec::new();
        for (n, entry) in fs::read_dir(dir)?.enumerate() {
            if n >= self.limits.per_dir || *entries >= self.limits.entries {
                self.truncated = true;
                break;
            }
            *entries += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    // An unreadable listing of the chosen root must not look complete.
                    if root_dir {
                        self.root_error
                            .get_or_insert(format!("{}: {e}", dir.display()));
                    }
                    self.warnings
                        .add(format!("{}: skipped an entry: {e}", dir.display()));
                    continue;
                }
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                self.warnings.add(format!(
                    "{}: skipped a name that is not UTF-8",
                    dir.display()
                ));
                continue;
            };
            let ty = match entry.file_type() {
                Ok(ty) => ty,
                Err(e) => {
                    self.warnings.add(format!(
                        "{}: skipped, cannot stat: {e}",
                        dir.join(&name).display()
                    ));
                    continue;
                }
            };
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
        let kind = image_kind(name).filter(|k| !self.packages || *k == Kind::Pkg);
        let Some(kind) = kind else { return };
        match path_id(&path) {
            Ok((id, _)) if id.0 != dev => self.warnings.add(format!(
                "{}: skipped, on another filesystem than its folder",
                path.display()
            )),
            Ok((id, _)) if !self.seen_images.insert(id) => {}
            Ok(_) if self.image_probes >= self.limits.images => self.truncated = true,
            Ok(_) => {
                self.image_probes += 1;
                self.candidates.push((path, kind));
            }
            Err(e) => self.warnings.add(format!("{}: {e}", path.display())),
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
        let id = match path_id(&path) {
            Ok((id, _)) => id,
            Err(e) => return self.warnings.add(format!("{}: {e}", path.display())),
        };
        if self.root_ids.contains(&id) || !self.seen_dirs.insert(id) {
            return;
        }
        if id.0 != dev
            && let Err(why) = (self.classify)(&path)
        {
            return self.warnings.add(format!("{}: {why}", path.display()));
        }
        queue.push_back((path, depth, id.0));
    }
}

/// Opens `rel` under the folder `dir` as a regular file (no link at the end) on the device
/// `dev` of the folder, with its size; `None` when absent, not a regular file or elsewhere.
fn folder_open(dir: &Path, rel: &str, dev: u64) -> Option<(fs::File, u64)> {
    let file = scan::open_nofollow(&dir.join(rel)).ok()?;
    let meta = file.metadata().ok()?;
    (meta.is_file() && handle_id(&file).ok()?.0 == dev).then_some((file, meta.len()))
}

/// The bytes of `rel` under `dir`, at most `max`; `None` when absent, empty, too big, not a
/// regular file or on another device.
fn folder_file(dir: &Path, rel: &str, max: u64, dev: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let (file, len) = folder_open(dir, rel, dev)?;
    if len == 0 || len > max {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

/// `sce_sys` must be a plain directory on the folder's own device before anything under it is
/// opened (a mount there could be a network share).
fn plain_dir(path: &Path, dev: u64) -> bool {
    path.symlink_metadata().is_ok_and(|m| {
        !is_link(m.file_type(), || Some(m.clone()))
            && m.is_dir()
            && path_id(path).is_ok_and(|(id, _)| id.0 == dev)
    })
}

/// What the list shows, from `param.json` bytes; `None` without a usable title ID.
fn entry(
    path: PathBuf,
    kind: Format,
    param: &[u8],
    has_cover: bool,
    (lz4_packed, zstd_packed): (bool, bool),
    additional_content: bool,
) -> Option<LibraryEntry> {
    let info = parse_param(param);
    let json = info.param_json.as_ref()?;
    Some(LibraryEntry {
        path,
        kind,
        title_id: info.title_id?,
        title_name: crate::inspect::title_name(json),
        title_names: crate::inspect::title_names(json),
        version: json
            .get("contentVersion")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        has_cover,
        lz4_packed,
        zstd_packed,
        additional_content,
    })
}

/// `reader::detect` on a game folder: its `ampr_assets.index` is a regular file (on the
/// folder's device) that starts with the manifest magic. One open and an 8-byte read.
fn folder_packed(dir: &Path, dev: u64) -> bool {
    use ps5_dump_forge_lz4::{MANIFEST, format::PAK_MAGIC};
    folder_magic(dir, MANIFEST, PAK_MAGIC, dev)
}

/// `name` in a game folder is a regular file (on the folder's device) that starts with `magic`.
fn folder_magic(dir: &Path, name: &str, magic: &[u8; 8], dev: u64) -> bool {
    use std::io::Read;
    let Some((mut file, _)) = folder_open(dir, name, dev) else {
        return false;
    };
    let mut head = [0u8; 8];
    file.read_exact(&mut head).is_ok() && &head == magic
}

/// A game folder's entry, or `None` when it is not a usable one (a folder's problems are
/// its own business: no warning).
fn probe_folder(dir: &Path) -> Option<LibraryEntry> {
    let dev = path_id(dir).ok()?.0.0;
    // Without one it is additional content (a DLC, an update), listed too.
    let no_eboot = folder_open(dir, "eboot.bin", dev).is_none();
    if !plain_dir(&dir.join("sce_sys"), dev) {
        return None;
    }
    let param = folder_file(dir, "sce_sys/param.json", MAX_PARAM_JSON, dev)?;
    // Metadata only: the icon is read when the list asks for it.
    let cover = folder_open(dir, "sce_sys/icon0.png", dev)
        .is_some_and(|(_, len)| (1..=MAX_LIBRARY_COVER).contains(&len));
    let packed = folder_packed(dir, dev);
    let zstd = folder_magic(
        dir,
        crate::zstd_pack::PACK_INDEX,
        crate::zstd_pack::INDEX_MAGIC,
        dev,
    );
    entry(
        dir.to_path_buf(),
        Format::Folder,
        &param,
        cover,
        (packed, zstd),
        no_eboot,
    )
}

/// An image's entry. Opening reads the image's own metadata, so this costs more than a folder.
fn probe_image(path: &Path, kind: Kind) -> Result<Option<LibraryEntry>, String> {
    // A patch package is no game by itself.
    if kind == Kind::Pkg && ps5_dump_forge_fpkg::is_patch(path) {
        return Ok(None);
    }
    let mut tree = crate::convert::open_source(path, kind, &AtomicBool::new(false))
        .map_err(|e| format!("skipped, cannot open: {e}"))?;
    let size = |name: &str| tree.files().iter().find(|f| f.path == name).map(|f| f.size);
    let (cover, param_size) = (size("sce_sys/icon0.png"), size("sce_sys/param.json"));
    let no_eboot = size("eboot.bin").is_none();
    if param_size.is_none_or(|s| s == 0 || s > MAX_PARAM_JSON) {
        return Err("skipped, sce_sys/param.json is missing or too big".to_string());
    }
    let param = tree
        .read("sce_sys/param.json")
        .map_err(|e| format!("skipped, param.json: {e}"))?;
    let format = match kind {
        Kind::Folder => Format::Folder,
        Kind::Exfat => Format::Exfat,
        Kind::Ffpkg => Format::Ffpkg,
        Kind::Ffpfs => Format::Ffpfs,
        Kind::Ffpfsc => Format::Ffpfsc,
        Kind::Pkg => Format::Pkg,
        // `image_kind` names none: the Library does not list archives.
        Kind::Archive(_) => return Ok(None),
    };
    let has_cover = cover.is_some_and(|s| (1..=MAX_LIBRARY_COVER).contains(&s));
    // The image is open already: its file list, then 8 bytes of the manifest when listed.
    let packed = ps5_dump_forge_lz4::reader::detect(tree.as_mut()).unwrap_or(false);
    let zstd = crate::zstd_pack::packed(tree.as_mut());
    entry(
        path.to_path_buf(),
        format,
        &param,
        has_cover,
        (packed, zstd),
        no_eboot,
    )
    .map(Some)
    .ok_or_else(|| "skipped, param.json has no usable title ID".to_string())
}

/// The cover of a supported image or plain game folder as a PNG data URL. `None` when the
/// icon is absent, empty, over 512 KiB (before or after reading) or not a PNG; an error when
/// the source is a link, is under ShadowMountPlus's mounts, or cannot be opened. No locality
/// check: a cover is one read of a path the page listed, which may be from a chosen network
/// folder.
pub(crate) fn library_cover(path: &Path) -> anyhow::Result<Option<String>> {
    cover_with(path, MAX_LIBRARY_COVER, &explicit)
}

/// The classifier for an explicitly chosen folder: locality is ignored, only ShadowMountPlus's
/// mounts are refused.
fn explicit(path: &Path) -> Result<(), String> {
    if shadowmnt(path) {
        return Err("is under ShadowMountPlus's mounts".to_string());
    }
    Ok(())
}

/// A root or cover path must not itself be a link, and both the path and its canonical form
/// (every link among its ancestors resolved) must pass the classifier.
fn eligible(path: &Path, classify: Classify<'_>) -> Result<(), String> {
    classify(path)?;
    let meta = path.symlink_metadata().map_err(|e| e.to_string())?;
    if is_link(meta.file_type(), || Some(meta.clone())) {
        return Err("is a link".to_string());
    }
    let real = path.canonicalize().map_err(|e| e.to_string())?;
    classify(&real)
}

fn cover_with(path: &Path, max: u64, classify: Classify<'_>) -> anyhow::Result<Option<String>> {
    use anyhow::{anyhow, bail};
    if crate::net::is_network(path) {
        return net_cover(path, max);
    }
    eligible(path, classify).map_err(|why| anyhow!("{}: {why}", path.display()))?;
    let meta = path.symlink_metadata()?;
    // The same five extensions as the scan (never `.ufs2`).
    let kind = if meta.is_dir() {
        Kind::Folder
    } else {
        path.file_name()
            .and_then(|n| n.to_str())
            .and_then(image_kind)
            .ok_or_else(|| anyhow!("{} is not a supported image", path.display()))?
    };
    let png = if kind == Kind::Folder {
        let dev = path_id(path)?.0.0;
        if !plain_dir(&path.join("sce_sys"), dev) {
            return Ok(None);
        }
        folder_file(path, "sce_sys/icon0.png", max, dev)
    } else {
        if !meta.is_file() {
            bail!("{} is not a regular file", path.display());
        }
        let folder = path.parent().filter(|p| !p.as_os_str().is_empty());
        let file_dev = path_id(path)?.0.0;
        if folder.is_none_or(|p| path_id(p).map(|i| i.0.0).ok() != Some(file_dev)) {
            bail!(
                "{} is on another filesystem than its folder",
                path.display()
            );
        }
        let mut tree = crate::convert::open_source(path, kind, &AtomicBool::new(false))?;
        const ICON: &str = "sce_sys/icon0.png";
        let size = tree.files().iter().find(|f| f.path == ICON).map(|f| f.size);
        match size {
            Some(s) if (1..=max).contains(&s) => {
                tree.read(ICON).ok().filter(|b| b.len() as u64 <= max)
            }
            _ => None,
        }
    };
    Ok(png.and_then(|b| crate::inspect::png_data_url(&b)))
}

/// [`cover_with`] for an `smb://` or `flink://` folder or image: no links or mounts to check
/// on a share or a peer (a peer leaves out links and special files).
fn net_cover(path: &Path, max: u64) -> anyhow::Result<Option<String>> {
    const ICON: &str = "sce_sys/icon0.png";
    let png = match Kind::of(path)? {
        Kind::Folder => crate::net::read_small(path, ICON, max)?,
        kind => {
            let mut tree = crate::convert::open_source(path, kind, &AtomicBool::new(false))?;
            let size = tree.files().iter().find(|f| f.path == ICON).map(|f| f.size);
            match size {
                Some(s) if (1..=max).contains(&s) => tree.read(ICON).ok(),
                _ => None,
            }
        }
    };
    Ok(png
        .filter(|b| !b.is_empty() && b.len() as u64 <= max)
        .and_then(|b| crate::inspect::png_data_url(&b)))
}

/// Probes every candidate on up to four threads; results in candidate order. A panic skips
/// that candidate with a warning.
fn probe_all<T: Send>(
    candidates: &[(PathBuf, Kind)],
    limits: &Limits,
    start: Instant,
    probe: fn(&Path, Kind) -> Result<Option<T>, String>,
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
                        std::panic::catch_unwind(|| probe(&candidate.0, candidate.1))
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

pub(crate) fn scan_library(roots: &[PathBuf]) -> anyhow::Result<LibraryScan> {
    let _guard = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    if roots.is_empty() {
        return Ok(scan(roots, &LIMITS, &|_| Ok(())));
    }
    #[cfg(not(target_os = "freebsd"))]
    anyhow::bail!("automatic Library discovery runs only on the PS5");
    #[cfg(target_os = "freebsd")]
    Ok(scan(roots, &LIMITS, &ps5_classify))
}

/// Lists the game dumps under one folder the user chose, like [`scan_library`] but on any
/// filesystem, network shares and mounts nested inside it included (an explicit choice;
/// automatic discovery stays local). ShadowMountPlus's mounts are still refused.
pub(crate) fn scan_library_folder(root: &Path) -> anyhow::Result<LibraryScan> {
    refuse_folder(root)?;
    // A chosen folder may itself be a link (`/tmp`, `/var`): the walk starts at its target.
    // Links met during the walk are still not followed.
    let linked = root
        .symlink_metadata()
        .is_ok_and(|m| is_link(m.file_type(), || Some(m.clone())));
    if linked {
        return scan_chosen(&plain_spelling(root.canonicalize()?));
    }
    scan_chosen(root)
}

/// Windows' `canonicalize` spells `C:\x` as `\\?\C:\x` and `\\server\share` as
/// `\\?\UNC\server\share`; entry paths use the plain spellings. Other paths are unchanged.
fn plain_spelling(path: PathBuf) -> PathBuf {
    let Some(s) = path.to_str() else { return path };
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// Errors when the chosen folder (canonical) is a system one.
fn refuse_folder(root: &Path) -> anyhow::Result<()> {
    match folder_refusal(root) {
        Some(why) => anyhow::bail!(why),
        None => Ok(()),
    }
}

/// Why `path` is a folder (or inside one) the Library refuses as a chosen folder, as its
/// error says it; `None` when allowed, and always on the PS5. Compared canonical: a path that
/// isn't there yet (a move's target) by its folder's canonical path and its name, one whose
/// folder isn't there either as typed.
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

/// The walk of a chosen folder. The folder itself must be a readable directory (an error:
/// a vanished drive is not an empty library); failures below it are warnings.
fn scan_chosen(root: &Path) -> anyhow::Result<LibraryScan> {
    let _guard = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    // Checked under the lock, and again by the walk: a drive that vanished while this scan
    // waited is an error, not an empty library.
    eligible(root, &explicit).map_err(|why| anyhow::anyhow!("{}: {why}", root.display()))?;
    match scan_inner(&[root.to_path_buf()], &LIMITS, &explicit) {
        (_, Some(why)) => anyhow::bail!(why),
        (found, None) => Ok(found),
    }
}

fn scan(roots: &[PathBuf], limits: &Limits, classify: Classify<'_>) -> LibraryScan {
    scan_inner(roots, limits, classify).0
}

fn scan_inner(
    roots: &[PathBuf],
    limits: &Limits,
    classify: Classify<'_>,
) -> (LibraryScan, Option<String>) {
    let (mut walk, accepted) = walk(roots, limits, classify, false);
    let results = probe_all(&walk.candidates, limits, walk.start, probe_image);
    let mut entries = std::mem::take(&mut walk.folders);
    for (candidate, result) in walk.candidates.iter().zip(results) {
        let path = &candidate.0;
        match result {
            Ok(Some(e)) => entries.push(e),
            Ok(None) => {}
            Err(why) => {
                // Images the budget never reached are missing from the list.
                walk.truncated |= why == NOT_PROBED;
                walk.warnings.add(format!("{}: {why}", path.display()))
            }
        }
    }
    if entries.len() > limits.games {
        entries.truncate(limits.games);
        walk.truncated = true;
    }
    entries.sort_by_cached_key(|e| {
        let shown = e
            .title_name
            .as_deref()
            .unwrap_or(&e.title_id)
            .to_lowercase();
        (shown, e.title_id.clone(), e.version.clone(), e.path.clone())
    });
    let root_error = walk.root_error.take();
    let found = LibraryScan {
        roots: accepted.into_iter().map(|(r, _)| r).collect(),
        entries,
        truncated: walk.truncated,
        warnings: walk.warnings.finish(),
    };
    (found, root_error)
}

/// Walks the eligible `roots`: the walk (its candidates, folders and findings) and the roots
/// accepted, one per identity in the order given.
fn walk<'a>(
    roots: &[PathBuf],
    limits: &'a Limits,
    classify: Classify<'a>,
    packages: bool,
) -> (Walk<'a>, Vec<(PathBuf, FileId)>) {
    let mut walk = Walk {
        limits,
        classify,
        home: home(),
        start: Instant::now(),
        candidates: Vec::new(),
        folders: Vec::new(),
        warnings: Warnings::default(),
        truncated: false,
        seen_dirs: HashSet::new(),
        seen_images: HashSet::new(),
        root_ids: HashSet::new(),
        root_error: None,
        image_probes: 0,
        packages,
    };
    // Accepted roots, in the supplied order one per identity.
    let mut accepted = Vec::new();
    for root in roots {
        let id = match (eligible(root, classify), root.to_str(), path_id(root)) {
            (Err(why), ..) => Err(why),
            (_, None, _) => Err("not a UTF-8 path".to_string()),
            (_, _, Err(e)) => Err(e.to_string()),
            (_, _, Ok((id, _))) => Ok(id),
        };
        // Walk the canonical spelling so every child and shadowmnt check is canonical (`..` or
        // ancestor links in the typed path). Not on Windows: `\\?\C:\` spellings would leak into
        // entry paths; ShadowMountPlus's mounts exist only on FreeBSD anyway.
        #[cfg(not(windows))]
        let root = &root.canonicalize().unwrap_or_else(|_| root.clone());
        match id {
            Ok(id) if walk.root_ids.insert(id) => accepted.push((root.clone(), id)),
            Ok(_) => {}
            Err(why) => {
                walk.root_error
                    .get_or_insert(format!("{}: {why}", root.display()));
                walk.warnings.add(format!("{}: {why}", root.display()))
            }
        }
    }
    for (root, id) in &accepted {
        walk.seen_dirs.insert(*id);
        walk.root(root, *id);
    }
    (walk, accepted)
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

/// One `.pkg` the package scan found. The title fields come from the container's plaintext
/// `param.json`, so a package whose container can't be read is listed with none of them.
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
fn probe_package(path: &Path, _: Kind) -> Result<Option<PackageEntry>, String> {
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
        .is_some_and(|s| (1..=MAX_LIBRARY_COVER).contains(&s));
    let info = parse_param(peek.param_json.as_deref().unwrap_or_default());
    if let Some(json) = info.param_json.as_ref() {
        entry.title_name = crate::inspect::title_name(json);
        entry.title_names = crate::inspect::title_names(json);
        entry.version = (json.get("contentVersion").and_then(|v| v.as_str())).map(str::to_string);
    }
    entry.title_id = info.title_id;
    Ok(Some(entry))
}

/// Every `.pkg` under `roots`, walked as the Library walks (game folders entered too).
fn packages(roots: &[PathBuf], limits: &Limits, classify: Classify<'_>) -> PackageScan {
    let (mut walk, _) = walk(roots, limits, classify, true);
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
// ponytail: the buffers come back together from `read_files` (no per-file callback), so
// this is also the peak; real `param.json` files are a few KiB each.
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

/// The game folder inside an archive: `""` (its root) or `"Game/"`. Recognized as a folder is
/// here: `eboot.bin` beside `sce_sys/param.json` (1 B..4 MiB) that parses to a title ID, the
/// outermost one (a game is not entered), `__MACOSX` and junk names skipped. The listing
/// alone decides when it names one candidate; with several, their `param.json` are read.
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
    // No game: additional content (param.json without eboot.bin, by user decision).
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

    #[cfg(unix)]
    fn run(roots: &[&Path], limits: &Limits) -> LibraryScan {
        let roots: Vec<PathBuf> = roots.iter().map(|r| r.to_path_buf()).collect();
        scan(&roots, limits, &|_| Ok(()))
    }

    #[cfg(unix)]
    fn paths(scan: &LibraryScan) -> Vec<&Path> {
        scan.entries.iter().map(|e| e.path.as_path()).collect()
    }

    #[cfg(unix)]
    const OK: &str = r#"{"titleId":"PPSA00001"}"#;

    #[cfg(unix)]
    #[test]
    fn linked_root_and_linked_ancestor_are_refused() {
        let real = tmp("link-real");
        let g = game(&real, "g", OK);
        let base = tmp("link-base");
        let link = base.join("l");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let scan = run(&[&link], &LIMITS);
        assert!(scan.roots.is_empty() && scan.entries.is_empty());
        assert!(scan.warnings[0].contains("is a link"));
        // A cover below a linked ancestor: the classifier sees the real location.
        let real_c = real.canonicalize().unwrap();
        let deny = move |p: &Path| {
            if p.starts_with(&real_c) {
                Err("network".to_string())
            } else {
                Ok(())
            }
        };
        let via = base.join("l/g");
        let err = cover_with(&via, MAX_LIBRARY_COVER, &deny).unwrap_err();
        assert!(err.to_string().contains("network"));
        assert!(g.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_chosen_link_is_followed_unless_it_points_into_a_system_folder() {
        // Under the build folder: the temp dir is a refused system folder on macOS.
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/forge-chosen-link-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let g = game(&base, "real/g", r#"{"titleId":"PPSA00001"}"#);
        let link = base.join("alias");
        std::os::unix::fs::symlink(base.join("real"), &link).unwrap();
        let found = scan_library_folder(&link).unwrap();
        assert_eq!(paths(&found), [g.as_path()]);
        assert_eq!(found.roots, [base.join("real")]);
        let usr = base.join("usr-alias");
        std::os::unix::fs::symlink("/usr", &usr).unwrap();
        // FreeBSD (the PS5) has no system-folder list.
        if Os::HOST.is_some() {
            let why = scan_library_folder(&usr).unwrap_err().to_string();
            assert!(why.contains("system folder"), "{why}");
        }
        let _ = fs::remove_dir_all(&base);
    }

    /// The PS5's automatic scan with the real `statfs` classifier (run by scripts/test-freebsd.sh).
    #[cfg(target_os = "freebsd")]
    #[test]
    fn ps5_scan_uses_the_statfs_classifier() {
        let root = tmp("ps5-scan");
        let g = game(&root, "g", r#"{"titleId":"PPSA00001"}"#);
        let found = scan_library(std::slice::from_ref(&root)).unwrap();
        assert_eq!(paths(&found), [g.as_path()]);
        let found = scan_library(&[PathBuf::from("/mnt/shadowmnt")]).unwrap();
        assert!(found.roots.is_empty() && !found.warnings.is_empty());
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
