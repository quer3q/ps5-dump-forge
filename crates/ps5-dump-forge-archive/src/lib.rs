//! Streaming, read-only ZIP, 7z and RAR archives. No external programs are used.
mod parts;
mod rar;
mod sevenz;
mod stream;
mod zip;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub use stream::Reader;

pub const ARCHIVE_PASSWORD_REQUIRED: &str = "This archive needs a password.";
pub const ARCHIVE_PASSWORD_WRONG: &str = "The password is not correct.";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    #[serde(rename = "rar")]
    Rar,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub dir: bool,
    /// Its data needs the password (a whole archive's encryption shows on every entry).
    #[serde(default)]
    pub encrypted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub format: ArchiveFormat,
    pub entries: Vec<Entry>,
    pub volumes: Vec<PathBuf>,
    pub archive_bytes: u64,
    pub solid: bool,
    pub encrypted: bool,
}
#[derive(Debug)]
pub enum ArchiveError {
    PasswordRequired,
    PasswordWrong,
    NotFirstPart(String),
    MissingPart(String),
    Unsupported(String),
    Corrupt(String),
    Cancelled,
    Io(io::Error),
}
impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordRequired => f.write_str(ARCHIVE_PASSWORD_REQUIRED),
            Self::PasswordWrong => f.write_str(ARCHIVE_PASSWORD_WRONG),
            Self::NotFirstPart(n) => write!(f, "Choose the first part: {n}"),
            Self::MissingPart(n) => write!(f, "Missing part: {n}"),
            Self::Unsupported(s) | Self::Corrupt(s) => f.write_str(s),
            Self::Cancelled => f.write_str("Cancelled."),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ArchiveError {}
impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
fn corrupt(s: &str) -> ArchiveError {
    ArchiveError::Corrupt(s.into())
}
fn check(cancel: &AtomicBool) -> Result<(), ArchiveError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ArchiveError::Cancelled)
    } else {
        Ok(())
    }
}

/// Detects archive names without opening the file (including later volumes).
pub fn detect(path: &Path) -> Option<ArchiveFormat> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let base = if let Some((base, suffix)) = name.rsplit_once('.') {
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
            base
        } else {
            &name
        }
    } else {
        &name
    };
    if base.ends_with(".zip")
        || name.rsplit_once('.').is_some_and(|(_, s)| {
            s.len() >= 3 && s.starts_with('z') && s[1..].bytes().all(|b| b.is_ascii_digit())
        })
    {
        Some(ArchiveFormat::Zip)
    } else if base.ends_with(".7z") {
        Some(ArchiveFormat::SevenZ)
    } else if name.ends_with(".rar")
        || name.rsplit_once('.').is_some_and(|(_, s)| {
            // Old-style volumes: .r00-.r99, then .s00 and on (.zNN is zip's).
            s.len() == 3
                && (b'r'..=b'y').contains(&s.as_bytes()[0])
                && s[1..].bytes().all(|b| b.is_ascii_digit())
        })
    {
        Some(ArchiveFormat::Rar)
    } else {
        None
    }
}

