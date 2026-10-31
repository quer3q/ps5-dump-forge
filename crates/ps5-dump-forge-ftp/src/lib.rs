//! A small blocking plain-FTP client (RFC 959, with RFC 2389 FEAT, RFC 3659 MLSD/SIZE/MDTM/REST
//! and RFC 2428 EPSV): logs in (user and password, or anonymous), lists folders and reads files
//! over passive data connections in binary mode. One control connection per [`Session`], one
//! command at a time.
// ponytail: no TLS (FTPS), no active mode (PORT/EPRT), no reconnect; plaintext on the LAN.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 21;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(30);
/// Longest reply line read (a longer one is cut there and the rest read as the next line).
const MAX_LINE: u64 = 64 * 1024;
/// Most entries one listing returns (more is an error, never a cut list).
const MAX_ENTRIES: usize = 200_000;
/// Longest whole reply (FEAT and a multi-line error stay far below it).
const MAX_REPLY: usize = 1024 * 1024;
/// How long a transfer cut short waits for its reply before `ABOR`.
const ABORT_WAIT: Duration = Duration::from_secs(5);

/// A directory entry or a stat result. `mtime` is seconds since the Unix epoch, when the server
/// gives one; `link` marks a symbolic link (never followed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: Option<i64>,
    pub link: bool,
}

/// What the server's FEAT reply offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Features {
    mlst: bool,
    epsv: bool,
    utf8: bool,
    size: bool,
    rest: bool,
}

/// The control connection: commands go out on `stream`, replies come back through `reader`.
struct Control {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    features: Features,
    /// EPSV is tried until a server refuses it once; then PASV only.
    try_epsv: bool,
}

/// One logged-in connection to a server. Clones share the connection.
#[derive(Clone)]
pub struct Session(Arc<Mutex<Control>>);

/// Reads one reply: a single line `123 text`, or a multi-line `123-text` … `123 text` (the
/// lines between may say anything). Returns the code and the text, lines joined by `\n`.
fn read_reply<R: BufRead>(r: &mut R) -> io::Result<(u16, String)> {
    let first = read_line(r)?;
    let (code, sep) = parse_code(&first).ok_or_else(|| bad(format!("bad reply: {first}")))?;
    let mut text = first.get(4..).unwrap_or("").to_string();
    if sep == b'-' {
        let end = format!("{code} ");
        loop {
            let line = read_line(r)?;
            if line.starts_with(&end) || line == end.trim_end() {
                text.push('\n');
                text.push_str(line.get(4..).unwrap_or(""));
                break;
            }
            if text.len() + line.len() > MAX_REPLY {
                return Err(bad("reply too long".into()));
            }
            text.push('\n');
            text.push_str(&line);
        }
    }
    Ok((code, text))
}

/// One line without its `\r\n`; non-UTF-8 bytes replaced. End of stream is an error.
fn read_line<R: BufRead>(r: &mut R) -> io::Result<String> {
    let mut buf = Vec::new();
    r.by_ref().take(MAX_LINE).read_until(b'\n', &mut buf)?;
    if buf.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the FTP server closed the connection",
        ));
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// `123 ` / `123-` / a bare `123`: the code and the separator (a bare code counts as `' '`).
fn parse_code(line: &str) -> Option<(u16, u8)> {
    let b = line.as_bytes();
    if b.len() < 3 || !b[..3].iter().all(u8::is_ascii_digit) || b[0] == b'0' {
        return None;
    }
    let sep = match b.get(3) {
        None => b' ',
        Some(&s @ (b' ' | b'-')) => s,
        Some(_) => return None,
    };
    Some((line[..3].parse().ok()?, sep))
}

