//! Archive sources (`.zip`, `.7z`, `.rar`): the game folder inside is read straight from the
//! archive, file by file in the order the job asks (`ps5_dump_forge_archive::Reader`), with
//! nothing unpacked to disk.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use anyhow::bail;
use ps5_dump_forge_archive::{ArchiveError, ArchiveFormat, Reader};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::ConvertRequest;
use crate::convert::{Kind, SourceStamp};
use crate::jobs::Ctx;

pub(crate) fn name(format: ArchiveFormat) -> &'static str {
    match format {
        ArchiveFormat::Zip => "zip",
        ArchiveFormat::SevenZ => "7z",
        ArchiveFormat::Rar => "rar",
    }
}

/// The archive's game folder as the job's source, and the source stamp extended to every
/// part of the archive.
pub(crate) fn open(
    req: &ConvertRequest,
    ctx: &Ctx,
    format: ArchiveFormat,
    stamp: SourceStamp,
) -> anyhow::Result<(Box<dyn SourceTree>, SourceStamp)> {
    let kind = name(format);
    let given = req.archive_password.as_ref().map(|p| p.as_str());
    let password = password(&req.source, given, ctx.cancel)?;
    let password = password.as_deref();
    let listing = ps5_dump_forge_archive::list(&req.source, password)?;
    // The parts are known only once listed: stamped then, and listed again, so a part swapped
    // before its stamp shows as a different listing (the stamp catches any later change).
    let stamp = stamp.with_parts(&listing.volumes)?;
    if ps5_dump_forge_archive::list(&req.source, password)?.entries != listing.entries {
        bail!("{} changed while it was being read", req.source.display());
    }
    let prefix = crate::library::archive_game_root(&req.source, password, &listing, ctx.cancel)?;
    let mut files = Vec::new();
    let mut dirs = BTreeSet::new();
    let mut parents = BTreeSet::new();
    for e in listing.entries.iter() {
        let Some(rel) = e.path.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let rel = rel.strip_suffix('/').unwrap_or(rel);
        if rel.is_empty() {
            continue;
        }
        let mut at = rel;
        while let Some((parent, _)) = at.rsplit_once('/') {
            parents.insert(parent.to_string());
            at = parent;
        }
        if e.dir {
            dirs.insert(rel.to_string());
        } else {
            files.push(SourceFile {
                path: rel.to_string(),
                size: e.size,
            });
        }
    }
    let empty_dirs = dirs.difference(&parents).cloned().collect();
    // No read asks for an empty file, so nothing would run the archive's check of one: a
    // non-solid archive's are all decoded now (one pass of header skips), any data in one
    // fails. A solid archive's are checked as its reads go past them.
    // ponytail: a solid archive's empty files after the last one read go unchecked.
    let empty: std::collections::HashSet<String> = (files.iter())
        .filter(|f| f.size == 0)
        .map(|f| format!("{prefix}{}", f.path))
        .collect();
    if !empty.is_empty() && !listing.solid {
        heads(
            &req.source,
            password,
            &|p, size| empty.contains(p).then_some(size),
            0,
            ctx.cancel,
        )?;
    }
    let sizes = files.iter().map(|f| (f.path.clone(), f.size)).collect();
    let describe = format!(
        "{} (.{kind} archive{}, read in place)",
        req.source.display(),
        match prefix.as_str() {
            "" => String::new(),
            p => format!(", game in {p}"),
        }
    );
    let tree = ArchiveTree {
        reader: Reader::new(
            &req.source,
            password,
            listing,
            ctx.shared_cancel.clone().unwrap_or_default(),
        ),
        prefix,
        files,
        sizes,
        empty_dirs,
        describe,
    };
    Ok((
        Box::new(crate::scan::JunkFiltered::new(Box::new(tree))),
        stamp,
    ))
}

/// The game folder inside an archive: paths relative to it, bytes from the archive.
struct ArchiveTree {
    reader: Reader,
    /// The game folder's path in the archive (`""` or ending in `/`).
    prefix: String,
    files: Vec<SourceFile>,
    sizes: HashMap<String, u64>,
    empty_dirs: Vec<String>,
    describe: String,
}

impl SourceTree for ArchiveTree {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let size = self.sizes.get(path).copied().unwrap_or(0);
        let len = usize::try_from(size)
            .map_err(|_| ps5upload_fpkg::Error::Format(format!("{path} is too large")))?;
        self.read_range(path, 0, len)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let entry = format!("{}{path}", self.prefix);
        self.reader
            .read_at(&entry, offset, len)
            .map_err(|e| match e {
                ArchiveError::Cancelled => ps5upload_fpkg::Error::Cancelled,
                e => ps5upload_fpkg::Error::Format(format!("{path}: {e}")),
            })
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

/// Passwords release sites put on their archives, tried in this order on an archive that needs
/// one when none is given. Never logged or shown.
const KNOWN_PASSWORDS: [&str; 6] = [
    "DLPSGAME.COM",
    "[DLPSGAME.COM]",
    "SuperPSX",
    "www.DLPSGAME.COM",
    "downloadgameps3.com",
    "hako",
];

/// Archives whose password check passed, newest last, so picking a source decodes a solid
/// archive once and naming it or starting its job decodes none of it again.
// ponytail: at most 4 archives remembered (the oldest dropped), each valid while its stamp
// holds; a part swapped back to identical metadata goes unnoticed, as for a job's stamp.
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());
const MAX_OPENED: usize = 4;

