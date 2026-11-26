//! A small blocking NFS client: NFSv4.1 (RFC 8881) first, else NFSv3 (RFC 1813), over ONC RPC
//! on TCP. Absolute server paths; creates and renames never replace unless asked.
// ponytail: one TCP connection per [`Session`], requests serialized under a Mutex; no reconnect.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

mod rpc;
mod v3;
mod v4;

pub const DEFAULT_PORT: u16 = 2049;

/// Whether an NFS server answers on `stream`: an RPC NULL to NFS v3 (a version mismatch counts).
pub fn probe(stream: std::net::TcpStream, timeout: std::time::Duration) -> io::Result<()> {
    match rpc::Client::over(stream, timeout)?.null(v4::PROGRAM, 3) {
        Err(e) if matches!(rpc::rpc_error(&e), Some(rpc::RpcError::ProgMismatch { .. })) => Ok(()),
        r => r,
    }
}

/// `special`: neither a file, a folder nor a link (devices, FIFOs, sockets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub link: bool,
    pub special: bool,
    /// `fsid` (NFSv4 `major << 64 | minor`): an export or a filesystem mounted inside one starts
    /// where it changes.
    pub fsid: Option<u128>,
}

enum Proto {
    V3(v3::Nfs3),
    V4(v4::Nfs4),
}

/// Clones share the connection.
#[derive(Clone)]
pub struct Session(Arc<Mutex<Proto>>);

/// v4: its open state, closed on drop.
pub struct File {
    session: Session,
    path: String,
    open: Open,
}

enum Open {
    V3(v3::Open),
    V4(v4::Open),
}

macro_rules! each {
    ($proto:expr, $p:ident => $call:expr) => {
        match $proto {
            Proto::V3($p) => $call,
            Proto::V4($p) => $call,
        }
    };
}

impl Session {
    /// NFSv4.1 first (on `port`, else 2049), NFSv3 when the server has none (on `port`, else the
    /// portmapper's).
    pub fn connect(host: &str, port: Option<u16>) -> io::Result<Session> {
        let proto = match v4::Nfs4::connect(host, port) {
            Ok(c) => Proto::V4(c),
            Err(e) if fall_back(&e) => match v3::Nfs3::connect(host, port) {
                Ok(c) => Proto::V3(c),
                Err(e3) => {
                    return Err(io::Error::new(
                        e3.kind(),
                        format!("NFSv4.1 on {host}: {e}; NFSv3: {e3}"),
                    ));
                }
            },
            Err(e) => return Err(e),
        };
        Ok(Session(Arc::new(Mutex::new(proto))))
    }

    fn lock(&self) -> MutexGuard<'_, Proto> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn version(&self) -> &'static str {
        match *self.lock() {
            Proto::V3(_) => "NFSv3",
            Proto::V4(_) => "NFSv4.1",
        }
    }

    /// v4 may give `["/"]`: its pseudo-filesystem.
    pub fn exports(&self) -> io::Result<Vec<String>> {
        each!(&mut *self.lock(), p => p.exports())
    }

    pub fn stat(&self, path: &str) -> io::Result<Entry> {
        each!(&mut *self.lock(), p => p.stat(path)).map_err(|e| named(e, path))
    }

    /// A folder's entries without `.` and `..`; more than 200,000 is an error.
    pub fn list(&self, path: &str) -> io::Result<Vec<Entry>> {
        each!(&mut *self.lock(), p => p.list(path)).map_err(|e| named(e, path))
    }

    /// Fewer only at the end of the file.
    pub fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        each!(&mut *self.lock(), p => p.read_at(path, offset, len)).map_err(|e| named(e, path))
    }

    /// `AlreadyExists` when something is there.
    pub fn mkdir(&self, path: &str) -> io::Result<()> {
        each!(&mut *self.lock(), p => p.mkdir(path)).map_err(|e| named(e, path))
    }

    /// Never replacing one: `AlreadyExists` when taken.
    pub fn create(&self, path: &str) -> io::Result<File> {
        let open = match &mut *self.lock() {
            Proto::V3(p) => p.create(path).map(Open::V3),
            Proto::V4(p) => p.create(path).map(Open::V4),
        }
        .map_err(|e| named(e, path))?;
        Ok(File {
            session: self.clone(),
            path: path.to_string(),
            open,
        })
    }

    /// Never replacing what is at `to` (`AlreadyExists`).
    pub fn rename_no_replace(&self, from: &str, to: &str) -> io::Result<()> {
        each!(&mut *self.lock(), p => p.rename_no_replace(from, to))
            .map_err(|e| named(e, &format!("{from} -> {to}")))
    }

    /// Replacing a file there (never a folder).
    pub fn rename_replace(&self, from: &str, to: &str) -> io::Result<()> {
        each!(&mut *self.lock(), p => p.rename_replace(from, to))
            .map_err(|e| named(e, &format!("{from} -> {to}")))
    }

    pub fn remove(&self, path: &str) -> io::Result<()> {
        each!(&mut *self.lock(), p => p.remove(path)).map_err(|e| named(e, path))
    }

    pub fn rmdir(&self, path: &str) -> io::Result<()> {
        each!(&mut *self.lock(), p => p.rmdir(path)).map_err(|e| named(e, path))
    }

    pub fn free_space(&self, path: &str) -> io::Result<u64> {
        each!(&mut *self.lock(), p => p.free_space(path)).map_err(|e| named(e, path))
    }
}

impl File {
    /// Unstable: [`File::sync`] makes it durable.
    pub fn write_at(&self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        match (&mut *self.session.lock(), &self.open) {
            (Proto::V3(p), Open::V3(o)) => p.write_at(o, offset, bytes),
            (Proto::V4(p), Open::V4(o)) => p.write_at(o, offset, bytes),
            _ => unreachable!("a file opened by the session's own protocol"),
        }
        .map_err(|e| named(e, &self.path))
    }

    /// Fails when the server restarted since (its write verifier changed).
    pub fn sync(&self) -> io::Result<()> {
        match (&mut *self.session.lock(), &self.open) {
            (Proto::V3(p), Open::V3(o)) => p.sync(o),
            (Proto::V4(p), Open::V4(o)) => p.sync(o),
            _ => unreachable!("a file opened by the session's own protocol"),
        }
        .map_err(|e| named(e, &self.path))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let (Proto::V4(p), Open::V4(o)) = (&mut *self.session.lock(), &self.open) {
            p.close(o); // best effort
        }
    }
}

/// The server doesn't serve v4.1 (RPC mismatch, NFS4ERR_MINOR_VERS_MISMATCH as `Unsupported`)
/// or refused the connection (v3 may be on another port).
fn fall_back(e: &io::Error) -> bool {
    rpc::is_version_mismatch(e)
        || matches!(
            e.kind(),
            io::ErrorKind::Unsupported | io::ErrorKind::ConnectionRefused
        )
}

fn named(e: io::Error, path: &str) -> io::Error {
    io::Error::new(e.kind(), format!("{path}: {e}"))
}
