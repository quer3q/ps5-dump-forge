//! The peer side of reading in place (core's `remote::Remote`): what this device answers when a
//! paired device stats, lists or reads its files. Paths are relative to the share root, as the
//! transfer RPCs' are; the folder they sit in is resolved inside the root, the entry itself is
//! never followed: a link is reported as one, and special files (FIFOs, sockets, devices) and
//! names that aren't UTF-8 are listed marked (`special`, `bad_name`) and refused.
//!
//! Control RPCs: `stat {path}` → [`FsStat`], `fs_list {path}` → `{entries: [FsEntry]}` (an error
//! past `MAX_FS_LISTED`, never a cut list), `read_open {path}` → `{handle, size, mtime_ns}` (a
//! regular file), `read_close {handle}`. A handle belongs to the control session that opened it
//! and is closed with it. Reads go on a data lane joined under that session: `Request read
//! {handle, offset, len}` (`len` ≤ `CHUNK`), answered by one `Data` (head: job = handle, file 0,
//! the offset, BLAKE3 of the bytes; fewer bytes only where the file ends). Requests may be
//! pipelined; answers come in order. A failure is the lane's `Error {"transfer_failed: <why>"}`
//! and ends the lane.
//!
//! Writes (`Writes`): `mkdir {path}` → `{created}` (one level; an existing folder is fine),
//! `create {path}` → `{handle}` (exclusive, no link followed, 0777 on the PS5), `sync {handle}`,
//! `close {handle}`, `rename {from, to, replace?}` (never over another, unless `replace`: then
//! over a regular file only, never a folder or link; then both folders synced),
//! `remove {path}`, `free {path}` → `{free}` bytes. Only a file the session created is renamed
//! or removed; one it never renamed is kept `KEPT_FOR` once the session ends (removed at once
//! when the service stops), then removed. `reattach {path}` → `{handle, size, open}` moves such
//! a file (kept, or still held by another session of the same pairing) to the calling session,
//! so a writer whose session was lost resumes it; an older peer answers `unknown method`. A
//! path the `Busy` hook names (this device's unfinished jobs) is refused. Bytes go on a lane:
//! `Request write {handle, offset, len}` + its `Data` (BLAKE3), each answered `DataOk`;
//! pipelining ok.
//!
//! While a lane's read, write or put works the disk, the peer sends a `Ping` on the lane every
//! `PING_EVERY` (`pinging`), which the reader skips: a disk stall past `DEAD_AFTER` doesn't end
//! the lane. An older peer sends none.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ps5_dump_forge_core::DeleteError;
use ps5_dump_forge_core::remote::{self, FlinkPath, RemoteEntry, RemoteStat};
use ps5_dump_forge_flink::{
    CHUNK, Conn, DEAD_AFTER, DataHead, DataOk, Frame, Handshake, MAX_BODY, MAX_RPC_BODY,
    PING_EVERY, Request, Session, TRANSFER_FAILED, chunk_hash, client_join, data_frame, kind2,
    parse_data, send_refusal,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connect::{Inner, Rpc, join_checked};

/// Open read handles on this device, every peer and session together.
const MAX_READS: usize = 64;
/// ... and per control session, so one peer can't hold them all.
const MAX_READS_PER_SESSION: usize = MAX_READS / 2;
/// A lane's read of a handle its session no longer holds (closed, or the session ended).
const NO_OPEN_FILE: &str = "no open file";
/// A listing past this many entries is refused (the reply must fit one RPC body).
const MAX_FS_LISTED: usize = 100_000;
/// ... or past this many bytes of entries as JSON (room left for the `Response` around them).
const MAX_LIST_JSON: usize = MAX_RPC_BODY as usize - (64 << 10);

/// A file, folder or link as `lstat` sees it. A link's `dir`/`size` are false/0 (not followed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsStat {
    pub dir: bool,
    pub size: u64,
    /// Nanoseconds since the Unix epoch; `None` before it or where the filesystem keeps none.
    pub mtime_ns: Option<u64>,
    pub link: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    /// The name; lossy when `bad_name`.
    pub name: String,
    #[serde(flatten)]
    pub stat: FsStat,
    /// A FIFO, socket or device (listed as a file of size 0, never read).
    #[serde(default, skip_serializing_if = "is_false")]
    pub special: bool,
    /// A name that isn't UTF-8 here (a path can't name it).
    #[serde(default, skip_serializing_if = "is_false")]
    pub bad_name: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_false(b: &bool) -> bool {
    !b
}

fn mtime_ns(m: &fs::Metadata) -> Option<u64> {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
}

/// `None` for a special file.
fn describe(m: &fs::Metadata) -> Option<FsStat> {
    let t = m.file_type();
    let (dir, size, link) = if t.is_symlink() {
        (false, 0, true)
    } else if t.is_dir() {
        (true, 0, false)
    } else if t.is_file() {
        (false, m.len(), false)
    } else {
        return None;
    };
    Some(FsStat {
        dir,
        size,
        mtime_ns: mtime_ns(m),
        link,
    })
}

/// What paired peers may reach on this device: the share root (canonical), and on the PS5 (by
/// user request) within it only `/data` and the drives plugged in (`Roots::reach`), so a
/// peer lists `/` as `data` (and `mnt` while a drive is mounted) and is refused anything else.
pub struct Share {
    pub root: PathBuf,
    reach: Option<crate::paths::Roots>,
}

impl Share {
    pub(crate) fn new(root: PathBuf, reach: Option<crate::paths::Roots>) -> Share {
        Share { root, reach }
    }

    /// [`crate::paths::Roots::above`]: the names a listing of `dir` keeps (`None`: all).
    fn above(&self, dir: &Path) -> Option<Vec<std::ffi::OsString>> {
        self.reach.as_ref().and_then(|r| r.above(dir))
    }
}

/// The peer's root-relative `rel` (`""` = the root): its folder resolved (links and all)
/// inside the share root, its own name not followed; then the PS5's reach. Every RPC's path
/// comes through here. ponytail: a path check, no openat walk: a folder swapped for a link
/// between this check and the use is not caught.
fn locate(root: Option<&Share>, rel: &str) -> Result<PathBuf, String> {
    locate_as(root, rel, false)
}

/// [`locate`]; `browse` (stat, list) also lets through a folder above the PS5's roots.
fn locate_as(share: Option<&Share>, rel: &str, browse: bool) -> Result<PathBuf, String> {
    let share = share.ok_or("this device shares no folder")?;
    let root = share.root.as_path();
    let p = if rel.is_empty() {
        root.to_path_buf()
    } else {
        let full = join_checked(root, rel)?;
        let (parent, name) = match rel.rsplit_once('/') {
            Some((p, n)) => (join_checked(root, p)?, n),
            None => (root.to_path_buf(), rel),
        };
        let real = dunce::canonicalize(&parent).map_err(|e| io_text(&parent, &e))?;
        if !real.starts_with(root) {
            return Err(format!("{} is outside the shared folder", full.display()));
        }
        real.join(name)
    };
    match &share.reach {
        Some(reach) => reach.reach(&p, browse),
        None => Ok(p),
    }
}

/// Leads an RPC error for a path that isn't there, so the reader can tell `NotFound` apart.
pub const NOT_FOUND: &str = "not_found: ";
/// ... and for one that is there already (a create or rename never replaces it).
pub const EXISTS: &str = "exists: ";
/// ... and for a rename or remove that was done, its folders' sync failing after it.
pub const UNSYNCED: &str = "unsynced: ";

fn io_text(p: &Path, e: &io::Error) -> String {
    let lead = match e.kind() {
        io::ErrorKind::NotFound => NOT_FOUND,
        io::ErrorKind::AlreadyExists => EXISTS,
        _ => "",
    };
    format!("{lead}{}: {e}", p.display())
}

fn lstat(p: &Path) -> Result<(fs::Metadata, FsStat), String> {
    let m = fs::symlink_metadata(p).map_err(|e| io_text(p, &e))?;
    let s = describe(&m).ok_or_else(|| format!("{} is not a file or folder", p.display()))?;
    Ok((m, s))
}

pub fn stat(root: Option<&Share>, rel: &str) -> Result<FsStat, String> {
    Ok(lstat(&locate_as(root, rel, true)?)?.1)
}

/// A folder's entries by name; special files and names that aren't UTF-8 are marked, never left
/// out, so a reader refuses them as a local scan does.
pub fn list(root: Option<&Share>, rel: &str) -> Result<Vec<FsEntry>, String> {
    let dir = locate_as(root, rel, true)?;
    if !lstat(&dir)?.1.dir {
        return Err(format!("{} is not a folder", dir.display()));
    }
    // Above the PS5's roots: only the way to them, a linked `/data` as the folder it is.
    let keep = root.and_then(|s| s.above(&dir));
    let at = |e: io::Error| format!("{}: {e}", dir.display());
    let (mut entries, mut bytes) = (Vec::new(), 0);
    for e in fs::read_dir(&dir).map_err(at)? {
        let e = e.map_err(at)?;
        let (name, bad_name) = match e.file_name().into_string() {
            Ok(name) => (name, false),
            Err(raw) => (raw.to_string_lossy().into_owned(), true),
        };
        let m = match &keep {
            Some(keep) if !keep.iter().any(|k| *k == e.file_name()) => continue,
            Some(_) => fs::metadata(e.path()),
            None => fs::symlink_metadata(e.path()),
        };
        // Gone since `read_dir` listed it: left out; any other failure fails the listing.
        let m = match m {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}/{name}: {e}", dir.display())),
        };
        let special = describe(&m).is_none();
        let stat = describe(&m).unwrap_or(FsStat {
            dir: false,
            size: 0,
            mtime_ns: mtime_ns(&m),
            link: false,
        });
        let entry = FsEntry {
            name,
            stat,
            special,
            bad_name,
        };
        bytes += serde_json::to_vec(&entry).map_or(usize::MAX / 2, |j| j.len() + 1);
        if entries.len() >= MAX_FS_LISTED || bytes > MAX_LIST_JSON {
            return Err(format!(
                "{} holds too many entries to list (more than {MAX_FS_LISTED})",
                dir.display()
            ));
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Opens a regular file for reading: the handle must be the entry `lstat` saw (so a link swapped
/// in after the check is refused). ponytail: off unix only `is_file` is re-checked on the handle.
fn open_file(p: &Path) -> Result<(fs::File, FsStat), String> {
    let (seen, stat) = lstat(p)?;
    if stat.dir || stat.link {
        return Err(format!("{} is not a file", p.display()));
    }
    let at = |e: io::Error| format!("{}: {e}", p.display());
    // Non-blocking: a FIFO swapped in after the check can't stall the open (no effect on a file).
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(p)
            .map_err(at)?
    };
    #[cfg(not(unix))]
    let file = fs::File::open(p).map_err(at)?;
    let got = file.metadata().map_err(at)?;
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        (got.dev(), got.ino()) == (seen.dev(), seen.ino())
    };
    #[cfg(not(unix))]
    let same = {
        let _ = &seen;
        true
    };
    if !got.is_file() || !same {
        return Err(format!("{} changed while it was opened", p.display()));
    }
    Ok((file, stat))
}

/// Up to `len` bytes at `at`; fewer only where the file ends.
fn read_at(f: &fs::File, mut at: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0; len];
    let mut n = 0;
    while n < len {
        #[cfg(unix)]
        let got = std::os::unix::fs::FileExt::read_at(f, &mut buf[n..], at);
        #[cfg(windows)]
        let got = std::os::windows::fs::FileExt::seek_read(f, &mut buf[n..], at);
        match got {
            Ok(0) => break,
            Ok(k) => {
                n += k;
                at += k as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    buf.truncate(n);
    Ok(buf)
}

struct Open {
    session: Session,
    file: Arc<fs::File>,
}

/// The read handles peers hold open on this device.
#[derive(Default)]
pub struct Reads {
    open: Mutex<HashMap<u64, Open>>,
    next: AtomicU64,
    /// The control sessions up now, by id: a lane joins one of them (reads by path hold no
    /// handle to find their session by).
    live: Mutex<HashMap<String, Session>>,
    /// Reads by path under way, by session: they count against the open-file caps.
    by_path: Mutex<HashMap<String, usize>>,
    /// Tests: a disk stall before each lane read.
    #[cfg(test)]
    stall: Mutex<Duration>,
}

/// One read by path counted in `Reads::by_path` until dropped.
struct PathRead<'a>(&'a Reads, String);

impl Drop for PathRead<'_> {
    fn drop(&mut self) {
        let mut busy = lock(&self.0.by_path);
        if let Some(n) = busy.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                busy.remove(&self.1);
            }
        }
    }
}

/// Closes a control session's handles when it ends (`Reads::guard`).
pub struct SessionReads<'a>(&'a Reads, String);

impl Drop for SessionReads<'_> {
    fn drop(&mut self) {
        lock(&self.0.open).retain(|_, o| o.session.id() != self.1);
        lock(&self.0.live).remove(&self.1);
    }
}

impl Reads {
    /// Whether a peer holds a file open for reading here, or reads one by path now.
    pub fn active(&self) -> bool {
        !lock(&self.open).is_empty() || !lock(&self.by_path).is_empty()
    }

    /// Whether the control session `sid` is up.
    pub fn is_live(&self, sid: &str) -> bool {
        lock(&self.live).contains_key(sid)
    }

    pub fn guard(&self, session: &Session) -> SessionReads<'_> {
        lock(&self.live).insert(session.id(), session.clone());
        SessionReads(self, session.id())
    }

    /// The control session with this id (up now, or holding a handle), for a lane's `Join`.
    pub fn session_of(&self, id: &str) -> Option<Session> {
        if let Some(s) = lock(&self.live).get(id) {
            return Some(s.clone());
        }
        let open = lock(&self.open);
        let o = open.values().find(|o| o.session.id() == id)?;
        Some(o.session.clone())
    }

    fn file(&self, sid: &str, handle: u64) -> Option<Arc<fs::File>> {
        match lock(&self.open).get(&handle) {
            Some(o) if o.session.id() == sid => Some(o.file.clone()),
            _ => None,
        }
    }

    /// A slot for one read by path of session `sid`: refused once the session has ended (its
    /// lanes outlive it a moment) or past the caps `open` keeps.
    fn path_read(&self, sid: &str) -> Result<PathRead<'_>, String> {
        if !self.is_live(sid) {
            return Err(format!("{NO_OPEN_FILE} (the session ended)"));
        }
        let open = lock(&self.open);
        let mut busy = lock(&self.by_path);
        let mine = open.values().filter(|o| o.session.id() == sid).count()
            + busy.get(sid).copied().unwrap_or(0);
        let all = open.len() + busy.values().sum::<usize>();
        if all >= MAX_READS || mine >= MAX_READS_PER_SESSION {
            let n = if mine >= MAX_READS_PER_SESSION {
                mine
            } else {
                all
            };
            return Err(format!(
                "{n} files are open for reading already; try again later"
            ));
        }
        *busy.entry(sid.to_string()).or_default() += 1;
        Ok(PathRead(self, sid.to_string()))
    }

    fn open(&self, hs: &Handshake, root: Option<&Share>, rel: &str) -> Result<Value, String> {
        let (file, stat) = open_file(&locate(root, rel)?)?;
        let mut open = lock(&self.open);
        let sid = hs.session.id();
        // Reads by path in flight count too, as `path_read` counts them (same lock order).
        let busy = lock(&self.by_path);
        let mine = open.values().filter(|o| o.session.id() == sid).count()
            + busy.get(&sid).copied().unwrap_or(0);
        let all = open.len() + busy.values().sum::<usize>();
        drop(busy);
        if all >= MAX_READS || mine >= MAX_READS_PER_SESSION {
            return Err(format!(
                "{} files are open for reading already; try again later",
                if mine >= MAX_READS_PER_SESSION {
                    mine
                } else {
                    all
                }
            ));
        }
        let handle = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let o = Open {
            session: hs.session.clone(),
            file: Arc::new(file),
        };
        open.insert(handle, o);
        Ok(serde_json::json!({ "handle": handle, "size": stat.size, "mtime_ns": stat.mtime_ns }))
    }

    fn close(&self, hs: &Handshake, handle: u64) -> Result<(), String> {
        let mut open = lock(&self.open);
        match open.get(&handle) {
            Some(o) if o.session.id() == hs.session.id() => {
                open.remove(&handle);
                Ok(())
            }
            _ => Err(format!("{NO_OPEN_FILE} {handle}")),
        }
    }

    /// The `read` requests of a lane (the first already received), one `Data` each.
    pub fn serve(
        &self,
        conn: &mut Conn,
        sid: &str,
        root: Option<&Share>,
        first: Frame,
        allowed: &dyn Fn() -> bool,
    ) -> io::Result<()> {
        let mut f = first;
        loop {
            if !allowed() {
                return Err(send_refusal(conn, ps5_dump_forge_flink::NOT_PAIRED));
            }
            match pinging(conn, || self.read(sid, root, &f)) {
                Ok((head, bytes)) => conn.send_frame(&data_frame(&head, &bytes)?)?,
                Err(why) => return Err(send_refusal(conn, &format!("{TRANSFER_FAILED}: {why}"))),
            }
            f = recv_kind(conn, kind2::REQUEST, "read")?;
        }
    }

    /// A `read {handle, offset, len}` of an open file, or `read {path, offset, len}`: the file
    /// opened (as `read_open` opens it), read, closed, and its stat after the read in the head.
    fn read(
        &self,
        sid: &str,
        root: Option<&Share>,
        f: &Frame,
    ) -> Result<(DataHead, Vec<u8>), String> {
        #[cfg(test)]
        std::thread::sleep(*lock(&self.stall));
        let req = serde_json::from_slice::<Request>(&f.body).map_err(|_| "bad read")?;
        if let Some(rel) = req.body["path"].as_str() {
            let (offset, len) = (req.body["offset"].as_u64(), req.body["len"].as_u64());
            let (Some(offset), Some(len)) = (offset, len) else {
                return Err("read needs path, offset and len".into());
            };
            if req.method != "read" || len > CHUNK {
                return Err(format!("bad read of {len} bytes"));
            }
            let _slot = self.path_read(sid)?;
            let (file, _) = open_file(&locate(root, rel)?)?;
            let bytes = read_at(&file, offset, len as usize).map_err(|e| e.to_string())?;
            drop(file);
            // As the `stat` RPC: the path resolved again, inside the share root.
            let after = stat(root, rel)?;
            let head = DataHead {
                job: 0,
                file: 0,
                offset,
                hash: chunk_hash(&bytes),
                stat: Some(serde_json::json!(after)),
            };
            return Ok((head, bytes));
        }
        let (handle, offset, len) = (
            req.body["handle"].as_u64(),
            req.body["offset"].as_u64(),
            req.body["len"].as_u64(),
        );
        let (Some(handle), Some(offset), Some(len)) = (handle, offset, len) else {
            return Err("read needs handle, offset and len".into());
        };
        if req.method != "read" || len > CHUNK {
            return Err(format!("bad read of {len} bytes"));
        }
        let file = self
            .file(sid, handle)
            .ok_or_else(|| format!("{NO_OPEN_FILE} {handle}"))?;
        let bytes = read_at(&file, offset, len as usize).map_err(|e| e.to_string())?;
        let head = DataHead {
            job: handle,
            file: 0,
            offset,
            hash: chunk_hash(&bytes),
            stat: None,
        };
        Ok((head, bytes))
    }
}

/// Whether a lane's first frame is a `Request` of `method`.
fn is_method(f: &Frame, method: &str) -> bool {
    f.kind == kind2::REQUEST
        && serde_json::from_slice::<Request>(&f.body).is_ok_and(|r| r.method == method)
}

