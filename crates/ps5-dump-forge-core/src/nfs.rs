//! NFS servers (`nfs://host[:port]/<server path>`, the server's absolute path) as a
//! [`crate::remote::Remote`], over `ps5-dump-forge-nfs` (v4.1, else v3); `nfs://host` lists
//! above the exports. No login: AUTH_SYS uid 0.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ps5_dump_forge_nfs::{DEFAULT_PORT, File, Session};

use crate::remote::{FlinkPath, Remote, RemoteEntry, RemoteStat};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn remote() -> Arc<dyn Remote> {
    remote_inner().clone()
}

fn remote_inner() -> &'static Arc<Nfs> {
    static R: OnceLock<Arc<Nfs>> = OnceLock::new();
    R.get_or_init(|| Arc::new(Nfs::default()))
}

/// One session per server, shared by listings, reads and uploads.
// ponytail: one request at a time per server; an upload stays on its session (a broken
// connection fails it).
#[derive(Default)]
struct Nfs {
    sessions: Mutex<HashMap<String, Session>>,
    uploads: Mutex<HashMap<u64, Upload>>,
    next: AtomicU64,
}

/// A file created for writing; closed (v4: its open state) when its last user drops it.
struct Upload {
    at: FlinkPath,
    file: Arc<File>,
}

fn connect(at: &FlinkPath) -> io::Result<Session> {
    let (host, port) = at.host_port();
    Session::connect(host, (port != DEFAULT_PORT).then_some(port))
}

/// A connection-level failure (worth one fresh connection; the client stays broken after one).
fn retry(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::TimedOut
            | io::ErrorKind::WouldBlock
    )
}

fn abs(at: &FlinkPath) -> String {
    format!("/{}", at.path)
}

/// `e` led by the server (the crate's text names the path).
fn named(at: &FlinkPath, e: io::Error) -> io::Error {
    let server = FlinkPath {
        path: String::new(),
        ..at.clone()
    };
    io::Error::new(e.kind(), format!("{}: {e}", server.shown()))
}

fn stat_of(e: &ps5_dump_forge_nfs::Entry) -> RemoteStat {
    RemoteStat {
        dir: e.dir && !e.link,
        size: if e.dir || e.link { 0 } else { e.size },
        mtime: e.mtime,
        link: e.link,
    }
}

impl Nfs {
    /// `f` on the server's session; a connection-level failure on a cached one connects once more.
    // ponytail: a request whose reply was lost is sent again (a create may then answer AlreadyExists).
    fn with<T>(&self, at: &FlinkPath, f: impl Fn(&Session) -> io::Result<T>) -> io::Result<T> {
        // Bound first: a guard in the `if let` would live through the connect.
        let cached = lock(&self.sessions).get(&at.key_id).cloned();
        if let Some(session) = cached {
            match f(&session) {
                Err(e) if retry(&e) => {
                    lock(&self.sessions).remove(&at.key_id);
                }
                done => return done.map_err(|e| named(at, e)),
            }
        }
        let session = connect(at).map_err(|e| named(at, e))?;
        lock(&self.sessions).insert(at.key_id.clone(), session.clone());
        f(&session).map_err(|e| named(at, e))
    }

    fn upload(&self, handle: u64) -> io::Result<(FlinkPath, Arc<File>)> {
        let uploads = lock(&self.uploads);
        let u = uploads.get(&handle).ok_or_else(|| no_upload(handle))?;
        Ok((u.at.clone(), u.file.clone()))
    }
}

/// The server, as its exports' parent.
const FOLDER: RemoteStat = RemoteStat {
    dir: true,
    size: 0,
    mtime: None,
    link: false,
};

impl Nfs {
    /// The export `at` is strictly inside (the longest); `None` at or above an export, never changed.
    fn export_of(&self, at: &FlinkPath) -> Result<Option<String>, crate::DeleteError> {
        if at.path.is_empty() {
            return Ok(None);
        }
        let exports = self.with(at, |s| s.exports()).map_err(|e| {
            crate::DeleteError::Failed(format!("listing the exports of {}: {e}", at.shown()))
        })?;
        Ok(inside_export(&abs(at), &exports))
    }
}

fn inside_export(path: &str, exports: &[String]) -> Option<String> {
    let path = std::path::Path::new(path);
    if exports
        .iter()
        .any(|e| std::path::Path::new(e).starts_with(path))
    {
        return None;
    }
    exports
        .iter()
        .filter(|e| path.starts_with(e))
        .max_by_key(|e| e.len())
        .cloned()
}

