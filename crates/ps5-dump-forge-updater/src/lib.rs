//! The PS5's ELF loader (elfldr, port 9021): a release's PS5 ELF downloaded over HTTPS and
//! streamed to it (`install`), or any ELF file (`send`). elfldr can't follow GitHub's 302, so it is
//! never handed a URL.
// ponytail: a raw stream carries no arguments, so a pending developer args file (ps5/entry.c)
// runs instead of `serve`.

pub mod psn;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const PORT: u16 = 9021;
const MAX_ELF: u64 = 64 << 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const SEND_TIME: Duration = Duration::from_secs(120);
/// Redirects included: the PS5 page waits a minute.
const DOWNLOAD_TIME: Duration = Duration::from_secs(50);
const RELEASES: &str = "https://github.com/quer3q/ps5-dump-forge/releases/download";
const MAGIC: &[u8; 4] = b"\x7fELF";

pub fn plain_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `NotConnected` when nothing answers.
fn dial(addr: SocketAddr) -> io::Result<TcpStream> {
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            format!("nothing answers at {addr}: start the PS5's ELF loader, then try again"),
        )
    })
}

/// The bytes sent; elfldr gives no answer, so a payload that fails to start isn't seen.
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

pub fn send_bytes(addr: SocketAddr, elf: &[u8]) -> io::Result<u64> {
    if elf.len() as u64 > MAX_ELF || !elf.starts_with(MAGIC) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an ELF payload (an ELF of at most 64 MiB)",
        ));
    }
    stream(addr, elf, elf.len() as u64, "the ELF")
}

fn stream(addr: SocketAddr, mut elf: impl Read, len: u64, what: &str) -> io::Result<u64> {
    let mut stream = dial(addr)?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    // A file that ends early is an error, not a cut ELF.
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
    // A short wait lets elfldr take the last bytes before the close; a peer that already closed
    // makes macOS refuse the timeout (EINVAL), which only skips the wait.
    if stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .is_ok()
    {
        let _ = stream.read(&mut [0u8; 256]);
    }
    Ok(sent)
}

#[derive(Debug)]
enum Elf {
    Memory(Vec<u8>),
    File(PathBuf),
}

/// v`version`'s PS5 ELF from `base`: 200 with a Content-Length of at most 64 MiB, whole, an ELF.
/// In memory when that much can be reserved, else saved over `spill` (the PS5's self copy, under
/// its lock); `force_spill` skips memory (tests).
// ponytail: TLS authenticates GitHub; nothing checks a signature or the ELF's version marker.
fn fetch(base: &str, version: &str, spill: Option<&Path>, force_spill: bool) -> io::Result<Elf> {
    if !plain_version(version) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{version:?} is not a release version (X.Y.Z)"),
        ));
    }
    let failed = |kind: io::ErrorKind, why: &dyn std::fmt::Display| {
        // `NotConnected` is kept for elfldr not answering (`dial`).
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
        // ureq follows an HTTPS to HTTP redirect otherwise (tests serve plain HTTP).
        .https_only(base.starts_with("https://"))
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

/// Exactly `len` bytes of `body` handed to `sink`; returns the first four.
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

/// `target` replaced as the self copy saves it (temp + sync + rename + folder sync, under
/// `.<name>.lock`); `false`, `target` untouched, when the bytes aren't an ELF.
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

fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// v`version`'s PS5 ELF streamed to elfldr. elfldr is probed first (`NotConnected` within 3 s),
/// then the whole download comes before the send: elfldr drops a connection idle for 15 s.
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
    drop(dial(addr)?);
    match fetch(base, version, spill, false)? {
        Elf::Memory(elf) => send_bytes(addr, &elf),
        Elf::File(path) => send(addr, &path),
    }
}

const LATEST: &str = "https://api.github.com/repos/quer3q/ps5-dump-forge/releases/latest";
const MAX_JSON: u64 = 4 << 20;
const MAX_CLI: u64 = 64 << 20;
const CLI_TIME: Duration = Duration::from_secs(120);

/// This build's CLI release asset (`{v}` the version); `None` where no release carries one.
pub fn cli_asset() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("ps5-dump-forge-cli-{v}-macos-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("ps5-dump-forge-cli-{v}-macos-x86-64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("ps5-dump-forge-cli-{v}-windows-x86-64.exe")
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some("ps5-dump-forge-cli-{v}-windows-arm64.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("ps5-dump-forge-cli-{v}-linux-x86-64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("ps5-dump-forge-cli-{v}-linux-arm64")
    } else {
        None
    }
}

pub fn asset_url(version: &str, asset: &str) -> String {
    format!("{RELEASES}/v{version}/{}", asset.replace("{v}", version))
}

/// By number; a current prerelease (`-pre4`) is older than its release (update.ts's `isNewer`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let core = current.split(['-', '+']).next().unwrap_or("");
    let parse = |v: &str| -> Option<Vec<u64>> { v.split('.').map(|p| p.parse().ok()).collect() };
    if !plain_version(latest) || !plain_version(core) {
        return false;
    }
    match (parse(latest), parse(core)) {
        (Some(l), Some(c)) if l != c => l > c,
        (Some(_), Some(_)) => core.len() < current.len() && current[core.len()..].starts_with('-'),
        _ => false,
    }
}

/// `listed`: the release has `asset` uploaded and not empty.
#[derive(Debug, PartialEq)]
pub struct Latest {
    pub version: String,
    pub listed: bool,
}

pub fn latest(asset: &str) -> io::Result<Latest> {
    let body = get(LATEST, MAX_JSON).map_err(|e| context("couldn't ask GitHub", e))?;
    parse_latest(&body, asset)
}

