//! NFSv3 (RFC 1813) with MOUNT v3 and the portmapper (RFC 1833's v2 GETPORT).
//!
//! Paths are absolute server paths. A path resolves through the longest export that is a
//! component-wise prefix of it (MNT once, the root handle cached), then LOOKUPs of the rest;
//! paths above or between exports form a virtual read-only folder tree, as v4's pseudo-fs does.
//! Reads: GETATTR, LOOKUP, READ in FSINFO rtmax chunks, READDIRPLUS (READDIR
//! where the server has none), FSSTAT. Writes: CREATE
//! GUARDED (never replacing), WRITE UNSTABLE in wtmax chunks and COMMIT on sync (refused when the
//! server's write verifier changed), MKDIR, REMOVE, and a no-replace rename (LINK + REMOVE, else
//! a check and RENAME).

use crate::Entry;
use crate::rpc::{self, Client, Xdr, XdrReader, bad};
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const PMAP_PROG: u32 = 100_000;
const PMAP_VERS: u32 = 2;
const PMAP_PORT: u16 = 111;
const PMAPPROC_GETPORT: u32 = 3;
const IPPROTO_TCP: u32 = 6;

const MOUNT_PROG: u32 = 100_005;
const MOUNT_VERS: u32 = 3;
const MOUNTPROC3_MNT: u32 = 1;
const MOUNTPROC3_EXPORT: u32 = 5;
/// MNTPATHLEN and MNTNAMLEN.
const MAX_DIRPATH: usize = 1024;
const MAX_NAME: usize = 255;
/// More exports (or groups of one) than this is a bad reply.
const MAX_EXPORTS: usize = 10_000;

const NFS_PROG: u32 = 100_003;
const NFS_VERS: u32 = 3;
const NFSPROC3_GETATTR: u32 = 1;
const NFSPROC3_LOOKUP: u32 = 3;
const NFSPROC3_READ: u32 = 6;
const NFSPROC3_WRITE: u32 = 7;
const NFSPROC3_CREATE: u32 = 8;
const NFSPROC3_MKDIR: u32 = 9;
const NFSPROC3_REMOVE: u32 = 12;
const NFSPROC3_RENAME: u32 = 14;
const NFSPROC3_LINK: u32 = 15;
const NFSPROC3_READDIR: u32 = 16;
const NFSPROC3_READDIRPLUS: u32 = 17;
const NFSPROC3_FSSTAT: u32 = 18;
const NFSPROC3_FSINFO: u32 = 19;
const NFSPROC3_COMMIT: u32 = 21;
/// stable_how UNSTABLE, createmode3 GUARDED.
const UNSTABLE: u32 = 0;
const GUARDED: u32 = 1;
/// Modes of what Forge creates.
const FILE_MODE: u32 = 0o666;
const DIR_MODE: u32 = 0o777;
/// NFS3_FHSIZE.
const MAX_FH: usize = 64;
/// ftype3 values.
const NF3REG: u32 = 1;
const NF3DIR: u32 = 2;
const NF3LNK: u32 = 5;
/// A folder with more entries than this (`.` and `..` counted, so a server repeating only
/// them can't page forever), or more name bytes in all, is an error.
pub(crate) const MAX_LIST: usize = 200_000;
pub(crate) const MAX_LIST_NAMES: usize = 64 << 20;
/// READDIRPLUS sizes asked for: names + cookies, and the whole reply (READDIR's count: the latter).
const DIRCOUNT: u32 = 32 * 1024;
const MAXCOUNT: u32 = 256 * 1024;
/// READ and WRITE chunks when the server's FSINFO gives no rtmax / wtmax, and the most we move
/// in one call (well inside rpc::MAX_RECORD).
const DEFAULT_XFER: u32 = 32 * 1024;
const MAX_XFER: u32 = 4 * 1024 * 1024;

const AUTH_NONE: u32 = 0;
const AUTH_SYS: u32 = 1;

/// Handles cached per path before the cache starts over, and how long one is trusted.
const MAX_HANDLES: usize = 4096;
const HANDLE_TTL: Duration = Duration::from_secs(3);

/// An nfsstat3 (MOUNT's mountstat3 shares its numbers) as an `io::Error`.
pub(crate) fn nfs_error(code: u32) -> io::Error {
    let (kind, text) = match code {
        1 => (ErrorKind::PermissionDenied, "not owner"),
        2 => (ErrorKind::NotFound, "no such file or folder"),
        5 => (ErrorKind::Other, "I/O error on the server"),
        6 => (ErrorKind::NotFound, "no such device"),
        13 => (ErrorKind::PermissionDenied, "permission denied"),
        17 => (ErrorKind::AlreadyExists, "already exists"),
        18 => (ErrorKind::CrossesDevices, "crosses filesystems"),
        19 => (ErrorKind::NotFound, "no such device"),
        20 => (ErrorKind::NotADirectory, "not a folder"),
        21 => (ErrorKind::IsADirectory, "is a folder"),
        22 => (ErrorKind::InvalidInput, "invalid argument"),
        27 => (ErrorKind::FileTooLarge, "file too large"),
        28 => (ErrorKind::StorageFull, "no space left on the server"),
        30 => (ErrorKind::ReadOnlyFilesystem, "read-only filesystem"),
        31 => (ErrorKind::TooManyLinks, "too many links"),
        63 => (ErrorKind::InvalidFilename, "name too long"),
        66 => (ErrorKind::DirectoryNotEmpty, "folder not empty"),
        69 => (ErrorKind::StorageFull, "quota exceeded"),
        70 => (ErrorKind::StaleNetworkFileHandle, "stale file handle"),
        71 => (ErrorKind::Other, "too many levels of remote in path"),
        10001 => (ErrorKind::InvalidData, "bad file handle"),
        10002 => (ErrorKind::Other, "update synchronization mismatch"),
        10003 => (ErrorKind::Other, "listing cookie is stale"),
        10004 => (ErrorKind::Unsupported, "operation not supported"),
        10005 => (ErrorKind::InvalidData, "buffer or request too small"),
        10006 => (ErrorKind::Other, "server fault"),
        10007 => (ErrorKind::Unsupported, "type not supported by the server"),
        10008 => (
            ErrorKind::ResourceBusy,
            "the server is busy, try again later",
        ),
        _ => (ErrorKind::Other, "error"),
    };
    io::Error::new(kind, format!("NFS: {text} ({code})"))
}

/// True when `e` came from NFS3ERR_STALE (drop the cached handle, look it up again).
pub(crate) fn is_stale(e: &io::Error) -> bool {
    e.kind() == ErrorKind::StaleNetworkFileHandle
}

/// An absolute `/`-separated path's components (empty ones dropped); `.`, `..` and NUL refused.
pub(crate) fn components(path: &str) -> io::Result<Vec<&str>> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "not an absolute server path",
        ));
    }
    let comps: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    if comps.iter().any(|c| *c == "." || *c == "..") {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "a path with . or .. in it",
        ));
    }
    Ok(comps)
}

/// The canonical form of `comps` (`/a/b`, `/` for none): the handle cache's key.
pub(crate) fn key(comps: &[&str]) -> String {
    format!("/{}", comps.join("/"))
}

/// One export: its path as the server gave it (for MNT), its components, its root handle once
/// mounted.
#[derive(Debug, Clone)]
pub(crate) struct Export {
    path: String,
    comps: Vec<String>,
    root: Option<Vec<u8>>,
}

impl Export {
    fn new(path: &str) -> Export {
        Export {
            path: path.to_string(),
            comps: path
                .split('/')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect(),
            root: None,
        }
    }
}

/// Where a path lies among the exports.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Place<'a> {
    /// Inside export number `export`; `rest` is the path below its root.
    Export {
        export: usize,
        rest: Vec<&'a str>,
    },
    /// Above or between exports: a virtual read-only folder holding `children` (sorted).
    Virtual(Vec<String>),
    NotFound,
}

/// Resolves `comps` by the longest export that is a component-wise prefix of it; else a path
/// that leads to exports is a virtual folder of their next components; else nothing is there.
pub(crate) fn place<'a>(exports: &[Export], comps: &[&'a str]) -> Place<'a> {
    let prefix =
        |e: &Export| e.comps.len() <= comps.len() && e.comps.iter().zip(comps).all(|(a, b)| a == b);
    if let Some((i, e)) = exports
        .iter()
        .enumerate()
        .filter(|(_, e)| prefix(e))
        .max_by_key(|(_, e)| e.comps.len())
    {
        return Place::Export {
            export: i,
            rest: comps[e.comps.len()..].to_vec(),
        };
    }
    let mut children: Vec<String> = exports
        .iter()
        .filter(|e| e.comps.len() > comps.len() && e.comps.iter().zip(comps).all(|(a, b)| a == b))
        .map(|e| e.comps[comps.len()].clone())
        .collect();
    if children.is_empty() {
        return Place::NotFound;
    }
    children.sort();
    children.dedup();
    Place::Virtual(children)
}

