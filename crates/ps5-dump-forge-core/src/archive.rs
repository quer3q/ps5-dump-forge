//! Archive sources (`.zip`, `.7z`, `.rar`): the game folder inside is read straight from the
//! archive, file by file in the order the job asks (`ps5_dump_forge_archive::Reader`), with
//! nothing unpacked to disk.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::bail;
use ps5_dump_forge_archive::{ArchiveError, ArchiveFormat, Reader};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::ConvertRequest;
use crate::convert::SourceStamp;
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
        ps5_dump_forge_archive::read_files(
            &req.source,
            password,
            &|p| empty.contains(p),
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

/// The password to read the archive `path` with: `given` as it is; none when the archive needs
/// none; else the first of [`KNOWN_PASSWORDS`] that opens its game folder (its `param.json` is
/// read, as a wrong password may still list the files). None fitting: the archive's own
/// "needs a password" error, unless one of them failed some other way (most likely the right
/// one, on a damaged or unusable archive): that error.
pub(crate) fn password(
    path: &Path,
    given: Option<&str>,
    cancel: &AtomicBool,
) -> anyhow::Result<Option<String>> {
    if given.is_some() {
        return Ok(given.map(str::to_owned));
    }
    // The game folder's `param.json` is read whatever the folder search did: with names in the
    // clear a wrong password lists the archive, and a lone game folder is found unread. A
    // `param.json` in the clear proves nothing, so then the smallest encrypted file of the
    // game is read too.
    let opens = |pw: Option<&str>| -> anyhow::Result<()> {
        let listing = ps5_dump_forge_archive::list(path, pw)?;
        let root = crate::library::archive_game_root(path, pw, &listing, cancel)?;
        let param = format!("{root}sce_sys/param.json");
        let game = |e: &&ps5_dump_forge_archive::Entry| !e.dir && e.path.starts_with(&root);
        let clear = (listing.entries.iter().filter(game)).any(|e| e.path == param && !e.encrypted);
        let proof = (listing.entries.iter().filter(game))
            .filter(|e| clear && e.encrypted && e.size > 0)
            .min_by_key(|e| e.size);
        // The proof is decoded and checked whole, but only its head is kept: it may be large.
        ps5_dump_forge_archive::read_heads(
            path,
            pw,
            &|p, size| {
                if p == param {
                    Some(size)
                } else {
                    proof.filter(|e| e.path == p).map(|_| size.min(64 << 10))
                }
            },
            crate::preflight::MAX_PARAM_JSON.saturating_add(64 << 10),
            cancel,
        )?;
        Ok(())
    };
    let is = |e: &anyhow::Error, what: &str| e.to_string() == what;
    let needed = match opens(None) {
        Ok(()) => return Ok(None),
        Err(e) if is(&e, crate::ARCHIVE_PASSWORD_REQUIRED) => e,
        Err(e) => return Err(e),
    };
    let mut other = None;
    for known in KNOWN_PASSWORDS {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        match opens(Some(known)) {
            Ok(()) => return Ok(Some(known.to_owned())),
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
