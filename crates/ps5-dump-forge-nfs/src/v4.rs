//! NFSv4.1 (RFC 8881) COMPOUNDs over one session (EXCHANGE_ID, CREATE_SESSION with one
//! fore-channel slot, every later compound led by SEQUENCE). Paths are walked from the
//! pseudo-fs root (PUTROOTFH + LOOKUPs, handles cached). Reads with the anonymous stateid
//! (else inside an OPEN); creates by OPEN (GUARDED4, else EXCLUSIVE4_1), writes UNSTABLE4 +
//! COMMIT (verifier checked), makes folders by CREATE, renames by LINK + REMOVE.

use crate::Entry;
use crate::rpc::{self, Xdr, XdrReader};
use crate::v3::{Handles, MAX_LIST, MAX_LIST_NAMES, components, key};
use std::io::{self, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Mutex, MutexGuard};

/// RPC program and version of NFSv4 (COMPOUND is procedure 1, NULL 0).
pub(crate) const PROGRAM: u32 = 100003;
pub(crate) const VERSION: u32 = 4;
pub(crate) const PROC_COMPOUND: u32 = 1;
/// The minor version every COMPOUND asks for.
pub(crate) const MINOR_VERSION: u32 = 1;

/// Operation numbers (RFC 8881 section 16.2, `nfs_opnum4`).
pub(crate) mod op {
    pub const CLOSE: u32 = 4;
    pub const COMMIT: u32 = 5;
    pub const CREATE: u32 = 6;
    pub const GETATTR: u32 = 9;
    pub const GETFH: u32 = 10;
    pub const LINK: u32 = 11;
    pub const LOOKUP: u32 = 15;
    pub const OPEN: u32 = 18;
    pub const PUTFH: u32 = 22;
    pub const PUTROOTFH: u32 = 24;
    pub const READ: u32 = 25;
    pub const READDIR: u32 = 26;
    pub const REMOVE: u32 = 28;
    pub const RENAME: u32 = 29;
    #[cfg(test)]
    pub const RESTOREFH: u32 = 31;
    pub const SAVEFH: u32 = 32;
    pub const WRITE: u32 = 38;
    pub const EXCHANGE_ID: u32 = 42;
    pub const CREATE_SESSION: u32 = 43;
    pub const DESTROY_SESSION: u32 = 44;
    pub const SEQUENCE: u32 = 53;
    pub const DESTROY_CLIENTID: u32 = 57;
    pub const RECLAIM_COMPLETE: u32 = 58;
    pub const ILLEGAL: u32 = 10044;
}

/// `nfsstat4` values this client tells apart (RFC 8881 section 15.1).
pub(crate) mod status {
    pub const OK: u32 = 0;
    pub const PERM: u32 = 1;
    pub const NOENT: u32 = 2;
    pub const IO: u32 = 5;
    pub const NXIO: u32 = 6;
    pub const ACCESS: u32 = 13;
    pub const EXIST: u32 = 17;
    pub const XDEV: u32 = 18;
    pub const NOTDIR: u32 = 20;
    pub const ISDIR: u32 = 21;
    pub const INVAL: u32 = 22;
    pub const FBIG: u32 = 27;
    pub const NOSPC: u32 = 28;
    pub const ROFS: u32 = 30;
    pub const MLINK: u32 = 31;
    pub const NAMETOOLONG: u32 = 63;
    pub const NOTEMPTY: u32 = 66;
    pub const DQUOT: u32 = 69;
    pub const STALE: u32 = 70;
    pub const BADHANDLE: u32 = 10001;
    pub const BAD_COOKIE: u32 = 10003;
    pub const NOTSUPP: u32 = 10004;
    pub const TOOSMALL: u32 = 10005;
    pub const SERVERFAULT: u32 = 10006;
    pub const BADTYPE: u32 = 10007;
    pub const DELAY: u32 = 10008;
    pub const SAME: u32 = 10009;
    pub const DENIED: u32 = 10010;
    pub const EXPIRED: u32 = 10011;
    pub const GRACE: u32 = 10013;
    pub const FHEXPIRED: u32 = 10014;
    pub const SHARE_DENIED: u32 = 10015;
    pub const WRONGSEC: u32 = 10016;
    pub const NOFILEHANDLE: u32 = 10020;
    pub const MINOR_VERS_MISMATCH: u32 = 10021;
    pub const BAD_STATEID: u32 = 10025;
    pub const NOT_SAME: u32 = 10027;
    pub const SYMLINK: u32 = 10029;
    pub const ATTRNOTSUPP: u32 = 10032;
    pub const BADXDR: u32 = 10036;
    pub const OPENMODE: u32 = 10038;
    pub const BADCHAR: u32 = 10040;
    pub const BADNAME: u32 = 10041;
    pub const OP_ILLEGAL: u32 = 10044;
    pub const FILE_OPEN: u32 = 10046;
    pub const BADSESSION: u32 = 10052;
    pub const BADSLOT: u32 = 10053;
    pub const COMPLETE_ALREADY: u32 = 10054;
    pub const SEQ_MISORDERED: u32 = 10063;
    pub const REQ_TOO_BIG: u32 = 10065;
    pub const REP_TOO_BIG: u32 = 10066;
    pub const OP_NOT_IN_SESSION: u32 = 10071;
    pub const DEADSESSION: u32 = 10078;
    pub const WRONG_TYPE: u32 = 10083;
}

/// The name of an `nfsstat4` this client knows, for messages.
fn status_name(code: u32) -> Option<&'static str> {
    use status::*;
    Some(match code {
        PERM => "NFS4ERR_PERM",
        NOENT => "NFS4ERR_NOENT",
        IO => "NFS4ERR_IO",
        NXIO => "NFS4ERR_NXIO",
        ACCESS => "NFS4ERR_ACCESS",
        EXIST => "NFS4ERR_EXIST",
        XDEV => "NFS4ERR_XDEV",
        NOTDIR => "NFS4ERR_NOTDIR",
        ISDIR => "NFS4ERR_ISDIR",
        INVAL => "NFS4ERR_INVAL",
        FBIG => "NFS4ERR_FBIG",
        NOSPC => "NFS4ERR_NOSPC",
        ROFS => "NFS4ERR_ROFS",
        MLINK => "NFS4ERR_MLINK",
        NAMETOOLONG => "NFS4ERR_NAMETOOLONG",
        NOTEMPTY => "NFS4ERR_NOTEMPTY",
        DQUOT => "NFS4ERR_DQUOT",
        STALE => "NFS4ERR_STALE",
        BADHANDLE => "NFS4ERR_BADHANDLE",
        BAD_COOKIE => "NFS4ERR_BAD_COOKIE",
        NOTSUPP => "NFS4ERR_NOTSUPP",
        TOOSMALL => "NFS4ERR_TOOSMALL",
        SERVERFAULT => "NFS4ERR_SERVERFAULT",
        BADTYPE => "NFS4ERR_BADTYPE",
        DELAY => "NFS4ERR_DELAY",
        SAME => "NFS4ERR_SAME",
        DENIED => "NFS4ERR_DENIED",
        EXPIRED => "NFS4ERR_EXPIRED",
        GRACE => "NFS4ERR_GRACE",
        FHEXPIRED => "NFS4ERR_FHEXPIRED",
        SHARE_DENIED => "NFS4ERR_SHARE_DENIED",
        WRONGSEC => "NFS4ERR_WRONGSEC",
        NOFILEHANDLE => "NFS4ERR_NOFILEHANDLE",
        MINOR_VERS_MISMATCH => "NFS4ERR_MINOR_VERS_MISMATCH",
        BAD_STATEID => "NFS4ERR_BAD_STATEID",
        NOT_SAME => "NFS4ERR_NOT_SAME",
        SYMLINK => "NFS4ERR_SYMLINK",
        ATTRNOTSUPP => "NFS4ERR_ATTRNOTSUPP",
        BADXDR => "NFS4ERR_BADXDR",
        OPENMODE => "NFS4ERR_OPENMODE",
        BADCHAR => "NFS4ERR_BADCHAR",
        BADNAME => "NFS4ERR_BADNAME",
        OP_ILLEGAL => "NFS4ERR_OP_ILLEGAL",
        FILE_OPEN => "NFS4ERR_FILE_OPEN",
        BADSESSION => "NFS4ERR_BADSESSION",
        COMPLETE_ALREADY => "NFS4ERR_COMPLETE_ALREADY",
        BADSLOT => "NFS4ERR_BADSLOT",
        SEQ_MISORDERED => "NFS4ERR_SEQ_MISORDERED",
        REQ_TOO_BIG => "NFS4ERR_REQ_TOO_BIG",
        REP_TOO_BIG => "NFS4ERR_REP_TOO_BIG",
        OP_NOT_IN_SESSION => "NFS4ERR_OP_NOT_IN_SESSION",
        DEADSESSION => "NFS4ERR_DEADSESSION",
        WRONG_TYPE => "NFS4ERR_WRONG_TYPE",
        _ => return None,
    })
}

/// An `nfsstat4` as an `io::Error`: the kind as v3 maps it, the code's name in the text.
pub(crate) fn nfs4_error(code: u32) -> io::Error {
    use status::*;
    let (kind, text) = match code {
        NOENT => (ErrorKind::NotFound, "no such file or folder"),
        EXIST => (ErrorKind::AlreadyExists, "already exists"),
        PERM => (ErrorKind::PermissionDenied, "not owner"),
        ACCESS => (ErrorKind::PermissionDenied, "permission denied"),
        NOSPC => (ErrorKind::StorageFull, "no space left on the server"),
        DQUOT => (ErrorKind::StorageFull, "quota exceeded"),
        ROFS => (ErrorKind::ReadOnlyFilesystem, "read-only filesystem"),
        ISDIR => (ErrorKind::IsADirectory, "is a folder"),
        NOTDIR => (ErrorKind::NotADirectory, "not a folder"),
        NOTEMPTY => (ErrorKind::DirectoryNotEmpty, "folder not empty"),
        XDEV => (ErrorKind::CrossesDevices, "crosses filesystems"),
        NAMETOOLONG => (ErrorKind::InvalidFilename, "name too long"),
        BADNAME | BADCHAR => (ErrorKind::InvalidFilename, "name the server refuses"),
        FBIG => (ErrorKind::FileTooLarge, "file too large"),
        INVAL => (ErrorKind::InvalidInput, "invalid argument"),
        STALE | FHEXPIRED => (ErrorKind::StaleNetworkFileHandle, "stale file handle"),
        BADHANDLE => (ErrorKind::InvalidData, "bad file handle"),
        MINOR_VERS_MISMATCH => (ErrorKind::Unsupported, "the server has no NFSv4.1"),
        NOTSUPP | ATTRNOTSUPP | OP_ILLEGAL => (ErrorKind::Unsupported, "operation not supported"),
        // An idle session the server let go (its lease ran out): a fresh connection's
        // EXCHANGE_ID and CREATE_SESSION make a new one, as after a dropped connection.
        BADSESSION | DEADSESSION => (ErrorKind::NotConnected, "the server ended the session"),
        _ => (ErrorKind::Other, "error"),
    };
    let name = status_name(code).unwrap_or("NFS4ERR_UNKNOWN");
    io::Error::new(kind, format!("NFS: {text} ({name}, {code})"))
}