/// A virtual folder's entry (no size, no time).
fn virtual_dir(name: &str) -> Entry {
    Entry {
        name: name.to_string(),
        dir: true,
        size: 0,
        mtime: None,
        link: false,
        special: false,
    }
}

/// File handles by canonical path ([`key`]), bounded: when full it starts over. A handle is
/// trusted for [`HANDLE_TTL`] only, then the path is looked up again: another client may have
/// renamed the object away and put another at its name (no STALE tells us so).
// ponytail: no LRU; at MAX_HANDLES paths the walks LOOKUP from the export root again. Within
// the TTL a name replaced by another client still reads the old object (as a kernel client's
// attribute cache does).
#[derive(Debug, Default)]
pub(crate) struct Handles {
    map: HashMap<String, (Vec<u8>, Instant)>,
}

impl Handles {
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.map
            .get(key)
            .filter(|(_, at)| at.elapsed() < HANDLE_TTL)
            .map(|(fh, _)| fh.as_slice())
    }

    pub fn put(&mut self, key: String, fh: Vec<u8>) {
        if self.map.len() >= MAX_HANDLES && !self.map.contains_key(&key) {
            self.map.clear();
        }
        self.map.insert(key, (fh, Instant::now()));
    }

    /// Drops `key` and everything below it (after a remove or rename, or on STALE).
    pub fn forget(&mut self, key: &str) {
        let dir = key.trim_end_matches('/');
        self.map
            .retain(|p, _| !(p == key || p.strip_prefix(dir).is_some_and(|r| r.starts_with('/'))));
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// `host` (a name, an IP, or `[v6]`) as an address on `port` (the first the resolver gives).
fn resolve(host: &str, port: u16) -> io::Result<SocketAddr> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, format!("{host}: no address found")))
}

/// The TCP ports of MOUNT v3 and NFS v3 on `ip`, from its portmapper.
fn getports(ip: std::net::IpAddr) -> io::Result<(u16, u16)> {
    let mut pmap = Client::connect(SocketAddr::new(ip, PMAP_PORT))?;
    let mut get = |prog: u32, vers: u32, what: &str| -> io::Result<u16> {
        let mut a = Xdr::new();
        a.u32(prog).u32(vers).u32(IPPROTO_TCP).u32(0);
        let r = pmap.call(PMAP_PROG, PMAP_VERS, PMAPPROC_GETPORT, &a.buf)?;
        match r.reader().u32()? {
            0 => Err(io::Error::new(
                ErrorKind::Unsupported,
                format!("the server's portmapper lists no {what} over TCP"),
            )),
            p => u16::try_from(p).map_err(|_| bad(format!("portmapper gave port {p}"))),
        }
    };
    let nfs = get(NFS_PROG, NFS_VERS, "NFSv3")?;
    let mount = get(MOUNT_PROG, MOUNT_VERS, "MOUNT v3")?;
    Ok((mount, nfs))
}

/// An EXPORT reply: the export paths in the server's order, duplicates and non-UTF-8 ones left
/// out (a path here is a `&str`).
fn parse_exports(r: &mut XdrReader) -> io::Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = 0;
    while r.bool()? {
        seen += 1;
        if seen > MAX_EXPORTS {
            return Err(bad(format!("more than {MAX_EXPORTS} exports")));
        }
        let dir = r.opaque(MAX_DIRPATH)?.to_vec();
        let mut groups = 0;
        while r.bool()? {
            groups += 1;
            if groups > MAX_EXPORTS {
                return Err(bad(format!(
                    "an export with more than {MAX_EXPORTS} groups"
                )));
            }
            r.opaque(MAX_NAME)?;
        }
        if let Ok(dir) = String::from_utf8(dir)
            && !out.contains(&dir)
        {
            out.push(dir);
        }
    }
    Ok(out)
}

/// A MNT reply: the export's root handle. An export that takes neither AUTH_SYS nor AUTH_NONE
/// (Kerberos only) is refused.
fn parse_mnt(r: &mut XdrReader) -> io::Result<Vec<u8>> {
    let stat = r.u32()?;
    if stat != 0 {
        return Err(nfs_error(stat));
    }
    let fh = r.opaque(MAX_FH)?.to_vec();
    let n = r.len(64)?;
    let mut flavors = Vec::with_capacity(n);
    for _ in 0..n {
        flavors.push(r.u32()?);
    }
    if !flavors.is_empty() && !flavors.iter().any(|f| *f == AUTH_SYS || *f == AUTH_NONE) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!("the export needs a login Forge doesn't speak (auth flavors {flavors:?})"),
        ));
    }
    Ok(fh)
}

/// An nfsstat3: `Ok` for NFS3_OK, else the error (the failure's body is left unread).
fn status(r: &mut XdrReader) -> io::Result<()> {
    match r.u32()? {
        0 => Ok(()),
        code => Err(nfs_error(code)),
    }
}

/// The parts of a fattr3 Forge uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fattr {
    pub kind: u32,
    pub size: u64,
    pub fileid: u64,
    /// mtime: seconds and nanoseconds since the epoch.
    pub mtime: (u32, u32),
}

impl Fattr {
    /// A fattr3 (21 XDR units).
    fn parse(r: &mut XdrReader) -> io::Result<Fattr> {
        let kind = r.u32()?;
        for _ in 0..4 {
            r.u32()?; // mode, nlink, uid, gid
        }
        let size = r.u64()?;
        r.u64()?; // used
        r.u64()?; // rdev (specdata3)
        r.u64()?; // fsid
        let fileid = r.u64()?;
        r.u64()?; // atime
        let mtime = (r.u32()?, r.u32()?);
        r.u64()?; // ctime
        Ok(Fattr {
            kind,
            size,
            fileid,
            mtime,
        })
    }

    /// A post_op_attr: the attributes when the server sent them.
    fn post_op(r: &mut XdrReader) -> io::Result<Option<Fattr>> {
        if r.bool()? {
            Fattr::parse(r).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The entry named `name` these attributes describe. Links and special files are marked
    /// (never followed or read); a folder's size is 0.
    pub fn entry(&self, name: String) -> Entry {
        let dir = self.kind == NF3DIR;
        Entry {
            name,
            dir,
            size: if dir { 0 } else { self.size },
            mtime: Some(
                std::time::UNIX_EPOCH + std::time::Duration::new(self.mtime.0.into(), self.mtime.1),
            ),
            link: self.kind == NF3LNK,
            special: !matches!(self.kind, NF3REG | NF3DIR | NF3LNK),
        }
    }
}

/// One READDIRPLUS (or READDIR) entry: its name (`utf8` false: not UTF-8 or holding `/` or NUL,
/// lossily decoded, never usable as a path),
/// cookie, and the attributes and handle when the server sent them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirEntry {
    pub name: String,
    pub utf8: bool,
    pub cookie: u64,
    pub attr: Option<Fattr>,
    pub fh: Option<Vec<u8>>,
}

/// One READDIRPLUS (or READDIR) page: its entries, the cookie verifier and whether it was the last.
#[derive(Debug)]
pub(crate) struct DirPage {
    pub entries: Vec<DirEntry>,
    pub verf: [u8; 8],
    pub eof: bool,
}

/// True when a listed name can be asked for again as a path component: UTF-8 without `/` or
/// NUL (a hostile `a/b` must never be taken for a child of this folder).
fn usable(raw: &[u8]) -> bool {
    std::str::from_utf8(raw).is_ok_and(|n| !n.contains(['/', '\0']))
}

/// A READDIRPLUS reply (status included); `plus` false: a READDIR reply (no attributes or
/// handles).
fn parse_readdir(r: &mut XdrReader, plus: bool) -> io::Result<DirPage> {
    status(r)?;
    Fattr::post_op(r)?; // the folder's own
    let verf: [u8; 8] = r.fixed(8)?.try_into().unwrap();
    let mut entries = Vec::new();
    while r.bool()? {
        if entries.len() >= MAX_LIST {
            return Err(bad(format!(
                "a listing page of more than {MAX_LIST} entries"
            )));
        }
        r.u64()?; // fileid
        let raw = r.opaque(MAX_DIRPATH)?;
        let cookie = r.u64()?;
        if !plus {
            entries.push(DirEntry {
                name: String::from_utf8_lossy(raw).into_owned(),
                utf8: usable(raw),
                cookie,
                attr: None,
                fh: None,
            });
            continue;
        }
        let attr = Fattr::post_op(r)?;
        let fh = if r.bool()? {
            Some(r.opaque(MAX_FH)?.to_vec())
        } else {
            None
        };
        entries.push(DirEntry {
            name: String::from_utf8_lossy(raw).into_owned(),
            utf8: usable(raw),
            cookie,
            attr,
            fh,
        });
    }
    let eof = r.bool()?;
    Ok(DirPage { entries, verf, eof })
}

pub(crate) struct Nfs3 {
    nfs: Client,
    nfs_addr: SocketAddr,
    /// MOUNT's own connection, when it is on another port than NFS (else `nfs` serves both).
    mount: Option<Client>,
    mount_addr: SocketAddr,
    exports: Vec<Export>,
    handles: Handles,
    /// FSINFO's rtmax and wtmax (READ and WRITE chunk sizes), once asked.
    // ponytail: one pair for the session, from the first filesystem asked; another export with
    // a smaller rtmax only gives shorter READs back (read_at loops), but a smaller wtmax may
    // refuse our WRITEs (a short count is looped over, an error fails the write).
    xfer: Option<(u32, u32)>,
    /// False once the server answered READDIRPLUS with NOTSUPP (unfs3 has none): READDIR then.
    plus: bool,
}

/// A file this session created: its handle and the write verifier its unstable writes got.
pub(crate) struct Open {
    fh: Vec<u8>,
    verf: Mutex<Verf>,
}

/// The verifier the WRITEs since the last COMMIT gave, and whether it changed among them.
#[derive(Debug, Default)]
struct Verf {
    seen: Option<[u8; 8]>,
    changed: bool,
}

impl Open {
    fn state(&self) -> MutexGuard<'_, Verf> {
        self.verf.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A WRITE's verifier: one unlike the earlier ones means the server restarted between them.
    fn wrote(&self, verf: [u8; 8]) {
        let mut v = self.state();
        match v.seen {
            None => v.seen = Some(verf),
            Some(old) if old != verf => v.changed = true,
            Some(_) => {}
        }
    }

