//! Checks made before anything is written. Each returns findings, so a
//! job can report every problem at once instead of failing on the first.

#[cfg(not(target_os = "freebsd"))]
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, bail};
use ps5upload_fpkg::source::{SourceTree, title_id_from_content_id};
use serde_json::Value;

use crate::Format;
use crate::finalize::followed_id;

/// The largest file FAT32 (`msdos`) can hold.
pub(crate) const FAT_MAX_FILE: u64 = 4 * 1024 * 1024 * 1024 - 1;
const MIB: u64 = 1024 * 1024;
/// A `param.json` is a few KiB; anything bigger is not one, and is not read into memory.
pub(crate) const MAX_PARAM_JSON: u64 = 4 * MIB;
// ponytail: a findings list is capped per kind so a folder of 100k bad names still makes a
// readable message; the count of the rest is given.
const MAX_LISTED: usize = 200;

/// `  kind: path` lines for `items`, capped at `MAX_LISTED`.
pub(crate) fn listing(kind: &str, items: &[String]) -> Vec<String> {
    let mut out: Vec<String> = items
        .iter()
        .take(MAX_LISTED)
        .map(|p| format!("  {kind}: {p}"))
        .collect();
    if items.len() > MAX_LISTED {
        out.push(format!(
            "  {kind}: ... and {} more",
            items.len() - MAX_LISTED
        ));
    }
    out
}

/// What the source's `param.json` says about the game.
#[derive(Debug, Default)]
pub(crate) struct GameInfo {
    pub param_json: Option<Value>,
    pub title_id: Option<String>,
    pub content_id: Option<String>,
    /// No `eboot.bin` at the root but a `param.json` with a title id: a DLC, an update or
    /// another package (by user decision; not a finding).
    pub additional_content: bool,
}

/// The finding for a source without `eboot.bin` that isn't additional content either.
pub(crate) const NO_EBOOT: &str = "eboot.bin is missing at the game root";

/// The finding for additional content converted to a target that needs a game.
pub(crate) const ADDITIONAL_CONTENT_TARGET: &str =
    "additional content (no eboot.bin) can only be packaged as fpkg or written to a folder";

/// Reads `sce_sys/param.json` (when there is one) and checks the game root's layout.
pub(crate) fn input(tree: &mut dyn SourceTree) -> (GameInfo, Vec<String>) {
    let mut findings = Vec::new();
    let has = |path: &str| tree.files().iter().find(|f| f.path == path).map(|f| f.size);
    let no_eboot = has("eboot.bin").is_none();
    let param_size = has("sce_sys/param.json");
    let mut info = GameInfo::default();
    match param_size {
        None => findings.push("sce_sys/param.json is missing".to_string()),
        Some(size) if size > MAX_PARAM_JSON => findings.push(format!(
            "sce_sys/param.json is {size} bytes, too big to be one"
        )),
        Some(_) => match tree.read("sce_sys/param.json") {
            Ok(bytes) => info = parse_param(&bytes),
            Err(e) => findings.push(format!("sce_sys/param.json: {e}")),
        },
    }
    if param_size.is_some_and(|s| s <= MAX_PARAM_JSON) {
        if info.param_json.is_none() {
            findings.push("sce_sys/param.json is not valid JSON".to_string());
        } else if info.title_id.is_none() {
            findings.push(
                "sce_sys/param.json has no usable titleId, and none follows from its contentId"
                    .to_string(),
            );
        }
    }
    info.additional_content = no_eboot && info.title_id.is_some();
    if no_eboot && !info.additional_content {
        findings.insert(0, NO_EBOOT.to_string());
    }
    (info, findings)
}

pub(crate) fn parse_param(bytes: &[u8]) -> GameInfo {
    let text = String::from_utf8_lossy(bytes);
    let Ok(json) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return GameInfo::default();
    };
    let field = |name: &str| json.get(name).and_then(Value::as_str).map(str::to_string);
    let content_id = field("contentId");
    let title_id = field("titleId")
        .filter(|id| is_title_id(id))
        .or_else(|| content_id.as_deref().and_then(title_from_content_id));
    GameInfo {
        param_json: Some(json),
        title_id,
        content_id,
        additional_content: false,
    }
}

