//! A small blocking SMB2 client (MS-SMB2): dialects 2.0.2 and 2.1, NTLMv2 or guest/anonymous,
//! HMAC-SHA256 signing when the server requires it. Lists shares and folders, reads files and
//! writes new ones (never over anything: files are created exclusively and renamed without
//! replacing). One TCP connection per [`Session`], one request at a time.
//! ponytail: no SMB 3.x (a server set to "SMB3 only" refuses it), no encryption, no DFS, no
//! reconnect; responses' signatures are not checked.

mod ntlm;
mod rpc;
mod wire;

use std::collections::HashMap;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wire::{Conn, Open, Req};

pub const DEFAULT_PORT: u16 = 445;
const TIMEOUT: Duration = Duration::from_secs(30);
/// Entries one folder listing may hold (Forge's folder walks stop at 200,000 too).
const MAX_LIST: usize = 200_000;

/// A directory entry or a stat result. `mtime` is seconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
}

/// One authenticated connection to a server. Clones share the connection.
#[derive(Clone)]
pub struct Session(Arc<Inner>);

struct Inner {
    host: String,
    conn: Mutex<Conn>,
    trees: Mutex<HashMap<String, u32>>,
}

/// An open file (for reading, or a new one for writing); closed on drop.
pub struct File {
    session: Session,
    tree: u32,
    id: [u8; 16],
    size: u64,
    mtime: i64,
}

/// Whether an SMB2 server answers on `tcp` (connected by the caller, who bounds the whole
/// exchange): a NEGOTIATE gets an SMB2 reply (any status: a server wanting SMB 3 refuses our
/// dialects, yet it is one), `timeout` for each read and write.
pub fn probe(tcp: TcpStream, timeout: Duration) -> io::Result<()> {
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    Conn::new(tcp).send(Conn::negotiate_req()?).map(drop)
}