/// One archive per lock, held across lookup, check and fill, so two callers for one archive
/// (Convert asks the name while inspect runs) decode it once.
static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);

/// How many decoding passes this file started (tests count them).
#[cfg(test)]
pub(crate) static READ_PASSES: AtomicUsize = AtomicUsize::new(0);

/// An archive the check opened. No `Debug`: it holds the password.
#[derive(Clone)]
struct Opened {
    /// The archive and its every part, as the check's listing named them.
    stamp: SourceStamp,
    password: Option<String>,
    /// The game folder's path in the archive (`""` or ending in `/`).
    root: String,
    param: Vec<u8>,
}

/// What one check pass read: the stamp, the game folder and the heads kept.
struct Checked {
    stamp: SourceStamp,
    root: String,
    heads: Heads,
}

/// Archive paths -> the bytes read from their start.
type Heads = BTreeMap<String, Vec<u8>>;

/// More files to read in the check's pass: archive paths -> bytes from the start to keep.
type Extra<'a> = &'a dyn Fn(&ps5_dump_forge_archive::Listing, &str) -> BTreeMap<String, u64>;

fn unlocked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`, waited for while `cancel` isn't set (a job behind another tab's inspect of the same
/// archive stays cancellable).
fn hold<'a>(
    lock: &'a Mutex<()>,
    cancel: &AtomicBool,
) -> anyhow::Result<std::sync::MutexGuard<'a, ()>> {
    loop {
        match lock.try_lock() {
            Ok(held) => return Ok(held),
            Err(std::sync::TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The lock for `path`; locks nobody holds are dropped.
fn path_lock(path: &Path) -> Arc<Mutex<()>> {
    let mut locks = unlocked(&LOCKS);
    locks.retain(|_, l| Arc::strong_count(l) > 1);
    locks.entry(path.to_path_buf()).or_default().clone()
}

/// The remembered archive `path` when it is unchanged and `given` fits it (its password, or it
/// needs none; no password fits only one of [`KNOWN_PASSWORDS`], so a typed password is never
/// handed to a request that didn't give it): the password to use (`given` first), its game
/// folder and `param.json`.
fn lookup(path: &Path, given: Option<&str>) -> Option<(Option<String>, String, Vec<u8>)> {
    let fits = |o: &&Opened| match (given, o.password.as_deref()) {
        (_, None) => true,
        (None, Some(found)) => KNOWN_PASSWORDS.contains(&found),
        (Some(given), Some(found)) => given == found,
    };
    let found = unlocked(&OPENED)
        .iter()
        .find(|o| o.stamp.path() == path)
        .filter(fits)
        .cloned()?;
    if !found.stamp.unchanged() {
        unlocked(&OPENED).retain(|o| !o.stamp.same(&found.stamp));
        return None;
    }
    Some((
        given.map(str::to_owned).or(found.password),
        found.root,
        found.param,
    ))
}

fn remember(path: &Path, password: Option<String>, checked: &Checked) {
    let Some(param) = checked.heads.get(&param_of(&checked.root)) else {
        return;
    };
    let mut opened = unlocked(&OPENED);
    opened.retain(|o| o.stamp.path() != path);
    if opened.len() >= MAX_OPENED {
        opened.remove(0);
    }
    opened.push(Opened {
        stamp: checked.stamp.clone(),
        password,
        root: checked.root.clone(),
        param: param.clone(),
    });
}

fn param_of(root: &str) -> String {
    format!("{root}sce_sys/param.json")
}

/// One decoding pass (every `read_heads` this file makes goes through here).
fn heads(
    path: &Path,
    password: Option<&str>,
    keep: &dyn Fn(&str, u64) -> Option<u64>,
    max_bytes: u64,
    cancel: &AtomicBool,
) -> Result<Heads, ArchiveError> {
    #[cfg(test)]
    READ_PASSES.fetch_add(1, Ordering::Relaxed);
    ps5_dump_forge_archive::read_heads(path, password, keep, max_bytes, cancel)
}

/// The password to read the archive `path` with: `given` as it is (nothing read); none when the
/// archive needs none; else the first of [`KNOWN_PASSWORDS`] that opens its game folder (its
/// `param.json` is read, as a wrong password may still list the files). None fitting: the
/// archive's own "needs a password" error, unless one of them failed some other way (most
/// likely the right one, on a damaged or unusable archive): that error. An archive checked
/// before and unchanged since is not read again.
pub(crate) fn password(
    path: &Path,
    given: Option<&str>,
    cancel: &AtomicBool,
) -> anyhow::Result<Option<String>> {
    if given.is_some() {
        return Ok(given.map(str::to_owned));
    }
    let lock = path_lock(path);
    let _held = hold(&lock, cancel)?;
    if let Some((password, _, _)) = lookup(path, None) {
        return Ok(password);
    }
    let (password, checked) = trial(path, None, &|_, _| BTreeMap::new(), cancel)?;
    remember(path, password.clone(), &checked);
    Ok(password)
}

/// The password, the game folder's `param.json` (at `<root>sce_sys/param.json`) and the heads
/// `extra` asks for, in one decoding pass: [`password`]'s answer, read with `given` when one is.
pub(crate) fn password_reading(
    path: &Path,
    given: Option<&str>,
    extra: Extra,
    cancel: &AtomicBool,
) -> anyhow::Result<(Option<String>, Heads)> {
    let lock = path_lock(path);
    let _held = hold(&lock, cancel)?;
    if let Some((password, root, param)) = lookup(path, given) {
        let pw = password.as_deref();
        let listing = ps5_dump_forge_archive::list(path, pw)?;
        let want = extra(&listing, &root);
        let mut read = BTreeMap::new();
        if !want.is_empty() {
            let max = want.values().fold(0u64, |n, k| n.saturating_add(*k));
            read = heads(path, pw, &|p, _| want.get(p).copied(), max, cancel)?;
        }
        read.insert(param_of(&root), param);
        return Ok((password, read));
    }
    let (password, checked) = trial(path, given, extra, cancel)?;
    remember(path, password.clone(), &checked);
    Ok((password, checked.heads))
}

/// The remembered archive's password and `param.json`, nothing read (see [`password`]).
pub(crate) fn cached_param(path: &Path, given: Option<&str>) -> Option<(Option<String>, Vec<u8>)> {
    let lock = path_lock(path);
    let _held = unlocked(&lock);
    lookup(path, given).map(|(password, _, param)| (password, param))
}

/// Opens the game folder with `pw` in one pass: its `param.json`, a proof and `extra`'s heads.
/// The game folder's `param.json` is read whatever the folder search did: with names in the
/// clear a wrong password lists the archive, and a lone game folder is found unread. A
/// `param.json` in the clear proves nothing, so then the smallest encrypted file of the game is
/// read too (decoded and checked whole, its head kept: it may be large).
fn check(
    path: &Path,
    pw: Option<&str>,
    extra: Extra,
    cancel: &AtomicBool,
) -> anyhow::Result<Checked> {
    // Stamped before the listing, so a change while it is read shows as a changed stamp.
    let stamp = SourceStamp::take(path, Kind::of(path)?)?;
    let listing = ps5_dump_forge_archive::list(path, pw)?;
    let stamp = stamp.with_parts(&listing.volumes)?;
    let root = crate::library::archive_game_root(path, pw, &listing, cancel)?;
    let param = param_of(&root);
    let game = |e: &&ps5_dump_forge_archive::Entry| !e.dir && e.path.starts_with(&root);
    let clear = (listing.entries.iter().filter(game)).any(|e| e.path == param && !e.encrypted);
    let proof = (listing.entries.iter().filter(game))
        .filter(|e| clear && e.encrypted && e.size > 0)
        .min_by_key(|e| e.size)
        .map(|e| e.path.clone());
    let want = extra(&listing, &root);
    let base = crate::preflight::MAX_PARAM_JSON.saturating_add(64 << 10);
    let max = want.values().fold(base, |n, k| n.saturating_add(*k));
    let keep = |p: &str, size: u64| {
        if p == param {
            return Some(size);
        }
        let proof = (proof.as_deref() == Some(p)).then(|| size.min(64 << 10));
        want.get(p).copied().max(proof)
    };
    let mut read = heads(path, pw, &keep, max, cancel)?;
    if let Some(p) = proof.filter(|p| !want.contains_key(p)) {
        read.remove(&p);
    }
    Ok(Checked {
        stamp,
        root,
        heads: read,
    })
}

/// `given`'s check, else none's, then each of [`KNOWN_PASSWORDS`]' (see [`password`]).
fn trial(
    path: &Path,
    given: Option<&str>,
    extra: Extra,
    cancel: &AtomicBool,
) -> anyhow::Result<(Option<String>, Checked)> {
    if given.is_some() {
        return Ok((given.map(str::to_owned), check(path, given, extra, cancel)?));
    }
    let is = |e: &anyhow::Error, what: &str| e.to_string() == what;
    let needed = match check(path, None, extra, cancel) {
        Ok(checked) => return Ok((None, checked)),
        Err(e) if is(&e, crate::ARCHIVE_PASSWORD_REQUIRED) => e,
        Err(e) => return Err(e),
    };
    let mut other = None;
    for known in KNOWN_PASSWORDS {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        match check(path, Some(known), extra, cancel) {
            Ok(checked) => return Ok((Some(known.to_owned()), checked)),
            Err(e)
                if is(&e, crate::ARCHIVE_PASSWORD_WRONG)
                    || is(&e, crate::ARCHIVE_PASSWORD_REQUIRED) => {}
            Err(e) => {
                other.get_or_insert(e);
            }
        }
    }
    Err(other.unwrap_or(needed))
}

/// Is `path` named as an archive this app reads?
pub(crate) fn is_archive(path: &Path) -> bool {
    !path.is_dir() && ps5_dump_forge_archive::detect(path).is_some()
}
