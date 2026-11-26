//! FLINK v2, Connect's Forge-to-Forge protocol over TCP. A frame is a 16-byte header (`"FL"`,
//! type, flags, channel u32 LE, body length u32 LE, CRC-32 of bytes 0..12) and its body; after the
//! handshake a 16-byte MAC follows: BLAKE3 keyed per direction over (u64 LE counter, header, body).
//! Authenticated, not encrypted. Any error closes the connection (the stream may be mid-frame).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const MAGIC: [u8; 2] = *b"FL";
pub const HEADER_LEN: usize = 16;
pub const TAG_LEN: usize = 16;
/// A data lane's largest body (an 8 MiB chunk plus its JSON).
pub const MAX_BODY: u32 = 16 << 20;
pub const MAX_CONTROL_BODY: u32 = 64 << 10;

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("flink: {msg}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: u8,
    pub flags: u8,
    pub channel: u32,
    pub len: u32,
}

impl Header {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0..2].copy_from_slice(&MAGIC);
        b[2] = self.kind;
        b[3] = self.flags;
        b[4..8].copy_from_slice(&self.channel.to_le_bytes());
        b[8..12].copy_from_slice(&self.len.to_le_bytes());
        let crc = crc32fast::hash(&b[0..12]);
        b[12..16].copy_from_slice(&crc.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8; HEADER_LEN], max_body: u32) -> io::Result<Header> {
        if b[0..2] != MAGIC {
            return Err(bad("bad magic"));
        }
        let crc = u32::from_le_bytes(b[12..16].try_into().unwrap());
        if crc != crc32fast::hash(&b[0..12]) {
            return Err(bad("bad header CRC"));
        }
        let len = u32::from_le_bytes(b[8..12].try_into().unwrap());
        if len > max_body {
            return Err(bad("frame body too large"));
        }
        Ok(Header {
            kind: b[2],
            flags: b[3],
            channel: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            len,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub flags: u8,
    pub channel: u32,
    pub body: Vec<u8>,
}

impl Frame {
    pub fn new(kind: u8, channel: u32, body: Vec<u8>) -> Frame {
        Frame {
            kind,
            flags: 0,
            channel,
            body,
        }
    }

    fn header(&self) -> io::Result<Header> {
        let len = u32::try_from(self.body.len()).map_err(|_| bad("frame body too large"))?;
        Ok(Header {
            kind: self.kind,
            flags: self.flags,
            channel: self.channel,
            len,
        })
    }
}

/// One direction's MAC state: its key and the counter of the next frame.
pub struct Mac {
    key: [u8; 32],
    counter: u64,
}

impl Mac {
    pub fn new(key: [u8; 32]) -> Mac {
        Mac { key, counter: 0 }
    }

    fn tag(&self, header: &[u8; HEADER_LEN], body: &[u8]) -> [u8; TAG_LEN] {
        self.tag_parts(header, &[body])
    }

    fn tag_parts(&self, header: &[u8; HEADER_LEN], parts: &[&[u8]]) -> [u8; TAG_LEN] {
        let mut h = blake3::Hasher::new_keyed(&self.key);
        h.update(&self.counter.to_le_bytes());
        h.update(header);
        for part in parts {
            h.update(part);
        }
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&h.finalize().as_bytes()[..TAG_LEN]);
        tag
    }

    fn advance(&mut self) -> io::Result<()> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| bad("frame counter exhausted"))?;
        Ok(())
    }

    pub fn seal(&mut self, header: &[u8; HEADER_LEN], body: &[u8]) -> io::Result<[u8; TAG_LEN]> {
        self.seal_parts(header, &[body])
    }

    fn seal_parts(
        &mut self,
        header: &[u8; HEADER_LEN],
        parts: &[&[u8]],
    ) -> io::Result<[u8; TAG_LEN]> {
        let tag = self.tag_parts(header, parts);
        self.advance()?;
        Ok(tag)
    }

    /// The counter moves on only on a match, so a replayed or reordered frame never does.
    pub fn open(
        &mut self,
        header: &[u8; HEADER_LEN],
        body: &[u8],
        tag: &[u8; TAG_LEN],
    ) -> io::Result<()> {
        if !ct_eq(&self.tag(header, body), tag) {
            return Err(bad("bad frame MAC"));
        }
        self.advance()
    }
}

fn ct_eq(a: &[u8; TAG_LEN], b: &[u8; TAG_LEN]) -> bool {
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// A small frame in one `write_all`, a large one (a data chunk) never copied.
pub fn write_frame<W: Write>(w: &mut W, frame: &Frame, mac: Option<&mut Mac>) -> io::Result<()> {
    write_parts(w, frame.header()?, &[&frame.body], mac)
}

fn write_parts<W: Write>(
    w: &mut W,
    header: Header,
    parts: &[&[u8]],
    mac: Option<&mut Mac>,
) -> io::Result<()> {
    const COPY_MAX: usize = 64 << 10;
    let header = header.encode();
    let tag = mac.map(|mac| mac.seal_parts(&header, parts)).transpose()?;
    let len: usize = parts.iter().map(|p| p.len()).sum();
    if len > COPY_MAX {
        w.write_all(&header)?;
        for part in parts {
            w.write_all(part)?;
        }
        if let Some(tag) = tag {
            w.write_all(&tag)?;
        }
        return w.flush();
    }
    let mut buf = Vec::with_capacity(HEADER_LEN + len + TAG_LEN);
    buf.extend_from_slice(&header);
    parts.iter().for_each(|p| buf.extend_from_slice(p));
    if let Some(tag) = tag {
        buf.extend_from_slice(&tag);
    }
    w.write_all(&buf)?;
    w.flush()
}

pub fn read_frame<R: Read>(r: &mut R, max_body: u32, mac: Option<&mut Mac>) -> io::Result<Frame> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    let h = Header::decode(&header, max_body)?;
    // Filled as it is read, not zeroed first (a data chunk is 8 MiB).
    let mut body = Vec::with_capacity(h.len as usize);
    r.by_ref().take(h.len.into()).read_to_end(&mut body)?;
    if body.len() != h.len as usize {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    if let Some(mac) = mac {
        let mut tag = [0u8; TAG_LEN];
        r.read_exact(&mut tag)?;
        mac.open(&header, &body, &tag)?;
    }
    Ok(Frame {
        kind: h.kind,
        flags: h.flags,
        channel: h.channel,
        body,
    })
}

/// On the PS5, the largest socket buffers its kernel takes (16 MiB down by a quarter each try:
/// FreeBSD refuses past `kern.ipc.maxsockbuf`); its defaults are 48/80 KiB. Measured: it clamps to
/// 512 KiB, and neither that, cubic, nor more connections moves its Wi-Fi send (~15 MB/s).
/// Elsewhere the OS autotunes (a set size turns that off on Linux).
pub fn tune_buffers(stream: &TcpStream) {
    #[cfg(target_os = "freebsd")]
    {
        use std::os::fd::AsRawFd;
        for opt in [libc::SO_SNDBUF, libc::SO_RCVBUF] {
            let mut want: libc::c_int = 16 << 20;
            while want >= 256 << 10 {
                // SAFETY: a valid socket and a c_int option value of its size.
                let r = unsafe {
                    libc::setsockopt(
                        stream.as_raw_fd(),
                        libc::SOL_SOCKET,
                        opt,
                        (&want as *const libc::c_int).cast(),
                        std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                    )
                };
                if r == 0 {
                    break;
                }
                want -= want / 4;
            }
        }
    }
    #[cfg(not(target_os = "freebsd"))]
    let _ = stream;
}

/// A FLINK connection: plain frames until `set_keys`, MAC'd ones after.
pub struct Conn {
    stream: TcpStream,
    max_body: u32,
    send_mac: Option<Mac>,
    recv_mac: Option<Mac>,
    timeout: Duration,
    deadline: Option<Instant>,
}

/// Reads bounded by the timeout and, while set, the deadline: a peer dripping bytes can't hold
/// the conn past it.
struct Bounded<'a> {
    stream: &'a TcpStream,
    timeout: Duration,
    deadline: Option<Instant>,
}