fn bad(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// A reply outside the wanted class as an error: not found, permission denied, a closing
/// connection (421: vsftpd's "Timeout." to an idle session, read before the reset), else the text.
fn reply_error(code: u16, text: &str) -> io::Error {
    let lower = text.to_ascii_lowercase();
    let kind = if code == 421 {
        io::ErrorKind::ConnectionAborted
    } else if code == 550 && (lower.contains("not found") || lower.contains("no such")) {
        io::ErrorKind::NotFound
    } else if code == 530 || code == 532 || lower.contains("permission") || lower.contains("denied")
    {
        io::ErrorKind::PermissionDenied
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, format!("FTP {code} {}", text.trim()))
}

/// Checks a reply's class (`2` for 2xx, ...) and hands back its text.
fn expect((code, text): (u16, String), class: u16) -> io::Result<String> {
    if code / 100 == class {
        Ok(text)
    } else {
        Err(reply_error(code, &text))
    }
}

/// The FEAT reply's feature lines (the first and last line are the reply's own).
fn parse_features(text: &str) -> Features {
    let mut f = Features::default();
    for line in text.lines().skip(1) {
        let word = line.split_whitespace().next().unwrap_or("");
        match word.to_ascii_uppercase().as_str() {
            "MLST" | "MLSD" => f.mlst = true,
            "EPSV" => f.epsv = true,
            "UTF8" => f.utf8 = true,
            "SIZE" => f.size = true,
            "REST" => f.rest = line.to_ascii_uppercase().contains("STREAM"),
            _ => {}
        }
    }
    f
}

impl Control {
    /// `MLSD` (when the server has it) or `LIST` of the folder `path`, without `.` and `..`; lines no parser reads are skipped. More than [`MAX_ENTRIES`]
    /// entries is an error.
    fn listing(&mut self, path: &str) -> io::Result<Vec<Entry>> {
        self.listing_rows(path, "").map(|(entries, _)| entries)
    }

    /// [`Control::listing`], and how many lines no parser read hold `name` (when not empty).
    fn listing_rows(&mut self, path: &str, name: &str) -> io::Result<(Vec<Entry>, usize)> {
        let mlsd = self.features.mlst;
        let data = self.data()?;
        let verb = if mlsd { "MLSD" } else { "LIST" };
        let line = if path.is_empty() {
            verb.to_string()
        } else {
            format!("{verb} {path}")
        };
        let (code, text) = self.cmd(&line)?;
        if code / 100 != 1 && code / 100 != 2 {
            return Err(reply_error(code, &text));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut r = BufReader::new(data);
        let mut out = Vec::new();
        let mut unread = 0;
        loop {
            let line = match read_line(&mut r) {
                Ok(line) => line,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            };
            let e = if mlsd {
                parse_mlsd(&line)
            } else {
                parse_unix(&line, now)
                    .or_else(|| parse_dos(&line))
                    .map(|e| Entry { mtime: None, ..e })
            };
            match e {
                Some(e) => out.push(e),
                None if !name.is_empty() && line.contains(name) => unread += 1,
                None => {}
            }
            if out.len() > MAX_ENTRIES {
                drop(r);
                let _ = self.read_reply();
                return Err(bad(format!("{path}: more than {MAX_ENTRIES} entries")));
            }
        }
        if code / 100 == 1 {
            expect(self.read_reply()?, 2)?;
        }
        Ok((out, unread))
    }

    fn read_reply(&mut self) -> io::Result<(u16, String)> {
        read_reply(&mut self.reader)
    }

    /// Sends one command line and reads its reply.
    fn cmd(&mut self, line: &str) -> io::Result<(u16, String)> {
        if line.contains(['\r', '\n']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an FTP command can't hold a line break",
            ));
        }
        self.stream.write_all(format!("{line}\r\n").as_bytes())?;
        self.stream.flush()?;
        self.read_reply()
    }

    /// After a transfer cut short: waits up to [`ABORT_WAIT`] for its final reply (426, 451,
    /// 226, ...); without one, sends `ABOR` and reads replies until 225/226 or another final
    /// one past a 426/451; then `NOOP`, its reply read past any late one of the transfer's.
    fn settle_cut(&mut self) -> io::Result<()> {
        self.stream.set_read_timeout(Some(ABORT_WAIT))?;
        let r = self.settle_cut_inner();
        self.stream.set_read_timeout(Some(TIMEOUT))?;
        r
    }

    fn settle_cut_inner(&mut self) -> io::Result<()> {
        let timed_out = |e: &io::Error| {
            matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            )
        };
        let settled = loop {
            match self.read_reply() {
                Ok((code, _)) if code / 100 == 1 => {}
                Ok(_) => break true,
                Err(e) if timed_out(&e) => break false,
                Err(e) => return Err(e),
            }
        };
        if !settled {
            let (mut code, _) = self.cmd("ABOR")?;
            while matches!(code, 426 | 451) || code / 100 == 1 {
                code = self.read_reply()?.0;
            }
        }
        // Some servers follow a 426 with a 226 (or a 226 with ABOR's 225): `NOOP`'s reply
        // marks where the cut transfer's replies end, so none is taken for a later command's.
        self.stream.write_all(b"NOOP\r\n")?;
        self.stream.flush()?;
        for _ in 0..8 {
            let (code, _) = self.read_reply()?;
            if !(code / 100 == 1 || matches!(code, 225 | 226 | 250 | 426 | 451)) {
                return Ok(());
            }
        }
        Err(bad("no reply to NOOP after a cut transfer".into()))
    }

    /// Opens a passive data connection: EPSV (the control connection's address, the port the
    /// server names), else PASV (its address, or the control peer's when that one can't be
    /// meant: see [`pasv_target`]).
    fn data(&mut self) -> io::Result<TcpStream> {
        let peer = self.stream.peer_addr()?;
        let mut addr = None;
        if self.try_epsv {
            let (code, text) = self.cmd("EPSV")?;
            match (code, parse_epsv(&text)) {
                (229, Some(port)) => addr = Some(SocketAddr::new(peer.ip(), port)),
                (229, None) => return Err(bad(format!("bad EPSV reply: {text}"))),
                _ => self.try_epsv = false,
            }
        }
        let addr = match addr {
            Some(a) => a,
            None => {
                let text = expect(self.cmd("PASV")?, 2)?;
                let (ip, port) =
                    parse_pasv(&text).ok_or_else(|| bad(format!("bad PASV reply: {text}")))?;
                SocketAddr::new(pasv_target(ip, peer.ip()), port)
            }
        };
        let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        tcp.set_write_timeout(Some(TIMEOUT))?;
        Ok(tcp)
    }
}

/// `229 Entering Extended Passive Mode (|||6446|)`: the port (any delimiter, RFC 2428).
fn parse_epsv(text: &str) -> Option<u16> {
    let inner = &text[text.find('(')? + 1..];
    let inner = &inner[..inner.find(')')?];
    let d = inner.chars().next()?;
    let parts: Vec<&str> = inner.split(d).collect();
    // `|||port|` splits into "", "", "", "port", "".
    if parts.len() != 5 || !parts[1..3].iter().all(|p| p.is_empty()) || !parts[4].is_empty() {
        return None;
    }
    parts[3].parse().ok().filter(|&p| p != 0)
}

/// `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)` (parentheses optional): address and port.
fn parse_pasv(text: &str) -> Option<(Ipv4Addr, u16)> {
    let text = text.find('(').map_or(text, |i| &text[i + 1..]);
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let nums: Vec<u8> = text[start..]
        .split(|c: char| !c.is_ascii_digit() && c != ',')
        .next()?
        .split(',')
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    let [a, b, c, d, p1, p2] = nums[..] else {
        return None;
    };
    let port = u16::from(p1) << 8 | u16::from(p2);
    (port != 0).then_some((Ipv4Addr::new(a, b, c, d), port))
}