    /// A COMMIT's verifier: it must be the one every WRITE since the last COMMIT got, else the
    /// server restarted and its unstable writes may be lost (every later sync fails too).
    fn committed(&self, verf: [u8; 8]) -> io::Result<()> {
        let mut v = self.state();
        if v.changed || v.seen.is_some_and(|s| s != verf) {
            v.changed = true;
            return Err(io::Error::other(
                "the server restarted while the file was written: some writes may be lost",
            ));
        }
        v.seen = None;
        Ok(())
    }
}

/// Skips a wcc_data (a pre_op_attr of size, mtime and ctime, then a post_op_attr).
fn skip_wcc(r: &mut XdrReader) -> io::Result<()> {
    if r.bool()? {
        r.fixed(24)?;
    }
    Fattr::post_op(r)?;
    Ok(())
}

/// A sattr3 that sets only the mode.
fn sattr_mode(a: &mut Xdr, mode: u32) {
    a.bool(true).u32(mode); // mode
    a.bool(false).bool(false).bool(false); // uid, gid, size
    a.u32(0).u32(0); // atime, mtime: DONT_CHANGE
}

/// A CREATE or MKDIR reply: the new object's handle when the server sent it.
fn made(r: &mut XdrReader) -> io::Result<Option<Vec<u8>>> {
    status(r)?;
    let fh = if r.bool()? {
        Some(r.opaque(MAX_FH)?.to_vec())
    } else {
        None
    };
    Fattr::post_op(r)?;
    skip_wcc(r)?;
    Ok(fh)
}

/// LINK refusals that mean "no hard link here" (the server, the filesystem or a folder can't
/// have one): the rename falls back to a check and RENAME. PERM, ACCES (unfs3 answers a
/// folder's EPERM so; a real refusal fails the RENAME too), XDEV, ISDIR, INVAL, NOTSUPP.
fn link_falls_back(code: u32) -> bool {
    matches!(code, 1 | 13 | 18 | 21 | 22 | 10004)
}

impl Nfs3 {
    /// Connects to NFS v3 on `host`: on `port` for both MOUNT and NFS (userspace servers serve
    /// both on one), else on the ports its portmapper gives. Reads the export list.
    pub fn connect(host: &str, port: Option<u16>) -> io::Result<Nfs3> {
        let ip = resolve(host, 0)?.ip();
        let (mount_port, nfs_port) = match port {
            Some(p) => (p, p),
            None => getports(ip)?,
        };
        let nfs_addr = SocketAddr::new(ip, nfs_port);
        let mut nfs = Client::connect(nfs_addr)?;
        // A version mismatch here says the server has no NFSv3.
        nfs.null(NFS_PROG, NFS_VERS)?;
        let mut s = Nfs3 {
            nfs,
            nfs_addr,
            mount: None,
            mount_addr: SocketAddr::new(ip, mount_port),
            exports: Vec::new(),
            handles: Handles::default(),
            xfer: None,
            plus: true,
        };
        s.exports = s.fetch_exports()?.iter().map(|p| Export::new(p)).collect();
        Ok(s)
    }

    /// Calls MOUNT v3 `proc`: over the NFS connection when they share a port, else over its
    /// own, connected when first needed and once more when a kept one broke (mountd may close
    /// an idle connection; MNT and EXPORT are safe to repeat).
    fn mount_call(&mut self, proc: u32, args: &[u8]) -> io::Result<rpc::Reply> {
        if self.mount_addr == self.nfs_addr {
            return self.nfs.call(MOUNT_PROG, MOUNT_VERS, proc, args);
        }
        if let Some(c) = &mut self.mount {
            match c.call(MOUNT_PROG, MOUNT_VERS, proc, args) {
                Err(e) if rpc::rpc_error(&e).is_none() => self.mount = None,
                r => return r,
            }
        }
        let c = self.mount.insert(Client::connect(self.mount_addr)?);
        c.call(MOUNT_PROG, MOUNT_VERS, proc, args)
    }

    fn fetch_exports(&mut self) -> io::Result<Vec<String>> {
        let r = self.mount_call(MOUNTPROC3_EXPORT, &[])?;
        parse_exports(&mut r.reader())
    }

    /// The server's exports, read again (a changed list starts the handle cache over).
    pub fn exports(&mut self) -> io::Result<Vec<String>> {
        let list = self.fetch_exports()?;
        if !list.iter().eq(self.exports.iter().map(|e| &e.path)) {
            self.exports = list.iter().map(|p| Export::new(p)).collect();
            self.handles.clear();
        }
        Ok(list)
    }

    /// Export `i`'s root handle (MNT once).
    fn root(&mut self, i: usize) -> io::Result<Vec<u8>> {
        if let Some(fh) = &self.exports[i].root {
            return Ok(fh.clone());
        }
        let mut a = Xdr::new();
        a.string(&self.exports[i].path);
        let r = self.mount_call(MOUNTPROC3_MNT, &a.buf)?;
        let fh = parse_mnt(&mut r.reader())?;
        self.exports[i].root = Some(fh.clone());
        Ok(fh)
    }

    /// The handle of `path`: cached, else LOOKUPs from the nearest cached folder above it (or
    /// its export's root). A path above or between exports has none (read-only, virtual).
    fn handle(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let comps = components(path)?;
        let (export, base) = match place(&self.exports, &comps) {
            Place::Export { export, rest } => (export, comps.len() - rest.len()),
            Place::Virtual(_) => {
                return Err(io::Error::new(
                    ErrorKind::ReadOnlyFilesystem,
                    "above the exports: a read-only folder",
                ));
            }
            Place::NotFound => {
                return Err(io::Error::new(ErrorKind::NotFound, "not inside an export"));
            }
        };
        // Only keys strictly below the export's root: one at or above it may hold another
        // export's view (a nested export listed from its parent's).
        let cached = (base + 1..=comps.len())
            .rev()
            .find_map(|n| Some((n, self.handles.get(&key(&comps[..n]))?.to_vec())));
        let (done, mut fh) = match cached {
            Some(c) => c,
            None => (base, self.root(export)?),
        };
        for n in done..comps.len() {
            fh = self.lookup(&fh, comps[n])?;
            self.handles.put(key(&comps[..=n]), fh.clone());
        }
        Ok(fh)
    }

    /// Runs `f`; on STALE every cached handle (and the exports' roots) is dropped and it runs
    /// once more from fresh LOOKUPs. Only for calls a STALE reply left undone.
    fn retry_stale<T>(&mut self, mut f: impl FnMut(&mut Self) -> io::Result<T>) -> io::Result<T> {
        match f(self) {
            Err(e) if is_stale(&e) => {
                self.handles.clear();
                for export in &mut self.exports {
                    export.root = None;
                }
                f(self)
            }
            r => r,
        }
    }