impl Read for Bounded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let late = || {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "flink: the handshake took too long",
            )
        };
        let Some(deadline) = self.deadline else {
            let mut stream = self.stream;
            return stream.read(buf);
        };
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(late());
        }
        self.stream.set_read_timeout(Some(left.min(self.timeout)))?;
        let mut stream = self.stream;
        stream.read(buf).map_err(|e| match e.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut if Instant::now() >= deadline => {
                late()
            }
            _ => e,
        })
    }
}

impl Conn {
    pub fn new(stream: TcpStream, max_body: u32, timeout: Duration) -> io::Result<Conn> {
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;
        tune_buffers(&stream);
        Ok(Conn {
            stream,
            max_body,
            send_mac: None,
            recv_mac: None,
            timeout,
            deadline: None,
        })
    }

    pub fn set_deadline(&mut self, deadline: Option<Instant>) -> io::Result<()> {
        self.deadline = deadline;
        if deadline.is_none() {
            self.stream.set_read_timeout(Some(self.timeout))?;
        }
        Ok(())
    }

    /// A peer that stopped reading ends a write in up to twice `timeout` (`write_all` retries a
    /// partial send).
    pub fn set_write_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.stream.set_write_timeout(Some(timeout))
    }

    fn reader(&self) -> Bounded<'_> {
        Bounded {
            stream: &self.stream,
            timeout: self.timeout,
            deadline: self.deadline,
        }
    }

    pub fn connect(addr: SocketAddr, max_body: u32, timeout: Duration) -> io::Result<Conn> {
        Conn::new(
            TcpStream::connect_timeout(&addr, timeout)?,
            max_body,
            timeout,
        )
    }

    pub fn set_keys(&mut self, send: [u8; 32], recv: [u8; 32]) {
        self.send_mac = Some(Mac::new(send));
        self.recv_mac = Some(Mac::new(recv));
    }

    pub fn set_max_body(&mut self, max_body: u32) {
        self.max_body = max_body;
    }

    pub fn send_frame(&mut self, frame: &Frame) -> io::Result<()> {
        write_frame(&mut self.stream, frame, self.send_mac.as_mut())
    }

    pub fn recv_frame(&mut self) -> io::Result<Frame> {
        let mut r = Bounded {
            stream: &self.stream,
            timeout: self.timeout,
            deadline: self.deadline,
        };
        read_frame(&mut r, self.max_body, self.recv_mac.as_mut())
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    /// Whether the peer closed its side, asked without blocking (for a wait in which it sends
    /// nothing).
    fn peer_left(&self) -> bool {
        if self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let left = match self.stream.peek(&mut [0u8; 1]) {
            Ok(n) => n == 0,
            Err(e) => e.kind() != io::ErrorKind::WouldBlock,
        };
        let _ = self.stream.set_nonblocking(false);
        left
    }
}

// ---- Handshake --------------------------------------------------------------------------------
//
// Plain `Hello` both ways, then the client's `Pair {commit}` (commit = H(its X25519 key)) or
// `Auth {}`. To a `Pair`: `PairWait {key}`, the client reveals `PairKey {key}` (checked against the
// commitment), both show one six-digit check number, the server pings while its user decides, then
// `PairOk {}` or `Error {code}`. The commitment leaves a man in the middle one 1-in-a-million guess.
// To an `Auth`: `AuthOk {}` or `Error {code}`. Then a MAC'd `Ready {}` each way proves the secret.
// ponytail: the check number helps only when someone compares both screens; a key id is claimed,
// not proved, so a known one is never paired again (forget it first).

pub mod kind {
    pub const HELLO: u8 = 1;
    pub const PAIR: u8 = 2;
    pub const PAIR_OK: u8 = 3;
    pub const AUTH: u8 = 4;
    pub const AUTH_OK: u8 = 5;
    pub const READY: u8 = 6;
    pub const ERROR: u8 = 7;
    // `kind2`'s are 8..=15.
    pub const PAIR_WAIT: u8 = 16;
    pub const PAIR_KEY: u8 = 17;
}

pub const VERSION: u32 = 2;
pub const PAIR_WAIT: Duration = if cfg!(test) {
    Duration::from_secs(3)
} else {
    Duration::from_secs(60)
};
/// The listening side's whole handshake (or a lane's Join), so a dripping peer can't hold a conn
/// slot; a pairing gets `PAIR_WAIT` more once its `Pair` is in.
pub const HANDSHAKE_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(1500)
} else {
    Duration::from_secs(15)
};
const PAIR_PING: Duration = Duration::from_secs(2);

pub const NOT_PAIRED: &str = "not_paired";
pub const DENIED: &str = "denied";
pub const TIMED_OUT: &str = "timed_out";
pub const TOO_MANY_ATTEMPTS: &str = "too_many_attempts";
pub const BAD_HELLO: &str = "bad_hello";
pub const BAD_KEY: &str = "bad_key";
/// Too many requests waiting (not counted against the asker).
pub const BUSY: &str = "busy";
pub const ALREADY_PAIRED: &str = "already_paired";

/// `port`: its FLINK listener (the peer stores `<tcp peer ip>:<port>`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Hello {
    pub v: u32,
    pub key_id: String,
    pub name: String,
    pub platform: String,
    pub port: u16,
    pub nonce: String,
}

#[derive(Clone, Debug)]
pub struct Me {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    pub port: u16,
}

/// An X25519 public key, hex.
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyBody {
    key: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CommitBody {
    commit: String,
}

fn commitment(public: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("forge flink v2 commit", public)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairRequest {
    /// Random, made by the server.
    pub id: String,
    pub key_id: String,
    pub name: String,
    pub platform: String,
    pub ip: std::net::IpAddr,
    pub check: String,
}

/// Asks this device's user about pairing requests.
pub trait Approver {
    /// False: the client is told `BUSY`, not counted against it.
    fn admit(&self, req: &PairRequest) -> bool;
    /// Up to `wait` for the answer; `None`: none yet (asked again after a ping).
    fn decide(&self, req: &PairRequest, wait: Duration) -> Option<bool>;
    /// Answered, timed out, or the peer left.
    fn done(&self, req: &PairRequest);
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ErrorBody {
    code: String,
}

/// Kept for deriving data-lane keys (`dir_keys`).
#[derive(Clone, Debug)]
pub struct Session {
    pub secret: [u8; 32],
    pub nonce_c: [u8; 16],
    pub nonce_s: [u8; 16],
}

/// A finished handshake (the conn keyed); `new_secret` set when it paired.
#[derive(Debug)]
pub struct Handshake {
    pub peer: Hello,
    pub remote: SocketAddr,
    pub new_secret: Option<[u8; 32]>,
    pub session: Session,
}

/// An `Error {code}` (or a local refusal) inside a `PermissionDenied` `io::Error`; see `refusal`.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "flink: refused: {}", self.0)
    }
}

impl std::error::Error for Refused {}

fn refused(code: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, Refused(code.to_string()))
}