/// Where PASV's data connection goes: the server's address, unless it is unspecified, the
/// control connection runs over IPv6, or one of the two is a private address and the other is
/// not (a server behind NAT naming its inside address, or one naming its outside address to a
/// client on its LAN); then the control connection's peer.
fn pasv_target(pasv: Ipv4Addr, peer: IpAddr) -> IpAddr {
    let local = |ip: Ipv4Addr| ip.is_private() || ip.is_loopback() || ip.is_link_local();
    match peer {
        IpAddr::V4(p) if !pasv.is_unspecified() && local(pasv) == local(p) => IpAddr::V4(pasv),
        _ => peer,
    }
}

impl Session {
    /// Whether `other` is a clone of this session (the same connection).
    pub fn same(&self, other: &Session) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Connects and logs in: `user` empty is an anonymous login (`anonymous`, password
    /// `forge@`). Then binary mode, and UTF-8 names when the server offers them.
    pub fn connect(host: &str, port: u16, user: &str, pass: &str) -> io::Result<Session> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("{host}: no address"))
        })?;
        let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        tcp.set_write_timeout(Some(TIMEOUT))?;
        let mut c = Control {
            reader: BufReader::new(tcp.try_clone()?),
            stream: tcp,
            features: Features::default(),
            try_epsv: true,
        };
        expect(c.read_reply()?, 2)?;
        let (user, pass) = if user.is_empty() {
            ("anonymous", "forge@")
        } else {
            (user, pass)
        };
        let (code, text) = c.cmd(&format!("USER {user}"))?;
        match code {
            230 => {}
            331 => {
                expect(c.cmd(&format!("PASS {pass}"))?, 2)?;
            }
            _ => return Err(reply_error(code, &text)),
        }
        if let Ok((211, text)) = c.cmd("FEAT") {
            c.features = parse_features(&text);
        }
        if c.features.utf8 {
            let _ = c.cmd("OPTS UTF8 ON");
        }
        expect(c.cmd("TYPE I")?, 2)?;
        Ok(Session(Arc::new(Mutex::new(c))))
    }

    fn control(&self) -> std::sync::MutexGuard<'_, Control> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Lists a folder (`MLSD` when the server has it, else `LIST`), without `.` and `..`;
    /// lines no parser reads are skipped. More than [`MAX_ENTRIES`] entries is an error.
    /// Times come from `MLSD` only: a `LIST` line's (minutes, or a day for an older file, in
    /// the server's zone) can't match `MDTM`'s, so a [`Session::stat`] could never agree.
    // ponytail: without MLST a file's change shows by its size only.
    pub fn list(&self, path: &str) -> io::Result<Vec<Entry>> {
        self.control().listing(path)
    }

    /// One path's entry, named by its last component: `MLST` when the server has it; else
    /// `SIZE` (a file, with no time, as [`Session::list`] gives none there), else `CWD` into it
    /// and back (a folder), else its folder's listing (not found when no line there can be it).
    // ponytail: without MLST, SIZE and CWD follow a link, so a link named directly reads as its
    // target (still within the server's tree); a link inside a listed folder is marked and refused.
    pub fn stat(&self, path: &str) -> io::Result<Entry> {
        let mut c = self.control();
        let name = last_name(path).to_string();
        if c.features.mlst {
            let (code, text) = c.cmd(&format!("MLST {path}"))?;
            // A 550 to MLST is a missing path whatever its text ("Failed to open file", ...),
            // unless it says it was refused.
            let e = reply_error(code, &text);
            if code == 550 && e.kind() == io::ErrorKind::Other {
                return Err(io::Error::new(io::ErrorKind::NotFound, e.to_string()));
            }
            let text = expect((code, text), 2)?;
            let line = text
                .lines()
                .skip(1)
                .find_map(|l| l.strip_prefix(' '))
                .ok_or_else(|| bad(format!("{path}: MLST gave no facts")))?;
            let mut e = parse_facts(line, false)
                .ok_or_else(|| bad(format!("{path}: bad MLST facts: {line}")))?;
            e.name = name;
            return Ok(e);
        }
        let (code, text) = c.cmd(&format!("SIZE {path}"))?;
        if code == 213 {
            let size = parse_size(&text)?;
            return Ok(Entry {
                name,
                dir: false,
                size,
                mtime: None,
                link: false,
            });
        }
        let pwd = expect(c.cmd("PWD")?, 2)?;
        let back = parse_pwd(&pwd).ok_or_else(|| bad(format!("bad PWD reply: {pwd}")))?;
        if c.cmd(&format!("CWD {path}"))?.0 / 100 == 2 {
            expect(c.cmd(&format!("CWD {back}"))?, 2)?;
            return Ok(Entry {
                name,
                dir: true,
                size: 0,
                mtime: None,
                link: false,
            });
        }
        // A file the server refuses SIZE for is still there: its folder's listing names it, so
        // a name is never taken for free (and `STOR` never replaces it) while it is. A folder
        // that can't be listed for another reason than its absence is no answer either.
        let parent = match path.trim_end_matches('/').rsplit_once('/') {
            Some(("", _)) => "/",
            Some((p, _)) => p,
            None => "",
        };
        match c.listing_rows(parent, &name) {
            Ok((listed, unread)) => {
                if let Some(e) = listed
                    .into_iter()
                    .find(|e| !name.is_empty() && e.name == name)
                {
                    return Ok(e);
                }
                // A line no parser reads may be this name: no answer, never "free".
                if unread > 0 {
                    return Err(bad(format!(
                        "{path}: can't tell whether it exists (its folder's listing has a line Forge can't read)"
                    )));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{path}: not found (FTP {code} {})", text.trim()),
        ))
    }

    /// Up to `len` bytes of a file from `offset` (`REST` + `RETR`); fewer only at the end of
    /// the file. Stopping before the end closes the data connection and settles the transfer's
    /// reply (with `ABOR` when the server says nothing for 5 s), so the session stays usable.
    // ponytail: ABOR goes without Telnet IP/Synch; a server that needs them times out instead.
    pub fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let mut c = self.control();
        let mut data = c.data()?;
        if offset > 0 {
            let (code, text) = c.cmd(&format!("REST {offset}"))?;
            if code != 350 {
                return Err(reply_error(code, &text));
            }
        }
        let (code, text) = c.cmd(&format!("RETR {path}"))?;
        if code / 100 != 1 {
            return Err(reply_error(code, &text));
        }
        let mut buf = Vec::with_capacity(len.min(16 << 20));
        (&mut data).take(len as u64).read_to_end(&mut buf)?;
        // At `len`, one more byte tells the file's end (the whole rest read) from more to come.
        let more = buf.len() == len && data.read(&mut [0u8])? > 0;
        if !more {
            drop(data);
            expect(c.read_reply()?, 2)?;
            return Ok(buf);
        }
        let _ = data.shutdown(std::net::Shutdown::Both);
        drop(data);
        c.settle_cut()?;
        Ok(buf)
    }

    /// A file's size (`SIZE`).
    pub fn size(&self, path: &str) -> io::Result<u64> {
        let mut c = self.control();
        parse_size(&expect(c.cmd(&format!("SIZE {path}"))?, 2)?)
    }

    /// Starts writing a file (`STOR`, replacing one the server has there): the bytes go through
    /// the returned [`Upload`], which [`Upload::finish`] completes. The control connection is
    /// busy until then: a caller that needs the server meanwhile uses another `Session`.
    pub fn store(&self, path: &str) -> io::Result<Upload> {
        self.store_at(path, 0)
    }

    /// [`Session::store`] from byte `offset` (`REST offset` first): on a server that resumes an
    /// upload in place, what is sent overwrites the file from there and nothing after it is cut
    /// (servers differ: callers check with a probe). `offset` 0 is a plain `STOR`, which replaces
    /// the file.
    pub fn store_at(&self, path: &str, offset: u64) -> io::Result<Upload> {
        let mut c = self.control();
        let data = c.data()?;
        if offset > 0 {
            let (code, text) = c.cmd(&format!("REST {offset}"))?;
            if code != 350 {
                return Err(reply_error(code, &text));
            }
        }
        let (code, text) = c.cmd(&format!("STOR {path}"))?;
        if code / 100 != 1 {
            return Err(reply_error(code, &text));
        }
        Ok(Upload {
            session: self.clone(),
            data: Some(data),
        })
    }

    /// Makes a folder (`MKD`); one already there as a folder is `AlreadyExists`.
    pub fn mkdir(&self, path: &str) -> io::Result<()> {
        let (code, text) = self.control().cmd(&format!("MKD {path}"))?;
        if code / 100 == 2 {
            return Ok(());
        }
        if matches!(code, 550 | 521) && self.stat(path).is_ok_and(|e| e.dir) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{path}: already exists"),
            ));
        }
        Err(reply_error(code, &text))
    }

    /// Renames `from` to `to` (`RNFR` + `RNTO`), never over an existing `to` (`AlreadyExists`).
    // ponytail: check, then rename; a `to` made in between is replaced where the server does so.
    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        match self.stat(to) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{to}: already exists"),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.rename_replace(from, to)
    }

    /// Renames the file `from` to `to` (`RNFR` + `RNTO`), over a file there: servers rename as
    /// the filesystem does, which replaces a file and refuses a folder.
    pub fn rename_replace(&self, from: &str, to: &str) -> io::Result<()> {
        let mut c = self.control();
        let (code, text) = c.cmd(&format!("RNFR {from}"))?;
        if code != 350 {
            return Err(reply_error(code, &text));
        }
        expect(c.cmd(&format!("RNTO {to}"))?, 2).map(drop)
    }

    /// Deletes a file (`DELE`).
    pub fn remove(&self, path: &str) -> io::Result<()> {
        expect(self.control().cmd(&format!("DELE {path}"))?, 2).map(drop)
    }
}