    /// Runs `f` on `path`'s handle, once more from fresh LOOKUPs on STALE.
    fn with_handle<T>(
        &mut self,
        path: &str,
        mut f: impl FnMut(&mut Self, &[u8]) -> io::Result<T>,
    ) -> io::Result<T> {
        self.retry_stale(|s| {
            let fh = s.handle(path)?;
            f(s, &fh)
        })
    }

    /// `path`'s parent folder and its own name, for making, renaming or removing it. An export
    /// or a folder above one is never changed: `taken` (AlreadyExists for a new one there).
    fn parent(&self, path: &str, taken: ErrorKind) -> io::Result<(String, String)> {
        let comps = components(path)?;
        match place(&self.exports, &comps) {
            Place::Export { rest, .. } if !rest.is_empty() => {}
            Place::NotFound => {
                return Err(io::Error::new(ErrorKind::NotFound, "not inside an export"));
            }
            _ => {
                return Err(io::Error::new(
                    taken,
                    "an export or a folder above one (never changed)",
                ));
            }
        }
        let (name, dir) = comps.split_last().expect("below an export root");
        Ok((key(dir), name.to_string()))
    }

    /// Calls NFS v3 `proc` with `args`.
    fn call(&mut self, proc: u32, args: &Xdr) -> io::Result<rpc::Reply> {
        self.nfs.call(NFS_PROG, NFS_VERS, proc, &args.buf)
    }

