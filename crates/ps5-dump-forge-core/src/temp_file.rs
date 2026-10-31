//! Writing a file durably through an exclusive temporary name beside it
//! (`.<name>.forge-<pid>-<n>.tmp`): created, filled, synced, then renamed over (or, never
//! replacing, onto) its target.

use std::fs::File;
use std::io;
use std::path::Path;
#[cfg(target_os = "freebsd")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;

use crate::finalize::create_new;

/// A sync here is not cancellable.
#[cfg(target_os = "freebsd")]
static NO_CANCEL: AtomicBool = AtomicBool::new(false);
/// Makes every temporary name of this process unique.
static ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Whether `path` is named like a temporary file of ours (`.<name>.forge-<pid>-<n>.tmp`),
/// e.g. one a killed job left: never indexed.
pub(crate) fn is_temp(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with('.') && name.ends_with(".tmp") && name.contains(".forge-")
}

/// Writes `dir/name` through an exclusive temporary name of this attempt in `dir`
/// (`.<name>.forge-<pid>-<n>.tmp`): `fill` writes it, it is synced and renamed over the
/// target (a file there is replaced, a link there itself, never followed). On any failure the
/// temporary file is deleted and the target is as it was. `dir` is left to the caller to sync.
pub(crate) fn write_temp(
    dir: &Path,
    name: &str,
    fill: &mut dyn FnMut(&mut File) -> io::Result<()>,
) -> anyhow::Result<()> {
    write_temp_as(dir, name, fill, sync, &|tmp, target| {
        std::fs::rename(tmp, target)
    })
}

/// [`write_temp`] that never replaces: the rename fails with `AlreadyExists` when the target
/// exists ([`crate::finalize::rename_no_replace`]), and the temporary file is deleted.
///
/// On macOS the file gets a plain fsync, not F_FULLFSYNC ([`crate::extract::sync_file`]): no
/// old file is lost by the rename, and the caller ends with one F_FULLFSYNC (a folder's
/// [`crate::finalize::fsync_dir`]) as the barrier for every file before it. [`write_temp`]
/// keeps the full sync, since its rename drops the file it replaces.
pub(crate) fn write_temp_new(
    dir: &Path,
    name: &str,
    fill: &mut dyn FnMut(&mut File) -> io::Result<()>,
) -> anyhow::Result<()> {
    write_temp_as(
        dir,
        name,
        fill,
        sync_new,
        &crate::finalize::rename_no_replace,
    )
}

/// A temporary name of this attempt for `name` (`.<name>.forge-<pid>-<n>.tmp`).
pub(crate) fn temp_name(name: &str) -> String {
    let attempt = ATTEMPT.fetch_add(1, Ordering::Relaxed);
    // A long name cut to 160 bytes, so the temporary one stays within 255.
    let cut = (0..=name.len().min(160))
        .rev()
        .find(|&i| name.is_char_boundary(i))
        .unwrap_or(0);
    let tmp = format!(
        ".{}.forge-{}-{attempt}.tmp",
        &name[..cut],
        std::process::id()
    );
    debug_assert!(is_temp(&tmp));
    tmp
}

fn write_temp_as(
    dir: &Path,
    name: &str,
    fill: &mut dyn FnMut(&mut File) -> io::Result<()>,
    sync: fn(&File) -> io::Result<()>,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> anyhow::Result<()> {
    let target = dir.join(name);
    let tmp = dir.join(temp_name(name));
    let mut file = create_new(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    // Ours: created above. Removed unless renamed, also when `fill` panics (a job catches it).
    let mut guard = TempGuard(Some(&tmp));
    let written = fill(&mut file).and_then(|()| sync(&file)).and_then(|()| {
        drop(file);
        rename(&tmp, &target)
    });
    if written.is_ok() {
        guard.0 = None;
    }
    drop(guard);
    written.with_context(|| format!("writing {}", target.display()))
}

/// Removes its temporary file when dropped armed.
struct TempGuard<'a>(Option<&'a Path>);

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if let Some(tmp) = self.0 {
            let _ = std::fs::remove_file(tmp);
        }
    }
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

#[cfg(target_os = "freebsd")]
use sync as sync_new;

#[cfg(not(target_os = "freebsd"))]
use crate::extract::sync_file as sync_new;