/// A file being written by [`Session::store`]. [`Upload::finish`] completes it; dropped
/// unfinished, the data connection is closed and the transfer's reply settled (the session stays
/// usable; what the server keeps of the file is its own choice, usually the bytes sent).
pub struct Upload {
    session: Session,
    data: Option<TcpStream>,
}

impl Upload {
    fn stream(&mut self) -> io::Result<&mut TcpStream> {
        self.data
            .as_mut()
            .ok_or_else(|| io::Error::other("the upload is finished"))
    }

    /// Closes the data connection and reads the transfer's reply (226/250, else an error).
    pub fn finish(mut self) -> io::Result<()> {
        let mut data = self
            .data
            .take()
            .expect("an unfinished upload has its data connection");
        let sent = data
            .flush()
            .and_then(|()| data.shutdown(std::net::Shutdown::Write));
        drop(data);
        let mut c = self.session.control();
        if let Err(e) = sent {
            let _ = c.settle_cut();
            return Err(e);
        }
        let (code, text) = c.read_reply()?;
        if matches!(code, 226 | 250) {
            Ok(())
        } else {
            Err(reply_error(code, &text))
        }
    }
}

impl Write for Upload {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream()?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream()?.flush()
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        if let Some(data) = self.data.take() {
            let _ = data.shutdown(std::net::Shutdown::Both);
            drop(data);
            let _ = self.session.control().settle_cut();
        }
    }
}

/// `213 12345`: the size.
fn parse_size(text: &str) -> io::Result<u64> {
    text.trim()
        .parse()
        .map_err(|_| bad(format!("bad SIZE reply: {text}")))
}

