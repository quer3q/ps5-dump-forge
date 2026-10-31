//! ONC RPC v2 (RFC 5531) over TCP: record marking (RFC 5531 §11), XDR (RFC 4506) by hand,
//! AUTH_SYS credentials, one call at a time with its reply matched by xid.
// ponytail: AUTH_SYS as root (uid 0, gid 0, machine "forge"): a `root_squash` export maps it to
// the anonymous user; no RPCSEC_GSS (Kerberos).

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// Largest reply record accepted (a READ or READDIR reply is sized well below it by the caller).
pub const MAX_RECORD: usize = 16 * 1024 * 1024;
/// Replies to other xids (left over from a call cut short) skipped before giving up.
const MAX_SKIPPED: usize = 64;
const LAST_FRAGMENT: u32 = 0x8000_0000;
/// Empty fragments one record may hold (legal, but endless ones would never end the record).
const MAX_EMPTY_FRAGMENTS: usize = 64;

const RPC_VERSION: u32 = 2;
const CALL: u32 = 0;
const REPLY: u32 = 1;
const MSG_ACCEPTED: u32 = 0;
const MSG_DENIED: u32 = 1;
const AUTH_NONE: u32 = 0;
const AUTH_SYS: u32 = 1;
const MACHINE: &str = "forge";

/// An RPC-level refusal (the call never reached the procedure). Carried inside an `io::Error`;
/// [`rpc_error`] gets it back, [`is_version_mismatch`] tells a version fallback apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    /// The server doesn't serve this program on this port.
    ProgUnavail,
    /// The program is served, only in versions `low..=high`.
    ProgMismatch {
        low: u32,
        high: u32,
    },
    ProcUnavail,
    GarbageArgs,
    SystemErr,
    /// The server speaks RPC versions `low..=high`, not 2.
    RpcMismatch {
        low: u32,
        high: u32,
    },
    /// Credentials refused (`auth_stat`, RFC 5531 §9).
    Auth(u32),
}

impl RpcError {
    fn kind(&self) -> io::ErrorKind {
        match self {
            RpcError::ProgUnavail
            | RpcError::ProgMismatch { .. }
            | RpcError::ProcUnavail
            | RpcError::RpcMismatch { .. } => io::ErrorKind::Unsupported,
            RpcError::Auth(_) => io::ErrorKind::PermissionDenied,
            RpcError::GarbageArgs | RpcError::SystemErr => io::ErrorKind::Other,
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::ProgUnavail => write!(f, "the server doesn't offer this RPC program"),
            RpcError::ProgMismatch { low, high } => {
                write!(
                    f,
                    "the server offers versions {low} to {high} of this RPC program"
                )
            }
            RpcError::ProcUnavail => write!(f, "the server doesn't offer this RPC procedure"),
            RpcError::GarbageArgs => write!(f, "the server couldn't decode the RPC arguments"),
            RpcError::SystemErr => write!(f, "the server failed the RPC call (system error)"),
            RpcError::RpcMismatch { low, high } => {
                write!(f, "the server speaks RPC versions {low} to {high}, not 2")
            }
            RpcError::Auth(stat) => {
                let what = match stat {
                    1 => "bad credentials",
                    2 => "credentials rejected",
                    3 => "bad verifier",
                    4 => "verifier rejected",
                    5 => "authentication too weak (the server may want a privileged source port)",
                    6 => "bogus response verifier",
                    _ => "authentication failed",
                };
                write!(f, "the server refused the RPC call: {what}")
            }
        }
    }
}

impl std::error::Error for RpcError {}

impl From<RpcError> for io::Error {
    fn from(e: RpcError) -> io::Error {
        io::Error::new(e.kind(), e)
    }
}

/// The RPC refusal inside `e`, if it is one.
pub fn rpc_error(e: &io::Error) -> Option<&RpcError> {
    e.get_ref()?.downcast_ref()
}

/// True when `e` says the server doesn't serve this program or version (try another one).
pub fn is_version_mismatch(e: &io::Error) -> bool {
    matches!(
        rpc_error(e),
        Some(RpcError::ProgUnavail | RpcError::ProgMismatch { .. } | RpcError::RpcMismatch { .. })
    )
}