    /// LOOKUP of `name` in folder `dir`: its handle, and its attributes when the server sent
    /// them. A link is not followed (its own handle).
    fn lookup_attr(&mut self, dir: &[u8], name: &str) -> io::Result<(Vec<u8>, Option<Fattr>)> {
        let mut a = Xdr::new();
        a.opaque(dir).string(name);
        let reply = self.call(NFSPROC3_LOOKUP, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        let fh = r.opaque(MAX_FH)?.to_vec();
        Ok((fh, Fattr::post_op(&mut r)?))
    }

    /// LOOKUP of `name` in folder `dir`.
    fn lookup(&mut self, dir: &[u8], name: &str) -> io::Result<Vec<u8>> {
        self.lookup_attr(dir, name).map(|(fh, _)| fh)
    }

    /// GETATTR of a handle.
    fn getattr(&mut self, fh: &[u8]) -> io::Result<Fattr> {
        let mut a = Xdr::new();
        a.opaque(fh);
        let reply = self.call(NFSPROC3_GETATTR, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        Fattr::parse(&mut r)
    }

    /// FSINFO's (rtmax, wtmax) for the filesystem holding `fh` (asked once), within our bounds.
    fn xfer(&mut self, fh: &[u8]) -> io::Result<(u32, u32)> {
        if let Some(x) = self.xfer {
            return Ok(x);
        }
        let mut a = Xdr::new();
        a.opaque(fh);
        let reply = self.call(NFSPROC3_FSINFO, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        Fattr::post_op(&mut r)?;
        let rtmax = r.u32()?;
        r.u32()?; // rtpref
        r.u32()?; // rtmult
        let wtmax = r.u32()?;
        let bound = |n: u32| {
            if n == 0 {
                DEFAULT_XFER
            } else {
                n.min(MAX_XFER)
            }
        };
        let x = (bound(rtmax), bound(wtmax));
        self.xfer = Some(x);
        Ok(x)
    }

    /// One READ: the bytes and whether they reach the end of the file.
    fn read(&mut self, fh: &[u8], offset: u64, count: u32) -> io::Result<(Vec<u8>, bool)> {
        let mut a = Xdr::new();
        a.opaque(fh).u64(offset).u32(count);
        let reply = self.call(NFSPROC3_READ, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        Fattr::post_op(&mut r)?;
        let n = r.u32()? as usize; // count: the data's own length again
        let eof = r.bool()?;
        let data = r.opaque(count as usize)?.to_vec();
        if n != data.len() {
            return Err(bad(format!(
                "the server's READ says {n} bytes but sent {}",
                data.len()
            )));
        }
        Ok((data, eof))
    }

    /// One unstable WRITE: how many bytes the server took and its write verifier.
    fn write(&mut self, fh: &[u8], offset: u64, data: &[u8]) -> io::Result<(usize, [u8; 8])> {
        let mut a = Xdr::new();
        a.opaque(fh)
            .u64(offset)
            .u32(data.len() as u32)
            .u32(UNSTABLE);
        a.opaque(data);
        let reply = self.call(NFSPROC3_WRITE, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        skip_wcc(&mut r)?;
        let count = r.u32()? as usize;
        r.u32()?; // committed: how stable; the COMMIT decides
        let verf = r.fixed(8)?.try_into().unwrap();
        if count > data.len() {
            return Err(bad(format!(
                "the server's WRITE took {count} of {} bytes",
                data.len()
            )));
        }
        Ok((count, verf))
    }

    /// LINK of `file` as `name` in folder `dir`: the nfsstat3 (0 = linked) for the caller to
    /// judge; STALE as an error (nothing done: [`Nfs3::retry_stale`] may run it again).
    fn link(&mut self, file: &[u8], dir: &[u8], name: &str) -> io::Result<u32> {
        let mut a = Xdr::new();
        a.opaque(file).opaque(dir).string(name);
        let reply = self.call(NFSPROC3_LINK, &a)?;
        match reply.reader().u32()? {
            70 => Err(nfs_error(70)),
            code => Ok(code),
        }
    }

    /// REMOVE of `name` in folder `dir`.
    fn remove_in(&mut self, dir: &[u8], name: &str) -> io::Result<()> {
        let mut a = Xdr::new();
        a.opaque(dir).string(name);
        let reply = self.call(NFSPROC3_REMOVE, &a)?;
        status(&mut reply.reader())
    }

    /// RENAME (replaces what is at the target: the caller checked first).
    fn rename(&mut self, from: (&[u8], &str), to: (&[u8], &str)) -> io::Result<()> {
        let mut a = Xdr::new();
        a.opaque(from.0).string(from.1).opaque(to.0).string(to.1);
        let reply = self.call(NFSPROC3_RENAME, &a)?;
        status(&mut reply.reader())
    }

    /// Every entry of folder `dir` (READDIRPLUS, page by page; READDIR where the server has no
    /// READDIRPLUS), `.` and `..` left out.
    fn readdir(&mut self, dir: &[u8]) -> io::Result<Vec<DirEntry>> {
        let (mut cookie, mut verf) = (0u64, [0u8; 8]);
        let mut out: Vec<DirEntry> = Vec::new();
        let (mut seen, mut names) = (0usize, 0usize);
        loop {
            let mut a = Xdr::new();
            a.opaque(dir).u64(cookie).fixed(&verf);
            let page = if self.plus {
                a.u32(DIRCOUNT).u32(MAXCOUNT);
                let reply = self.call(NFSPROC3_READDIRPLUS, &a)?;
                match parse_readdir(&mut reply.reader(), true) {
                    Err(e) if e.kind() == ErrorKind::Unsupported && cookie == 0 => {
                        self.plus = false;
                        continue;
                    }
                    page => page?,
                }
            } else {
                a.u32(MAXCOUNT);
                let reply = self.call(NFSPROC3_READDIR, &a)?;
                parse_readdir(&mut reply.reader(), false)?
            };
            let Some(last) = page.entries.last() else {
                if page.eof {
                    return Ok(out);
                }
                return Err(bad("the server's listing stopped short (an empty page)"));
            };
            cookie = last.cookie;
            verf = page.verf;
            seen += page.entries.len();
            names += page.entries.iter().map(|e| e.name.len()).sum::<usize>();
            if seen > MAX_LIST + 2 || names > MAX_LIST_NAMES {
                return Err(io::Error::new(
                    ErrorKind::OutOfMemory,
                    format!("a folder of more than {MAX_LIST} entries (or names too long)"),
                ));
            }
            out.extend(
                page.entries
                    .into_iter()
                    .filter(|e| e.name != "." && e.name != ".."),
            );
            if page.eof {
                return Ok(out);
            }
        }
    }

    /// A path above or between exports, as a folder entry (`Some`), or `None` for one inside
    /// an export. Nothing at all there is `NotFound`.
    fn virtual_place(&self, path: &str) -> io::Result<Option<Vec<String>>> {
        match place(&self.exports, &components(path)?) {
            Place::Export { .. } => Ok(None),
            Place::Virtual(children) => Ok(Some(children)),
            Place::NotFound => Err(io::Error::new(
                ErrorKind::NotFound,
                "no such file or folder (not inside an export)",
            )),
        }
    }

    pub fn stat(&mut self, path: &str) -> io::Result<Entry> {
        let name = components(path)?.last().copied().unwrap_or("/").to_string();
        if self.virtual_place(path)?.is_some() {
            return Ok(virtual_dir(&name));
        }
        let attr = self.with_handle(path, |s, fh| s.getattr(fh))?;
        Ok(attr.entry(name))
    }

    pub fn list(&mut self, path: &str) -> io::Result<Vec<Entry>> {
        if let Some(children) = self.virtual_place(path)? {
            return Ok(children.iter().map(|c| virtual_dir(c)).collect());
        }
        let entries = self.with_handle(path, |s, fh| s.readdir(fh))?;
        let comps = components(path)?;
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if !e.utf8 {
                // Not a name Forge can ask for again: listed, marked, never read.
                let mut entry = e
                    .attr
                    .map_or_else(|| virtual_dir(""), |a| a.entry(String::new()));
                entry.name = e.name;
                entry.dir = false;
                entry.special = true;
                out.push(entry);
                continue;
            }
            let child = format!("{}/{}", key(&comps).trim_end_matches('/'), e.name);
            if let Some(fh) = &e.fh {
                self.handles.put(child.clone(), fh.clone());
            }
            let attr = match e.attr {
                Some(a) => a,
                // The server left them out (it may; READDIR never sends them): LOOKUP by name,
                // which usually brings them, else GETATTR.
                // ponytail: one or two calls per entry, one after another.
                None => {
                    let found = self
                        .with_handle(path, |s, dir| s.lookup_attr(dir, &e.name))
                        .and_then(|(fh, attr)| match attr {
                            Some(a) => Ok((fh, a)),
                            None => self.getattr(&fh).map(|a| (fh, a)),
                        });
                    match found {
                        Ok((fh, a)) => {
                            self.handles.put(child, fh);
                            a
                        }
                        // Renamed or removed since the READDIR (a temporary name renamed into
                        // place): it isn't listed.
                        Err(err)
                            if matches!(
                                err.kind(),
                                io::ErrorKind::NotFound | io::ErrorKind::StaleNetworkFileHandle
                            ) =>
                        {
                            self.handles.forget(&child);
                            continue;
                        }
                        Err(err) => return Err(err),
                    }
                }
            };
            out.push(attr.entry(e.name));
        }
        Ok(out)
    }

    pub fn read_at(&mut self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.with_handle(path, |s, fh| {
            let (rtmax, _) = s.xfer(fh)?;
            let mut out = Vec::with_capacity(len.min(MAX_XFER as usize));
            while out.len() < len {
                let want = (len - out.len()).min(rtmax as usize) as u32;
                let at = offset
                    .checked_add(out.len() as u64)
                    .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "offset past 2^64"))?;
                let (data, eof) = s.read(fh, at, want)?;
                let got = data.len();
                out.extend_from_slice(&data);
                if eof {
                    break;
                }
                if got == 0 {
                    return Err(bad("the server's READ gave nothing before the end"));
                }
            }
            Ok(out)
        })
    }

    /// MKDIR (mode 0777); the server's EXIST is `AlreadyExists`.
    pub fn mkdir(&mut self, path: &str) -> io::Result<()> {
        let (dir, name) = self.parent(path, ErrorKind::AlreadyExists)?;
        let fh = self.with_handle(&dir, |s, dir| {
            let mut a = Xdr::new();
            a.opaque(dir).string(&name);
            sattr_mode(&mut a, DIR_MODE);
            let reply = s.call(NFSPROC3_MKDIR, &a)?;
            made(&mut reply.reader())
        })?;
        // A handle cached for this path is of something else (renamed away by another client).
        let key = key(&components(path)?);
        match fh {
            Some(fh) => self.handles.put(key, fh),
            None => self.handles.forget(&key),
        }
        Ok(())
    }

    /// CREATE GUARDED (mode 0666): never over something there (`AlreadyExists`).
    pub fn create(&mut self, path: &str) -> io::Result<Open> {
        let (dir, name) = self.parent(path, ErrorKind::AlreadyExists)?;
        let fh = self.with_handle(&dir, |s, dir| {
            let mut a = Xdr::new();
            a.opaque(dir).string(&name).u32(GUARDED);
            sattr_mode(&mut a, FILE_MODE);
            let reply = s.call(NFSPROC3_CREATE, &a)?;
            made(&mut reply.reader())
        })?;
        let key = key(&components(path)?);
        let fh = match fh {
            Some(fh) => {
                self.handles.put(key, fh.clone());
                fh
            }
            // The server may leave the handle out: look the new file up.
            None => {
                self.handles.forget(&key);
                self.handle(path)?
            }
        };
        Ok(Open {
            fh,
            verf: Mutex::default(),
        })
    }

    /// LINK `from` as `to` (refused when `to` is taken: atomic, never replacing), then REMOVE
    /// `from`. Where the server or the object has no hard links (a folder, NOTSUPP, ...): `to`
    /// must not be there, then RENAME.
    // ponytail: no identity: REMOVE goes by name, so a file another client put at `from`
    // between LINK and REMOVE is the one removed (NFS has no conditional remove).
    pub fn rename_no_replace(&mut self, from: &str, to: &str) -> io::Result<()> {
        let (from_dir, from_name) = self.parent(from, ErrorKind::ReadOnlyFilesystem)?;
        let (to_dir, to_name) = self.parent(to, ErrorKind::AlreadyExists)?;
        let (from_key, to_key) = (key(&components(from)?), key(&components(to)?));
        let code = self.retry_stale(|s| {
            let file = s.handle(from)?;
            let dir = s.handle(&to_dir)?;
            s.link(&file, &dir, &to_name)
        })?;
        self.handles.forget(&to_key);
        if code == 0 {
            let removed = self.with_handle(&from_dir, |s, dir| s.remove_in(dir, &from_name));
            self.handles.forget(&from_key);
            return removed.map_err(|e| {
                io::Error::new(e.kind(), format!("{to} was made, but {from} stays: {e}"))
            });
        }
        if !link_falls_back(code) {
            return Err(nfs_error(code));
        }
        // ponytail: two steps; something made at `to` between the check and the RENAME is
        // replaced (NFSv3's RENAME has no no-replace flag).
        match self.handle(to) {
            Ok(_) => return Err(io::Error::new(ErrorKind::AlreadyExists, "already exists")),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.rename_replace(from, to)
    }

    /// RENAME of `from` to `to`, replacing a file there (the server renames as POSIX does,
    /// which refuses to put a file over a folder).
    pub fn rename_replace(&mut self, from: &str, to: &str) -> io::Result<()> {
        let (from_dir, from_name) = self.parent(from, ErrorKind::ReadOnlyFilesystem)?;
        let (to_dir, to_name) = self.parent(to, ErrorKind::AlreadyExists)?;
        let (from_key, to_key) = (key(&components(from)?), key(&components(to)?));
        let renamed = self.retry_stale(|s| {
            let fd = s.handle(&from_dir)?;
            let td = s.handle(&to_dir)?;
            s.rename((&fd, &from_name), (&td, &to_name))
        });
        self.handles.forget(&from_key);
        self.handles.forget(&to_key);
        renamed
    }

    /// REMOVE of a file. Some servers (unfs3: libc `remove`) take an empty folder too, so a
    /// folder is refused first.
    // ponytail: two calls; a folder put at the name between them is removed if empty.
    pub fn remove(&mut self, path: &str) -> io::Result<()> {
        let (dir, name) = self.parent(path, ErrorKind::ReadOnlyFilesystem)?;
        self.handles.forget(&key(&components(path)?));
        if self.stat(path)?.dir {
            return Err(io::Error::new(ErrorKind::IsADirectory, "is a folder"));
        }
        let removed = self.with_handle(&dir, |s, dir| s.remove_in(dir, &name));
        self.handles.forget(&key(&components(path)?));
        removed
    }

    /// FSSTAT's abytes: what this user may still write there.
    pub fn free_space(&mut self, path: &str) -> io::Result<u64> {
        self.with_handle(path, |s, fh| {
            let mut a = Xdr::new();
            a.opaque(fh);
            let reply = s.call(NFSPROC3_FSSTAT, &a)?;
            let mut r = reply.reader();
            status(&mut r)?;
            Fattr::post_op(&mut r)?;
            r.u64()?; // tbytes
            r.u64()?; // fbytes
            r.u64() // abytes
        })
    }

    /// Unstable WRITEs of `bytes` at `offset` in wtmax chunks (a short count continues where
    /// the server stopped). A STALE handle fails: the file is never looked up again by path,
    /// which could be another file by now.
    pub fn write_at(&mut self, open: &Open, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let (_, wtmax) = self.xfer(&open.fh)?;
        let mut done = 0;
        while done < bytes.len() {
            let chunk = &bytes[done..][..(bytes.len() - done).min(wtmax as usize)];
            let at = offset
                .checked_add(done as u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "offset past 2^64"))?;
            let (count, verf) = self.write(&open.fh, at, chunk)?;
            open.wrote(verf);
            if count == 0 {
                return Err(bad("the server's WRITE took nothing"));
            }
            done += count;
        }
        Ok(())
    }

    /// COMMIT of the whole file; fails when the write verifier changed since the WRITEs.
    pub fn sync(&mut self, open: &Open) -> io::Result<()> {
        let mut a = Xdr::new();
        a.opaque(&open.fh).u64(0).u32(0);
        let reply = self.call(NFSPROC3_COMMIT, &a)?;
        let mut r = reply.reader();
        status(&mut r)?;
        skip_wcc(&mut r)?;
        open.committed(r.fixed(8)?.try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exports(paths: &[&str]) -> Vec<Export> {
        paths.iter().map(|p| Export::new(p)).collect()
    }

    fn at<'a>(ex: &[Export], path: &'a str) -> Place<'a> {
        place(ex, &components(path).unwrap())
    }

    #[test]
    fn components_are_checked() {
        assert_eq!(components("/").unwrap(), Vec::<&str>::new());
        assert_eq!(components("//srv//games/").unwrap(), ["srv", "games"]);
        for bad in ["", "srv/games", "/srv/../etc", "/srv/./x", "/a\0b"] {
            let e = components(bad).unwrap_err();
            assert_eq!(e.kind(), ErrorKind::InvalidInput, "{bad:?}");
        }
        assert_eq!(key(&[]), "/");
        assert_eq!(key(&["srv", "games"]), "/srv/games");
    }

    #[test]
    fn longest_export_wins() {
        let ex = exports(&["/srv", "/srv/games/", "/data/nfs"]);
        assert_eq!(
            at(&ex, "/srv/games/My Game/eboot.bin"),
            Place::Export {
                export: 1,
                rest: vec!["My Game", "eboot.bin"]
            }
        );
        assert_eq!(
            at(&ex, "/srv/games"),
            Place::Export {
                export: 1,
                rest: vec![]
            }
        );
        assert_eq!(
            at(&ex, "/srv/other"),
            Place::Export {
                export: 0,
                rest: vec!["other"]
            }
        );
        // Component-wise: /srv/gamesX is under /srv, not /srv/games.
        assert_eq!(
            at(&ex, "/srv/gamesX"),
            Place::Export {
                export: 0,
                rest: vec!["gamesX"]
            }
        );
        assert_eq!(
            at(&ex, "/data/nfs/a"),
            Place::Export {
                export: 2,
                rest: vec!["a"]
            }
        );
    }

    #[test]
    fn handle_cache_forgets_below_and_stays_bounded() {
        let mut h = Handles::default();
        for p in ["/srv", "/srv/a", "/srv/a/b", "/srv/ab", "/x"] {
            h.put(p.to_string(), p.as_bytes().to_vec());
        }
        h.forget("/srv/a");
        assert!(h.get("/srv/a").is_none() && h.get("/srv/a/b").is_none());
        assert_eq!(h.get("/srv/ab"), Some(&b"/srv/ab"[..]));
        assert!(h.get("/srv").is_some() && h.get("/x").is_some());
        h.forget("/");
        assert!(h.map.is_empty());
        for i in 0..MAX_HANDLES {
            h.put(format!("/{i}"), vec![1]);
        }
        h.put("/0".to_string(), vec![2]); // a known key: no start over
        assert_eq!(h.map.len(), MAX_HANDLES);
        h.put("/new".to_string(), vec![3]);
        assert_eq!(h.map.len(), 1);
        assert_eq!(h.get("/new"), Some(&[3u8][..]));
        // An old handle is looked up again (another client may have replaced the name).
        let old = Instant::now().checked_sub(HANDLE_TTL).unwrap();
        h.map.insert("/old".to_string(), (vec![4], old));
        assert!(h.get("/old").is_none());
    }

    #[test]
    fn listed_names_with_a_slash_or_nul_are_unusable() {
        assert!(usable(b"My Game") && usable("Игра".as_bytes()));
        assert!(!usable(b"a/b") && !usable(b"a\0b") && !usable(b"\xff"));
    }

    #[test]
    fn link_refusals_that_fall_back_to_rename() {
        // PERM, ACCES, XDEV, ISDIR, INVAL, NOTSUPP: no hard link for this object or server.
        for code in [1, 13, 18, 21, 22, 10004] {
            assert!(link_falls_back(code), "{code}");
        }
        // EXIST (the target is taken), NOENT, NOSPC, ROFS, STALE: a real answer.
        for code in [0, 2, 17, 28, 30, 70] {
            assert!(!link_falls_back(code), "{code}");
        }
    }

    #[test]
    fn write_verifier_must_hold_until_commit() {
        let open = || Open {
            fh: vec![1],
            verf: Mutex::default(),
        };
        let (a, b) = (*b"verf0001", *b"verf0002");
        // Same verifier throughout; a commit with no writes before it.
        let o = open();
        o.committed(b).unwrap();
        o.wrote(a);
        o.wrote(a);
        o.committed(a).unwrap();
        // After a commit, writes start afresh (the server may have restarted in between).
        o.wrote(b);
        o.committed(b).unwrap();
        // The COMMIT's verifier differs from the WRITEs': lost, and stays failed.
        let o = open();
        o.wrote(a);
        let e = o.committed(b).unwrap_err();
        assert!(e.to_string().contains("restarted"), "{e}");
        assert!(o.committed(b).is_err() && o.committed(a).is_err());
        // The WRITEs' verifier changed between them, even if the COMMIT matches the last.
        let o = open();
        o.wrote(a);
        o.wrote(b);
        assert!(o.committed(b).is_err());
    }

    /// Appends a fattr3 of `kind`, `size` and `mtime` to `x`.
    fn fattr(x: &mut Xdr, kind: u32, size: u64, mtime: (u32, u32)) {
        x.u32(kind).u32(0o777).u32(1).u32(0).u32(0);
        x.u64(size).u64(size).u64(0).u64(9).u64(42);
        x.u32(1).u32(2).u32(mtime.0).u32(mtime.1).u32(3).u32(4);
    }

    /// A one-port server (unfs3's shape, v3 only: NFS v4 calls get PROG_MISMATCH) answering
    /// each call with `answer(prog, proc, args, reply)` (the accepted reply's header already
    /// in) until the client hangs up; gives back the (prog, proc) of every call. A connection
    /// that only asked for v4 (lib.rs tries v4.1 first) is followed by the next.
    fn serve(
        listener: std::net::TcpListener,
        mut answer: impl FnMut(u32, u32, &mut XdrReader, &mut Xdr),
    ) -> Vec<(u32, u32)> {
        let mut calls = Vec::new();
        while calls.is_empty() {
            serve_one(&listener, &mut answer, &mut calls);
        }
        calls
    }

    fn serve_one(
        listener: &std::net::TcpListener,
        answer: &mut impl FnMut(u32, u32, &mut XdrReader, &mut Xdr),
        calls: &mut Vec<(u32, u32)>,
    ) {
        use std::io::{Read, Write};
        let (mut s, _) = listener.accept().unwrap();
        let mut head = [0u8; 4];
        while s.read_exact(&mut head).is_ok() {
            let mut call = vec![0; (u32::from_be_bytes(head) & 0x7fff_ffff) as usize];
            s.read_exact(&mut call).unwrap();
            let mut r = XdrReader::new(&call);
            let xid = r.u32().unwrap();
            let [_, _, prog, vers, proc] = [(); 5].map(|_| r.u32().unwrap());
            for _ in 0..2 {
                r.u32().unwrap();
                r.opaque(400).unwrap();
            }
            let mut x = Xdr::new();
            x.u32(xid).u32(1).u32(0).u32(0).u32(0);
            if prog == NFS_PROG && vers != NFS_VERS {
                x.u32(2).u32(NFS_VERS).u32(NFS_VERS); // PROG_MISMATCH: a v3-only server
            } else {
                calls.push((prog, proc));
                answer(prog, proc, &mut r, x.u32(0));
            }
            let mut out = (0x8000_0000 | x.buf.len() as u32).to_be_bytes().to_vec();
            out.extend_from_slice(&x.buf);
            s.write_all(&out).unwrap();
        }
    }

    /// The read tests' server: NULL, EXPORT, MNT and the read procedures over a fixed tree.
    fn fake_server(listener: std::net::TcpListener) -> Vec<(u32, u32)> {
        serve(listener, read_answer)
    }

    fn read_answer(prog: u32, proc: u32, r: &mut XdrReader, x: &mut Xdr) {
        match (prog, proc) {
            (MOUNT_PROG, MOUNTPROC3_EXPORT) => {
                x.bool(true).string("/srv/games").bool(false);
                x.bool(true).string("/srv/music").bool(false).bool(false);
            }
            (MOUNT_PROG, MOUNTPROC3_MNT) if r.opaque(1024).unwrap() == b"/srv/games" => {
                x.u32(0).opaque(b"games-root").u32(1).u32(AUTH_SYS);
            }
            (MOUNT_PROG, MOUNTPROC3_MNT) => {
                x.u32(13);
            }
            (NFS_PROG, NFSPROC3_GETATTR) => match r.opaque(64).unwrap() {
                b"games-root" | b"sub-fh" => fattr(x.u32(0), NF3DIR, 4096, (5, 0)),
                b"f-fh" => fattr(x.u32(0), NF3REG, 11, (7, 9)),
                _ => {
                    x.u32(70);
                }
            },
            (NFS_PROG, NFSPROC3_LOOKUP) => {
                let dir = r.opaque(64).unwrap().to_vec();
                match (&dir[..], r.opaque(255).unwrap()) {
                    (b"games-root", b"f") => {
                        fattr(x.u32(0).opaque(b"f-fh").bool(true), NF3REG, 11, (7, 9));
                        x.bool(false);
                    }
                    (b"games-root", b"sub") => {
                        x.u32(0).opaque(b"sub-fh").u32(0).u32(0);
                    }
                    _ => {
                        x.u32(2).bool(false);
                    }
                }
            }
            (NFS_PROG, NFSPROC3_FSINFO) => {
                x.u32(0).bool(false).u32(4); // rtmax 4: READs in chunks
                x.u32(4).u32(1).u32(4).u32(4).u32(1).u32(4).u64(1 << 40);
                x.u32(0).u32(1).u32(0);
            }
            (NFS_PROG, NFSPROC3_READ) => {
                const DATA: &[u8] = b"hello world";
                r.opaque(64).unwrap();
                let off = (r.u64().unwrap() as usize).min(DATA.len());
                let end = (off + r.u32().unwrap() as usize).min(DATA.len());
                x.u32(0).bool(false).u32((end - off) as u32);
                x.bool(end == DATA.len()).opaque(&DATA[off..end]);
            }
            (NFS_PROG, NFSPROC3_READDIRPLUS) => {
                r.opaque(64).unwrap();
                x.u32(0).bool(false).fixed(b"verf0001");
                if r.u64().unwrap() == 0 {
                    x.bool(true)
                        .u64(1)
                        .string(".")
                        .u64(1)
                        .bool(false)
                        .bool(false);
                    fattr(
                        x.bool(true).u64(2).string("f").u64(2).bool(true),
                        NF3REG,
                        11,
                        (7, 9),
                    );
                    x.bool(true).opaque(b"f-fh").bool(false).bool(false);
                } else {
                    x.bool(true)
                        .u64(3)
                        .string("sub")
                        .u64(3)
                        .bool(false)
                        .bool(false);
                    x.bool(false).bool(true);
                }
            }
            (NFS_PROG, NFSPROC3_FSSTAT) => {
                x.u32(0).bool(false).u64(1 << 30).u64(1 << 20).u64(12345);
                x.u64(0).u64(0).u64(0).u32(0);
            }
            _ => {}
        }
    }

    #[test]
    fn reads_through_one_port() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || fake_server(listener));
        let mut s = Nfs3::connect("127.0.0.1", Some(port)).unwrap();
        let f = s.stat("/srv/games/f").unwrap();
        assert_eq!((f.name.as_str(), f.size, f.dir), ("f", 11, false));
        let root = s.stat("/srv/games").unwrap();
        assert!(root.dir && root.name == "games");
        // Two pages; "." left out; "sub" came without attributes (asked for by name).
        let list = s.list("/srv/games").unwrap();
        let names: Vec<(&str, bool)> = list.iter().map(|e| (e.name.as_str(), e.dir)).collect();
        assert_eq!(names, [("f", false), ("sub", true)]);
        assert_eq!(list[0], f, "a listing and a stat agree");
        // rtmax 4: three READs, short only at the end.
        assert_eq!(s.read_at("/srv/games/f", 0, 100).unwrap(), b"hello world");
        assert_eq!(s.read_at("/srv/games/f", 6, 3).unwrap(), b"wor");
        assert!(s.read_at("/srv/games/f", 20, 4).unwrap().is_empty());
        assert_eq!(s.free_space("/srv/games/sub").unwrap(), 12345);
        let e = s.stat("/srv/games/nothing").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::NotFound);
        drop(s);
        let calls = server.join().unwrap();
        let count = |p| calls.iter().filter(|c| **c == (NFS_PROG, p)).count();
        assert_eq!(count(NFSPROC3_FSINFO), 1, "FSINFO asked once");
        assert_eq!(count(NFSPROC3_READDIRPLUS), 2);
        assert_eq!(count(NFSPROC3_READ), 3 + 1 + 1, "{calls:?}");
    }