/// Seconds since the Unix epoch of a UTC date and time; `None` when a field is out of range.
fn utc_secs(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Option<i64> {
    // A year past 9999 (a server's garbage) would overflow the day count.
    if !(1..=9999).contains(&y)
        || !(1..=12).contains(&mo)
        || !(1..=31).contains(&d)
        || h > 23
        || mi > 59
        || s > 60
    {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((mo + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + i64::from(h * 3600 + mi * 60 + s))
}

/// The UTC year of a Unix time.
fn year_of(secs: i64) -> i64 {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    era * 400 + yoe + i64::from(mp >= 10)
}

/// A name `list` keeps: not empty, not `.` or `..`.
fn keep(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}

/// One MLSD line: `type=file;size=12;modify=20261021091500.123; name`. `cdir`/`pdir` entries,
/// `.` and `..` give `None`. A type other than `file`/`dir` (a symlink, a device, ...) is
/// marked `link`: never followed or read.
fn parse_mlsd(line: &str) -> Option<Entry> {
    parse_facts(line, true)
}

/// A fact line: `listing` (MLSD) skips `cdir`/`pdir`, `.` and `..`; otherwise (MLST's one
/// line, whose name is the path asked for) any name is taken and `cdir`/`pdir` are folders.
fn parse_facts(line: &str, listing: bool) -> Option<Entry> {
    let (facts, name) = line.split_once(' ')?;
    if listing && !keep(name) {
        return None;
    }
    let mut e = Entry {
        name: name.to_string(),
        dir: false,
        size: 0,
        mtime: None,
        link: false,
    };
    let mut typed = false;
    for fact in facts.split(';').filter(|f| !f.is_empty()) {
        let (k, v) = fact.split_once('=')?;
        match k.to_ascii_lowercase().as_str() {
            "type" => {
                typed = true;
                match v.to_ascii_lowercase().as_str() {
                    "file" => {}
                    "dir" => e.dir = true,
                    "cdir" | "pdir" if listing => return None,
                    "cdir" | "pdir" => e.dir = true,
                    _ => e.link = true,
                }
            }
            "size" => e.size = v.parse().ok()?,
            "modify" => e.mtime = parse_time14(v),
            _ => {}
        }
    }
    typed.then_some(e)
}

/// `YYYYMMDDHHMMSS` with an optional `.fraction` (MLSD's `modify`, MDTM's reply), in UTC.
fn parse_time14(v: &str) -> Option<i64> {
    let v = v.trim().split('.').next().unwrap_or("");
    if v.len() != 14 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| v[r].parse::<u32>().ok();
    utc_secs(
        i64::from(n(0..4)?),
        n(4..6)?,
        n(6..8)?,
        n(8..10)?,
        n(10..12)?,
        n(12..14)?,
    )
}

/// `257 "/a ""b"" c" is the current directory`: the quoted path, `""` read as `"`.
fn parse_pwd(text: &str) -> Option<String> {
    let mut chars = text[text.find('"')? + 1..].chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
            } else {
                return Some(out);
            }
        }
        out.push(c);
    }
    None
}

