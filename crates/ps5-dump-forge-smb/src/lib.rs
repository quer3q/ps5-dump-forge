//! A small blocking SMB2 client (MS-SMB2 2.0.2/2.1, NTLMv2 or guest, signing when required).
//! One TCP connection per [`Session`], one request at a time.
//! ponytail: no SMB 3.x, encryption, DFS or reconnect; responses' signatures are not checked.

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
const MAX_LIST: usize = 200_000;

/// A directory entry or a stat result; `mtime` in Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
    /// A reparse point (link, junction): never entered; a delete removes the link itself.
    pub link: bool,
}

#[derive(Clone)]
pub struct Session(Arc<Inner>);

struct Inner {
    host: String,
    conn: Mutex<Conn>,
    trees: Mutex<HashMap<String, u32>>,
}

/// Closed on drop.
pub struct File {
    session: Session,
    tree: u32,
    id: [u8; 16],
    size: u64,
    mtime: i64,
}

/// Whether an SMB2 server answers on `tcp`: any NEGOTIATE reply counts (an SMB3-only one too).
pub fn probe(tcp: TcpStream, timeout: Duration) -> io::Result<()> {
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    Conn::new(tcp).send(Conn::negotiate_req()?).map(drop)
}

impl Session {
    /// `user` empty is a guest login; `DOMAIN\user` names a domain.
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

    /// The disk shares (no trailing `$`), via srvsvc NetShareEnumAll.
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

    /// `path` `/`-separated, `""` for the share's root; no `.` and `..`.
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

    pub fn stat(&self, share: &str, path: &str) -> io::Result<Entry> {
        let tree = self.tree(share)?;
        let (id, entry) = self.create(tree, path, Open::Read { pipe: false })?;
        self.close(tree, &id);
        Ok(entry)
    }

    /// Exclusive: `AlreadyExists` when anything is there.
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

    pub fn mkdir(&self, share: &str, path: &str) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::NewDir)?;
        self.close(tree, &id);
        Ok(())
    }

    /// Never replaces `to` (`AlreadyExists`).
    pub fn rename(&self, share: &str, from: &str, to: &str) -> io::Result<()> {
        self.rename_as(share, from, to, false)
    }

    /// A file over a file (the server refuses to replace a folder).
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

    pub fn remove(&self, share: &str, path: &str) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::Delete)?;
        // The delete happens at the close.
        let r = self.conn().send(Req::close(tree, &id))?;
        match r.status {
            0 => Ok(()),
            s => Err(wire::status_error(s, path)),
        }
    }

    pub fn rmdir(&self, share: &str, path: &str) -> io::Result<()> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::DeleteDir)?;
        let r = self.conn().send(Req::close(tree, &id))?;
        match r.status {
            0 => Ok(()),
            s => Err(wire::status_error(s, path)),
        }
    }

    pub fn free_space(&self, share: &str, path: &str) -> io::Result<u64> {
        let tree = self.tree(share)?;
        let (id, _) = self.create(tree, path, Open::Read { pipe: false })?;
        let r = self.conn().free_space(tree, &id);
        self.close(tree, &id);
        r
    }

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

    pub fn flush(&self) -> io::Result<()> {
        self.session.conn().flush(self.tree, &self.id)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        self.session.close(self.tree, &self.id);
    }
}