fn parse_latest(body: &[u8], asset: &str) -> io::Result<Latest> {
    let bad =
        |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("GitHub's reply {why}"));
    let release: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| bad("is not JSON"))?;
    let version = release["tag_name"]
        .as_str()
        .and_then(|t| t.strip_prefix('v'))
        .filter(|v| plain_version(v))
        .ok_or_else(|| bad("has no release version (vX.Y.Z)"))?
        .to_string();
    let name = asset.replace("{v}", &version);
    let listed = release["assets"].as_array().is_some_and(|assets| {
        assets.iter().any(|a| {
            a["name"].as_str() == Some(&name)
                && a["state"].as_str() == Some("uploaded")
                && a["size"].as_u64().is_some_and(|s| s > 0)
        })
    });
    Ok(Latest { version, listed })
}

/// Checked against the release's SHA256SUMS.
pub fn download_cli(version: &str, asset: &str) -> io::Result<Vec<u8>> {
    download_cli_from(RELEASES, version, asset)
}

fn download_cli_from(base: &str, version: &str, asset: &str) -> io::Result<Vec<u8>> {
    if !plain_version(version) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{version:?} is not a release version (X.Y.Z)"),
        ));
    }
    let name = asset.replace("{v}", version);
    let why = format!("couldn't download {name}");
    let sums = get(&format!("{base}/v{version}/SHA256SUMS"), MAX_JSON)
        .map_err(|e| context(&format!("couldn't download v{version}'s SHA256SUMS"), e))?;
    let bytes = get(&format!("{base}/v{version}/{name}"), MAX_CLI).map_err(|e| context(&why, e))?;
    check_sum(&String::from_utf8_lossy(&sums), &name, &bytes)?;
    Ok(bytes)
}

fn check_sum(sums: &str, name: &str, bytes: &[u8]) -> io::Result<()> {
    let listed = sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        (parts.next()?.trim_start_matches('*') == name && parts.next().is_none()).then_some(hash)
    });
    let Some(listed) = listed else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("SHA256SUMS doesn't list {name}"),
        ));
    };
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let hex: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if !listed.eq_ignore_ascii_case(&hex) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{name} doesn't match its SHA256SUMS line (got {hex})"),
        ));
    }
    Ok(())
}

fn get(url: &str, max: u64) -> io::Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(CLI_TIME))
        .https_only(url.starts_with("https://"))
        .http_status_as_error(false)
        .build()
        .into();
    let other = |e: &dyn std::fmt::Display| io::Error::other(e.to_string());
    let reply = agent
        .get(url)
        .header("Accept", "application/vnd.github+json, */*")
        .call()
        .map_err(|e| other(&e))?;
    if reply.status() != 200 {
        return Err(other(&format!("the server answered {}", reply.status())));
    }
    reply
        .into_body()
        .with_config()
        .limit(max)
        .read_to_vec()
        .map_err(|e| other(&e))
}

fn context(what: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{what}: {e}"))
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

    #[test]
    fn versions_compare_by_number_and_a_prerelease_is_older() {
        assert!(is_newer("0.5.8", "0.5.7"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("0.5.7", "0.5.7-pre1"));
        assert!(!is_newer("0.5.7", "0.5.7"));
        assert!(!is_newer("0.5.7", "0.5.7+build"));
        assert!(!is_newer("0.5.6", "0.5.7"));
        assert!(!is_newer("0.5", "0.5.7"));
        assert!(!is_newer("0.5.8", "junk"));
        let body = br#"{"tag_name":"v0.5.8","assets":[
            {"name":"ps5-dump-forge-cli-0.5.8-x","state":"uploaded","size":9},
            {"name":"ps5-dump-forge-cli-0.5.8-y","state":"new","size":9}]}"#;
        let latest = |a| parse_latest(body, a).unwrap().listed;
        assert!(latest("ps5-dump-forge-cli-{v}-x"));
        assert!(!latest("ps5-dump-forge-cli-{v}-y"), "not uploaded");
        assert!(!latest("ps5-dump-forge-cli-{v}-z"));
    }

    #[test]
    fn a_cli_is_taken_only_when_its_sha256sums_line_matches() {
        let cli = b"new cli".to_vec();
        let hex: String = ring::digest::digest(&ring::digest::SHA256, &cli)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let sums = format!(
            "{hex}  ps5-dump-forge-cli-1.2.3-x\n{}  ps5-dump-forge-cli-1.2.4-x\n",
            "0".repeat(64)
        );
        let (base, _) = serve(move |path| match path {
            "/releases/v1.2.3/SHA256SUMS" | "/releases/v1.2.4/SHA256SUMS" => {
                ok(sums.as_bytes(), sums.len())
            }
            p if p.starts_with("/releases/v1.2.") => ok(b"new cli", 7),
            _ => {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
            }
        });
        let asset = "ps5-dump-forge-cli-{v}-x";
        assert_eq!(download_cli_from(&base, "1.2.3", asset).unwrap(), cli);
        let e = download_cli_from(&base, "1.2.4", asset).unwrap_err();
        assert!(e.to_string().contains("doesn't match"), "{e}");
        let e = download_cli_from(&base, "1.2.3", "ps5-dump-forge-cli-{v}-other").unwrap_err();
        assert!(e.to_string().contains("doesn't list"), "{e}");
        assert!(
            download_cli_from(&base, "1.2.5", asset).is_err(),
            "no SHA256SUMS"
        );
        assert!(check_sum(&format!("{hex} *a.exe"), "a.exe", &cli).is_ok());
    }
}