/// The last component of a path (`a/b/` gives `b`).
fn last_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// The whitespace-separated words of a line, each with the byte offset just past it.
fn words(line: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(|c: char| !c.is_ascii_whitespace()) {
        let len = rest[start..]
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(rest.len() - start);
        let end = line.len() - rest.len() + start + len;
        out.push((&rest[start..start + len], end));
        rest = &rest[start + len..];
    }
    out
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// One unix `ls -l` line: `drwxr-xr-x 2 owner group 4096 Oct 21 09:15 name` (group optional;
/// a year in place of the time). A `l` entry's ` -> target` is cut off and it is marked `link`,
/// as is any other type but `-` and `d`. `now` (Unix seconds) gives the year a time-of-day
/// date is in: this one, or the last when that would be in the future.
// ponytail: dates are the server's local time read as UTC; a year-only date is at midnight.
fn parse_unix(line: &str, now: i64) -> Option<Entry> {
    let w = words(line);
    let perms = w.first()?.0;
    let kind = perms.chars().next()?;
    if perms.len() < 10 || !"-dlcbps".contains(kind) {
        return None;
    }
    // The date: a month word followed by a day and a time or year, after the size.
    let i = (3..w.len().saturating_sub(2)).find(|&i| {
        MONTHS.contains(&w[i].0.to_ascii_lowercase().as_str())
            && w[i + 1]
                .0
                .parse::<u32>()
                .is_ok_and(|d| (1..=31).contains(&d))
            && w[i - 1].0.parse::<u64>().is_ok()
    })?;
    let size = w[i - 1].0.parse().ok()?;
    let mo = MONTHS
        .iter()
        .position(|m| *m == w[i].0.to_ascii_lowercase())? as u32
        + 1;
    let day = w[i + 1].0.parse().ok()?;
    let (t, end) = w[i + 2];
    let mtime = if let Some((h, m)) = t.split_once(':') {
        let (h, m) = (h.parse().ok()?, m.parse().ok()?);
        let year = year_of(now);
        let at = utc_secs(year, mo, day, h, m, 0)?;
        if at > now + 86_400 {
            utc_secs(year - 1, mo, day, h, m, 0)
        } else {
            Some(at)
        }
    } else {
        utc_secs(t.parse().ok()?, mo, day, 0, 0, 0)
    };
    let mut name = line.get(end + 1..)?;
    if kind == 'l' {
        name = name.split_once(" -> ").map_or(name, |(n, _)| n);
    }
    keep(name).then(|| Entry {
        name: name.to_string(),
        dir: kind == 'd',
        size,
        mtime,
        link: kind != '-' && kind != 'd',
    })
}

/// One DOS/IIS line: `10-21-26  09:15PM       <DIR>          name` or `... 12345 name`
/// (a 2- or 4-digit year; 2-digit ones below 70 are 20xx).
fn parse_dos(line: &str) -> Option<Entry> {
    let w = words(line);
    let (date, time, what) = (w.first()?.0, w.get(1)?.0, w.get(2)?);
    let mut d = date.split('-');
    let (mo, day, y) = (d.next()?, d.next()?, d.next()?);
    let y: i64 = y.parse().ok()?;
    let y = match y {
        0..=69 => 2000 + y,
        70..=99 => 1900 + y,
        _ => y,
    };
    let upper = time.to_ascii_uppercase();
    let (hm, pm) = match upper.strip_suffix("PM") {
        Some(hm) => (hm, true),
        None => (upper.strip_suffix("AM").unwrap_or(&upper), false),
    };
    let (h, m) = hm.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    let h = match (upper.ends_with('M'), pm) {
        (false, _) => h,
        (true, false) => h % 12,
        (true, true) => h % 12 + 12,
    };
    let mtime = utc_secs(y, mo.parse().ok()?, day.parse().ok()?, h, m, 0);
    let dir = what.0.eq_ignore_ascii_case("<DIR>");
    let size = if dir { 0 } else { what.0.parse().ok()? };
    let name = line[what.1..].trim_start();
    keep(name).then(|| Entry {
        name: name.to_string(),
        dir,
        size,
        mtime,
        link: false,
    })
}

/// A file written at any offset over one session of its own (its control connection is busy
/// while a `STOR` is open). In order it is one `STOR`; a write before the file's end closes that and
/// restarts at the offset (`REST` + `STOR`), which the server must take without cutting the file
/// ([`Session::can_rewrite`] checks); a write past the end fills the gap with zeros. A restart at 0
/// would be a new upload (cutting the file), so a rewrite from offset 0 skips the leading bytes it
/// repeats, and one that changes the first byte is refused. [`Writer::pause`] completes the open
/// `STOR` (the server then has every byte, for reading back); the next write resumes.
pub struct Writer {
    session: Session,
    path: String,
    up: Option<Upload>,
    /// Where the open `STOR` writes next.
    pos: u64,
    /// The file's length as written.
    len: u64,
    /// Its first bytes as sent.
    head: Vec<u8>,
}

const HEAD: usize = 64;

impl Session {
    /// A new file at `path` (a plain `STOR`, which replaces one there) written through a
    /// [`Writer`]. Use a session of its own: it is busy while the file goes up.
    pub fn writer(&self, path: &str) -> io::Result<Writer> {
        Ok(Writer {
            up: Some(self.store(path)?),
            session: self.clone(),
            path: path.to_string(),
            pos: 0,
            len: 0,
            head: Vec::new(),
        })
    }

    /// Whether this server rewrites a file in place from an offset (`REST` + `STOR` cutting
    /// nothing after what is sent), which [`Writer`] needs to go back: a probe file named
    /// `probe` (a free name in a writable folder) is written, rewritten, read back and removed;
    /// a name already taken is refused (`AlreadyExists`), never written over.
    // ponytail: the check and the `STOR` are two steps, as in [`Session::writer`]'s callers.
    pub fn can_rewrite(&self, probe: &str) -> io::Result<bool> {
        match self.stat(probe) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{probe}: already exists"),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut made = false;
        let mut attempt = || -> io::Result<Vec<u8>> {
            let mut up = self.store(probe)?;
            made = true;
            up.write_all(b"0123456789")?;
            up.finish()?;
            let mut up = self.store_at(probe, 2)?;
            up.write_all(b"ab")?;
            up.finish()?;
            self.read_at(probe, 0, 16)
        };
        let got = attempt();
        // A probe left behind is said, never passed over: the folder would keep it.
        if made
            && let Err(e) = self.remove(probe)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(io::Error::new(
                e.kind(),
                format!("{probe} was left behind: it could not be removed: {e}"),
            ));
        }
        Ok(got? == b"01ab456789")
    }
}

impl Writer {
    /// The file's length as written.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether nothing was written yet.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Writes `bytes` at `offset` (see [`Writer`]).
    pub fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        if self.up.is_some() && offset == self.pos {
            return self.send(bytes);
        }
        self.pause()?;
        if offset >= self.len {
            // At or past the end: on from there, zeros over a gap.
            self.resume(self.len)?;
            let zeros = [0u8; 64 << 10];
            while self.pos < offset {
                let n = (offset - self.pos).min(zeros.len() as u64) as usize;
                self.send(&zeros[..n])?;
            }
            return self.send(bytes);
        }
        let skip = match offset {
            0 => (self.head.iter().zip(bytes))
                .take_while(|(a, b)| a == b)
                .count(),
            _ => 0,
        };
        if skip == bytes.len() {
            return Ok(());
        }
        if offset + skip as u64 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "an FTP server can't rewrite a file's first byte",
            ));
        }
        self.resume(offset + skip as u64)?;
        self.send(&bytes[skip..])
    }

    /// Completes the open `STOR` (the server then has every byte); the next write resumes.
    pub fn pause(&mut self) -> io::Result<()> {
        match self.up.take() {
            Some(up) => up.finish(),
            None => Ok(()),
        }
    }

    /// Completes the file.
    pub fn finish(mut self) -> io::Result<()> {
        self.pause()
    }

    fn resume(&mut self, at: u64) -> io::Result<()> {
        self.up = Some(self.session.store_at(&self.path, at)?);
        self.pos = at;
        Ok(())
    }

    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let up = self
            .up
            .as_mut()
            .ok_or_else(|| io::Error::other("no open upload"))?;
        up.write_all(bytes)?;
        if (self.pos as usize) < HEAD {
            let from = self.pos as usize;
            let end = (from + bytes.len()).min(HEAD);
            if self.head.len() < end {
                self.head.resize(end, 0);
            }
            self.head[from..end].copy_from_slice(&bytes[..end - from]);
        }
        self.pos += bytes.len() as u64;
        self.len = self.len.max(self.pos);
        Ok(())
    }
}

