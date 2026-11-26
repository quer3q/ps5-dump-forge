//! SMB2 messages on one TCP connection (direct TCP: a 4-byte length, then the message).

use crate::{Entry, ntlm};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::io::{self, Read, Write};
use std::net::TcpStream;

const NEGOTIATE: u16 = 0;
const SESSION_SETUP: u16 = 1;
const TREE_CONNECT: u16 = 3;
const CREATE: u16 = 5;
const CLOSE: u16 = 6;
const FLUSH: u16 = 7;
const READ: u16 = 8;
const WRITE: u16 = 9;
const IOCTL: u16 = 11;
const QUERY_DIRECTORY: u16 = 14;
const QUERY_INFO: u16 = 16;
const SET_INFO: u16 = 17;
const MORE_PROCESSING: u32 = 0xC000_0016;
const PENDING: u32 = 0x0000_0103;
const NO_MORE_FILES: u32 = 0x8000_0006;
const END_OF_FILE: u32 = 0xC000_0011;
const BUFFER_OVERFLOW: u32 = 0x8000_0005;
/// The largest read or write at once (multi-credit, SMB 2.1's large MTU).
const READ_MAX: usize = 1 << 20;

/// How [`Conn::create`] opens a path; `Delete`/`DeleteDir` delete when closed.
#[derive(Clone, Copy)]
pub(crate) enum Open {
    Read { pipe: bool },
    NewFile,
    NewDir,
    Rename,
    Delete,
    DeleteDir,
}

/// The body is what follows the 64-byte header.
pub(crate) struct Req {
    cmd: u16,
    tree: u32,
    body: Vec<u8>,
    charge: u16,
}

pub(crate) struct Resp {
    pub status: u32,
    pub head: Vec<u8>,
    /// Header included: offsets in bodies count from the header.
    pub msg: Vec<u8>,
}

impl Resp {
    fn u16(&self, at: usize) -> usize {
        self.msg
            .get(64 + at..66 + at)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize)
    }
    fn u32(&self, at: usize) -> u32 {
        self.msg
            .get(64 + at..68 + at)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&self, at: usize) -> u64 {
        self.msg
            .get(64 + at..72 + at)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn bytes(&self, off: usize, len: usize) -> io::Result<&[u8]> {
        self.msg
            .get(off..off.saturating_add(len))
            .ok_or_else(|| bad("a short reply"))
    }
    /// `len` bytes at the server's offset `off`, which must point past the reply's fixed part.
    fn data(&self, off: usize, len: usize, fixed: usize) -> io::Result<&[u8]> {
        if len == 0 {
            return Ok(&[]);
        }
        if off < 64 + fixed {
            return Err(bad("a reply whose data overlaps its header"));
        }
        self.bytes(off, len)
    }
}

/// Replies skipped while one request waits (interim or late ones), so none holds a job forever.
const MAX_SKIPPED: usize = 64;

impl Req {
    pub(crate) fn close(tree: u32, id: &[u8; 16]) -> Req {
        let mut b = vec![24, 0, 0, 0, 0, 0, 0, 0];
        b.extend_from_slice(id);
        Req {
            cmd: CLOSE,
            tree,
            body: b,
            charge: 1,
        }
    }
}

pub(crate) struct Conn {
    tcp: TcpStream,
    next_id: u64,
    credits: u32,
    session: u64,
    sign_key: Option<[u8; 16]>,
    dialect: u16,
    large_mtu: bool,
    max_read: usize,
    max_write: usize,
    sign_required: bool,
}

fn written(n: usize, len: usize) -> io::Result<usize> {
    match n {
        0 => Err(bad("a WRITE that wrote nothing")),
        n if n > len => Err(bad("a WRITE that wrote more than was sent")),
        n => Ok(n),
    }
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("SMB: {what}"))
}