impl Remote for Nfs {
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
        if path.path.is_empty() {
            return Ok(FOLDER);
        }
        self.with(path, |s| s.stat(&abs(path)).map(|e| stat_of(&e)))
    }

    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
        let entries = self.with(path, |s| s.list(&abs(path)))?;
        Ok(entries
            .into_iter()
            .map(|e| {
                let s = stat_of(&e);
                RemoteEntry {
                    dir: s.dir,
                    size: s.size,
                    mtime: s.mtime,
                    link: s.link,
                    special: e.special,
                    bad_name: false,
                    name: e.name,
                }
            })
            .collect())
    }

    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.with(path, |s| s.read_at(&abs(path), offset, len))
    }

    fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
        self.with(path, |s| s.mkdir(&abs(path)))
    }

    fn create(&self, path: &FlinkPath) -> io::Result<u64> {
        let file = self.with(path, |s| s.create(&abs(path)))?;
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
        file.sync().map_err(|e| named(&at, e))
    }

    fn close(&self, handle: u64) -> io::Result<()> {
        // Closed when its last user (a write in flight) lets go of it.
        let upload = lock(&self.uploads).remove(&handle);
        match upload {
            Some(_) => Ok(()),
            None => Err(no_upload(handle)),
        }
    }

    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        same_server(from, to)?;
        self.with(from, |s| s.rename_no_replace(&abs(from), &abs(to)))
    }

    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        same_server(from, to)?;
        self.with(from, |s| s.rename_replace(&abs(from), &abs(to)))
    }

    fn remove(&self, path: &FlinkPath) -> io::Result<()> {
        self.with(path, |s| s.remove(&abs(path)))
    }

    fn rmdir(&self, path: &FlinkPath) -> io::Result<()> {
        self.with(path, |s| s.rmdir(&abs(path)))
    }

    // NFSv4 names no exports: an export root shows as an fsid differing from its parent's.
    fn fs_id(&self, path: &FlinkPath) -> io::Result<Option<u128>> {
        if path.path.is_empty() {
            return Ok(None);
        }
        self.with(path, |s| s.stat(&abs(path)).map(|e| e.fsid))
    }

    fn delete_path(&self, path: &FlinkPath) -> Result<(), crate::DeleteError> {
        if self.export_of(path)?.is_none() {
            return Err(crate::delete::nas_root(path));
        }
        crate::delete::nas_delete(self, path)
    }

    fn move_path(
        &self,
        from: &FlinkPath,
        to: &FlinkPath,
        replace: bool,
    ) -> Result<(), crate::DeleteError> {
        let Some(export) = self.export_of(from)? else {
            return Err(crate::delete::nas_root(from));
        };
        match self.export_of(to)? {
            None => Err(crate::delete::nas_root(to)),
            Some(e) if e != export || from.key_id != to.key_id => {
                Err(crate::delete::nas_other_share(from, to))
            }
            Some(_) => crate::delete::nas_move(self, from, to, replace),
        }
    }

    fn free_space(&self, dir: &FlinkPath) -> io::Result<u64> {
        self.with(dir, |s| s.free_space(&abs(dir)))
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
        format!("no NFS upload {handle}"),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn at(s: &str) -> FlinkPath {
        FlinkPath::parse(Path::new(s)).unwrap()
    }

    #[test]
    fn refusals() {
        let refuse = crate::net::refuse;
        assert!(refuse(Path::new("nfs://h/srv/g"), "an edit").is_err());
        let nfs = Nfs::default();
        assert_eq!(nfs.stat(&at("nfs://h")).unwrap(), FOLDER);
        let kind = |r: io::Result<()>| r.unwrap_err().kind();
        assert_eq!(kind(nfs.write_at(7, 0, b"x")), io::ErrorKind::InvalidInput);
        assert_eq!(kind(nfs.sync(7)), io::ErrorKind::InvalidInput);
        assert_eq!(kind(nfs.close(7)), io::ErrorKind::InvalidInput);
        assert_eq!(
            kind(nfs.rename_no_replace(&at("nfs://a/s/x"), &at("nfs://b/s/x"))),
            io::ErrorKind::CrossesDevices
        );
        assert!(retry(&io::Error::from(io::ErrorKind::BrokenPipe)));
        assert!(!retry(&io::Error::from(io::ErrorKind::AlreadyExists)));
        assert!(!retry(&io::Error::from(io::ErrorKind::NotFound)));
    }

    #[test]
    fn links_are_never_folders() {
        let e = ps5_dump_forge_nfs::Entry {
            name: "l".into(),
            dir: true,
            size: 9,
            mtime: None,
            link: true,
            special: false,
            fsid: None,
        };
        let s = stat_of(&e);
        assert!(s.link && !s.dir && s.size == 0);
    }
}
