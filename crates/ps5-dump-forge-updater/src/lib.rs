//! The PS5's ELF loader (elfldr, port 9021) from here: a Dump Forge release's PS5 ELF downloaded
//! over HTTPS and streamed to it (`install`: the PS5 server's self-update, and the desktop app's
//! Install on PS5), and any ELF file streamed to it (`send`: the app's Send a payload). elfldr
//! can't follow GitHub's 302 to its asset host, so it is never handed a URL; a Forge ELF started
//! with no arguments runs `serve`.
// ponytail: a raw stream carries no arguments, so a pending developer args file
// (`/data/dumpforge/args`, ps5/entry.c) runs instead of `serve`; elfldr's `?uri=...&args=serve`
// would skip it, but that needs a URL elfldr can fetch.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// elfldr's port.
pub const PORT: u16 = 9021;
/// The largest ELF taken (a payload is a few MB, Dump Forge's about 16 MB).
const MAX_ELF: u64 = 64 << 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// The longest a send may take in all (64 MiB at under 1 MB/s).
const SEND_TIME: Duration = Duration::from_secs(120);
/// The longest a download may take in all, redirects included: the PS5 page waits a minute.
const DOWNLOAD_TIME: Duration = Duration::from_secs(50);
/// Where each release's assets are.
const RELEASES: &str = "https://github.com/quer3q/ps5-dump-forge/releases/download";
const MAGIC: &[u8; 4] = b"\x7fELF";

/// `X.Y.Z`, each part 1 to 9 digits.
pub fn plain_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// A connection to elfldr at `addr`; `NotConnected` when nothing answers there.
fn dial(addr: SocketAddr) -> io::Result<TcpStream> {
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            format!("nothing answers at {addr}: start the PS5's ELF loader, then try again"),
        )
    })
}

/// Streams the ELF file at `path` to elfldr at `addr`, which runs it; the bytes sent. Refused:
/// a file that isn't an ELF or is over 64 MiB. elfldr gives no answer, so a sent payload that
/// fails to start isn't seen here.
pub fn send(addr: SocketAddr, path: &Path) -> io::Result<u64> {
    // Checked before the open: opening a FIFO would wait for a writer.
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a file", path.display()),
        ));
    }
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    let len = meta.len();
    let mut magic = [0u8; 4];
    if !meta.is_file() || len > MAX_ELF || file.read_exact(&mut magic).is_err() || magic != *MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not an ELF payload (an ELF file of at most 64 MiB)",
                path.display()
            ),
        ));
    }
    let what = path.display().to_string();
    stream(addr, (&magic[..]).chain(file), len, &what)
}

/// Streams `elf` to elfldr at `addr`, as `send` does a file.
pub fn send_bytes(addr: SocketAddr, elf: &[u8]) -> io::Result<u64> {
    if elf.len() as u64 > MAX_ELF || !elf.starts_with(MAGIC) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an ELF payload (an ELF of at most 64 MiB)",
        ));
    }
    stream(addr, elf, elf.len() as u64, "the ELF")
}

/// `len` bytes of `elf` to elfldr at `addr`; `what` names it when it ends early.
fn stream(addr: SocketAddr, mut elf: impl Read, len: u64, what: &str) -> io::Result<u64> {
    let mut stream = dial(addr)?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    // 1 MiB at a time within `SEND_TIME` in all; a file that ends early is an error, not a cut ELF.
    let deadline = Instant::now() + SEND_TIME;
    let (mut sent, mut buf) = (0u64, vec![0u8; 1 << 20]);
    while sent < len {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("sending to {addr} took too long"),
            ));
        }
        let n = elf.read(&mut buf[..(len - sent).min(1 << 20) as usize])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("{what} got shorter while it was sent"),
            ));
        }
        stream.write_all(&buf[..n])?;
        sent += n as u64;
    }
    stream.flush()?;
    stream.shutdown(Shutdown::Write)?;
    // Whatever elfldr writes back (a payload's output with some loaders) is left unread; a
    // short wait lets it take the last bytes before the close.
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let _ = stream.read(&mut [0u8; 256]);
    Ok(sent)
}