pub fn refusal(e: &io::Error) -> Option<&str> {
    e.get_ref()?.downcast_ref::<Refused>().map(|r| r.0.as_str())
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != 2 * N || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

struct Transcript {
    nonce_c: [u8; 16],
    nonce_s: [u8; 16],
    key_c: [u8; 16],
    key_s: [u8; 16],
}

impl Transcript {
    fn bytes(&self) -> Vec<u8> {
        [self.nonce_c, self.nonce_s, self.key_c, self.key_s].concat()
    }

    fn pair_material(&self, shared: &[u8; 32], pub_c: &[u8; 32], pub_s: &[u8; 32]) -> Vec<u8> {
        [&shared[..], &self.bytes(), pub_c, pub_s].concat()
    }

    fn pair_secret(&self, shared: &[u8; 32], pub_c: &[u8; 32], pub_s: &[u8; 32]) -> [u8; 32] {
        blake3::derive_key(
            "forge flink v2 pair",
            &self.pair_material(shared, pub_c, pub_s),
        )
    }

    fn check(&self, shared: &[u8; 32], pub_c: &[u8; 32], pub_s: &[u8; 32]) -> String {
        let h = blake3::derive_key(
            "forge flink v2 check",
            &self.pair_material(shared, pub_c, pub_s),
        );
        let n = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) % 1_000_000;
        format!("{n:06}")
    }
}

fn x25519_pair() -> io::Result<(x25519_dalek::StaticSecret, [u8; 32])> {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
    let secret = x25519_dalek::StaticSecret::from(k);
    let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
    Ok((secret, public))
}

/// `None` for a low-order point.
fn shared_with(mine: &x25519_dalek::StaticSecret, theirs: [u8; 32]) -> Option<[u8; 32]> {
    let shared = mine.diffie_hellman(&x25519_dalek::PublicKey::from(theirs));
    shared.was_contributory().then(|| shared.to_bytes())
}

/// (c2s, s2c) for the session, or a data lane when `lane_nonce` is non-empty.
pub fn dir_keys(s: &Session, lane_nonce: &[u8]) -> ([u8; 32], [u8; 32]) {
    let key = |dir: &[u8]| {
        let m = [&s.secret[..], &s.nonce_c, &s.nonce_s, lane_nonce, dir].concat();
        blake3::derive_key("forge flink v1 session", &m)
    };
    (key(b"c2s"), key(b"s2c"))
}

/// Denied or unanswered pairing requests: 5 per source IP per 10 minutes, then refused unasked.
/// No cap across addresses: many could then lock everyone out.
#[derive(Default)]
pub struct Budget {
    fails: Vec<(std::time::Instant, std::net::IpAddr)>,
}

impl Budget {
    pub const WINDOW: Duration = Duration::from_secs(600);
    pub const PER_IP: usize = 5;

    fn prune(&mut self, now: std::time::Instant) {
        self.fails
            .retain(|(t, _)| now.saturating_duration_since(*t) < Self::WINDOW);
    }

    pub fn allows(&mut self, ip: std::net::IpAddr, now: std::time::Instant) -> bool {
        self.prune(now);
        self.fails.iter().filter(|(_, i)| *i == ip).count() < Self::PER_IP
    }

    pub fn fail(&mut self, ip: std::net::IpAddr, now: std::time::Instant) {
        self.prune(now);
        self.fails.push((now, ip));
    }

    #[cfg(test)]
    fn failures(&mut self, now: std::time::Instant) -> usize {
        self.prune(now);
        self.fails.len()
    }
}

fn send_json<T: serde::Serialize>(conn: &mut Conn, kind: u8, body: &T) -> io::Result<()> {
    let body = serde_json::to_vec(body).map_err(|e| bad(&format!("json: {e}")))?;
    conn.send_frame(&Frame::new(kind, 0, body))
}

fn send_error(conn: &mut Conn, code: &str) -> io::Error {
    let _ = send_json(
        conn,
        kind::ERROR,
        &ErrorBody {
            code: code.to_string(),
        },
    );
    refused(code)
}

fn parse<T: serde::de::DeserializeOwned>(f: &Frame) -> io::Result<T> {
    serde_json::from_slice(&f.body).map_err(|e| bad(&format!("json: {e}")))
}

fn expect(conn: &mut Conn, want: u8) -> io::Result<Frame> {
    let f = conn.recv_frame()?;
    if f.kind == kind::ERROR {
        let e: ErrorBody = parse(&f)?;
        return Err(refused(&e.code));
    }
    if f.kind != want {
        return Err(bad(&format!("unexpected frame {} (wanted {want})", f.kind)));
    }
    Ok(f)
}

fn check_hello(f: &Frame) -> Option<(Hello, [u8; 16], [u8; 16])> {
    let h: Hello = parse(f).ok()?;
    let key = unhex::<16>(&h.key_id)?;
    let nonce = unhex::<16>(&h.nonce)?;
    (h.v == VERSION).then_some((h, key, nonce))
}

fn hello(me: &Me, nonce: &[u8; 16]) -> Hello {
    Hello {
        v: VERSION,
        key_id: me.key_id.clone(),
        name: me.name.clone(),
        platform: me.platform.clone(),
        port: me.port,
        nonce: hex(nonce),
    }
}

fn my_key(me: &Me) -> io::Result<[u8; 16]> {
    unhex::<16>(&me.key_id).ok_or_else(|| bad("own key id is not 32 hex digits"))
}

fn new_nonce() -> io::Result<[u8; 16]> {
    let mut n = [0u8; 16];
    getrandom::fill(&mut n).map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
    Ok(n)
}

fn finish(conn: &mut Conn, session: &Session, client: bool) -> io::Result<()> {
    let (c2s, s2c) = dir_keys(session, &[]);
    if client {
        conn.set_keys(c2s, s2c);
    } else {
        conn.set_keys(s2c, c2s);
    }
    conn.send_frame(&Frame::new(kind::READY, 0, b"{}".to_vec()))?;
    expect(conn, kind::READY).map(|_| ())
}

