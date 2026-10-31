//! A paired Forge's files: paths `flink://<key_id>/<path>` (`path` relative to the peer's share
//! root: the PS5's `/`, a computer's home). Core never talks FLINK itself: the Connect service
//! registers a [`Remote`] once ([`set`]) and everything here goes through it ([`get`]). A folder
//! is served as a [`RemoteTree`]; an image is read through [`RemoteReader`]. A server's files
//! (`ftp://`, `smb://`, `nfs://`) go through the same calls, answered by [`crate::ftp`],
//! [`crate::smb`] and [`crate::nfs`].

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::scan::is_junk;

/// What [`Remote::stat`] says of a path. `mtime` is the same value [`Remote::list`] gives, so
/// a listing's entry and a later stat compare equal while the file is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteStat {
    pub dir: bool,
    /// A file's length; 0 for a folder.
    pub size: u64,
    pub mtime: Option<SystemTime>,
    /// A symbolic link (not followed: `dir` false, `size` 0); never read.
    pub link: bool,
}

/// One entry of a folder on a peer (links are not followed: the peer lists what they are).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteEntry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    /// A symbolic link (`dir`/`size` may describe its target); never walked or read.
    pub link: bool,
    /// A FIFO, socket or device: refused wherever a local scan refuses one.
    pub special: bool,
    /// A name that isn't UTF-8 on the peer (`name` is its lossy spelling): refused likewise.
    pub bad_name: bool,
}

/// Whether `name` survives a `flink://` round trip as one part (`parse` reads `\` as `/`).
pub(crate) fn addressable(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['/', '\\'])
}

/// A paired peer's files, as the Connect service reaches them. Every path is confined to the
/// peer's share root by the peer; every call is one request and may block on the network.
pub trait Remote: Send + Sync {
    /// The file or folder at `path` (`NotFound` when there is none).
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat>;
    /// A folder's entries, in any order; special files and names that aren't UTF-8 are marked
    /// (`RemoteEntry::special`, `bad_name`), and `read_at` refuses anything but a regular file.
    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>>;
    /// `len` bytes of the file at `path` from `offset`; fewer only where the file ends.
    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>>;
    /// [`Remote::read_at`], then the file's stat taken after the read (a peer may answer both
    /// in one round trip, which is what makes many small files quick).
    fn read_checked(
        &self,
        path: &FlinkPath,
        offset: u64,
        len: usize,
    ) -> io::Result<(Vec<u8>, RemoteStat)> {
        let bytes = self.read_at(path, offset, len)?;
        Ok((bytes, self.stat(path)?))
    }
    /// The whole new file `to` (at most [`PUT_MAX`] bytes) as `tmp`, created (never over
    /// anything), written, synced and closed, then renamed to `to` without replacing; `tmp`
    /// removed on any failure. A peer may do it all in one round trip, which is what makes many
    /// small files quick.
    fn put_new(&self, tmp: &FlinkPath, to: &FlinkPath, bytes: &[u8]) -> io::Result<()> {
        put(self, tmp, to, bytes, false)
    }
    /// Makes the folder `path` (its parent must exist; `AlreadyExists` when it does).
    fn mkdir(&self, path: &FlinkPath) -> io::Result<()>;
    /// Creates the file `path` for writing, never replacing one (`AlreadyExists`); the handle
    /// names it in the calls below until [`Remote::close`].
    fn create(&self, path: &FlinkPath) -> io::Result<u64>;
    /// Writes `bytes` at `offset` of a created file.
    fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()>;
    /// Makes what was written durable on the peer.
    fn sync(&self, handle: u64) -> io::Result<()>;
    /// Closes a created file; the handle is gone after this, whatever it returns.
    fn close(&self, handle: u64) -> io::Result<()>;
    /// Renames on the same peer, never replacing `to` (`AlreadyExists`).
    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()>;
    /// Renames the file `from` on the same device over the file `to` (or to a free name),
    /// never over a folder or a link: what an extract or transfer the user confirmed
    /// replacing ([`crate::ConvertRequest::replace`]) puts in place.
    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()>;
    /// Removes the file `path` (the peer allows only files a job of this device created).
    fn remove(&self, path: &FlinkPath) -> io::Result<()>;
    /// Bytes free on the volume holding the folder `dir`.
    fn free_space(&self, dir: &FlinkPath) -> io::Result<u64>;
    /// Deletes the file or folder `path` for good, as the device's own Delete would (a paired
    /// Forge: its `Jobs::delete_path` and its protected paths). Refused unless the device
    /// does it: a NAS share is read only here.
    fn delete_path(&self, path: &FlinkPath) -> Result<(), crate::DeleteError> {
        Err(crate::delete::not_here(path))
    }
    /// Moves or renames `from` to `to` on the same device, as its own Move to would
    /// (`replace`: the user confirmed taking over a `to` there). Refused unless the device
    /// does it, as [`Remote::delete_path`].
    fn move_path(
        &self,
        from: &FlinkPath,
        _to: &FlinkPath,
        _replace: bool,
    ) -> Result<(), crate::DeleteError> {
        Err(crate::delete::not_here(from))
    }
}

/// [`Remote::put_new`] in calls: `tmp` created, written, synced and closed, then renamed to
/// `to` (over a file there when `replace`); `tmp` removed on any failure.
fn put<R: Remote + ?Sized>(
    remote: &R,
    tmp: &FlinkPath,
    to: &FlinkPath,
    bytes: &[u8],
    replace: bool,
) -> io::Result<()> {
    let handle = remote.create(tmp)?;
    let done = (|| {
        if !bytes.is_empty() {
            remote.write_at(handle, 0, bytes)?;
        }
        remote.sync(handle)?;
        remote.close(handle)?;
        if replace {
            remote.rename_replace(tmp, to)
        } else {
            remote.rename_no_replace(tmp, to)
        }
    })();
    if done.is_err() {
        let _ = remote.close(handle);
        let _ = remote.remove(tmp);
    }
    done
}

fn registry() -> &'static RwLock<Option<Arc<dyn Remote>>> {
    static R: RwLock<Option<Arc<dyn Remote>>> = RwLock::new(None);
    &R
}

/// Registers the Connect service's peers (a later call replaces the earlier one).
pub fn set(remote: Arc<dyn Remote>) {
    *registry().write().unwrap_or_else(|e| e.into_inner()) = Some(remote);
}

/// The registered [`Remote`]; an error before [`set`] (no Connect service runs).
pub fn get() -> io::Result<Arc<dyn Remote>> {
    registry()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "no paired device service"))
}

/// The [`Remote`] that serves `at`: Connect's for a paired Forge, a client for a server.
pub fn get_for(at: &FlinkPath) -> io::Result<Arc<dyn Remote>> {
    match at.scheme {
        Scheme::Flink => get(),
        Scheme::Ftp => Ok(crate::ftp::remote()),
        Scheme::Smb => Ok(crate::smb::remote()),
        Scheme::Nfs => Ok(crate::nfs::remote()),
    }
}

/// Whether `path` is an `ftp://` path (a saved FTP server's).
pub fn is_ftp(path: &Path) -> bool {
    has_scheme(path, Scheme::Ftp.prefix())
}

/// Whether `path` is on a device this one reads and writes through a [`Remote`]: a paired
/// Forge (`flink://`) or a server (`ftp://`, `smb://`, `nfs://`).
pub fn is_remote(path: &Path) -> bool {
    Scheme::of(path).is_some()
}

/// Whether `path` is on a NAS share (`smb://`, `nfs://`): a game source and an output, but
/// nothing on it is edited, deleted or moved, and no file is added to an edit from it.
pub fn is_nas(path: &Path) -> bool {
    matches!(Scheme::of(path), Some(Scheme::Smb | Scheme::Nfs))
}

fn has_scheme(path: &Path, scheme: &str) -> bool {
    // Bytes, not a str slice: a path with a multi-byte character near its start is no panic.
    path.to_str().is_some_and(|s| {
        s.len() > scheme.len()
            && s.as_bytes()[..scheme.len()].eq_ignore_ascii_case(scheme.as_bytes())
    })
}

/// Which kind of device a [`FlinkPath`] is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Scheme {
    /// A paired Forge, by key id.
    #[default]
    Flink,
    /// An FTP server, by `host[:port]`.
    Ftp,
    /// An SMB server, by `host[:port]`; a path's first part is the share.
    Smb,
    /// An NFS server, by `host[:port]`; a path is the server's absolute path.
    Nfs,
}

impl Scheme {
    const ALL: [Scheme; 4] = [Scheme::Flink, Scheme::Ftp, Scheme::Smb, Scheme::Nfs];

    fn prefix(self) -> &'static str {
        match self {
            Scheme::Flink => "flink://",
            Scheme::Ftp => "ftp://",
            Scheme::Smb => "smb://",
            Scheme::Nfs => "nfs://",
        }
    }

    /// A server's port when its address names none.
    fn default_port(self) -> u16 {
        match self {
            Scheme::Flink => 0,
            Scheme::Ftp => ps5_dump_forge_ftp::DEFAULT_PORT,
            Scheme::Smb => ps5_dump_forge_smb::DEFAULT_PORT,
            Scheme::Nfs => ps5_dump_forge_nfs::DEFAULT_PORT,
        }
    }

    /// The scheme `path` is spelled with, when it is a [`Remote`] one.
    pub fn of(path: &Path) -> Option<Scheme> {
        Scheme::ALL
            .into_iter()
            .find(|s| has_scheme(path, s.prefix()))
    }
}