/// A downloaded ELF: in memory, or saved over the spill file when memory couldn't hold it.
#[derive(Debug)]
pub enum Elf {
    Memory(Vec<u8>),
    File(PathBuf),
}

/// v`version`'s PS5 ELF from its GitHub release, redirects followed, within 50 s. Refused: a
/// version that isn't `X.Y.Z` (`InvalidInput`), a reply other than 200, one without a
/// Content-Length or over 64 MiB, a body cut short or not an ELF. Held in memory when that much
/// can be reserved; else, with a `spill` file (the PS5's self copy), that file is replaced by it
/// (under the self copy's lock, so a starting copy's save can't interleave) and named.
// ponytail: TLS authenticates GitHub; nothing checks a signature or the ELF's version marker
// (scripts/release-ps5.sh wants exactly one copy of the marker's text in the PS5 ELF).
pub fn download(version: &str, spill: Option<&Path>) -> io::Result<Elf> {
    fetch(RELEASES, version, spill, false)
}

/// `download` from `base`; `force_spill` skips memory (tests).
fn fetch(base: &str, version: &str, spill: Option<&Path>, force_spill: bool) -> io::Result<Elf> {
    if !plain_version(version) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{version:?} is not a release version (X.Y.Z)"),
        ));
    }
    let failed = |kind: io::ErrorKind, why: &dyn std::fmt::Display| {
        // `NotConnected` means elfldr doesn't answer (`dial`), never a download's socket.
        let kind = match kind {
            io::ErrorKind::NotConnected => io::ErrorKind::Other,
            kind => kind,
        };
        io::Error::new(kind, format!("couldn't download v{version}: {why}"))
    };
    let other = io::ErrorKind::Other;
    let url = format!("{base}/v{version}/ps5-dump-forge-{version}-ps5.elf");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIME))
        .http_status_as_error(false)
        .build()
        .into();
    let reply = agent.get(&url).call().map_err(|e| failed(other, &e))?;
    if reply.status() != 200 {
        return Err(failed(
            other,
            &format!("the server answered {}", reply.status()),
        ));
    }
    let len = reply
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .ok_or_else(|| failed(other, &"the reply has no Content-Length"))?;
    if len > MAX_ELF {
        return Err(failed(other, &format!("{len} bytes, over 64 MiB")));
    }
    let not_elf = || failed(io::ErrorKind::InvalidData, &"it is not an ELF");
    let mut body = reply.into_body().into_reader();
    let mut elf = Vec::new();
    if !force_spill && elf.try_reserve_exact(len as usize).is_ok() {
        copy_exact(&mut body, len, |chunk| {
            elf.extend_from_slice(chunk);
            Ok(())
        })
        .map_err(|e| failed(e.kind(), &e))?;
        return match elf.starts_with(MAGIC) {
            true => Ok(Elf::Memory(elf)),
            false => Err(not_elf()),
        };
    }
    let Some(target) = spill else {
        let why = format!("the ELF ({len} bytes) doesn't fit in memory");
        return Err(failed(io::ErrorKind::OutOfMemory, &why));
    };
    match save(&mut body, len, target) {
        Ok(true) => Ok(Elf::File(target.to_path_buf())),
        Ok(false) => Err(not_elf()),
        Err(e) => Err(failed(e.kind(), &e)),
    }
}

/// Exactly `len` bytes of `body` handed to `sink`, 64 KiB at a time; the first four bytes.
fn copy_exact(
    body: &mut impl Read,
    len: u64,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<[u8; 4]> {
    let (mut got, mut head, mut buf) = (0u64, [0u8; 4], vec![0u8; 64 << 10]);
    while got < len {
        let n = match body.read(&mut buf[..(len - got).min(64 << 10) as usize]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("the download ended after {got} of {len} bytes"),
                ));
            }
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if got < 4 {
            let k = (4 - got as usize).min(n);
            head[got as usize..got as usize + k].copy_from_slice(&buf[..k]);
        }
        sink(&buf[..n])?;
        got += n as u64;
    }
    if body.read(&mut [0u8; 1])? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the download is longer than its {len} bytes"),
        ));
    }
    Ok(head)
}