/// The title id a content id names, when it is a plain one.
pub(crate) fn title_from_content_id(content_id: &str) -> Option<String> {
    title_id_from_content_id(content_id)
        .filter(|id| is_title_id(id))
        .map(str::to_string)
}

/// A title id becomes a file name, and `param.json` is untrusted: only plain ids pass
/// (`PPSA01234`), never anything with a separator or a dot in it.
fn is_title_id(id: &str) -> bool {
    (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The output as an absolute path whose parent exists. The `.part` goes next to it.
pub(crate) fn output_path(output: &Path) -> anyhow::Result<PathBuf> {
    let Some(name) = output.file_name() else {
        bail!("{} does not name a file or folder", output.display());
    };
    let parent = match output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let parent = parent
        .canonicalize()
        .with_context(|| format!("output folder {}", parent.display()))?;
    if !parent.is_dir() {
        bail!("{} is not a folder", parent.display());
    }
    Ok(parent.join(name))
}

/// The extension that makes ShadowMountPlus pick the right driver.
pub(crate) fn extension(format: Format) -> Option<&'static str> {
    match format {
        // An LZ4 packed folder is a folder: SMP mounts nothing, so no extension.
        Format::Folder | Format::Lz4 => None,
        Format::Exfat => Some("exfat"),
        Format::Ffpkg => Some("ffpkg"),
        Format::Ffpfs => Some("ffpfs"),
        Format::Ffpfsc => Some("ffpfsc"),
        Format::Pkg => Some("pkg"),
    }
}

/// The longest output name for `format` (UTF-8 bytes, the extension and its dot excluded, a
/// `-N` included): what ShadowMountPlus mounts an image from, and the same 63 bytes for a folder
/// and a `.pkg` (not image-mounted), so every output follows one rule.
// SMP (drakmor/ShadowMountPlus src/sm_image.c, build_image_mount_point_for_fs) nmounts every
// image at `<base>/<stem>_<8 hex: fnv1a32 of the full path>`, the stem cut at the name's last
// dot, base `/mnt/shadowmnt` (`/mnt/shadowmnt/pfsc` for a `.ffpfsc` container). The PS5
// kernel is FreeBSD 11: nmount's fspath, NUL included, must fit MNAMELEN (88) or it fails with
// ENAMETOOLONG, so at most 87 bytes: 87 - 15 ("/mnt/shadowmnt/") - 9 ("_xxxxxxxx") = 63, and
// 87 - 20 ("/mnt/shadowmnt/pfsc/") - 9 = 58. A `.ffpfsc`'s inner image, `<TITLE_ID>.<ext>`,
// is mounted under `/mnt/shadowmnt` too and is always short.
pub(crate) fn stem_limit(format: Format) -> usize {
    const FSPATH_MAX: usize = 88 - 1; // MNAMELEN, without its NUL
    const HASH: usize = "_xxxxxxxx".len();
    match format {
        Format::Ffpfsc => FSPATH_MAX - "/mnt/shadowmnt/pfsc/".len() - HASH,
        _ => FSPATH_MAX - "/mnt/shadowmnt/".len() - HASH,
    }
}

/// `output` is absolute (from [`output_path`]).
pub(crate) fn output(source: &Path, output: &Path, format: Format) -> Vec<String> {
    let mut findings = Vec::new();
    if output.symlink_metadata().is_ok() {
        findings.push(format!("{} already exists", output.display()));
    }
    if let Some(ext) = extension(format) {
        let got = output.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !got.eq_ignore_ascii_case(ext) {
            findings.push(format!(
                "{} must end in .{ext}: the extension picks the mount driver",
                output.display()
            ));
        }
    }
    findings.extend(stem_finding(output, format));
    if let Ok(source) = source.canonicalize()
        && source.is_dir()
        && inside(output, &source)
    {
        findings.push(format!(
            "{} is inside the source folder {}",
            output.display(),
            source.display()
        ));
    }
    findings
}

/// Why `path`'s name is too long for a `format` output ([`stem_limit`]), if it is.
pub(crate) fn stem_finding(path: &Path, format: Format) -> Option<String> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    // A file's stem as SMP takes it: up to the last dot; a folder's whole name.
    let stem = match format {
        Format::Folder | Format::Lz4 => &*name,
        _ => name.rsplit_once('.').map_or(&*name, |(stem, _)| stem),
    };
    let max = stem_limit(format);
    (stem.len() > max).then(|| match extension(format) {
        Some(ext @ ("exfat" | "ffpkg" | "ffpfs" | "ffpfsc")) => format!(
            "{name} is {} bytes long without its .{ext}: ShadowMountPlus fails to mount a \
                 .{ext} whose name is over {max} bytes (UTF-8, extension excluded), as its mount \
                 point would pass the PS5's 87-byte limit",
            stem.len()
        ),
        Some(ext) => format!(
            "{name} is {} bytes long without its .{ext}: output names are at most {max} \
                 bytes (UTF-8, extension excluded)",
            stem.len()
        ),
        None => format!(
            "{name} is {} bytes long: output names are at most {max} bytes (UTF-8)",
            stem.len()
        ),
    })
}