/// The connecting side. With `pairing` (given the check number to show) it asks to pair; without,
/// it needs `peers`' secret for the peer, else `NOT_PAIRED` before sending anything secret.
pub fn client_handshake(
    conn: &mut Conn,
    me: &Me,
    peers: &dyn Fn(&str) -> Option<[u8; 32]>,
    pairing: Option<&mut dyn FnMut(&str)>,
) -> io::Result<Handshake> {
    let key_c = my_key(me)?;
    let nonce_c = new_nonce()?;
    send_json(conn, kind::HELLO, &hello(me, &nonce_c))?;
    let f = expect(conn, kind::HELLO)?;
    let (peer, key_s, nonce_s) = check_hello(&f).ok_or_else(|| bad("bad hello"))?;
    if key_s == key_c {
        return Err(bad("that is this device"));
    }
    let t = Transcript {
        nonce_c,
        nonce_s,
        key_c,
        key_s,
    };
    let (secret, new_secret) = match pairing {
        Some(on_check) => {
            let (mine, pub_c) = x25519_pair()?;
            let commit = hex(&commitment(&pub_c));
            send_json(conn, kind::PAIR, &CommitBody { commit })?;
            let wait: KeyBody = parse(&expect(conn, kind::PAIR_WAIT)?)?;
            let pub_s = unhex::<32>(&wait.key).ok_or_else(|| bad("the device sent a bad key"))?;
            let shared =
                shared_with(&mine, pub_s).ok_or_else(|| bad("the device sent a bad key"))?;
            send_json(conn, kind::PAIR_KEY, &KeyBody { key: hex(&pub_c) })?;
            on_check(&t.check(&shared, &pub_c, &pub_s));
            let deadline = std::time::Instant::now() + PAIR_WAIT + PAIR_PING * 5;
            loop {
                let f = conn.recv_frame()?;
                match f.kind {
                    kind::PAIR_OK => break,
                    kind::ERROR => return Err(refused(&parse::<ErrorBody>(&f)?.code)),
                    kind2::PING if std::time::Instant::now() < deadline => {}
                    kind2::PING => return Err(refused(TIMED_OUT)),
                    k => return Err(bad(&format!("unexpected frame {k} (wanted PairOk)"))),
                }
            }
            let s = t.pair_secret(&shared, &pub_c, &pub_s);
            (s, Some(s))
        }
        None => {
            let s = peers(&peer.key_id).ok_or_else(|| refused(NOT_PAIRED))?;
            send_json(conn, kind::AUTH, &serde_json::json!({}))?;
            expect(conn, kind::AUTH_OK)?;
            (s, None)
        }
    };
    let session = Session {
        secret,
        nonce_c,
        nonce_s,
    };
    finish(conn, &session, true)?;
    Ok(Handshake {
        peer,
        remote: conn.peer_addr()?,
        new_secret,
        session,
    })
}

/// Connect's probe: Hellos swapped, then the caller drops the conn (nothing paired or counted).
pub fn introduce(conn: &mut Conn, me: &Me) -> io::Result<Hello> {
    let key_c = my_key(me)?;
    send_json(conn, kind::HELLO, &hello(me, &new_nonce()?))?;
    let f = expect(conn, kind::HELLO)?;
    let (peer, key_s, _) = check_hello(&f).ok_or_else(|| bad("bad hello"))?;
    if key_s == key_c {
        return Err(bad("that is this device"));
    }
    Ok(peer)
}

/// The listening side: a `Pair` goes to `approver` within `budget`, an `Auth` needs `peers`'
/// secret. Every refusal is sent and returned as `refused(code)`; the caller closes the conn.
pub fn server_handshake(
    conn: &mut Conn,
    me: &Me,
    peers: &dyn Fn(&str) -> Option<[u8; 32]>,
    approver: &dyn Approver,
    budget: &std::sync::Mutex<Budget>,
) -> io::Result<Handshake> {
    conn.set_deadline(Some(Instant::now() + HANDSHAKE_WAIT))?;
    let hs = server_steps(conn, me, peers, approver, budget);
    hs.and_then(|hs| conn.set_deadline(None).map(|()| hs))
}

fn server_steps(
    conn: &mut Conn,
    me: &Me,
    peers: &dyn Fn(&str) -> Option<[u8; 32]>,
    approver: &dyn Approver,
    budget: &std::sync::Mutex<Budget>,
) -> io::Result<Handshake> {
    let key_s = my_key(me)?;
    let f = conn.recv_frame()?;
    let Some((peer, key_c, nonce_c)) = (f.kind == kind::HELLO).then(|| check_hello(&f)).flatten()
    else {
        return Err(send_error(conn, BAD_HELLO));
    };
    let nonce_s = new_nonce()?;
    send_json(conn, kind::HELLO, &hello(me, &nonce_s))?;
    let t = Transcript {
        nonce_c,
        nonce_s,
        key_c,
        key_s,
    };
    let f = conn.recv_frame()?;
    let (secret, new_secret) = match f.kind {
        kind::PAIR => {
            conn.set_deadline(Some(Instant::now() + PAIR_WAIT + HANDSHAKE_WAIT))?;
            let ip = conn.peer_addr()?.ip();
            // A key id is claimed, not proved: a known one never replaces its pairing.
            if peers(&peer.key_id).is_some() {
                return Err(send_error(conn, ALREADY_PAIRED));
            }
            let lock = || budget.lock().unwrap_or_else(|e| e.into_inner());
            if !lock().allows(ip, std::time::Instant::now()) {
                return Err(send_error(conn, TOO_MANY_ATTEMPTS));
            }
            let body: Option<CommitBody> = parse(&f).ok();
            let Some(commit) = body.and_then(|b| unhex::<32>(&b.commit)) else {
                return Err(send_error(conn, BAD_KEY));
            };
            let (mine, pub_s) = x25519_pair()?;
            send_json(conn, kind::PAIR_WAIT, &KeyBody { key: hex(&pub_s) })?;
            let reveal: Option<KeyBody> = parse(&expect(conn, kind::PAIR_KEY)?).ok();
            let pub_c = reveal.and_then(|b| unhex::<32>(&b.key));
            let Some(pub_c) = pub_c.filter(|k| commitment(k) == commit) else {
                return Err(send_error(conn, BAD_KEY));
            };
            let Some(shared) = shared_with(&mine, pub_c) else {
                return Err(send_error(conn, BAD_KEY));
            };
            let req = PairRequest {
                id: hex(&new_nonce()?),
                key_id: peer.key_id.clone(),
                name: peer.name.clone(),
                platform: peer.platform.clone(),
                ip,
                check: t.check(&shared, &pub_c, &pub_s),
            };
            if !approver.admit(&req) {
                return Err(send_error(conn, BUSY));
            }
            let answer = ask(conn, approver, &req);
            approver.done(&req);
            // A requester that left counts too, else asking and leaving would prompt without end.
            if !matches!(answer, Ok(true)) {
                lock().fail(ip, std::time::Instant::now());
            }
            match answer {
                Ok(true) => {}
                Ok(false) => return Err(send_error(conn, DENIED)),
                Err(e) if refusal(&e) == Some(TIMED_OUT) => {
                    return Err(send_error(conn, TIMED_OUT));
                }
                Err(e) => return Err(e),
            }
            send_json(conn, kind::PAIR_OK, &serde_json::json!({}))?;
            let s = t.pair_secret(&shared, &pub_c, &pub_s);
            (s, Some(s))
        }
        kind::AUTH => match peers(&peer.key_id) {
            Some(s) => {
                send_json(conn, kind::AUTH_OK, &serde_json::json!({}))?;
                (s, None)
            }
            None => return Err(send_error(conn, NOT_PAIRED)),
        },
        _ => return Err(send_error(conn, NOT_PAIRED)),
    };
    let session = Session {
        secret,
        nonce_c,
        nonce_s,
    };
    finish(conn, &session, false)?;
    Ok(Handshake {
        peer,
        remote: conn.peer_addr()?,
        new_secret,
        session,
    })
}