/// Whether `path` is a `flink://` path.
pub fn is_flink(path: &Path) -> bool {
    has_scheme(path, Scheme::Flink.prefix())
}

/// A server's `host[:port]` as its paths spell it: lower case, IPv6 in brackets, the port only
/// when it isn't `default`; `None` for anything else (a user name, a path, an empty host, a bad
/// port).
pub fn authority(s: &str, default: u16) -> Option<String> {
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        match rest {
            "" => (host, None),
            _ => (host, Some(rest.strip_prefix(':')?)),
        }
    } else {
        match s.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h, Some(p)),
            Some(_) => (s, None), // a bare IPv6 address
            None => (s, None),
        }
    };
    let port = match port {
        Some(p) => p.parse::<u16>().ok().filter(|&p| p != 0)?,
        None => default,
    };
    let ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':' | b'%'));
    if !ok {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    Some(match port == default {
        true => host,
        false => format!("{host}:{port}"),
    })
}

/// An [`authority`] as host (IPv6 without brackets) and port.
pub fn host_port(authority: &str, default: u16) -> (&str, u16) {
    let (host, rest) = match authority.strip_prefix('[') {
        Some(v6) => v6.split_once(']').unwrap_or((v6, "")),
        None => match authority.rsplit_once(':') {
            Some((h, _)) => (h, &authority[h.len()..]),
            None => (authority, ""),
        },
    };
    let port = rest.strip_prefix(':').and_then(|p| p.parse().ok());
    (host, port.unwrap_or(default))
}

/// A parsed `flink://` path: `key_id` the peer's (32 hex digits, lower case), `path`
/// `/`-separated and empty at its share root. On a server (`ftp://`, `smb://`, `nfs://`
/// `host[:port]/path`) `key_id` is the server's [`authority`] and `path` is from the server's
/// root (for SMB its first part is the share).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlinkPath {
    pub key_id: String,
    pub path: String,
    pub scheme: Scheme,
}

impl FlinkPath {
    /// Whether this path is `other` or inside it, on the same device: compared as spelled (the
    /// device resolves its paths), ASCII case-insensitively on an SMB share (an NFS server's
    /// paths, as a POSIX filesystem's, case-sensitively).
    // ponytail: a FLINK peer or FTP server that folds case is compared case-sensitively.
    pub(crate) fn within(&self, other: &FlinkPath) -> bool {
        let fold = |p: &str| match self.scheme {
            Scheme::Smb => p.to_ascii_lowercase(),
            _ => p.to_string(),
        };
        self.scheme == other.scheme
            && self.key_id == other.key_id
            && Path::new(&fold(&self.path)).starts_with(fold(&other.path))
    }

    pub fn parse(path: &Path) -> anyhow::Result<Self> {
        let scheme = Scheme::of(path);
        let bad = || {
            anyhow::anyhow!(
                "{}: not a {}device/path address",
                path.display(),
                scheme.unwrap_or_default().prefix()
            )
        };
        let scheme = scheme.ok_or_else(bad)?;
        let s = path.to_str().ok_or_else(bad)?;
        // Windows may have turned the slashes around.
        let rest = s[scheme.prefix().len()..].replace('\\', "/");
        let (key_id, rest) = rest.split_once('/').unwrap_or((rest.as_str(), ""));
        let key_id = match scheme {
            Scheme::Flink => (key_id.len() == 32 && key_id.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| key_id.to_ascii_lowercase()),
            _ => authority(key_id, scheme.default_port()),
        }
        .ok_or_else(bad)?;
        // A trailing `/` names the same folder; any other empty, `.` or `..` part is refused.
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        if !rest.is_empty() && rest.split('/').any(|p| matches!(p, "" | "." | "..")) {
            return Err(bad());
        }
        Ok(Self {
            key_id,
            path: rest.to_string(),
            scheme,
        })
    }

    /// The `flink://` (or server) spelling.
    pub fn url(&self) -> PathBuf {
        let mut s = format!("{}{}", self.scheme.prefix(), self.key_id);
        if !self.path.is_empty() {
            s.push('/');
            s.push_str(&self.path);
        }
        PathBuf::from(s)
    }

    /// This server's host and port (a server path's only).
    pub(crate) fn host_port(&self) -> (&str, u16) {
        host_port(&self.key_id, self.scheme.default_port())
    }

    /// The folder above; `None` at the share root.
    pub fn parent(&self) -> Option<Self> {
        if self.path.is_empty() {
            return None;
        }
        let mut up = self.clone();
        up.path = match self.path.rsplit_once('/') {
            Some((head, _)) => head.to_string(),
            None => String::new(),
        };
        Some(up)
    }

    /// `name` (one plain name) inside this folder.
    pub fn join(&self, name: &str) -> Self {
        let mut down = self.clone();
        down.path = if self.path.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.path)
        };
        down
    }

    pub(crate) fn shown(&self) -> String {
        self.url().display().to_string()
    }
}

/// (length, modification time) of a file on a peer, for [`crate::convert::SourceStamp`].
pub fn seen(path: &Path) -> anyhow::Result<(u64, Option<SystemTime>)> {
    let at = FlinkPath::parse(path)?;
    let s = get_for(&at)?.stat(&at).with_context(|| at.shown())?;
    // A link's own size and mtime would stamp it as a file.
    if s.link {
        bail!("{}: a link (not followed)", at.shown());
    }
    Ok((s.size, s.mtime))
}

/// One folder of a peer, as [`crate::list_dir`] lists a local one.
pub fn list_dir(path: &Path) -> anyhow::Result<crate::Listing> {
    let at = FlinkPath::parse(path)?;
    listing(&*get_for(&at)?, &at)
}

fn listing(remote: &dyn Remote, at: &FlinkPath) -> anyhow::Result<crate::Listing> {
    use crate::listing::{LIST_MAX, ListEntry};
    let mut entries: Vec<ListEntry> = remote
        .list(at)
        .with_context(|| at.shown())?
        .into_iter()
        // A name `parse` can't give back (a `\` on a Unix peer) would open another path; one
        // that isn't UTF-8 is left out, as a local listing leaves it out.
        .filter(|e| addressable(&e.name) && !e.bad_name)
        .map(|e| ListEntry {
            path: at.join(&e.name).url(),
            size: (!e.dir).then_some(e.size),
            dir: e.dir,
            name: e.name,
            link: e.link,
            mounted: false,
        })
        .collect();
    let truncated = entries.len() > LIST_MAX;
    entries.truncate(LIST_MAX);
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
    Ok(crate::Listing {
        path: at.url(),
        parent: at.parent().map(|p| p.url()),
        entries,
        truncated,
    })
}

/// `at` as the folder above lists it (so a link shows as one); the share root, by a stat.
pub(crate) fn entry(remote: &dyn Remote, at: &FlinkPath) -> io::Result<RemoteEntry> {
    let Some(up) = at.parent() else {
        let s = remote.stat(at)?;
        return Ok(RemoteEntry {
            name: String::new(),
            dir: s.dir,
            size: s.size,
            mtime: s.mtime,
            ..Default::default()
        });
    };
    let name = at.path.rsplit('/').next().unwrap_or(&at.path);
    (remote.list(&up)?.into_iter())
        .find(|e| e.name == name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
}

/// `len` bytes of `path` from `offset` (cut at `stamp`'s length), then a stat: fails unless
/// every byte came and the file still has `stamp`'s length and mtime.
fn fetch(
    remote: &dyn Remote,
    path: &FlinkPath,
    stamp: &RemoteStat,
    offset: u64,
    len: usize,
) -> io::Result<Vec<u8>> {
    let len = len.min(usize::try_from(stamp.size.saturating_sub(offset)).unwrap_or(usize::MAX));
    let shown = || path.shown();
    // Nothing to read (an empty file, or at its end) still checks the stamp.
    let at = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", shown()));
    let (buf, now) = if len == 0 {
        (Vec::new(), remote.stat(path).map_err(at)?)
    } else {
        remote.read_checked(path, offset, len).map_err(at)?
    };
    if now != *stamp {
        return Err(io::Error::other(format!(
            "{} changed while it was being read",
            shown()
        )));
    }
    if buf.len() != len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("{}: short read", shown()),
        ));
    }
    Ok(buf)
}

/// At most this many entries and this deep, as a folder scan would go.
const MAX_ENTRIES: usize = 1_000_000;
const MAX_DEPTH: usize = 64;

/// A game folder on a peer. Sizes and mtimes are listed up front and every read fails when its
/// file's size or mtime moved since. Junk ([`is_junk`]) is skipped, as the folder scanner skips it;
/// a link or a name a `flink://` path can't spell refuses the folder, as a local scan refuses links.
/// A read that continues the last one fetches 64 MiB ahead (checked when fetched, as
/// [`RemoteReader`]'s window is).
pub struct RemoteTree {
    remote: Arc<dyn Remote>,
    root: FlinkPath,
    files: Vec<SourceFile>,
    seen: HashMap<String, RemoteStat>,
    empty_dirs: Vec<String>,
    /// Where the last read ended (path, offset): a read starting there is sequential.
    next: Option<(String, u64)>,
    /// Bytes read ahead of a sequential read (path, offset, bytes).
    ahead: Option<(String, u64, Vec<u8>)>,
}

