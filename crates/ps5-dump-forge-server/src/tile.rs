//! The PS5 home-screen tile `/user/app/PDFG00001`, a `deeplinkUri` to the web UI. Adapted from
//! ps5-ai-cli's `launcher/install.c` (GPL-3.0-or-later); `ps5/launcher.c` registers the title.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde_json::json;

pub(crate) const TITLE_ID: &str = "PDFG00001";
use crate::notice::ICON;
/// The title folder this payload created, by `(st_dev, st_ino)`, plus `registered 0` once that
/// succeeded; only that folder is ever written to. Outside it, so app registration never meets it.
/// ponytail: a run cut off between creating the folder and writing this file leaves an empty
/// PDFG00001 that is then never ours (delete it by hand); and a filesystem that hands a deleted
/// folder's inode number to another app's new PDFG00001 makes that one look ours.
#[cfg(target_env = "ps5")]
pub(crate) const OWNER_FILE: &str = "/data/dumpforge/tile-PDFG00001.owner";
/// Taken over while `OWNER_FILE` is missing, only from a real folder (not entry.c's link).
#[cfg(target_env = "ps5")]
const OLD_OWNER_FILE: &str = "/data/ps5-dump-forge/tile-PDFG00001.owner";
const OWNER_HEADER: &str = "Dump Forge home-screen tile";
/// Earlier versions': still ours.
const OLD_OWNER_HEADER: &str = "PS5 Dump Forge home-screen tile";

#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Ready,
    /// The registration's result (not 0: retried on the next start).
    Registered(i32),
    Left(String),
}

fn param_json(port: u16) -> Vec<u8> {
    let param = json!({
        "applicationCategoryType": 65536,
        "titleId": TITLE_ID,
        "localizedParameters": {
            "defaultLanguage": "en-US",
            "en-US": { "titleName": "Dump Forge" },
        },
        "deeplinkUri": format!("http://127.0.0.1:{port}/"),
    });
    (serde_json::to_string_pretty(&param).unwrap() + "\n").into_bytes()
}

/// Calls `register` if anything was written or no registration has succeeded yet. Symlinks are
/// never followed; nothing is written into a title folder this payload didn't create.
// ponytail: checks, then writes by path: another program on the console swapping a symlink in
// between the metadata check and the rename could redirect a write. Out of scope by the user's
// no-protections decision; the upgrade is openat/renameat on held O_DIRECTORY|O_NOFOLLOW
// descriptors.
pub(crate) fn ensure(
    apps: &Path,
    owner: &Path,
    old_owner: &Path,
    port: u16,
    register: impl FnOnce() -> i32,
) -> io::Result<Outcome> {
    let title = apps.join(TITLE_ID);
    let sys = title.join("sce_sys");
    let (owned, registered) = match fs::symlink_metadata(&title) {
        Err(e) if e.kind() == ErrorKind::NotFound => {
            fs::create_dir(&title)?;
            let made = fs::symlink_metadata(&title).and_then(|m| record(owner, &m, false));
            if let Err(e) = made {
                // Still empty: no one's folder.
                let _ = fs::remove_dir(&title);
                return Err(e);
            }
            (true, false)
        }
        Err(e) => return Err(e),
        Ok(meta) if !meta.is_dir() => {
            return Ok(Outcome::Left(format!(
                "{} is not a folder",
                title.display()
            )));
        }
        Ok(meta) => {
            let ours = owner_text(&meta, false);
            let mut text = read_owner(owner).unwrap_or_default();
            if !fs::exists(owner)?
                && real_dir(old_owner.parent())
                && let Some(old) = read_owner(old_owner).filter(|t| t.starts_with(&ours))
            {
                record(owner, &meta, old == owner_text(&meta, true))?;
                text = old;
            }
            (text.starts_with(&ours), text == owner_text(&meta, true))
        }
    };
    let not_ours = |what: &Path| {
        let why = format!(
            "{} is not ours (no {}); left alone",
            what.display(),
            owner.display()
        );
        Ok(Outcome::Left(why))
    };
    match fs::symlink_metadata(&sys) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Ok(Outcome::Left(format!("{} is not a folder", sys.display()))),
        Err(e) if e.kind() == ErrorKind::NotFound && owned => fs::create_dir(&sys)?,
        Err(e) if e.kind() == ErrorKind::NotFound => return not_ours(&title),
        Err(e) => return Err(e),
    }

    let files = [
        ("param.json", param_json(port)),
        ("icon0.png", ICON.to_vec()),
    ];
    let mut stale = Vec::new();
    for (name, bytes) in &files {
        let path = sys.join(name);
        match state(&path, bytes)? {
            State::Same => {}
            State::NotFile => {
                let why = format!("{} is not a regular file; left alone", path.display());
                return Ok(Outcome::Left(why));
            }
            State::Missing | State::Differs if !owned => return not_ours(&path),
            State::Missing | State::Differs => stale.push((*name, bytes)),
        }
    }
    if stale.is_empty() && (registered || !owned) {
        return Ok(Outcome::Ready);
    }
    let folder = fs::symlink_metadata(&title)?;
    if !stale.is_empty() {
        // Not registered as it will be until registration succeeds again.
        record(owner, &folder, false)?;
    }
    for (name, bytes) in stale {
        write(&sys, name, bytes)?;
    }
    let code = register();
    if code == 0 {
        record(owner, &folder, true)?;
    }
    Ok(Outcome::Registered(code))
}