pub fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// XDR encoder: big-endian 4-byte units, opaque data padded to 4.
#[derive(Debug, Default, Clone)]
pub struct Xdr {
    pub buf: Vec<u8>,
}

impl Xdr {
    pub fn new() -> Xdr {
        Xdr::default()
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u32(v as u32)
    }

    /// Fixed-length opaque: the bytes and their padding, no length.
    pub fn fixed(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self.buf.resize(self.buf.len() + pad(bytes.len()), 0);
        self
    }

    /// Variable-length opaque: the length, then the padded bytes.
    pub fn opaque(&mut self, bytes: &[u8]) -> &mut Self {
        self.u32(bytes.len() as u32).fixed(bytes)
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        self.opaque(s.as_bytes())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

fn pad(len: usize) -> usize {
    (4 - len % 4) % 4
}

/// XDR decoder over a reply body; running past the end is `InvalidData`.
#[derive(Debug, Clone)]
pub struct XdrReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> XdrReader<'a> {
    pub fn new(buf: &'a [u8]) -> XdrReader<'a> {
        XdrReader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| bad("the NFS server's reply is cut short"))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn bool(&mut self) -> io::Result<bool> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(bad(format!("bad XDR boolean {v}"))),
        }
    }

    /// Fixed-length opaque of `n` bytes (padding skipped).
    pub fn fixed(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let s = self.take(n)?;
        self.take(pad(n))?;
        Ok(s)
    }

    /// Variable-length opaque of at most `max` bytes.
    pub fn opaque(&mut self, max: usize) -> io::Result<&'a [u8]> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(bad(format!(
                "XDR opaque of {n} bytes, at most {max} expected"
            )));
        }
        self.fixed(n)
    }

    /// An array length of at most `max` elements.
    pub fn len(&mut self, max: usize) -> io::Result<usize> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(bad(format!(
                "XDR array of {n} elements, at most {max} expected"
            )));
        }
        Ok(n)
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Everything not read yet (consumed).
    #[cfg(test)]
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }
}

/// A successful reply: the procedure's results start at `start`.
#[derive(Debug)]
pub struct Reply {
    buf: Vec<u8>,
    start: usize,
}

impl Reply {
    pub fn reader(&self) -> XdrReader<'_> {
        XdrReader::new(&self.buf[self.start..])
    }
}

/// One TCP connection to one RPC server; one call at a time.
pub struct Client {
    stream: TcpStream,
    xid: u32,
    stamp: u32,
    /// Set once a call fails part way: the stream may be mid-record, so nothing more is sent.
    broken: bool,
}

impl Client {
    pub fn connect(addr: SocketAddr) -> io::Result<Client> {
        Client::over(connect(addr)?, TIMEOUT)
    }

    /// A client over a connected `stream`, each read and write given `timeout`.
    pub fn over(stream: TcpStream, timeout: Duration) -> io::Result<Client> {
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        // A fresh xid space per connection, so a server's duplicate request cache never matches
        // a call of an earlier connection from the same (reused privileged) port.
        let xid = now.subsec_nanos() ^ (now.as_secs() as u32).rotate_left(16) ^ std::process::id();
        Ok(Client {
            stream,
            xid,
            stamp: now.as_secs() as u32,
            broken: false,
        })
    }

    /// Calls `proc` of `prog` version `vers` with the XDR-encoded `args`.
    pub fn call(&mut self, prog: u32, vers: u32, proc: u32, args: &[u8]) -> io::Result<Reply> {
        if self.broken {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the connection to the NFS server broke; connect again",
            ));
        }
        self.xid = self.xid.wrapping_add(1);
        let call = Call {
            xid: self.xid,
            prog,
            vers,
            proc,
            stamp: self.stamp,
        };
        exchange(&mut self.stream, &call, args).inspect_err(|e| {
            // An RPC refusal is a whole reply: the stream stays in step.
            if rpc_error(e).is_none() {
                self.broken = true;
            }
        })
    }

    /// The NULL procedure (0): a ping that tells whether `prog` version `vers` is served.
    pub fn null(&mut self, prog: u32, vers: u32) -> io::Result<()> {
        self.call(prog, vers, 0, &[]).map(drop)
    }
}