/// True when `e` came from NFS4ERR_STALE/FHEXPIRED (drop the cached handle, look it up again).
pub(crate) fn is_stale(e: &io::Error) -> bool {
    e.kind() == ErrorKind::StaleNetworkFileHandle
}

/// COMPOUND4args being built: an empty tag, minor version 1, then the ops in order.
#[derive(Default)]
pub(crate) struct Compound {
    ops: Xdr,
    count: u32,
}

impl Compound {
    pub fn new() -> Compound {
        Compound::default()
    }

    /// Starts the next op: its number is written, the caller appends its arguments.
    pub fn op(&mut self, op: u32) -> &mut Xdr {
        self.count += 1;
        self.ops.u32(op)
    }

    /// The number of ops so far.
    #[cfg(test)]
    pub fn len(&self) -> u32 {
        self.count
    }

    /// The encoded COMPOUND4args.
    pub fn into_bytes(self) -> Vec<u8> {
        let mut head = Xdr::new();
        head.string("").u32(MINOR_VERSION).u32(self.count);
        let mut out = head.into_bytes();
        out.extend_from_slice(&self.ops.into_bytes());
        out
    }
}

/// Walks a COMPOUND4res op by op: `op` checks the number and status of the next result
/// and hands back the reader at its body; the first failing op ends the walk with its error.
pub(crate) struct Walk<'a> {
    r: XdrReader<'a>,
    status: u32,
    left: u32,
    failed: Option<u32>,
}

/// Results a COMPOUND reply may hold (RFC 8881 caps nothing; this bounds a hostile count).
const MAX_RESULTS: usize = 1024;

impl<'a> Walk<'a> {
    /// Reads the overall status, the tag and the result count.
    pub fn new(mut r: XdrReader<'a>) -> io::Result<Walk<'a>> {
        let status = r.u32()?;
        r.opaque(1024)?; // tag: ours is empty, a server echoes it
        let left = r.len(MAX_RESULTS)? as u32;
        Ok(Walk {
            r,
            status,
            left,
            failed: None,
        })
    }

    /// The overall status (that of the last op the server ran).
    pub fn status(&self) -> u32 {
        self.status
    }

    /// The next result, which must be op `op`: its body on NFS4_OK, else its status as an error.
    pub fn op(&mut self, op: u32) -> io::Result<&mut XdrReader<'a>> {
        if let Some(code) = self.failed {
            return Err(nfs4_error(code));
        }
        if self.left == 0 {
            // The server stopped early: the overall status says why (an OK reply that
            // is short is malformed).
            return Err(match self.status {
                status::OK => rpc::bad(format!("NFSv4 reply has no result for op {op}")),
                code => {
                    self.failed = Some(code);
                    nfs4_error(code)
                }
            });
        }
        self.left -= 1;
        let got = self.r.u32()?;
        let code = self.r.u32()?;
        if got != op && !(got == op::ILLEGAL && code != status::OK) {
            return Err(rpc::bad(format!(
                "NFSv4 reply has op {got} where op {op} was sent"
            )));
        }
        if code != status::OK {
            self.failed = Some(code);
            return Err(nfs4_error(code));
        }
        Ok(&mut self.r)
    }

    /// Checks the walk read every result and the overall status is OK.
    pub fn finish(&self) -> io::Result<()> {
        if let Some(code) = self.failed {
            return Err(nfs4_error(code));
        }
        if self.status != status::OK {
            return Err(nfs4_error(self.status));
        }
        if self.left != 0 {
            return Err(rpc::bad(format!(
                "NFSv4 reply has {} results more than were read",
                self.left
            )));
        }
        Ok(())
    }
}

/// One NFSv4.1 session: its client id and session id, the one fore-channel slot's next
/// seqid and the sizes the server granted.
pub(crate) struct Nfs4 {
    client: rpc::Client,
    clientid: u64,
    session: [u8; 16],
    /// The seqid the next SEQUENCE on slot 0 carries.
    seqid: u32,
    /// The fore channel's largest request and response (READ/WRITE chunks fit inside).
    max_request: u32,
    max_response: u32,
    /// The most ops one compound may carry (the server may grant fewer than asked).
    max_ops: u32,
    /// File handles by canonical path (v3's bounded cache).
    handles: Handles,
    /// Set once the server refused a READ with the anonymous stateid: reads OPEN first.
    open_reads: bool,
}

/// A file this session created: its handle, its open stateid and the WRITE verifiers since
/// the last COMMIT.
pub(crate) struct Open {
    fh: Vec<u8>,
    stateid: [u8; 16],
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

/// EXCHANGE_ID's `state_protect4_a`/`_r` discriminant for no state protection.
const SP4_NONE: u32 = 0;
/// The fore channel this client asks for: one slot, requests and replies up to 4 MiB
/// (the server lowers them to what it allows).
const ASK_MAX_REQUEST: u32 = 4 << 20;
const ASK_MAX_RESPONSE: u32 = 4 << 20;
const ASK_MAX_OPS: u32 = 16;
/// Header room every fore-channel request and reply needs beyond a READ's or WRITE's data.
const HEADROOM: u32 = 4096;
/// The smallest request and reply a session must allow: the headers plus room for a READDIR
/// entry, a READ or a WRITE of at least 4 KiB.
const MIN_CHANNEL: u32 = 2 * HEADROOM;

/// A client owner unique to this process and connection (two sessions of one process must
/// not share one: a new verifier for a known owner reads as a client reboot and drops state).
fn owner() -> Vec<u8> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "forge".into());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("ps5-dump-forge {host} {} {n}", std::process::id()).into_bytes()
}

/// An 8-byte verifier: this connection's start time.
fn verifier() -> [u8; 8] {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    ((now.as_secs() << 32) ^ u64::from(now.subsec_nanos())).to_be_bytes()
}

/// Appends a `channel_attrs4` (no RDMA).
fn channel(x: &mut Xdr, max_request: u32, max_response: u32, cached: u32, ops: u32) {
    x.u32(0).u32(max_request).u32(max_response).u32(cached);
    x.u32(ops).u32(1).u32(0);
}

/// Reads a `channel_attrs4`: (max request, max response, max requests, max ops).
fn read_channel(r: &mut XdrReader) -> io::Result<(u32, u32, u32, u32)> {
    let _headerpad = r.u32()?;
    let (request, response) = (r.u32()?, r.u32()?);
    let _cached = r.u32()?;
    let ops = r.u32()?;
    let requests = r.u32()?;
    for _ in 0..r.len(1)? {
        r.u32()?; // ca_rdma_ird
    }
    Ok((request, response, requests, ops))
}

/// The fewest ops per compound this client needs (a LINK: SEQUENCE, PUTFH, SAVEFH, PUTFH, LINK).
const MIN_OPS: u32 = 5;

impl Nfs4 {
    /// Connects to `host` (`port`, else 2049): EXCHANGE_ID, CREATE_SESSION (one slot, no
    /// back channel), RECLAIM_COMPLETE. A server without v4.1 gives `Unsupported`.
    pub fn connect(host: &str, port: Option<u16>) -> io::Result<Nfs4> {
        let addr = resolve(host, port.unwrap_or(crate::DEFAULT_PORT))?;
        let client = rpc::Client::connect(addr)?;
        Nfs4::open_session(client).map_err(|e| {
            if rpc::is_version_mismatch(&e) {
                io::Error::new(ErrorKind::Unsupported, format!("no NFSv4.1: {e}"))
            } else {
                e
            }
        })
    }

    fn open_session(mut client: rpc::Client) -> io::Result<Nfs4> {
        client.null(PROGRAM, VERSION)?;
        let mut c = Compound::new();
        let x = c.op(op::EXCHANGE_ID);
        x.fixed(&verifier()).opaque(&owner());
        x.u32(0).u32(SP4_NONE).u32(0); // no flags, no state protection, no implementation id
        let reply = client.call(PROGRAM, VERSION, PROC_COMPOUND, &c.into_bytes())?;
        let mut w = Walk::new(reply.reader())?;
        let r = w.op(op::EXCHANGE_ID)?;
        let (clientid, sequence, _flags) = (r.u64()?, r.u32()?, r.u32()?);
        if r.u32()? != SP4_NONE {
            return Err(rpc::bad("the NFSv4.1 server asks for state protection"));
        }
        r.u64()?; // server_owner4: minor id, major id
        r.opaque(1024)?;
        r.opaque(1024)?; // server scope
        for _ in 0..r.len(1)? {
            r.opaque(1024)?; // nfs_impl_id4: domain, name, date
            r.opaque(1024)?;
            r.u64()?;
            r.u32()?;
        }
        w.finish()?;

        let mut c = Compound::new();
        let x = c.op(op::CREATE_SESSION);
        x.u64(clientid).u32(sequence).u32(0); // no flags: no persistence, no back channel
        channel(x, ASK_MAX_REQUEST, ASK_MAX_RESPONSE, HEADROOM, ASK_MAX_OPS);
        channel(x, HEADROOM, HEADROOM, 0, 2);
        x.u32(0).u32(1).u32(0); // callback program 0, one security parameter: AUTH_NONE
        let reply = client.call(PROGRAM, VERSION, PROC_COMPOUND, &c.into_bytes())?;
        let mut w = Walk::new(reply.reader())?;
        let r = w.op(op::CREATE_SESSION)?;
        let session: [u8; 16] = r
            .fixed(16)?
            .try_into()
            .map_err(|_| rpc::bad("session id"))?;
        let (_sequence, _flags) = (r.u32()?, r.u32()?);
        let (max_request, max_response, slots, max_ops) = read_channel(r)?;
        read_channel(r)?;
        w.finish()?;
        // Never more than asked (a grant past it would size our buffers), never too little.
        let (max_request, max_response, max_ops) = (
            max_request.min(ASK_MAX_REQUEST),
            max_response.min(ASK_MAX_RESPONSE),
            max_ops.min(ASK_MAX_OPS),
        );
        if slots == 0
            || max_request < MIN_CHANNEL
            || max_response < MIN_CHANNEL
            || max_ops < MIN_OPS
        {
            return Err(rpc::bad(format!(
                "NFSv4.1 session too small: {slots} slots, requests {max_request} B, replies {max_response} B, {max_ops} ops"
            )));
        }

        let mut nfs = Nfs4 {
            client,
            clientid,
            session,
            seqid: 1,
            max_request,
            max_response,
            max_ops,
            handles: Handles::default(),
            open_reads: false,
        };
        let mut c = nfs.sequenced();
        c.op(op::RECLAIM_COMPLETE).bool(false); // rca_one_fs: all filesystems
        nfs.run(c, |w| {
            let done = w.op(op::RECLAIM_COMPLETE).map(drop);
            // A server that saw this client id before already has it.
            if done.is_err() && w.status() == status::COMPLETE_ALREADY {
                return Ok(());
            }
            done.and_then(|()| w.finish())
        })?;
        Ok(nfs)
    }

    /// A compound that starts with SEQUENCE on slot 0 (the slot's next seqid, no caching).
    fn sequenced(&self) -> Compound {
        let mut c = Compound::new();
        c.op(op::SEQUENCE)
            .fixed(&self.session)
            .u32(self.seqid)
            .u32(0)
            .u32(0)
            .bool(false);
        c
    }