/// Whether a lane's first frame asks for reads (else it carries writes).
pub fn is_read(f: &Frame) -> bool {
    is_method(f, "read")
}

/// Whether a lane's first frame carries writes.
pub fn is_write(f: &Frame) -> bool {
    is_method(f, "write") || is_method(f, "put")
}

/// The next frame of `kind` on a lane, pings answered on the way and pongs skipped (a reader
/// before `pinging` answered this side's pings).
fn recv_kind(conn: &mut Conn, kind: u8, lane: &str) -> io::Result<Frame> {
    loop {
        let f = conn.recv_frame()?;
        match f.kind {
            kind2::PING => conn.send_frame(&Frame::new(kind2::PONG, 0, Vec::new()))?,
            kind2::PONG => {}
            k if k == kind => return Ok(f),
            k => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("flink: unexpected frame {k} on a {lane} lane"),
                ));
            }
        }
    }
}

/// A lane's disk work (a chunk read or write, a put) on its own thread while this one sends a
/// `Ping` every `PING_EVERY`, as the control conn does while an RPC runs: the other side's lane
/// reader skips them, so a disk stall longer than `DEAD_AFTER` doesn't read there as a dead
/// peer. Only this thread writes to `conn`, so frames leave in the order their MACs counted. A
/// failed ping stops the pings; the answer's send then reports the broken lane.
fn pinging<T: Send>(conn: &mut Conn, work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| {
        let (done, finished) = std::sync::mpsc::channel::<()>();
        let worker = s.spawn(move || {
            let out = work();
            drop(done);
            out
        });
        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
            finished.recv_timeout(PING_EVERY)
        {
            if conn
                .send_frame(&Frame::new(kind2::PING, 0, Vec::new()))
                .is_err()
            {
                break;
            }
        }
        worker
            .join()
            .unwrap_or_else(|p| std::panic::resume_unwind(p))
    })
}

/// The RPCs above and the writes below; `None` for any other method. `me`: this device's name.
pub fn answer(
    root: Option<&Share>,
    reads: &Reads,
    writes: &Writes,
    me: &str,
    hs: &Handshake,
    r: &Request,
) -> Option<Result<Value, String>> {
    let text = |k: &str| match r.body.get(k) {
        Some(Value::String(s)) => Ok(s.as_str()),
        _ => Err(format!("{} needs a {k:?} string", r.method)),
    };
    let path = || text("path");
    let handle =
        || (r.body["handle"].as_u64()).ok_or_else(|| format!("{} needs a handle", r.method));
    let sid = hs.session.id();
    Some(match r.method.as_str() {
        "stat" => path()
            .and_then(|p| stat(root, p))
            .map(|s| serde_json::json!(s)),
        "fs_list" => path()
            .and_then(|p| list(root, p))
            .map(|entries| serde_json::json!({ "entries": entries })),
        "read_open" => path().and_then(|p| reads.open(hs, root, p)),
        "read_close" => handle().and_then(|h| reads.close(hs, h).map(|()| Value::Null)),
        "mkdir" => path().and_then(|p| writes.mkdir(root, p, me)),
        "create" => path().and_then(|p| writes.create(hs, root, p, me)),
        "sync" => handle().and_then(|h| writes.sync(&sid, h).map(|()| Value::Null)),
        "close" => handle().and_then(|h| writes.close(&sid, h).map(|()| Value::Null)),
        "rename" => text("from").and_then(|from| {
            let to = text("to")?;
            let replace = r.body["replace"].as_bool().unwrap_or(false);
            writes
                .rename(&sid, root, from, to, replace, me)
                .map(|()| Value::Null)
        }),
        "remove" => path().and_then(|p| writes.remove(&sid, root, p, me).map(|()| Value::Null)),
        "free" => path().and_then(|p| free(root, p)),
        "delete_path" => path().and_then(|p| writes.delete_path(root, p).map(|()| Value::Null)),
        "move_path" => text("from").and_then(|from| {
            let to = text("to")?;
            let replace = r.body["replace"].as_bool().unwrap_or(false);
            (writes.move_path(root, from, to, replace)).map(|()| Value::Null)
        }),
        #[cfg(test)]
        "reattach" if writes.old.load(Ordering::SeqCst) => return None,
        "reattach" => path().and_then(|p| writes.reattach(hs, root, p, me)),
        _ => return None,
    })
}

// ---- Writes: a paired device writing files here -----------------------------------------------

/// Files peers may have created and not yet renamed, on the device and per session.
const MAX_WRITES: usize = 64;
const MAX_WRITES_PER_SESSION: usize = MAX_WRITES / 2;

/// Whether a path is at, in or above one of this device's unfinished jobs' paths.
pub type Busy = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// Why a file must not be put in place of a path here (core's `Jobs::replace_check`: what
/// Delete refuses, an undo journal's image, this device's jobs' paths; on the PS5 mounted
/// images): a peer's replacing rename is refused with it.
pub type ReplaceCheck = Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;

/// A paired device's Delete here: this device's own `Jobs::delete_path` with its protected
/// paths (the server's keep list, the app's home folder).
pub type DeleteHook = Arc<dyn Fn(&Path) -> Result<(), DeleteError> + Send + Sync>;
/// ... and its Move to (`from`, `to`, `replace`): `Jobs::move_path`.
pub type MoveHook = Arc<dyn Fn(&Path, &Path, bool) -> Result<(), DeleteError> + Send + Sync>;

/// Leads the RPC error of a refused delete or move, so the caller keeps the kind (a missing
/// path leads with [`NOT_FOUND`]; any other failure has no lead).
pub const REFUSED: &str = "refused: ";
/// ... and of one a job, or a peer's write, holds off.
pub const BUSY: &str = "busy: ";

/// How long a file whose session ended unrenamed is kept for a `reattach` (the writer's link
/// broke and it reconnects), then removed.
pub const KEPT_FOR: Duration = Duration::from_secs(60);

/// A file a session created: open until `close`, kept `KEPT_FOR` after the session ends
/// unless renamed, then removed.
struct Created {
    session: Session,
    path: PathBuf,
    file: Option<Arc<fs::File>>,
    /// Held through each lane write (`write_owned`) and by `reattach`: a write of the old
    /// session under way ends before the file changes hands, and a later one is refused.
    gate: Arc<Mutex<()>>,
    /// Set when its session ended: removed at that instant unless reattached first.
    kept_until: Option<Instant>,
}

/// The files peers write on this device. Only a file a session created itself (exclusively)
/// is written, renamed (never over another) or removed through it.
#[derive(Default)]
pub struct Writes {
    files: Mutex<HashMap<u64, Created>>,
    next: AtomicU64,
    busy: Mutex<Option<Busy>>,
    /// Asked before a replacing rename (unset: nothing beyond the checks here).
    replace_check: Mutex<Option<ReplaceCheck>>,
    /// A peer's Delete and Move to here (unset: refused).
    delete_move: Mutex<Option<(DeleteHook, MoveHook)>>,
    /// The control sessions up now (`guard`), changed and checked under `files`' lock: a put
    /// on a lane that outlives its session creates nothing its sweep would miss.
    live: Mutex<HashSet<String>>,
    /// The service stops: a session that ends removes its files at once (`stop`).
    stopping: std::sync::atomic::AtomicBool,
    /// Pairings forgotten here (by secret): their files are never resumed, so they go at once.
    forgotten: Mutex<HashSet<[u8; 32]>>,
    /// Tests: a disk stall in each lane write or put, its file's gate held.
    #[cfg(test)]
    stall: Mutex<Duration>,
    /// Tests: `KEPT_FOR` shortened.
    #[cfg(test)]
    pub(crate) kept_for: Mutex<Option<Duration>>,
    /// Tests: answers as a peer from before `reattach`.
    #[cfg(test)]
    pub(crate) old: std::sync::atomic::AtomicBool,
}

/// Keeps what a control session created and never renamed for `KEPT_FOR` when it ends (the
/// writer's link broke: it may `reattach`), then `sweep` removes it; at once while stopping or
/// once the pairing is forgotten here.
pub struct SessionWrites<'a>(&'a Writes, String);

impl Drop for SessionWrites<'_> {
    fn drop(&mut self) {
        let mut files = lock(&self.0.files);
        lock(&self.0.live).remove(&self.1);
        let until = Instant::now() + self.0.kept_for();
        for c in files.values_mut() {
            if c.session.id() == self.1 {
                c.kept_until = Some(until);
            }
        }
        drop(files);
        self.0.sweep();
    }
}

/// A [`DeleteError`] as an RPC error, its kind in the lead.
fn delete_text(e: DeleteError) -> String {
    match e {
        DeleteError::Refused(m) => format!("{REFUSED}{m}"),
        DeleteError::Missing(m) => format!("{NOT_FOUND}{m}"),
        DeleteError::Busy(m) => format!("{BUSY}{m}"),
        DeleteError::Failed(m) => m,
    }
}

/// The peer's answer to a delete or move, its kind back from the lead.
fn deleted(e: io::Error) -> DeleteError {
    let text = e.to_string();
    if e.kind() == io::ErrorKind::NotFound {
        return DeleteError::Missing(text);
    }
    if let Some(m) = text.strip_prefix(REFUSED) {
        return DeleteError::Refused(m.to_string());
    }
    if let Some(m) = text.strip_prefix(BUSY) {
        return DeleteError::Busy(m.to_string());
    }
    if text.starts_with("unknown method") {
        return DeleteError::Refused(
            "that device's Dump Forge is older: update it to delete or move its files from here"
                .to_string(),
        );
    }
    DeleteError::Failed(text)
}