impl RemoteTree {
    pub fn open(
        remote: Arc<dyn Remote>,
        root: &FlinkPath,
        cancel: &AtomicBool,
    ) -> anyhow::Result<Self> {
        if !remote.stat(root).with_context(|| root.shown())?.dir {
            bail!("{}: not a folder", root.shown());
        }
        let mut tree = Self {
            remote,
            root: root.clone(),
            files: Vec::new(),
            seen: HashMap::new(),
            empty_dirs: Vec::new(),
            next: None,
            ahead: None,
        };
        let mut todo = vec![(String::new(), 0usize)];
        let mut count = 0usize;
        while let Some((rel, depth)) = todo.pop() {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let at = tree.at(&rel);
            let entries = tree.remote.list(&at).with_context(|| at.shown())?;
            count += entries.len();
            // Junk is skipped unread, as the folder scanner skips it: a link in `.Trashes`
            // refuses nothing, and a folder holding only junk is empty.
            let entries: Vec<RemoteEntry> =
                entries.into_iter().filter(|e| !is_junk(&e.name)).collect();
            if entries.is_empty() && !rel.is_empty() {
                tree.empty_dirs.push(rel.clone());
            }
            if count > MAX_ENTRIES || depth >= MAX_DEPTH {
                bail!("{}: too many files or folders", root.shown());
            }
            for e in entries {
                if e.bad_name {
                    bail!("{}: name is not valid UTF-8: {:?}", at.shown(), e.name);
                }
                if e.special {
                    bail!("{}: special file", at.join(&e.name).shown());
                }
                if !addressable(&e.name) {
                    bail!(
                        "{}: a name a flink:// path can't spell: {:?}",
                        at.shown(),
                        e.name
                    );
                }
                if e.link {
                    bail!("{}: symlink (not followed)", at.join(&e.name).shown());
                }
                let path = if rel.is_empty() {
                    e.name
                } else {
                    format!("{rel}/{}", e.name)
                };
                if e.dir {
                    todo.push((path, depth + 1));
                } else {
                    let stamp = RemoteStat {
                        dir: false,
                        size: e.size,
                        mtime: e.mtime,
                        link: false,
                    };
                    tree.seen.insert(path.clone(), stamp);
                    tree.files.push(SourceFile { path, size: e.size });
                }
            }
        }
        tree.files.sort_by(|a, b| a.path.cmp(&b.path));
        tree.empty_dirs.sort();
        Ok(tree)
    }

    /// The size and mtime `path` was listed with.
    pub(crate) fn stamp(&self, path: &str) -> Option<RemoteStat> {
        self.seen.get(path).copied()
    }

    /// A listed file's modification time as listed.
    pub(crate) fn mtime(&self, path: &str) -> Option<SystemTime> {
        self.seen.get(path).and_then(|s| s.mtime)
    }

    fn at(&self, rel: &str) -> FlinkPath {
        if rel.is_empty() {
            self.root.clone()
        } else {
            rel.split('/').fold(self.root.clone(), |at, p| at.join(p))
        }
    }
}

impl SourceTree for RemoteTree {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let size = self.seen.get(path).map_or(0, |s| s.size);
        let len = usize::try_from(size).map_err(|_| io::Error::other("file too big"))?;
        self.read_range(path, 0, len)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let stamp = *self.seen.get(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{path}: not in the folder"),
            )
        })?;
        // Served from what was read ahead when it holds the whole range.
        if let Some((p, at, bytes)) = &self.ahead
            && p == path
            && offset >= *at
            && offset - *at + len as u64 <= bytes.len() as u64
        {
            let from = (offset - *at) as usize;
            let out = bytes[from..from + len].to_vec();
            self.next = Some((path.to_string(), offset + len as u64));
            return Ok(out);
        }
        self.ahead = None;
        // A sequential read fetches a whole window, so the peer serves it on several lanes.
        let sequential = self
            .next
            .as_ref()
            .is_some_and(|(p, o)| p == path && *o == offset);
        let want = if sequential { len.max(WINDOW_MAX) } else { len };
        let bytes = fetch(&*self.remote, &self.at(path), &stamp, offset, want)?;
        self.next = Some((path.to_string(), offset + len.min(bytes.len()) as u64));
        if bytes.len() <= len {
            return Ok(bytes);
        }
        let out = bytes[..len].to_vec();
        self.ahead = Some((path.to_string(), offset, bytes));
        Ok(out)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!("folder {}", self.root.shown())
    }
}

/// An image file on a peer as `Read + Seek`, read ahead in a window that starts at 1 MiB (the
/// image readers ask for headers a few hundred bytes at a time) and doubles up to 64 MiB while
/// the reads stay sequential. Its length and mtime are taken at open; every read fails once
/// either moved.
// ponytail: one request at a time; the Connect side deals a request's 8 MiB pieces over its
// data lanes, so a full window keeps 8 in flight, but none while the window is being used.
pub struct RemoteReader {
    remote: Arc<dyn Remote>,
    path: FlinkPath,
    stamp: RemoteStat,
    pos: u64,
    window: Vec<u8>,
    window_at: u64,
    /// The last window's length, and where a read that continues the last one starts.
    want: usize,
    next: u64,
    /// The largest window read ahead (`WINDOW_MAX` unless [`Self::window_max`] lowers it).
    max: usize,
}

const WINDOW_MIN: usize = 1 << 20;
/// 8 pieces of the Connect side's 8 MiB, one per data lane.
const WINDOW_MAX: usize = 64 << 20;

impl RemoteReader {
    pub fn open(remote: Arc<dyn Remote>, path: &FlinkPath) -> anyhow::Result<Self> {
        let stamp = remote.stat(path).with_context(|| path.shown())?;
        if stamp.dir {
            bail!("{}: a folder, not a file", path.shown());
        }
        // A link's stat is the link's own (size 0): read, it would pass for an empty file.
        if stamp.link {
            bail!("{}: a link (not followed)", path.shown());
        }
        Ok(Self {
            remote,
            path: path.clone(),
            stamp,
            pos: 0,
            window: Vec::new(),
            window_at: 0,
            want: WINDOW_MIN,
            next: 0,
            max: WINDOW_MAX,
        })
    }

    /// Reads ahead at most `max` bytes (at least `WINDOW_MIN`), for many readers at once.
    pub(crate) fn window_max(mut self, max: usize) -> Self {
        self.max = max.clamp(WINDOW_MIN, WINDOW_MAX);
        self
    }

    pub fn len(&self) -> u64 {
        self.stamp.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The file's stat now, when it still has the length and mtime it was opened with.
    pub fn stamp_now(&self) -> io::Result<Option<RemoteStat>> {
        let now = self.remote.stat(&self.path)?;
        Ok((now == self.stamp).then_some(now))
    }

    fn fetch(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let buf = fetch(&*self.remote, &self.path, &self.stamp, self.pos, len)?;
        self.next = self.pos + buf.len() as u64;
        Ok(buf)
    }
}

impl Read for RemoteReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.stamp.size {
            return Ok(0);
        }
        let end = self.window_at + self.window.len() as u64;
        if self.pos < self.window_at || self.pos >= end {
            self.want = if self.pos == self.next && self.next > 0 {
                (self.want * 2).min(self.max)
            } else {
                WINDOW_MIN
            };
            // A read at least a window long skips the window.
            if buf.len() >= self.want {
                let data = self.fetch(buf.len().min(self.max))?;
                buf[..data.len()].copy_from_slice(&data);
                self.pos += data.len() as u64;
                return Ok(data.len());
            }
            self.window = self.fetch(self.want)?;
            self.window_at = self.pos;
        }
        let from = usize::try_from(self.pos - self.window_at).unwrap_or(usize::MAX);
        let n = buf.len().min(self.window.len().saturating_sub(from));
        buf[..n].copy_from_slice(&self.window[from..from + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RemoteReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.stamp.size.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos =
            pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before 0"))?;
        Ok(self.pos)
    }
}

impl RemoteReader {
    /// A fresh reader of the same file, stamped now.
    pub(crate) fn reopen(&self) -> anyhow::Result<Self> {
        Self::open(self.remote.clone(), &self.path)
    }
}

/// The output `path` on a peer: its folder must be there. Every format can go there: an image
/// through a [`RemoteFile`], a `.pkg` through a [`RemotePkgOut`], a folder (LZ4 packs included)
/// through a [`RemoteDir`].
pub(crate) fn output_path(path: &Path) -> anyhow::Result<PathBuf> {
    let at = FlinkPath::parse(path)?;
    let Some(dir) = at.parent() else {
        bail!("{} does not name a file", path.display());
    };
    let s = get_for(&dir)?
        .stat(&dir)
        .with_context(|| format!("output folder {}", dir.shown()))?;
    if !s.dir {
        bail!("{} is not a folder", dir.shown());
    }
    Ok(at.url())
}

/// Whether something is at `path` on a peer: anything but "not found" counts, so a name is
/// never taken for free while the device can't say.
pub(crate) fn exists(path: &Path) -> bool {
    let Ok(at) = FlinkPath::parse(path) else {
        return false;
    };
    get_for(&at)
        .and_then(|r| r.stat(&at))
        .map_or_else(|e| e.kind() != io::ErrorKind::NotFound, |_| true)
}

/// Free space in the folder `dir` on a peer against `need`, as [`crate::preflight::destination`]
/// checks a local one.
// ponytail: the peer's file system (FAT32's 4 GiB limit) is not asked; its write fails then.
pub(crate) fn destination(dir: &Path, need: u64) -> crate::preflight::Destination {
    const MIB: u64 = 1 << 20;
    let free = FlinkPath::parse(dir).and_then(|at| {
        get_for(&at)?
            .free_space(&at)
            .with_context(|| format!("free space in {}", at.shown()))
    });
    let mut findings = Vec::new();
    match &free {
        Ok(free) if need > *free => findings.push(format!(
            "not enough space in {}: about {} MiB needed, {} MiB free",
            dir.display(),
            need.div_ceil(MIB),
            free / MIB
        )),
        Ok(_) => {}
        Err(e) => findings.push(format!("{e:#}")),
    }
    crate::preflight::Destination {
        findings,
        notes: Vec::new(),
        free: free.ok(),
        #[cfg(target_os = "freebsd")]
        dest: None,
    }
}

/// Files up to this size go to a peer whole, in one [`Remote::put_new`] (one Connect lane
/// chunk).
pub const PUT_MAX: usize = 8 << 20;

/// Largest piece a [`RemoteFile`] sends at once: 8 of the Connect side's 8 MiB pieces, one per
/// data lane.
const WRITE_CHUNK: usize = 64 << 20;

/// A file created on a peer, written as `Write + Seek`: sequential writes are kept until 64 MiB
/// are buffered (or a write lands elsewhere), then sent with their offset. [`finish`] sends
/// the rest, syncs and closes it; dropped unfinished, what is buffered is lost.
///
/// [`finish`]: RemoteFile::finish
pub struct RemoteFile {
    remote: Arc<dyn Remote>,
    handle: u64,
    pos: u64,
    /// The file's length as written so far, the buffer included.
    len: u64,
    buf: Vec<u8>,
    buf_at: u64,
}

impl RemoteFile {
    fn new(remote: Arc<dyn Remote>, handle: u64) -> Self {
        Self {
            remote,
            handle,
            pos: 0,
            len: 0,
            buf: Vec::new(),
            buf_at: 0,
        }
    }