/// The user's answer, asked in `PAIR_PING` slices with a ping between; `refused(TIMED_OUT)` after
/// `PAIR_WAIT`. A client that closed is seen by `peer_left` (a ping fails only slices later).
fn ask(conn: &mut Conn, approver: &dyn Approver, req: &PairRequest) -> io::Result<bool> {
    let start = std::time::Instant::now();
    loop {
        let left = PAIR_WAIT.saturating_sub(start.elapsed());
        if left.is_zero() {
            return Err(refused(TIMED_OUT));
        }
        if let Some(answer) = approver.decide(req, left.min(PAIR_PING)) {
            return Ok(answer);
        }
        if conn.peer_left() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "the requester left",
            ));
        }
        conn.send_frame(&Frame::new(kind2::PING, 0, Vec::new()))?;
    }
}

// ---- Control: RPC and liveness ----------------------------------------------------------------
//
// `Request {id, method, body}` / `Response {id, ok|err}` (JSON) plus empty `Ping`/`Pong`, split
// into one reader (`recv_control`, which answers pings) and a shared `Sender`. With `timeout =
// DEAD_AFTER`, a peer silent that long fails the reader's next read: the dead signal.

pub mod kind2 {
    pub const PING: u8 = 8;
    pub const PONG: u8 = 9;
    pub const REQUEST: u8 = 10;
    pub const RESPONSE: u8 = 11;
    pub const JOIN: u8 = 12;
    pub const JOIN_OK: u8 = 13;
    pub const DATA: u8 = 14;
    pub const DATA_OK: u8 = 15;
}

pub const PING_EVERY: Duration = Duration::from_secs(2);
pub const DEAD_AFTER: Duration = Duration::from_secs(12);
pub const BAD_JOIN: &str = "bad_join";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub body: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub ok: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

impl Response {
    pub fn new(id: u64, result: Result<serde_json::Value, String>) -> Response {
        match result {
            Ok(ok) => Response { id, ok, err: None },
            Err(e) => Response {
                id,
                ok: serde_json::Value::Null,
                err: Some(e),
            },
        }
    }

    pub fn result(self) -> Result<serde_json::Value, String> {
        match self.err {
            Some(e) => Err(e),
            None => Ok(self.ok),
        }
    }
}

fn json_frame<T: serde::Serialize>(kind: u8, body: &T) -> io::Result<Frame> {
    let body = serde_json::to_vec(body).map_err(|e| bad(&format!("json: {e}")))?;
    Ok(Frame::new(kind, 0, body))
}

impl Request {
    pub fn frame(&self) -> io::Result<Frame> {
        json_frame(kind2::REQUEST, self)
    }
}

impl Response {
    pub fn frame(&self) -> io::Result<Frame> {
        json_frame(kind2::RESPONSE, self)
    }
}

#[derive(Debug, PartialEq)]
pub enum Control {
    Request(Request),
    Response(Response),
}

pub struct Sender {
    stream: TcpStream,
    mac: Option<Mac>,
}

impl Sender {
    pub fn send_frame(&mut self, frame: &Frame) -> io::Result<()> {
        write_frame(&mut self.stream, frame, self.mac.as_mut())
    }

    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

pub struct Receiver {
    stream: TcpStream,
    max_body: u32,
    mac: Option<Mac>,
}

impl Receiver {
    pub fn recv_frame(&mut self) -> io::Result<Frame> {
        read_frame(&mut self.stream, self.max_body, self.mac.as_mut())
    }

    /// A `Ping` is answered through `tx`; a read timeout means the peer is dead.
    pub fn recv_control(&mut self, tx: &std::sync::Mutex<Sender>) -> io::Result<Control> {
        loop {
            let f = self.recv_frame().map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                    io::Error::new(io::ErrorKind::TimedOut, "flink: the peer went silent")
                }
                _ => e,
            })?;
            match f.kind {
                kind2::PING => lock(tx).send_frame(&Frame::new(kind2::PONG, 0, Vec::new()))?,
                kind2::PONG => {}
                kind2::REQUEST => return parse(&f).map(Control::Request),
                kind2::RESPONSE => return parse(&f).map(Control::Response),
                kind::ERROR => return Err(refused(&parse::<ErrorBody>(&f)?.code)),
                k => return Err(bad(&format!("unexpected control frame {k}"))),
            }
        }
    }
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Conn {
    pub fn split(self) -> io::Result<(Sender, Receiver)> {
        let tx = Sender {
            stream: self.stream.try_clone()?,
            mac: self.send_mac,
        };
        let rx = Receiver {
            stream: self.stream,
            max_body: self.max_body,
            mac: self.recv_mac,
        };
        Ok((tx, rx))
    }
}

// ---- Data lanes -------------------------------------------------------------------------------
//
// Another TCP conn to the FLINK port: `Join {session, lane, nonce}` MAC'd under the session's c2s
// key at counter 0, answered by a `JoinOk {}` under `dir_keys(session, nonce)` or `Error
// {bad_join}`. A replayed Join would get the same lane keys, so the caller takes each nonce once.

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Join {
    pub session: String,
    pub lane: u32,
    pub nonce: String,
}

impl Session {
    /// The public id `Join` names.
    pub fn id(&self) -> String {
        let m = [self.nonce_c, self.nonce_s].concat();
        hex(&blake3::derive_key("forge flink v1 session id", &m)[..16])
    }
}

pub fn client_join(conn: &mut Conn, session: &Session, lane: u32) -> io::Result<()> {
    let nonce = new_nonce()?;
    let join = json_frame(
        kind2::JOIN,
        &Join {
            session: session.id(),
            lane,
            nonce: hex(&nonce),
        },
    )?;
    let (c2s, _) = dir_keys(session, &[]);
    write_frame(&mut conn.stream, &join, Some(&mut Mac::new(c2s)))?;
    // A refusal comes plain, a `JoinOk` with a lane MAC.
    let (header, h, body) = read_head(&mut conn.stream)?;
    if h.kind == kind::ERROR {
        let e: ErrorBody = serde_json::from_slice(&body).map_err(|e| bad(&format!("json: {e}")))?;
        return Err(refused(&e.code));
    }
    if h.kind != kind2::JOIN_OK {
        return Err(bad(&format!("unexpected frame {} (wanted JoinOk)", h.kind)));
    }
    let mut tag = [0u8; TAG_LEN];
    conn.stream.read_exact(&mut tag)?;
    let (c2s, s2c) = dir_keys(session, &nonce);
    conn.set_keys(c2s, s2c);
    conn.recv_mac
        .as_mut()
        .map_or(Ok(()), |m| m.open(&header, &body, &tag))
}

/// A frame without its tag, bounded by `MAX_CONTROL_BODY`.
fn read_head(r: &mut impl Read) -> io::Result<([u8; HEADER_LEN], Header, Vec<u8>)> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    let h = Header::decode(&header, MAX_CONTROL_BODY)?;
    let mut body = vec![0u8; h.len as usize];
    r.read_exact(&mut body)?;
    Ok((header, h, body))
}

