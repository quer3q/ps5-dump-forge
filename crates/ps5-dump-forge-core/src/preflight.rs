//! Checks made before anything is written. Each returns findings, so a
//! job can report every problem at once instead of failing on the first.

use std::io;
use std::path::{Path, PathBuf};

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
}

/// Reads `sce_sys/param.json` (when there is one) and checks the game root's layout.
pub(crate) fn input(tree: &mut dyn SourceTree) -> (GameInfo, Vec<String>) {
    let mut findings = Vec::new();
    let has = |path: &str| tree.files().iter().find(|f| f.path == path).map(|f| f.size);
    if has("eboot.bin").is_none() {
        findings.push("eboot.bin is missing at the game root".to_string());
    }
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
        Format::Folder => None,
        Format::Exfat => Some("exfat"),
        Format::Ffpkg => Some("ffpkg"),
        Format::Ffpfs => Some("ffpfs"),
        Format::Ffpfsc => Some("ffpfsc"),
        Format::Pkg => Some("pkg"),
    }
}

/// The longest `.ffpfs`/`.ffpfsc` file name SMP mounts, in UTF-8 bytes, extension included.
// ponytail: empirical: PS5 UltraPack saw SMP's mount fail with ENAMETOOLONG above about 63
// bytes; no limit is known for `.exfat`/`.ffpkg`. Widen after a console test.
pub(crate) const MAX_PFS_NAME: usize = 63;

/// The name limit for `format`'s output file, when it has one.
pub(crate) fn name_limit(format: Format) -> Option<usize> {
    matches!(format, Format::Ffpfs | Format::Ffpfsc).then_some(MAX_PFS_NAME)
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
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    if let Some(max) = name_limit(format)
        && name.len() > max
    {
        findings.push(format!(
            "{name} is {} bytes long: ShadowMountPlus fails to mount a .{} whose name is over \
             {max} bytes (UTF-8, extension included)",
            name.len(),
            extension(format).unwrap_or_default()
        ));
    }
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

/// Free space and the 4 GiB file limit, on the folder the output goes to. `largest` is the
/// biggest single file the output creates.
pub(crate) fn destination(dir: &Path, need: u64, largest: u64) -> (Vec<String>, Vec<String>) {
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    match fs_stat(dir) {
        Ok((free, fat)) => {
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
    (findings, notes)
}

#[cfg(unix)]
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
// still fails the write.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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

    /// An image reader's tree: the root checks do not depend on the source being a folder.
    struct Mem(Vec<ps5upload_fpkg::source::SourceFile>, Vec<u8>);

    impl SourceTree for Mem {
        fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
            &self.0
        }
        fn read(&mut self, _: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            Ok(self.1.clone())
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    #[test]
    fn input_checks_any_tree() {
        let file = |path: &str| ps5upload_fpkg::source::SourceFile {
            path: path.into(),
            size: 20,
        };
        let mut tree = Mem(vec![file("data/eboot.bin")], Vec::new());
        let (_, findings) = input(&mut tree);
        assert_eq!(findings.len(), 2, "{findings:?}");
        let param = br#"{"titleId":"PPSA01234"}"#.to_vec();
        let mut tree = Mem(vec![file("eboot.bin"), file("sce_sys/param.json")], param);
        let (info, findings) = input(&mut tree);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(info.title_id.as_deref(), Some("PPSA01234"));
    }

    #[test]
    fn listing_caps() {
        let items: Vec<String> = (0..MAX_LISTED + 5).map(|i| i.to_string()).collect();
        let lines = listing("x", &items);
        assert_eq!(lines.len(), MAX_LISTED + 1);
        assert!(lines.last().unwrap().contains("5 more"));
    }

    #[test]
    fn free_space_is_known_here() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let (free, fat) = fs_stat(Path::new(".")).unwrap();
            assert!(free > 0);
            fat.unwrap();
        }
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