struct Call {
    xid: u32,
    prog: u32,
    vers: u32,
    proc: u32,
    stamp: u32,
}

/// The record mark and call header for a call whose arguments are `args_len` bytes.
fn encode_call(c: &Call, args_len: usize) -> io::Result<Vec<u8>> {
    let mut cred = Xdr::new();
    cred.u32(c.stamp).string(MACHINE).u32(0).u32(0).u32(0); // uid, gid, no gids
    let mut x = Xdr::new();
    x.u32(0) // the record mark, filled below
        .u32(c.xid)
        .u32(CALL)
        .u32(RPC_VERSION)
        .u32(c.prog)
        .u32(c.vers)
        .u32(c.proc)
        .u32(AUTH_SYS)
        .opaque(&cred.buf)
        .u32(AUTH_NONE)
        .u32(0);
    let len = x.buf.len() - 4 + args_len;
    if len >= LAST_FRAGMENT as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RPC call too large",
        ));
    }
    x.buf[..4].copy_from_slice(&(LAST_FRAGMENT | len as u32).to_be_bytes());
    Ok(x.buf)
}

/// Sends one call (a single fragment) and reads records until the reply to its xid.
fn exchange<S: Read + Write>(s: &mut S, c: &Call, args: &[u8]) -> io::Result<Reply> {
    s.write_all(&encode_call(c, args.len())?)?;
    s.write_all(args)?;
    s.flush()?;
    for _ in 0..=MAX_SKIPPED {
        let buf = read_record(s)?;
        if let Some(start) = parse_reply(&buf, c.xid)? {
            return Ok(Reply { buf, start });
        }
    }
    Err(bad("the NFS server sent too many replies to other calls"))
}

/// Reads one record: fragments joined until the one marked last; at most [`MAX_RECORD`] bytes.
/// At most [`MAX_EMPTY_FRAGMENTS`] empty fragments, so a record always ends.
// ponytail: the 30 s timeout is per socket read, not per call; a server that drips bytes
// keeps a call alive up to MAX_RECORD of them.
fn read_record<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut empty = 0;
    loop {
        let mut mark = [0u8; 4];
        r.read_exact(&mut mark)?;
        let mark = u32::from_be_bytes(mark);
        let len = (mark & !LAST_FRAGMENT) as usize;
        if len == 0 && mark & LAST_FRAGMENT == 0 {
            empty += 1;
            if empty > MAX_EMPTY_FRAGMENTS {
                return Err(bad("the NFS server sent a record of empty fragments"));
            }
        }
        if buf.len() + len > MAX_RECORD {
            return Err(bad(format!(
                "the NFS server's reply is larger than {MAX_RECORD} bytes"
            )));
        }
        // Grown as bytes arrive (a reply's usual size reserved): a length alone allocates
        // little, so a server that names 16 MiB and stalls costs no 16 MiB.
        let want = buf.len() + len;
        buf.reserve(len.min(2 << 20));
        r.take(len as u64).read_to_end(&mut buf)?;
        if buf.len() != want {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if mark & LAST_FRAGMENT != 0 {
            return Ok(buf);
        }
    }
}