fn write_all_at(f: &fs::File, bytes: &[u8], at: u64) -> io::Result<()> {
    #[cfg(unix)]
    return std::os::unix::fs::FileExt::write_all_at(f, bytes, at);
    #[cfg(windows)]
    {
        let mut n = 0;
        while n < bytes.len() {
            match std::os::windows::fs::FileExt::seek_write(f, &bytes[n..], at + n as u64) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(k) => n += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// `{free}`: the bytes free on the filesystem of the folder `rel`.
fn free(root: Option<&Share>, rel: &str) -> Result<Value, String> {
    let p = locate(root, rel)?;
    // `statfs` follows the folder's own name: on the PS5 where it leads must be reachable too.
    if let Some(reach) = root.and_then(|s| s.reach.as_ref()) {
        let real = dunce::canonicalize(&p).map_err(|e| io_text(&p, &e))?;
        reach.reach(&real, false)?;
    }
    let free = ps5_dump_forge_core::out::free_space(&p).map_err(|e| io_text(&p, &e))?;
    Ok(serde_json::json!({ "free": free }))
}

/// Syncs the folders `paths` sit in.
fn sync_parents(paths: &[&Path]) -> Result<(), String> {
    let mut done: Vec<&Path> = Vec::new();
    for up in paths.iter().filter_map(|p| p.parent()) {
        if !done.contains(&up) {
            ps5_dump_forge_core::out::sync_dir(up).map_err(|e| io_text(up, &e))?;
            done.push(up);
        }
    }
    Ok(())
}

impl Writes {
    pub fn guard(&self, session: &Session) -> SessionWrites<'_> {
        let files = lock(&self.files);
        lock(&self.live).insert(session.id());
        drop(files);
        SessionWrites(self, session.id())
    }

    /// The control session that created a file with this id, for a lane's `Join`.
    pub fn session_of(&self, id: &str) -> Option<Session> {
        let files = lock(&self.files);
        let c = files.values().find(|c| c.session.id() == id)?;
        Some(c.session.clone())
    }

    pub fn set_busy(&self, busy: Busy) {
        *lock(&self.busy) = Some(busy);
    }

    pub fn set_replace_check(&self, check: ReplaceCheck) {
        *lock(&self.replace_check) = Some(check);
    }

    pub fn set_delete_move(&self, delete: DeleteHook, move_: MoveHook) {
        *lock(&self.delete_move) = Some((delete, move_));
    }

    /// A peer's Delete of `rel` (never the shared folder itself), confined as every RPC's path
    /// is, then judged as this device's own Delete judges it.
    fn delete_path(&self, root: Option<&Share>, rel: &str) -> Result<(), String> {
        let (delete, _) = self.delete_move()?;
        let p = Self::own(root, rel)?;
        delete(&p).map_err(delete_text)
    }

    /// A peer's Move to: both paths confined, then this device's own Move to.
    fn move_path(
        &self,
        root: Option<&Share>,
        from: &str,
        to: &str,
        replace: bool,
    ) -> Result<(), String> {
        let (_, move_) = self.delete_move()?;
        let (from, to) = (Self::own(root, from)?, Self::own(root, to)?);
        move_(&from, &to, replace).map_err(delete_text)
    }

    fn delete_move(&self) -> Result<(DeleteHook, MoveHook), String> {
        let hooks = lock(&self.delete_move).clone();
        hooks.ok_or_else(|| {
            format!("{REFUSED}this device doesn't delete or move files for a paired device")
        })
    }

    /// `rel` [`locate`]d, refused for the share root itself.
    fn own(root: Option<&Share>, rel: &str) -> Result<PathBuf, String> {
        if rel.is_empty() {
            return Err(format!(
                "{REFUSED}the shared folder itself is not deleted or moved"
            ));
        }
        locate(root, rel).map_err(|e| match e.starts_with(NOT_FOUND) {
            true => e,
            false => format!("{REFUSED}{e}"),
        })
    }

    /// Whether a peer has a file here it created and hasn't renamed yet: one being written, or
    /// one kept for its writer to reattach.
    pub fn active(&self) -> bool {
        !lock(&self.files).is_empty()
    }

    /// Whether `p` is at, in or above a file a peer is writing here (created, not yet
    /// renamed): this device's jobs refuse such a path when they start.
    pub fn writing(&self, p: &Path) -> bool {
        let files: Vec<PathBuf> = lock(&self.files).values().map(|c| c.path.clone()).collect();
        ps5_dump_forge_core::path_in_use(p, &files)
    }

    /// Refuses a path one of this device's unfinished jobs uses, and (PS5) one in
    /// ShadowMount+'s mount folder.
    fn check(&self, p: &Path, me: &str) -> Result<(), String> {
        if let Some(why) = ps5_dump_forge_core::out::refusal(p) {
            return Err(why);
        }
        let busy = lock(&self.busy).clone();
        match busy {
            Some(busy) if busy(p) => Err(format!("{} is in use by a job on {me}", p.display())),
            _ => Ok(()),
        }
    }

    /// One folder (its parent must exist); `{created: false}` when it is a folder already.
    fn mkdir(&self, root: Option<&Share>, rel: &str, me: &str) -> Result<Value, String> {
        let p = locate(root, rel)?;
        self.check(&p, me)?;
        match ps5_dump_forge_core::out::make_dir(&p) {
            Ok(()) => {
                sync_parents(&[&p])?;
                Ok(serde_json::json!({ "created": true }))
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && lstat(&p)?.1.dir => {
                Ok(serde_json::json!({ "created": false }))
            }
            Err(e) => Err(io_text(&p, &e)),
        }
    }

    /// `{handle}` of a new file at `rel` (never one that exists; a link there not followed).
    fn create(
        &self,
        hs: &Handshake,
        root: Option<&Share>,
        rel: &str,
        me: &str,
    ) -> Result<Value, String> {
        let p = locate(root, rel)?;
        self.check(&p, me)?;
        let handle = self.create_at(&hs.session, p)?;
        Ok(serde_json::json!({ "handle": handle }))
    }

    /// A new file at `p` (checked) for `session`, within the write caps.
    fn create_at(&self, session: &Session, p: PathBuf) -> Result<u64, String> {
        let mut files = lock(&self.files);
        let sid = session.id();
        if !lock(&self.live).contains(&sid) {
            return Err(format!("{NO_OPEN_FILE} (the session ended)"));
        }
        let mine = files.values().filter(|c| c.session.id() == sid).count();
        if files.len() >= MAX_WRITES || mine >= MAX_WRITES_PER_SESSION {
            return Err(format!(
                "{} files are being written already; try again later",
                mine.max(files.len())
            ));
        }
        let file = ps5_dump_forge_core::out::create_new(&p).map_err(|e| io_text(&p, &e))?;
        let handle = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let c = Created {
            session: session.clone(),
            path: p,
            file: Some(Arc::new(file)),
            gate: Arc::default(),
            kept_until: None,
        };
        files.insert(handle, c);
        Ok(handle)
    }

    fn file(&self, sid: &str, handle: u64) -> Result<Arc<fs::File>, String> {
        match lock(&self.files).get(&handle) {
            Some(Created {
                session,
                file: Some(f),
                kept_until: None,
                ..
            }) if session.id() == sid => Ok(f.clone()),
            _ => Err(format!("{NO_OPEN_FILE} {handle}")),
        }
    }

    fn kept_for(&self) -> Duration {
        #[cfg(test)]
        if let Some(d) = *lock(&self.kept_for) {
            return d;
        }
        KEPT_FOR
    }

    /// The service stops: every kept file is removed now, and those of sessions ending later
    /// as they end.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.sweep();
    }

    /// The pairing with `secret` is forgotten here: its kept files go now, and those of its
    /// sessions ending later as they end.
    pub fn forget(&self, secret: [u8; 32]) {
        lock(&self.forgotten).insert(secret);
        self.sweep();
    }

    /// Removes the kept files past their `KEPT_FOR` (run every few seconds), and all of them
    /// while stopping or of a forgotten pairing.
    pub fn sweep(&self) {
        let now = Instant::now();
        let stopping = self.stopping.load(Ordering::SeqCst);
        let forgotten = lock(&self.forgotten).clone();
        let mut gone = Vec::new();
        lock(&self.files).retain(|_, c| {
            let go = c.kept_until.is_some_and(|until| {
                until <= now || stopping || forgotten.contains(&c.session.secret)
            });
            if go {
                gone.push(c.path.clone());
            }
            !go
        });
        // ponytail: by path: a file put in its place by hand meanwhile would go too.
        for p in &gone {
            let _ = fs::remove_file(p);
        }
        // Synced, so the freed room shows (exFAT on the PS5), as a local `.part`'s cleanup does.
        let _ = sync_parents(&gone.iter().map(PathBuf::as_path).collect::<Vec<_>>());
    }

    /// `{handle, size, open}` of the unrenamed file at `rel`, moved to this session: one kept
    /// after its session ended, or one another session of the same pairing still holds (its
    /// end not seen here yet; it can't use the file after this). Only the device that created
    /// it (the pairing's secret) takes it over; the handle stays the same.
    fn reattach(
        &self,
        hs: &Handshake,
        root: Option<&Share>,
        rel: &str,
        me: &str,
    ) -> Result<Value, String> {
        let p = locate(root, rel)?;
        self.check(&p, me)?;
        self.reattach_at(&hs.session, &p)
    }

    /// `reattach` of the file at `p` (located, checked) for `session`.
    fn reattach_at(&self, session: &Session, p: &Path) -> Result<Value, String> {
        let sid = session.id();
        let not_kept = || format!("{} is not an unfinished file to resume", p.display());
        let find = |files: &HashMap<u64, Created>| {
            (files.iter())
                .find(|(_, c)| c.path == p && c.session.secret == session.secret)
                .map(|(h, c)| (*h, c.gate.clone()))
        };
        // A write of the old session under way ends first (it holds the gate): none of its
        // writes lands once the file is this session's.
        let (handle, gate) = find(&lock(&self.files)).ok_or_else(not_kept)?;
        let _held = lock(&gate);
        let mut files = lock(&self.files);
        if !lock(&self.live).contains(&sid) {
            return Err(format!("{NO_OPEN_FILE} (the session ended)"));
        }
        if find(&files).map(|f| f.0) != Some(handle) {
            return Err(not_kept());
        }
        let mine = files.values().filter(|c| c.session.id() == sid).count();
        if files[&handle].session.id() != sid && mine >= MAX_WRITES_PER_SESSION {
            return Err(format!(
                "{mine} files are being written already; try again later"
            ));
        }
        let c = files.get_mut(&handle).ok_or_else(not_kept)?;
        let size = match &c.file {
            Some(f) => f.metadata(),
            None => fs::symlink_metadata(p),
        }
        .map_err(|e| io_text(p, &e))?
        .len();
        c.session = session.clone();
        c.kept_until = None;
        Ok(serde_json::json!({ "handle": handle, "size": size, "open": c.file.is_some() }))
    }

    fn sync(&self, sid: &str, handle: u64) -> Result<(), String> {
        let f = self.file(sid, handle)?;
        ps5_dump_forge_core::out::sync_file(&f).map_err(|e| format!("sync: {e}"))
    }

    /// Closes the handle; the file stays this session's until it is renamed or removed. A
    /// closed one closes again fine (a close whose answer was lost, asked again after a
    /// `reattach`).
    fn close(&self, sid: &str, handle: u64) -> Result<(), String> {
        match lock(&self.files).get_mut(&handle) {
            Some(c) if c.session.id() == sid && c.kept_until.is_none() => {
                c.file = None;
                Ok(())
            }
            _ => Err(format!("{NO_OPEN_FILE} {handle}")),
        }
    }

    /// The handle of the file at `p` this session created.
    fn created(files: &HashMap<u64, Created>, sid: &str, p: &Path) -> Result<u64, String> {
        (files.iter())
            .find(|(_, c)| c.session.id() == sid && c.path == p)
            .map(|(h, _)| *h)
            .ok_or_else(|| format!("{} is not a file this session created", p.display()))
    }

    /// Renames a file this session created to `to`, never over another unless `replace` (a
    /// job the user confirmed replacing): then over a regular file only, never a folder, a
    /// link, a special file, a file a peer is writing or what the replace check (core's
    /// `Jobs::replace_check`) refuses (the check passing, `to` must still be what it was); then it is
    /// the device's (no longer removed with the session) and both folders are synced.
    fn rename(
        &self,
        sid: &str,
        root: Option<&Share>,
        from: &str,
        to: &str,
        replace: bool,
        me: &str,
    ) -> Result<(), String> {
        let (src, dst) = (locate(root, from)?, locate(root, to)?);
        self.check(&src, me)?;
        self.check(&dst, me)?;
        if replace {
            // Asked before `files` is locked: core's check asks `writing` (that lock) too.
            let check = lock(&self.replace_check).clone();
            if let Some(check) = check {
                check(&dst)?;
            }
        }
        let mut files = lock(&self.files);
        let handle = Self::created(&files, sid, &src)?;
        if files[&handle].file.is_some() {
            return Err(format!("{} is still open; close it first", src.display()));
        }
        if !replace {
            ps5_dump_forge_core::rename_no_replace(&src, &dst).map_err(|e| io_text(&dst, &e))?;
        } else {
            // Another file a peer is writing (open, closed and not renamed, or kept for a
            // reattach) is never replaced: its writer would rename these bytes as its own.
            // Checked under `files`' lock, so none is created there before the rename.
            let others: Vec<PathBuf> = (files.iter())
                .filter(|(h, _)| **h != handle)
                .map(|(_, c)| c.path.clone())
                .collect();
            if ps5_dump_forge_core::path_in_use(&dst, &others) {
                return Err(format!(
                    "{} is being written by a paired device: nothing is put in its place",
                    dst.display()
                ));
            }
            match fs::symlink_metadata(&dst) {
                Ok(m) if m.file_type().is_file() => {}
                Ok(_) => {
                    return Err(format!(
                        "{} is not a file: nothing is put in its place",
                        dst.display()
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_text(&dst, &e)),
            }
            // ponytail: a folder put at `to` after the check fails the rename (a file never
            // goes over a folder); a link put there is replaced itself, never followed.
            fs::rename(&src, &dst).map_err(|e| io_text(&dst, &e))?;
        }
        files.remove(&handle);
        drop(files);
        sync_parents(&[&src, &dst]).map_err(|e| format!("{UNSYNCED}{e}"))
    }

    /// Removes a file this session created and hasn't renamed (its handle closed first); it
    /// stays this session's, removed when the session ends, if the removal fails.
    fn remove(&self, sid: &str, root: Option<&Share>, rel: &str, me: &str) -> Result<(), String> {
        let p = locate(root, rel)?;
        self.check(&p, me)?;
        let mut files = lock(&self.files);
        let handle = Self::created(&files, sid, &p)?;
        if let Some(c) = files.get_mut(&handle) {
            c.file = None;
        }
        match fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_text(&p, &e)),
        }
        files.remove(&handle);
        drop(files);
        sync_parents(&[&p]).map_err(|e| format!("{UNSYNCED}{e}"))
    }

    /// The `write`s and `put`s of a lane (the first request already received): each a
    /// `Request write {handle, offset, len}` or `put {tmp, path, len}` and then its `Data`
    /// (head: job = handle or 0, file 0, the offset, BLAKE3), answered `DataOk` once written.
    /// A failure is the lane's `Error` and ends it.
    pub fn serve(
        &self,
        conn: &mut Conn,
        session: &Session,
        root: Option<&Share>,
        me: &str,
        first: Frame,
        allowed: &dyn Fn() -> bool,
    ) -> io::Result<()> {
        let sid = session.id();
        let mut f = first;
        loop {
            let data = recv_kind(conn, kind2::DATA, "write")?;
            if !allowed() {
                return Err(send_refusal(conn, ps5_dump_forge_flink::NOT_PAIRED));
            }
            let done = pinging(conn, || {
                if is_method(&f, "put") {
                    self.put(session, root, me, &f, &data)
                } else {
                    self.write(&sid, &f, &data)
                }
            });
            match done {
                Ok(ok) => conn.send_frame(&ok.frame()?)?,
                Err(why) => return Err(send_refusal(conn, &format!("{TRANSFER_FAILED}: {why}"))),
            }
            f = recv_kind(conn, kind2::REQUEST, "write")?;
        }
    }

    /// A whole small file in one round trip, as `create`, `write`, `sync`, `close` and
    /// `rename` would write it: `tmp` created (never over anything), written, synced, closed
    /// and renamed to `path` without replacing, the folders synced; on any failure `tmp` is
    /// removed.
    fn put(
        &self,
        session: &Session,
        root: Option<&Share>,
        me: &str,
        f: &Frame,
        data: &Frame,
    ) -> Result<DataOk, String> {
        let req = serde_json::from_slice::<Request>(&f.body).map_err(|_| "bad put")?;
        let (tmp, to, len) = (
            req.body["tmp"].as_str(),
            req.body["path"].as_str(),
            req.body["len"].as_u64(),
        );
        let (Some(tmp), Some(to), Some(len)) = (tmp, to, len) else {
            return Err("put needs tmp, path and len".into());
        };
        if len > CHUNK {
            return Err(format!("bad put of {len} bytes"));
        }
        let (head, bytes) = parse_data(data).map_err(|e| e.to_string())?;
        if (head.job, head.file, head.offset, bytes.len() as u64) != (0, 0, 0, len) {
            return Err("a chunk doesn't match its put".into());
        }
        if chunk_hash(bytes) != head.hash {
            return Err("a chunk's hash doesn't match".into());
        }
        let sid = session.id();
        let p = locate(root, tmp)?;
        self.check(&p, me)?;
        let handle = self.create_at(session, p)?;
        let written = (|| {
            self.write_owned(&sid, handle, bytes, 0)?;
            self.sync(&sid, handle)?;
            self.close(&sid, handle)?;
            self.rename(&sid, root, tmp, to, false, me)
        })();
        if let Err(why) = written {
            let _ = self.remove(&sid, root, tmp, me);
            return Err(why);
        }
        Ok(DataOk {
            job: 0,
            file: 0,
            offset: 0,
            complete: true,
        })
    }

    /// `bytes` at `at` of the file `handle`, if `sid` owns it, under the file's gate: owned
    /// when the gate is taken (not only before), so a `reattach` waits for this write and one
    /// after it is refused.
    fn write_owned(&self, sid: &str, handle: u64, bytes: &[u8], at: u64) -> Result<(), String> {
        let gate = (lock(&self.files).get(&handle))
            .map(|c| c.gate.clone())
            .ok_or_else(|| format!("{NO_OPEN_FILE} {handle}"))?;
        let _held = lock(&gate);
        let file = self.file(sid, handle)?;
        #[cfg(test)]
        {
            let stall = *lock(&self.stall);
            std::thread::sleep(stall);
        }
        write_all_at(&file, bytes, at).map_err(|e| format!("write: {e}"))
    }

    fn write(&self, sid: &str, f: &Frame, data: &Frame) -> Result<DataOk, String> {
        let req = serde_json::from_slice::<Request>(&f.body).map_err(|_| "bad write")?;
        let (handle, offset, len) = (
            req.body["handle"].as_u64(),
            req.body["offset"].as_u64(),
            req.body["len"].as_u64(),
        );
        let (Some(handle), Some(offset), Some(len)) = (handle, offset, len) else {
            return Err("write needs handle, offset and len".into());
        };
        if req.method != "write" || len > CHUNK {
            return Err(format!("bad write of {len} bytes"));
        }
        let (head, bytes) = parse_data(data).map_err(|e| e.to_string())?;
        if (head.job, head.file, head.offset, bytes.len() as u64) != (handle, 0, offset, len) {
            return Err("a chunk doesn't match its write".into());
        }
        if chunk_hash(bytes) != head.hash {
            return Err("a chunk's hash doesn't match".into());
        }
        self.write_owned(sid, handle, bytes, offset)?;
        Ok(DataOk {
            job: handle,
            file: 0,
            offset,
            complete: false,
        })
    }
}

// ---- The reading side (core's `Remote` builds on these) -----------------------------------------

/// A data lane to the peer of `hs`, joined under its session (lane number `n`).
pub fn join_lane(hs: &Handshake, n: u32) -> io::Result<Conn> {
    let mut c = Conn::connect(hs.remote, MAX_BODY, DEAD_AFTER)?;
    client_join(&mut c, &hs.session, n)?;
    Ok(c)
}

/// Asks for `len` bytes at `offset` of `handle` (answers come in the order asked).
pub fn send_read(conn: &mut Conn, handle: u64, offset: u64, len: u64) -> io::Result<()> {
    let req = Request {
        id: 0,
        method: "read".into(),
        body: serde_json::json!({ "handle": handle, "offset": offset, "len": len }),
    };
    conn.send_frame(&req.frame()?)
}

/// A lane's next answer; the peer's `Error` as an error. Its pings (`pinging`, sent while its
/// disk works) are skipped, unanswered: the lane's read timeout measures silence, not disk time.
fn recv_answer(conn: &mut Conn) -> io::Result<Frame> {
    let f = loop {
        let f = conn.recv_frame()?;
        match f.kind {
            kind2::PING | kind2::PONG => {}
            _ => break f,
        }
    };
    if f.kind == ps5_dump_forge_flink::kind::ERROR {
        let why = serde_json::from_slice::<Value>(&f.body).unwrap_or_default();
        return Err(io::Error::other(
            why["code"].as_str().unwrap_or(TRANSFER_FAILED).to_string(),
        ));
    }
    Ok(f)
}

/// The answer to a `send_read`: its bytes, BLAKE3-checked.
pub fn recv_read(conn: &mut Conn, handle: u64, offset: u64, len: u64) -> io::Result<Vec<u8>> {
    let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("flink: {why}"));
    let f = recv_answer(conn)?;
    if f.kind != kind2::DATA {
        return Err(bad("wanted Data"));
    }
    let (head, bytes) = parse_data(&f)?;
    if (head.job, head.file, head.offset) != (handle, 0, offset) || bytes.len() as u64 > len {
        return Err(bad("stray Data"));
    }
    if chunk_hash(bytes) != head.hash {
        return Err(bad("a chunk's hash doesn't match"));
    }
    Ok(bytes.to_vec())
}

pub fn read_range(conn: &mut Conn, handle: u64, offset: u64, len: u64) -> io::Result<Vec<u8>> {
    send_read(conn, handle, offset, len)?;
    recv_read(conn, handle, offset, len)
}

/// Sends `bytes` (at most `CHUNK`) for `offset` of the created file `handle`.
pub fn send_write(conn: &mut Conn, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
    let req = Request {
        id: 0,
        method: "write".into(),
        body: serde_json::json!({ "handle": handle, "offset": offset, "len": bytes.len() }),
    };
    conn.send_frame(&req.frame()?)?;
    let head = DataHead {
        job: handle,
        file: 0,
        offset,
        hash: chunk_hash(bytes),
        stat: None,
    };
    conn.send_frame(&data_frame(&head, bytes)?)
}

/// The answer to a `send_write` (they come in the order sent).
pub fn recv_written(conn: &mut Conn, handle: u64, offset: u64) -> io::Result<()> {
    let f = recv_answer(conn)?;
    let ok = (f.kind == kind2::DATA_OK)
        .then(|| serde_json::from_slice::<DataOk>(&f.body).ok())
        .flatten();
    match ok {
        Some(ok) if (ok.job, ok.file, ok.offset) == (handle, 0, offset) => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "flink: wanted the write's DataOk",
        )),
    }
}

// ---- Core's `Remote`: this device reading a paired device's files ------------------------------

/// Read handles one session keeps open (the oldest closed past this).
const KEPT_HANDLES: usize = 16;
/// Idle lanes kept for the next reads.
const KEPT_LANES: usize = 8;
/// A session or lane idle this long is redialled first: the peer drops it at `DEAD_AFTER`.
const IDLE: Duration = Duration::from_secs(DEAD_AFTER.as_secs() / 2);

/// Every running Connect service (tests run several at once); core's registry holds one
/// dispatcher over them, which asks the first that has the device paired.
fn services() -> &'static Mutex<Vec<Arc<ConnectRemote>>> {
    static S: Mutex<Vec<Arc<ConnectRemote>>> = Mutex::new(Vec::new());
    &S
}

struct Services;

impl Services {
    fn of(&self, key_id: &str) -> io::Result<Arc<ConnectRemote>> {
        let mut all = lock(services());
        all.retain(|r| r.inner.strong_count() > 0);
        all.iter()
            .find(|r| r.inner.upgrade().is_some_and(|i| i.serves(key_id)))
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "that device isn't paired"))
    }

    /// The service that created the file `handle`.
    fn owner(&self, handle: u64) -> io::Result<Arc<ConnectRemote>> {
        (lock(services()).iter())
            .find(|r| lock(&r.files).contains_key(&handle))
            .cloned()
            .ok_or_else(|| no_file(handle))
    }
}

/// `Connect::start_rooted`: adds the service (and registers the dispatcher once).
pub(crate) fn register(inner: &Arc<Inner>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    lock(services()).push(ConnectRemote::new(inner));
    ONCE.call_once(|| remote::set(Arc::new(Services)));
}

/// One service's paired devices as core's `Remote`. Per device one control session (stat,
/// list, read handles, created files) and a few idle data lanes, kept between calls; `read_at`
/// and `write_at` may run on several threads, each on lanes of its own.
pub struct ConnectRemote {
    inner: Weak<Inner>,
    peers: Mutex<HashMap<String, Arc<PeerFs>>>,
    /// Files created on peers by core's handle (unique across services, so `Services` can
    /// route a handle), until renamed or removed.
    files: Mutex<HashMap<u64, Made>>,
}

/// A file this device created on a peer: the peer's handle under the session of `generation`,
/// which owns it (the peer removes it, unrenamed, when that session ends).
#[derive(Clone)]
struct Made {
    at: FlinkPath,
    generation: u64,
    handle: u64,
    open: bool,
    /// The end of the furthest write the peer confirmed (offset + length): a resume finding
    /// the file shorter (cut while the link was down) is refused, never leaving a gap.
    acked: u64,
}

#[derive(Default)]
struct PeerFs {
    session: Mutex<Option<Sess>>,
    /// Idle lanes with the generation of the session they joined.
    lanes: Mutex<Vec<(u64, Conn, Instant)>>,
    /// ... and those that wrote (a lane reads or writes, never both).
    write_lanes: Mutex<Vec<(u64, Conn, Instant)>>,
    /// Files created and not yet renamed or removed: their session is kept alive meanwhile.
    writing: AtomicUsize,
    generation: AtomicU64,
    /// The last call or lane read for this device (`None` before any).
    active: Mutex<Option<Instant>>,
    /// Lane reads under way (a slow chunk may take longer than `IDLE`).
    reading: AtomicUsize,
}

impl PeerFs {
    fn touch(&self) {
        *lock(&self.active) = Some(Instant::now());
    }

    /// Unused for `IDLE` with no read under way: its session has had no keepalive since, so it
    /// is redialled.
    fn idle(&self) -> bool {
        self.reading.load(Ordering::SeqCst) == 0
            && self.writing.load(Ordering::SeqCst) == 0
            && lock(&self.active).is_none_or(|t| t.elapsed() > IDLE)
    }
}

/// A lane read under way: its session is kept alive until it ends.
struct Reading<'a>(&'a PeerFs);

impl<'a> Reading<'a> {
    fn new(peer: &'a PeerFs) -> Reading<'a> {
        peer.reading.fetch_add(1, Ordering::SeqCst);
        Reading(peer)
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        self.0.touch();
        self.0.reading.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Keeps a device's session (generation `g`) alive while it is in use: the peer drops a control
/// session silent for `DEAD_AFTER`, and every read handle with it, while the reads themselves go
/// on lanes. Stops once the session is replaced, broken or idle (then left to expire).
fn keep(peer: Weak<PeerFs>, g: u64) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(PING_EVERY * 2);
            let Some(peer) = peer.upgrade() else { return };
            let mut s = lock(&peer.session);
            match s.as_mut() {
                Some(sess) if sess.generation == g && !peer.idle() => {
                    if sess.rpc.call("info", Value::Null).is_err() {
                        *s = None;
                        return;
                    }
                }
                _ => return,
            }
        }
    });
}

struct Sess {
    rpc: Rpc,
    hs: Arc<Handshake>,
    generation: u64,
    /// Open handles by path, oldest first: (path, handle, size, mtime_ns).
    handles: Vec<(String, u64, u64, Option<u64>)>,
}

impl Sess {
    fn forget(&mut self, path: &str) {
        if let Some(i) = self.handles.iter().position(|h| h.0 == path) {
            let (_, h, ..) = self.handles.remove(i);
            let _ = self
                .rpc
                .call("read_close", serde_json::json!({ "handle": h }));
        }
    }
}

/// A peer's answer (`Other`, its text) as core expects it: `NotFound` when nothing is there,
/// `AlreadyExists` when something is.
fn answered(e: io::Error) -> io::Error {
    if e.kind() != io::ErrorKind::Other {
        return e;
    }
    let text = e.to_string();
    for (lead, kind) in [
        (NOT_FOUND, io::ErrorKind::NotFound),
        (EXISTS, io::ErrorKind::AlreadyExists),
    ] {
        if let Some(rest) = text.strip_prefix(lead) {
            return io::Error::new(kind, rest.to_string());
        }
    }
    e
}

/// A rename or remove the peer did, its folders' sync failing after it.
fn unsynced(e: &io::Error) -> bool {
    e.to_string().starts_with(UNSYNCED)
}

/// Whether the error is the peer's own answer (the session is fine), not a broken link.
fn is_answer(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::Other | io::ErrorKind::NotFound)
}

fn stat_of(v: Value) -> io::Result<FsStat> {
    serde_json::from_value(v).map_err(io::Error::other)
}

fn mtime(ns: Option<u64>) -> Option<SystemTime> {
    ns.map(|n| UNIX_EPOCH + Duration::from_nanos(n))
}