    fn send(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.remote.write_at(self.handle, self.buf_at, &self.buf)?;
            // Freed, not kept: an LZ4 pack keeps every volume's file open until it ends.
            self.buf = Vec::new();
        }
        Ok(())
    }

    /// Sends what is buffered, then syncs and closes the file.
    pub fn finish(mut self) -> io::Result<()> {
        self.sync_open()?;
        self.remote.close(self.handle)
    }

    /// Sends what is buffered and syncs, leaving the file open.
    fn sync_open(&mut self) -> io::Result<()> {
        self.send()?;
        self.remote.sync(self.handle)
    }
}

impl Write for RemoteFile {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.is_empty() {
            self.buf_at = self.pos;
        } else if self.buf_at + self.buf.len() as u64 != self.pos {
            self.send()?;
            self.buf_at = self.pos;
        }
        let n = data.len().min(WRITE_CHUNK - self.buf.len());
        self.buf.extend_from_slice(&data[..n]);
        self.pos += n as u64;
        self.len = self.len.max(self.pos);
        if self.buf.len() == WRITE_CHUNK {
            self.send()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send()
    }
}

impl Seek for RemoteFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos =
            pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before 0"))?;
        Ok(self.pos)
    }
}

/// A `.part` this job created on a peer, as [`crate::finalize::Part`] is a local one: dropped
/// without [`publish`](RemotePart::publish), it is removed (failure, cancel or panic alike).
// ponytail: no identity check; the peer only renames or removes a file this session created.
pub(crate) struct RemotePart {
    remote: Arc<dyn Remote>,
    handle: u64,
    at: FlinkPath,
    url: PathBuf,
    published: bool,
}

impl RemotePart {
    /// Creates `path` (exclusively) on its peer.
    pub(crate) fn create_file(path: &Path) -> anyhow::Result<(Self, RemoteFile)> {
        let at = FlinkPath::parse(path)?;
        Self::create(get_for(&at)?, at)
    }

    fn create(remote: Arc<dyn Remote>, at: FlinkPath) -> anyhow::Result<(Self, RemoteFile)> {
        let handle = remote
            .create(&at)
            .with_context(|| format!("creating {}", at.shown()))?;
        let file = RemoteFile::new(remote.clone(), handle);
        let part = Self {
            remote,
            handle,
            url: at.url(),
            at,
            published: false,
        };
        Ok((part, file))
    }

    /// The part, still open, written again from its start (an LZ4 volume's header); its
    /// length is not known to the writer, so it is never sought from the end.
    fn rewrite(&self) -> RemoteFile {
        RemoteFile::new(self.remote.clone(), self.handle)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.url
    }

    /// The part as written, read back from the peer.
    pub(crate) fn reader(&self) -> anyhow::Result<RemoteReader> {
        RemoteReader::open(self.remote.clone(), &self.at)
    }

    /// Renames the (verified, synced and closed) part to `output` on the peer, never over
    /// anything there.
    pub(crate) fn publish(self, output: &Path) -> anyhow::Result<()> {
        self.publish_as(output, false)
    }

    /// [`RemotePart::publish`], over a file at `output` when `replace`.
    fn publish_as(mut self, output: &Path, replace: bool) -> anyhow::Result<()> {
        let to = FlinkPath::parse(output)?;
        let renamed = if replace {
            self.remote.rename_replace(&self.at, &to)
        } else {
            self.remote.rename_no_replace(&self.at, &to)
        };
        renamed.with_context(|| format!("renaming {} to {}", self.at.shown(), to.shown()))?;
        self.published = true;
        Ok(())
    }
}

impl Drop for RemotePart {
    fn drop(&mut self) {
        if !self.published {
            // Closed first (a second close of a finished part only errs): an open upload would
            // stay in the client's table and could keep the file from being removed.
            let _ = self.remote.close(self.handle);
            let _ = self.remote.remove(&self.at);
        }
    }
}

/// A `.pkg` part on a peer as the package builder writes it ([`ps5upload_fpkg::PkgOut`]): the
/// clones (the in-place Kraken spool, the self-check's reader, the job's verify) share one write
/// buffer, sequential writes held up to 64 MiB and sent at their offset, each clone at a position
/// of its own (the builder seeks before every read and write). A read sends what is held, then
/// reads the part back by path; `sync_out` sends it and syncs on the peer. Nothing is kept on
/// this device: a production build spools in place (no `PS5UPLOAD_FPKG_SPOOL_DIR`) and digests
/// each compressed block as it writes it (vendor patch 0026), so the package crosses the network
/// once written and once read by the builder's self-check, then the job's verify reads it.
// Each clone reads ahead and serves the verifier's small reads from up to `PKG_WINDOWS` windows
// (its reads interleave a few streams: metadata entries and the blocks they name) until anything
// is written: over FTP every read is a data connection of its own (a 10k-file package's verify
// took ~500 s with one per read). A window starts at 256 KiB and the one a read continues is
// replaced by one twice its size, up to 4 MiB; the least recently used goes first; a read
// larger than that is served and not kept.
// ponytail: windows per clone, not shared; one lane at a time.
pub(crate) struct RemotePkgOut {
    shared: Arc<Mutex<PkgShared>>,
    pos: u64,
    windows: Vec<Window>,
}

/// Bytes read at `at`, valid while the shared `written` count is still this one.
struct Window {
    at: u64,
    bytes: Vec<u8>,
    written: u64,
    /// The size of the window that replaces this one when a read continues it.
    next: usize,
}

const PKG_WINDOWS: usize = 4;
const PKG_AHEAD_MIN: usize = 256 << 10;
const PKG_AHEAD_MAX: usize = 4 << 20;

struct PkgShared {
    remote: Arc<dyn Remote>,
    at: FlinkPath,
    handle: u64,
    buf: Vec<u8>,
    buf_at: u64,
    /// The part's length as written so far, what is held included.
    len: u64,
    /// Writes so far (every clone's read-ahead window is stale once it moves).
    written: u64,
}

impl PkgShared {
    fn send(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.remote.write_at(self.handle, self.buf_at, &self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }
}

impl RemotePkgOut {
    /// The `.part` `path`, created (exclusively) on its peer.
    pub(crate) fn create(path: &Path) -> anyhow::Result<(RemotePart, Self)> {
        let (part, _file) = RemotePart::create_file(path)?;
        let shared = PkgShared {
            remote: part.remote.clone(),
            at: part.at.clone(),
            handle: part.handle,
            buf: Vec::new(),
            buf_at: 0,
            len: 0,
            written: 0,
        };
        let out = Self::clone_of(Arc::new(Mutex::new(shared)));
        Ok((part, out))
    }