pub(crate) fn status_error(status: u32, what: &str) -> io::Error {
    use io::ErrorKind::*;
    let (kind, text) = match status {
        0xC000_0034 | 0xC000_003A | 0xC000_000F => (NotFound, "not found"),
        0xC000_00CC => (NotFound, "no such share"),
        0xC000_0022 => (PermissionDenied, "access denied"),
        0xC000_006D => (PermissionDenied, "wrong user name or password"),
        0xC000_006E | 0xC000_0072 | 0xC000_0193 => (PermissionDenied, "the account can't log in"),
        0xC000_00BB => (Unsupported, "not supported by the server"),
        0xC000_0035 => (AlreadyExists, "already exists"),
        0xC000_007F => (StorageFull, "the disk is full"),
        0xC000_00A2 => (ReadOnlyFilesystem, "the share is read only"),
        0xC000_0043 => (ResourceBusy, "in use"),
        0xC000_0101 => (DirectoryNotEmpty, "the folder isn't empty"),
        0xC000_00BA => (IsADirectory, "a folder"),
        0xC000_0103 => (NotADirectory, "not a folder"),
        _ => (Other, "failed"),
    };
    io::Error::new(kind, format!("{what}: {text} (NT status 0x{status:08X})"))
}

impl Conn {
    pub(crate) fn new(tcp: TcpStream) -> Conn {
        Conn {
            tcp,
            next_id: 0,
            credits: 1,
            session: 0,
            sign_key: None,
            dialect: 0,
            large_mtu: false,
            max_read: 65536,
            max_write: 65536,
            sign_required: false,
        }
    }