    /// Sends `c` (from [`Nfs4::sequenced`]), checks its SEQUENCE result (the slot's seqid
    /// moves on once the server ran it), then hands the walk after it to `parse`, which reads
    /// the other results and ends with `finish`.
    fn run<T>(
        &mut self,
        c: Compound,
        parse: impl FnOnce(&mut Walk) -> io::Result<T>,
    ) -> io::Result<T> {
        let reply = self
            .client
            .call(PROGRAM, VERSION, PROC_COMPOUND, &c.into_bytes())?;
        let mut w = Walk::new(reply.reader())?;
        let r = w.op(op::SEQUENCE)?;
        let seqid = self.seqid;
        self.seqid = seqid.wrapping_add(1);
        let (session, got, slot) = (r.fixed(16)?, r.u32()?, r.u32()?);
        let _highest_and_target_and_flags = (r.u32()?, r.u32()?, r.u32()?);
        if session != self.session || got != seqid || slot != 0 {
            return Err(rpc::bad(
                "NFSv4.1 SEQUENCE reply for another session, slot or request",
            ));
        }
        parse(&mut w)
    }

    pub fn exports(&mut self) -> io::Result<Vec<String>> {
        Ok(vec!["/".into()])
    }
}

/// LOOKUPs one compound carries at most (each with a GETFH; with SEQUENCE and PUTFH: 14 of
/// the 16 ops asked for), fewer when the server granted fewer ops.
const MAX_LOOKUPS: usize = 6;
/// READDIR's `dircount` (names and cookies) and its largest `maxcount`.
const DIRCOUNT: u32 = 32 << 10;
const MAX_DIR_REPLY: u32 = 256 << 10;

impl Nfs4 {
    /// `path`'s file handle: cached, else PUTROOTFH (or the nearest cached folder's PUTFH)
    /// and LOOKUPs, each one's handle cached. Links are not followed.
    fn handle(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let comps = components(path)?;
        let mut done = comps.len();
        let mut fh = loop {
            if let Some(fh) = self.handles.get(&key(&comps[..done])) {
                break Some(fh.to_vec());
            }
            if done == 0 {
                break None;
            }
            done -= 1;
        };
        while fh.is_none() || done < comps.len() {
            let lookups = (((self.max_ops - 2) / 2) as usize).clamp(1, MAX_LOOKUPS);
            let take = &comps[done..comps.len().min(done + lookups)];
            let mut c = self.sequenced();
            let put = match &fh {
                Some(h) => {
                    c.op(op::PUTFH).opaque(h);
                    op::PUTFH
                }
                None => {
                    c.op(op::PUTROOTFH);
                    op::PUTROOTFH
                }
            };
            for name in take {
                c.op(op::LOOKUP).string(name);
                c.op(op::GETFH);
            }
            if take.is_empty() {
                c.op(op::GETFH); // the root's own handle
            }
            let got = self.run(c, |w| {
                w.op(put)?;
                let mut got = Vec::new();
                for _ in 0..take.len().max(1) {
                    if !take.is_empty() {
                        w.op(op::LOOKUP)?;
                    }
                    got.push(w.op(op::GETFH)?.opaque(FHSIZE)?.to_vec());
                }
                w.finish()?;
                Ok(got)
            })?;
            for h in got {
                done = (done + 1).min(comps.len());
                self.handles.put(key(&comps[..done]), h.clone());
                fh = Some(h);
            }
        }
        Ok(fh.expect("walked to the path"))
    }

    /// Runs `f` on `path`'s handle; on STALE the cache is dropped and it runs once more
    /// from fresh LOOKUPs.
    fn with_handle<T>(
        &mut self,
        path: &str,
        mut f: impl FnMut(&mut Self, &[u8]) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut once = |s: &mut Self| {
            let fh = s.handle(path)?;
            f(s, &fh)
        };
        match once(self) {
            Err(e) if is_stale(&e) => {
                self.handles.clear();
                once(self)
            }
            r => r,
        }
    }

    /// GETATTR of `attrs` on `fh`.
    fn getattr(&mut self, fh: &[u8], attrs: [u32; 2]) -> io::Result<Fattr> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(fh);
        put_bitmap(c.op(op::GETATTR), attrs);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            let a = Fattr::read(w.op(op::GETATTR)?)?;
            w.finish()?;
            Ok(a)
        })
    }

    pub fn stat(&mut self, path: &str) -> io::Result<Entry> {
        let name = components(path)?.last().map_or("/", |c| c).to_string();
        self.with_handle(path, |s, fh| s.getattr(fh, STAT_ATTRS)?.entry(name.clone()))
    }

    /// `space_avail` of the filesystem holding `path`.
    pub fn free_space(&mut self, path: &str) -> io::Result<u64> {
        self.with_handle(path, |s, fh| {
            s.getattr(fh, FREE_ATTRS)?.space_avail.ok_or_else(|| {
                io::Error::new(ErrorKind::Unsupported, "the server gives no space_avail")
            })
        })
    }

    /// A folder's entries (no `.`/`..`), READDIR page by page; each entry's handle cached.
    /// A name that isn't UTF-8 or holds `/` is listed as special (never usable as a path).
    pub fn list(&mut self, path: &str) -> io::Result<Vec<Entry>> {
        let base = key(&components(path)?);
        self.with_handle(path, |s, fh| {
            // The reply, headers included, must fit the session's max_response.
            let maxcount = s.max_response.saturating_sub(HEADROOM).min(MAX_DIR_REPLY);
            let (mut out, mut cookie, mut verf) = (Vec::new(), 0u64, [0u8; 8]);
            let (mut seen, mut names) = (0usize, 0usize);
            loop {
                let mut c = s.sequenced();
                c.op(op::PUTFH).opaque(fh);
                let x = c.op(op::READDIR);
                x.u64(cookie)
                    .fixed(&verf)
                    .u32(DIRCOUNT.min(maxcount))
                    .u32(maxcount);
                put_bitmap(x, LIST_ATTRS);
                let page = s.run(c, |w| {
                    w.op(op::PUTFH)?;
                    let page = DirPage::read(w.op(op::READDIR)?)?;
                    w.finish()?;
                    Ok(page)
                })?;
                if page.entries.is_empty() && !page.eof {
                    return Err(rpc::bad(
                        "NFSv4 READDIR page with no entries before the end",
                    ));
                }
                // `.` and `..` counted too: a server repeating only them can't page forever.
                seen += page.entries.len();
                names += page.entries.iter().map(|e| e.name.len()).sum::<usize>();
                if seen > MAX_LIST + 2 || names > MAX_LIST_NAMES {
                    return Err(io::Error::new(
                        ErrorKind::OutOfMemory,
                        format!("a folder of more than {MAX_LIST} entries (or names too long)"),
                    ));
                }
                for e in page.entries {
                    cookie = e.cookie;
                    if e.name == b"." || e.name == b".." {
                        continue;
                    }
                    if !e.utf8 {
                        out.push(Entry {
                            name: String::from_utf8_lossy(&e.name).into_owned(),
                            dir: false,
                            size: 0,
                            mtime: None,
                            link: false,
                            special: true,
                        });
                        continue;
                    }
                    let name = String::from_utf8(e.name).expect("checked by DirPage::read");
                    let child = if base == "/" {
                        format!("/{name}")
                    } else {
                        format!("{base}/{name}")
                    };
                    if let Some(h) = &e.attr.fh {
                        s.handles.put(child.clone(), h.clone());
                    }
                    out.push(match e.attr.kind {
                        Some(_) => e.attr.entry(name)?,
                        None => s.stat(&child)?, // the server sent no attributes
                    });
                }
                verf = page.verf;
                if page.eof {
                    return Ok(out);
                }
            }
        })
    }
}

/// The anonymous stateid (all zero): READ without an OPEN, where the server allows it.
const ANON_STATEID: [u8; 16] = [0; 16];
/// OPEN's share_access (read, no delegation wanted), opentype and claim (RFC 8881 18.16).
const OPEN4_SHARE_ACCESS_READ: u32 = 1;
const OPEN4_SHARE_ACCESS_WANT_NO_DELEG: u32 = 0x0400;
const OPEN4_NOCREATE: u32 = 0;
const CLAIM_FH: u32 = 4;
/// The open owner of this client's reads.
const READ_OWNER: &[u8] = b"forge-read";

impl Nfs4 {
    /// Up to `len` bytes at `offset` (fewer only at the end): with the anonymous stateid,
    /// else (once the server refused it, for the rest of the session) inside an OPEN for
    /// read, closed after.
    pub fn read_at(&mut self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.with_handle(path, |s, fh| {
            if !s.open_reads {
                match s.read_with(fh, &ANON_STATEID, offset, len) {
                    Err(_) if s.open_reads => {}
                    r => return r,
                }
            }
            let stateid = s.open_read(fh)?;
            let r = s.read_with(fh, &stateid, offset, len);
            s.close_stateid(fh, &stateid);
            r
        })
    }

    /// READs of `fh` under `stateid`, each as big as the fore channel's replies allow.
    fn read_with(
        &mut self,
        fh: &[u8],
        stateid: &[u8; 16],
        offset: u64,
        len: usize,
    ) -> io::Result<Vec<u8>> {
        // The reply, headers included, must fit the session's max_response.
        let chunk = self.max_response.saturating_sub(HEADROOM).max(1) as usize;
        let mut out = Vec::with_capacity(len.min(chunk));
        while out.len() < len {
            let count = (len - out.len()).min(chunk) as u32;
            let at = offset
                .checked_add(out.len() as u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "offset past 2^64"))?;
            let mut c = self.sequenced();
            c.op(op::PUTFH).opaque(fh);
            c.op(op::READ).fixed(stateid).u64(at).u32(count);
            let anon = *stateid == ANON_STATEID;
            let mut refused = false;
            let got = self.run(c, |w| {
                w.op(op::PUTFH)?;
                let r = match w.op(op::READ) {
                    Ok(r) => r,
                    Err(e) => {
                        refused =
                            anon && matches!(w.status(), status::BAD_STATEID | status::OPENMODE);
                        return Err(e);
                    }
                };
                let eof = r.bool()?;
                let data = r.opaque(count as usize)?.to_vec();
                w.finish()?;
                Ok((data, eof))
            });
            if refused {
                self.open_reads = true;
            }
            let (data, eof) = got?;
            if data.is_empty() && !eof {
                return Err(rpc::bad("NFSv4 READ returned nothing before the end"));
            }
            out.extend_from_slice(&data);
            if eof {
                break;
            }
        }
        Ok(out)
    }

    /// OPEN of `fh` for read (CLAIM_FH, no create): its open stateid.
    fn open_read(&mut self, fh: &[u8]) -> io::Result<[u8; 16]> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(fh);
        c.op(op::OPEN)
            .u32(0) // seqid: unused in v4.1
            .u32(OPEN4_SHARE_ACCESS_READ | OPEN4_SHARE_ACCESS_WANT_NO_DELEG)
            .u32(0) // share_deny: none
            .u64(self.clientid)
            .opaque(READ_OWNER)
            .u32(OPEN4_NOCREATE)
            .u32(CLAIM_FH);
        // RFC 8881 2.10.6.1.3: a GETFH after OPEN, else a server may call it unsafe.
        c.op(op::GETFH);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            let id = open_result(w.op(op::OPEN)?)?;
            w.op(op::GETFH)?.opaque(FHSIZE)?;
            w.finish()?;
            Ok(id)
        })
    }

    /// CLOSE of an open stateid (best effort: the lease ends it anyway).
    fn close_stateid(&mut self, fh: &[u8], stateid: &[u8; 16]) {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(fh);
        c.op(op::CLOSE).u32(0).fixed(stateid);
        let _ = self.run(c, |w| {
            w.op(op::PUTFH)?;
            w.op(op::CLOSE)?;
            w.finish()
        });
    }
}