/// The created file's session ended (the link broke or went silent) and it wasn't resumed: the
/// peer removes it.
fn lost(c: &Made) -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        format!(
            "{}: the connection to the device was lost, which removes the unfinished file",
            c.at.url().display()
        ),
    )
}

fn no_file(handle: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no file open on another device as {handle}"),
    )
}

/// An idle lane of generation `g` from `pool`, or a new one joined under `hs`.
fn take_lane(pool: &Mutex<Vec<(u64, Conn, Instant)>>, hs: &Handshake, g: u64) -> io::Result<Conn> {
    let kept = {
        let mut lanes = lock(pool);
        lanes.retain(|l| l.0 == g && l.2.elapsed() < IDLE);
        lanes.pop()
    };
    match kept {
        Some((_, c, _)) => Ok(c),
        None => join_lane(hs, 1),
    }
}

fn put_lane(pool: &Mutex<Vec<(u64, Conn, Instant)>>, g: u64, conn: Conn) {
    let mut lanes = lock(pool);
    if lanes.len() < KEPT_LANES {
        lanes.push((g, conn, Instant::now()));
    }
}

/// Created file ids, unique across services.
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// Retried `put`s, for their temporary names.
static NEXT_RETRY: AtomicU64 = AtomicU64::new(0);

/// A temporary name of a retried `put` beside `tmp` (`<tmp stem>-r<n>.tmp`), never the first
/// attempt's: that one may still be under way on the peer.
fn retry_name(tmp: &str) -> String {
    let n = NEXT_RETRY.fetch_add(1, Ordering::SeqCst) + 1;
    match tmp.strip_suffix(".tmp") {
        Some(stem) => format!("{stem}-r{n}.tmp"),
        None => format!("{tmp}-r{n}"),
    }
}

/// Times one call on a created file takes it over on a fresh session (`reattach`).
const RESUMES: usize = 3;

impl ConnectRemote {
    /// `op` on the created file `id` (`c` its record). When the session that created it is
    /// lost, a fresh session takes the file over (`reattach`) and `op` runs again there, at
    /// most `RESUMES` times and within the peer's `KEPT_FOR`. `at`: where the call resumes, for
    /// the log (`None`: the file's size). A peer without `reattach` (older) leaves the loss.
    /// A redone write is positional, so a piece sent twice is the same bytes; core hashes what
    /// it writes (BLAKE3) as it goes, and a full verify reads the file back against that.
    // ponytail: the line goes to stderr (the serve log), not the job's log.
    fn resuming<T>(
        &self,
        id: u64,
        mut c: Made,
        at: Option<u64>,
        op: impl Fn(&Made) -> io::Result<T>,
    ) -> io::Result<T> {
        let start = Instant::now();
        let mut tries = 0;
        loop {
            match op(&c) {
                Err(e)
                    if e.kind() == io::ErrorKind::ConnectionAborted
                        && tries < RESUMES
                        && start.elapsed() < KEPT_FOR =>
                {
                    tries += 1;
                    match self.reattach(id, &c, at) {
                        Ok(next) => c = next,
                        Err(_) => return Err(e),
                    }
                }
                done => return done,
            }
        }
    }

    /// `reattach` of `c` on the device's session (a fresh one: the old one is gone), which
    /// then owns it: the record `id` moves to that session's generation and the handle given.
    /// A file found shorter than a write the peer confirmed (cut while the link was down) is
    /// removed there and let go: a resume would leave a gap.
    fn reattach(&self, id: u64, c: &Made, at: Option<u64>) -> io::Result<Made> {
        let acked = lock(&self.files).get(&id).map_or(0, |m| m.acked);
        let body = serde_json::json!({ "path": c.at.path });
        let (v, generation) = self.with(&c.at.key_id, |s| {
            Ok((s.rpc.call("reattach", body.clone())?, s.generation))
        })?;
        let name = (self.inner.upgrade())
            .map_or_else(|| c.at.key_id.clone(), |i| i.peer_name(&c.at.key_id));
        let size = v["size"].as_u64();
        if let Some(size) = size.filter(|size| *size < acked) {
            let _ = self.with(&c.at.key_id, |s| s.rpc.call("remove", body.clone()));
            self.drop_file(id);
            eprintln!(
                "connect: {name} holds /{} at {size} bytes, shorter than the {acked} written: not resumed",
                c.at.path
            );
            return Err(io::Error::other("the unfinished file was cut short"));
        }
        let handle = v["handle"]
            .as_u64()
            .ok_or_else(|| io::Error::other("flink: reattach gave no handle"))?;
        let next = {
            let mut files = lock(&self.files);
            let made = files.get_mut(&id).ok_or_else(|| lost(c))?;
            made.generation = generation;
            made.handle = handle;
            made.clone()
        };
        let offset = at.or(size).unwrap_or(0);
        eprintln!(
            "connect: reconnected to {name}, resuming /{} at {offset}",
            c.at.path
        );
        Ok(next)
    }

    /// `bytes` at `offset` of `c` on its session: a broken lane once more on fresh lanes.
    fn write_on(&self, c: &Made, offset: u64, bytes: &[u8]) -> io::Result<()> {
        self.on_session(c, |_| Ok(()))?;
        if bytes.is_empty() {
            return Ok(());
        }
        let e = match self.write_lanes(c, offset, bytes) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        let text = e.to_string();
        // The handle gone under the write: its session ended (the file kept for a while).
        if text.starts_with(&format!("{TRANSFER_FAILED}: {NO_OPEN_FILE}")) {
            return Err(lost(c));
        }
        if text.starts_with(TRANSFER_FAILED) {
            return Err(e);
        }
        // A broken lane: once more on fresh lanes while the session still holds the file
        // (positional writes, so a piece written twice is the same bytes).
        self.on_session(c, |s| s.rpc.call("info", Value::Null).map(drop))?;
        self.write_lanes(c, offset, bytes).map_err(|e| {
            if e.to_string()
                .starts_with(&format!("{TRANSFER_FAILED}: {NO_OPEN_FILE}"))
            {
                lost(c)
            } else {
                e
            }
        })
    }

    /// `rename {from, to, replace}` on the peer; the file is the device's once renamed.
    fn rename_as(&self, from: &FlinkPath, to: &FlinkPath, replace: bool) -> io::Result<()> {
        if from.key_id != to.key_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a file is renamed on its own device only",
            ));
        }
        let body = if replace {
            serde_json::json!({ "from": from.path, "to": to.path, "replace": true })
        } else {
            serde_json::json!({ "from": from.path, "to": to.path })
        };
        let Some((id, c)) = self.created(from) else {
            // Not created here: the peer refuses it (only its session's files are renamed).
            return self.with(&from.key_id, |s| {
                s.rpc.call("rename", body.clone()).map(drop)
            });
        };
        let sent = std::cell::Cell::new(false);
        // A lost session resumes: a file the rename moved already is no longer reattached.
        let mut done = self.resuming(id, c, None, |c| {
            self.on_session(c, |s| {
                sent.set(true);
                s.rpc.call("rename", body.clone()).map(drop)
            })
        });
        // The link broke during the rename: the peer either renamed it or removes it.
        if sent.get()
            && done
                .as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::ConnectionAborted)
        {
            done = Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                format!(
                    "{}: the connection to the device was lost during the rename; {} may or may not be there",
                    from.url().display(),
                    to.url().display()
                ),
            ));
        }
        if done.is_ok()
            || done
                .as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::ConnectionAborted || unsynced(e))
        {
            self.drop_file(id);
        }
        done
    }

    pub(crate) fn new(inner: &Arc<Inner>) -> Arc<ConnectRemote> {
        Arc::new(ConnectRemote {
            inner: Arc::downgrade(inner),
            peers: Mutex::default(),
            files: Mutex::default(),
        })
    }

    fn peer(&self, key_id: &str) -> Arc<PeerFs> {
        lock(&self.peers).entry(key_id.into()).or_default().clone()
    }

    /// `f` on the device's session: a fresh one when there is none or it sat idle, and once
    /// more on a fresh one when a kept session turns out broken.
    fn with<T>(&self, key_id: &str, f: impl Fn(&mut Sess) -> io::Result<T>) -> io::Result<T> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| io::Error::other("the Connect service has stopped"))?;
        let peer = self.peer(key_id);
        let mut s = lock(&peer.session);
        if !inner.serves(key_id) {
            *s = None;
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                ps5_dump_forge_flink::NOT_PAIRED,
            ));
        }
        if s.as_ref()
            .is_some_and(|s| !inner.paired_session(key_id, &s.hs.session))
        {
            *s = None;
        }
        if peer.idle() {
            *s = None;
        }
        let mut kept = s.is_some();
        loop {
            if s.is_none() {
                let (rpc, hs) = inner.session(key_id)?;
                let generation = peer.generation.fetch_add(1, Ordering::SeqCst) + 1;
                *s = Some(Sess {
                    rpc,
                    hs: Arc::new(hs),
                    generation,
                    handles: Vec::new(),
                });
                keep(Arc::downgrade(&peer), generation);
            }
            let sess = s.as_mut().expect("set above");
            match f(sess) {
                Ok(v) => {
                    peer.touch();
                    return Ok(v);
                }
                Err(e) if is_answer(&e) => return Err(answered(e)),
                Err(e) => {
                    *s = None;
                    if !std::mem::take(&mut kept) {
                        return Err(e);
                    }
                }
            }
        }
    }

    /// The handle for `path` (opened when not kept), the session's handshake and generation.
    fn handle(&self, at: &FlinkPath) -> io::Result<(u64, Arc<Handshake>, u64)> {
        self.with(&at.key_id, |s| {
            if let Some(i) = s.handles.iter().position(|h| h.0 == at.path) {
                let h = s.handles.remove(i);
                let id = h.1;
                s.handles.push(h);
                return Ok((id, s.hs.clone(), s.generation));
            }
            let v = s
                .rpc
                .call("read_open", serde_json::json!({ "path": at.path }))?;
            let id = v["handle"]
                .as_u64()
                .ok_or_else(|| io::Error::other("flink: read_open gave no handle"))?;
            if s.handles.len() >= KEPT_HANDLES {
                let old = s.handles[0].0.clone();
                s.forget(&old);
            }
            let (size, ns) = (v["size"].as_u64().unwrap_or(0), v["mtime_ns"].as_u64());
            s.handles.push((at.path.clone(), id, size, ns));
            Ok((id, s.hs.clone(), s.generation))
        })
    }

    /// `len` bytes at `offset` of handle `h`: `CHUNK` pieces dealt round robin to up to
    /// `KEPT_LANES` lanes of generation `g` (kept or joined), as `write_lanes` writes; one short
    /// piece means the file ended there.
    fn read_lanes(
        peer: &PeerFs,
        hs: &Handshake,
        g: u64,
        h: u64,
        offset: u64,
        len: usize,
    ) -> io::Result<Vec<u8>> {
        let pieces: Vec<(u64, u64)> = (0..len as u64)
            .step_by(CHUNK as usize)
            .map(|o| (offset + o, (len as u64 - o).min(CHUNK)))
            .collect();
        let n = pieces.len().clamp(1, KEPT_LANES);
        let group =
            |k: usize| -> Vec<(u64, u64)> { pieces.iter().skip(k).step_by(n).copied().collect() };
        let results: Vec<io::Result<Vec<Vec<u8>>>> = if n == 1 {
            vec![Self::read_lane(peer, hs, g, h, &pieces)]
        } else {
            std::thread::scope(|sc| {
                let runs: Vec<_> = (0..n)
                    .map(|k| {
                        let mine = group(k);
                        sc.spawn(move || Self::read_lane(peer, hs, g, h, &mine))
                    })
                    .collect();
                runs.into_iter()
                    .map(|r| {
                        r.join()
                            .unwrap_or_else(|_| Err(io::Error::other("a read panicked")))
                    })
                    .collect()
            })
        };
        let mut lanes = Vec::with_capacity(n);
        let mut errs = Vec::new();
        for r in results {
            match r {
                Ok(got) => lanes.push(got.into_iter()),
                Err(e) => errs.push(e),
            }
        }
        if let Some(e) = first_error(errs) {
            return Err(e);
        }
        // Piece i came on lane i % n, in order there.
        let mut out = Vec::with_capacity(len);
        for (i, &(o, _)) in pieces.iter().enumerate() {
            let bytes = lanes[i % n].next().unwrap_or_default();
            if out.len() as u64 == o - offset {
                out.extend_from_slice(&bytes);
            }
        }
        Ok(out)
    }

    /// `pieces` (offset, length) of handle `h` read on one lane of generation `g` (kept or
    /// joined): every piece asked at once, answered in order.
    fn read_lane(
        peer: &PeerFs,
        hs: &Handshake,
        g: u64,
        h: u64,
        pieces: &[(u64, u64)],
    ) -> io::Result<Vec<Vec<u8>>> {
        let _reading = Reading::new(peer);
        let mut conn = take_lane(&peer.lanes, hs, g)?;
        for &(o, n) in pieces {
            send_read(&mut conn, h, o, n)?;
        }
        let mut out = Vec::with_capacity(pieces.len());
        for &(o, n) in pieces {
            out.push(recv_read(&mut conn, h, o, n)?);
            peer.touch();
        }
        put_lane(&peer.lanes, g, conn);
        Ok(out)
    }

    /// `len` bytes at `offset` of the peer's file `rel` (at most `CHUNK`), with its stat after
    /// the read, on one lane of generation `g`.
    fn read_path(
        peer: &PeerFs,
        hs: &Handshake,
        g: u64,
        rel: &str,
        offset: u64,
        len: usize,
    ) -> io::Result<(Vec<u8>, RemoteStat)> {
        let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("flink: {why}"));
        let _reading = Reading::new(peer);
        let mut conn = take_lane(&peer.lanes, hs, g)?;
        let req = Request {
            id: 0,
            method: "read".into(),
            body: serde_json::json!({ "path": rel, "offset": offset, "len": len }),
        };
        conn.send_frame(&req.frame()?)?;
        let f = recv_answer(&mut conn)?;
        if f.kind != kind2::DATA {
            return Err(bad("wanted Data"));
        }
        let (head, bytes) = parse_data(&f)?;
        if (head.job, head.file, head.offset) != (0, 0, offset) || bytes.len() > len {
            return Err(bad("stray Data"));
        }
        if chunk_hash(bytes) != head.hash {
            return Err(bad("a chunk's hash doesn't match"));
        }
        let st = stat_of(
            head.stat
                .ok_or_else(|| bad("a read by path came without its stat"))?,
        )?;
        let bytes = bytes.to_vec();
        put_lane(&peer.lanes, g, conn);
        peer.touch();
        Ok((
            bytes,
            RemoteStat {
                dir: st.dir,
                size: st.size,
                mtime: mtime(st.mtime_ns),
                link: st.link,
            },
        ))
    }

    fn file(&self, handle: u64) -> io::Result<Made> {
        lock(&self.files)
            .get(&handle)
            .filter(|c| c.open)
            .cloned()
            .ok_or_else(|| no_file(handle))
    }

    /// The file created at `at` (open or closed), with its core handle.
    fn created(&self, at: &FlinkPath) -> Option<(u64, Made)> {
        (lock(&self.files).iter())
            .find(|(_, c)| c.at == *at)
            .map(|(h, c)| (*h, c.clone()))
    }

    /// Lets go of a created file (renamed, removed, or lost with its session).
    fn drop_file(&self, handle: u64) {
        if let Some(c) = lock(&self.files).remove(&handle) {
            self.peer(&c.at.key_id)
                .writing
                .fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// `f` on the session that created `c`, never on another: the file went with it.
    fn on_session<T>(&self, c: &Made, f: impl FnOnce(&mut Sess) -> io::Result<T>) -> io::Result<T> {
        let peer = self.peer(&c.at.key_id);
        let mut s = lock(&peer.session);
        if s.as_ref().is_some_and(|s| {
            !self
                .inner
                .upgrade()
                .is_some_and(|i| i.paired_session(&c.at.key_id, &s.hs.session))
        }) {
            *s = None;
        }
        match s.as_mut() {
            Some(sess) if sess.generation == c.generation => match f(sess) {
                Ok(v) => {
                    peer.touch();
                    Ok(v)
                }
                Err(e) if is_answer(&e) => Err(answered(e)),
                Err(_) => {
                    *s = None;
                    Err(lost(c))
                }
            },
            _ => Err(lost(c)),
        }
    }

    /// `pieces` written on one lane of generation `g`: all sent, then every answer read.
    fn write_lane(
        peer: &PeerFs,
        hs: &Handshake,
        g: u64,
        h: u64,
        pieces: &[(u64, &[u8])],
    ) -> io::Result<()> {
        let mut conn = take_lane(&peer.write_lanes, hs, g)?;
        let mut sent = 0;
        let mut failed = None;
        for &(o, bytes) in pieces {
            if let Err(e) = send_write(&mut conn, h, o, bytes) {
                failed = Some(e);
                break;
            }
            sent += 1;
        }
        // A refusal explains a failed send better than the send's own error.
        for &(o, _) in &pieces[..sent] {
            recv_written(&mut conn, h, o)?;
            peer.touch();
        }
        if let Some(e) = failed {
            return Err(e);
        }
        put_lane(&peer.write_lanes, g, conn);
        Ok(())
    }

    /// `bytes` at `offset` of `c`, in `CHUNK` pieces dealt round robin to up to `KEPT_LANES`
    /// lanes; the peer's refusal before a broken lane's error.
    fn write_lanes(&self, c: &Made, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let peer = self.peer(&c.at.key_id);
        let hs = self.on_session(c, |s| Ok(s.hs.clone()))?;
        let pieces: Vec<(u64, &[u8])> = (bytes.chunks(CHUNK as usize).enumerate())
            .map(|(i, b)| (offset + i as u64 * CHUNK, b))
            .collect();
        let n = pieces.len().min(KEPT_LANES);
        let group =
            |k: usize| -> Vec<(u64, &[u8])> { pieces.iter().skip(k).step_by(n).copied().collect() };
        let results: Vec<io::Result<()>> = if n == 1 {
            vec![Self::write_lane(
                &peer,
                &hs,
                c.generation,
                c.handle,
                &pieces,
            )]
        } else {
            std::thread::scope(|sc| {
                let runs: Vec<_> = (0..n)
                    .map(|k| {
                        let (peer, hs, mine) = (&peer, &hs, group(k));
                        sc.spawn(move || Self::write_lane(peer, hs, c.generation, c.handle, &mine))
                    })
                    .collect();
                runs.into_iter()
                    .map(|r| {
                        r.join()
                            .unwrap_or_else(|_| Err(io::Error::other("a write panicked")))
                    })
                    .collect()
            })
        };
        let errs: Vec<io::Error> = results.into_iter().filter_map(Result::err).collect();
        first_error(errs).map_or(Ok(()), Err)
    }
}

/// Of several lanes' errors, the peer's refusal before a broken lane's (it explains the break).
fn first_error(mut errs: Vec<io::Error>) -> Option<io::Error> {
    match errs
        .iter()
        .position(|e| e.to_string().starts_with(TRANSFER_FAILED))
    {
        Some(i) => Some(errs.swap_remove(i)),
        None => errs.into_iter().next(),
    }
}

impl remote::Remote for Services {
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
        self.of(&path.key_id)?.stat(path)
    }
    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
        self.of(&path.key_id)?.list(path)
    }
    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.of(&path.key_id)?.read_at(path, offset, len)
    }
    fn read_checked(
        &self,
        path: &FlinkPath,
        offset: u64,
        len: usize,
    ) -> io::Result<(Vec<u8>, RemoteStat)> {
        self.of(&path.key_id)?.read_checked(path, offset, len)
    }
    fn put_new(&self, tmp: &FlinkPath, to: &FlinkPath, bytes: &[u8]) -> io::Result<()> {
        self.of(&to.key_id)?.put_new(tmp, to, bytes)
    }
    fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
        self.of(&path.key_id)?.mkdir(path)
    }
    fn create(&self, path: &FlinkPath) -> io::Result<u64> {
        self.of(&path.key_id)?.create(path)
    }
    fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
        self.owner(handle)?.write_at(handle, offset, bytes)
    }
    fn sync(&self, handle: u64) -> io::Result<()> {
        self.owner(handle)?.sync(handle)
    }
    fn close(&self, handle: u64) -> io::Result<()> {
        self.owner(handle)?.close(handle)
    }
    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        self.of(&from.key_id)?.rename_no_replace(from, to)
    }
    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        self.of(&from.key_id)?.rename_replace(from, to)
    }
    fn remove(&self, path: &FlinkPath) -> io::Result<()> {
        self.of(&path.key_id)?.remove(path)
    }
    fn free_space(&self, dir: &FlinkPath) -> io::Result<u64> {
        self.of(&dir.key_id)?.free_space(dir)
    }
    fn delete_path(&self, path: &FlinkPath) -> Result<(), DeleteError> {
        self.of(&path.key_id)
            .map_err(|e| unpaired(path, e))?
            .delete_path(path)
    }
    fn move_path(
        &self,
        from: &FlinkPath,
        to: &FlinkPath,
        replace: bool,
    ) -> Result<(), DeleteError> {
        self.of(&from.key_id)
            .map_err(|e| unpaired(from, e))?
            .move_path(from, to, replace)
    }
}