    fn clone_of(shared: Arc<Mutex<PkgShared>>) -> Self {
        Self {
            shared,
            pos: 0,
            windows: Vec::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PkgShared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends what is held, syncs and closes the part (every clone is done with it).
    pub(crate) fn finish(self) -> io::Result<()> {
        let mut s = self.lock();
        s.send()?;
        s.remote.sync(s.handle)?;
        s.remote.close(s.handle)
    }
}

impl Write for RemotePkgOut {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let pos = self.pos;
        let mut s = self.lock();
        if s.buf_at + s.buf.len() as u64 != pos || s.buf.len() == WRITE_CHUNK {
            s.send()?;
            s.buf_at = pos;
        }
        let n = data.len().min(WRITE_CHUNK - s.buf.len());
        s.buf.extend_from_slice(&data[..n]);
        s.len = s.len.max(pos + n as u64);
        s.written += 1;
        drop(s);
        self.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.lock().send()
    }
}

impl Read for RemotePkgOut {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let shared = self.shared.clone();
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let pos = self.pos;
        self.windows.retain(|w| w.written == s.written);
        let end = |w: &Window| w.at + w.bytes.len() as u64;
        let i = match self
            .windows
            .iter()
            .position(|w| w.at <= pos && pos < end(w))
        {
            Some(i) => i,
            None => {
                s.send()?;
                let left = usize::try_from(s.len.saturating_sub(pos)).unwrap_or(usize::MAX);
                let next = match self.windows.iter().position(|w| end(w) == pos) {
                    Some(i) => (self.windows.remove(i).next * 2).min(PKG_AHEAD_MAX),
                    None => PKG_AHEAD_MIN,
                };
                let want = buf.len().max(next).min(left);
                if want == 0 {
                    return Ok(0);
                }
                let bytes = s.remote.read_at(&s.at, pos, want)?;
                if bytes.is_empty() {
                    return Ok(0);
                }
                if bytes.len() > PKG_AHEAD_MAX {
                    // A large read (the sweep's 8 MiB batches) is served, not kept: windows stay
                    // within `PKG_WINDOWS` x 4 MiB per clone.
                    let n = buf.len().min(bytes.len());
                    buf[..n].copy_from_slice(&bytes[..n]);
                    self.pos += n as u64;
                    return Ok(n);
                }
                if self.windows.len() == PKG_WINDOWS {
                    self.windows.remove(0);
                }
                self.windows.push(Window {
                    at: pos,
                    bytes,
                    written: s.written,
                    next,
                });
                self.windows.len() - 1
            }
        };
        drop(s);
        // Most recently used last.
        let w = self.windows.remove(i);
        let from = (pos - w.at) as usize;
        let n = buf.len().min(w.bytes.len() - from);
        buf[..n].copy_from_slice(&w.bytes[from..from + n]);
        self.windows.push(w);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RemotePkgOut {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let len = self.lock().len;
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos =
            pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before 0"))?;
        Ok(self.pos)
    }
}

impl ps5upload_fpkg::PkgOut for RemotePkgOut {
    fn try_clone_out(&self) -> io::Result<Box<dyn ps5upload_fpkg::PkgOut>> {
        Ok(Box::new(Self::clone_of(self.shared.clone())))
    }

    fn out_len(&self) -> io::Result<u64> {
        Ok(self.lock().len)
    }

    fn sync_out(&self) -> io::Result<()> {
        let mut s = self.lock();
        s.send()?;
        s.remote.sync(s.handle)
    }
}

/// `dir/name` on its peer as [`crate::temp_file::write_temp_new`] writes a local file: an
/// exclusive temporary name beside it that `fill` writes, sent, synced and closed, then renamed
/// without replacing (over a file there when `replace`; the peer syncs the folder). On any
/// failure the temporary file is removed. What `fill` writes is held up to [`PUT_MAX`] bytes:
/// a new file that small goes in one [`Remote::put_new`] (a replacing one in [`put`]'s calls);
/// a larger one is created at that point and streamed.
fn write_temp_new(
    remote: &Arc<dyn Remote>,
    dir: &FlinkPath,
    name: &str,
    replace: bool,
    fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
) -> anyhow::Result<()> {
    let target = dir.join(name);
    let tmp = dir.join(&crate::temp_file::temp_name(name));
    let mut out = Spill {
        remote,
        tmp: &tmp,
        held: Vec::new(),
        open: None,
    };
    fill(&mut out).with_context(|| format!("writing {}", target.shown()))?;
    match out.open {
        Some((part, file)) => {
            file.finish()
                .with_context(|| format!("writing {}", target.shown()))?;
            part.publish_as(&target.url(), replace)
        }
        None if replace => put(remote.as_ref(), &tmp, &target, &out.held, true)
            .with_context(|| format!("writing {}", target.shown())),
        None => remote
            .put_new(&tmp, &target, &out.held)
            .with_context(|| format!("writing {}", target.shown())),
    }
}

/// [`write_temp_new`]'s writer: bytes held until they pass [`PUT_MAX`], then the temporary file
/// created on the peer and everything streamed into it.
struct Spill<'a> {
    remote: &'a Arc<dyn Remote>,
    tmp: &'a FlinkPath,
    held: Vec<u8>,
    open: Option<(RemotePart, RemoteFile)>,
}

impl Write for Spill<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.open.is_none() && self.held.len() + data.len() <= PUT_MAX {
            self.held.extend_from_slice(data);
            return Ok(data.len());
        }
        if self.open.is_none() {
            let (part, mut file) = RemotePart::create(self.remote.clone(), self.tmp.clone())
                .map_err(|e| io::Error::other(format!("{e:#}")))?;
            file.write_all(&std::mem::take(&mut self.held))?;
            self.open = Some((part, file));
        }
        self.open.as_mut().map_or(Ok(0), |(_, f)| f.write(data))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.open.as_mut().map_or(Ok(()), |(_, f)| f.flush())
    }
}

/// A folder a job writes on a peer, as [`crate::extract::Dest`] writes a local `.part`:
/// folders are made as needed (one already there that this job didn't make or [`keep`] is an
/// error: two names folded together, or something planted), each file through a temporary
/// name.
///
/// [`keep`]: RemoteDir::keep
// ponytail: no rollback: a failure leaves the folders and the files finished before it (the
// peer removes only this session's unrenamed files).
pub(crate) struct RemoteDir {
    remote: Arc<dyn Remote>,
    root: FlinkPath,
    url: PathBuf,
    created: HashSet<String>,
}

impl RemoteDir {
    /// The existing folder `path` on its peer.
    pub(crate) fn open(path: &Path) -> anyhow::Result<Self> {
        let root = FlinkPath::parse(path)?;
        let remote = get_for(&root)?;
        if !remote.stat(&root).with_context(|| root.shown())?.dir {
            bail!("{} is not a folder", root.shown());
        }
        Ok(Self::at(remote, root))
    }

    /// A new folder at `path` on its peer, made exclusively (anything there fails).
    pub(crate) fn create(path: &Path) -> anyhow::Result<Self> {
        let root = FlinkPath::parse(path)?;
        let remote = get_for(&root)?;
        remote
            .mkdir(&root)
            .with_context(|| format!("creating {}", root.shown()))?;
        Ok(Self::at(remote, root))
    }

    fn at(remote: Arc<dyn Remote>, root: FlinkPath) -> Self {
        Self {
            remote,
            url: root.url(),
            root,
            created: HashSet::new(),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.url
    }

    /// The folder `rel` (`""` is the root), made with its parents as needed.
    pub(crate) fn dir(&mut self, rel: &str) -> anyhow::Result<FlinkPath> {
        let mut cur = self.root.clone();
        let mut prefix = String::new();
        for name in rel.split('/').filter(|n| !n.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(name);
            cur = cur.join(name);
            if self.created.contains(&prefix) {
                continue;
            }
            match self.remote.mkdir(&cur) {
                Ok(()) => {
                    self.created.insert(prefix.clone());
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    bail!("{}: another entry already took this name", cur.shown())
                }
                Err(e) => return Err(e).with_context(|| format!("creating {}", cur.shown())),
            }
        }
        Ok(cur)
    }

    /// The folder `rel`, already there, taken as one [`RemoteDir::dir`] made: a folder a job
    /// merges into ([`crate::ConvertRequest::replace`]).
    pub(crate) fn keep(&mut self, rel: &str) {
        self.created.insert(rel.trim_matches('/').to_string());
    }

    /// The new file `rel`, its folders made as needed, written by `fill`.
    pub(crate) fn write_new(
        &mut self,
        rel: &str,
        fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        self.write(rel, false, fill)
    }

    /// [`RemoteDir::write_new`], over the file at `rel` when `replace`.
    pub(crate) fn write(
        &mut self,
        rel: &str,
        replace: bool,
        fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
        let dir = self.dir(parent)?;
        write_temp_new(&self.remote, &dir, name, replace, fill)
    }

    /// The folder as written, read back from the peer.
    pub(crate) fn tree(&self, cancel: &AtomicBool) -> anyhow::Result<RemoteTree> {
        RemoteTree::open(self.remote.clone(), &self.root, cancel)
    }

    /// Where the LZ4 packer puts its files: this folder's root.
    pub(crate) fn packs(&self) -> RemotePacks<'_> {
        RemotePacks {
            dir: self,
            parts: Vec::new(),
        }
    }
}