/// The Join's session, lane and nonce (the caller refuses a nonce taken before: a replay), the
/// conn keyed; anything wrong is `refused(BAD_JOIN)`.
pub fn server_join(
    conn: &mut Conn,
    sessions: &dyn Fn(&str) -> Option<Session>,
) -> io::Result<(Session, u32, [u8; 16])> {
    conn.set_deadline(Some(Instant::now() + HANDSHAKE_WAIT))?;
    let (header, h, body) = read_head(&mut conn.reader())?;
    let mut tag = [0u8; TAG_LEN];
    conn.reader().read_exact(&mut tag)?;
    conn.set_deadline(None)?;
    let join = (h.kind == kind2::JOIN)
        .then(|| serde_json::from_slice::<Join>(&body).ok())
        .flatten();
    let checked = join.and_then(|j| {
        let session = sessions(&j.session)?;
        let nonce = unhex::<16>(&j.nonce)?;
        let (c2s, _) = dir_keys(&session, &[]);
        Mac::new(c2s).open(&header, &body, &tag).ok()?;
        Some((session, j.lane, nonce))
    });
    let Some((session, lane, nonce)) = checked else {
        return Err(send_error(conn, BAD_JOIN));
    };
    let (c2s, s2c) = dir_keys(&session, &nonce);
    conn.set_keys(s2c, c2s);
    conn.send_frame(&Frame::new(kind2::JOIN_OK, 0, b"{}".to_vec()))?;
    Ok((session, lane, nonce))
}

// ---- Data -------------------------------------------------------------------------------------
//
// `u32 LE head length ‖ DataHead JSON ‖ bytes` (`hash` their BLAKE3), answered by `DataOk` or
// `Error {"transfer_failed: <why>"}` and the lane closed.

pub const CHUNK: u64 = 8 << 20;
pub const TRANSFER_FAILED: &str = "transfer_failed";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataHead {
    pub job: u64,
    pub file: u32,
    pub offset: u64,
    pub hash: String,
    /// A read by path: the stat after the read, so one round trip checks it is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<serde_json::Value>,
}

/// `complete`: this chunk ended the job.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataOk {
    pub job: u64,
    pub file: u32,
    pub offset: u64,
    pub complete: bool,
}

impl DataOk {
    pub fn frame(&self) -> io::Result<Frame> {
        json_frame(kind2::DATA_OK, self)
    }
}

pub fn chunk_hash(bytes: &[u8]) -> String {
    hex(blake3::hash(bytes).as_bytes())
}

fn data_prefix(head: &DataHead, extra: usize) -> io::Result<Vec<u8>> {
    let json = serde_json::to_vec(head).map_err(|e| bad(&format!("json: {e}")))?;
    let mut body = Vec::with_capacity(4 + json.len() + extra);
    body.extend_from_slice(&(json.len() as u32).to_le_bytes());
    body.extend_from_slice(&json);
    Ok(body)
}

pub fn data_frame(head: &DataHead, bytes: &[u8]) -> io::Result<Frame> {
    let mut body = data_prefix(head, bytes.len())?;
    body.extend_from_slice(bytes);
    Ok(Frame::new(kind2::DATA, 0, body))
}

impl Conn {
    /// `data_frame` without copying `bytes`.
    pub fn send_data(&mut self, head: &DataHead, bytes: &[u8]) -> io::Result<()> {
        let prefix = data_prefix(head, 0)?;
        let len =
            u32::try_from(prefix.len() + bytes.len()).map_err(|_| bad("frame body too large"))?;
        let header = Header {
            kind: kind2::DATA,
            flags: 0,
            channel: 0,
            len,
        };
        let mac = self.send_mac.as_mut();
        write_parts(&mut self.stream, header, &[&prefix, bytes], mac)
    }
}

pub fn parse_data(f: &Frame) -> io::Result<(DataHead, &[u8])> {
    let n = f.body.get(..4).ok_or_else(|| bad("short data frame"))?;
    let n = u32::from_le_bytes(n.try_into().unwrap_or_default()) as usize;
    let json = f
        .body
        .get(4..4 + n)
        .ok_or_else(|| bad("short data frame"))?;
    let head = serde_json::from_slice(json).map_err(|e| bad(&format!("json: {e}")))?;
    Ok((head, &f.body[4 + n..]))
}

pub fn send_refusal(conn: &mut Conn, code: &str) -> io::Error {
    send_error(conn, code)
}

/// A control conn's frames after the handshake.
pub const MAX_RPC_BODY: u32 = MAX_BODY;
pub const MAX_PATH: usize = 1024;