/// One READDIR entry: its raw name (`utf8` false when it isn't UTF-8 or holds `/` or NUL),
/// cookie and attributes.
#[derive(Debug)]
pub(crate) struct DirEntry {
    pub cookie: u64,
    pub name: Vec<u8>,
    pub utf8: bool,
    pub attr: Fattr,
}

/// One READDIR4resok: the cookie verifier, the entries and whether the folder ended.
#[derive(Debug)]
pub(crate) struct DirPage {
    pub verf: [u8; 8],
    pub entries: Vec<DirEntry>,
    pub eof: bool,
}

impl DirPage {
    pub fn read(r: &mut XdrReader) -> io::Result<DirPage> {
        let mut verf = [0u8; 8];
        verf.copy_from_slice(r.fixed(8)?);
        let mut entries = Vec::new();
        while r.bool()? {
            if entries.len() > MAX_LIST {
                return Err(rpc::bad("NFSv4 READDIR page too long"));
            }
            let cookie = r.u64()?;
            let name = r.opaque(4096)?.to_vec();
            let utf8 = std::str::from_utf8(&name).is_ok_and(|n| !n.contains(['/', '\0']));
            let attr = Fattr::read(r)?;
            entries.push(DirEntry {
                cookie,
                name,
                utf8,
                attr,
            });
        }
        let eof = r.bool()?;
        Ok(DirPage { verf, entries, eof })
    }
}

/// OPEN for writing with create (`opentype4` OPEN4_CREATE, `createmode4` GUARDED4 or
/// EXCLUSIVE4_1, the file named in its folder: CLAIM_NULL).
const OPEN4_SHARE_ACCESS_WRITE: u32 = 2;
const OPEN4_CREATE: u32 = 1;
const GUARDED4: u32 = 1;
const EXCLUSIVE4_1: u32 = 3;
const CLAIM_NULL: u32 = 0;
/// `open_delegation_type4` values.
const OPEN_DELEGATE_NONE: u32 = 0;
const OPEN_DELEGATE_READ: u32 = 1;
const OPEN_DELEGATE_WRITE: u32 = 2;
const OPEN_DELEGATE_NONE_EXT: u32 = 3;
/// `stable_how4` UNSTABLE4 and `createtype4` NF4DIR is [`NF4DIR`].
const UNSTABLE4: u32 = 0;
/// The open owner of the files this client creates.
const WRITE_OWNER: &[u8] = b"forge-write";

/// `path`'s folder and name; the root has neither (`at_root`: the error kind then).
fn parent(path: &str, at_root: ErrorKind) -> io::Result<(String, String)> {
    let comps = components(path)?;
    match comps.split_last() {
        Some((name, dir)) => Ok((key(dir), name.to_string())),
        None => Err(io::Error::new(at_root, "the root can't be changed")),
    }
}

/// A `fattr4` that sets only the mode.
fn put_mode(x: &mut Xdr, mode: u32) {
    put_bitmap(x, bits(&[attr::MODE]));
    let mut v = Xdr::new();
    v.u32(mode);
    x.opaque(&v.into_bytes());
}

/// Skips a `change_info4`.
fn skip_cinfo(r: &mut XdrReader) -> io::Result<()> {
    r.bool()?;
    r.u64()?;
    r.u64()?;
    Ok(())
}

/// Skips a `bitmap4` (at most 8 words).
fn skip_bitmap(r: &mut XdrReader) -> io::Result<()> {
    for _ in 0..r.len(8)? {
        r.u32()?;
    }
    Ok(())
}

/// Skips an `nfsace4`.
fn skip_ace(r: &mut XdrReader) -> io::Result<()> {
    r.u32()?;
    r.u32()?;
    r.u32()?;
    r.opaque(4096)?;
    Ok(())
}

/// Reads an `OPEN4resok`: the open stateid (the rest is read past).
// ponytail: a delegation granted although none was wanted (no callback channel) is not
// returned; the lease ends it.
fn open_result(r: &mut XdrReader) -> io::Result<[u8; 16]> {
    let mut id = [0u8; 16];
    id.copy_from_slice(r.fixed(16)?);
    skip_cinfo(r)?;
    r.u32()?; // rflags
    skip_bitmap(r)?; // attrset
    match r.u32()? {
        OPEN_DELEGATE_NONE => {}
        OPEN_DELEGATE_READ => {
            r.fixed(16)?;
            r.bool()?;
            skip_ace(r)?;
        }
        OPEN_DELEGATE_WRITE => {
            r.fixed(16)?;
            r.bool()?;
            match r.u32()? {
                1 => drop(r.u64()?),
                2 => drop((r.u32()?, r.u32()?)),
                _ => return Err(rpc::bad("NFSv4 OPEN: unknown space limit")),
            }
            skip_ace(r)?;
        }
        OPEN_DELEGATE_NONE_EXT => {
            // WND4_CONTENTION and WND4_RESOURCE carry a bool.
            if matches!(r.u32()?, 1 | 2) {
                r.bool()?;
            }
        }
        _ => return Err(rpc::bad("NFSv4 OPEN: unknown delegation type")),
    }
    Ok(id)
}

impl Nfs4 {
    /// A new empty file (mode 0666) open for writing: OPEN with GUARDED4 create, else (a
    /// server without it) EXCLUSIVE4_1; `AlreadyExists` when the name is taken.
    pub fn create(&mut self, path: &str) -> io::Result<Open> {
        let (dir, name) = parent(path, ErrorKind::AlreadyExists)?;
        let made = self.with_handle(&dir, |s, dh| s.open_create(dh, &name, GUARDED4));
        let (fh, stateid) = match made {
            Err(e) if matches!(e.kind(), ErrorKind::Unsupported | ErrorKind::InvalidInput) => {
                self.with_handle(&dir, |s, dh| s.open_create(dh, &name, EXCLUSIVE4_1))?
            }
            r => r?,
        };
        self.handles.put(key(&components(path)?), fh.clone());
        Ok(Open {
            fh,
            stateid,
            verf: Mutex::default(),
        })
    }

    /// OPEN of `name` in the folder `dh`, created by `how`: the new file's handle and stateid.
    fn open_create(&mut self, dh: &[u8], name: &str, how: u32) -> io::Result<(Vec<u8>, [u8; 16])> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(dh);
        let o = c.op(op::OPEN);
        o.u32(0) // seqid: unused in v4.1
            .u32(OPEN4_SHARE_ACCESS_WRITE | OPEN4_SHARE_ACCESS_WANT_NO_DELEG)
            .u32(0) // share_deny: none
            .u64(self.clientid)
            .opaque(WRITE_OWNER)
            .u32(OPEN4_CREATE)
            .u32(how);
        if how == EXCLUSIVE4_1 {
            // ponytail: a time verifier; a file another create made in the same nanosecond
            // with it would be opened instead of refused.
            o.fixed(&verifier());
        }
        put_mode(o, 0o666);
        o.u32(CLAIM_NULL).string(name);
        c.op(op::GETFH);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            let id = open_result(w.op(op::OPEN)?)?;
            let fh = w.op(op::GETFH)?.opaque(FHSIZE)?.to_vec();
            w.finish()?;
            Ok((fh, id))
        })
    }

    /// CLOSE of the file's open state (best effort, on drop).
    pub fn close(&mut self, open: &Open) {
        self.close_stateid(&open.fh, &open.stateid);
    }
}

impl Nfs4 {
    /// UNSTABLE4 WRITEs of `bytes` at `offset`, each as big as the fore channel's requests
    /// allow (a short count continues where it stopped; none is an error).
    pub fn write_at(&mut self, open: &Open, offset: u64, bytes: &[u8]) -> io::Result<()> {
        // The request, headers included, must fit the session's max_request.
        let chunk = self.max_request.saturating_sub(HEADROOM).max(1) as usize;
        let mut done = 0;
        while done < bytes.len() {
            let part = &bytes[done..bytes.len().min(done + chunk)];
            let at = offset
                .checked_add(done as u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "offset past 2^64"))?;
            let mut c = self.sequenced();
            c.op(op::PUTFH).opaque(&open.fh);
            c.op(op::WRITE)
                .fixed(&open.stateid)
                .u64(at)
                .u32(UNSTABLE4)
                .opaque(part);
            let (count, verf) = self.run(c, |w| {
                w.op(op::PUTFH)?;
                let r = w.op(op::WRITE)?;
                let count = r.u32()? as usize;
                r.u32()?; // committed: recorded by the verifier either way
                let verf: [u8; 8] = r.fixed(8)?.try_into().expect("8 bytes");
                w.finish()?;
                Ok((count, verf))
            })?;
            if count == 0 || count > part.len() {
                return Err(rpc::bad("NFSv4 WRITE wrote nothing or more than sent"));
            }
            open.wrote(verf);
            done += count;
        }
        Ok(())
    }

    /// COMMIT of the whole file; fails when the server's write verifier changed since the
    /// WRITEs (it restarted: unstable writes may be lost).
    pub fn sync(&mut self, open: &Open) -> io::Result<()> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(&open.fh);
        c.op(op::COMMIT).u64(0).u32(0);
        let verf = self.run(c, |w| {
            w.op(op::PUTFH)?;
            let verf: [u8; 8] = w.op(op::COMMIT)?.fixed(8)?.try_into().expect("8 bytes");
            w.finish()?;
            Ok(verf)
        })?;
        open.committed(verf)
    }

    /// A new folder (mode 0777) by CREATE NF4DIR; `AlreadyExists` when the name is taken.
    pub fn mkdir(&mut self, path: &str) -> io::Result<()> {
        let (dir, name) = parent(path, ErrorKind::AlreadyExists)?;
        self.with_handle(&dir, |s, dh| {
            let mut c = s.sequenced();
            c.op(op::PUTFH).opaque(dh);
            let o = c.op(op::CREATE);
            o.u32(NF4DIR).string(&name);
            put_mode(o, 0o777);
            s.run(c, |w| {
                w.op(op::PUTFH)?;
                let r = w.op(op::CREATE)?;
                skip_cinfo(r)?;
                skip_bitmap(r)?; // attrset
                w.finish()
            })
        })?;
        // A handle cached for this path is of something else (renamed away by another client).
        self.handles.forget(&key(&components(path)?));
        Ok(())
    }

    /// REMOVE of a file. v4's REMOVE takes an empty folder too, so a folder is refused first.
    // ponytail: two compounds; a folder put at the name between them is removed if empty.
    pub fn remove(&mut self, path: &str) -> io::Result<()> {
        let (dir, name) = parent(path, ErrorKind::ReadOnlyFilesystem)?;
        self.handles.forget(&key(&components(path)?));
        if self.stat(path)?.dir {
            return Err(io::Error::new(ErrorKind::IsADirectory, "is a folder"));
        }
        let removed = self.with_handle(&dir, |s, dh| s.remove_in(dh, &name));
        self.handles.forget(&key(&components(path)?));
        removed
    }

    /// REMOVE of `name` in the folder `dh`.
    fn remove_in(&mut self, dh: &[u8], name: &str) -> io::Result<()> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(dh);
        c.op(op::REMOVE).string(name);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            skip_cinfo(w.op(op::REMOVE)?)?;
            w.finish()
        })
    }
}