fn unpaired(at: &FlinkPath, e: io::Error) -> DeleteError {
    DeleteError::Refused(format!("{}: {e}", at.url().display()))
}

impl remote::Remote for ConnectRemote {
    fn stat(&self, path: &FlinkPath) -> io::Result<RemoteStat> {
        let st = self.with(&path.key_id, |s| {
            let st = s.rpc.call("stat", serde_json::json!({ "path": path.path }));
            // Nothing there (or no longer a file to stat): a kept handle of it is let go too.
            if st.as_ref().is_err_and(is_answer) {
                s.forget(&path.path);
            }
            let st = stat_of(st?)?;
            // A kept handle of an older file (replaced or changed since) is let go.
            if s.handles
                .iter()
                .any(|h| h.0 == path.path && (h.2, h.3) != (st.size, st.mtime_ns))
            {
                s.forget(&path.path);
            }
            Ok(st)
        })?;
        Ok(RemoteStat {
            dir: st.dir,
            size: st.size,
            mtime: mtime(st.mtime_ns),
            link: st.link,
        })
    }

    fn list(&self, path: &FlinkPath) -> io::Result<Vec<RemoteEntry>> {
        let v = self.with(&path.key_id, |s| {
            s.rpc
                .call("fs_list", serde_json::json!({ "path": path.path }))
        })?;
        let entries: Vec<FsEntry> =
            serde_json::from_value(v["entries"].clone()).map_err(io::Error::other)?;
        Ok(entries
            .into_iter()
            .map(|e| RemoteEntry {
                name: e.name,
                dir: e.stat.dir,
                size: e.stat.size,
                mtime: mtime(e.stat.mtime_ns),
                link: e.stat.link,
                special: e.special,
                bad_name: e.bad_name,
            })
            .collect())
    }

    fn read_at(&self, path: &FlinkPath, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let peer = self.peer(&path.key_id);
        let (h, hs, g) = self.handle(path)?;
        let e = match Self::read_lanes(&peer, &hs, g, h, offset, len) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        let text = e.to_string();
        let gone = text.starts_with(&format!("{TRANSFER_FAILED}: {NO_OPEN_FILE}"));
        // The peer's own refusal of the read (its file failed it): as it is.
        if text.starts_with(TRANSFER_FAILED) && !gone {
            return Err(e);
        }
        // A broken lane, or the handle closed under the read (its session ended, or another
        // read let it go): the session is checked (redialled when broken, which reopens the
        // handle; other reads' handles stay) and the read asked once more on another lane.
        self.with(&path.key_id, |s| {
            if gone && s.generation == g {
                s.handles.retain(|k| k.1 != h);
            }
            s.rpc.call("info", Value::Null).map(drop)
        })?;
        let (h, hs, g) = self.handle(path)?;
        Self::read_lanes(&peer, &hs, g, h, offset, len)
    }

    /// Up to one chunk of a file with no kept handle: one lane round trip by path, the stat
    /// after the read in its answer (no `read_open`, `read_close` or `stat`), asked once more
    /// on a broken lane as `read_at` does. Longer reads, and files read through a handle, as
    /// `read_at` and `stat`.
    fn read_checked(
        &self,
        path: &FlinkPath,
        offset: u64,
        len: usize,
    ) -> io::Result<(Vec<u8>, RemoteStat)> {
        let kept = self.with(&path.key_id, |s| {
            Ok(s.handles.iter().any(|h| h.0 == path.path))
        })?;
        if kept || len as u64 > CHUNK {
            let bytes = self.read_at(path, offset, len)?;
            return Ok((bytes, self.stat(path)?));
        }
        let peer = self.peer(&path.key_id);
        let once = || {
            let (hs, g) = self.with(&path.key_id, |s| Ok((s.hs.clone(), s.generation)))?;
            Self::read_path(&peer, &hs, g, &path.path, offset, len)
        };
        match once() {
            Ok(got) => Ok(got),
            Err(e) if e.to_string().starts_with(TRANSFER_FAILED) => Err(e),
            Err(_) => {
                self.with(&path.key_id, |s| s.rpc.call("info", Value::Null).map(drop))?;
                once()
            }
        }
    }

    /// A small file in one round trip: `put` on a write lane, which the peer answers once the
    /// file is written, synced and renamed (larger ones: the trait's create, write, sync, close
    /// and rename, which resume a lost session). A broken lane or session: asked once more,
    /// whole, on a checked (else fresh) session under a temporary name of its own (the first
    /// may still be writing its own); the first may have landed, so a `to` already there
    /// holding exactly these bytes is that one.
    fn put_new(&self, tmp: &FlinkPath, to: &FlinkPath, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() as u64 > CHUNK || tmp.key_id != to.key_id {
            let handle = self.create(tmp)?;
            let done = (|| {
                if !bytes.is_empty() {
                    self.write_at(handle, 0, bytes)?;
                }
                self.sync(handle)?;
                self.close(handle)?;
                self.rename_no_replace(tmp, to)
            })();
            if done.is_err() {
                let _ = self.close(handle);
                let _ = self.remove(tmp);
            }
            return done;
        }
        let peer = self.peer(&to.key_id);
        // Its session is kept alive while the put runs, as a lane read's is.
        let _busy = Reading::new(&peer);
        let once = |tmp: &str, check: bool| -> io::Result<()> {
            let (hs, g) = self.with(&to.key_id, |s| {
                if check {
                    s.rpc.call("info", Value::Null)?;
                }
                Ok((s.hs.clone(), s.generation))
            })?;
            let mut conn = take_lane(&peer.write_lanes, &hs, g)?;
            let req = Request {
                id: 0,
                method: "put".into(),
                body: serde_json::json!({ "tmp": tmp, "path": to.path, "len": bytes.len() }),
            };
            conn.send_frame(&req.frame()?)?;
            let head = DataHead {
                job: 0,
                file: 0,
                offset: 0,
                hash: chunk_hash(bytes),
                stat: None,
            };
            conn.send_frame(&data_frame(&head, bytes)?)?;
            recv_written(&mut conn, 0, 0)?;
            put_lane(&peer.write_lanes, g, conn);
            peer.touch();
            Ok(())
        };
        let named = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", to.url().display()));
        let e = match once(&tmp.path, false) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        if e.to_string().starts_with(TRANSFER_FAILED) || is_answer(&e) {
            return Err(named(e));
        }
        match once(&retry_name(&tmp.path), true) {
            Ok(()) => Ok(()),
            Err(again)
                if again.to_string().contains(EXISTS)
                    && remote::Remote::read_at(self, to, 0, bytes.len() + 1)
                        .is_ok_and(|b| b == bytes) =>
            {
                Ok(())
            }
            Err(again) => Err(named(again)),
        }
    }

