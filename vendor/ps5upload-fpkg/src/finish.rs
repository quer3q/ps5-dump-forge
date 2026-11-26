//! The last step of a build: giving a finished file its name.
//!
//! On Windows a virus scanner or the search indexer opens a file the moment it is closed, and
//! for as long as it holds it a rename fails with a sharing violation or "access is denied". A
//! build that ran for hours must not end on that, so the rename is tried again for a few
//! seconds. Elsewhere a rename never fails this way and there is one attempt.

use std::io;
use std::path::Path;
use std::time::Duration;

/// How many times a rename is tried again, the wait doubling from 100 ms up to 2 s: about
/// ten seconds in all.
const RETRIES: u32 = 8;

/// Whether `e` is what Windows says while another program briefly holds the file:
/// `ERROR_ACCESS_DENIED` (5), `ERROR_SHARING_VIOLATION` (32), `ERROR_LOCK_VIOLATION` (33).
pub fn held_by_another_program(e: &io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Runs `op`, again while it fails the way [`held_by_another_program`] names.
pub fn retry_held<T>(op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    retry_while(
        RETRIES,
        Duration::from_millis(100),
        held_by_another_program,
        op,
    )
}

fn retry_while<T>(
    retries: u32,
    first_wait: Duration,
    again: impl Fn(&io::Error) -> bool,
    mut op: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut wait = first_wait;
    let mut left = retries;
    loop {
        match op() {
            Err(e) if left > 0 && again(&e) => {
                left -= 1;
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(2));
            }
            r => return r,
        }
    }
}

/// `std::fs::rename`, tried again while another program briefly holds the file.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    retry_held(|| std::fs::rename(from, to))
}

fn kept() -> &'static std::sync::Mutex<Vec<std::path::PathBuf>> {
    static KEPT: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
    &KEPT
}

/// Whether `path` is a finished output [`kept_message`] left under its `.partial` name: a
/// sweep of build leftovers passes it by for as long as this process runs.
pub fn is_kept(path: &Path) -> bool {
    kept()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|p| p == path)
}

/// What a build says when its finished, checked output could not be given its name: the
/// output is kept under its `.partial` name, since writing it again may take hours.
pub fn kept_message(partial: &Path, target: &Path, e: &dyn std::fmt::Display) -> String {
    {
        let mut kept = kept().lock().unwrap_or_else(|e| e.into_inner());
        if !kept.iter().any(|p| p == partial) {
            kept.push(partial.to_path_buf());
        }
    }
    format!(
        "{} is finished and checked but could not be given its name ({e}). It is kept as {}: \
         rename it by hand before building again",
        target.display(),
        partial.display()
    )
}