/// LINK refusals that mean "no hard link here" (the server, the filesystem or a folder can't
/// have one): the rename falls back to a check and RENAME.
fn link_falls_back(code: u32) -> bool {
    matches!(
        code,
        status::PERM
            | status::XDEV
            | status::ISDIR
            | status::INVAL
            | status::NOTSUPP
            | status::WRONG_TYPE
    )
}

impl Nfs4 {
    /// Runs `f`; on STALE every cached handle is dropped and it runs once more.
    fn retry_stale<T>(&mut self, mut f: impl FnMut(&mut Self) -> io::Result<T>) -> io::Result<T> {
        match f(self) {
            Err(e) if is_stale(&e) => {
                self.handles.clear();
                f(self)
            }
            r => r,
        }
    }

    /// Renames `from` to `to`, never replacing what is at `to`: LINK then REMOVE (LINK
    /// refuses a taken name); where there are no hard links, a check that `to` is free and
    /// RENAME.
    // ponytail: no identity: REMOVE goes by name, so a file another client put at `from`
    // between LINK and REMOVE is the one removed (NFS has no conditional remove).
    pub fn rename_no_replace(&mut self, from: &str, to: &str) -> io::Result<()> {
        let (from_dir, from_name) = parent(from, ErrorKind::ReadOnlyFilesystem)?;
        let (to_dir, to_name) = parent(to, ErrorKind::AlreadyExists)?;
        let (from_key, to_key) = (key(&components(from)?), key(&components(to)?));
        let code = self.retry_stale(|s| {
            let file = s.handle(from)?;
            let dir = s.handle(&to_dir)?;
            s.link(&file, &dir, &to_name)
        })?;
        self.handles.forget(&to_key);
        if code == status::OK {
            let removed = self.with_handle(&from_dir, |s, dh| s.remove_in(dh, &from_name));
            self.handles.forget(&from_key);
            return removed.map_err(|e| {
                io::Error::new(e.kind(), format!("{to} was made, but {from} stays: {e}"))
            });
        }
        // ponytail: two steps; something made at `to` between the check and the RENAME is
        // replaced (RENAME has no no-replace flag).
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
        let (from_dir, from_name) = parent(from, ErrorKind::ReadOnlyFilesystem)?;
        let (to_dir, to_name) = parent(to, ErrorKind::AlreadyExists)?;
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

    /// LINK of `file` as `name` in the folder `dir`: `OK`, or the refusal's code when it is
    /// one [`link_falls_back`] takes (other failures are errors).
    fn link(&mut self, file: &[u8], dir: &[u8], name: &str) -> io::Result<u32> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(file);
        c.op(op::SAVEFH);
        c.op(op::PUTFH).opaque(dir);
        c.op(op::LINK).string(name);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            w.op(op::SAVEFH)?;
            w.op(op::PUTFH)?;
            let r = match w.op(op::LINK) {
                Ok(r) => r,
                Err(e) => {
                    let code = w.status();
                    return if link_falls_back(code) {
                        Ok(code)
                    } else {
                        Err(e)
                    };
                }
            };
            skip_cinfo(r)?;
            w.finish()?;
            Ok(status::OK)
        })
    }

    /// RENAME of `from` (folder handle, name) to `to` (replacing what is there).
    fn rename(&mut self, from: (&[u8], &str), to: (&[u8], &str)) -> io::Result<()> {
        let mut c = self.sequenced();
        c.op(op::PUTFH).opaque(from.0);
        c.op(op::SAVEFH);
        c.op(op::PUTFH).opaque(to.0);
        c.op(op::RENAME).string(from.1).string(to.1);
        self.run(c, |w| {
            w.op(op::PUTFH)?;
            w.op(op::SAVEFH)?;
            w.op(op::PUTFH)?;
            let r = w.op(op::RENAME)?;
            skip_cinfo(r)?; // source
            skip_cinfo(r)?; // target
            w.finish()
        })
    }
}

impl Drop for Nfs4 {
    /// DESTROY_SESSION, then DESTROY_CLIENTID (best effort: the lease ends them anyway).
    fn drop(&mut self) {
        let mut c = Compound::new();
        c.op(op::DESTROY_SESSION).fixed(&self.session);
        let bytes = c.into_bytes();
        if self
            .client
            .call(PROGRAM, VERSION, PROC_COMPOUND, &bytes)
            .is_ok()
        {
            let mut c = Compound::new();
            c.op(op::DESTROY_CLIENTID).u64(self.clientid);
            let _ = self
                .client
                .call(PROGRAM, VERSION, PROC_COMPOUND, &c.into_bytes());
        }
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

/// The `fattr4` attribute numbers this client asks for (RFC 8881 5.6, 5.7).
mod attr {
    pub const TYPE: u32 = 1;
    pub const SIZE: u32 = 4;
    pub const FILEHANDLE: u32 = 19;
    pub const FILEID: u32 = 20;
    pub const MODE: u32 = 33;
    pub const SPACE_AVAIL: u32 = 42;
    pub const TIME_MODIFY: u32 = 53;
}

/// A `bitmap4` of attribute numbers (each below 64).
const fn bits(attrs: &[u32]) -> [u32; 2] {
    let mut words = [0u32; 2];
    let mut i = 0;
    while i < attrs.len() {
        words[(attrs[i] / 32) as usize] |= 1 << (attrs[i] % 32);
        i += 1;
    }
    words
}

/// What GETATTR asks for a stat; READDIR asks the same plus each entry's handle.
const STAT_ATTRS: [u32; 2] = bits(&[attr::TYPE, attr::SIZE, attr::FILEID, attr::TIME_MODIFY]);
const LIST_ATTRS: [u32; 2] = bits(&[
    attr::TYPE,
    attr::SIZE,
    attr::FILEHANDLE,
    attr::FILEID,
    attr::TIME_MODIFY,
]);
const FREE_ATTRS: [u32; 2] = bits(&[attr::SPACE_AVAIL]);

/// Appends a `bitmap4`.
fn put_bitmap(x: &mut Xdr, words: [u32; 2]) {
    x.u32(2).u32(words[0]).u32(words[1]);
}

/// `nfs_ftype4` values.
const NF4REG: u32 = 1;
const NF4DIR: u32 = 2;
const NF4LNK: u32 = 5;
/// Largest `nfs_fh4` (NFS4_FHSIZE).
const FHSIZE: usize = 128;

/// The attributes of one `fattr4` this client reads; those the server left out are `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Fattr {
    pub kind: Option<u32>,
    pub size: Option<u64>,
    pub fh: Option<Vec<u8>>,
    pub fileid: Option<u64>,
    pub space_avail: Option<u64>,
    /// `time_modify`: seconds since the epoch (may be negative) and nanoseconds.
    pub mtime: Option<(i64, u32)>,
}

impl Fattr {
    /// Reads a `fattr4` (bitmap, then the values in attribute order). An attribute this
    /// client didn't ask for can't be skipped (its size is unknown), so it is refused.
    pub fn read(r: &mut XdrReader) -> io::Result<Fattr> {
        let n = r.len(8)?;
        let mut words = [0u32; 8];
        for w in words.iter_mut().take(n) {
            *w = r.u32()?;
        }
        let vals = r.opaque(1 << 20)?;
        let mut v = XdrReader::new(vals);
        let mut a = Fattr::default();
        for (i, &w) in words.iter().enumerate() {
            for b in 0..32 {
                if w & (1 << b) == 0 {
                    continue;
                }
                match i as u32 * 32 + b {
                    attr::TYPE => a.kind = Some(v.u32()?),
                    attr::SIZE => a.size = Some(v.u64()?),
                    attr::FILEHANDLE => a.fh = Some(v.opaque(FHSIZE)?.to_vec()),
                    attr::FILEID => a.fileid = Some(v.u64()?),
                    attr::SPACE_AVAIL => a.space_avail = Some(v.u64()?),
                    attr::TIME_MODIFY => {
                        let secs = v.u64()? as i64;
                        let nsecs = v.u32()?;
                        if nsecs >= 1_000_000_000 {
                            return Err(rpc::bad("NFSv4 time_modify has nseconds >= 1e9"));
                        }
                        a.mtime = Some((secs, nsecs));
                    }
                    other => {
                        return Err(rpc::bad(format!(
                            "NFSv4 server sent attribute {other} unasked"
                        )));
                    }
                }
            }
        }
        if v.remaining() != 0 {
            return Err(rpc::bad("NFSv4 fattr4 has bytes past its attributes"));
        }
        Ok(a)
    }