/// Whether `path` (absolute) lies inside the folder `dir`. Compared by identity, not by
/// spelling, because a case-insensitive volume accepts `/Games/X` for `/games/x`.
fn inside(path: &Path, dir: &Path) -> bool {
    if path.starts_with(dir) {
        return true;
    }
    let Ok(dir) = followed_id(dir) else {
        return false;
    };
    path.ancestors()
        .skip(1)
        .any(|a| followed_id(a).is_ok_and(|id| id == dir))
}

/// Directory levels below the root an `.exfat`/`.ffpkg` output can hold.
// ponytail: the vendored readers' walkers stop at MAX_DEPTH = 32 (vendor/ps5upload-fpkg
// src/exfat.rs, src/ufs2_source.rs); our writers would go deeper, but nothing could read the
// image back (verification, extraction), so deeper trees are refused up front.
pub(crate) const MAX_DIR_DEPTH: usize = 32;

/// Paths nested deeper than [`MAX_DIR_DEPTH`] directories: a file's directories are the
/// components before its name, an empty directory's are all of its own.
pub(crate) fn too_deep(tree: &dyn SourceTree) -> Vec<String> {
    let levels = |p: &str| p.split('/').count();
    let deep: Vec<String> = tree
        .files()
        .iter()
        .filter(|f| levels(&f.path) - 1 > MAX_DIR_DEPTH)
        .map(|f| f.path.clone())
        .chain(
            tree.empty_dirs()
                .iter()
                .filter(|d| levels(d) > MAX_DIR_DEPTH)
                .cloned(),
        )
        .collect();
    let kind = format!("nested deeper than {MAX_DIR_DEPTH} directories (image readers' limit)");
    listing(&kind, &deep)
}

/// Bytes the output needs on disk, before the writer's own plan is known.
pub(crate) fn estimate(tree: &dyn SourceTree) -> u64 {
    let files = tree
        .files()
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    files.saturating_add(files / 50).saturating_add(64 * MIB)
}

/// What [`destination`] found: findings refuse the job, notes are logged.
pub(crate) struct Destination {
    pub findings: Vec<String>,
    pub notes: Vec<String>,
    /// The folder's free bytes, when they could be read.
    pub free: Option<u64>,
    /// What the probe saw, for folder findings and publishing; None when it failed (then
    /// `findings` says why).
    #[cfg(target_os = "freebsd")]
    pub dest: Option<crate::dest::Dest>,
}

