//! NAS shares (`smb://host[:port]/<share>/<path>`): a game source and an output, read and
//! written through the [`crate::remote::Remote`] calls a paired Forge's files use, over
//! `ps5-dump-forge-smb`. A path's first part is the share; `smb://host` alone lists the server's
//! disk shares. Credentials come from [`set_credentials`] (the Connect service's saved NAS
//! devices); a host with none logs in as guest. A game on a share is still never changed in
//! place: edits, delete and move refuse an `smb://` path
//! ([`crate::net::refuse`]).

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ps5_dump_forge_smb::{File, Session};

use crate::remote::{FlinkPath, Remote, RemoteEntry, RemoteStat, Scheme};

/// One saved NAS's login. An empty `user` is a guest login.
#[derive(Clone, PartialEq, Eq)]
pub struct SmbCredential {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for SmbCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmbCredential")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &"<redacted>")
            .finish()
    }
}

fn credentials() -> &'static Mutex<Vec<SmbCredential>> {
    static C: OnceLock<Mutex<Vec<SmbCredential>>> = OnceLock::new();
    C.get_or_init(Mutex::default)
}

/// Replaces the known logins (hosts compared ASCII case-insensitively) and drops every cached
/// session, so the next use logs in with what was saved.
pub fn set_credentials(creds: Vec<SmbCredential>) {
    *lock(credentials()) = creds;
    let smb = remote_inner();
    let mut sessions = lock(&smb.sessions);
    sessions.clear();
    smb.logins.fetch_add(1, Ordering::SeqCst);
}

/// Whether `path` is an `smb://` path.
pub fn is_smb(path: &Path) -> bool {
    Scheme::of(path) == Some(Scheme::Smb)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The SMB client as a [`Remote`].
pub(crate) fn remote() -> Arc<dyn Remote> {
    remote_inner().clone()
}

fn remote_inner() -> &'static Arc<Smb> {
    static R: OnceLock<Arc<Smb>> = OnceLock::new();
    R.get_or_init(|| Arc::new(Smb::default()))
}

/// One logged-in session per server, shared by listings, reads and uploads (requests take
/// turns on its one connection).
// ponytail: one request at a time per server (no pipelining, no second connection for
// uploads); a read opens the file each time (create + read + close: an output being written
// is read back with its size now, not a cached handle's).
#[derive(Default)]
struct Smb {
    sessions: Mutex<HashMap<String, Session>>,
    uploads: Mutex<HashMap<u64, Upload>>,
    next: AtomicU64,
    /// Bumped (under `sessions`) whenever the logins change: a login that began before is
    /// used once but not cached.
    logins: AtomicU64,
}

/// A file created for writing; closed when dropped.
struct Upload {
    at: FlinkPath,
    file: Arc<File>,
}

fn login(at: &FlinkPath) -> io::Result<Session> {
    let (host, port) = at.host_port();
    let cred = lock(credentials())
        .iter()
        .find(|c| c.port == port && c.host.trim_matches(['[', ']']).eq_ignore_ascii_case(host))
        .cloned();
    let (user, pass) = cred.map_or_else(Default::default, |c| (c.user, c.password));
    Session::connect(host, port, &user, &pass)
}

/// A connection-level failure (worth one fresh login), not the server's answer.
fn retry(e: &io::Error) -> bool {
    !matches!(
        e.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::IsADirectory
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::InvalidData
            | io::ErrorKind::AlreadyExists
            | io::ErrorKind::DirectoryNotEmpty
            | io::ErrorKind::StorageFull
            | io::ErrorKind::ReadOnlyFilesystem
            | io::ErrorKind::ResourceBusy
            | io::ErrorKind::Unsupported
            | io::ErrorKind::CrossesDevices
    )
}

/// `at`'s share and the path within it (`""` for the share's root).
fn split(at: &FlinkPath) -> (&str, &str) {
    at.path.split_once('/').unwrap_or((&at.path, ""))
}

/// `at`'s share and path when it is inside a share (not the server itself).
fn in_share(at: &FlinkPath) -> io::Result<(&str, &str)> {
    if at.path.is_empty() {
        return Err(invalid(at, "name a share"));
    }
    Ok(split(at))
}

