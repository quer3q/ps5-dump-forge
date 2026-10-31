//! FTP servers (`ftp://host[:port]/path`): a game source and an output, read and written through
//! the [`crate::remote::Remote`] calls a paired Forge's files use, over `ps5-dump-forge-ftp`.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ps5_dump_forge_ftp::Session;

use crate::remote::{FlinkPath, Remote, RemoteEntry, RemoteStat};

/// `host[:port]` as `ftp://` paths spell it ([`crate::remote::authority`]): lower case, the
/// port only when it isn't 21; `None` for anything else (a user name, a path, an empty host, a
/// bad port).
pub fn authority(s: &str) -> Option<String> {
    crate::remote::authority(s, ps5_dump_forge_ftp::DEFAULT_PORT)
}

/// A saved FTP server's login: `host[:port]` as `ftp://` paths spell it ([`authority`]); an empty
/// user is anonymous.
#[derive(Clone)]
pub struct FtpLogin {
    pub authority: String,
    pub user: String,
    pub password: String,
}

fn logins() -> &'static Mutex<HashMap<String, FtpLogin>> {
    static L: OnceLock<Mutex<HashMap<String, FtpLogin>>> = OnceLock::new();
    L.get_or_init(Mutex::default)
}

/// The saved FTP servers' logins (the server hands them over at start and on every change); a
/// server not listed is tried anonymously. Sessions made with an old login are dropped.
pub fn set_logins(list: Vec<FtpLogin>) {
    *lock(logins()) = list.into_iter().map(|l| (l.authority.clone(), l)).collect();
    let ftp = remote_inner();
    let mut sessions = lock(&ftp.sessions);
    sessions.clear();
    ftp.logins.fetch_add(1, Ordering::SeqCst);
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The FTP client as a [`Remote`].
pub(crate) fn remote() -> Arc<dyn Remote> {
    remote_inner().clone()
}

fn remote_inner() -> &'static Arc<Ftp> {
    static R: OnceLock<Arc<Ftp>> = OnceLock::new();
    R.get_or_init(|| Arc::new(Ftp::default()))
}

/// One shared session per server for listings, stats and reads, and a session of its own for
/// each upload (an FTP control connection is busy while a file goes up).
// ponytail: no reconnect beyond one fresh login when a cached session died (`with`); no
// read-ahead here (each `read_at` is one REST + RETR; `RemotePkgOut` keeps windows of its own).
#[derive(Default)]
struct Ftp {
    sessions: Mutex<HashMap<String, Session>>,
    uploads: Mutex<HashMap<u64, Upload>>,
    next: AtomicU64,
    /// Bumped (under `sessions`) whenever the logins change: a session that began logging in
    /// before is used once but not cached.
    logins: AtomicU64,
}

/// A file going up, through the crate's [`ps5_dump_forge_ftp::Writer`] (any offset, `REST` to go
/// back) on a session of its own.
struct Upload {
    at: FlinkPath,
    w: ps5_dump_forge_ftp::Writer,
}

/// Whether the FTP server holding `dir` can rewrite a file in place (an fpkg and LZ4 packs in a
/// folder go back over what they wrote): the crate's probe, with a probe file in `dir`.
pub(crate) fn can_rewrite(dir: &FlinkPath) -> io::Result<bool> {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let probe = dir.join(&format!(".forge-rest-probe-{}-{n}", std::process::id()));
    connect(&dir.key_id)?.can_rewrite(&abs(&probe))
}

fn connect(authority: &str) -> io::Result<Session> {
    let (host, port) = host_port(authority);
    let login = lock(logins()).get(authority).cloned();
    let (user, pass) = login.map_or((String::new(), String::new()), |l| (l.user, l.password));
    Session::connect(host, port, &user, &pass)
}

/// `host[:port]` (`[v6]` or `[v6]:port` for IPv6) as host and port (21 when not given).
fn host_port(authority: &str) -> (&str, u16) {
    crate::remote::host_port(authority, ps5_dump_forge_ftp::DEFAULT_PORT)
}

/// The server's absolute path for `at`.
fn abs(at: &FlinkPath) -> String {
    format!("/{}", at.path)
}

fn named(at: &FlinkPath, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {e}", at.url().display()))
}

fn stat_of(e: &ps5_dump_forge_ftp::Entry) -> RemoteStat {
    RemoteStat {
        dir: e.dir,
        size: if e.dir { 0 } else { e.size },
        mtime: mtime(e.mtime),
        link: e.link,
    }
}

/// A failure of the connection, not the server's answer: the session is dead. A timeout counts
/// too (a late reply would answer the next command), though it isn't retried ([`timed_out`]).
fn dead(e: &io::Error) -> bool {
    timed_out(e)
        || matches!(
            e.kind(),
            io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::UnexpectedEof
        )
}