/// Free space and the 4 GiB file limit, on the folder the output goes to. `largest` is the
/// biggest single file the output creates.
#[cfg(not(target_os = "freebsd"))]
pub(crate) fn destination(dir: &Path, need: u64, largest: u64, _: &AtomicBool) -> Destination {
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let mut room = None;
    match fs_stat(dir) {
        Ok((free, fat)) => {
            room = Some(free);
            if need > free {
                findings.push(format!(
                    "not enough space in {}: about {} MiB needed, {} MiB free",
                    dir.display(),
                    need.div_ceil(MIB),
                    free / MIB
                ));
            }
            match fat {
                Ok(true) if largest > FAT_MAX_FILE => findings.push(format!(
                    "{} is FAT32, which cannot hold a file over 4 GiB ({largest} bytes needed)",
                    dir.display()
                )),
                Ok(_) => {}
                Err(e) => notes.push(format!(
                    "filesystem type not checked for {}: {e}",
                    dir.display()
                )),
            }
        }
        Err(e) => notes.push(format!(
            "free space and filesystem type not checked for {}: {e}",
            dir.display()
        )),
    }
    Destination {
        findings,
        notes,
        free: room,
    }
}

/// The same checks on what the destination probe saw (U1 in ps5/README.md). A USB drive is
/// never tested on hardware, so this fails closed: a probe that fails refuses the job.
#[cfg(target_os = "freebsd")]
pub(crate) fn destination(dir: &Path, need: u64, largest: u64, cancel: &AtomicBool) -> Destination {
    let dest = match crate::dest::probe(dir, cancel) {
        Ok(dest) => dest,
        Err(e) => {
            return Destination {
                findings: vec![format!("{e:#}")],
                notes: Vec::new(),
                free: None,
                dest: None,
            };
        }
    };
    let mut findings = Vec::new();
    let mut notes = vec![dest.describe()];
    notes.extend(dest.notes.iter().cloned());
    if need > dest.free {
        // Sony's exFAT shows space a delete freed as used for a while (U6).
        findings.push(format!(
            "not enough space in {}: about {} MiB needed, {} MiB free (space a delete just \
             freed may still be being freed; try again in a minute)",
            dir.display(),
            need.div_ceil(MIB),
            dest.free / MIB
        ));
    }
    if largest > dest.max_file {
        let limit = if dest.max_file == FAT_MAX_FILE {
            "4 GiB".to_string()
        } else {
            format!("{} bytes", dest.max_file)
        };
        findings.push(format!(
            "{} ({}) cannot hold a file over {limit} ({largest} bytes needed)",
            dir.display(),
            dest.fstype
        ));
    }
    Destination {
        findings,
        notes,
        free: Some(dest.free),
        dest: Some(dest),
    }
}

/// Folder output's name findings on a destination that `folds` names, plus every non-ASCII
/// path when it refuses non-ASCII names (U2).
#[cfg(any(target_os = "freebsd", test))]
pub(crate) fn folder_names(
    files: &[String],
    empty_dirs: &[String],
    folds: bool,
    ascii_only: bool,
) -> Vec<String> {
    let mut findings = crate::extract::extraction_findings(files, empty_dirs, folds);
    if ascii_only {
        let bad: Vec<String> = files
            .iter()
            .chain(empty_dirs)
            .filter(|p| !p.is_ascii())
            .cloned()
            .collect();
        findings.extend(listing(
            "name not ASCII (the output folder refuses non-ASCII names)",
            &bad,
        ));
    }
    findings
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path holds a NUL byte"))
}

/// (free bytes for an unprivileged user, whether the filesystem is FAT). The FAT answer can
/// fail on its own (Windows asks the volume root separately).
#[cfg(target_os = "macos")]
fn fs_stat(dir: &Path) -> io::Result<(u64, io::Result<bool>)> {
    let c = c_path(dir)?;
    // SAFETY: `c` is a valid C string and `st` a properly sized out-parameter.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let name: Vec<u8> = st
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    let free = st.f_bavail.saturating_mul(u64::from(st.f_bsize));
    Ok((free, Ok(name == b"msdos")))
}