/// `None` unless a regular file.
fn read_owner(path: &Path) -> Option<String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => {
            let text = fs::read_to_string(path).unwrap_or_default();
            Some(match text.strip_prefix(OLD_OWNER_HEADER) {
                Some(rest) => format!("{OWNER_HEADER}{rest}"),
                None => text,
            })
        }
        _ => None,
    }
}

fn real_dir(path: Option<&Path>) -> bool {
    path.is_some_and(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()))
}

fn owner_text(folder: &fs::Metadata, registered: bool) -> String {
    let done = if registered { "registered 0\n" } else { "" };
    format!(
        "{OWNER_HEADER}\nfolder {} {}\n{done}",
        folder.dev(),
        folder.ino()
    )
}

fn record(owner: &Path, folder: &fs::Metadata, registered: bool) -> io::Result<()> {
    let dir = owner.parent().unwrap_or(Path::new("."));
    let name = owner
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tile.owner");
    write(dir, name, owner_text(folder, registered).as_bytes())
}

#[derive(Debug, PartialEq)]
enum State {
    Same,
    Missing,
    Differs,
    NotFile,
}

fn state(path: &Path, want: &[u8]) -> io::Result<State> {
    Ok(match fs::symlink_metadata(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => State::Missing,
        Err(e) => return Err(e),
        Ok(meta) if !meta.is_file() => State::NotFile,
        Ok(meta) if meta.len() != want.len() as u64 => State::Differs,
        // ponytail: checked, then read; nothing else writes here while the payload starts.
        Ok(_) if fs::read(path)? == want => State::Same,
        Ok(_) => State::Differs,
    })
}