    pub(crate) fn send(&mut self, r: Req) -> io::Result<Resp> {
        let charge = if self.dialect >= 0x0210 {
            r.charge.max(1)
        } else {
            0
        };
        let id = self.next_id;
        self.next_id += u64::from(charge.max(1));
        self.credits = self.credits.saturating_sub(u32::from(charge.max(1)));
        let mut m = Vec::with_capacity(64 + r.body.len());
        m.extend_from_slice(b"\xFESMB");
        m.extend_from_slice(&64u16.to_le_bytes());
        m.extend_from_slice(&charge.to_le_bytes());
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&r.cmd.to_le_bytes());
        m.extend_from_slice(&64u16.to_le_bytes()); // credits asked for
        let sign = self.sign_key.is_some() && r.cmd != SESSION_SETUP;
        m.extend_from_slice(&(if sign { 8u32 } else { 0 }).to_le_bytes());
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&id.to_le_bytes());
        m.extend_from_slice(&0xFEFFu32.to_le_bytes());
        m.extend_from_slice(&r.tree.to_le_bytes());
        m.extend_from_slice(&self.session.to_le_bytes());
        m.extend_from_slice(&[0; 16]);
        m.extend_from_slice(&r.body);
        if let (true, Some(key)) = (sign, self.sign_key) {
            let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&key).expect("16-byte key");
            mac.update(&m);
            m[48..64].copy_from_slice(&mac.finalize().into_bytes()[..16]);
        }
        let len = u32::try_from(m.len()).map_err(|_| bad("a request too large"))?;
        self.tcp.write_all(&len.to_be_bytes())?;
        self.tcp.write_all(&m)?;
        let mut skipped = 0;
        loop {
            let mut n = [0u8; 4];
            self.tcp.read_exact(&mut n)?;
            let len = (u32::from_be_bytes(n) & 0x00FF_FFFF) as usize;
            // Grown as bytes arrive: a server naming 16 MiB and stalling costs no 16 MiB.
            let mut msg = Vec::with_capacity(len.min(2 << 20));
            (&mut self.tcp).take(len as u64).read_to_end(&mut msg)?;
            if msg.len() != len {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if len < 64 || &msg[..4] != b"\xFESMB" {
                // Out of step: a connection-level error, so the session cache logs in afresh.
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "SMB: not an SMB2 reply",
                ));
            }
            let status = u32::from_le_bytes(msg[8..12].try_into().unwrap());
            let granted = u16::from_le_bytes([msg[14], msg[15]]);
            self.credits = self.credits.saturating_add(u32::from(granted));
            let flags = u32::from_le_bytes(msg[16..20].try_into().unwrap());
            let mid = u64::from_le_bytes(msg[24..32].try_into().unwrap());
            if status == PENDING && flags & 2 != 0 || mid != id {
                skipped += 1;
                if skipped > MAX_SKIPPED {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "SMB: no final reply",
                    ));
                }
                continue;
            }
            return Ok(Resp {
                status,
                head: msg[..64].to_vec(),
                msg,
            });
        }
    }

    pub(crate) fn negotiate(&mut self) -> io::Result<()> {
        let r = self.send(Conn::negotiate_req()?)?;
        if r.status != 0 {
            return Err(status_error(
                r.status,
                "negotiate (the server may need SMB 3)",
            ));
        }
        self.sign_required = r.u16(2) & 2 != 0;
        self.dialect = r.u16(4) as u16;
        if !matches!(self.dialect, 0x0202 | 0x0210) {
            return Err(bad("the server chose a dialect we didn't offer"));
        }
        self.large_mtu = self.dialect >= 0x0210 && r.u32(24) & 4 != 0;
        let cap = |max: u32| {
            let max = max as usize;
            if self.large_mtu {
                max.min(READ_MAX)
            } else {
                max.min(65536)
            }
        };
        self.max_read = cap(r.u32(32));
        self.max_write = cap(r.u32(36));
        // A zero size would make every READ and WRITE move nothing, forever.
        if self.max_read == 0 || self.max_write == 0 {
            return Err(bad("the server allows no reads or writes (max size 0)"));
        }
        Ok(())
    }

    pub(crate) fn negotiate_req() -> io::Result<Req> {
        let mut b = Vec::new();
        b.extend_from_slice(&36u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes()); // dialect count
        b.extend_from_slice(&1u16.to_le_bytes()); // signing enabled
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes()); // large MTU
        let mut guid = [0u8; 16];
        getrandom::fill(&mut guid).map_err(|e| io::Error::other(e.to_string()))?;
        b.extend_from_slice(&guid);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&0x0202u16.to_le_bytes());
        b.extend_from_slice(&0x0210u16.to_le_bytes());
        Ok(Req {
            cmd: NEGOTIATE,
            tree: 0,
            body: b,
            charge: 0,
        })
    }

    pub(crate) fn login(&mut self, user: &str, pass: &str) -> io::Result<()> {
        let r = self.setup(&ntlm::negotiate())?;
        if r.status != MORE_PROCESSING {
            return Err(status_error(r.status, "log in"));
        }
        self.session = u64::from_le_bytes(r.head[40..48].try_into().unwrap());
        let challenge = r.data(r.u16(4), r.u16(6), 8)?.to_vec();
        let (token, key) = ntlm::authenticate(&challenge, user, pass)?;
        let r = self.setup(&token)?;
        if r.status != 0 {
            return Err(status_error(r.status, "log in"));
        }
        let guest = r.u16(2) & 3 != 0; // IS_GUEST or IS_NULL
        if !guest && self.sign_required {
            self.sign_key = key;
        }
        Ok(())
    }

    fn setup(&mut self, token: &[u8]) -> io::Result<Resp> {
        let mut b = vec![25, 0, 0, 1]; // structure size, flags, signing enabled
        b.extend_from_slice(&0u32.to_le_bytes()); // capabilities
        b.extend_from_slice(&0u32.to_le_bytes()); // channel
        b.extend_from_slice(&(64u16 + 24).to_le_bytes());
        b.extend_from_slice(&(token.len() as u16).to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(token);
        self.send(Req {
            cmd: SESSION_SETUP,
            tree: 0,
            body: b,
            charge: 1,
        })
    }

    pub(crate) fn tree_connect(&mut self, unc: &str) -> io::Result<u32> {
        let path = ntlm::utf16(unc);
        let mut b = vec![9, 0, 0, 0];
        b.extend_from_slice(&(64u16 + 8).to_le_bytes());
        b.extend_from_slice(&(path.len() as u16).to_le_bytes());
        b.extend_from_slice(&path);
        let r = self.send(Req {
            cmd: TREE_CONNECT,
            tree: 0,
            body: b,
            charge: 1,
        })?;
        if r.status != 0 {
            return Err(status_error(r.status, unc));
        }
        Ok(u32::from_le_bytes(r.head[36..40].try_into().unwrap()))
    }
}

fn filetime_to_unix(ft: u64) -> i64 {
    (ft / 10_000_000) as i64 - 11_644_473_600
}