    #[test]
    fn writes_through_the_session() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut disk = Disk::default();
            let calls = serve(listener, |p, c, r, x| disk.answer(p, c, r, x));
            (calls, disk)
        });
        let kind = |r: io::Result<crate::File>| r.err().map(|e| e.kind());
        // v4.1 isn't there: the session falls back to v3 (through lib.rs, as callers do).
        let s = crate::Session::connect("127.0.0.1", Some(port)).unwrap();
        assert_eq!(s.version(), "NFSv3");
        {
            // wtmax 4: out of order, in chunks of at most 4.
            let f = s.create("/srv/games/a.bin").unwrap();
            f.write_at(6, b"world!").unwrap();
            f.write_at(0, b"hello ").unwrap();
            f.sync().unwrap();
            f.sync().unwrap(); // nothing new written: fine
        }
        assert_eq!(
            s.read_at("/srv/games/a.bin", 0, 100).unwrap(),
            b"hello world!"
        );
        let taken = Some(ErrorKind::AlreadyExists);
        assert_eq!(kind(s.create("/srv/games/a.bin")), taken);
        assert_eq!(kind(s.create("/srv/games")), taken, "an export root");
        assert_eq!(kind(s.create("/srv")), taken, "above the exports");
        assert_eq!(kind(s.create("/home/x")), Some(ErrorKind::NotFound));
        assert_eq!(kind(s.create("/srv/games/no/x")), Some(ErrorKind::NotFound));
        let e = s.create("/srv/games/a.bin").err().unwrap();
        assert!(e.to_string().starts_with("/srv/games/a.bin: "), "{e}");
        // A handle left out of CREATE's reply is looked up; a WRITE taking half continues.
        {
            let f = s.create("/srv/games/x.nofh").unwrap();
            f.write_at(0, b"abc").unwrap();
            let f = s.create("/srv/games/y.half").unwrap();
            f.write_at(0, b"0123456789").unwrap();
            f.sync().unwrap();
        }
        assert_eq!(s.read_at("/srv/games/x.nofh", 0, 9).unwrap(), b"abc");
        assert_eq!(
            s.read_at("/srv/games/y.half", 0, 99).unwrap(),
            b"0123456789"
        );
        // The server restarted between the WRITEs and the COMMIT.
        {
            let f = s.create("/srv/games/z.reboot").unwrap();
            f.write_at(0, b"lost?").unwrap();
            let e = f.sync().unwrap_err();
            assert!(e.to_string().contains("restarted"), "{e}");
        }
        // Folders.
        s.mkdir("/srv/games/d").unwrap();
        assert!(s.stat("/srv/games/d").unwrap().dir);
        let mkdir = |p| s.mkdir(p).unwrap_err().kind();
        assert_eq!(mkdir("/srv/games/d"), ErrorKind::AlreadyExists);
        assert_eq!(mkdir("/srv/games"), ErrorKind::AlreadyExists);
        assert_eq!(mkdir("/srv"), ErrorKind::AlreadyExists);
        assert_eq!(mkdir("/srv/games/no/d"), ErrorKind::NotFound);
        // Renames: LINK + REMOVE; refused over a file; folders and NOTSUPP fall back.
        s.rename_no_replace("/srv/games/a.bin", "/srv/games/d/b.bin")
            .unwrap();
        let gone = |p| s.stat(p).is_err_and(|e| e.kind() == ErrorKind::NotFound);
        assert!(gone("/srv/games/a.bin"));
        assert_eq!(s.read_at("/srv/games/d/b.bin", 0, 5).unwrap(), b"hello");
        drop(s.create("/srv/games/c.bin").unwrap());
        let e = s.rename_no_replace("/srv/games/c.bin", "/srv/games/d/b.bin");
        assert_eq!(e.unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert!(!gone("/srv/games/c.bin"));
        s.rename_no_replace("/srv/games/d", "/srv/games/e").unwrap();
        assert!(gone("/srv/games/d") && gone("/srv/games/d/b.bin"));
        assert_eq!(s.read_at("/srv/games/e/b.bin", 0, 5).unwrap(), b"hello");
        s.mkdir("/srv/games/f").unwrap();
        let e = s.rename_no_replace("/srv/games/e", "/srv/games/f");
        assert_eq!(e.unwrap_err().kind(), ErrorKind::AlreadyExists);
        drop(s.create("/srv/games/g.nolink").unwrap());
        s.rename_no_replace("/srv/games/g.nolink", "/srv/games/h")
            .unwrap();
        assert!(gone("/srv/games/g.nolink") && !gone("/srv/games/h"));
        let e = s.rename_no_replace("/srv/games/nothing", "/srv/games/i");
        assert_eq!(e.unwrap_err().kind(), ErrorKind::NotFound);
        let e = s.rename_no_replace("/srv/games/h", "/srv/games");
        assert_eq!(e.unwrap_err().kind(), ErrorKind::AlreadyExists);
        // Removes.
        s.remove("/srv/games/e/b.bin").unwrap();
        assert!(gone("/srv/games/e/b.bin"));
        assert_eq!(
            s.remove("/srv/games/e/b.bin").unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            s.remove("/srv/games/e").unwrap_err().kind(),
            ErrorKind::IsADirectory
        );
        assert_eq!(
            s.remove("/srv/games").unwrap_err().kind(),
            ErrorKind::ReadOnlyFilesystem
        );
        drop(s);
        let (calls, disk) = server.join().unwrap();
        assert!(disk.writes.iter().all(|w| w.1 <= 4), "{:?}", disk.writes);
        assert_eq!(&disk.writes[..2], [(6, 4), (10, 2)], "chunks of wtmax");
        let count = |p| calls.iter().filter(|c| **c == (NFS_PROG, p)).count();
        assert_eq!(count(NFSPROC3_FSINFO), 1);
        assert_eq!(count(NFSPROC3_RENAME), 2, "the folder and the NOTSUPP file");
    }

    /// An in-memory export `/srv/games` for the write tests: a handle is the path's bytes.
    /// Names steer it: `*nofh` CREATEs give no handle, LINK of `*nolink` is NOTSUPP (of a
    /// folder ISDIR), WRITEs to `*half` take half, COMMITs of `*reboot` give another verifier.
    #[derive(Default)]
    struct Disk {
        nodes: std::collections::BTreeMap<String, Option<Vec<u8>>>,
        /// Every WRITE's (offset, count asked).
        writes: Vec<(u64, usize)>,
    }

    impl Disk {
        fn answer(&mut self, prog: u32, proc: u32, r: &mut XdrReader, x: &mut Xdr) {
            let path = |r: &mut XdrReader| String::from_utf8(r.opaque(64).unwrap().to_vec());
            let dirop = |r: &mut XdrReader| {
                let dir = path(r).unwrap();
                format!(
                    "{dir}/{}",
                    std::str::from_utf8(r.opaque(255).unwrap()).unwrap()
                )
            };
            let ok_wcc = |x: &mut Xdr| {
                x.u32(0).bool(false).bool(false);
            };
            match (prog, proc) {
                (MOUNT_PROG, MOUNTPROC3_EXPORT) => {
                    x.bool(true).string("/srv/games").bool(false).bool(false);
                }
                (MOUNT_PROG, MOUNTPROC3_MNT) => {
                    self.nodes.entry("/srv/games".into()).or_insert(None);
                    x.u32(0).opaque(b"/srv/games").u32(0);
                }
                (NFS_PROG, NFSPROC3_GETATTR) => match self.nodes.get(&path(r).unwrap()) {
                    Some(None) => fattr(x.u32(0), NF3DIR, 4096, (5, 0)),
                    Some(Some(d)) => fattr(x.u32(0), NF3REG, d.len() as u64, (7, 9)),
                    None => {
                        x.u32(70);
                    }
                },
                (NFS_PROG, NFSPROC3_LOOKUP) => {
                    let p = dirop(r);
                    if self.nodes.contains_key(&p) {
                        x.u32(0).opaque(p.as_bytes()).bool(false).bool(false);
                    } else {
                        x.u32(2).bool(false);
                    }
                }
                (NFS_PROG, NFSPROC3_FSINFO) => {
                    x.u32(0).bool(false).u32(4).u32(4).u32(1).u32(4); // rtmax, wtmax 4
                    x.u32(4).u32(1).u32(4).u64(1 << 40).u32(0).u32(1).u32(0);
                }
                (NFS_PROG, NFSPROC3_READ) => {
                    let data = self.nodes[&path(r).unwrap()].clone().unwrap();
                    let off = (r.u64().unwrap() as usize).min(data.len());
                    let end = (off + r.u32().unwrap() as usize).min(data.len());
                    x.u32(0).bool(false).u32((end - off) as u32);
                    x.bool(end == data.len()).opaque(&data[off..end]);
                }
                (NFS_PROG, NFSPROC3_CREATE | NFSPROC3_MKDIR) => {
                    let p = dirop(r);
                    let parent = &p[..p.rfind('/').unwrap()];
                    if proc == NFSPROC3_CREATE {
                        assert_eq!(r.u32().unwrap(), GUARDED);
                    }
                    let mode = if r.bool().unwrap() {
                        r.u32().unwrap()
                    } else {
                        0
                    };
                    assert_eq!(
                        mode,
                        if proc == NFSPROC3_CREATE {
                            0o666
                        } else {
                            0o777
                        }
                    );
                    if self.nodes.contains_key(&p) {
                        x.u32(17).bool(false).bool(false);
                    } else if self.nodes.get(parent) != Some(&None) {
                        x.u32(2).bool(false).bool(false);
                    } else {
                        let file = proc == NFSPROC3_CREATE;
                        self.nodes.insert(p.clone(), file.then(Vec::new));
                        if p.ends_with("nofh") {
                            x.u32(0).bool(false);
                        } else {
                            x.u32(0).bool(true).opaque(p.as_bytes());
                        }
                        x.bool(false).bool(false).bool(false);
                    }
                }
                (NFS_PROG, NFSPROC3_WRITE) => {
                    let p = path(r).unwrap();
                    let (off, count) = (r.u64().unwrap(), r.u32().unwrap() as usize);
                    assert_eq!(r.u32().unwrap(), UNSTABLE);
                    let data = r.opaque(1 << 20).unwrap();
                    assert_eq!(data.len(), count);
                    self.writes.push((off, count));
                    let Some(Some(file)) = self.nodes.get_mut(&p) else {
                        x.u32(70).bool(false).bool(false);
                        return;
                    };
                    let took = if p.ends_with("half") {
                        count.div_ceil(2)
                    } else {
                        count
                    };
                    let end = off as usize + took;
                    if file.len() < end {
                        file.resize(end, 0);
                    }
                    file[off as usize..end].copy_from_slice(&data[..took]);
                    ok_wcc(x);
                    x.u32(took as u32).u32(UNSTABLE).fixed(b"verf0001");
                }
                (NFS_PROG, NFSPROC3_COMMIT) => {
                    let p = path(r).unwrap();
                    ok_wcc(x);
                    x.fixed(if p.ends_with("reboot") {
                        b"verf0002"
                    } else {
                        b"verf0001"
                    });
                }
                (NFS_PROG, NFSPROC3_REMOVE) => {
                    let p = dirop(r);
                    match self.nodes.get(&p) {
                        Some(None) => x.u32(21),
                        Some(Some(_)) => {
                            self.nodes.remove(&p);
                            x.u32(0)
                        }
                        None => x.u32(2),
                    };
                    x.bool(false).bool(false);
                }
                (NFS_PROG, NFSPROC3_LINK) => {
                    let file = path(r).unwrap();
                    let to = dirop(r);
                    match self.nodes.get(&file).cloned() {
                        None => x.u32(70),
                        Some(None) => x.u32(21),
                        Some(_) if file.ends_with("nolink") => x.u32(10004),
                        Some(_) if self.nodes.contains_key(&to) => x.u32(17),
                        Some(node) => {
                            self.nodes.insert(to, node);
                            x.u32(0)
                        }
                    };
                    x.bool(false).bool(false).bool(false);
                }
                (NFS_PROG, NFSPROC3_RENAME) => {
                    let (from, to) = (dirop(r), dirop(r));
                    let moved: Vec<String> = self
                        .nodes
                        .keys()
                        .filter(|k| **k == from || k.starts_with(&format!("{from}/")))
                        .cloned()
                        .collect();
                    if moved.is_empty() {
                        x.u32(2);
                    } else {
                        for k in moved {
                            let node = self.nodes.remove(&k).unwrap();
                            self.nodes.insert(format!("{to}{}", &k[from.len()..]), node);
                        }
                        ok_wcc(x);
                    }
                    x.bool(false).bool(false);
                }
                _ => {}
            }
        }
    }
}