#[cfg(target_os = "linux")]
#[allow(clippy::unnecessary_cast)] // field widths differ between Linux targets
fn fs_stat(dir: &Path) -> io::Result<(u64, io::Result<bool>)> {
    const MSDOS_SUPER_MAGIC: i64 = 0x4d44;
    let c = c_path(dir)?;
    // SAFETY: `c` is a valid C string and both out-parameters are properly sized.
    let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut vfs) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut fs: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut fs) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let free = (vfs.f_bavail as u64).saturating_mul(vfs.f_frsize as u64);
    Ok((free, Ok(fs.f_type as i64 == MSDOS_SUPER_MAGIC)))
}

/// FAT12/16/32 report `FAT` or `FAT32`; exFAT has no 4 GiB limit.
#[cfg(windows)]
fn fs_stat(dir: &Path) -> io::Result<(u64, io::Result<bool>)> {
    let free = crate::win::free_bytes(dir)?;
    let fat = crate::win::fs_name(dir).map(|name| matches!(name.as_str(), "FAT" | "FAT32"));
    Ok((free, fat))
}

// ponytail: other OSes are not wired; the job logs that the checks were skipped and ENOSPC
// still fails the write. FreeBSD (the PS5) probes the folder instead.
#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    windows,
    target_os = "freebsd"
)))]
fn fs_stat(_: &Path) -> io::Result<(u64, io::Result<bool>)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "not implemented on this OS",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_id_from_param() {
        let info = parse_param(
            b"\xef\xbb\xbf{\"titleId\":\"PPSA01234\",\"contentId\":\"UP0000-PPSA01234_00-X\"}",
        );
        assert_eq!(info.title_id.as_deref(), Some("PPSA01234"));
        let info = parse_param(b"{\"contentId\":\"UP0000-PPSA05555_00-X\"}");
        assert_eq!(info.title_id.as_deref(), Some("PPSA05555"));
        let info = parse_param(b"{\"titleId\":\"../../etc\"}");
        assert_eq!(info.title_id, None);
        assert!(parse_param(b"not json").param_json.is_none());
    }

    #[test]
    fn folder_names_follow_the_probe() {
        let paths = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
        let pair = paths(&["Eboot.bin", "eboot.bin"]);
        assert_eq!(folder_names(&pair, &[], true, false).len(), 1);
        assert!(folder_names(&pair, &[], false, false).is_empty());
        // NFC and NFD spellings collide only where names fold.
        let nfc = paths(&["caf\u{e9}", "cafe\u{301}"]);
        assert_eq!(folder_names(&nfc, &[], true, false).len(), 1);
        assert!(folder_names(&nfc, &[], false, false).is_empty());
        let files = paths(&["eboot.bin", "sce_sys/caf\u{e9}.png", "\u{e9}/a"]);
        let dirs = paths(&["data/\u{fc}"]);
        assert!(folder_names(&files, &dirs, true, false).is_empty());
        let found = folder_names(&files, &dirs, false, true);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(
            found.iter().all(|l| l.contains("name not ASCII")),
            "{found:?}"
        );
        assert!(found[2].ends_with("data/\u{fc}"), "{found:?}");
    }

    /// CI's temp folder is on NTFS; the canonical form is a `\\?\` path.
    #[cfg(windows)]
    #[test]
    fn windows_free_space_and_fat() {
        let temp = std::env::temp_dir();
        let unicode = crate::test_dir("fs-stat-ünïcødé");
        for dir in [temp.clone(), temp.canonicalize().unwrap(), unicode] {
            let (free, fat) = fs_stat(&dir).unwrap();
            assert!(free > 0, "{}", dir.display());
            assert!(!fat.unwrap(), "{}", dir.display());
        }
    }
}