/// Where the results start in a reply to `xid`; `None` for a record about another xid (or a
/// call from the server, which we never take); an RPC refusal as its [`RpcError`].
fn parse_reply(buf: &[u8], xid: u32) -> io::Result<Option<usize>> {
    let mut r = XdrReader::new(buf);
    if r.u32()? != xid || r.u32()? != REPLY {
        return Ok(None);
    }
    match r.u32()? {
        MSG_ACCEPTED => {
            r.u32()?; // verifier flavor
            r.opaque(400)?;
            let err = match r.u32()? {
                0 => return Ok(Some(buf.len() - r.remaining())),
                1 => RpcError::ProgUnavail,
                2 => RpcError::ProgMismatch {
                    low: r.u32()?,
                    high: r.u32()?,
                },
                3 => RpcError::ProcUnavail,
                4 => RpcError::GarbageArgs,
                5 => RpcError::SystemErr,
                s => return Err(bad(format!("bad RPC accept status {s}"))),
            };
            Err(err.into())
        }
        MSG_DENIED => {
            let err = match r.u32()? {
                0 => RpcError::RpcMismatch {
                    low: r.u32()?,
                    high: r.u32()?,
                },
                1 => RpcError::Auth(r.u32()?),
                s => return Err(bad(format!("bad RPC reject status {s}"))),
            };
            Err(err.into())
        }
        s => Err(bad(format!("bad RPC reply status {s}"))),
    }
}

/// Most privileged source ports tried before falling back to any port.
#[cfg(unix)]
const MAX_PORT_TRIES: usize = 64;

/// Connects from a privileged source port (1023 down to 600) when the system lets us bind one
/// (Linux exports are `secure` by default and refuse other ports), else from any port.
// ponytail: without root (or macOS's unprivileged low ports) a `secure` export refuses the
// fallback connection; Windows always takes the fallback.
pub fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
    #[cfg(unix)]
    for port in (600..=1023u16).rev().take(MAX_PORT_TRIES) {
        match privileged::connect_from(addr, port) {
            Ok(s) => return Ok(s),
            // Taken here, or the same 4-tuple still in TIME_WAIT: the next port.
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => continue,
            Err(e) if e.kind() == io::ErrorKind::AddrNotAvailable => continue,
            // Not allowed to bind a low port: any port.
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => break,
            Err(e) => return Err(e),
        }
    }
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
}