/// Through an `O_EXCL` temporary file (never through a symlink), synced, renamed over it.
fn write(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = dir.join(format!(".{name}.forge-tmp"));
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, dir.join(name))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// In the background: never blocks or fails the server.
#[cfg(target_env = "ps5")]
pub(crate) fn install(port: u16) {
    unsafe extern "C" {
        fn ps5_register_title(title_id: *const std::ffi::c_char) -> std::ffi::c_int;
    }
    let register = || {
        let id = std::ffi::CString::new(TITLE_ID).unwrap();
        // SAFETY: a NUL-terminated string that outlives the call.
        unsafe { ps5_register_title(id.as_ptr()) }
    };
    let _ = std::thread::Builder::new()
        .name("forge-tile".into())
        .spawn(move || {
            let line = match ensure(
                Path::new("/user/app"),
                Path::new(OWNER_FILE),
                Path::new(OLD_OWNER_FILE),
                port,
                register,
            ) {
                Ok(Outcome::Ready) => "up to date".to_string(),
                Ok(Outcome::Registered(0)) => "registered (ps5_register_title returned 0)".into(),
                Ok(Outcome::Registered(code)) => format!(
                    "registration failed: ps5_register_title returned {code} ({:#x}); \
                     retried on the next start",
                    code as u32
                ),
                Ok(Outcome::Left(why)) => format!("not installed: {why}"),
                Err(e) => format!("not installed: {e}"),
            };
            println!("ps5-dump-forge serve: home-screen tile {TITLE_ID}: {line}");
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn installs_once_and_leaves_others_alone() {
        let apps = std::env::temp_dir().join(format!("forge-tile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&apps);
        fs::create_dir_all(&apps).unwrap();
        let title = apps.join(TITLE_ID);
        let sys = title.join("sce_sys");
        let owner = apps.join("tile.owner");
        let (calls, code) = (Cell::new(0), Cell::new(0));
        let run = |port| {
            ensure(&apps, &owner, &apps.join("old/tile.owner"), port, || {
                calls.set(calls.get() + 1);
                code.get()
            })
            .unwrap()
        };
        let registered = || {
            fs::read_to_string(&owner)
                .unwrap()
                .ends_with("registered 0\n")
        };

        assert_eq!(run(8095), Outcome::Registered(0));
        assert_eq!(fs::read(sys.join("param.json")).unwrap(), param_json(8095));
        assert_eq!(fs::read(sys.join("icon0.png")).unwrap(), ICON);
        let param: serde_json::Value =
            serde_json::from_slice(&fs::read(sys.join("param.json")).unwrap()).unwrap();
        assert_eq!(param["deeplinkUri"], "http://127.0.0.1:8095/");
        assert_eq!(param["applicationCategoryType"], 65536);
        assert!(registered());
        // Idempotent: nothing written, no registration.
        assert_eq!(run(8095), Outcome::Ready);
        assert_eq!(calls.get(), 1);
        // Ours, on another port: rewritten and registered again.
        assert_eq!(run(9000), Outcome::Registered(0));
        assert_eq!(fs::read(sys.join("param.json")).unwrap(), param_json(9000));
        assert_eq!(calls.get(), 2);
        let names: Vec<_> = fs::read_dir(&sys).unwrap().flatten().collect();
        assert_eq!(names.len(), 2, "no temporary files left");

        // A failed registration is retried on every start, though the files match.
        code.set(-1);
        assert_eq!(run(8095), Outcome::Registered(-1));
        assert!(!registered());
        assert_eq!(run(8095), Outcome::Registered(-1));
        code.set(0);
        assert_eq!(run(8095), Outcome::Registered(0));
        assert!(registered());
        assert_eq!(run(8095), Outcome::Ready);
        assert_eq!(calls.get(), 5);

        // A symlink is never followed or replaced, even in our folder.
        let elsewhere = apps.join("elsewhere.png");
        fs::write(&elsewhere, b"theirs").unwrap();
        fs::remove_file(sys.join("icon0.png")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, sys.join("icon0.png")).unwrap();
        assert!(matches!(run(9000), Outcome::Left(why) if why.contains("not a regular file")));
        assert_eq!(fs::read(&elsewhere).unwrap(), b"theirs");
        assert_eq!(calls.get(), 5);

        // Removed from the home screen (the title folder deleted): written and registered.
        fs::remove_dir_all(&title).unwrap();
        assert_eq!(run(8095), Outcome::Registered(0));
        assert_eq!(calls.get(), 6);

        // Another app's PDFG00001 (made while ours exists, so not our inode number): left alone.
        let other = apps.join("other");
        fs::create_dir_all(other.join("sce_sys")).unwrap();
        fs::remove_dir_all(&title).unwrap();
        fs::rename(&other, &title).unwrap();
        fs::write(sys.join("param.json"), b"{\"titleId\":\"PDFG00001\"}").unwrap();
        assert!(matches!(run(8095), Outcome::Left(why) if why.contains("is not ours")));
        assert_eq!(
            fs::read(sys.join("param.json")).unwrap(),
            b"{\"titleId\":\"PDFG00001\"}"
        );
        assert!(!sys.join("icon0.png").exists());
        // Identical files in it are fine, but not ours to register.
        fs::write(sys.join("param.json"), param_json(8095)).unwrap();
        fs::write(sys.join("icon0.png"), ICON).unwrap();
        assert_eq!(run(8095), Outcome::Ready);
        // No owner file at all: the same.
        fs::remove_file(&owner).unwrap();
        fs::remove_file(sys.join("icon0.png")).unwrap();
        assert!(matches!(run(8095), Outcome::Left(why) if why.contains("is not ours")));
        fs::remove_dir_all(&sys).unwrap();
        assert!(matches!(run(8095), Outcome::Left(why) if why.contains("is not ours")));
        assert!(!sys.exists());
        assert_eq!(calls.get(), 6);

        // A title "folder" that is a symlink is left alone.
        fs::remove_dir_all(&title).unwrap();
        std::os::unix::fs::symlink(&apps, &title).unwrap();
        assert!(matches!(run(8095), Outcome::Left(why) if why.contains("not a folder")));
        assert_eq!(calls.get(), 6);
        fs::remove_dir_all(&apps).unwrap();
    }
}