/// `target` replaced by exactly `len` bytes of `body` as the self copy saves it: a temporary
/// file beside it (`.<name>.<pid>.tmp`), synced, renamed over it, the folder synced, under its
/// lock (`.<name>.lock`, flock). `false`, `target` untouched: the bytes aren't an ELF.
fn save(body: &mut impl Read, len: u64, target: &Path) -> io::Result<bool> {
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("elf");
    let lock = no_follow(OpenOptions::new().create(true).truncate(false).write(true))
        .open(dir.join(format!(".{name}.lock")))?;
    lock.lock()?;
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    // Only a dead process with our pid can have left this one.
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut file = no_follow(OpenOptions::new().write(true).create_new(true)).open(&tmp)?;
        if copy_exact(body, len, |chunk| file.write_all(chunk))? != *MAGIC {
            return Ok(false);
        }
        file.sync_all()?;
        fs::rename(&tmp, target)?;
        sync_dir(dir).map(|()| true)
    })();
    if !matches!(result, Ok(true)) {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn no_follow(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(options, libc::O_NOFOLLOW);
    options
}

/// A folder synced (a rename in it made durable); Windows has no folder handle to sync.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// v`version`'s PS5 ELF (`download`) streamed to elfldr at `addr` (`send_bytes`, or `send` of the
/// spilled file); the bytes sent. elfldr is probed first (`NotConnected` within 3 s rather than
/// after a download), then the whole download comes before the send: elfldr drops a connection
/// that sends nothing for 15 s.
pub fn install(addr: SocketAddr, version: &str, spill: Option<&Path>) -> io::Result<u64> {
    install_from(RELEASES, addr, version, spill)
}

fn install_from(
    base: &str,
    addr: SocketAddr,
    version: &str,
    spill: Option<&Path>,
) -> io::Result<u64> {
    if !plain_version(version) {
        return fetch(base, version, spill, false).map(|_| 0);
    }
    // Connect and close, as `GET /api/elfldr` does (elfldr logs an unknown payload, nothing more).
    drop(dial(addr)?);
    match fetch(base, version, spill, false)? {
        Elf::Memory(elf) => send_bytes(addr, &elf),
        Elf::File(path) => send(addr, &path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn elf_body() -> Vec<u8> {
        MAGIC
            .iter()
            .copied()
            .chain((0..300_000u32).map(|i| i as u8))
            .collect()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-updater-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An HTTP server on its own thread for the rest of the test process: each request answered
    /// with `reply(path)` and the connection closed; the URL base to fetch from, and the paths
    /// asked so far.
    fn serve(
        reply: impl Fn(&str) -> Vec<u8> + Send + 'static,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/releases", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = asked.clone();
        std::thread::spawn(move || {
            for s in listener.incoming() {
                let mut s = s.unwrap();
                let (mut req, mut buf) = (Vec::new(), [0u8; 1024]);
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&req);
                let path = text.split(' ').nth(1).unwrap_or("").to_string();
                // Recorded before the reply: the client may return as soon as it has it.
                seen.lock().unwrap().push(path.clone());
                let _ = s.write_all(&reply(&path));
            }
        });
        (base, asked)
    }

    fn ok(body: &[u8], len: usize) -> Vec<u8> {
        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
        [head.as_bytes(), body].concat()
    }

    /// The release paths: v1.2.3 a 302 to the ELF, v1.2.4 cut short, v1.2.5 not an ELF, any
    /// other 404.
    fn release(path: &str) -> Vec<u8> {
        let elf = elf_body();
        match path {
            "/releases/v1.2.3/ps5-dump-forge-1.2.3-ps5.elf" => b"HTTP/1.1 302 Found\r\n\
                Location: /assets/signed?x=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            "/assets/signed?x=1" => ok(&elf, elf.len()),
            "/releases/v1.2.4/ps5-dump-forge-1.2.4-ps5.elf" => ok(&elf[..10_000], elf.len()),
            "/releases/v1.2.5/ps5-dump-forge-1.2.5-ps5.elf" => ok(b"MZ not an elf", 13),
            _ => {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
            }
        }
    }

    #[test]
    fn an_elf_is_streamed_whole_and_anything_else_refused() {
        let dir = temp_dir("send");
        let elf = dir.join("x.elf");
        let body = elf_body();
        fs::write(&elf, &body).unwrap();
        let other = dir.join("x.bin");
        fs::write(&other, b"MZ not an elf").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let got = std::thread::spawn(move || {
            let mut all = Vec::new();
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut one = Vec::new();
                s.read_to_end(&mut one).unwrap();
                all.push(one);
            }
            all
        });
        assert_eq!(send(addr, &elf).unwrap(), body.len() as u64);
        assert_eq!(send_bytes(addr, &body).unwrap(), body.len() as u64);
        assert_eq!(got.join().unwrap(), [body.clone(), body]);

        let e = send(addr, &other).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        let e = send_bytes(addr, b"MZ not an elf").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert!(send(addr, &dir).is_err(), "a folder");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_release_is_downloaded_through_its_redirect_whole_or_refused() {
        let (base, asked) = serve(release);
        match fetch(&base, "1.2.3", None, false).unwrap() {
            Elf::Memory(elf) => assert_eq!(elf, elf_body()),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            *asked.lock().unwrap(),
            [
                "/releases/v1.2.3/ps5-dump-forge-1.2.3-ps5.elf",
                "/assets/signed?x=1"
            ]
        );
        assert!(fetch(&base, "1.2.4", None, false).is_err(), "cut short");
        let e = fetch(&base, "1.2.5", None, false).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData, "{e}");
        let e = fetch(&base, "1.2.6", None, false).unwrap_err();
        assert!(e.to_string().contains("answered 404"), "{e}");
        let e = fetch(&base, "1.2", None, false).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        let e = fetch(&base, "1.2.3", None, true).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::OutOfMemory, "no memory, no spill");
    }

    #[test]
    fn without_memory_the_spill_file_is_replaced_first() {
        let (base, _) = serve(release);
        let dir = temp_dir("spill");
        let target = dir.join("ps5-dump-forge.elf");
        fs::write(&target, b"\x7fELF old copy").unwrap();
        let tmp = dir.join(format!(".ps5-dump-forge.elf.{}.tmp", std::process::id()));
        fs::write(&tmp, b"stale").unwrap();
        match fetch(&base, "1.2.3", Some(&target), true).unwrap() {
            Elf::File(path) => assert_eq!(path, target),
            other => panic!("{other:?}"),
        }
        assert_eq!(fs::read(&target).unwrap(), elf_body());
        assert!(dir.join(".ps5-dump-forge.elf.lock").is_file());

        // Not an ELF, or cut short: the copy stays, no temporary file is left.
        for version in ["1.2.5", "1.2.4"] {
            assert!(fetch(&base, version, Some(&target), true).is_err());
            assert_eq!(fs::read(&target).unwrap(), elf_body());
        }
        assert!(!tmp.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn install_probes_elfldr_then_downloads_then_sends() {
        let (base, asked) = serve(release);
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let e = install(closed, "1.2", None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        // Nothing answers: refused before anything is downloaded.
        let e = install_from(&base, closed, "1.2.3", None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotConnected);
        assert!(asked.lock().unwrap().is_empty());

        // An elfldr taking `n` connections; what each sent.
        let elfldr = |n: usize| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let got = std::thread::spawn(move || {
                (0..n)
                    .map(|_| {
                        let (mut s, _) = listener.accept().unwrap();
                        let mut all = Vec::new();
                        s.read_to_end(&mut all).unwrap();
                        all
                    })
                    .collect::<Vec<_>>()
            });
            (addr, got)
        };
        // A download that fails: the probe only, and not reported as elfldr's.
        let (addr, got) = elfldr(1);
        let e = install_from(&base, addr, "1.2.6", None).unwrap_err();
        assert_ne!(e.kind(), io::ErrorKind::NotConnected);
        assert_eq!(got.join().unwrap(), [Vec::<u8>::new()]);
        // The probe (nothing sent), then the whole ELF on a second connection.
        let (addr, got) = elfldr(2);
        let sent = install_from(&base, addr, "1.2.3", None).unwrap();
        assert_eq!(sent, elf_body().len() as u64);
        assert_eq!(got.join().unwrap(), [Vec::new(), elf_body()]);
    }
}