/// The in-process test server, for this crate's tests and (feature `fake`) other crates'.
#[cfg(any(test, feature = "fake"))]
pub mod fake;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    fn reply(bytes: &[u8]) -> io::Result<(u16, String)> {
        read_reply(&mut Cursor::new(bytes.to_vec()))
    }

    #[test]
    fn multi_line_reply() {
        let mut r = Cursor::new(
            b"220-Hello\r\n there\r\n220-still\r\n221 not the end\r\n220 ready\r\n230 next\r\n"
                .to_vec(),
        );
        let (code, text) = read_reply(&mut r).unwrap();
        assert_eq!(code, 220);
        assert_eq!(text, "Hello\n there\n220-still\n221 not the end\nready");
        assert_eq!(read_reply(&mut r).unwrap(), (230, "next".into()));
    }

    #[test]
    fn bad_and_cut_replies() {
        assert_eq!(reply(b"").unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(
            reply(b"211-Features\r\n UTF8\r\n").unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        for bad in [&b"hello\r\n"[..], b"22 x\r\n", b"220x\r\n", b"012 x\r\n"] {
            assert_eq!(reply(bad).unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn error_mapping() {
        let kind = |code, text| reply_error(code, text).kind();
        assert_eq!(
            kind(550, "No such file or directory."),
            io::ErrorKind::NotFound
        );
        assert_eq!(kind(550, "File not found"), io::ErrorKind::NotFound);
        assert_eq!(
            kind(550, "Permission denied."),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            kind(530, "Login incorrect."),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(kind(532, "Need account"), io::ErrorKind::PermissionDenied);
        assert_eq!(kind(553, "Access denied"), io::ErrorKind::PermissionDenied);
        assert_eq!(kind(550, "Failed to open file."), io::ErrorKind::Other);
        let e = reply_error(421, "Too many users ");
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted);
        assert_eq!(e.to_string(), "FTP 421 Too many users");
        assert_eq!(expect((257, "\"/\"".into()), 2).unwrap(), "\"/\"");
        assert!(expect((331, "Password".into()), 2).is_err());
    }

    #[test]
    fn pasv_address_choice() {
        let v4 = |a, b, c, d| Ipv4Addr::new(a, b, c, d);
        let peer = IpAddr::V4(v4(192, 168, 1, 20));
        assert_eq!(
            pasv_target(v4(192, 168, 1, 21), peer),
            IpAddr::V4(v4(192, 168, 1, 21))
        );
        assert_eq!(pasv_target(v4(0, 0, 0, 0), peer), peer);
        assert_eq!(pasv_target(v4(203, 0, 113, 5), peer), peer);
        let public = IpAddr::V4(v4(203, 0, 113, 5));
        assert_eq!(pasv_target(v4(10, 0, 0, 2), public), public);
        assert_eq!(
            pasv_target(v4(198, 51, 100, 7), public),
            IpAddr::V4(v4(198, 51, 100, 7))
        );
        let v6: IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(pasv_target(v4(192, 168, 1, 21), v6), v6);
    }

    fn entry(name: &str, dir: bool, size: u64, mtime: Option<i64>, link: bool) -> Entry {
        Entry {
            name: name.into(),
            dir,
            size,
            mtime,
            link,
        }
    }

    #[test]
    fn mlsd_lines() {
        let t = utc_secs(2026, 10, 21, 9, 15, 0);
        assert_eq!(
            parse_mlsd("type=file;size=12345;modify=20261021091500.123; eboot.bin"),
            Some(entry("eboot.bin", false, 12345, t, false))
        );
        assert_eq!(
            parse_mlsd("Type=dir;Modify=20261021091500;UNIX.mode=0755; sce sys "),
            Some(entry("sce sys ", true, 0, t, false))
        );
        assert_eq!(
            parse_mlsd("type=OS.unix=symlink;size=4; link"),
            Some(entry("link", false, 4, None, true))
        );
        assert_eq!(
            parse_mlsd("type=OS.unix=slink:/x;modify=bad; l"),
            Some(entry("l", false, 0, None, true))
        );
        for skip in [
            "type=cdir;modify=20261021091500; /games",
            "type=pdir; ..",
            "type=file; .",
            "type=file;",
            "size=1; untyped",
            "type=file;size=x; bad",
            "nofacts",
        ] {
            assert_eq!(parse_mlsd(skip), None, "{skip}");
        }
    }

    /// A file written out of order (as a package is: its header last): a gap filled, a middle
    /// rewritten, the start rewritten past its unchanged first byte; a changed first byte refused.
    #[test]
    fn a_writer_goes_back_with_rest() {
        let server = super::fake::start();
        let s = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        assert!(s.can_rewrite("/probe").unwrap());
        let w_session = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        let mut w = w_session.writer("/pkg").unwrap();
        w.write_at(0, b"MAGIC").unwrap();
        w.write_at(16, b"data-one").unwrap(); // a gap of zeros
        w.write_at(24, b"data-two").unwrap(); // in order
        w.pause().unwrap();
        assert_eq!(s.read_at("/pkg", 16, 8).unwrap(), b"data-one");
        w.write_at(20, b"ONE!").unwrap(); // the middle again
        w.write_at(0, b"MAGIC-head").unwrap(); // the start: past "MAGIC", unchanged
        let e = w.write_at(0, b"xAGIC").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Unsupported);
        assert_eq!(w.len(), 32);
        w.finish().unwrap();
        let mut want = b"MAGIC-head".to_vec();
        want.resize(16, 0);
        want.extend_from_slice(b"dataONE!data-two");
        assert_eq!(s.read_at("/pkg", 0, 64).unwrap(), want);
    }

    /// A file the server denies SIZE for is still found (in its folder's listing), so the
    /// rewrite probe never writes over it; a missing name is still not found.
    #[test]
    fn a_file_without_size_is_never_taken_for_free() {
        let server = super::fake::start();
        fs::write(server.root.join("nosize.bin"), b"keep").unwrap();
        let s = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        let e = s.stat("/nosize.bin").unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.size), ("nosize.bin", false, 4));
        let err = s.can_rewrite("/nosize.bin").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(server.root.join("nosize.bin")).unwrap(), b"keep");
        let err = s.stat("/nosize-missing").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(
            s.stat("/no/such/x").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    /// A file whose listing line no parser reads is no answer to `stat` (never "free"), so the
    /// rewrite probe never writes over it; another name in that folder is still not found.
    #[test]
    fn an_unread_listing_line_is_no_answer() {
        let server = super::fake::start();
        fs::create_dir(server.root.join("odd")).unwrap();
        fs::write(server.root.join("odd/nosize-odd.bin"), b"keep").unwrap();
        let s = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        let err = s.stat("/odd/nosize-odd.bin").unwrap_err();
        assert_ne!(err.kind(), io::ErrorKind::NotFound, "{err}");
        assert_ne!(
            s.can_rewrite("/odd/nosize-odd.bin").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            fs::read(server.root.join("odd/nosize-odd.bin")).unwrap(),
            b"keep"
        );
        assert_eq!(
            s.stat("/odd/nosize-else").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    /// A cut transfer's late 226 (after its 426) is never read as the next command's reply.
    #[test]
    fn a_cut_transfer_s_late_reply_is_not_taken_for_the_next() {
        let server = super::fake::start();
        fs::write(server.root.join("chatty.bin"), [7u8; 100]).unwrap();
        let s = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        assert_eq!(s.read_at("/chatty.bin", 0, 10).unwrap(), [7u8; 10]);
        assert_eq!(s.size("/chatty.bin").unwrap(), 100);
    }

    #[test]
    fn session_against_fake_server() {
        let server = super::fake::start();
        let s = Session::connect("127.0.0.1", server.port, "", "").unwrap();
        let kind = |r: io::Result<()>| r.unwrap_err().kind();

        s.mkdir("games").unwrap();
        assert_eq!(kind(s.mkdir("games")), io::ErrorKind::AlreadyExists);

        let body: Vec<u8> = (0..300_000u32).map(|i| (i * 7 + i / 251) as u8).collect();
        let mut up = s.store("/games/eboot.bin").unwrap();
        for chunk in body.chunks(65_536) {
            up.write_all(chunk).unwrap();
        }
        up.finish().unwrap();
        assert_eq!(fs::read(server.root.join("games/eboot.bin")).unwrap(), body);

        let listed = s.list("/games").unwrap();
        assert_eq!(
            listed,
            vec![entry(
                "eboot.bin",
                false,
                300_000,
                None, // a LIST line's time is never kept
                false
            )]
        );
        assert_eq!(
            s.stat("/games/eboot.bin").unwrap(),
            listed[0],
            "a file's stat agrees with its folder's listing"
        );
        assert_eq!(
            s.stat("games").unwrap(),
            entry("games", true, 0, None, false)
        );
        assert_eq!(
            s.stat("/games/missing").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(s.size("/games/eboot.bin").unwrap(), 300_000);

        let read = |off: u64, len: usize| s.read_at("/games/eboot.bin", off, len).unwrap();
        assert_eq!(read(0, 1000), &body[..1000]);
        assert_eq!(read(150_000, 4096), &body[150_000..154_096]);
        assert_eq!(read(299_000, 1000), &body[299_000..]);
        assert_eq!(read(299_900, 1000), &body[299_900..]);
        assert_eq!(read(400_000, 10), b"");
        assert_eq!(read(10, 0), b"");
        assert_eq!(read(0, 300_000), body);

        s.store("games/other").unwrap().finish().unwrap();
        assert_eq!(
            kind(s.rename("games/eboot.bin", "games/other")),
            io::ErrorKind::AlreadyExists
        );
        s.rename("games/eboot.bin", "games/renamed.bin").unwrap();
        assert_eq!(s.size("games/renamed.bin").unwrap(), 300_000);
        // Replacing: over the file, never over a folder.
        s.rename_replace("games/renamed.bin", "games/other")
            .unwrap();
        assert_eq!(s.size("games/other").unwrap(), 300_000);
        assert_eq!(
            kind(s.size("games/renamed.bin").map(drop)),
            io::ErrorKind::NotFound
        );
        s.mkdir("games/sub").unwrap();
        assert!(s.rename_replace("games/other", "games/sub").is_err());
        assert!(s.stat("games/sub").unwrap().dir);
        fs::remove_dir(server.root.join("games/sub")).unwrap();
        s.rename("games/other", "games/renamed.bin").unwrap();
        s.store("games/other").unwrap().finish().unwrap();
        s.remove("games/renamed.bin").unwrap();
        s.remove("games/other").unwrap();
        assert_eq!(s.list("games").unwrap(), vec![]);
        assert_eq!(kind(s.remove("games/other")), io::ErrorKind::NotFound);

        // An upload dropped part way leaves the session usable.
        let mut up = s.store("games/cut").unwrap();
        up.write_all(&body[..1000]).unwrap();
        drop(up);
        assert_eq!(s.size("games/cut").unwrap(), 1000);
    }
}