/// `at`'s share and path when it is below a share's root (what can be made, renamed or
/// removed).
fn below_share(at: &FlinkPath) -> io::Result<(&str, &str)> {
    match split(at) {
        (_, "") => Err(invalid(at, "a server or share itself can't be changed")),
        parts => Ok(parts),
    }
}

fn invalid(at: &FlinkPath, why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{}: {why}", at.shown()),
    )
}

fn named(at: &FlinkPath, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {e}", at.shown()))
}

fn mtime(secs: i64) -> Option<SystemTime> {
    let secs = u64::try_from(secs).ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

/// A folder with no size or time: the server, as its shares' parent.
const FOLDER: RemoteStat = RemoteStat {
    dir: true,
    size: 0,
    mtime: None,
    link: false,
};

impl Smb {
    /// `f` on the server's session (made when there is none); a connection-level failure on a
    /// cached session logs in once more and tries again.
    // ponytail: a request whose reply was lost is sent again: a create, mkdir or rename that
    // the server did then answers AlreadyExists or NotFound (a created `.part` stays behind).
    fn with<T>(&self, at: &FlinkPath, f: impl Fn(&Session) -> io::Result<T>) -> io::Result<T> {
        // Bound first: a guard in the `if let` would live through the login's own lock.
        let cached = lock(&self.sessions).get(&at.key_id).cloned();
        if let Some(session) = cached {
            match f(&session) {
                Err(e) if retry(&e) => {
                    lock(&self.sessions).remove(&at.key_id);
                }
                done => return done.map_err(|e| named(at, e)),
            }
        }
        let logins = self.logins.load(Ordering::SeqCst);
        let session = login(at).map_err(|e| named(at, e))?;
        let mut sessions = lock(&self.sessions);
        if self.logins.load(Ordering::SeqCst) == logins {
            sessions.insert(at.key_id.clone(), session.clone());
        }
        drop(sessions);
        f(&session).map_err(|e| named(at, e))
    }

    fn upload(&self, handle: u64) -> io::Result<(FlinkPath, Arc<File>)> {
        let uploads = lock(&self.uploads);
        let u = uploads.get(&handle).ok_or_else(|| no_upload(handle))?;
        Ok((u.at.clone(), u.file.clone()))
    }
}

impl Remote for Smb {
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
        if path.path.is_empty() {
            return Ok(FOLDER);
        }
        let (share, rest) = split(path);
        self.with(path, |s| {
            let e = s.stat(share, rest)?;
            Ok(RemoteStat {
                dir: e.dir,
                size: if e.dir { 0 } else { e.size },
                mtime: mtime(e.mtime),
                link: false,
            })
        })
    }

    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
        if path.path.is_empty() {
            let shares = self.with(path, |s| s.shares())?;
            return Ok(shares
                .into_iter()
                .map(|name| RemoteEntry {
                    name,
                    dir: true,
                    ..Default::default()
                })
                .collect());
        }
        let (share, rest) = split(path);
        let entries = self.with(path, |s| s.list(share, rest))?;
        Ok(entries
            .into_iter()
            .map(|e| RemoteEntry {
                dir: e.dir,
                size: if e.dir { 0 } else { e.size },
                mtime: mtime(e.mtime),
                name: e.name,
                ..Default::default()
            })
            .collect())
    }

    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let (share, rest) = in_share(path)?;
        self.with(path, |s| {
            let file = s.open(share, rest)?;
            // Grown as the bytes come, 16 MiB at a time: a server may list any size.
            let mut buf = Vec::new();
            while buf.len() < len {
                let have = buf.len();
                let want = (len - have).min(16 << 20);
                buf.resize(have + want, 0);
                let n = file.read_at(offset + have as u64, &mut buf[have..])?;
                buf.truncate(have + n);
                if n < want {
                    break;
                }
            }
            Ok(buf)
        })
    }

    fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
        let (share, rest) = below_share(path)?;
        self.with(path, |s| s.mkdir(share, rest))
    }

    fn create(&self, path: &FlinkPath) -> io::Result<u64> {
        let (share, rest) = below_share(path)?;
        let file = self.with(path, |s| s.create_file(share, rest))?;
        let handle = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let upload = Upload {
            at: path.clone(),
            file: Arc::new(file),
        };
        lock(&self.uploads).insert(handle, upload);
        Ok(handle)
    }

    fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let (at, file) = self.upload(handle)?;
        file.write_at(offset, bytes).map_err(|e| named(&at, e))
    }

    fn sync(&self, handle: u64) -> io::Result<()> {
        let (at, file) = self.upload(handle)?;
        file.flush().map_err(|e| named(&at, e))
    }

    fn close(&self, handle: u64) -> io::Result<()> {
        // The handle is closed when its last user (a write in flight) lets go of it.
        let upload = lock(&self.uploads).remove(&handle);
        match upload {
            Some(_) => Ok(()),
            None => Err(no_upload(handle)),
        }
    }

    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        let (share, old, new) = same_share(from, to)?;
        self.with(from, |s| s.rename(share, old, new))
    }

    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        let (share, old, new) = same_share(from, to)?;
        self.with(from, |s| s.rename_replace(share, old, new))
    }

    fn remove(&self, path: &FlinkPath) -> io::Result<()> {
        let (share, rest) = below_share(path)?;
        self.with(path, |s| s.remove(share, rest))
    }

    fn free_space(&self, dir: &FlinkPath) -> io::Result<u64> {
        let (share, rest) = in_share(dir)?;
        self.with(dir, |s| s.free_space(share, rest))
    }
}