impl Conn {
    pub(crate) fn create(
        &mut self,
        tree: u32,
        name: &str,
        how: Open,
    ) -> io::Result<([u8; 16], Entry)> {
        let name16 = ntlm::utf16(name);
        // Dispositions FILE_OPEN 1, FILE_CREATE 2. Options: DIRECTORY 1, NON_DIRECTORY 0x40,
        // DELETE_ON_CLOSE 0x1000, OPEN_REPARSE_POINT 0x20_0000 (a delete removes a link itself).
        let (access, disposition, options): (u32, u32, u32) = match how {
            Open::Read { pipe: true } => (0x0012_019F, 1, 0),
            Open::Read { pipe: false } => (0x0012_0089, 1, 0),
            Open::NewFile => (0x0013_019F, 2, 0x40),
            Open::NewDir => (0x0012_0089, 2, 1),
            Open::Rename => (0x0011_0080, 1, 0),
            Open::Delete => (0x0011_0080, 1, 0x0020_1040),
            Open::DeleteDir => (0x0011_0080, 1, 0x0020_1001),
        };
        let mut b = vec![57, 0, 0, 0];
        b.extend_from_slice(&2u32.to_le_bytes()); // impersonation
        b.extend_from_slice(&[0; 16]); // create flags, reserved
        b.extend_from_slice(&access.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // attributes
        b.extend_from_slice(&7u32.to_le_bytes()); // share read | write | delete
        b.extend_from_slice(&disposition.to_le_bytes());
        b.extend_from_slice(&options.to_le_bytes());
        b.extend_from_slice(&(64u16 + 56).to_le_bytes());
        b.extend_from_slice(&(name16.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0; 8]); // no create contexts
        b.extend_from_slice(&name16);
        if name16.is_empty() {
            b.push(0);
        }
        let r = self.send(Req {
            cmd: CREATE,
            tree,
            body: b,
            charge: 1,
        })?;
        if r.status != 0 {
            return Err(status_error(
                r.status,
                if name.is_empty() { "the share" } else { name },
            ));
        }
        let e = Entry {
            name: String::new(),
            dir: r.u32(56) & 0x10 != 0,
            size: r.u64(48),
            mtime: filetime_to_unix(r.u64(24)),
            link: r.u32(56) & REPARSE_POINT != 0,
        };
        Ok((r.bytes(64 + 64, 16)?.try_into().unwrap(), e))
    }

    /// One batch of FileDirectoryInformation; `None` once there are no more.
    pub(crate) fn query_dir(
        &mut self,
        tree: u32,
        id: &[u8; 16],
        restart: bool,
    ) -> io::Result<Option<Vec<Entry>>> {
        let pattern = ntlm::utf16("*");
        let mut b = vec![33, 0, 1, u8::from(restart)];
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(&(64u16 + 32).to_le_bytes());
        b.extend_from_slice(&(pattern.len() as u16).to_le_bytes());
        b.extend_from_slice(&65536u32.to_le_bytes());
        b.extend_from_slice(&pattern);
        let r = self.send(Req {
            cmd: QUERY_DIRECTORY,
            tree,
            body: b,
            charge: 1,
        })?;
        if r.status == NO_MORE_FILES {
            return Ok(None);
        }
        if r.status != 0 {
            return Err(status_error(r.status, "list a folder"));
        }
        let mut buf = r.data(r.u16(2), r.u32(4) as usize, 8)?;
        let mut out = Vec::new();
        loop {
            if buf.len() < 64 {
                return Err(bad("a short folder entry"));
            }
            let u32at = |at: usize| u32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
            let u64at = |at: usize| u64::from_le_bytes(buf[at..at + 8].try_into().unwrap());
            let (next, name_len) = (u32at(0) as usize, u32at(60) as usize);
            if name_len % 2 != 0 {
                return Err(bad("a folder entry with an odd name length"));
            }
            let name = buf
                .get(64..64 + name_len)
                .ok_or_else(|| bad("a short folder entry"))?;
            let name: Vec<u16> = name
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            out.push(Entry {
                name: String::from_utf16(&name).map_err(|_| bad("a name that isn't UTF-16"))?,
                dir: u32at(56) & 0x10 != 0,
                size: u64at(40),
                mtime: filetime_to_unix(u64at(24)),
                link: u32at(56) & REPARSE_POINT != 0,
            });
            if next == 0 {
                break;
            }
            // Past its own entry and within the batch, or the rest of the batch would be lost.
            if next < 64 + name_len || next > buf.len() {
                return Err(bad("a folder entry pointing outside its batch"));
            }
            buf = &buf[next..];
        }
        Ok(Some(out))
    }

    pub(crate) fn read(
        &mut self,
        tree: u32,
        id: &[u8; 16],
        offset: u64,
        buf: &mut [u8],
    ) -> io::Result<usize> {
        // Every credit covers 64 KiB; ask for no more than the credits we hold.
        let budget = (self.credits.max(1) as usize) * 65536;
        let len = buf
            .len()
            .min(self.max_read)
            .min(if self.large_mtu { budget } else { 65536 });
        let mut b = vec![49, 0, 0x50, 0];
        b.extend_from_slice(&(len as u32).to_le_bytes());
        b.extend_from_slice(&offset.to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(&[0; 16]); // minimum, channel, remaining, channel info
        b.push(0);
        let charge = len.div_ceil(65536) as u16;
        let r = self.send(Req {
            cmd: READ,
            tree,
            body: b,
            charge,
        })?;
        if r.status == END_OF_FILE {
            return Ok(0);
        }
        if r.status != 0 {
            return Err(status_error(r.status, "read"));
        }
        let offset = *r.msg.get(64 + 2).ok_or_else(|| bad("a short READ reply"))?;
        let data = r.data(offset as usize, r.u32(4) as usize, 16)?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    pub(crate) fn write(
        &mut self,
        tree: u32,
        id: &[u8; 16],
        offset: u64,
        bytes: &[u8],
    ) -> io::Result<usize> {
        let budget = (self.credits.max(1) as usize) * 65536;
        let len = bytes
            .len()
            .min(self.max_write)
            .min(if self.large_mtu { budget } else { 65536 });
        let mut b = vec![49, 0];
        b.extend_from_slice(&(64u16 + 48).to_le_bytes()); // data offset
        b.extend_from_slice(&(len as u32).to_le_bytes());
        b.extend_from_slice(&offset.to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(&[0; 16]); // channel, remaining, channel info, flags
        b.extend_from_slice(&bytes[..len]);
        let r = self.send(Req {
            cmd: WRITE,
            tree,
            body: b,
            charge: len.div_ceil(65536) as u16,
        })?;
        if r.status != 0 {
            return Err(status_error(r.status, "write"));
        }
        written(r.u32(4) as usize, len)
    }

    pub(crate) fn flush(&mut self, tree: u32, id: &[u8; 16]) -> io::Result<()> {
        let mut b = vec![24, 0, 0, 0, 0, 0, 0, 0];
        b.extend_from_slice(id);
        let r = self.send(Req {
            cmd: FLUSH,
            tree,
            body: b,
            charge: 1,
        })?;
        match r.status {
            0 => Ok(()),
            s => Err(status_error(s, "flush")),
        }
    }

    /// `to` a `\`-separated share path; `replace` is FileRenameInformation's ReplaceIfExists.
    pub(crate) fn rename(
        &mut self,
        tree: u32,
        id: &[u8; 16],
        to: &str,
        replace: bool,
    ) -> io::Result<()> {
        let name = ntlm::utf16(to);
        let mut info = vec![0u8; 16]; // ReplaceIfExists, reserved, RootDirectory
        info[0] = u8::from(replace);
        info.extend_from_slice(&(name.len() as u32).to_le_bytes());
        info.extend_from_slice(&name);
        info.resize(info.len().max(24), 0); // the record is at least 24 bytes (MS-FSCC)
        let r = self.set_info(tree, id, 10, &info)?;
        match r {
            0 => Ok(()),
            s => Err(status_error(s, to)),
        }
    }

    fn set_info(&mut self, tree: u32, id: &[u8; 16], class: u8, info: &[u8]) -> io::Result<u32> {
        let mut b = vec![33, 0, 1, class]; // SMB2_0_INFO_FILE
        b.extend_from_slice(&(info.len() as u32).to_le_bytes());
        b.extend_from_slice(&(64u16 + 32).to_le_bytes());
        b.extend_from_slice(&[0; 6]); // reserved, additional information
        b.extend_from_slice(id);
        b.extend_from_slice(info);
        Ok(self
            .send(Req {
                cmd: SET_INFO,
                tree,
                body: b,
                charge: 1,
            })?
            .status)
    }

    /// FileFsFullSizeInformation's caller-available bytes.
    pub(crate) fn free_space(&mut self, tree: u32, id: &[u8; 16]) -> io::Result<u64> {
        let mut b = vec![41, 0, 2, 7]; // SMB2_0_INFO_FILESYSTEM, FileFsFullSizeInformation
        b.extend_from_slice(&64u32.to_le_bytes()); // output buffer length
        b.extend_from_slice(&[0; 16]); // input offset, reserved, input length, additional, flags
        b.extend_from_slice(id);
        let r = self.send(Req {
            cmd: QUERY_INFO,
            tree,
            body: b,
            charge: 1,
        })?;
        if r.status != 0 {
            return Err(status_error(r.status, "free space"));
        }
        let info = r.data(r.u16(2), r.u32(4) as usize, 8)?;
        let field = |at: usize| -> io::Result<u64> {
            info.get(at..at + 8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                .ok_or_else(|| bad("a short free space reply"))
        };
        let units = field(8)?;
        let unit = info
            .get(24..32)
            .map(|b| {
                let spu = u32::from_le_bytes(b[..4].try_into().unwrap());
                let bps = u32::from_le_bytes(b[4..].try_into().unwrap());
                u64::from(spu) * u64::from(bps)
            })
            .ok_or_else(|| bad("a short free space reply"))?;
        Ok(units.saturating_mul(unit))
    }

    pub(crate) fn ioctl(
        &mut self,
        tree: u32,
        id: &[u8; 16],
        code: u32,
        input: &[u8],
    ) -> io::Result<Vec<u8>> {
        let mut b = vec![57, 0, 0, 0];
        b.extend_from_slice(&code.to_le_bytes());
        b.extend_from_slice(id);
        b.extend_from_slice(&(64u32 + 56).to_le_bytes());
        b.extend_from_slice(&(input.len() as u32).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // max input response
        b.extend_from_slice(&0u32.to_le_bytes()); // output offset
        b.extend_from_slice(&0u32.to_le_bytes()); // output count
        b.extend_from_slice(&65536u32.to_le_bytes()); // max output response
        b.extend_from_slice(&1u32.to_le_bytes()); // FSCTL
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(input);
        let r = self.send(Req {
            cmd: IOCTL,
            tree,
            body: b,
            charge: 1,
        })?;
        if r.status != 0 && r.status != BUFFER_OVERFLOW {
            return Err(status_error(r.status, "list shares"));
        }
        Ok(r.data(r.u32(32) as usize, r.u32(36) as usize, 48)?.to_vec())
    }
}

/// FILE_ATTRIBUTE_REPARSE_POINT (a server following links itself shows the target instead).
const REPARSE_POINT: u32 = 0x400;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_data_never_comes_from_the_header() {
        let mut msg = vec![0u8; 64 + 16];
        msg.extend_from_slice(b"data");
        let r = Resp {
            status: 0,
            head: msg[..64].to_vec(),
            msg,
        };
        assert_eq!(r.data(80, 4, 16).unwrap(), b"data");
        assert_eq!(r.data(0, 0, 16).unwrap(), b"");
        assert!(r.data(0, 4, 16).is_err()); // the header as file data
        assert!(r.data(79, 4, 16).is_err()); // inside the fixed part
        assert!(r.data(80, 5, 16).is_err()); // past the reply
    }

    #[test]
    fn write_counts_past_what_was_sent_are_refused() {
        assert_eq!(written(3, 4).unwrap(), 3);
        assert_eq!(written(4, 4).unwrap(), 4);
        assert!(written(0, 4).is_err());
        assert!(written(5, 4).is_err());
    }
}