/// LZ4 pack files written into a [`RemoteDir`]'s root, each under a temporary name and kept
/// open until [`RemotePacks::publish`] (the packer rewrites every volume's header last; a peer
/// has no reopen), then closed and renamed without replacing. Dropped, they are removed.
// ponytail: a peer takes 32 unrenamed files per session, so a pack of more than about 30
// volumes (~120 GiB of packs) fails there.
pub(crate) struct RemotePacks<'a> {
    dir: &'a RemoteDir,
    parts: Vec<(String, RemotePart)>,
}

impl RemotePacks<'_> {
    pub(crate) fn publish(self) -> anyhow::Result<()> {
        for (name, part) in self.parts {
            part.remote
                .close(part.handle)
                .with_context(|| format!("closing {}", part.at.shown()))?;
            part.publish(&self.dir.root.join(&name).url())?;
        }
        Ok(())
    }
}

impl ps5_dump_forge_lz4::writer::PackOutput for RemotePacks<'_> {
    type W = RemoteFile;

    fn create(&mut self, name: &str) -> ps5upload_fpkg::Result<RemoteFile> {
        let tmp = self.dir.root.join(&crate::temp_file::temp_name(name));
        let (part, file) = RemotePart::create(self.dir.remote.clone(), tmp)
            .map_err(|e| io::Error::other(format!("{e:#}")))?;
        self.parts.push((name.to_string(), part));
        Ok(file)
    }

    fn reopen(&mut self, name: &str) -> ps5upload_fpkg::Result<RemoteFile> {
        let part = self.parts.iter().find(|(n, _)| n == name);
        let part = part.ok_or_else(|| io::Error::other(format!("{name} was not packed here")))?;
        Ok(part.1.rewrite())
    }

    fn finish(&mut self, _: &str, mut w: RemoteFile) -> ps5upload_fpkg::Result<()> {
        Ok(w.sync_open()?)
    }
}

/// The zstd packer writes through the same files as the LZ4 one.
impl ps5_dump_forge_zstd::writer::PackOutput for RemotePacks<'_> {
    type W = RemoteFile;

    fn create(&mut self, name: &str) -> ps5upload_fpkg::Result<RemoteFile> {
        ps5_dump_forge_lz4::writer::PackOutput::create(self, name)
    }

    fn reopen(&mut self, name: &str) -> ps5upload_fpkg::Result<RemoteFile> {
        ps5_dump_forge_lz4::writer::PackOutput::reopen(self, name)
    }

    fn finish(&mut self, name: &str, w: RemoteFile) -> ps5upload_fpkg::Result<()> {
        ps5_dump_forge_lz4::writer::PackOutput::finish(self, name, w)
    }
}

/// A file on a peer, to read back what a job wrote there.
pub(crate) fn reader(path: &Path) -> anyhow::Result<RemoteReader> {
    let at = FlinkPath::parse(path)?;
    RemoteReader::open(get_for(&at)?, &at)
}

/// The source at a `flink://` path: a folder, or an `.exfat`/`.ffpkg`/`.ffpfs`/`.ffpfsc` image
/// (junk filtering is the caller's); a server's (`ftp://`, `smb://`, `nfs://`) alike.
pub(crate) fn open(
    path: &Path,
    kind: crate::convert::Kind,
    cancel: &AtomicBool,
) -> anyhow::Result<Box<dyn SourceTree>> {
    use crate::convert::Kind;
    use ps5upload_fpkg::exfat::{ExFat, ExFatSource};
    let at = FlinkPath::parse(path)?;
    let remote = get_for(&at)?;
    let reader = || RemoteReader::open(remote.clone(), &at);
    let shown = at.shown();
    let with_path = |e: ps5upload_fpkg::Error| anyhow::anyhow!("{shown}: {e}");
    Ok(match kind {
        Kind::Folder => Box::new(RemoteTree::open(remote.clone(), &at, cancel)?),
        Kind::Exfat => {
            let r = reader()?;
            let len = r.len();
            let file = ps5upload_fpkg::PkgFile::from_reader(Box::new(r), len);
            let volume = ExFat::from_file(file, &shown).map_err(with_path)?;
            Box::new(ExFatSource::from_volume(volume, format!("exfat {shown}")).map_err(with_path)?)
        }
        Kind::Ffpkg => Box::new(
            ps5upload_fpkg::ufs2_source::Ufs2Source::from_reader(
                Box::new(reader()?),
                shown.clone(),
            )
            .map_err(with_path)?,
        ),
        Kind::Ffpfs => Box::new(
            ps5upload_fpkg::pfs_source::PfsSource::from_reader(Box::new(reader()?), shown.clone())
                .map_err(with_path)?,
        ),
        Kind::Ffpfsc => {
            ps5_dump_forge_pfs::open_ffpfsc(Box::new(reader()?), &shown)
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .0
        }
        // ponytail: as on a NAS, a `.pkg` or an archive isn't read (their readers take a local
        // path); copy it over first.
        Kind::Pkg | Kind::Archive(_) => bail!(
            "{shown}: a .pkg or archive on another device can't be read yet: copy it to this \
             device first"
        ),
    })
}

/// What [`crate::convert::Kind::of`] needs: whether the path is a folder (a share root is).
pub(crate) fn is_dir(path: &Path) -> anyhow::Result<bool> {
    let at = FlinkPath::parse(path)?;
    Ok(at.path.is_empty() || get_for(&at)?.stat(&at).with_context(|| at.shown())?.dir)
}

/// The file `rel` under the folder `path` on a peer when it is a file of at most `max` bytes,
/// else `None` (missing, a folder, a link (never followed: an FTP `RETR` would read its
/// target), or bigger).
pub(crate) fn read_small(path: &Path, rel: &str, max: u64) -> anyhow::Result<Option<Vec<u8>>> {
    let at = rel
        .split('/')
        .fold(FlinkPath::parse(path)?, |at, part| at.join(part));
    read_small_at(&*get_for(&at)?, &at, max)
}

