//! Patching a game folder in place for LZ4 traces (`lz4_patch`): the trace runtime, no stale
//! journal or logs, and a fresh `ampr_emu.index` the next session's journal is read through.
//! The user asked for it on the console, where a traced copy would need the room twice.
//! `lz4_unpatch` is the same with the release runtime.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5_dump_forge_lz4::runtime::{self, RELEASE, TRACE};
use ps5_dump_forge_lz4::{INDEX, JOURNAL, LOGS, MANIFEST, RUNTIME};
use ps5upload_fpkg::source::SourceTree;

use crate::Lz4Patch;
use crate::convert::Kind;
use crate::finalize::{create_new, fsync_dir};
use crate::scan::ScannedFolder;

/// A scan here is not cancellable.
static NO_CANCEL: AtomicBool = AtomicBool::new(false);
/// Makes every temporary name of this process unique.
static ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Whether `path` is named like a temporary file of a patch (`.<name>.forge-<pid>-<n>.tmp`),
/// e.g. one a killed patch left: never indexed, and left out of the traces' membership check.
pub(crate) fn is_temp(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with('.') && name.ends_with(".tmp") && name.contains(".forge-")
}

/// The runtime a patch installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Runtime {
    /// `lz4_patch`: the tracing build.
    Trace,
    /// `lz4_unpatch`: the release build.
    Release,
}

pub(crate) fn patch(folder: &Path, runtime: Runtime) -> anyhow::Result<Lz4Patch> {
    let (bytes, verb) = match runtime {
        Runtime::Trace => (TRACE, "patch"),
        Runtime::Release => (RELEASE, "unpatch"),
    };
    let kind = Kind::of(folder)?;
    if kind != Kind::Folder {
        bail!(
            "{} is a .{} image: only a game folder is {verb}ed here (an .exfat or .ffpkg image \
             is {verb}ed by a conversion in place; other formats are read-only on the console)",
            folder.display(),
            kind.name()
        );
    }
    let mut tree = ScannedFolder::scan(folder, &NO_CANCEL)?;
    let root = tree.root().to_path_buf();
    if ps5_dump_forge_lz4::reader::detect(&mut tree).context("looking for LZ4 packs")? {
        bail!(
            "{} holds LZ4 packs ({MANIFEST}): unpack it first, then {verb} the unpacked folder",
            root.display()
        );
    }
    if !ps5upload_fpkg::source::imports_ampr(&mut tree, "eboot.bin") {
        bail!(
            "eboot.bin does not import libSceAmpr: LZ4 traces only work for a title that uses \
             AMPR"
        );
    }
    // Everything is checked before the first write. The scanner refused links and special
    // files; a folder where a file goes is refused here.
    let fakelib = root.join("fakelib");
    let fakelib_exists = match std::fs::symlink_metadata(&fakelib) {
        Ok(meta) if meta.is_dir() => true,
        Ok(_) => bail!("{} is not a folder; not {verb}ing", fakelib.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e).with_context(|| fakelib.display().to_string()),
    };
    for rel in [RUNTIME, INDEX, JOURNAL].iter().chain(LOGS.iter()) {
        let path = root.join(rel);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => {}
            Ok(_) => bail!("{} is not a regular file; not {verb}ing", path.display()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| path.display().to_string()),
        }
    }
    // The index for the files after the patch: the new runtime in; the old index, journal,
    // logs and any patch's temporary files out.
    let stale = |p: &str| p == JOURNAL || LOGS.contains(&p);
    let mut rows: Vec<(String, u64)> = tree
        .files()
        .iter()
        .filter(|f| f.path != INDEX && f.path != RUNTIME && !stale(&f.path) && !is_temp(&f.path))
        .map(|f| (f.path.clone(), f.size))
        .collect();
    rows.push((RUNTIME.to_string(), bytes.len() as u64));
    let mtime = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let index = ps5upload_fpkg::ampr_index::build(&rows, mtime).ok_or_else(|| {
        anyhow::anyhow!("two paths differ only in case, which {INDEX} cannot tell apart")
    })?;
    // Room for both new files beside the old ones; on the PS5 this is the destination probe
    // (U1), which fails closed like every other write there.
    let need = (bytes.len() + index.len()) as u64;
    let largest = bytes.len().max(index.len()) as u64;
    let dest = crate::preflight::destination(&root, need, largest, &NO_CANCEL);
    if !dest.findings.is_empty() {
        bail!("{}", dest.findings.join("\n"));
    }

    if !fakelib_exists {
        std::fs::create_dir(&fakelib).with_context(|| format!("creating {}", fakelib.display()))?;
        if let Err(e) = crate::finalize::make_dir_open(&fakelib) {
            // Ours, just created and still empty: the folder stays as it was.
            let _ = std::fs::remove_dir(&fakelib);
            return Err(e).with_context(|| format!("setting the mode of {}", fakelib.display()));
        }
        fsync_dir(&root).with_context(|| format!("syncing {}", root.display()))?;
    }
    replace(&fakelib, "libSceAmpr.sprx", bytes)?;
    let mut removed = Vec::new();
    for f in tree.files().iter().filter(|f| stale(&f.path)) {
        let path = root.join(&f.path);
        std::fs::remove_file(&path).with_context(|| format!("deleting {}", path.display()))?;
        removed.push(f.path.clone());
    }
    // The old journal must not come back beside the new index after a power loss: its ids
    // would name the new index's records.
    if !removed.is_empty() {
        fsync_dir(&root).with_context(|| format!("syncing {}", root.display()))?;
    }
    replace(&root, INDEX, &index)?;
    Ok(Lz4Patch {
        indexed: rows.len(),
        removed,
        warning: runtime::WARNING.to_string(),
    })
}

/// Writes `bytes` to `dir/name` durably: a temporary name of this attempt in `dir`, created
/// exclusively, synced, renamed over the target, then `dir` synced.
fn replace(dir: &Path, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let target = dir.join(name);
    let attempt = ATTEMPT.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(
        ".{name}.forge-{}-{attempt}.tmp",
        std::process::id()
    ));
    debug_assert!(is_temp(&tmp.to_string_lossy()));
    let mut file = create_new(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    let written = file
        .write_all(bytes)
        .and_then(|()| sync(&file))
        .and_then(|()| {
            drop(file);
            std::fs::rename(&tmp, &target)
        });
    if let Err(e) = written {
        // Ours: created above, never renamed.
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("writing {}", target.display()));
    }
    fsync_dir(dir).with_context(|| format!("syncing {}", dir.display()))
}

/// U4 on FreeBSD: ps5upload's retries, and a sync that only succeeded on a retry fails.
#[cfg(target_os = "freebsd")]
fn sync(file: &File) -> io::Result<()> {
    match crate::durable::sync_retry(file, false, &NO_CANCEL)? {
        crate::durable::Synced::Retried => Err(crate::durable::retried_sync()),
        _ => Ok(()),
    }
}

#[cfg(not(target_os = "freebsd"))]
fn sync(file: &File) -> io::Result<()> {
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_names() {
        assert!(is_temp(".ampr_emu.index.forge-12-0.tmp"));
        assert!(is_temp("fakelib/.libSceAmpr.sprx.forge-12-3.tmp"));
        assert!(!is_temp("ampr_emu.index"));
        assert!(!is_temp("data/a.forge-1.tmp"));
        assert!(!is_temp("data/.hidden.tmp"));
    }
}