/// `from`'s share and both paths below it, when `to` is on the same share of the same server.
fn same_share<'a>(
    from: &'a FlinkPath,
    to: &'a FlinkPath,
) -> io::Result<(&'a str, &'a str, &'a str)> {
    let (share, old) = below_share(from)?;
    let (to_share, new) = below_share(to)?;
    if from.key_id != to.key_id || !share.eq_ignore_ascii_case(to_share) {
        return Err(io::Error::new(
            io::ErrorKind::CrossesDevices,
            format!(
                "{} -> {}: can't move between shares",
                from.shown(),
                to.shown()
            ),
        ));
    }
    Ok((share, old, new))
}

fn no_upload(handle: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no SMB upload {handle}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::FlinkPath;

    #[test]
    fn local_paths_with_wide_characters_are_not_smb() {
        for p in [
            "C:\\Игры\\game",
            "/mnt/Игры",
            "/tmp/ü/x",
            "日本語/ゲーム",
            "smb:/",
            "smb://",
        ] {
            assert!(!is_smb(Path::new(p)), "{p}");
        }
        assert!(is_smb(Path::new("SMB://nas/games")));
        assert!(is_smb(Path::new("smb://h/s")) && !is_smb(Path::new("smb:x")));
    }

    #[test]
    fn parses_and_spells_paths() {
        let p =
            FlinkPath::parse(Path::new("SMB://NAS.local:4455/games/Stub Game/sce_sys")).unwrap();
        assert_eq!(p.scheme, Scheme::Smb);
        assert_eq!(p.key_id, "nas.local:4455");
        assert_eq!(p.host_port(), ("nas.local", 4455));
        assert_eq!(split(&p), ("games", "Stub Game/sce_sys"));
        assert_eq!(
            p.url(),
            Path::new("smb://nas.local:4455/games/Stub Game/sce_sys")
        );
        let up = p.parent().unwrap().parent().unwrap();
        assert_eq!(up.url(), Path::new("smb://nas.local:4455/games"));
        assert_eq!(split(&up), ("games", ""));
        let server = up.parent().unwrap();
        assert_eq!(server.url(), Path::new("smb://nas.local:4455"));
        assert!(server.parent().is_none());

        let p = FlinkPath::parse(Path::new("smb://nas:445/s")).unwrap();
        assert_eq!((p.key_id.as_str(), p.host_port()), ("nas", ("nas", 445)));
        let v6 = FlinkPath::parse(Path::new("smb://[FE80::1]/s")).unwrap();
        assert_eq!(v6.key_id, "[fe80::1]");
        assert_eq!(v6.host_port(), ("fe80::1", 445));
        assert_eq!(v6.url(), Path::new("smb://[fe80::1]/s"));
        let v6 = FlinkPath::parse(Path::new("smb://[fe80::1]:4455/s/x")).unwrap();
        assert_eq!(v6.host_port(), ("fe80::1", 4455));
        for bad in [
            "smb://",
            "smb://h:0/s",
            "smb://h/s/../x",
            "smb://h/s/./x",
            "smb://u@h/s",
            "/local/x",
        ] {
            assert!(FlinkPath::parse(Path::new(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn refusals() {
        let refuse = crate::net::refuse;
        assert!(refuse(Path::new("smb://h/s/g"), "an edit").is_err());
        assert!(refuse(Path::new("/local/g"), "an edit").is_ok());
        let cred = SmbCredential {
            host: "h".into(),
            port: 445,
            user: "u".into(),
            password: "secret".into(),
        };
        assert!(!format!("{cred:?}").contains("secret"));
        // Nothing is made, renamed or removed at the server or a share itself, and nothing
        // moves between shares; none of these reach the network.
        let at = |s: &str| FlinkPath::parse(Path::new(s)).unwrap();
        let smb = Smb::default();
        let kind = |r: io::Result<()>| r.unwrap_err().kind();
        assert_eq!(
            kind(smb.mkdir(&at("smb://h/s"))),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            smb.create(&at("smb://h")).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            kind(smb.remove(&at("smb://h/s"))),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            kind(smb.rename_no_replace(&at("smb://h/s"), &at("smb://h/t"))),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            kind(smb.rename_no_replace(&at("smb://h/s/a"), &at("smb://h/t/a"))),
            io::ErrorKind::CrossesDevices
        );
        assert_eq!(
            smb.free_space(&at("smb://h")).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(smb.stat(&at("smb://h")).unwrap(), FOLDER);
        assert_eq!(kind(smb.write_at(7, 0, b"x")), io::ErrorKind::InvalidInput);
    }

    /// The adapter against a real server: `FORGE_SMB_TEST=smb://host[:port]/share` (guest),
    /// the share holding a writable `out` folder (tests/scripts/nas.sh).
    #[test]
    fn a_share_is_read_and_written() {
        let Ok(url) = std::env::var("FORGE_SMB_TEST") else {
            eprintln!("skipped: set FORGE_SMB_TEST");
            return;
        };
        let share = FlinkPath::parse(Path::new(&url)).unwrap();
        let smb = Smb::default();
        let server = share.parent().unwrap();
        let shares = smb.list(&server).unwrap();
        assert!(
            shares.iter().any(|e| e.dir && e.name == share.path),
            "{shares:?}"
        );
        assert!(smb.stat(&share).unwrap().dir);

        let out = share.join("out");
        let dir = out.join(&format!("adapter-{}", std::process::id()));
        smb.mkdir(&dir).unwrap();
        assert_eq!(
            smb.mkdir(&dir).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let tmp = dir.join("a.tmp");
        let h = smb.create(&tmp).unwrap();
        assert_eq!(
            smb.create(&tmp).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let bytes: Vec<u8> = (0..3 << 20).map(|i| (i % 253) as u8).collect();
        smb.write_at(h, 1 << 20, &bytes[1 << 20..]).unwrap();
        smb.write_at(h, 0, &bytes[..1 << 20]).unwrap();
        smb.sync(h).unwrap();
        // Read back while still open, with its size now.
        assert_eq!(smb.read_at(&tmp, 0, bytes.len() + 9).unwrap(), bytes);
        smb.close(h).unwrap();

        let file = dir.join("a.bin");
        smb.rename_no_replace(&tmp, &file).unwrap();
        let h = smb.create(&tmp).unwrap();
        smb.close(h).unwrap();
        let kind = smb.rename_no_replace(&tmp, &file).unwrap_err().kind();
        assert_eq!(kind, io::ErrorKind::AlreadyExists);
        let stat = smb.stat(&file).unwrap();
        let listed = smb.list(&dir).unwrap();
        let entry = listed.iter().find(|e| e.name == "a.bin").unwrap();
        assert_eq!((stat.size, stat.mtime), (entry.size, entry.mtime));
        assert!(stat.mtime.is_some() && !stat.dir);
        assert_eq!(smb.read_at(&file, 5, 3).unwrap(), &bytes[5..8]);
        assert!(smb.free_space(&dir).unwrap() > 0);

        smb.remove(&tmp).unwrap();
        smb.remove(&file).unwrap();
        assert_eq!(smb.stat(&file).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(smb.list(&dir).unwrap().is_empty());
        // `remove` takes files only; the empty folder goes through the share's folder here.
        if let Ok(local) = std::env::var("FORGE_SMB_TEST_SHARE_DIR") {
            let rel = &dir.path[share.path.len() + 1..];
            std::fs::remove_dir(Path::new(&local).join(rel)).unwrap();
        }
    }
}