/// A socket timeout: `WouldBlock` on unix, `TimedOut` on Windows.
fn timed_out(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn mtime(secs: Option<i64>) -> Option<SystemTime> {
    let secs = u64::try_from(secs?).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

impl Ftp {
    /// `f` on the server's shared session (made when there is none); a failure that isn't the
    /// server's answer drops the session, and on a cached one (a server closes idle sessions,
    /// vsftpd after 300 s) `f` runs once more on a fresh login.
    // ponytail: as smb.rs, a request whose reply was lost is sent again: a mkdir or rename that
    // took may then answer AlreadyExists / NotFound. Calls that find no session at once each log
    // in (the last one is cached).
    fn with<T>(&self, at: &FlinkPath, f: impl Fn(&Session) -> io::Result<T>) -> io::Result<T> {
        // Bound first: a guard in the `if let` would live through the login's own lock.
        let cached = lock(&self.sessions).get(&at.key_id).cloned();
        if let Some(session) = cached {
            match f(&session) {
                Err(e) if dead(&e) => {
                    // Only this session: another call may have cached a fresh one meanwhile.
                    let mut sessions = lock(&self.sessions);
                    if sessions.get(&at.key_id).is_some_and(|s| s.same(&session)) {
                        sessions.remove(&at.key_id);
                    }
                    drop(sessions);
                    if timed_out(&e) {
                        return Err(named(at, e));
                    }
                }
                done => return done.map_err(|e| named(at, e)),
            }
        }
        let logins = self.logins.load(Ordering::SeqCst);
        let session = connect(&at.key_id).map_err(|e| named(at, e))?;
        let r = f(&session);
        let mut sessions = lock(&self.sessions);
        if !matches!(&r, Err(e) if dead(e)) && self.logins.load(Ordering::SeqCst) == logins {
            sessions.insert(at.key_id.clone(), session);
        }
        drop(sessions);
        r.map_err(|e| named(at, e))
    }
}

impl Remote for Ftp {
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
        if path.path.is_empty() {
            return Ok(RemoteStat {
                dir: true,
                size: 0,
                mtime: None,
                link: false,
            });
        }
        self.with(path, |s| s.stat(&abs(path)).map(|e| stat_of(&e)))
    }

    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
        let entries = self.with(path, |s| s.list(&abs(path)))?;
        Ok(entries
            .into_iter()
            .map(|e| RemoteEntry {
                dir: e.dir,
                size: if e.dir { 0 } else { e.size },
                mtime: mtime(e.mtime),
                link: e.link,
                special: false,
                bad_name: false,
                name: e.name,
            })
            .collect())
    }

    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        // A file still going up: its open `STOR` is completed first.
        for u in lock(&self.uploads).values_mut().filter(|u| &u.at == path) {
            u.w.pause().map_err(|e| named(path, e))?;
        }
        self.with(path, |s| s.read_at(&abs(path), offset, len))
    }

    fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
        self.with(path, |s| s.mkdir(&abs(path)))
    }

    fn create(&self, path: &FlinkPath) -> io::Result<u64> {
        // Never over a file there: FTP's STOR would replace it.
        match self.stat(path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{}: already exists", path.url().display()),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        // ponytail: the check and the STOR are two steps; another writer between them is replaced.
        let session = connect(&path.key_id).map_err(|e| named(path, e))?;
        let w = session.writer(&abs(path)).map_err(|e| named(path, e))?;
        let handle = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let upload = Upload {
            at: path.clone(),
            w,
        };
        lock(&self.uploads).insert(handle, upload);
        Ok(handle)
    }

    // ponytail: one lock over every upload's network calls (one transfer runs at a time).
    fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let mut uploads = lock(&self.uploads);
        let u = uploads.get_mut(&handle).ok_or_else(|| no_upload(handle))?;
        let at = u.at.clone();
        u.w.write_at(offset, bytes).map_err(|e| named(&at, e))
    }

    // ponytail: FTP has no sync: the upload is completed (the server has every byte); what it
    // keeps on its disk is its own matter.
    fn sync(&self, handle: u64) -> io::Result<()> {
        let mut uploads = lock(&self.uploads);
        uploads
            .get_mut(&handle)
            .ok_or_else(|| no_upload(handle))?
            .w
            .pause()
    }

    fn close(&self, handle: u64) -> io::Result<()> {
        let upload = lock(&self.uploads).remove(&handle);
        match upload {
            Some(u) => u.w.finish(),
            None => Err(no_upload(handle)),
        }
    }

    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        same_server(from, to)?;
        self.with(from, |s| s.rename(&abs(from), &abs(to)))
    }

    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        same_server(from, to)?;
        self.with(from, |s| s.rename_replace(&abs(from), &abs(to)))
    }

    fn remove(&self, path: &FlinkPath) -> io::Result<()> {
        self.with(path, |s| s.remove(&abs(path)))
    }

    // ponytail: FTP can't tell free space; a full disk fails the upload itself.
    fn free_space(&self, _: &FlinkPath) -> io::Result<u64> {
        Ok(u64::MAX)
    }
}

fn same_server(from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
    if from.key_id != to.key_id {
        return Err(io::Error::new(
            io::ErrorKind::CrossesDevices,
            format!(
                "{} -> {}: can't move between servers",
                from.shown(),
                to.shown()
            ),
        ));
    }
    Ok(())
}

fn no_upload(handle: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no FTP upload {handle}"),
    )
}