#[cfg(unix)]
mod privileged {
    use super::CONNECT_TIMEOUT;
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::net::{SocketAddr, TcpStream};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    /// A TCP connection to `addr` from local `port` (bind, then a non-blocking connect polled
    /// for at most [`CONNECT_TIMEOUT`]).
    pub fn connect_from(addr: SocketAddr, port: u16) -> io::Result<TcpStream> {
        let family = match addr {
            SocketAddr::V4(_) => libc::AF_INET,
            SocketAddr::V6(_) => libc::AF_INET6,
        };
        // SAFETY: plain socket(2); the descriptor is owned at once.
        let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh descriptor nothing else owns.
        let stream = TcpStream::from(unsafe { OwnedFd::from_raw_fd(fd) });
        let local = match addr {
            SocketAddr::V4(_) => SocketAddr::from(([0, 0, 0, 0], port)),
            SocketAddr::V6(_) => SocketAddr::from(([0u16; 8], port)),
        };
        let (sa, len) = sockaddr(&local);
        // SAFETY: `sa` is a valid sockaddr of `len` bytes.
        if unsafe { libc::bind(fd, (&raw const sa).cast(), len) } != 0 {
            return Err(io::Error::last_os_error());
        }
        stream.set_nonblocking(true)?;
        let (sa, len) = sockaddr(&addr);
        // SAFETY: as above.
        if unsafe { libc::connect(fd, (&raw const sa).cast(), len) } != 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(e);
            }
            let mut p = libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: one valid pollfd.
            let n = unsafe { libc::poll(&mut p, 1, CONNECT_TIMEOUT.as_millis() as libc::c_int) };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("connecting to {addr} timed out"),
                ));
            }
            if let Some(e) = stream.take_error()? {
                return Err(e);
            }
            stream.peer_addr()?; // not connected after all: the error says so
        }
        stream.set_nonblocking(false)?;
        Ok(stream)
    }

    fn sockaddr(addr: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
        // SAFETY: an all-zero sockaddr_storage is valid.
        let mut ss: libc::sockaddr_storage = unsafe { zeroed() };
        let len = match addr {
            SocketAddr::V4(a) => {
                // SAFETY: sockaddr_storage is large and aligned enough for sockaddr_in.
                let sin = unsafe { &mut *(&raw mut ss).cast::<libc::sockaddr_in>() };
                sin.sin_family = libc::AF_INET as libc::sa_family_t;
                sin.sin_port = a.port().to_be();
                sin.sin_addr.s_addr = u32::from_ne_bytes(a.ip().octets());
                size_of::<libc::sockaddr_in>()
            }
            SocketAddr::V6(a) => {
                // SAFETY: as above, for sockaddr_in6.
                let sin6 = unsafe { &mut *(&raw mut ss).cast::<libc::sockaddr_in6>() };
                sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
                sin6.sin6_port = a.port().to_be();
                sin6.sin6_addr.s6_addr = a.ip().octets();
                sin6.sin6_flowinfo = a.flowinfo();
                sin6.sin6_scope_id = a.scope_id();
                size_of::<libc::sockaddr_in6>()
            }
        };
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        {
            ss.ss_len = len as u8;
        }
        (ss, len as libc::socklen_t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn record(fragments: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, f) in fragments.iter().enumerate() {
            let last = if i + 1 == fragments.len() {
                LAST_FRAGMENT
            } else {
                0
            };
            out.extend_from_slice(&(last | f.len() as u32).to_be_bytes());
            out.extend_from_slice(f);
        }
        out
    }

    /// An accepted reply to `xid`: a verifier, then `stat` and `body`.
    fn accepted(xid: u32, verf: &[u8], stat: u32, body: &[u8]) -> Vec<u8> {
        let mut x = Xdr::new();
        x.u32(xid)
            .u32(REPLY)
            .u32(MSG_ACCEPTED)
            .u32(AUTH_NONE)
            .opaque(verf)
            .u32(stat);
        x.buf.extend_from_slice(body);
        x.buf
    }

    #[test]
    fn records_are_bounded() {
        // Two fragments each under the cap whose sum is over it.
        let half = (MAX_RECORD / 2 + 1) as u32;
        let mut bytes = half.to_be_bytes().to_vec();
        bytes.resize(4 + half as usize, 0);
        bytes.extend((LAST_FRAGMENT | half).to_be_bytes());
        let e = read_record(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        // A fragment cut short is an error, not a short record.
        let mut cut = record(&[b"abcdef"]);
        cut.truncate(7);
        assert!(read_record(&mut Cursor::new(cut)).is_err());
        // Empty fragments: a few are fine, an endless run is refused.
        let mut few = vec![&b""[..]; MAX_EMPTY_FRAGMENTS];
        few.push(b"x");
        assert_eq!(read_record(&mut Cursor::new(record(&few))).unwrap(), b"x");
        let endless = vec![0u8; 4 * (MAX_EMPTY_FRAGMENTS + 1)];
        let e = read_record(&mut Cursor::new(endless)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn call_header_carries_auth_sys() {
        let c = Call {
            xid: 0x11223344,
            prog: 100003,
            vers: 4,
            proc: 1,
            stamp: 7,
        };
        let h = encode_call(&c, 12).unwrap();
        let mut r = XdrReader::new(&h);
        assert_eq!(r.u32().unwrap(), LAST_FRAGMENT | (h.len() as u32 - 4 + 12));
        let head: Vec<u32> = (0..7).map(|_| r.u32().unwrap()).collect();
        assert_eq!(head, [0x11223344, CALL, 2, 100003, 4, 1, AUTH_SYS]);
        let cred = r.opaque(400).unwrap();
        // stamp, "forge" (5 bytes + 3 padding), uid 0, gid 0, no gids.
        assert_eq!(
            cred,
            [
                0, 0, 0, 7, 0, 0, 0, 5, b'f', b'o', b'r', b'g', b'e', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0
            ]
        );
        assert_eq!((r.u32().unwrap(), r.u32().unwrap()), (AUTH_NONE, 0));
        assert_eq!(r.remaining(), 0);
    }

    /// A stream whose reads come from `input` and whose writes go to `output`.
    struct Duplex {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for Duplex {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Duplex {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn exchange_matches_the_xid() {
        let mut input = record(&[&accepted(6, b"", 0, &[0, 0, 0, 1])]);
        let reply = accepted(7, b"", 0, &[0, 0, 0, 2, 0, 0, 0, 3]);
        input.extend(record(&[&reply[..10], &reply[10..]]));
        let mut s = Duplex {
            input: Cursor::new(input),
            output: Vec::new(),
        };
        let c = Call {
            xid: 7,
            prog: 100005,
            vers: 3,
            proc: 5,
            stamp: 0,
        };
        let got = exchange(&mut s, &c, &[0xAA; 8]).unwrap();
        let mut r = got.reader();
        assert_eq!((r.u32().unwrap(), r.u32().unwrap()), (2, 3));
        assert_eq!(r.remaining(), 0);
        // What went out is one record: header, then the arguments.
        let sent = read_record(&mut Cursor::new(&s.output)).unwrap();
        assert!(sent.ends_with(&[0xAA; 8]));
        assert_eq!(&sent[..4], &7u32.to_be_bytes());
    }

    #[test]
    fn exchange_gives_up_on_endless_other_replies() {
        let mut input = Vec::new();
        for _ in 0..=MAX_SKIPPED {
            input.extend(record(&[&accepted(1, b"", 0, &[])]));
        }
        input.extend(record(&[&accepted(2, b"", 0, &[])]));
        let mut s = Duplex {
            input: Cursor::new(input),
            output: Vec::new(),
        };
        let c = Call {
            xid: 2,
            prog: 1,
            vers: 1,
            proc: 0,
            stamp: 0,
        };
        assert_eq!(
            exchange(&mut s, &c, &[]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn xdr_round_trip() {
        let mut x = Xdr::new();
        x.u32(1)
            .u64(0x0102_0304_0506_0708)
            .bool(true)
            .opaque(b"abcde")
            .string("")
            .fixed(&[9, 9]);
        assert_eq!(x.buf.len(), 4 + 8 + 4 + (4 + 8) + 4 + 4);
        let mut r = XdrReader::new(&x.buf);
        assert_eq!(r.u32().unwrap(), 1);
        assert_eq!(r.u64().unwrap(), 0x0102_0304_0506_0708);
        assert!(r.bool().unwrap());
        assert_eq!(r.opaque(5).unwrap(), b"abcde");
        assert_eq!(r.opaque(0).unwrap(), b"");
        assert_eq!(r.fixed(2).unwrap(), [9, 9]);
        assert_eq!(r.remaining(), 0);
        assert!(r.u32().is_err());

        let mut x = Xdr::new();
        x.opaque(b"abcde").u32(2).u32(7);
        let mut r = XdrReader::new(&x.buf);
        assert!(r.opaque(4).is_err()); // longer than allowed
        let mut r = XdrReader::new(&x.buf[12..]);
        assert!(r.bool().is_err()); // 2 is no boolean
        assert!(XdrReader::new(&x.buf[12..]).len(1).is_err());
        assert_eq!(XdrReader::new(&x.buf[16..]).rest(), [0, 0, 0, 7]);
        // Padding missing at the end.
        assert!(XdrReader::new(&x.buf[..9]).opaque(8).is_err());
    }

    #[test]
    fn loopback_call() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, peer) = listener.accept().unwrap();
            for _ in 0..2 {
                let call = read_record(&mut s).unwrap();
                let xid = XdrReader::new(&call).u32().unwrap();
                let reply = if call[16..20] == 3u32.to_be_bytes() {
                    accepted(xid, b"", 0, &[])
                } else {
                    let mut v = Xdr::new();
                    v.u32(3).u32(3);
                    accepted(xid, b"", 2, &v.buf)
                };
                s.write_all(&record(&[&reply])).unwrap();
            }
            peer.port()
        });
        let mut c = Client::connect(addr).unwrap();
        c.null(100003, 3).unwrap();
        let e = c.null(100003, 4).unwrap_err();
        assert!(is_version_mismatch(&e));
        assert!(!c.broken, "a refusal keeps the connection in step");
        let port = server.join().unwrap();
        eprintln!("connected from port {port}");
    }
}