/// Whether `path` names, by its name alone, a later part of the set `first` starts. A RAR
/// `.partN.rar` number is compared as a number: `x.part10.rar` follows `x.part1.rar`.
pub fn later_part_of(path: &Path, first: &Path) -> bool {
    let key = |name: &str| {
        let lower = name.to_ascii_lowercase();
        match lower
            .strip_suffix(".rar")
            .and_then(|s| s.rsplit_once(".part"))
        {
            Some((stem, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                format!("{stem}.part{}.rar", n.trim_start_matches('0'))
            }
            _ => lower,
        }
    };
    let Some(first) = first.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    // A `.rar` set's old-style volumes run .r00 to .z99; `.zNN` alone is a split zip's name.
    let format = if first.to_ascii_lowercase().ends_with(".rar") {
        Some(ArchiveFormat::Rar)
    } else {
        detect(path)
    };
    let name = path.file_name().and_then(|n| n.to_str());
    (name.zip(format))
        .and_then(|(name, format)| parts::first_of(name, format))
        .is_some_and(|f| key(&f) == key(first))
}

// Validate before writing anything, including entries outside the selected prefix.
fn safe_path(name: &str, dir: bool) -> Result<String, ArchiveError> {
    let name = if dir {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if name.is_empty()
        || name.contains(['\\', '\0', ':'])
        || name
            .split('/')
            .any(|s| s.is_empty() || s == ".." || s == ".")
    {
        return Err(corrupt("Archive contains an unsafe path."));
    }
    Ok(name.to_owned())
}
fn special(mode: u32, dir: bool) -> Result<(), ArchiveError> {
    let kind = mode & 0o170000;
    if kind != 0 && kind != if dir { 0o040000 } else { 0o100000 } {
        return Err(ArchiveError::Unsupported(
            "Archive links and special files are not supported.".into(),
        ));
    }
    Ok(())
}
fn validate(entries: &[Entry]) -> Result<(), ArchiveError> {
    let mut names = BTreeMap::new();
    for e in entries {
        safe_path(&e.path, e.dir)?;
        if names.insert(&e.path, e.dir).is_some() {
            return Err(corrupt("Archive contains duplicate paths."));
        }
    }
    for e in entries {
        let mut parent = e.path.as_str();
        while let Some((p, _)) = parent.rsplit_once('/') {
            if names.get(&p.to_owned()) == Some(&false) {
                return Err(corrupt("Archive file is also used as a directory."));
            }
            parent = p;
        }
    }
    Ok(())
}
pub fn list(path: &Path, password: Option<&str>) -> Result<Listing, ArchiveError> {
    let format = detect(path)
        .ok_or_else(|| ArchiveError::Unsupported("Unsupported archive format.".into()))?;
    let volumes = parts::discover(path, format)?;
    let mut result = match format {
        ArchiveFormat::Zip => zip::list(&volumes)?,
        ArchiveFormat::SevenZ => sevenz::list(&volumes, password)?,
        ArchiveFormat::Rar => rar::list(path, password)?,
    };
    validate(&result.entries)?;
    if result.volumes.is_empty() {
        result.volumes = volumes;
    }
    result.archive_bytes = result.volumes.iter().try_fold(0u64, |n, p| {
        n.checked_add(fs::metadata(p)?.len())
            .ok_or_else(|| corrupt("Archive is too large."))
    })?;
    Ok(result)
}

trait Sink {
    fn wants(&mut self, path: &str) -> bool;
    fn begin(&mut self, entry: &Entry) -> Result<bool, ArchiveError>;
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), ArchiveError>;
    fn end(&mut self) -> Result<(), ArchiveError>;
    fn done(&self) -> bool;
    /// The current entry has all this sink wants of it (a ZIP walk may stop decoding it).
    fn enough(&self) -> bool {
        false
    }
    /// Reads whole files front to back (a job's session), so a 7z block may decode on several
    /// threads; small reads (Inspect, naming, covers, password probes) stay on one.
    fn streams(&self) -> bool {
        false
    }
}
fn walk(
    path: &Path,
    password: Option<&str>,
    listing: &Listing,
    sink: &mut dyn Sink,
    cancel: &AtomicBool,
) -> Result<(), ArchiveError> {
    check(cancel)?;
    if sink.done() {
        return Ok(());
    }
    match listing.format {
        ArchiveFormat::Zip => zip::walk(&listing.volumes, sink, cancel),
        ArchiveFormat::SevenZ => sevenz::walk(&listing.volumes, password, sink, cancel),
        ArchiveFormat::Rar => rar::walk(path, password, sink, cancel),
    }
}
/// Reads only selected files. `max_bytes` limits the sum of their unpacked sizes.
pub fn read_files(
    path: &Path,
    password: Option<&str>,
    wanted: &dyn Fn(&str) -> bool,
    max_bytes: u64,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    read_heads(
        path,
        password,
        &|p, size| wanted(p).then_some(size),
        max_bytes,
        cancel,
    )
}
/// Like [`read_files`], keeping only the first `keep(path, size)` bytes of each file (`None`
/// or `Some(0)` of a non-empty file: not read). `max_bytes` limits the sum kept. A ZIP entry stops decoding once its head is read (its
/// CRC is then not checked); 7z and RAR entries are decoded and checked whole, the rest dropped.
pub fn read_heads(
    path: &Path,
    password: Option<&str>,
    keep: &dyn Fn(&str, u64) -> Option<u64>,
    max_bytes: u64,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    check(cancel)?;
    let listing = list(path, password)?;
    let selected: BTreeMap<_, _> = listing
        .entries
        .iter()
        .filter(|e| !e.dir)
        .filter_map(|e| {
            let n = keep(&e.path, e.size)?.min(e.size);
            (n > 0 || e.size == 0).then(|| (e.path.clone(), n))
        })
        .collect();
    let total = selected.values().try_fold(0u64, |n, k| {
        n.checked_add(*k)
            .ok_or_else(|| corrupt("Selected archive files exceed the byte limit."))
    })?;
    if total > max_bytes {
        return Err(ArchiveError::Unsupported(
            "Selected archive files exceed the byte limit.".into(),
        ));
    }
    let mut sink = Memory {
        selected,
        files: BTreeMap::new(),
        current: String::new(),
        keep: 0,
        expected: 0,
        seen: 0,
    };
    walk(path, password, &listing, &mut sink, cancel)?;
    if !sink.done() {
        return Err(corrupt("Selected archive entries are missing."));
    }
    Ok(sink.files)
}
struct Memory {
    /// Path and the bytes to keep, until read.
    selected: BTreeMap<String, u64>,
    files: BTreeMap<String, Vec<u8>>,
    current: String,
    keep: u64,
    expected: u64,
    seen: u64,
}
impl Sink for Memory {
    fn wants(&mut self, p: &str) -> bool {
        self.selected.contains_key(p)
    }
    fn begin(&mut self, e: &Entry) -> Result<bool, ArchiveError> {
        let Some(&keep) = self.selected.get(&e.path) else {
            return Ok(false);
        };
        self.current.clone_from(&e.path);
        self.keep = keep;
        self.expected = e.size;
        self.seen = 0;
        let mut out = Vec::new();
        out.try_reserve(keep as usize)
            .map_err(|_| corrupt("Cannot allocate selected archive files."))?;
        self.files.insert(e.path.clone(), out);
        Ok(true)
    }
    fn chunk(&mut self, b: &[u8]) -> Result<(), ArchiveError> {
        if b.len() as u64 > self.expected.saturating_sub(self.seen) {
            return Err(corrupt("Archive entry exceeds its declared size."));
        }
        let out = self.files.get_mut(&self.current).unwrap();
        let room = self
            .keep
            .saturating_sub(out.len() as u64)
            .min(b.len() as u64);
        out.extend_from_slice(&b[..room as usize]);
        self.seen += b.len() as u64;
        Ok(())
    }
    fn enough(&self) -> bool {
        self.keep < self.expected && self.seen >= self.keep
    }
    fn end(&mut self) -> Result<(), ArchiveError> {
        if self.files[&self.current].len() as u64 != self.keep {
            return Err(corrupt("Archive entry size does not match."));
        }
        self.selected.remove(&self.current);
        Ok(())
    }
    fn done(&self) -> bool {
        self.selected.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_entries() {
        for name in [
            "../evil",
            "/abs",
            "C:/drive",
            "a\\b",
            "nul\0name",
            "a//b",
            "a/./b",
            "",
        ] {
            assert!(safe_path(name, false).is_err(), "{name:?}");
        }
        assert!(special(0o120777, false).is_err() && special(0o100644, false).is_ok());
        let e = |path: &str, dir| Entry {
            path: path.into(),
            size: 0,
            dir,
            encrypted: false,
        };
        assert!(validate(&[e("a", false), e("a", false)]).is_err());
        assert!(validate(&[e("a", false), e("a/b", false)]).is_err());
        assert!(validate(&[e("a", true), e("a/b", false)]).is_ok());
    }
}