    fn mkdir(&self, path: &FlinkPath) -> io::Result<()> {
        let v = self.with(&path.key_id, |s| {
            s.rpc
                .call("mkdir", serde_json::json!({ "path": path.path }))
        })?;
        match v["created"].as_bool() {
            Some(true) => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{}: the folder exists", path.url().display()),
            )),
        }
    }

    fn create(&self, path: &FlinkPath) -> io::Result<u64> {
        // The session is checked first (redialled when broken); the create itself is never
        // asked twice: a reply lost after the peer made the file would answer `AlreadyExists`.
        let (made, generation) = self.with(&path.key_id, |s| {
            s.rpc.call("info", Value::Null)?;
            let made = s
                .rpc
                .call("create", serde_json::json!({ "path": path.path }));
            Ok((made, s.generation))
        })?;
        let v = match made {
            Ok(v) => v,
            Err(e) if is_answer(&e) => return Err(answered(e)),
            // The reply was lost: the session is ended, so the peer removes what it may have made.
            Err(e) => {
                let peer = self.peer(&path.key_id);
                let mut s = lock(&peer.session);
                if s.as_ref().is_some_and(|s| s.generation == generation) {
                    *s = None;
                }
                return Err(e);
            }
        };
        let handle = v["handle"]
            .as_u64()
            .ok_or_else(|| io::Error::other("flink: create gave no handle"))?;
        self.peer(&path.key_id)
            .writing
            .fetch_add(1, Ordering::SeqCst);
        let id = NEXT_FILE.fetch_add(1, Ordering::SeqCst) + 1;
        let c = Made {
            at: path.clone(),
            generation,
            handle,
            open: true,
            acked: 0,
        };
        lock(&self.files).insert(id, c);
        Ok(id)
    }

    /// Resumed on a fresh session when the file's session is lost (`resuming`).
    fn write_at(&self, handle: u64, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let c = self.file(handle)?;
        self.resuming(handle, c, Some(offset), |c| self.write_on(c, offset, bytes))?;
        if let Some(c) = lock(&self.files).get_mut(&handle) {
            c.acked = c.acked.max(offset + bytes.len() as u64);
        }
        Ok(())
    }

    fn sync(&self, handle: u64) -> io::Result<()> {
        let c = self.file(handle)?;
        self.resuming(handle, c, None, |c| {
            self.on_session(c, |s| {
                s.rpc
                    .call("sync", serde_json::json!({ "handle": c.handle }))
                    .map(drop)
            })
        })
    }

    fn close(&self, handle: u64) -> io::Result<()> {
        let mut c = self.file(handle)?;
        if let Some(c) = lock(&self.files).get_mut(&handle) {
            c.open = false;
        }
        c.open = false;
        // Asked again after a `reattach`, a close that landed closes again fine.
        self.resuming(handle, c, None, |c| {
            self.on_session(c, |s| {
                s.rpc
                    .call("close", serde_json::json!({ "handle": c.handle }))
                    .map(drop)
            })
        })
    }

    fn rename_no_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        self.rename_as(from, to, false)
    }

    /// The peer checks what it replaces (a regular file no peer is writing, refused where its
    /// own job's replace would be). A peer older
    /// than `replace` ignores the field and refuses a taken name (`exists`): nothing replaced.
    fn rename_replace(&self, from: &FlinkPath, to: &FlinkPath) -> io::Result<()> {
        self.rename_as(from, to, true)
    }

    fn remove(&self, path: &FlinkPath) -> io::Result<()> {
        let body = serde_json::json!({ "path": path.path });
        let Some((id, c)) = self.created(path) else {
            return self.with(&path.key_id, |s| {
                s.rpc.call("remove", body.clone()).map(drop)
            });
        };
        let done = self.on_session(&c, |s| s.rpc.call("remove", body).map(drop));
        match done {
            Ok(()) => {
                self.drop_file(id);
                Ok(())
            }
            // The session went: the peer removes the unfinished file once it isn't resumed.
            Err(e) if e.kind() == io::ErrorKind::ConnectionAborted => {
                self.drop_file(id);
                Ok(())
            }
            // Removed, its folder's sync failed: the peer let the file go already.
            Err(e) if unsynced(&e) => {
                self.drop_file(id);
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    fn free_space(&self, dir: &FlinkPath) -> io::Result<u64> {
        let v = self.with(&dir.key_id, |s| {
            s.rpc.call("free", serde_json::json!({ "path": dir.path }))
        })?;
        v["free"]
            .as_u64()
            .ok_or_else(|| io::Error::other("flink: free gave no number"))
    }

    /// The peer deletes it as its own Delete would (`delete_path` RPC).
    // ponytail: a session found broken is tried once more; a delete the peer did before the
    // link broke then reads as missing.
    fn delete_path(&self, path: &FlinkPath) -> Result<(), DeleteError> {
        let body = serde_json::json!({ "path": path.path });
        self.with(&path.key_id, |s| {
            s.rpc.call("delete_path", body.clone()).map(drop)
        })
        .map_err(deleted)
    }

    /// The peer moves it as its own Move to would (`move_path` RPC); one device only.
    fn move_path(
        &self,
        from: &FlinkPath,
        to: &FlinkPath,
        replace: bool,
    ) -> Result<(), DeleteError> {
        if from.key_id != to.key_id {
            return Err(DeleteError::Refused(
                "a file is moved on its own device only".to_string(),
            ));
        }
        let body = serde_json::json!({ "from": from.path, "to": to.path, "replace": replace });
        let moved = self.with(&from.key_id, |s| {
            s.rpc.call("move_path", body.clone()).map(drop)
        });
        match moved.map_err(deleted) {
            // The retry of a move the peer did before the link broke finds `from` gone: `to`
            // there and `from` not means it moved.
            Err(DeleteError::Missing(_)) if self.stat(to).is_ok() && self.stat(from).is_err() => {
                Ok(())
            }
            r => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::{Connect, Rpc};
    use ps5_dump_forge_flink::refusal;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-fs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// `b` paired with `a`, which shares `share`: a control session from b to a.
    fn paired(tag: &str, share: &Path) -> (Connect, Connect, Rpc, Handshake, [PathBuf; 2]) {
        paired_within(tag, share, None)
    }

    /// [`paired`], `a` reaching only `reach` within its share (as the PS5 does).
    fn paired_within(
        tag: &str,
        share: &Path,
        reach: Option<crate::paths::Roots>,
    ) -> (Connect, Connect, Rpc, Handshake, [PathBuf; 2]) {
        let dirs = [temp_dir(&format!("{tag}-a")), temp_dir(&format!("{tag}-b"))];
        let share = Some(share.to_path_buf());
        let a = Connect::start_confined(&dirs[0], 0, share, reach).unwrap();
        let b = Connect::start_rooted(&dirs[1], 0, None).unwrap();
        let sa = a.state();
        let o =
            crate::connect::pair_for_test(&b, &a, &format!("127.0.0.1:{}", sa.device.port), true);
        assert_eq!(o.state, "paired", "{o:?}");
        let (rpc, hs) = b.session(&sa.device.key_id).unwrap();
        (a, b, rpc, hs, dirs)
    }

    fn call(rpc: &mut Rpc, method: &str, path: &str) -> io::Result<Value> {
        rpc.call(method, serde_json::json!({ "path": path }))
    }

    /// The PS5's reach (by user request): `/` lists `data` (a link here, shown as the folder)
    /// and `mnt` while a drive is mounted, `mnt` only the mounted drives, and every RPC refuses
    /// a path elsewhere.
    #[cfg(unix)]
    #[test]
    fn a_ps5_share_reaches_only_data_and_mounted_drives() {
        let share = temp_dir("reach");
        for d in ["user/data/g", "mnt/usb0", "mnt/usb1", "system"] {
            fs::create_dir_all(share.join(d)).unwrap();
        }
        let share = dunce::canonicalize(&share).unwrap();
        std::os::unix::fs::symlink(share.join("user/data"), share.join("data")).unwrap();
        fs::write(share.join("user/data/g/f"), b"game").unwrap();
        fs::write(share.join("system/secret"), b"no").unwrap();
        let roots = [
            (share.join("data"), false),
            (share.join("mnt/usb0"), true),
            (share.join("mnt/usb1"), true),
        ];
        let reach = crate::paths::Roots::fake(&roots, crate::paths::fake_statfs);
        let (a, b, mut rpc, _hs, dirs) = paired_within("reach", &share, Some(reach));
        let names = |rpc: &mut Rpc, path: &str| -> Vec<FsEntry> {
            let v = call(rpc, "fs_list", path).unwrap();
            serde_json::from_value(v["entries"].clone()).unwrap()
        };
        let listed = names(&mut rpc, "");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].name == "data" && listed[0].stat.dir && !listed[0].stat.link);
        let data: FsStat = serde_json::from_value(call(&mut rpc, "stat", "data").unwrap()).unwrap();
        assert!(data.dir && !data.link);
        assert_eq!(names(&mut rpc, "data/g")[0].name, "f");
        let refused = |rpc: &mut Rpc, method: &str, body: Value| {
            let e = rpc.call(method, body).unwrap_err().to_string();
            assert!(
                e.contains("is not on this console's storage or a plugged-in drive"),
                "{method}: {e}"
            );
        };
        let at = |p: &str| serde_json::json!({ "path": p });
        for m in [
            "stat",
            "fs_list",
            "read_open",
            "mkdir",
            "create",
            "remove",
            "free",
        ] {
            refused(&mut rpc, m, at("system/secret"));
        }
        refused(&mut rpc, "reattach", at("system/x.part"));
        refused(&mut rpc, "stat", at("mnt"));
        refused(&mut rpc, "fs_list", at("mnt/usb0"));
        for m in ["create", "mkdir", "free"] {
            refused(&mut rpc, m, at(""));
        }
        let to = serde_json::json!({ "from": "data/g/f", "to": "system/f", "replace": true });
        refused(&mut rpc, "rename", to);
        assert!(call(&mut rpc, "read_open", "data/g/f").is_ok());
        assert!(call(&mut rpc, "free", "data").is_ok());
        // `free` of a link inside the roots that leads out of them is refused too.
        std::os::unix::fs::symlink(share.join("system"), share.join("user/data/out")).unwrap();
        refused(&mut rpc, "free", at("data/out"));

        // A drive plugged in: `mnt` appears, holding it alone.
        fs::write(share.join("mnt/usb0/mounted"), b"").unwrap();
        let listed: Vec<String> = names(&mut rpc, "").into_iter().map(|e| e.name).collect();
        assert_eq!(listed, ["data", "mnt"]);
        let listed: Vec<String> = names(&mut rpc, "mnt").into_iter().map(|e| e.name).collect();
        assert_eq!(listed, ["usb0"]);
        assert!(call(&mut rpc, "mkdir", "mnt/usb0/out").is_ok());
        refused(&mut rpc, "mkdir", at("mnt/usb1/out"));
        refused(&mut rpc, "mkdir", at("mnt/out"));
        drop((rpc, a, b));
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn stat_list_and_ranged_reads() {
        let share = temp_dir("share");
        let outside = temp_dir("outside");
        fs::create_dir_all(share.join("g/d")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), b"no").unwrap();
        let data: Vec<u8> = (0..CHUNK + 100).map(|i| (i * 7 % 251) as u8).collect();
        fs::write(share.join("g/big.bin"), &data).unwrap();
        fs::write(share.join("g/empty"), b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("big.bin", share.join("g/ln")).unwrap();
            symlink(&outside, share.join("g/out")).unwrap();
            let fifo = std::ffi::CString::new(share.join("g/fifo").to_str().unwrap()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        }
        let (a, b, mut rpc, hs, dirs) = paired("rw", &share);

        // stat: a file, a folder, the root; a link reported, not followed.
        let st: FsStat =
            serde_json::from_value(call(&mut rpc, "stat", "g/big.bin").unwrap()).unwrap();
        assert_eq!(
            (st.dir, st.size, st.link),
            (false, data.len() as u64, false)
        );
        assert!(st.mtime_ns.is_some());
        let root: FsStat = serde_json::from_value(call(&mut rpc, "stat", "").unwrap()).unwrap();
        assert!(root.dir);

        // fs_list: names sorted, the list's mtime the stat's.
        let v = call(&mut rpc, "fs_list", "g").unwrap();
        let entries: Vec<FsEntry> = serde_json::from_value(v["entries"].clone()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        let big = entries.iter().find(|e| e.name == "big.bin").unwrap();
        assert_eq!(big.stat, st);
        assert!(entries.iter().find(|e| e.name == "d").unwrap().stat.dir);
        #[cfg(unix)]
        {
            assert_eq!(names, ["big.bin", "d", "empty", "fifo", "ln", "out"]);
            let fifo = entries.iter().find(|e| e.name == "fifo").unwrap();
            assert!(fifo.special && !fifo.bad_name && fifo.stat.size == 0);
            assert!(entries.iter().filter(|e| e.special || e.bad_name).count() == 1);
            let ln: FsStat =
                serde_json::from_value(call(&mut rpc, "stat", "g/ln").unwrap()).unwrap();
            assert_eq!((ln.link, ln.dir, ln.size), (true, false, 0));
            let out = entries.iter().find(|e| e.name == "out").unwrap();
            assert!(out.stat.link && !out.stat.dir);
            let e = call(&mut rpc, "stat", "g/fifo").unwrap_err();
            assert!(e.to_string().contains("not a file or folder"), "{e}");
            // A link is never read or listed through; one leading outside is refused as a folder.
            assert!(call(&mut rpc, "read_open", "g/ln").is_err());
            assert!(call(&mut rpc, "read_open", "g/fifo").is_err());
            assert!(call(&mut rpc, "fs_list", "g/out").is_err());
            let e = call(&mut rpc, "stat", "g/out/secret").unwrap_err();
            assert!(e.to_string().contains("outside the shared folder"), "{e}");
            let e = call(&mut rpc, "read_open", "g/out/secret").unwrap_err();
            assert!(e.to_string().contains("outside the shared folder"), "{e}");
        }
        #[cfg(not(unix))]
        assert_eq!(names, ["big.bin", "d", "empty"]);
        for bad in ["../x", "g/../g/big.bin", "/etc", "g//big.bin"] {
            assert!(call(&mut rpc, "stat", bad).is_err(), "{bad}");
            assert!(call(&mut rpc, "read_open", bad).is_err(), "{bad}");
        }
        assert!(call(&mut rpc, "read_open", "g/d").is_err(), "a folder");
        assert!(call(&mut rpc, "fs_list", "g/big.bin").is_err(), "a file");
        assert!(call(&mut rpc, "stat", "g/nope").is_err());

        // read_open + ranged reads over a lane: across the 8 MiB boundary, at and past EOF.
        let o = call(&mut rpc, "read_open", "g/big.bin").unwrap();
        assert_eq!(o["size"], data.len() as u64);
        assert_eq!(o["mtime_ns"], serde_json::json!(st.mtime_ns));
        let h = o["handle"].as_u64().unwrap();
        let mut lane = join_lane(&hs, 0).unwrap();
        let n = data.len() as u64;
        let at = CHUNK - 50;
        assert_eq!(
            read_range(&mut lane, h, at, 100).unwrap(),
            &data[at as usize..][..100]
        );
        assert_eq!(
            read_range(&mut lane, h, 0, CHUNK).unwrap(),
            &data[..CHUNK as usize]
        );
        // Pipelined: two asks, two answers in order.
        send_read(&mut lane, h, n - 10, 64).unwrap();
        send_read(&mut lane, h, n, 64).unwrap();
        assert_eq!(
            recv_read(&mut lane, h, n - 10, 64).unwrap(),
            &data[n as usize - 10..]
        );
        assert!(recv_read(&mut lane, h, n, 64).unwrap().is_empty());
        // A second lane joins under the same session.
        let mut lane2 = join_lane(&hs, 1).unwrap();
        assert_eq!(read_range(&mut lane2, h, 1, 3).unwrap(), &data[1..4]);
        // Too long a read fails the lane.
        let e = read_range(&mut lane2, h, 0, CHUNK + 1).unwrap_err();
        assert!(e.to_string().starts_with(TRANSFER_FAILED), "{e}");

        // An empty file reads as nothing; a closed handle is refused.
        let o = call(&mut rpc, "read_open", "g/empty").unwrap();
        let he = o["handle"].as_u64().unwrap();
        assert!(read_range(&mut lane, he, 0, 10).unwrap().is_empty());
        rpc.call("read_close", serde_json::json!({ "handle": he }))
            .unwrap();
        assert!(
            rpc.call("read_close", serde_json::json!({ "handle": he }))
                .is_err()
        );
        let e = read_range(&mut lane, he, 0, 10).unwrap_err();
        assert!(
            refusal(&e).is_some() || e.to_string().starts_with(TRANSFER_FAILED),
            "{e}"
        );

        // Another session can't use the handle, nor join a lane once the session ends.
        let (mut rpc2, hs2) = b.session(&a.state().device.key_id).unwrap();
        assert!(
            rpc2.call("read_close", serde_json::json!({ "handle": h }))
                .is_err()
        );
        drop(rpc);
        for _ in 0..100 {
            if join_lane(&hs, 2).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            join_lane(&hs, 3).is_err(),
            "the session's handles closed with it"
        );
        drop((rpc2, hs2));

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share, &outside]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A name that isn't UTF-8 is listed, marked (Linux: macOS filesystems refuse such names).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_bad_name_is_listed_marked() {
        use std::os::unix::ffi::OsStrExt;
        let share = temp_dir("badname");
        fs::create_dir_all(&share).unwrap();
        fs::write(share.join(std::ffi::OsStr::from_bytes(b"bad\xff")), b"x").unwrap();
        let entries = list(Some(&Share::new(share.clone(), None)), "").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].bad_name && !entries[0].special);
        assert_eq!(entries[0].name, "bad\u{fffd}");
        let _ = fs::remove_dir_all(&share);
    }

    /// `rename` with `replace`: over a regular file only, never a folder, a link or a path
    /// this device's jobs use; without it a taken name is refused as ever.
    #[test]
    fn rename_replaces_a_file_only_when_asked() {
        let share = temp_dir("rshare");
        fs::create_dir_all(share.join("g/dir")).unwrap();
        fs::write(share.join("g/exists"), b"old").unwrap();
        fs::write(share.join("g/busy"), b"old busy").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(share.join("g/exists"), share.join("g/link")).unwrap();
        let (a, b, mut rpc, _hs, dirs) = paired("r", &share);
        a.set_busy(Arc::new(|p: &Path| p.ends_with("busy")));
        let rename = |rpc: &mut Rpc, from: &str, to: &str, replace: bool| {
            let body = serde_json::json!({ "from": from, "to": to, "replace": replace });
            rpc.call("rename", body)
        };
        let made = |rpc: &mut Rpc, at: &str| {
            let h = call(rpc, "create", at).unwrap()["handle"].as_u64().unwrap();
            rpc.call("close", serde_json::json!({ "handle": h }))
                .unwrap();
        };

        made(&mut rpc, "g/f.part");
        let e = rename(&mut rpc, "g/f.part", "g/exists", false).unwrap_err();
        assert!(e.to_string().contains("exists"), "{e}");
        for (to, why) in [("g/dir", "not a file"), ("g/busy", "in use by a job on")] {
            let e = rename(&mut rpc, "g/f.part", to, true).unwrap_err();
            assert!(e.to_string().contains(why), "{to}: {e}");
        }
        #[cfg(unix)]
        {
            let e = rename(&mut rpc, "g/f.part", "g/link", true).unwrap_err();
            assert!(e.to_string().contains("not a file"), "{e}");
            assert!(
                fs::symlink_metadata(share.join("g/link"))
                    .unwrap()
                    .is_symlink()
            );
        }
        assert!(share.join("g/dir").is_dir());
        assert_eq!(fs::read(share.join("g/busy")).unwrap(), b"old busy");

        rename(&mut rpc, "g/f.part", "g/exists", true).unwrap();
        assert_eq!(fs::read(share.join("g/exists")).unwrap(), b"");
        assert!(!share.join("g/f.part").exists());
        // Renamed, it is the device's: never renamed again, replacing or not.
        assert!(rename(&mut rpc, "g/exists", "g/other", true).is_err());
        // To a free name, replace or not.
        made(&mut rpc, "g/n.part");
        rename(&mut rpc, "g/n.part", "g/new", true).unwrap();
        assert!(share.join("g/new").is_file());

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A replacing rename never goes over a file another peer is writing: open, closed and
    /// unrenamed, or kept for its reattach.
    #[test]
    fn a_replace_never_takes_another_peers_unfinished_file() {
        let share = temp_dir("r2share");
        fs::create_dir_all(&share).unwrap();
        let (a, b, mut rpc, _hs, dirs) = paired("r2", &share);
        let key = a.state().device.key_id;
        let dir_c = temp_dir("r2-c");
        let c = Connect::start_rooted(&dir_c, 0, None).unwrap();
        let addr = format!("127.0.0.1:{}", a.state().device.port);
        let o = crate::connect::pair_for_test(&c, &a, &addr, true);
        assert_eq!(o.state, "paired", "{o:?}");
        let (mut theirs, hs_c) = c.session(&key).unwrap();
        *lock(&a.writes().kept_for) = Some(Duration::from_secs(30));

        let h = call(&mut theirs, "create", "x.part").unwrap()["handle"]
            .as_u64()
            .unwrap();
        let mut lane = join_lane(&hs_c, 0).unwrap();
        send_write(&mut lane, h, 0, b"theirs").unwrap();
        recv_written(&mut lane, h, 0).unwrap();
        let mine = call(&mut rpc, "create", "y.part").unwrap()["handle"]
            .as_u64()
            .unwrap();
        rpc.call("close", serde_json::json!({ "handle": mine }))
            .unwrap();
        let replace = |rpc: &mut Rpc| {
            let body = serde_json::json!({ "from": "y.part", "to": "x.part", "replace": true });
            rpc.call("rename", body).unwrap_err()
        };
        let e = replace(&mut rpc);
        assert!(
            e.to_string().contains("being written by a paired device"),
            "open: {e}"
        );
        theirs
            .call("close", serde_json::json!({ "handle": h }))
            .unwrap();
        let e = replace(&mut rpc);
        assert!(
            e.to_string().contains("being written by a paired device"),
            "closed: {e}"
        );
        drop((theirs, lane));
        std::thread::sleep(Duration::from_millis(300));
        let e = replace(&mut rpc);
        assert!(
            e.to_string().contains("being written by a paired device"),
            "kept: {e}"
        );
        assert_eq!(fs::read(share.join("x.part")).unwrap(), b"theirs");
        assert!(share.join("y.part").exists());

        a.shutdown();
        b.shutdown();
        c.shutdown();
        for d in dirs.iter().chain([&share, &dir_c]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A replacing rename passes core's `Jobs::replace_check` first (Connect's hook): an image
    /// with an interrupted edit's undo journal beside it, and the journal, are never replaced.
    #[test]
    fn a_replace_is_refused_where_a_jobs_replace_would_be() {
        let share = temp_dir("r3share");
        fs::create_dir_all(share.join("g")).unwrap();
        fs::write(share.join("g/game.exfat"), b"image").unwrap();
        fs::write(share.join("g/game.exfat.forge-undo"), b"journal").unwrap();
        fs::write(share.join("g/plain"), b"old").unwrap();
        let (a, b, mut rpc, _hs, dirs) = paired("r3", &share);
        let jobs = Arc::new(ps5_dump_forge_core::Jobs::new(|_| {}));
        let j = jobs.clone();
        a.set_replace_check(Arc::new(move |p| {
            j.replace_check(p).map_err(|e| e.to_string())
        }));
        let made = |rpc: &mut Rpc, at: &str| {
            let h = call(rpc, "create", at).unwrap()["handle"].as_u64().unwrap();
            rpc.call("close", serde_json::json!({ "handle": h }))
                .unwrap();
        };
        let rename = |rpc: &mut Rpc, to: &str| {
            let body = serde_json::json!({ "from": "g/f.part", "to": to, "replace": true });
            rpc.call("rename", body)
        };

        made(&mut rpc, "g/f.part");
        for (to, why) in [
            ("g/game.exfat", "interrupted edit is pending"),
            ("g/game.exfat.forge-undo", "undo journal"),
        ] {
            let e = rename(&mut rpc, to).unwrap_err();
            assert!(e.to_string().contains(why), "{to}: {e}");
        }
        assert_eq!(fs::read(share.join("g/game.exfat")).unwrap(), b"image");
        assert_eq!(
            fs::read(share.join("g/game.exfat.forge-undo")).unwrap(),
            b"journal"
        );
        // A plain file, and a free name, pass.
        rename(&mut rpc, "g/plain").unwrap();
        assert_eq!(fs::read(share.join("g/plain")).unwrap(), b"");
        made(&mut rpc, "g/f.part");
        rename(&mut rpc, "g/new").unwrap();
        assert!(share.join("g/new").is_file());

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// What the quits see (`Connect::activity_hook`, `peers_active`): a file a peer created and
    /// hasn't renamed, kept for a reattach too, and a file it holds open for reading.
    #[test]
    fn a_peers_open_files_count_as_activity() {
        let share = temp_dir("actshare");
        fs::create_dir_all(&share).unwrap();
        fs::write(share.join("game.bin"), b"game").unwrap();
        let (a, b, mut rpc, _hs, dirs) = paired("act", &share);
        let active = a.activity_hook();
        assert!(!active() && !a.peers_active());

        let h = call(&mut rpc, "create", "f.part").unwrap()["handle"]
            .as_u64()
            .unwrap();
        assert!(active() && a.peers_active());
        rpc.call("close", serde_json::json!({ "handle": h }))
            .unwrap();
        assert!(active(), "unrenamed: still the peer's");
        rpc.call("rename", serde_json::json!({ "from": "f.part", "to": "f" }))
            .unwrap();
        assert!(!active());

        let h = call(&mut rpc, "read_open", "game.bin").unwrap()["handle"]
            .as_u64()
            .unwrap();
        assert!(active());
        rpc.call("read_close", serde_json::json!({ "handle": h }))
            .unwrap();
        assert!(!active());

        // A session that ends with a file unrenamed: active while it is kept for a reattach.
        *lock(&a.writes().kept_for) = Some(Duration::from_secs(2));
        call(&mut rpc, "create", "g.part").unwrap();
        drop(rpc);
        std::thread::sleep(Duration::from_millis(500));
        a.writes().sweep();
        assert!(active(), "kept for a reattach");
        let wait = Instant::now() + Duration::from_secs(10);
        while share.join("g.part").exists() {
            assert!(Instant::now() < wait, "never removed");
            a.writes().sweep();
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!active());

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn writes_create_rename_and_clean_up() {
        let share = temp_dir("wshare");
        let outside = temp_dir("woutside");
        fs::create_dir_all(share.join("g")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(share.join("g/exists"), b"old").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, share.join("g/out")).unwrap();
        let (a, b, mut rpc, hs, dirs) = paired("w", &share);
        let handle = |v: Value| v["handle"].as_u64().unwrap();
        let two = |rpc: &mut Rpc, m: &str, from: &str, to: &str| {
            rpc.call(m, serde_json::json!({ "from": from, "to": to }))
        };

        // mkdir: one level; an existing folder is fine, anything else is not.
        assert_eq!(call(&mut rpc, "mkdir", "g/new").unwrap()["created"], true);
        assert_eq!(call(&mut rpc, "mkdir", "g/new").unwrap()["created"], false);
        assert!(call(&mut rpc, "mkdir", "g/exists").is_err());
        assert!(call(&mut rpc, "mkdir", "g/no/deeper").is_err());

        // create is exclusive.
        let h = handle(call(&mut rpc, "create", "g/new/f.part").unwrap());
        assert!(call(&mut rpc, "create", "g/new/f.part").is_err());
        let e = call(&mut rpc, "create", "g/exists").unwrap_err();
        assert!(e.to_string().contains("exists"), "{e}");
        // This device's jobs see it as written: the file, its folders, nothing beside it.
        let writing = a.writing_hook();
        assert!(writing(&share.join("g/new/f.part")) && writing(&share.join("g")));
        assert!(!writing(&share.join("g/exists")) && !writing(&outside));

        // Receiving a peer's file is busy even though it isn't a local job. Delete and Move
        // protect the file, its parents and a move's destination; unrelated entries work.
        let jobs = ps5_dump_forge_core::Jobs::new(|_| {});
        jobs.set_external_busy(a.writing_hook());
        let busy = |r| assert!(matches!(r, Err(ps5_dump_forge_core::DeleteError::Busy(_))));
        busy(jobs.delete_path(&share.join("g/new/f.part"), &[]));
        busy(jobs.delete_path(&share.join("g/new"), &[]));
        busy(jobs.move_path(&share.join("g/new"), &share.join("moved"), false, &[]));
        busy(jobs.move_path(
            &share.join("g/exists"),
            &share.join("g/new/f.part"),
            false,
            &[],
        ));
        fs::write(share.join("spare"), b"spare").unwrap();
        jobs.move_path(&share.join("spare"), &share.join("moved"), false, &[])
            .unwrap();
        jobs.delete_path(&share.join("moved"), &[]).unwrap();
        assert!(share.join("g/new/f.part").exists());

        // Writes across chunks, pipelined on a lane, then sync, close, rename.
        let data: Vec<u8> = (0..CHUNK + 100).map(|i| (i * 13 % 251) as u8).collect();
        let mut lane = join_lane(&hs, 0).unwrap();
        let cut = CHUNK as usize;
        send_write(&mut lane, h, 0, &data[..cut]).unwrap();
        send_write(&mut lane, h, CHUNK, &data[cut..]).unwrap();
        recv_written(&mut lane, h, 0).unwrap();
        recv_written(&mut lane, h, CHUNK).unwrap();
        rpc.call("sync", serde_json::json!({ "handle": h }))
            .unwrap();
        rpc.call("close", serde_json::json!({ "handle": h }))
            .unwrap();
        assert!(
            rpc.call("sync", serde_json::json!({ "handle": h }))
                .is_err()
        );
        let e = two(&mut rpc, "rename", "g/new/f.part", "g/exists").unwrap_err();
        assert!(e.to_string().contains("exists"), "{e}");
        two(&mut rpc, "rename", "g/new/f.part", "g/new/f").unwrap();
        assert_eq!(fs::read(share.join("g/new/f")).unwrap(), data);
        assert!(!writing(&share.join("g")));
        assert_eq!(fs::read(share.join("g/exists")).unwrap(), b"old");
        // Renamed, it is the device's: no second rename, no remove.
        assert!(two(&mut rpc, "rename", "g/new/f", "g/new/f2").is_err());
        assert!(call(&mut rpc, "remove", "g/new/f").is_err());
        assert!(call(&mut rpc, "remove", "g/exists").is_err());
        // A closed handle can't be written; a write on a refused lane ends it.
        send_write(&mut lane, h, 0, b"x").unwrap();
        assert!(recv_written(&mut lane, h, 0).is_err());

        // remove: a file this session created. An open one can't be renamed.
        call(&mut rpc, "create", "g/new/gone.part").unwrap();
        let e = two(&mut rpc, "rename", "g/new/gone.part", "g/new/gone").unwrap_err();
        assert!(e.to_string().contains("still open"), "{e}");
        call(&mut rpc, "remove", "g/new/gone.part").unwrap();
        assert!(!share.join("g/new/gone.part").exists());

        // Escapes: `..`, an absolute path, a link to outside the share.
        for bad in ["../x", "/etc/x", "g/../../x"] {
            assert!(call(&mut rpc, "create", bad).is_err(), "{bad}");
            assert!(call(&mut rpc, "mkdir", bad).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            let e = call(&mut rpc, "create", "g/out/x").unwrap_err();
            assert!(e.to_string().contains("outside"), "{e}");
            assert!(call(&mut rpc, "mkdir", "g/out/y").is_err());
            assert!(fs::read_dir(&outside).unwrap().next().is_none());
        }

        // Free space of a folder.
        let free = call(&mut rpc, "free", "g").unwrap()["free"].as_u64();
        assert!(free.is_some_and(|n| n > 0));

        // The busy hook refuses what this device's jobs use.
        a.set_busy(Arc::new(|p: &Path| p.ends_with("busy")));
        let e = call(&mut rpc, "create", "g/busy").unwrap_err();
        assert!(e.to_string().contains("in use by a job on"), "{e}");
        assert!(call(&mut rpc, "mkdir", "g/busy").is_err());
        assert!(two(&mut rpc, "rename", "g/new/f", "g/busy").is_err());

        // A session that ends keeps what it created and never renamed for a while (its writer
        // may resume it), busy for this device's jobs meanwhile; then it is removed.
        *lock(&a.writes().kept_for) = Some(Duration::from_secs(2));
        let o = handle(call(&mut rpc, "create", "g/new/orphan.part").unwrap());
        let mut lane2 = join_lane(&hs, 1).unwrap();
        send_write(&mut lane2, o, 0, b"half").unwrap();
        recv_written(&mut lane2, o, 0).unwrap();
        assert!(share.join("g/new/orphan.part").exists());
        drop(rpc);
        std::thread::sleep(Duration::from_millis(500));
        a.writes().sweep();
        assert!(share.join("g/new/orphan.part").exists());
        assert!(writing(&share.join("g/new/orphan.part")));
        // Its old session's lane can't write it any more.
        send_write(&mut lane2, o, 4, b"more").unwrap();
        assert!(recv_written(&mut lane2, o, 4).is_err());
        let start = Instant::now();
        while share.join("g/new/orphan.part").exists() && start.elapsed() < Duration::from_secs(10)
        {
            a.writes().sweep();
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            start.elapsed() >= Duration::from_millis(1000),
            "kept a while"
        );
        assert!(!share.join("g/new/orphan.part").exists());
        assert!(!writing(&share.join("g/new")));
        assert!(share.join("g/new/f").exists());

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share, &outside]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn forgetting_revokes_cached_sessions_and_lanes() {
        use ps5_dump_forge_core::remote::Remote;

        // Forget from either end: the receiving service closes its inbound sockets, and the
        // initiating service refuses its own cached connection and open write handles.
        for local in [false, true] {
            let tag = format!("forget-{local}");
            let share = temp_dir(&tag);
            fs::create_dir_all(&share).unwrap();
            fs::write(share.join("read.bin"), b"original").unwrap();
            let (a, b, mut rpc, hs, dirs) = paired(&tag, &share);
            // Forgotten on the side written to, unfinished files go at once; forgotten on the
            // writer's, that side can't tell: kept for a resume as after a lost link.
            if local {
                *lock(&a.writes().kept_for) = Some(Duration::from_millis(200));
            }
            let h = call(&mut rpc, "create", "raw.part").unwrap()["handle"]
                .as_u64()
                .unwrap();
            let mut lane = join_lane(&hs, 0).unwrap();
            send_write(&mut lane, h, 0, b"before").unwrap();
            recv_written(&mut lane, h, 0).unwrap();
            let r = call(&mut rpc, "read_open", "read.bin").unwrap()["handle"]
                .as_u64()
                .unwrap();
            let mut read_lane = join_lane(&hs, 1).unwrap();
            assert_eq!(read_range(&mut read_lane, r, 0, 8).unwrap(), b"original");

            let remote = b.remote();
            let at = |p: &str| {
                FlinkPath::parse(&PathBuf::from(format!("flink://{}/{p}", a.me().key_id))).unwrap()
            };
            let open = remote.create(&at("cached.part")).unwrap();
            remote.write_at(open, 0, b"before").unwrap();
            assert_eq!(remote.read_at(&at("read.bin"), 0, 8).unwrap(), b"original");
            if local {
                b.forget(&a.me().key_id).unwrap();
                assert!(call(&mut rpc, "stat", "read.bin").is_err());
            } else {
                a.forget(&b.me().key_id).unwrap();
                assert!(call(&mut rpc, "stat", "read.bin").is_err());
                assert!(
                    send_write(&mut lane, h, 0, b"after")
                        .and_then(|()| recv_written(&mut lane, h, 0))
                        .is_err()
                );
                assert!(read_range(&mut read_lane, r, 0, 8).is_err());
                assert!(join_lane(&hs, 2).is_err());
            }
            assert!(remote.write_at(open, 0, b"after").is_err());
            assert!(remote.read_at(&at("read.bin"), 0, 8).is_err());
            assert!(remote.create(&at("after.part")).is_err());
            assert!(!share.join("after.part").exists());
            assert_eq!(fs::read(share.join("read.bin")).unwrap(), b"original");
            let _ = remote.close(open);
            let _ = remote.remove(&at("cached.part"));
            drop(rpc);
            let until = Instant::now() + Duration::from_secs(5);
            while share.join("raw.part").exists() || share.join("cached.part").exists() {
                assert!(Instant::now() < until, "unfinished peer files were kept");
                a.writes().sweep();
                std::thread::sleep(Duration::from_millis(10));
            }
            if !local {
                // Re-pairing permits a fresh session; the old session's secret and lanes
                // still cannot be used, even though the peer's key id is paired again.
                let paired = crate::connect::pair_for_test(
                    &b,
                    &a,
                    &format!("127.0.0.1:{}", a.me().port),
                    true,
                );
                assert_eq!(paired.state, "paired");
                assert!(join_lane(&hs, 3).is_err());
                assert_eq!(remote.read_at(&at("read.bin"), 0, 8).unwrap(), b"original");
            }
            a.shutdown();
            b.shutdown();
            for d in dirs.iter().chain([&share]) {
                fs::remove_dir_all(d).unwrap();
            }
        }
    }

    #[test]
    fn a_peer_game_is_read_through_core() {
        use ps5_dump_forge_core::remote::{Remote, RemoteReader, RemoteTree};
        use std::io::{Read, Seek, SeekFrom};
        let share = temp_dir("core-share");
        let game = share.join("game");
        fs::create_dir_all(game.join("sce_sys")).unwrap();
        fs::create_dir_all(game.join("data/empty")).unwrap();
        fs::write(game.join("eboot.bin"), b"\x7fELF fake eboot").unwrap();
        fs::write(
            game.join("sce_sys/param.json"),
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
        )
        .unwrap();
        let big: Vec<u8> = (0..2 * CHUNK + 5).map(|i| (i * 13 % 251) as u8).collect();
        fs::write(game.join("data/big.bin"), &big).unwrap();
        let (a, b, rpc, _, dirs) = paired("core", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let at = |rel: &str| FlinkPath::parse(Path::new(&format!("flink://{key}/{rel}"))).unwrap();
        // Built directly: other tests' services replace nothing here.
        let r: Arc<dyn Remote> = b.remote();

        let st = r.stat(&at("game/data/big.bin")).unwrap();
        assert_eq!((st.dir, st.size), (false, big.len() as u64));
        let listed = r.list(&at("game/data")).unwrap();
        let e = listed.iter().find(|e| e.name == "big.bin").unwrap();
        assert_eq!((e.size, e.mtime, e.link), (st.size, st.mtime, false));
        let e = r.stat(&at("game/nothing")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound, "{e}");
        assert!(
            r.read_at(&at("game/data"), 0, 1).is_err(),
            "a folder isn't read"
        );

        // read_at: across chunks in one call, at the end, from several threads at once.
        let p = at("game/data/big.bin");
        let n = CHUNK as usize + 9;
        assert_eq!(r.read_at(&p, 3, n).unwrap(), big[3..3 + n]);
        // Pieces on several lanes, put back in order; past the end, what there is.
        assert_eq!(r.read_at(&p, 0, big.len()).unwrap(), big);
        assert_eq!(r.read_at(&p, 1, 3 * CHUNK as usize).unwrap(), big[1..]);
        assert_eq!(
            r.read_at(&p, big.len() as u64 - 2, 10).unwrap(),
            big[big.len() - 2..]
        );
        std::thread::scope(|s| {
            for t in 0..4u64 {
                let (r, p, big) = (r.clone(), p.clone(), &big);
                s.spawn(move || {
                    let o = t * 1_000_003;
                    assert_eq!(
                        r.read_at(&p, o, 1 << 20).unwrap(),
                        big[o as usize..][..1 << 20]
                    );
                });
            }
        });
        let mut rd = RemoteReader::open(r.clone(), &p).unwrap();
        rd.seek(SeekFrom::Start(CHUNK - 4)).unwrap();
        let mut buf = vec![0; 100];
        rd.read_exact(&mut buf).unwrap();
        assert_eq!(buf, big[CHUNK as usize - 4..][..100]);
        // A link is never opened as a (0-byte) file.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("game/data/big.bin", share.join("ln")).unwrap();
            assert!(r.stat(&at("ln")).unwrap().link);
            let e = RemoteReader::open(r.clone(), &at("ln")).err().unwrap();
            assert!(e.to_string().contains("a link"), "{e}");
        }

        // A file replaced since its handle was kept: the new bytes, not the old file's.
        let small = at("game/sce_sys/param.json");
        r.read_at(&small, 0, 4).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            game.join("sce_sys/param.json.new"),
            b"{\"titleId\":\"PPSA09999\"} ",
        )
        .unwrap();
        fs::rename(
            game.join("sce_sys/param.json.new"),
            game.join("sce_sys/param.json"),
        )
        .unwrap();
        r.stat(&small).unwrap();
        assert_eq!(
            r.read_at(&small, 0, 23).unwrap(),
            b"{\"titleId\":\"PPSA09999\"}"
        );
        fs::write(
            game.join("sce_sys/param.json"),
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
        )
        .unwrap();

        // A read by path (no handle kept): the bytes and the stat after it in one round trip;
        // links and folders are refused as `read_open` refuses them.
        let eboot = at("game/eboot.bin");
        let (bytes, st) = r.read_checked(&eboot, 5, 100).unwrap();
        assert_eq!(
            (bytes.as_slice(), st),
            (&b"fake eboot"[..], r.stat(&eboot).unwrap())
        );
        #[cfg(unix)]
        assert!(r.read_checked(&at("ln"), 0, 4).is_err(), "a link");
        assert!(r.read_checked(&at("game/data"), 0, 4).is_err(), "a folder");
        assert_eq!(r.read_checked(&eboot, 0, 4).unwrap().0, b"\x7fELF");

        // The tree core converts from, and core's own inspect through the registered service.
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut tree = RemoteTree::open(r.clone(), &at("game"), &cancel).unwrap();
        use ps5upload_fpkg::source::SourceTree;
        let mut names: Vec<String> = tree.files().iter().map(|f| f.path.clone()).collect();
        names.sort();
        assert_eq!(names, ["data/big.bin", "eboot.bin", "sce_sys/param.json"]);
        assert_eq!(tree.read_range("data/big.bin", 0, big.len()).unwrap(), big);
        let url = PathBuf::from(format!("flink://{key}/game"));
        let i = ps5_dump_forge_core::inspect(&url).unwrap();
        assert_eq!(i.title_id.as_deref(), Some("PPSA01234"));
        assert_eq!(i.empty_dirs, ["data/empty"]);

        // The device gone: a kept session fails over to a fresh dial, which names the device.
        a.shutdown();
        let e = r.stat(&p).unwrap_err();
        assert!(e.to_string().contains("can't be reached"), "{e}");
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }
    #[test]
    fn a_game_is_converted_onto_a_peer_through_core() {
        use ps5_dump_forge_core::{ConvertRequest, Event, JobReport, Jobs};
        use std::sync::mpsc;
        let share = temp_dir("conv-share");
        let src = temp_dir("conv-src");
        for game in [&share.join("game"), &src] {
            fs::create_dir_all(game.join("sce_sys")).unwrap();
            fs::create_dir_all(game.join("data")).unwrap();
            fs::write(game.join("eboot.bin"), b"\x7fELF fake eboot").unwrap();
            fs::write(
                game.join("sce_sys/param.json"),
                br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
            )
            .unwrap();
            let big: Vec<u8> = (0..CHUNK + 5).map(|i| (i * 7 % 251) as u8).collect();
            fs::write(game.join("data/big.bin"), big).unwrap();
        }
        fs::create_dir_all(share.join("out")).unwrap();
        let (a, b, rpc, _, dirs) = paired("conv", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let url = |rel: &str| PathBuf::from(format!("flink://{key}/{rel}"));
        let request = |source: &Path, format: &str, output: &Path| -> ConvertRequest {
            serde_json::from_value(serde_json::json!({
                "source": source, "format": format, "output": output, "full_verify": true
            }))
            .unwrap()
        };
        // This device's jobs, each waited for; core reaches the peer through the registered
        // dispatcher.
        let (tx, rx) = mpsc::channel::<Result<JobReport, String>>();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |e| {
            if let Event::Done { result, .. } = e {
                let _ = lock(&tx).send(result);
            }
        });
        let run = |req: ConvertRequest| {
            jobs.start(req);
            rx.recv_timeout(Duration::from_secs(120)).unwrap()
        };

        // A game here to an .exfat on the peer, fully verified as read back from it.
        let report = run(request(&src, "exfat", &url("out/game.exfat"))).unwrap();
        assert!(
            report.checks.iter().any(|c| c.starts_with("published")),
            "{report:?}"
        );
        assert!(share.join("out/game.exfat").is_file());
        let i = ps5_dump_forge_core::inspect(&url("out/game.exfat")).unwrap();
        assert_eq!(i.title_id.as_deref(), Some("PPSA01234"));
        // And a .pkg, the builder writing through FLINK (its spool and self-check read back
        // there), fully verified; the peer's file is a package core reads.
        let report = run(request(&src, "pkg", &url("out/game.pkg"))).unwrap();
        assert!(
            report.checks.iter().any(|c| c.starts_with("published")),
            "{report:?}"
        );
        let i = ps5_dump_forge_core::inspect(&share.join("out/game.pkg")).unwrap();
        assert_eq!(i.title_id.as_deref(), Some("PPSA01234"));

        // The peer's own unfinished job holds its output: the peer refuses to write there.
        let (hold, held) = mpsc::channel::<()>();
        let held = Mutex::new(held);
        let (done_tx, done) = mpsc::channel::<Result<JobReport, String>>();
        let done_tx = Mutex::new(done_tx);
        let peer_jobs = Jobs::new(move |e| match e {
            Event::Stages { .. } => {
                let _ = lock(&held).recv();
            }
            Event::Done { result, .. } => {
                let _ = lock(&done_tx).send(result);
            }
            _ => {}
        });
        a.set_busy(peer_jobs.uses_hook());
        let real = dunce::canonicalize(&share).unwrap();
        peer_jobs.start(request(
            &real.join("game"),
            "folder",
            &real.join("out/held"),
        ));
        let e = run(request(&src, "folder", &url("out/held"))).unwrap_err();
        assert!(e.contains("in use by a job"), "{e}");
        hold.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
        // Once it is done, a folder goes there as any other.
        let report = run(request(&src, "folder", &url("out/copy"))).unwrap();
        assert_eq!(report.output, url("out/copy"));
        assert_eq!(
            fs::read(share.join("out/copy/data/big.bin")).unwrap(),
            fs::read(src.join("data/big.bin")).unwrap()
        );
        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share, &src]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_peer_file_is_written_through_core() {
        use ps5_dump_forge_core::remote::RemoteReader;
        use std::io::Read;
        let share = temp_dir("wcore-share");
        fs::create_dir_all(&share).unwrap();
        fs::write(share.join("taken"), b"theirs").unwrap();
        let (a, b, rpc, _, dirs) = paired("wcore", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let at = |rel: &str| FlinkPath::parse(Path::new(&format!("flink://{key}/{rel}"))).unwrap();
        // Through core's dispatcher: handles must route back to this service.
        let r = remote::get().unwrap();

        r.mkdir(&at("out")).unwrap();
        let e = r.mkdir(&at("out")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists, "{e}");
        assert!(r.free_space(&at("out")).unwrap() > 0);

        // 20 MiB in odd pieces, some past CHUNK (dealt to several lanes).
        let total = 20 << 20;
        let data: Vec<u8> = (0..total).map(|i| (i * 7 % 253) as u8).collect();
        let h = r.create(&at("out/game.bin.part")).unwrap();
        let e = r.create(&at("out/game.bin.part")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists, "{e}");
        let sizes = [
            1,
            4093,
            CHUNK as usize + 3,
            65_537,
            2 * CHUNK as usize + 11,
            777_777,
        ];
        let mut at_ = 0;
        for n in sizes.iter().cycle() {
            if at_ == total {
                break;
            }
            let end = (at_ + n).min(total);
            r.write_at(h, at_ as u64, &data[at_..end]).unwrap();
            at_ = end;
        }
        r.sync(h).unwrap();
        r.close(h).unwrap();
        assert!(r.write_at(h, 0, b"x").is_err(), "a closed handle is gone");
        let e = r
            .rename_no_replace(&at("out/game.bin.part"), &at("taken"))
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists, "{e}");
        assert_eq!(fs::read(share.join("taken")).unwrap(), b"theirs");
        r.rename_no_replace(&at("out/game.bin.part"), &at("out/game.bin"))
            .unwrap();
        let mut back = Vec::new();
        RemoteReader::open(r.clone(), &at("out/game.bin"))
            .unwrap()
            .read_to_end(&mut back)
            .unwrap();
        assert!(back == data, "the file reads back as written");

        // Remove: a created file only (a renamed one is the peer's now).
        let h = r.create(&at("out/tmp.part")).unwrap();
        r.write_at(h, 0, b"partial").unwrap();
        r.close(h).unwrap();
        r.remove(&at("out/tmp.part")).unwrap();
        assert!(!share.join("out/tmp.part").exists());
        assert!(r.remove(&at("out/game.bin")).is_err());
        assert!(share.join("out/game.bin").exists());

        // Replacing: a created file over the one there, through core's dispatcher.
        let h = r.create(&at("out/new.part")).unwrap();
        r.write_at(h, 0, b"new").unwrap();
        r.close(h).unwrap();
        r.rename_replace(&at("out/new.part"), &at("taken")).unwrap();
        assert_eq!(fs::read(share.join("taken")).unwrap(), b"new");
        assert!(!share.join("out/new.part").exists());

        // The session lost: an open handle fails, saying the file went with it.
        let h = r.create(&at("out/lost.part")).unwrap();
        a.shutdown();
        let e = r.write_at(h, 0, &data[..CHUNK as usize + 1]).unwrap_err();
        assert!(
            e.to_string().contains("connection to the device was lost"),
            "{e}"
        );
        let e = r.sync(h).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted, "{e}");
        let _ = r.close(h);
        r.remove(&at("out/lost.part")).unwrap();
        assert!(r.write_at(h, 0, b"x").is_err());
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A peer's Delete and Move to: refused until the peer sets its hooks, then its own
    /// `Jobs::delete_path`/`move_path` (its protected paths) inside its share only; core's
    /// `Jobs` hands a `flink://` path to the peer.
    #[cfg(unix)]
    #[test]
    fn a_peer_deletes_and_moves_its_files_for_this_device() {
        use ps5_dump_forge_core::Jobs;
        let share = temp_dir("delmove-share");
        let outside = temp_dir("delmove-outside");
        for d in ["g/sub", "keep"] {
            fs::create_dir_all(share.join(d)).unwrap();
        }
        fs::create_dir_all(&outside).unwrap();
        fs::write(share.join("g/a.bin"), b"a").unwrap();
        fs::write(share.join("g/b.bin"), b"b").unwrap();
        fs::write(share.join("g/sub/x"), b"x").unwrap();
        fs::write(outside.join("f"), b"theirs").unwrap();
        std::os::unix::fs::symlink(&outside, share.join("out")).unwrap();
        let share = dunce::canonicalize(&share).unwrap();
        let (a, b, mut rpc, _, dirs) = paired("delmove", &share);
        let key = a.state().device.key_id;
        let at = |rel: &str| FlinkPath::parse(Path::new(&format!("flink://{key}/{rel}"))).unwrap();
        let r = remote::get().unwrap();

        // No hooks: refused, nothing gone.
        let e = r.delete_path(&at("g/a.bin")).unwrap_err();
        assert!(matches!(e, DeleteError::Refused(_)), "{e:?}");
        assert!(share.join("g/a.bin").exists());

        let a_jobs = Arc::new(Jobs::new(|_| {}));
        let keep = vec![share.join("keep")];
        let (j, k) = (a_jobs.clone(), keep.clone());
        let (j2, k2) = (a_jobs.clone(), keep.clone());
        a.set_delete_move(
            Arc::new(move |p| j.delete_path(p, &k)),
            Arc::new(move |f, t, replace| j2.move_path(f, t, replace, &k2)),
        );

        r.delete_path(&at("g/a.bin")).unwrap();
        assert!(!share.join("g/a.bin").exists());
        let e = r.delete_path(&at("g/a.bin")).unwrap_err();
        assert!(matches!(e, DeleteError::Missing(_)), "{e:?}");
        // The peer's protected paths, and its share root, stay.
        let e = r.delete_path(&at("keep")).unwrap_err();
        assert!(matches!(e, DeleteError::Refused(_)), "{e:?}");
        assert!(share.join("keep").is_dir());
        let e = r.delete_path(&at("")).unwrap_err();
        assert!(matches!(e, DeleteError::Refused(_)), "{e:?}");

        // Outside the share: through a link, and spelled with `..` on the wire.
        let e = r.delete_path(&at("out/f")).unwrap_err();
        assert!(
            matches!(&e, DeleteError::Refused(m) if m.contains("outside")),
            "{e:?}"
        );
        let e = r
            .move_path(&at("g/b.bin"), &at("out/b.bin"), false)
            .unwrap_err();
        assert!(matches!(e, DeleteError::Refused(_)), "{e:?}");
        for m in ["delete_path", "move_path"] {
            let body = serde_json::json!({ "path": "../x", "from": "../x", "to": "g/y" });
            let e = rpc.call(m, body).unwrap_err().to_string();
            assert!(e.starts_with(REFUSED), "{m}: {e}");
        }
        assert_eq!(fs::read(outside.join("f")).unwrap(), b"theirs");
        assert!(!outside.join("b.bin").exists());

        // Moves: a folder renamed, a taken name kept unless replaced.
        r.move_path(&at("g/sub"), &at("moved"), false).unwrap();
        assert_eq!(fs::read(share.join("moved/x")).unwrap(), b"x");
        assert!(!share.join("g/sub").exists());
        let e = r
            .move_path(&at("g/b.bin"), &at("moved/x"), false)
            .unwrap_err();
        assert!(!matches!(e, DeleteError::Missing(_)), "{e:?}");
        assert_eq!(fs::read(share.join("moved/x")).unwrap(), b"x");
        r.move_path(&at("g/b.bin"), &at("moved/x"), true).unwrap();
        assert_eq!(fs::read(share.join("moved/x")).unwrap(), b"b");

        // Core's Jobs on this side: a flink path goes to the peer; across devices refused.
        let b_jobs = Jobs::new(|_| {});
        let url = |rel: &str| at(rel).url();
        b_jobs.delete_path(&url("moved/x"), &[]).unwrap();
        assert!(!share.join("moved/x").exists());
        let e = b_jobs
            .move_path(&url("moved"), &dirs[1].join("moved"), false, &[])
            .unwrap_err();
        assert!(matches!(e, DeleteError::Refused(_)), "{e:?}");
        assert!(share.join("moved").is_dir());

        drop(rpc);
        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share, &outside]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn reads_survive_a_lost_handle_a_dropped_lane_and_the_peer_timeout() {
        use ps5_dump_forge_core::remote::Remote;
        let share = temp_dir("keep-share");
        fs::create_dir_all(&share).unwrap();
        let data: Vec<u8> = (0..3 << 20).map(|i| (i * 7 % 253) as u8).collect();
        fs::write(share.join("f.bin"), &data).unwrap();
        let (a, b, rpc, _, dirs) = paired("keep", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let p = FlinkPath::parse(Path::new(&format!("flink://{key}/f.bin"))).unwrap();
        let r = b.remote();
        assert_eq!(r.read_at(&p, 0, 16).unwrap(), data[..16]);
        let peer = r.peer(&key);

        // A kept handle the peer no longer holds: reopened, the read asked again.
        lock(&peer.session).as_mut().unwrap().handles[0].1 = 999_999;
        assert_eq!(r.read_at(&p, 5, 100).unwrap(), data[5..105]);

        // A dead kept lane: that read retries; the others, sharing the session, never notice.
        let (hs, g) = {
            let s = lock(&peer.session);
            let s = s.as_ref().unwrap();
            (s.hs.clone(), s.generation)
        };
        let dead = join_lane(&hs, 1).unwrap();
        dead.try_clone_stream()
            .unwrap()
            .shutdown(std::net::Shutdown::Both)
            .unwrap();
        lock(&peer.lanes).push((g, dead, Instant::now()));
        std::thread::scope(|s| {
            for t in 0..4usize {
                let (r, p, data) = (&r, &p, &data);
                s.spawn(move || {
                    let o = t * 500_003;
                    assert_eq!(
                        r.read_at(p, o as u64, 1 << 20).unwrap(),
                        data[o..][..1 << 20]
                    );
                });
            }
        });

        // Reads of a kept handle (no control call) for longer than the peer waits on a silent
        // session: the keepalive holds it, so the handle stays open.
        let start = Instant::now();
        while start.elapsed() < DEAD_AFTER + Duration::from_secs(2) {
            assert_eq!(r.read_at(&p, 7, 9).unwrap(), data[7..16]);
            std::thread::sleep(Duration::from_millis(200));
        }
        assert_eq!(lock(&peer.session).as_ref().unwrap().generation, g);

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_lane_outlasts_a_disk_stall_longer_than_its_timeout() {
        // Lanes keyed as real ones, with 3 s standing in for `DEAD_AFTER`, and a peer whose disk
        // takes 4.5 s for each write, put and read: its pings keep both lanes up (without them
        // each wait below times out).
        let timeout = Duration::from_secs(3);
        let stall = Duration::from_millis(4500);
        let root = temp_dir("stall");
        fs::create_dir_all(&root).unwrap();
        // As a share root is kept.
        let root = dunce::canonicalize(&root).unwrap();
        let data: Vec<u8> = (0..CHUNK as usize).map(|i| (i * 7 % 253) as u8).collect();
        fs::write(root.join("src.bin"), &data).unwrap();
        let session = Session {
            secret: [7; 32],
            nonce_c: [1; 16],
            nonce_s: [2; 16],
        };
        let (reads, writes) = (Reads::default(), Writes::default());
        *lock(&reads.stall) = stall;
        *lock(&writes.stall) = stall;
        let _r = reads.guard(&session);
        let _w = writes.guard(&session);
        let h = writes.create_at(&session, root.join("out.part")).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (c2s, s2c) = ([3; 32], [4; 32]);
        let lane = || {
            let mut c = Conn::connect(addr, MAX_BODY, timeout).unwrap();
            c.set_keys(c2s, s2c);
            c
        };
        let (mut w, mut r) = (lane(), lane());
        let start = Instant::now();
        let share = Share::new(root.clone(), None);
        std::thread::scope(|s| {
            let (reads, writes, session, root) = (&reads, &writes, &session, &share);
            for _ in 0..2 {
                let stream = listener.accept().unwrap().0;
                let mut conn = Conn::new(stream, MAX_BODY, timeout).unwrap();
                conn.set_keys(s2c, c2s);
                s.spawn(move || {
                    let first = conn.recv_frame().unwrap();
                    let _ = if is_read(&first) {
                        reads.serve(&mut conn, &session.id(), Some(root), first, &|| true)
                    } else {
                        writes.serve(&mut conn, session, Some(root), "me", first, &|| true)
                    };
                });
            }
            // The write lane: a chunk, then (after a pong, as a reader before `pinging`
            // answered pings) a put.
            let data = &data;
            s.spawn(move || {
                send_write(&mut w, h, 0, data).unwrap();
                recv_written(&mut w, h, 0).unwrap();
                let pong = Frame::new(kind2::PONG, 0, Vec::new());
                w.send_frame(&pong).unwrap();
                let req = Request {
                    id: 0,
                    method: "put".into(),
                    body: serde_json::json!({ "tmp": "put.tmp", "path": "put.bin", "len": 5 }),
                };
                w.send_frame(&req.frame().unwrap()).unwrap();
                let head = DataHead {
                    job: 0,
                    file: 0,
                    offset: 0,
                    hash: chunk_hash(b"small"),
                    stat: None,
                };
                w.send_frame(&data_frame(&head, b"small").unwrap()).unwrap();
                recv_written(&mut w, 0, 0).unwrap();
            });
            // The read lane meanwhile: a chunk by path.
            let req = Request {
                id: 0,
                method: "read".into(),
                body: serde_json::json!({ "path": "src.bin", "offset": 0, "len": CHUNK }),
            };
            r.send_frame(&req.frame().unwrap()).unwrap();
            let f = recv_answer(&mut r).unwrap();
            let (head, bytes) = parse_data(&f).unwrap();
            assert_eq!(
                (head.offset, head.hash.as_str()),
                (0, chunk_hash(data).as_str())
            );
            assert!(bytes == data.as_slice(), "the chunk read is the file's");
            drop(r);
        });
        assert!(
            start.elapsed() >= stall * 2,
            "the write lane waited out both stalls"
        );
        assert!(fs::read(root.join("out.part")).unwrap() == data);
        assert_eq!(fs::read(root.join("put.bin")).unwrap(), b"small");
        assert!(!root.join("put.tmp").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_write_resumes_on_a_fresh_session_when_its_session_is_lost() {
        use ps5_dump_forge_core::remote::Remote;
        let share = temp_dir("resume-share");
        fs::create_dir_all(&share).unwrap();
        let (a, b, rpc, _, dirs) = paired("resume", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let at = |rel: &str| FlinkPath::parse(Path::new(&format!("flink://{key}/{rel}"))).unwrap();
        let r = b.remote();
        let peer = r.peer(&key);
        let generation = || lock(&peer.session).as_ref().map(|s| s.generation);
        let n = 20 * CHUNK as usize + 5;
        let data: Vec<u8> = (0..n).map(|i| (i * 11 % 251) as u8).collect();
        let first = CHUNK as usize + 5;

        let h = r.create(&at("f.part")).unwrap();
        r.write_at(h, 0, &data[..first]).unwrap();
        // The control conn dropped while the rest is written (or just before): the write
        // resumes the file on a fresh session.
        let g = generation().unwrap();
        std::thread::scope(|s| {
            let (r, data) = (&r, &data);
            let w = s.spawn(move || r.write_at(h, first as u64, &data[first..]));
            std::thread::sleep(Duration::from_millis(5));
            *lock(&peer.session) = None;
            w.join().unwrap().unwrap();
        });
        assert!(generation().unwrap() > g, "a fresh session");
        // Dropped again before the sync, the close and the rename: each resumes.
        for step in 0..3 {
            *lock(&peer.session) = None;
            match step {
                0 => r.sync(h),
                1 => r.close(h),
                _ => r.rename_no_replace(&at("f.part"), &at("f")),
            }
            .unwrap_or_else(|e| panic!("step {step}: {e}"));
        }
        assert!(
            fs::read(share.join("f")).unwrap() == data,
            "the bytes match"
        );
        assert!(!share.join("f.part").exists());
        assert!(!a.writing_hook()(&share));

        // A peer from before `reattach`: today's error.
        a.writes().old.store(true, Ordering::SeqCst);
        let h = r.create(&at("old.part")).unwrap();
        r.write_at(h, 0, b"half").unwrap();
        *lock(&peer.session) = None;
        let e = r.write_at(h, 4, b"more").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted, "{e}");
        assert!(
            e.to_string().contains("connection to the device was lost"),
            "{e}"
        );
        let _ = r.close(h);
        r.remove(&at("old.part")).unwrap();

        // The service stopping removes a kept file at once.
        a.writes().old.store(false, Ordering::SeqCst);
        let h = r.create(&at("stop.part")).unwrap();
        r.write_at(h, 0, b"half").unwrap();
        *lock(&peer.session) = None;
        std::thread::sleep(Duration::from_millis(300));
        assert!(share.join("stop.part").exists(), "kept for a resume");
        a.shutdown();
        assert!(!share.join("stop.part").exists());
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn only_the_device_that_created_a_file_reattaches_it() {
        let share = temp_dir("reat-share");
        fs::create_dir_all(&share).unwrap();
        let (a, b, mut rpc, hs, dirs) = paired("reat", &share);
        let key = a.state().device.key_id;
        // A third device, paired with `a` too.
        let dir_c = temp_dir("reat-c");
        let c = Connect::start_rooted(&dir_c, 0, None).unwrap();
        let addr = format!("127.0.0.1:{}", a.state().device.port);
        let o = crate::connect::pair_for_test(&c, &a, &addr, true);
        assert_eq!(o.state, "paired", "{o:?}");
        let (mut theirs, _) = c.session(&key).unwrap();

        let h = call(&mut rpc, "create", "x.part").unwrap()["handle"]
            .as_u64()
            .unwrap();
        let mut lane = join_lane(&hs, 0).unwrap();
        send_write(&mut lane, h, 0, b"hello").unwrap();
        recv_written(&mut lane, h, 0).unwrap();
        // Held by b's live session, then kept once it ends: c never takes it.
        let e = call(&mut theirs, "reattach", "x.part").unwrap_err();
        assert!(e.to_string().contains("not an unfinished file"), "{e}");
        drop(rpc);
        std::thread::sleep(Duration::from_millis(300));
        let e = call(&mut theirs, "reattach", "x.part").unwrap_err();
        assert!(e.to_string().contains("not an unfinished file"), "{e}");
        assert!(call(&mut theirs, "reattach", "nothing.part").is_err());
        // b's next session takes it over, at its size, and finishes it.
        let (mut mine, hs2) = b.session(&key).unwrap();
        let v = call(&mut mine, "reattach", "x.part").unwrap();
        assert_eq!(
            (v["handle"].as_u64(), v["size"].as_u64()),
            (Some(h), Some(5))
        );
        assert_eq!(v["open"], true);
        let mut lane = join_lane(&hs2, 0).unwrap();
        send_write(&mut lane, h, 5, b" world").unwrap();
        recv_written(&mut lane, h, 5).unwrap();
        for m in ["close", "close"] {
            mine.call(m, serde_json::json!({ "handle": h })).unwrap();
        }
        mine.call("rename", serde_json::json!({ "from": "x.part", "to": "x" }))
            .unwrap();
        assert_eq!(fs::read(share.join("x")).unwrap(), b"hello world");

        a.shutdown();
        b.shutdown();
        c.shutdown();
        for d in dirs.iter().chain([&share, &dir_c]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A `reattach` waits for a write of the old session under way (a disk stall), and a write
    /// of the old session after it is refused: none lands once the file changed hands.
    #[test]
    fn a_reattach_waits_for_the_old_sessions_write_and_refuses_later_ones() {
        let root = temp_dir("gate");
        fs::create_dir_all(&root).unwrap();
        let root = dunce::canonicalize(&root).unwrap();
        let old = Session {
            secret: [7; 32],
            nonce_c: [1; 16],
            nonce_s: [2; 16],
        };
        let new = Session {
            secret: [7; 32],
            nonce_c: [5; 16],
            nonce_s: [6; 16],
        };
        let writes = Writes::default();
        let (_o, _n) = (writes.guard(&old), writes.guard(&new));
        let p = root.join("f.part");
        let h = writes.create_at(&old, p.clone()).unwrap();
        *lock(&writes.stall) = Duration::from_millis(1500);
        std::thread::scope(|s| {
            let w = s.spawn(|| writes.write_owned(&old.id(), h, b"old bytes", 0));
            std::thread::sleep(Duration::from_millis(300));
            *lock(&writes.stall) = Duration::ZERO;
            let start = Instant::now();
            let v = writes.reattach_at(&new, &p).unwrap();
            assert!(
                start.elapsed() >= Duration::from_millis(900),
                "waited for the write under way"
            );
            assert_eq!(v["size"].as_u64(), Some(9), "{v}");
            w.join().unwrap().unwrap();
        });
        let e = writes.write_owned(&old.id(), h, b"late", 0).unwrap_err();
        assert!(e.starts_with(NO_OPEN_FILE), "{e}");
        writes.write_owned(&new.id(), h, b"new", 0).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"new bytes");
        drop((_o, _n));
        let _ = fs::remove_dir_all(&root);
    }

    /// A file cut on the peer while the link was down (shorter than a write it confirmed) is
    /// never resumed: a resumed write would leave a gap. The loss stands and the peer's file goes.
    #[test]
    fn a_resume_refuses_a_file_cut_shorter_than_written() {
        use ps5_dump_forge_core::remote::Remote;
        let share = temp_dir("cut-share");
        fs::create_dir_all(&share).unwrap();
        let (a, b, rpc, _, dirs) = paired("cut", &share);
        drop(rpc);
        let key = a.state().device.key_id;
        let at = |rel: &str| FlinkPath::parse(Path::new(&format!("flink://{key}/{rel}"))).unwrap();
        let r = b.remote();
        let peer = r.peer(&key);

        let h = r.create(&at("cut.part")).unwrap();
        r.write_at(h, 0, &[9; 4096]).unwrap();
        *lock(&peer.session) = None;
        fs::OpenOptions::new()
            .write(true)
            .open(share.join("cut.part"))
            .unwrap()
            .set_len(100)
            .unwrap();
        let e = r.write_at(h, 4096, b"more").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted, "{e}");
        assert!(
            e.to_string().contains("connection to the device was lost"),
            "{e}"
        );
        assert!(!share.join("cut.part").exists(), "removed on the peer");
        assert!(!a.writing_hook()(&share));

        a.shutdown();
        b.shutdown();
        for d in dirs.iter().chain([&share]) {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// A retried `put` writes under a temporary name of its own, never the first attempt's.
    #[test]
    fn a_retried_put_takes_a_fresh_temporary_name() {
        let tmp = ".a.bin.forge-12-3.tmp";
        let (one, two) = (retry_name(tmp), retry_name(tmp));
        assert_ne!(one, tmp);
        assert_ne!(one, two);
        for name in [&one, &two] {
            assert!(
                name.starts_with(".a.bin.forge-12-3-r") && name.ends_with(".tmp"),
                "{name}"
            );
        }
        assert!(retry_name("plain").starts_with("plain-r"));
    }
}