impl Session {
    /// Connects and logs in: `user` empty is an anonymous (guest) login; `DOMAIN\user` names a
    /// domain.
    pub fn connect(host: &str, port: u16, user: &str, pass: &str) -> io::Result<Session> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("{host}: no address"))
        })?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        tcp.set_write_timeout(Some(TIMEOUT))?;
        tcp.set_nodelay(true)?;
        let mut conn = Conn::new(tcp);
        conn.negotiate()?;
        conn.login(user, pass)?;
        Ok(Session(Arc::new(Inner {
            host: host.to_string(),
            conn: Mutex::new(conn),
            trees: Mutex::new(HashMap::new()),
        })))
    }

    /// The disk shares (names without a trailing `$`), via srvsvc NetShareEnumAll.
    pub fn shares(&self) -> io::Result<Vec<String>> {
        let tree = self.tree("IPC$")?;
        let pipe = self.create(tree, "srvsvc", Open::Read { pipe: true })?;
        let out = rpc::share_enum(&self.0.host, |req| {
            self.conn().ioctl(tree, &pipe.0, 0x0011_C017, req)
        });
        self.close(tree, &pipe.0);
        let mut names: Vec<String> = out?.into_iter().filter(|n| !n.ends_with('$')).collect();
        names.sort();
        Ok(names)
    }

    /// The entries of a folder (`path` `/`-separated, `""` for the share's root), without `.`
    /// and `..`.
    pub fn list(&self, share: &str, path: &str) -> io::Result<Vec<Entry>> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::Read { pipe: false })?;
        let mut out = Vec::new();
        let mut first = true;
        let res = loop {
            match self.conn().query_dir(tree, &id, first) {
                // A server that never says "no more files" can't grow this without bound.
                Ok(Some(_)) if out.len() >= MAX_LIST => {
                    break Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("SMB: a folder of more than {MAX_LIST} entries"),
                    ));
                }
                Ok(Some(batch)) => out.extend(batch),
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            }
            first = false;
        };
        self.close(tree, &id);
        res?;
        out.retain(|e: &Entry| e.name != "." && e.name != "..");
        Ok(out)
    }

    /// What a path is (the name is its last component; `""` for the root).
    pub fn stat(&self, share: &str, path: &str) -> io::Result<Entry> {
        let tree = self.tree(share)?;
        let (id, entry) = self.create(tree, path, Open::Read { pipe: false })?;
        self.close(tree, &id);
        Ok(entry)
    }

    /// A new file at `path` for [`File::write_at`] (`AlreadyExists` when anything is there).
    pub fn create_file(&self, share: &str, path: &str) -> io::Result<File> {
        let tree = self.tree(share)?;
        let (id, e) = self.create(tree, path, Open::NewFile)?;
        Ok(File {
            session: self.clone(),
            tree,
            id,
            size: 0,
            mtime: e.mtime,
        })
    }

    /// Makes the folder `path` (its parent must exist; `AlreadyExists` when it does).
    pub fn mkdir(&self, share: &str, path: &str) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::NewDir)?;
        self.close(tree, &id);
        Ok(())
    }

    /// Renames a file or folder within the share, never replacing `to` (`AlreadyExists`).
    pub fn rename(&self, share: &str, from: &str, to: &str) -> io::Result<()> {
        self.rename_as(share, from, to, false)
    }

    /// Renames the file `from` within the share over the file `to` (or to a free name); the
    /// server refuses to replace a folder.
    pub fn rename_replace(&self, share: &str, from: &str, to: &str) -> io::Result<()> {
        self.rename_as(share, from, to, true)
    }

    fn rename_as(&self, share: &str, from: &str, to: &str, replace: bool) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, from, Open::Rename)?;
        let to = to.trim_matches('/').replace('/', "\\");
        let r = self.conn().rename(tree, &id, &to, replace);
        self.close(tree, &id);
        r
    }

    /// Deletes the file `path`.
    pub fn remove(&self, share: &str, path: &str) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::Delete)?;
        // The delete happens at the close: its answer is the delete's.
        let r = self.conn().send(Req::close(tree, &id))?;
        match r.status {
            0 => Ok(()),
            s => Err(wire::status_error(s, path)),
        }
    }

    /// Bytes this login may still write on the volume holding the folder `path`.
    pub fn free_space(&self, share: &str, path: &str) -> io::Result<u64> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::Read { pipe: false })?;
        let r = self.conn().free_space(tree, &id);
        self.close(tree, &id);
        r
    }

    /// Opens a file for reading.
    pub fn open(&self, share: &str, path: &str) -> io::Result<File> {
        let tree = self.tree(share)?;
        let (id, e) = self.create(tree, path, Open::Read { pipe: false })?;
        if e.dir {
            self.close(tree, &id);
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{path}: a folder"),
            ));
        }
        Ok(File {
            session: self.clone(),
            tree,
            id,
            size: e.size,
            mtime: e.mtime,
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Conn> {
        self.0.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn tree(&self, share: &str) -> io::Result<u32> {
        let mut trees = self.0.trees.lock().unwrap_or_else(|e| e.into_inner());
        let key = share.to_lowercase();
        if let Some(&t) = trees.get(&key) {
            return Ok(t);
        }
        let t = self
            .conn()
            .tree_connect(&format!(r"\\{}\{share}", self.0.host))?;
        trees.insert(key, t);
        Ok(t)
    }

    fn create(&self, tree: u32, path: &str, how: Open) -> io::Result<([u8; 16], Entry)> {
        let name = path.trim_matches('/').replace('/', "\\");
        let leaf = name.rsplit('\\').next().unwrap_or("").to_string();
        let (id, mut e) = self.conn().create(tree, &name, how)?;
        e.name = leaf;
        Ok((id, e))
    }

    fn close(&self, tree: u32, id: &[u8; 16]) {
        let _ = self.conn().send(Req::close(tree, id));
    }
}

impl File {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn mtime(&self) -> i64 {
        self.mtime
    }

    /// Reads up to `buf.len()` bytes at `offset`; 0 at the end of the file.
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut done = 0;
        while done < buf.len() && offset + (done as u64) < self.size {
            let n = self.session.conn().read(
                self.tree,
                &self.id,
                offset + done as u64,
                &mut buf[done..],
            )?;
            if n == 0 {
                break;
            }
            done += n;
        }
        Ok(done)
    }
}

impl File {
    /// Writes all of `bytes` at `offset` (a file made by [`Session::create_file`]).
    pub fn write_at(&self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let mut done = 0;
        while done < bytes.len() {
            done += self.session.conn().write(
                self.tree,
                &self.id,
                offset + done as u64,
                &bytes[done..],
            )?;
        }
        Ok(())
    }

    /// Makes what was written durable on the server (FLUSH).
    pub fn flush(&self) -> io::Result<()> {
        self.session.conn().flush(self.tree, &self.id)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        self.session.close(self.tree, &self.id);
    }
}