    /// The entry named `name` these attributes describe (as v3's: links and special files
    /// marked, a folder's size 0). Without a type it can't be told: an error.
    pub fn entry(&self, name: String) -> io::Result<Entry> {
        let kind = self
            .kind
            .ok_or_else(|| rpc::bad("NFSv4 server sent no type"))?;
        let dir = kind == NF4DIR;
        let mtime = self.mtime.and_then(|(s, n)| {
            let epoch = std::time::UNIX_EPOCH;
            if s >= 0 {
                epoch.checked_add(std::time::Duration::new(s as u64, n))
            } else {
                epoch
                    .checked_sub(std::time::Duration::from_secs(s.unsigned_abs()))?
                    .checked_add(std::time::Duration::from_nanos(n.into()))
            }
        });
        Ok(Entry {
            name,
            dir,
            size: if dir { 0 } else { self.size.unwrap_or(0) },
            mtime,
            link: kind == NF4LNK,
            special: !matches!(kind, NF4REG | NF4DIR | NF4LNK),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A COMPOUND4res: overall status, tag, then (op, status, body words) per result.
    fn reply(overall: u32, tag: &str, results: &[(u32, u32, &[u32])]) -> Vec<u8> {
        let mut x = Xdr::new();
        x.u32(overall).string(tag).u32(results.len() as u32);
        for (op, code, body) in results {
            x.u32(*op).u32(*code);
            for w in *body {
                x.u32(*w);
            }
        }
        x.into_bytes()
    }

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks(4)
            .map(|c| u32::from_be_bytes(c.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn compound_encodes_tag_minor_version_and_ops() {
        let mut c = Compound::new();
        c.op(op::PUTROOTFH);
        c.op(op::LOOKUP).string("games");
        c.op(op::GETATTR).u32(1).u32(0x0000_0102);
        assert_eq!(c.len(), 3);
        let mut want = vec![0, 1, 3, op::PUTROOTFH, op::LOOKUP, 5];
        want.push(u32::from_be_bytes(*b"game"));
        want.push(u32::from_be_bytes(*b"s\0\0\0"));
        want.extend([op::GETATTR, 1, 0x0000_0102]);
        assert_eq!(words(&c.into_bytes()), want);
    }

    #[test]
    fn walk_refuses_a_result_for_another_op() {
        let buf = reply(0, "", &[(op::GETFH, 0, &[0])]);
        let mut w = Walk::new(XdrReader::new(&buf)).unwrap();
        assert_eq!(w.op(op::PUTFH).unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn walk_finish_refuses_unread_results_and_truncation() {
        let buf = reply(0, "", &[(op::SAVEFH, 0, &[]), (op::RESTOREFH, 0, &[])]);
        let mut w = Walk::new(XdrReader::new(&buf)).unwrap();
        w.op(op::SAVEFH).unwrap();
        assert_eq!(w.finish().unwrap_err().kind(), ErrorKind::InvalidData);
        // A body cut off mid-result.
        let buf = reply(0, "", &[(op::SAVEFH, 0, &[])]);
        let mut w = Walk::new(XdrReader::new(&buf[..buf.len() - 4])).unwrap();
        assert_eq!(w.op(op::SAVEFH).unwrap_err().kind(), ErrorKind::InvalidData);
        // A hostile result count.
        let mut x = Xdr::new();
        x.u32(0).string("").u32(u32::MAX);
        assert!(Walk::new(XdrReader::new(&x.into_bytes())).is_err());
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A server answering each call with `answer(proc, args, reply)` (the reply's header up to
    /// the verifier already in; `answer` writes the accept status and the body) until the
    /// client hangs up.
    pub(super) fn serve(
        listener: TcpListener,
        mut answer: impl FnMut(u32, &mut XdrReader, &mut Xdr),
    ) {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = [0u8; 4];
        while s.read_exact(&mut head).is_ok() {
            let mut call = vec![0; (u32::from_be_bytes(head) & 0x7fff_ffff) as usize];
            s.read_exact(&mut call).unwrap();
            let mut r = XdrReader::new(&call);
            let xid = r.u32().unwrap();
            let [_, _, prog, _, proc] = [(); 5].map(|_| r.u32().unwrap());
            assert_eq!(prog, PROGRAM);
            for _ in 0..2 {
                r.u32().unwrap();
                r.opaque(400).unwrap();
            }
            let mut x = Xdr::new();
            x.u32(xid).u32(1).u32(0).u32(0).u32(0);
            answer(proc, &mut r, &mut x);
            let mut out = (0x8000_0000 | x.buf.len() as u32).to_be_bytes().to_vec();
            out.extend_from_slice(&x.buf);
            s.write_all(&out).unwrap();
        }
    }

    pub(super) const SESSION: [u8; 16] = [9; 16];

    /// A v4.1 server's answers: every op it saw (SEQUENCE as `(op, seqid)`), RECLAIM_COMPLETE
    /// answered with `reclaim`.
    fn session_server(listener: TcpListener, reclaim: u32) -> Vec<(u32, u32)> {
        let mut seen = Vec::new();
        serve(listener, |proc, r, x| {
            x.u32(0); // SUCCESS
            if proc == 0 {
                return;
            }
            assert_eq!(r.opaque(64).unwrap(), b"");
            assert_eq!(r.u32().unwrap(), MINOR_VERSION);
            let count = r.u32().unwrap();
            let first = r.u32().unwrap();
            match first {
                op::EXCHANGE_ID => {
                    r.fixed(8).unwrap();
                    let owner = r.opaque(1024).unwrap();
                    assert!(owner.starts_with(b"ps5-dump-forge "), "{owner:?}");
                    assert_eq!([r.u32().unwrap(), r.u32().unwrap()], [0, SP4_NONE]);
                    assert_eq!(r.u32().unwrap(), 0);
                    seen.push((first, 0));
                    x.u32(0).string("").u32(1).u32(first).u32(0);
                    x.u64(0x1122).u32(7).u32(0).u32(SP4_NONE);
                    x.u64(1).opaque(b"srv").opaque(b"scope");
                    x.u32(1).string("dom").string("fake").u64(0).u32(0);
                }
                op::CREATE_SESSION => {
                    assert_eq!((r.u64().unwrap(), r.u32().unwrap()), (0x1122, 7));
                    assert_eq!(r.u32().unwrap(), 0, "no flags");
                    let fore = read_channel(r).unwrap();
                    assert_eq!(
                        (fore.0, fore.1, fore.2),
                        (ASK_MAX_REQUEST, ASK_MAX_RESPONSE, 1)
                    );
                    read_channel(r).unwrap();
                    assert_eq!([0, 1, 0].map(|_| r.u32().unwrap()), [0, 1, 0]);
                    seen.push((first, 0));
                    x.u32(0).string("").u32(1).u32(first).u32(0);
                    x.fixed(&SESSION).u32(7).u32(0);
                    channel(x, 1 << 20, 1 << 20, 4096, 8);
                    channel(x, 4096, 4096, 0, 2);
                }
                op::SEQUENCE => {
                    assert_eq!(r.fixed(16).unwrap(), SESSION);
                    let seqid = r.u32().unwrap();
                    assert_eq!([r.u32().unwrap(), r.u32().unwrap()], [0, 0]);
                    assert!(!r.bool().unwrap());
                    seen.push((first, seqid));
                    let next = (count > 1).then(|| r.u32().unwrap());
                    let status = if next.is_some() { reclaim } else { 0 };
                    x.u32(status).string("").u32(count).u32(first).u32(0);
                    x.fixed(&SESSION).u32(seqid).u32(0).u32(0).u32(0).u32(0);
                    if let Some(next) = next {
                        assert_eq!(next, op::RECLAIM_COMPLETE);
                        assert!(!r.bool().unwrap());
                        seen.push((next, 0));
                        x.u32(next).u32(reclaim);
                    }
                }
                op::DESTROY_SESSION | op::DESTROY_CLIENTID => {
                    if first == op::DESTROY_SESSION {
                        assert_eq!(r.fixed(16).unwrap(), SESSION);
                    } else {
                        assert_eq!(r.u64().unwrap(), 0x1122);
                    }
                    seen.push((first, 0));
                    x.u32(0).string("").u32(1).u32(first).u32(0);
                }
                other => panic!("unexpected op {other}"),
            }
            assert_eq!(r.remaining(), 0, "op {first} arguments read in full");
        });
        seen
    }

    pub(super) fn listen() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn connect_sets_up_a_session_and_steps_the_slot_seqid() {
        for reclaim in [status::OK, status::COMPLETE_ALREADY] {
            let (listener, port) = listen();
            let server = std::thread::spawn(move || session_server(listener, reclaim));
            let mut nfs = Nfs4::connect("127.0.0.1", Some(port)).unwrap();
            assert_eq!(nfs.exports().unwrap(), ["/"]);
            assert_eq!((nfs.max_request, nfs.max_response), (1 << 20, 1 << 20));
            assert_eq!(nfs.max_ops, 8);
            for _ in 0..2 {
                let c = nfs.sequenced();
                nfs.run(c, |w| w.finish()).unwrap();
            }
            drop(nfs);
            use op::*;
            assert_eq!(
                server.join().unwrap(),
                [
                    (EXCHANGE_ID, 0),
                    (CREATE_SESSION, 0),
                    (SEQUENCE, 1),
                    (RECLAIM_COMPLETE, 0),
                    (SEQUENCE, 2),
                    (SEQUENCE, 3),
                    (DESTROY_SESSION, 0),
                    (DESTROY_CLIENTID, 0),
                ]
            );
        }
    }

    #[test]
    fn no_minor_version_1_is_unsupported() {
        let (listener, port) = listen();
        let server = std::thread::spawn(move || {
            serve(listener, |proc, _, x| {
                x.u32(0);
                if proc == PROC_COMPOUND {
                    x.u32(status::MINOR_VERS_MISMATCH).string("").u32(0);
                }
            })
        });
        let e = Nfs4::connect("127.0.0.1", Some(port)).err().unwrap();
        assert_eq!(e.kind(), ErrorKind::Unsupported, "{e}");
        assert!(e.to_string().contains("NFS4ERR_MINOR_VERS_MISMATCH"), "{e}");
        server.join().unwrap();
    }
}

#[cfg(test)]
mod attr_tests {
    use super::*;

    /// A `fattr4` with `words` set and `vals` as its attribute values.
    pub(super) fn fattr(x: &mut Xdr, words: &[u32], vals: &Xdr) {
        x.u32(words.len() as u32);
        for w in words {
            x.u32(*w);
        }
        x.opaque(&vals.buf);
    }

    #[test]
    fn fattr4_refuses_what_it_cannot_read() {
        let bad = |words: &[u32], v: &Xdr| {
            let mut x = Xdr::new();
            fattr(&mut x, words, v);
            Fattr::read(&mut XdrReader::new(&x.buf)).unwrap_err()
        };
        let mut v = Xdr::new();
        v.u32(0);
        bad(&[1 << 0], &v); // supported_attrs: unasked, size unknown
        let mut v = Xdr::new();
        v.u32(NF4REG).u32(9);
        bad(&[1 << 1], &v); // bytes left over
        let mut v = Xdr::new();
        v.u64(1).u32(1_000_000_000);
        bad(&[0, 1 << 21], &v); // nseconds out of range
        bad(&[1 << 4], &Xdr::new()); // size cut short
        let mut x = Xdr::new();
        x.u32(9); // more bitmap words than any server needs
        assert!(Fattr::read(&mut XdrReader::new(&x.buf)).is_err());
    }
}

#[cfg(test)]
mod file_tests {
    use super::session_tests::{SESSION, listen, serve};
    use super::*;
    use std::net::TcpListener;

    const DATA: &[u8] = b"0123456789";
    const NON_UTF8: &[u8] = b"/games/\xff";

    /// The fake tree: (kind, fileid) of `path`; everything under `/deep` is a folder.
    fn node(path: &[u8]) -> Option<(u32, u64)> {
        match path {
            b"/" | b"/games" | b"/games/sub" => Some((NF4DIR, path.len() as u64)),
            b"/games/a.bin" => Some((NF4REG, 3)),
            p if p == NON_UTF8 => Some((NF4REG, 4)),
            p if p.starts_with(b"/deep") => Some((NF4DIR, 5)),
            _ => None,
        }
    }

    fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
        let mut p = dir.to_vec();
        if p != b"/" {
            p.push(b'/');
        }
        p.extend_from_slice(name);
        p
    }

    /// The attributes in `words` the fake has for `path` (handle = `H` + the path).
    fn attrs(x: &mut Xdr, path: &[u8], words: &[u32]) {
        let (kind, fileid) = node(path).unwrap();
        let has = |a: u32| {
            words
                .get(a as usize / 32)
                .is_some_and(|w| w & (1 << (a % 32)) != 0)
        };
        let mut v = Xdr::new();
        let mut sent = [0u32; 2];
        for a in [attr::TYPE, attr::SIZE, attr::FILEHANDLE, attr::FILEID] {
            if has(a) {
                sent[a as usize / 32] |= 1 << (a % 32);
                match a {
                    attr::TYPE => v.u32(kind),
                    attr::SIZE => v.u64(if kind == NF4REG {
                        DATA.len() as u64
                    } else {
                        4096
                    }),
                    attr::FILEHANDLE => v.opaque(&[b"H", path].concat()),
                    _ => v.u64(fileid),
                };
            }
        }
        for (a, bit) in [(attr::SPACE_AVAIL, 1 << 10), (attr::TIME_MODIFY, 1 << 21)] {
            if has(a) {
                sent[1] |= bit;
                if a == attr::SPACE_AVAIL {
                    v.u64(1 << 30);
                } else {
                    v.u64(100).u32(7);
                }
            }
        }
        super::attr_tests::fattr(x, &sent, &v);
    }

    /// Answers SEQUENCE-led compounds over the fake tree (READs give at most 4 bytes,
    /// READDIR 2 entries a page) and returns every op seen. With `refuse_anon` a READ under
    /// the anonymous stateid fails with BAD_STATEID.
    fn file_server(listener: TcpListener, refuse_anon: bool) -> Vec<u32> {
        let mut seen = Vec::new();
        serve(listener, |_proc, r, x| {
            x.u32(0); // SUCCESS
            r.opaque(64).unwrap();
            r.u32().unwrap();
            let (mut status, mut done, mut body) = (0, 0, Xdr::new());
            let mut cur: Vec<u8> = Vec::new();
            for _ in 0..r.u32().unwrap() {
                let opnum = r.u32().unwrap();
                seen.push(opnum);
                let mut res = Xdr::new();
                match opnum {
                    op::SEQUENCE => {
                        assert_eq!(r.fixed(16).unwrap(), SESSION);
                        let seqid = r.u32().unwrap();
                        r.fixed(12).unwrap();
                        res.fixed(&SESSION).u32(seqid).u32(0).u32(0).u32(0).u32(0);
                    }
                    op::PUTROOTFH => cur = b"/".to_vec(),
                    op::PUTFH => cur = r.opaque(FHSIZE).unwrap()[1..].to_vec(),
                    op::LOOKUP => {
                        cur = join(&cur, r.opaque(255).unwrap());
                        status = if node(&cur).is_some() {
                            0
                        } else {
                            status::NOENT
                        };
                    }
                    op::GETFH => drop(res.opaque(&[b"H", &cur[..]].concat())),
                    op::GETATTR => {
                        let words: Vec<u32> =
                            (0..r.u32().unwrap()).map(|_| r.u32().unwrap()).collect();
                        attrs(&mut res, &cur, &words);
                    }
                    op::READDIR => {
                        let cookie = r.u64().unwrap() as usize;
                        r.fixed(8).unwrap();
                        let (_dircount, _maxcount) = (r.u32().unwrap(), r.u32().unwrap());
                        let words: Vec<u32> =
                            (0..r.u32().unwrap()).map(|_| r.u32().unwrap()).collect();
                        let names: [&[u8]; 5] = [b".", b"a.bin", b"..", b"sub", b"\xff"];
                        res.fixed(&[1; 8]);
                        let end = names.len().min(cookie + 2);
                        for (i, name) in names.iter().enumerate().take(end).skip(cookie) {
                            res.bool(true).u64(i as u64 + 1).opaque(name);
                            let path = match *name {
                                b"." | b".." => cur.clone(),
                                n => join(&cur, n),
                            };
                            attrs(&mut res, &path, &words);
                        }
                        res.bool(false).bool(end == names.len());
                    }
                    op::READ => {
                        let anon = r.fixed(16).unwrap() == ANON_STATEID;
                        let (at, count) = (r.u64().unwrap() as usize, r.u32().unwrap() as usize);
                        if anon && refuse_anon {
                            status = status::BAD_STATEID;
                        } else {
                            let data = &DATA[at.min(10)..(at + count.min(4)).min(10)];
                            res.bool(at + data.len() >= 10).opaque(data);
                        }
                    }
                    op::OPEN => {
                        let [_, access, deny] = [0; 3].map(|_| r.u32().unwrap());
                        assert_eq!((access, deny), (0x401, 0));
                        assert_eq!(r.u64().unwrap(), 0x1122);
                        assert_eq!(r.opaque(64).unwrap(), READ_OWNER);
                        assert_eq!([r.u32().unwrap(), r.u32().unwrap()], [0, CLAIM_FH]);
                        res.fixed(&[7; 16])
                            .bool(true)
                            .u64(1)
                            .u64(1)
                            .u32(0)
                            .u32(0)
                            .u32(0);
                    }
                    op::CLOSE => {
                        r.u32().unwrap();
                        assert_eq!(r.fixed(16).unwrap(), [7; 16]);
                        res.fixed(&[0; 16]);
                    }
                    op::DESTROY_SESSION => drop(r.fixed(16).unwrap()),
                    op::DESTROY_CLIENTID => drop(r.u64().unwrap()),
                    other => panic!("unexpected op {other}"),
                }
                body.u32(opnum).u32(status);
                done += 1;
                if status != 0 {
                    break;
                }
                body.fixed(&res.buf);
            }
            x.u32(status).string("").u32(done).fixed(&body.buf);
        });
        seen
    }

    /// A session on a fresh fake server (no EXCHANGE_ID/CREATE_SESSION: the fields are set
    /// as they would be), READ replies of 4 KiB at most.
    fn session(refuse_anon: bool) -> (Nfs4, std::thread::JoinHandle<Vec<u32>>) {
        let (listener, port) = listen();
        let server = std::thread::spawn(move || file_server(listener, refuse_anon));
        let addr = resolve("127.0.0.1", port).unwrap();
        let nfs = Nfs4 {
            client: rpc::Client::connect(addr).unwrap(),
            clientid: 0x1122,
            session: SESSION,
            seqid: 1,
            max_request: 1 << 20,
            max_response: HEADROOM + 4096,
            max_ops: ASK_MAX_OPS,
            handles: Handles::default(),
            open_reads: false,
        };
        (nfs, server)
    }

    fn count(seen: &[u32], opnum: u32) -> usize {
        seen.iter().filter(|o| **o == opnum).count()
    }

    #[test]
    fn stats_lists_and_reads_through_the_pseudo_fs() {
        let (mut nfs, server) = session(false);
        let a = nfs.stat("/games/a.bin").unwrap();
        assert_eq!((a.name.as_str(), a.dir, a.size), ("a.bin", false, 10));
        assert_eq!(
            nfs.stat("/games/a.bin").unwrap(),
            a,
            "second stat from the cache"
        );
        let root = nfs.stat("/").unwrap();
        assert_eq!((root.name.as_str(), root.dir, root.size), ("/", true, 0));
        let list = nfs.list("/games").unwrap();
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.bin", "sub", "\u{fffd}"]);
        assert_eq!(list[0], a, "a listing's entry equals its stat");
        assert!(list[1].dir && !list[1].special);
        assert!(
            list[2].special && !list[2].dir,
            "a non-UTF-8 name is never a path"
        );
        assert_eq!(nfs.read_at("/games/a.bin", 2, 5).unwrap(), b"23456");
        assert_eq!(nfs.read_at("/games/a.bin", 8, 10).unwrap(), b"89");
        assert_eq!(nfs.read_at("/games/a.bin", 0, 0).unwrap(), b"");
        assert_eq!(nfs.free_space("/games").unwrap(), 1 << 30);
        let e = nfs.stat("/games/nope").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::NotFound);
        assert!(nfs.stat("relative").is_err() && nfs.stat("/a/../b").is_err());
        drop(nfs);
        let seen = server.join().unwrap();
        // /games/a.bin walked once (2 LOOKUPs), /games/nope once from the cached /games.
        assert_eq!(count(&seen, op::LOOKUP), 3, "{seen:?}");
        assert_eq!(count(&seen, op::READDIR), 3, "pages of 2 entries");
        assert_eq!(count(&seen, op::READ), 2 + 1, "4-byte replies");
        assert_eq!(count(&seen, op::OPEN), 0, "the anonymous stateid sufficed");
    }

    #[test]
    fn a_walk_fits_the_ops_the_server_granted() {
        let (mut nfs, server) = session(false);
        nfs.max_ops = 8; // SEQUENCE + PUTFH + 3 LOOKUP/GETFH pairs
        nfs.stat("/deep/1/2/3/4/5/6/7/8/9").unwrap();
        drop(nfs);
        let seen = server.join().unwrap();
        assert_eq!(count(&seen, op::LOOKUP), 10, "{seen:?}");
        // 3 + 3 + 3 + 1 LOOKUPs: PUTROOTFH, then 3 PUTFHs, then the GETATTR's PUTFH.
        assert_eq!(count(&seen, op::PUTROOTFH), 1);
        assert_eq!(count(&seen, op::PUTFH), 4, "{seen:?}");
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    /// An OPEN4resok with stateid 7s and the delegation part `deleg`.
    fn open_ok(deleg: &Xdr) -> Vec<u8> {
        let mut x = Xdr::new();
        x.fixed(&[7; 16]).bool(true).u64(1).u64(2).u32(0);
        x.u32(2).u32(0).u32(bits(&[attr::MODE])[1]);
        let mut out = x.into_bytes();
        out.extend_from_slice(&self_bytes(deleg));
        out.extend_from_slice(&[0, 0, 0, 9]); // the next result's first word
        out
    }

    fn self_bytes(x: &Xdr) -> Vec<u8> {
        x.clone().into_bytes()
    }

    #[test]
    fn open_results_are_read_past_every_delegation() {
        let mut ace = Xdr::new();
        ace.u32(0).u32(0).u32(1).string("EVERYONE@");
        let mut cases = Vec::new();
        cases.push(Xdr::new().u32(OPEN_DELEGATE_NONE).clone());
        let mut read = Xdr::new();
        read.u32(OPEN_DELEGATE_READ).fixed(&[1; 16]).bool(false);
        read.fixed(&self_bytes(&ace));
        cases.push(read);
        for limit in [
            Xdr::new().u32(1).u64(5).clone(),
            Xdr::new().u32(2).u32(3).u32(4).clone(),
        ] {
            let mut w = Xdr::new();
            w.u32(OPEN_DELEGATE_WRITE).fixed(&[2; 16]).bool(true);
            w.fixed(&self_bytes(&limit)).fixed(&self_bytes(&ace));
            cases.push(w);
        }
        cases.push(Xdr::new().u32(OPEN_DELEGATE_NONE_EXT).u32(0).clone());
        cases.push(
            Xdr::new()
                .u32(OPEN_DELEGATE_NONE_EXT)
                .u32(1)
                .bool(true)
                .clone(),
        );
        for (i, deleg) in cases.iter().enumerate() {
            let buf = open_ok(deleg);
            let mut r = XdrReader::new(&buf);
            assert_eq!(open_result(&mut r).unwrap(), [7; 16], "case {i}");
            assert_eq!(r.u32().unwrap(), 9, "case {i}: read exactly its own bytes");
        }
        for bad in [Xdr::new().u32(9).clone(), {
            let mut w = Xdr::new();
            w.u32(OPEN_DELEGATE_WRITE).fixed(&[2; 16]).bool(true).u32(7);
            w
        }] {
            let buf = open_ok(&bad);
            let e = open_result(&mut XdrReader::new(&buf)).unwrap_err();
            assert_eq!(e.kind(), ErrorKind::InvalidData);
        }
    }

    #[test]
    fn the_write_verifier_must_hold_until_commit() {
        let open = || Open {
            fh: vec![1],
            stateid: [0; 16],
            verf: Mutex::default(),
        };
        let o = open();
        o.wrote([1; 8]);
        o.wrote([1; 8]);
        o.committed([1; 8]).unwrap();
        o.wrote([2; 8]); // a new verifier after a commit is fine
        o.committed([2; 8]).unwrap();
        o.committed([3; 8]).unwrap(); // nothing written since: nothing to lose
        let o = open();
        o.wrote([1; 8]);
        assert!(o.committed([2; 8]).is_err(), "COMMIT's verifier differs");
        assert!(o.committed([2; 8]).is_err(), "and stays failed");
        let o = open();
        o.wrote([1; 8]);
        o.wrote([2; 8]);
        assert!(o.committed([2; 8]).is_err(), "the WRITEs' verifier changed");
    }

    use super::session_tests::{SESSION, listen, serve};
    use std::collections::BTreeMap;
    use std::net::TcpListener;

    /// The fake server's files (`Some(data)`) and folders (`None`) by path; the handle of a
    /// path is `H` + the path.
    type Disk = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

    fn under(dir: &[u8], name: &[u8]) -> Vec<u8> {
        if dir == b"/" {
            [b"/", name].concat()
        } else {
            [dir, b"/", name].concat()
        }
    }

    /// A `fattr4` setting the mode: its value.
    fn mode_of(r: &mut XdrReader) -> u32 {
        let words: Vec<u32> = (0..r.u32().unwrap()).map(|_| r.u32().unwrap()).collect();
        assert_eq!(words, [0, bits(&[attr::MODE])[1]]);
        let v = r.opaque(64).unwrap();
        u32::from_be_bytes(v.try_into().unwrap())
    }

    /// A v4.1 server writing into `disk`: WRITEs take 3 bytes at most; `link` false answers
    /// LINK NOTSUPP, `guarded` false GUARDED4 NOTSUPP; a COMMIT of a file named `restart` gives
    /// another verifier. Every op it saw, and the disk at the end.
    fn disk_server(listener: TcpListener, link: bool, guarded: bool) -> (Vec<u32>, Disk) {
        let mut disk = Disk::new();
        disk.insert(b"/".to_vec(), None);
        let mut seen = Vec::new();
        serve(listener, |_proc, r, x| {
            x.u32(0); // SUCCESS
            r.opaque(64).unwrap();
            r.u32().unwrap();
            let (mut status, mut done, mut body) = (0, 0, Xdr::new());
            let (mut cur, mut saved): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
            for _ in 0..r.u32().unwrap() {
                let opnum = r.u32().unwrap();
                seen.push(opnum);
                let mut res = Xdr::new();
                let cinfo = |res: &mut Xdr| {
                    res.bool(true).u64(1).u64(2);
                };
                match opnum {
                    op::SEQUENCE => {
                        assert_eq!(r.fixed(16).unwrap(), SESSION);
                        let seqid = r.u32().unwrap();
                        r.fixed(12).unwrap();
                        res.fixed(&SESSION).u32(seqid).u32(0).u32(0).u32(0).u32(0);
                    }
                    op::PUTROOTFH => cur = b"/".to_vec(),
                    op::PUTFH => cur = r.opaque(FHSIZE).unwrap()[1..].to_vec(),
                    op::SAVEFH => saved = cur.clone(),
                    op::GETFH => drop(res.opaque(&[b"H", &cur[..]].concat())),
                    op::LOOKUP => {
                        cur = under(&cur, r.opaque(255).unwrap());
                        if !disk.contains_key(&cur) {
                            status = status::NOENT;
                        }
                    }
                    op::OPEN => {
                        let [_, access, deny] = [0; 3].map(|_| r.u32().unwrap());
                        assert_eq!((access, deny), (0x402, 0));
                        assert_eq!(r.u64().unwrap(), 0x1122);
                        assert_eq!(r.opaque(64).unwrap(), WRITE_OWNER);
                        assert_eq!(r.u32().unwrap(), OPEN4_CREATE);
                        let how = r.u32().unwrap();
                        if how == EXCLUSIVE4_1 {
                            r.fixed(8).unwrap();
                        }
                        assert_eq!(mode_of(r), 0o666);
                        assert_eq!(r.u32().unwrap(), CLAIM_NULL);
                        let path = under(&cur, r.opaque(255).unwrap());
                        if how == GUARDED4 && !guarded {
                            status = status::NOTSUPP;
                        } else if disk.contains_key(&path) {
                            status = status::EXIST;
                        } else {
                            disk.insert(path.clone(), Some(Vec::new()));
                            res.fixed(&[5; 16]);
                            cinfo(&mut res);
                            res.u32(0).u32(0).u32(OPEN_DELEGATE_NONE);
                            cur = path;
                        }
                    }
                    op::WRITE => {
                        assert_eq!(r.fixed(16).unwrap(), [5; 16]);
                        let at = r.u64().unwrap() as usize;
                        assert_eq!(r.u32().unwrap(), UNSTABLE4);
                        let data = r.opaque(1 << 20).unwrap();
                        let take = &data[..data.len().min(3)];
                        let file = disk.get_mut(&cur).unwrap().as_mut().unwrap();
                        if file.len() < at + take.len() {
                            file.resize(at + take.len(), 0);
                        }
                        file[at..at + take.len()].copy_from_slice(take);
                        res.u32(take.len() as u32).u32(0).fixed(&[1; 8]);
                    }
                    op::COMMIT => {
                        assert_eq!((r.u64().unwrap(), r.u32().unwrap()), (0, 0));
                        res.fixed(&[if cur.ends_with(b"/restart") { 2 } else { 1 }; 8]);
                    }
                    op::GETATTR => {
                        // Only the type, whatever was asked.
                        for _ in 0..r.u32().unwrap() {
                            r.u32().unwrap();
                        }
                        let kind = if disk[&cur].is_some() { NF4REG } else { NF4DIR };
                        put_bitmap(&mut res, bits(&[attr::TYPE]));
                        res.opaque(&kind.to_be_bytes());
                    }
                    op::CLOSE => {
                        r.u32().unwrap();
                        assert_eq!(r.fixed(16).unwrap(), [5; 16]);
                        res.fixed(&[0; 16]);
                    }
                    op::CREATE => {
                        assert_eq!(r.u32().unwrap(), NF4DIR);
                        let path = under(&cur, r.opaque(255).unwrap());
                        assert_eq!(mode_of(r), 0o777);
                        if disk.insert(path, None).is_some() {
                            status = status::EXIST;
                        }
                        cinfo(&mut res);
                        res.u32(0);
                    }
                    op::REMOVE => {
                        let path = under(&cur, r.opaque(255).unwrap());
                        if disk.remove(&path).is_none() {
                            status = status::NOENT;
                        }
                        cinfo(&mut res);
                    }
                    op::RENAME => {
                        let from = under(&saved, r.opaque(255).unwrap());
                        let to = under(&cur, r.opaque(255).unwrap());
                        match disk.remove(&from) {
                            Some(node) => drop(disk.insert(to, node)),
                            None => status = status::NOENT,
                        }
                        cinfo(&mut res);
                        cinfo(&mut res);
                    }
                    op::LINK => {
                        let to = under(&cur, r.opaque(255).unwrap());
                        if !link {
                            status = status::NOTSUPP;
                        } else if disk.contains_key(&to) {
                            status = status::EXIST;
                        } else {
                            let node = disk[&saved].clone();
                            disk.insert(to, node);
                        }
                        cinfo(&mut res);
                    }
                    op::DESTROY_SESSION => drop(r.fixed(16).unwrap()),
                    op::DESTROY_CLIENTID => drop(r.u64().unwrap()),
                    other => panic!("unexpected op {other}"),
                }
                body.u32(opnum).u32(status);
                done += 1;
                if status != 0 {
                    break;
                }
                body.fixed(&res.buf);
            }
            x.u32(status).string("").u32(done).fixed(&body.buf);
        });
        (seen, disk)
    }

    /// A session on a fresh [`disk_server`]; WRITEs of 1 KiB at most per request.
    fn session(link: bool, guarded: bool) -> (Nfs4, std::thread::JoinHandle<(Vec<u32>, Disk)>) {
        let (listener, port) = listen();
        let server = std::thread::spawn(move || disk_server(listener, link, guarded));
        let nfs = Nfs4 {
            client: rpc::Client::connect(resolve("127.0.0.1", port).unwrap()).unwrap(),
            clientid: 0x1122,
            session: SESSION,
            seqid: 1,
            max_request: HEADROOM + 1024,
            max_response: 1 << 20,
            max_ops: ASK_MAX_OPS,
            handles: Handles::default(),
            open_reads: false,
        };
        (nfs, server)
    }

    fn count(seen: &[u32], opnum: u32) -> usize {
        seen.iter().filter(|&&o| o == opnum).count()
    }

    #[test]
    fn creates_writes_renames_and_removes() {
        let (mut nfs, server) = session(true, true);
        nfs.mkdir("/d").unwrap();
        assert_eq!(
            nfs.mkdir("/d").unwrap_err().kind(),
            ErrorKind::AlreadyExists
        );
        let o = nfs.create("/d/f").unwrap();
        assert_eq!(
            nfs.create("/d/f").err().unwrap().kind(),
            ErrorKind::AlreadyExists
        );
        let big: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        nfs.write_at(&o, 5, &big).unwrap(); // 1 KiB requests, 3 bytes taken each
        nfs.write_at(&o, 0, b"hello").unwrap();
        nfs.sync(&o).unwrap();
        nfs.close(&o);
        nfs.rename_no_replace("/d/f", "/d/g").unwrap();
        let h = nfs.create("/d/h").unwrap();
        nfs.close(&h);
        let e = nfs.rename_no_replace("/d/g", "/d/h").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::AlreadyExists);
        nfs.remove("/d/h").unwrap();
        assert_eq!(nfs.remove("/d/h").unwrap_err().kind(), ErrorKind::NotFound);
        assert_eq!(
            nfs.remove("/d").unwrap_err().kind(),
            ErrorKind::IsADirectory
        );
        assert_eq!(nfs.mkdir("/").unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert_eq!(
            nfs.remove("/").unwrap_err().kind(),
            ErrorKind::ReadOnlyFilesystem
        );
        let r = nfs.create("/restart").unwrap();
        nfs.write_at(&r, 0, b"x").unwrap();
        assert!(nfs.sync(&r).unwrap_err().to_string().contains("restarted"));
        drop(nfs);
        let (seen, disk) = server.join().unwrap();
        let mut want = b"hello".to_vec();
        want.extend_from_slice(&big);
        assert_eq!(disk[&b"/d/g".to_vec()], Some(want));
        assert!(!disk.contains_key(b"/d/f".as_slice()) && !disk.contains_key(b"/d/h".as_slice()));
        assert_eq!(count(&seen, op::RENAME), 0, "renamed by LINK + REMOVE");
        assert_eq!(count(&seen, op::WRITE), 2500usize.div_ceil(3) + 2 + 1);
        assert_eq!(count(&seen, op::COMMIT), 2);
        assert_eq!(count(&seen, op::CLOSE), 2);
    }

    #[test]
    fn without_links_or_guarded_create_it_falls_back() {
        let (mut nfs, server) = session(false, false);
        for path in ["/x", "/z"] {
            let o = nfs.create(path).unwrap(); // GUARDED4 refused: EXCLUSIVE4_1
            nfs.close(&o);
        }
        nfs.rename_no_replace("/x", "/y").unwrap(); // LINK refused: check, RENAME
        let e = nfs.rename_no_replace("/y", "/z").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::AlreadyExists);
        assert_eq!(
            nfs.create("/z").err().unwrap().kind(),
            ErrorKind::AlreadyExists
        );
        drop(nfs);
        let (seen, disk) = server.join().unwrap();
        assert!(disk.contains_key(b"/y".as_slice()) && disk.contains_key(b"/z".as_slice()));
        assert!(!disk.contains_key(b"/x".as_slice()));
        assert_eq!(count(&seen, op::OPEN), 6);
        assert_eq!(count(&seen, op::RENAME), 1, "none over the taken /z");
    }
}