/// A relative `/`-separated path: at most `MAX_PATH` bytes, no `\` or NUL, no empty, `.` or `..`
/// part.
pub fn check_path(p: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("bad path {p:?}: {why}"));
    if p.is_empty() {
        return bad("empty");
    }
    if p.len() > MAX_PATH {
        return bad("too long");
    }
    if p.starts_with('/') {
        return bad("absolute");
    }
    if p.contains('\\') || p.contains('\0') {
        return bad("a backslash or NUL in it");
    }
    if p.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return bad("an empty, . or .. part");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;

    fn frame() -> Frame {
        Frame {
            kind: 7,
            flags: 2,
            channel: 0x0102_0304,
            body: b"{\"v\":1}".to_vec(),
        }
    }

    #[test]
    fn frame_round_trip_plain_and_maced() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame(), None).unwrap();
        assert_eq!(&buf[..2], b"FL");
        assert_eq!(
            read_frame(&mut Cursor::new(&buf), MAX_CONTROL_BODY, None).unwrap(),
            frame()
        );

        let (mut tx, mut rx) = (Mac::new([9; 32]), Mac::new([9; 32]));
        let mut buf = Vec::new();
        for _ in 0..3 {
            write_frame(&mut buf, &frame(), Some(&mut tx)).unwrap();
        }
        let mut r = Cursor::new(&buf);
        for _ in 0..3 {
            assert_eq!(
                read_frame(&mut r, MAX_CONTROL_BODY, Some(&mut rx)).unwrap(),
                frame()
            );
        }
    }

    #[test]
    fn bad_crc_magic_and_length_rejected() {
        let mut h = Header {
            kind: 1,
            flags: 0,
            channel: 0,
            len: 10,
        }
        .encode();
        h[5] ^= 1;
        assert!(Header::decode(&h, MAX_BODY).is_err());
        let mut h = Header {
            kind: 1,
            flags: 0,
            channel: 0,
            len: 10,
        }
        .encode();
        h[0] = b'X';
        assert!(Header::decode(&h, MAX_BODY).is_err());
        let h = Header {
            kind: 1,
            flags: 0,
            channel: 0,
            len: MAX_CONTROL_BODY + 1,
        }
        .encode();
        assert!(Header::decode(&h, MAX_CONTROL_BODY).is_err());
        assert!(Header::decode(&h, MAX_BODY).is_ok());
    }

    #[test]
    fn bad_mac_rejected() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame(), Some(&mut Mac::new([1; 32]))).unwrap();
        let wrong_key = read_frame(
            &mut Cursor::new(&buf),
            MAX_BODY,
            Some(&mut Mac::new([2; 32])),
        );
        assert!(wrong_key.is_err());
        let last = buf.len() - 1;
        buf[last] ^= 0x80;
        let flipped = read_frame(
            &mut Cursor::new(&buf),
            MAX_BODY,
            Some(&mut Mac::new([1; 32])),
        );
        assert!(flipped.is_err());
    }

    #[test]
    fn replayed_frame_rejected() {
        let mut tx = Mac::new([5; 32]);
        let mut first = Vec::new();
        write_frame(&mut first, &frame(), Some(&mut tx)).unwrap();
        let mut rx = Mac::new([5; 32]);
        read_frame(&mut Cursor::new(&first), MAX_BODY, Some(&mut rx)).unwrap();
        // The same bytes again: the receiver now expects counter 1.
        assert!(read_frame(&mut Cursor::new(&first), MAX_BODY, Some(&mut rx)).is_err());
    }

    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    type Peers = Arc<Mutex<HashMap<String, [u8; 32]>>>;

    fn me(id: u8, name: &str) -> Me {
        Me {
            key_id: hex(&[id; 16]),
            name: name.into(),
            platform: "test".into(),
            port: 9883,
        }
    }

    /// The server's user: `Some` answers at once, `None` never (the request times out). Keeps
    /// every request it was asked about.
    struct Answer(Option<bool>, Mutex<Vec<PairRequest>>);

    impl Approver for Answer {
        fn admit(&self, _: &PairRequest) -> bool {
            true
        }
        fn decide(&self, req: &PairRequest, wait: Duration) -> Option<bool> {
            self.1.lock().unwrap().push(req.clone());
            if self.0.is_none() {
                std::thread::sleep(wait);
            }
            self.0
        }
        fn done(&self, _: &PairRequest) {}
    }

    /// One handshake over loopback: (client result, server result, the check number the client
    /// showed, the requests the server's user saw). `pair`: the client asks to pair, the
    /// server's user answering so.
    fn run(
        pair: Option<Option<bool>>,
        cpeers: &Peers,
        speers: &Peers,
        budget: &Arc<Mutex<Budget>>,
    ) -> (
        io::Result<Handshake>,
        io::Result<Handshake>,
        Option<String>,
        Vec<PairRequest>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let t = Duration::from_secs(5);
        let (speers, budget) = (speers.clone(), budget.clone());
        let answer = pair.flatten();
        let server = std::thread::spawn(move || {
            let mut c = Conn::new(listener.accept().unwrap().0, MAX_CONTROL_BODY, t).unwrap();
            let lookup = |k: &str| speers.lock().unwrap().get(k).copied();
            let user = Answer(answer, Mutex::new(Vec::new()));
            let r = server_handshake(&mut c, &me(2, "ps5"), &lookup, &user, &budget);
            if let Ok(h) = &r {
                if let Some(s) = h.new_secret {
                    speers.lock().unwrap().insert(h.peer.key_id.clone(), s);
                }
                // One MAC'd frame each way after the handshake.
                let f = c.recv_frame().unwrap();
                c.send_frame(&f).unwrap();
            }
            (r, user.1.into_inner().unwrap())
        });
        let mut c = Conn::connect(addr, MAX_CONTROL_BODY, t).unwrap();
        let lookup = |k: &str| cpeers.lock().unwrap().get(k).copied();
        let mut shown = None;
        let mut on_check = |n: &str| shown = Some(n.to_string());
        let pairing = pair.map(|_| &mut on_check as &mut dyn FnMut(&str));
        let r = client_handshake(&mut c, &me(1, "mac"), &lookup, pairing);
        if let Ok(h) = &r {
            if let Some(s) = h.new_secret {
                cpeers.lock().unwrap().insert(h.peer.key_id.clone(), s);
            }
            c.send_frame(&Frame::new(9, 1, b"rpc".to_vec())).unwrap();
            assert_eq!(c.recv_frame().unwrap().body, b"rpc");
        }
        drop(c);
        let (s, asked) = server.join().unwrap();
        (r, s, shown, asked)
    }

    #[test]
    fn a_dripping_handshake_is_cut_at_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let t = Duration::from_secs(5);
            let mut c = Conn::new(listener.accept().unwrap().0, MAX_CONTROL_BODY, t).unwrap();
            let user = Answer(Some(true), Mutex::new(Vec::new()));
            let budget = Mutex::new(Budget::default());
            let start = std::time::Instant::now();
            let r = server_handshake(&mut c, &me(2, "ps5"), &|_: &str| None, &user, &budget);
            (r.map(|_| ()), start.elapsed())
        });
        let mut client = TcpStream::connect(addr).unwrap();
        // A Hello one byte every 200 ms: each read well within the 5 s timeout, never done.
        let header = Header {
            kind: kind::HELLO,
            flags: 0,
            channel: 0,
            len: 100,
        }
        .encode();
        for b in header.iter().cycle().take(40) {
            if client.write_all(&[*b]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let (r, took) = server.join().unwrap();
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(took < HANDSHAKE_WAIT + Duration::from_secs(1), "{took:?}");
    }

    #[test]
    fn pair_then_known_peers() {
        let (cp, sp) = (Peers::default(), Peers::default());
        let budget = Arc::new(Mutex::new(Budget::default()));
        // Unknown and not asking to pair: refused before anything secret is sent.
        let (c, s, _, asked) = run(None, &cp, &sp, &budget);
        assert_eq!(refusal(&c.unwrap_err()), Some(NOT_PAIRED));
        assert!(s.is_err() && asked.is_empty());

        // Approved: both hold the same secret, and both screens showed the same check number.
        let (c, s, shown, asked) = run(Some(Some(true)), &cp, &sp, &budget);
        let (c, s) = (c.unwrap(), s.unwrap());
        assert_eq!(c.peer.name, "ps5");
        assert_eq!(s.peer.name, "mac");
        assert_eq!(s.peer.port, 9883);
        assert_eq!(c.new_secret, s.new_secret);
        assert!(c.new_secret.is_some());
        let check = shown.unwrap();
        assert_eq!(check.len(), 6);
        assert!(check.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(asked[0].check, check);
        assert_eq!(asked[0].name, "mac");
        assert_eq!(asked[0].key_id, hex(&[1; 16]));

        // Paired already: asking again is refused unasked (a claimed key id never replaces one).
        let (c, s, shown, asked) = run(Some(Some(true)), &cp, &sp, &budget);
        assert_eq!(refusal(&c.unwrap_err()), Some(ALREADY_PAIRED));
        assert_eq!(refusal(&s.unwrap_err()), Some(ALREADY_PAIRED));
        assert!(shown.is_none() && asked.is_empty());

        let (c, s, _, asked) = run(None, &cp, &sp, &budget);
        let (c, s) = (c.unwrap(), s.unwrap());
        assert!(c.new_secret.is_none() && s.new_secret.is_none() && asked.is_empty());
        assert_eq!(c.session.secret, s.session.secret);
        assert_eq!(dir_keys(&c.session, b"lane"), dir_keys(&s.session, b"lane"));

        // The server forgot the client: not_paired.
        sp.lock().unwrap().clear();
        let (c, s, _, _) = run(None, &cp, &sp, &budget);
        assert_eq!(refusal(&c.unwrap_err()), Some(NOT_PAIRED));
        assert_eq!(refusal(&s.unwrap_err()), Some(NOT_PAIRED));
        assert_eq!(
            budget.lock().unwrap().failures(std::time::Instant::now()),
            0
        );
    }

    #[test]
    fn denied_and_unanswered_requests_are_counted() {
        let (cp, sp) = (Peers::default(), Peers::default());
        let budget = Arc::new(Mutex::new(Budget::default()));
        // No answer within PAIR_WAIT: timed out on both sides.
        let (c, s, shown, _) = run(Some(None), &cp, &sp, &budget);
        assert_eq!(refusal(&c.unwrap_err()), Some(TIMED_OUT));
        assert_eq!(refusal(&s.unwrap_err()), Some(TIMED_OUT));
        assert!(shown.is_some(), "the check number shows while it waits");
        for _ in 1..Budget::PER_IP {
            let (c, s, _, _) = run(Some(Some(false)), &cp, &sp, &budget);
            assert_eq!(refusal(&c.unwrap_err()), Some(DENIED));
            assert_eq!(refusal(&s.unwrap_err()), Some(DENIED));
        }
        assert!(cp.lock().unwrap().is_empty() && sp.lock().unwrap().is_empty());
        // Out of tries: refused without asking the user.
        let (c, _, shown, asked) = run(Some(Some(true)), &cp, &sp, &budget);
        assert_eq!(refusal(&c.unwrap_err()), Some(TOO_MANY_ATTEMPTS));
        assert!(shown.is_none() && asked.is_empty());
        assert_eq!(
            budget.lock().unwrap().failures(std::time::Instant::now()),
            Budget::PER_IP
        );
    }

    /// A paired, keyed control conn over loopback: ((client conn, session), (server conn,
    /// session), the listener for lanes).
    fn paired() -> ((Conn, Session), (Conn, Session), TcpListener) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let l = listener.try_clone().unwrap();
        let server = std::thread::spawn(move || {
            let mut c = Conn::new(l.accept().unwrap().0, MAX_CONTROL_BODY, DEAD_AFTER).unwrap();
            let budget = Mutex::new(Budget::default());
            let user = Answer(Some(true), Mutex::new(Vec::new()));
            let h = server_handshake(&mut c, &me(2, "ps5"), &|_| None, &user, &budget);
            (c, h.unwrap().session)
        });
        let mut c = Conn::connect(addr, MAX_CONTROL_BODY, DEAD_AFTER).unwrap();
        let h = client_handshake(&mut c, &me(1, "mac"), &|_| None, Some(&mut |_| {})).unwrap();
        ((c, h.session), server.join().unwrap(), listener)
    }

    #[test]
    fn rpc_round_trip_with_ping() {
        let ((c, _), (s, _), _) = paired();
        let server = std::thread::spawn(move || {
            let (tx, mut rx) = s.split().unwrap();
            let tx = Mutex::new(tx);
            // The client's Ping is answered on the way; then one request, echoed back.
            let Control::Request(r) = rx.recv_control(&tx).unwrap() else {
                panic!("wanted a request");
            };
            let answer = match r.method.as_str() {
                "info" => Response::new(r.id, Ok(r.body)),
                _ => Response::new(r.id, Err("unknown method".into())),
            };
            lock(&tx).send_frame(&answer.frame().unwrap()).unwrap();
            let Control::Request(r) = rx.recv_control(&tx).unwrap() else {
                panic!("wanted a request");
            };
            lock(&tx)
                .send_frame(
                    &Response::new(r.id, Err("unknown method".into()))
                        .frame()
                        .unwrap(),
                )
                .unwrap();
        });
        let (tx, mut rx) = c.split().unwrap();
        let tx = Mutex::new(tx);
        lock(&tx)
            .send_frame(&Frame::new(kind2::PING, 0, Vec::new()))
            .unwrap();
        let req = Request {
            id: 7,
            method: "info".into(),
            body: serde_json::json!({"path": "/data"}),
        };
        lock(&tx).send_frame(&req.frame().unwrap()).unwrap();
        // The Pong is swallowed; the response comes up.
        let Control::Response(r) = rx.recv_control(&tx).unwrap() else {
            panic!("wanted a response");
        };
        assert_eq!(r.id, 7);
        assert_eq!(r.result(), Ok(serde_json::json!({"path": "/data"})));
        let req = Request {
            id: 8,
            method: "nope".into(),
            body: serde_json::Value::Null,
        };
        lock(&tx).send_frame(&req.frame().unwrap()).unwrap();
        let Control::Response(r) = rx.recv_control(&tx).unwrap() else {
            panic!("wanted a response");
        };
        assert_eq!((r.id, r.result()), (8, Err("unknown method".into())));
        server.join().unwrap();
        // Wire shape: {id, method, body} and {id, ok} / {id, err}.
        let ok = serde_json::to_value(Response::new(1, Ok(serde_json::json!(3)))).unwrap();
        assert_eq!(ok, serde_json::json!({"id": 1, "ok": 3}));
    }

    #[test]
    fn lane_join_checked() {
        let ((_c, cs), (_s, ss), listener) = paired();
        assert_eq!(cs.id(), ss.id());
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let lookup = |id: &str| (id == ss.id()).then(|| ss.clone());
            let mut out = Vec::new();
            for _ in 0..2 {
                let mut c = Conn::new(listener.accept().unwrap().0, MAX_BODY, DEAD_AFTER).unwrap();
                let r = server_join(&mut c, &lookup);
                if r.is_ok() {
                    let f = c.recv_frame().unwrap();
                    c.send_frame(&f).unwrap();
                }
                out.push(r.map(|(_, lane, _)| lane));
            }
            out
        });
        // A good Join, then a MAC'd frame both ways under the lane keys.
        let mut c = Conn::connect(addr, MAX_BODY, DEAD_AFTER).unwrap();
        client_join(&mut c, &cs, 3).unwrap();
        c.send_frame(&Frame::new(kind2::DATA, 1, vec![9; 1000]))
            .unwrap();
        assert_eq!(c.recv_frame().unwrap().body, vec![9; 1000]);
        // The right session id with the wrong secret: a bad proof, refused.
        let mut forged = cs.clone();
        forged.secret[0] ^= 1;
        let mut c = Conn::connect(addr, MAX_BODY, DEAD_AFTER).unwrap();
        assert_eq!(
            refusal(&client_join(&mut c, &forged, 0).unwrap_err()),
            Some(BAD_JOIN)
        );
        let out = server.join().unwrap();
        assert_eq!(out[0].as_ref().unwrap(), &3);
        assert_eq!(refusal(out[1].as_ref().unwrap_err()), Some(BAD_JOIN));
    }

    #[test]
    fn manifest_paths_checked() {
        for ok in ["a", "Game/sce_sys/param.json", "a b/c.d", "...x"] {
            assert_eq!(check_path(ok), Ok(()), "{ok}");
        }
        let long = "a/".repeat(MAX_PATH / 2) + "b";
        for bad in [
            "",
            "/etc/passwd",
            "a/../b",
            "..",
            "./a",
            "a/./b",
            "a//b",
            "a/",
            "a\\b",
            "C:\\x",
            "a\0b",
            &long,
        ] {
            assert!(check_path(bad).is_err(), "{bad:?}");
        }
    }
}