fn read_small_at(remote: &dyn Remote, at: &FlinkPath, max: u64) -> anyhow::Result<Option<Vec<u8>>> {
    let at = at.clone();
    let stamp = match remote.stat(&at) {
        Ok(s) if !s.dir && !s.link && s.size <= max => s,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow::Error::new(e).context(at.shown())),
    };
    let len = usize::try_from(stamp.size).unwrap_or(usize::MAX);
    Ok(Some(fetch(remote, &at, &stamp, 0, len)?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    /// A file's bytes and mtime (seconds); `None` is a folder.
    type Node = Option<(Vec<u8>, u64)>;

    /// A peer in memory: files and folders by path (`""` is the root), reads counted.
    #[derive(Default)]
    pub(crate) struct Fake {
        nodes: Mutex<HashMap<String, Node>>,
        reads: AtomicUsize,
        /// Bytes `read_at` has served.
        pub(crate) read_bytes: std::sync::atomic::AtomicU64,
        /// Paths listed as links.
        pub(crate) links: Mutex<Vec<String>>,
        /// Paths listed as special files, and as names that aren't UTF-8.
        pub(crate) specials: Mutex<Vec<String>>,
        pub(crate) bad_names: Mutex<Vec<String>>,
        /// Files created here, by handle.
        handles: Mutex<HashMap<u64, String>>,
        /// What `free_space` says (unset: 1 TiB).
        pub(crate) free: Mutex<Option<u64>>,
        /// Every path ever created here.
        pub(crate) created: Mutex<Vec<String>>,
    }

    /// Handles unique across every fake, so [`Peers`] finds a handle's peer.
    static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);

    impl Fake {
        pub(crate) fn with(files: &[(&str, Vec<u8>)], dirs: &[&str]) -> Arc<Self> {
            let fake = Self::default();
            let mut nodes = fake.nodes.lock().unwrap();
            nodes.insert(String::new(), None);
            for d in dirs {
                nodes.insert(d.to_string(), None);
            }
            for (p, data) in files {
                nodes.insert(p.to_string(), Some((data.clone(), 1)));
            }
            drop(nodes);
            Arc::new(fake)
        }

        /// A file's bytes, `None` when there is none.
        pub(crate) fn file(&self, path: &str) -> Option<Vec<u8>> {
            self.nodes
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .flatten()
                .map(|f| f.0)
        }

        /// Every path held, sorted.
        pub(crate) fn paths(&self) -> Vec<String> {
            let mut paths: Vec<String> = self.nodes.lock().unwrap().keys().cloned().collect();
            paths.sort();
            paths
        }

        pub(crate) fn touch(&self, path: &str) {
            let mut nodes = self.nodes.lock().unwrap();
            nodes.get_mut(path).unwrap().as_mut().unwrap().1 += 1;
        }

        fn stat_of(node: &Node) -> RemoteStat {
            match node {
                None => RemoteStat {
                    dir: true,
                    size: 0,
                    mtime: None,
                    link: false,
                },
                Some((data, t)) => RemoteStat {
                    dir: false,
                    size: data.len() as u64,
                    mtime: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(*t)),
                    link: false,
                },
            }
        }
    }

    fn missing() -> io::Error {
        io::Error::from(io::ErrorKind::NotFound)
    }

    impl Remote for Fake {
        fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
            let nodes = self.nodes.lock().unwrap();
            let s = nodes
                .get(&path.path)
                .map(Self::stat_of)
                .ok_or_else(missing)?;
            // As the peer's lstat: a link is itself, never its target.
            if self.links.lock().unwrap().contains(&path.path) {
                return Ok(RemoteStat {
                    dir: false,
                    size: 0,
                    link: true,
                    ..s
                });
            }
            Ok(s)
        }

        fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
            let nodes = self.nodes.lock().unwrap();
            let prefix = if path.path.is_empty() {
                String::new()
            } else {
                format!("{}/", path.path)
            };
            Ok(nodes
                .iter()
                .filter_map(|(p, node)| {
                    let name = p.strip_prefix(&prefix)?;
                    (!name.is_empty() && !name.contains('/')).then(|| {
                        let s = Self::stat_of(node);
                        let link = self.links.lock().unwrap().contains(p);
                        RemoteEntry {
                            name: name.to_string(),
                            dir: s.dir,
                            size: s.size,
                            mtime: s.mtime,
                            link,
                            special: self.specials.lock().unwrap().contains(p),
                            bad_name: self.bad_names.lock().unwrap().contains(p),
                        }
                    })
                })
                .collect())
        }

        fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let nodes = self.nodes.lock().unwrap();
            let (data, _) = nodes
                .get(&path.path)
                .cloned()
                .flatten()
                .ok_or_else(missing)?;
            let from = (offset as usize).min(data.len());
            let bytes = data[from..(from + len).min(data.len())].to_vec();
            (self.read_bytes).fetch_add(bytes.len() as u64, Ordering::Relaxed);
            Ok(bytes)
        }

        fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
            let mut nodes = self.nodes.lock().unwrap();
            if nodes.contains_key(&path.path) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            nodes.insert(path.path.clone(), None);
            Ok(())
        }

        fn create(&self, path: &FlinkPath) -> io::Result<u64> {
            let mut nodes = self.nodes.lock().unwrap();
            if nodes.contains_key(&path.path) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            nodes.insert(path.path.clone(), Some((Vec::new(), 1)));
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed) as u64;
            self.handles
                .lock()
                .unwrap()
                .insert(handle, path.path.clone());
            self.created.lock().unwrap().push(path.path.clone());
            Ok(handle)
        }

        fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
            let path = self.handles.lock().unwrap().get(&handle).cloned();
            let mut nodes = self.nodes.lock().unwrap();
            let Some(Some((data, _))) = path.and_then(|p| nodes.get_mut(&p)) else {
                return Err(missing());
            };
            let end = offset as usize + bytes.len();
            if data.len() < end {
                data.resize(end, 0);
            }
            data[offset as usize..end].copy_from_slice(bytes);
            Ok(())
        }

        fn sync(&self, _: u64) -> io::Result<()> {
            Ok(())
        }

        fn close(&self, _: u64) -> io::Result<()> {
            Ok(())
        }

        fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
            let mut nodes = self.nodes.lock().unwrap();
            if nodes.contains_key(&to.path) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            let node = nodes.remove(&from.path).ok_or_else(missing)?;
            nodes.insert(to.path.clone(), node);
            Ok(())
        }

        fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
            let mut nodes = self.nodes.lock().unwrap();
            // As the peer: a file only over a file, never over a folder or a link.
            let link = self.links.lock().unwrap().contains(&to.path);
            if matches!(nodes.get(&to.path), Some(None)) || link {
                return Err(io::Error::other("not a file: nothing is put in its place"));
            }
            match nodes.get(&from.path) {
                Some(Some(_)) => {}
                Some(None) => return Err(io::Error::other("not a file")),
                None => return Err(missing()),
            }
            let node = nodes.remove(&from.path).ok_or_else(missing)?;
            nodes.insert(to.path.clone(), node);
            Ok(())
        }

        fn remove(&self, path: &FlinkPath) -> io::Result<()> {
            self.nodes
                .lock()
                .unwrap()
                .remove(&path.path)
                .map(drop)
                .ok_or_else(missing)
        }

        fn free_space(&self, _: &FlinkPath) -> io::Result<u64> {
            Ok(self.free.lock().unwrap().unwrap_or(1 << 40))
        }
    }

    fn at(path: &str) -> FlinkPath {
        FlinkPath {
            key_id: KEY.to_string(),
            scheme: Scheme::Flink,
            path: path.to_string(),
        }
    }

    fn bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    #[test]
    fn within_compares_device_scheme_and_smb_case() {
        let p = |s: &str| FlinkPath::parse(Path::new(s)).unwrap();
        assert!(p("smb://nas/g/Game/out").within(&p("smb://NAS/G/game")));
        assert!(p("smb://nas/g/game").within(&p("smb://nas/g/game")));
        assert!(!p("smb://nas/g/gamex").within(&p("smb://nas/g/game")));
        assert!(!p("smb://nas/g/game/out").within(&p("ftp://nas/g/game")));
        assert!(!p("smb://nas:1445/g/game/out").within(&p("smb://nas/g/game")));
        assert!(!p("ftp://nas/g/Game/out").within(&p("ftp://nas/g/game")));
        assert!(p("nfs://nas/srv/g/game/out").within(&p("nfs://NAS/srv/g/game")));
        assert!(!p("nfs://nas/srv/g/Game/out").within(&p("nfs://nas/srv/g/game")));
        assert!(!p("nfs://nas/srv/g/game").within(&p("smb://nas/srv/g/game")));
        assert!(!p("nfs://nas:12049/srv/g").within(&p("nfs://nas/srv/g")));
    }

    /// `within` and `in_use` compare as spelled, so a path with an empty, `.` or `..` part
    /// is refused on every scheme.
    #[test]
    fn parse_refuses_dot_and_empty_parts() {
        let parse = |s: &str| FlinkPath::parse(Path::new(s));
        let upper = KEY.to_ascii_uppercase();
        let p = parse(&format!("FLINK://{upper}/data/Stub Game/")).unwrap();
        assert_eq!(p, at("data/Stub Game"));
        assert_eq!(
            p.url(),
            PathBuf::from(format!("flink://{KEY}/data/Stub Game"))
        );
        assert_eq!(parse(&format!("flink://{KEY}\\a\\b")).unwrap(), at("a/b"));
        for ok in [format!("flink://{KEY}"), format!("flink://{KEY}/")] {
            assert_eq!(parse(&ok).unwrap(), at(""), "{ok}");
        }
        for scheme in ["flink", "smb", "nfs", "ftp"] {
            let dev = match scheme {
                "flink" => KEY,
                _ => "nas",
            };
            assert!(
                parse(&format!("{scheme}://{dev}/s/a/b")).is_ok(),
                "{scheme}"
            );
            for rest in ["/s/a/../b", "/s/a/./b", "/s/a//b", "//a", "/s/..", "/."] {
                let bad = format!("{scheme}://{dev}{rest}");
                assert!(parse(&bad).is_err(), "{bad}");
            }
            assert!(
                parse(&format!("{scheme}:///a")).is_err(),
                "{scheme}: empty device"
            );
        }
        for bad in ["flink://", "flink://abc/x", "/local/x"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse(&format!("flink://{}g/x", &KEY[1..])).is_err());
    }

    /// Every test that goes through the registry ([`get`]) shares one `Peers`, each with a
    /// peer (key) of its own, so tests running at once never replace each other's.
    #[derive(Default)]
    struct Peers(Mutex<HashMap<String, Arc<Fake>>>);

    impl Peers {
        fn of(&self, at: &FlinkPath) -> io::Result<Arc<Fake>> {
            let peers = self.0.lock().unwrap();
            let gone = || io::Error::new(io::ErrorKind::NotConnected, "no such device");
            peers.get(&at.key_id).cloned().ok_or_else(gone)
        }

        fn owner(&self, handle: u64) -> io::Result<Arc<Fake>> {
            let peers = self.0.lock().unwrap();
            let held = |f: &&Arc<Fake>| f.handles.lock().unwrap().contains_key(&handle);
            peers.values().find(held).cloned().ok_or_else(missing)
        }
    }

    impl Remote for Peers {
        fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
            self.of(path)?.stat(path)
        }

        fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
            self.of(path)?.list(path)
        }

        fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
            self.of(path)?.read_at(path, offset, len)
        }

        fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
            self.of(path)?.mkdir(path)
        }

        fn create(&self, path: &FlinkPath) -> io::Result<u64> {
            self.of(path)?.create(path)
        }

        fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
            self.owner(handle)?.write_at(handle, offset, bytes)
        }

        fn sync(&self, _: u64) -> io::Result<()> {
            Ok(())
        }

        fn close(&self, _: u64) -> io::Result<()> {
            Ok(())
        }

        fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
            self.of(from)?.rename_no_replace(from, to)
        }

        fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
            self.of(from)?.rename_replace(from, to)
        }

        fn remove(&self, path: &FlinkPath) -> io::Result<()> {
            self.of(path)?.remove(path)
        }

        fn free_space(&self, dir: &FlinkPath) -> io::Result<u64> {
            self.of(dir)?.free_space(dir)
        }
    }

    /// A registered peer holding a copy of the local folder `dir`; its share root.
    pub(crate) fn peer(dir: &Path) -> (FlinkPath, Arc<Fake>) {
        static PEERS: std::sync::OnceLock<Arc<Peers>> = std::sync::OnceLock::new();
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let peers = PEERS.get_or_init(|| {
            let peers = Arc::new(Peers::default());
            set(peers.clone());
            peers
        });
        let fake = Fake::with(&[], &[]);
        let mut nodes = fake.nodes.lock().unwrap();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(&at).unwrap() {
                let path = entry.unwrap().path();
                let rel = path.strip_prefix(dir).unwrap();
                let rel = rel.to_string_lossy().replace('\\', "/");
                if path.is_dir() {
                    nodes.insert(rel, None);
                    todo.push(path);
                } else {
                    nodes.insert(rel, Some((std::fs::read(&path).unwrap(), 1)));
                }
            }
        }
        drop(nodes);
        let key_id = format!("{:032x}", NEXT.fetch_add(1, Ordering::Relaxed));
        peers.0.lock().unwrap().insert(key_id.clone(), fake.clone());
        let root = FlinkPath {
            key_id,
            path: String::new(),
            scheme: Scheme::Flink,
        };
        (root, fake)
    }

    #[test]
    fn reader_fails_once_the_file_changes() {
        let fake = Fake::with(&[("img.exfat", bytes(3 << 20))], &[]);
        let mut r = RemoteReader::open(fake.clone(), &at("img.exfat")).unwrap();
        let mut buf = [0u8; 512];
        r.read_exact(&mut buf).unwrap();
        fake.touch("img.exfat");
        // Still inside the window: no request, no error.
        r.read_exact(&mut buf).unwrap();
        r.seek(SeekFrom::Start(2 << 20)).unwrap();
        let e = r.read_exact(&mut buf).unwrap_err();
        assert!(
            e.to_string().contains("changed while it was being read"),
            "{e}"
        );
    }

    #[test]
    fn tree_lists_and_reads() {
        let fake = Fake::with(
            &[
                ("g/eboot.bin", bytes(100)),
                ("g/sce_sys/param.json", bytes(10)),
                ("g/empty.bin", Vec::new()),
                ("other.bin", bytes(1)),
            ],
            &["g", "g/sce_sys", "g/data", "g/data/none"],
        );
        let cancel = AtomicBool::new(false);
        let mut tree = RemoteTree::open(fake.clone(), &at("g"), &cancel).unwrap();
        let files: Vec<_> = tree
            .files()
            .iter()
            .map(|f| (f.path.as_str(), f.size))
            .collect();
        assert_eq!(
            files,
            [
                ("eboot.bin", 100),
                ("empty.bin", 0),
                ("sce_sys/param.json", 10)
            ]
        );
        assert_eq!(tree.empty_dirs(), ["data/none"]);
        assert_eq!(tree.read("eboot.bin").unwrap(), bytes(100));
        assert_eq!(tree.read("empty.bin").unwrap(), Vec::<u8>::new());
        assert_eq!(
            tree.read_range("eboot.bin", 90, 50).unwrap(),
            bytes(100)[90..]
        );
        assert_eq!(tree.read("sce_sys/param.json").unwrap(), bytes(10));
        assert!(tree.read("other.bin").is_err());
        assert_eq!(tree.describe(), format!("folder flink://{KEY}/g"));
        fake.touch("g/eboot.bin");
        let e = tree.read_range("eboot.bin", 0, 10).unwrap_err().to_string();
        assert!(e.contains("changed while it was being read"), "{e}");
        // An empty file is checked too.
        fake.nodes
            .lock()
            .unwrap()
            .insert("g/empty.bin".into(), Some((bytes(4), 1)));
        let e = tree.read("empty.bin").unwrap_err().to_string();
        assert!(e.contains("changed while it was being read"), "{e}");
        assert!(RemoteTree::open(fake.clone(), &at("other.bin"), &cancel).is_err());
        // Links and names a path can't spell refuse the folder.
        fake.links.lock().unwrap().push("g/sce_sys".into());
        let e = RemoteTree::open(fake.clone(), &at("g"), &cancel)
            .err()
            .unwrap();
        assert!(e.to_string().contains("symlink (not followed)"), "{e}");
        fake.links.lock().unwrap().clear();
        // So do special files and names that aren't UTF-8 (a listing leaves the latter out).
        fake.specials.lock().unwrap().push("g/sce_sys".into());
        let e = RemoteTree::open(fake.clone(), &at("g"), &cancel).err();
        assert!(e.unwrap().to_string().contains("special file"));
        fake.specials.lock().unwrap().clear();
        fake.bad_names.lock().unwrap().push("g/sce_sys".into());
        let e = RemoteTree::open(fake.clone(), &at("g"), &cancel).err();
        assert!(e.unwrap().to_string().contains("not valid UTF-8"));
        let l = listing(&*fake, &at("g")).unwrap();
        assert!(!l.entries.is_empty() && l.entries.iter().all(|e| e.name != "sce_sys"));
        fake.bad_names.lock().unwrap().clear();
        // Junk is skipped unread: a link inside it refuses nothing, a folder of junk is empty.
        for d in ["g/.Trashes", "g/.Trashes/x", "g/data/j"] {
            fake.nodes.lock().unwrap().insert(d.into(), None);
        }
        (fake.nodes.lock().unwrap()).insert("g/data/j/._a".into(), Some((bytes(3), 1)));
        fake.links.lock().unwrap().push("g/.Trashes/x".into());
        let tree = RemoteTree::open(fake.clone(), &at("g"), &cancel).unwrap();
        assert_eq!(tree.files().len(), 3);
        assert_eq!(tree.empty_dirs(), ["data/j", "data/none"]);
        fake.links.lock().unwrap().clear();
        fake.nodes.lock().unwrap().insert("g/a\\b".into(), None);
        assert!(RemoteTree::open(fake.clone(), &at("g"), &cancel).is_err());
        fake.nodes.lock().unwrap().remove("g/a\\b");
        cancel.store(true, Ordering::Relaxed);
        assert!(RemoteTree::open(fake, &at("g"), &cancel).is_err());
    }

    /// Every file under `dir`, with its length and mtime.
    fn local_files(dir: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
        let mut found = Vec::new();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(&at).unwrap() {
                let path = entry.unwrap().path();
                let meta = std::fs::symlink_metadata(&path).unwrap();
                if meta.is_dir() {
                    todo.push(path.clone());
                }
                found.push((path, meta.len(), meta.modified().ok()));
            }
        }
        found.sort();
        found
    }

    #[test]
    fn an_fpkg_on_a_peer_keeps_nothing_here_and_is_read_back_only_to_check_it() {
        use crate::{ConvertRequest, Event};
        let dir = crate::test_dir("remote-pkg-no-disk");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("sce_sys")).unwrap();
        std::fs::create_dir_all(src.join("data")).unwrap();
        std::fs::write(src.join("eboot.bin"), b"\x7fELF fake eboot").unwrap();
        std::fs::write(
            src.join("sce_sys/param.json"),
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
        )
        .unwrap();
        // Incompressible, so the package is mostly its compressed image.
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let noise: Vec<u8> = (0..6 << 20)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        std::fs::write(src.join("data/noise.bin"), &noise).unwrap();
        std::fs::create_dir_all(dir.join("dest")).unwrap();
        let (root, fake) = peer(&dir.join("dest"));
        let before = local_files(&dir);

        let out = root.join("game.pkg").url();
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": src, "format": "pkg", "output": out
        }))
        .unwrap();
        // Full, so the self-check's reads don't depend on a seed.
        req.full_verify = true;
        // The bytes read from the peer when the builder starts its self-check and when it ends.
        let marks = Arc::new(Mutex::new(Vec::new()));
        let (seen, counted) = (marks.clone(), fake.clone());
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| {
            if let Event::Log { line, .. } = e
                && (line == "fpkg: verifying" || line == "fpkg: done")
            {
                let read = counted.read_bytes.load(Ordering::Relaxed);
                seen.lock().unwrap().push((line, read));
            }
        });
        let cancel = AtomicBool::new(false);
        let report = crate::convert::run(&req, &crate::jobs::Ctx::new(1, &emit, &cancel))
            .unwrap_or_else(|e| panic!("{e:#}"));
        assert_eq!(report.output, out);

        // Nothing written here: no spool, part or scratch file beside the source or anywhere
        // in the test's folders.
        assert_eq!(local_files(&dir), before);
        let size = fake.file("game.pkg").unwrap().len() as u64;
        assert!(size > noise.len() as u64, "{size}");
        assert_eq!(fake.paths(), ["", "game.pkg"]);
        let marks = marks.lock().unwrap().clone();
        let [(v, verifying), (d, done)] = &marks[..] else {
            panic!("{marks:?}")
        };
        assert_eq!((v.as_str(), d.as_str()), ("fpkg: verifying", "fpkg: done"));
        // Compressed blocks are digested as written (vendor patch 0026): nothing is read back
        // before the self-check (it was the whole image, about the package, before).
        assert_eq!(*verifying, 0, "read back before the self-check");
        // The full self-check reads the package twice, its block sweep and its
        // playgo-chunk.crc pass (vendor `verify_file_sampled`; fast mode samples both), plus
        // a few metadata blocks.
        let build = done - verifying;
        assert!(
            build >= size && build <= 2 * size + (1 << 20),
            "the build read {build} bytes of a {size}-byte package"
        );
        // The job's own verify reads it at most once more, plus what its read-ahead windows
        // fetch past a jump (256 KiB each).
        let total = fake.read_bytes.load(Ordering::Relaxed);
        assert!(
            total <= done + size + (4 << 20),
            "{total} of {size}, done {done}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
