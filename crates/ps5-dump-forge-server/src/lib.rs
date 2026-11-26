//! `ps5-dump-forge serve`: the web UI over plain HTTP/1.1 (routes and limits: ps5/README.md).
//! No protections, by decision: it behaves like the Tauri commands over HTTP, for anyone
//! (and any web page in a browser) that can reach it on the network.

mod api;
// Public for the desktop app, which runs Connect without the HTTP server.
pub mod connect;
pub mod connect_fs;
pub mod connect_jobs;
#[cfg(target_env = "ps5")]
mod debug;
// A few helpers (`address`, `keepalive`, ...) only tests use yet.
#[allow(dead_code)]
mod discovery;
mod http;
mod mdns;
pub mod nas;
mod notice;
mod paths;
pub mod payload;
pub mod probe;
#[cfg(any(all(test, unix), target_env = "ps5"))]
mod self_copy;
mod shared;
mod table;
mod wsd;
// Unix: the owner file names the title folder by (st_dev, st_ino).
#[cfg(any(all(test, unix), target_env = "ps5"))]
mod tile;

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use std::path::PathBuf;

/// `ps5/entry.c`'s `FORGE_DIR`.
pub const FORGE_DIR: &str = "/data/dumpforge";

/// All a PS5 listing or paired device reaches (drives only while mounted; never `/mnt/shadowmnt`).
pub const PS5_ROOTS: [&str; 11] = [
    "/data",
    "/mnt/usb0",
    "/mnt/usb1",
    "/mnt/usb2",
    "/mnt/usb3",
    "/mnt/usb4",
    "/mnt/usb5",
    "/mnt/usb6",
    "/mnt/usb7",
    "/mnt/ext0",
    "/mnt/ext1",
];

pub const DEFAULT_PORT: u16 = 8095;

/// scripts/release-ps5.sh wants exactly one in the PS5 ELF; the self copy reads it on disk.
pub static VERSION_MARKER: &[u8] =
    concat!("ps5-dump-forge-version:", env!("CARGO_PKG_VERSION"), "\0").as_bytes();

const MAX_HANDLERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Ps5,
    Host,
}

pub struct Options {
    pub port: u16,
    /// On the PS5 also all that listings and paired devices reach.
    pub roots: Vec<PathBuf>,
    pub platform: Platform,
    pub notify: Arc<dyn Fn(&str) + Send + Sync>,
    /// For one whole request, however slowly it trickles in.
    pub deadline: Duration,
    /// For one whole response, however slowly the client reads.
    pub write_timeout: Duration,
    pub exit: Arc<dyn Fn(i32) + Send + Sync>,
    /// Set from a signal handler: quits as `POST /api/quit` does.
    pub interrupt: Option<&'static AtomicBool>,
    pub elfldr: SocketAddr,
    /// `None` runs no Connect service.
    pub connect_dir: Option<PathBuf>,
    /// `FORGE_FLINK_PORT` for local tests of two copies (0 = any).
    pub connect_port: u16,
    /// `None` keeps settings in memory only.
    pub settings_dir: Option<PathBuf>,
}

impl Options {
    pub fn new(
        port: u16,
        roots: Vec<PathBuf>,
        platform: Platform,
        notify: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        Self {
            port,
            roots,
            platform,
            notify: Arc::new(move |line: &str| notify(line)),
            deadline: Duration::from_secs(10),
            write_timeout: Duration::from_secs(30),
            exit: Arc::new(|code| std::process::exit(code)),
            interrupt: None,
            elfldr: SocketAddr::from((Ipv4Addr::LOCALHOST, ps5_dump_forge_updater::PORT)),
            connect_dir: None,
            connect_port: std::env::var("FORGE_FLINK_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(connect::PORT),
            settings_dir: None,
        }
    }
}

/// Serves until quit; another copy of any version on the port is taken over when idle.
pub fn serve(opts: Options) -> io::Result<()> {
    #[cfg(target_env = "ps5")]
    self_copy::start();
    let result = match bind(opts.port) {
        Ok(listener) => serve_on(listener, opts),
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => match running_forge(opts.port) {
            Some(peer) => take_over(opts, &peer),
            None => {
                let line = notice::port_in_use(notice::console_lang(), opts.port);
                (opts.notify)(&line);
                Err(io::Error::new(io::ErrorKind::AddrInUse, line))
            }
        },
        Err(e) => Err(e),
    };
    // Not serving: a newer payload loaded over a busy older one still saves itself.
    #[cfg(target_env = "ps5")]
    self_copy::wait();
    result
}

fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
}

/// Asks the copy on the port to quit if idle, then binds once the port is free (up to 30 s).
fn take_over(opts: Options, peer: &Peer) -> io::Result<()> {
    api::usable_roots(&opts)?;
    let (lang, new, port) = (notice::console_lang(), env!("CARGO_PKG_VERSION"), opts.port);
    let old = peer.version.as_str();
    let failed = |opts: &Options| {
        let line = notice::not_taken_over(lang, old, new);
        (opts.notify)(&line);
        Err(io::Error::new(io::ErrorKind::TimedOut, line))
    };
    // A copy without `handover` (v0.1.1 and older) ignores `if_idle` and cancels its jobs on
    // quit: asked only when its job list shows none unfinished.
    // ponytail: a job admitted between the two requests isn't listed yet and gets cancelled,
    // which needs someone acting in that instant.
    let body = match peer.handover {
        true => r#"{"if_idle":true}"#,
        false if idle_by_jobs(port) => "{}",
        false => {
            (opts.notify)(notice::update_busy(lang));
            return Ok(());
        }
    };
    let request = format!(
        "POST /api/quit HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    match local(port, &request, 64 * 1024)
        .ok()
        .and_then(|r| status_of(&r))
    {
        Some(200) => {}
        Some(409) => {
            (opts.notify)(notice::update_busy(lang));
            return Ok(());
        }
        _ => return failed(&opts),
    }
    let start = Instant::now();
    let listener = loop {
        match bind(port) {
            Ok(listener) => break listener,
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                if start.elapsed() >= Duration::from_secs(30) {
                    return failed(&opts);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e),
        }
    };
    // Saving again after the older copy exited makes the newest version's the last.
    #[cfg(target_env = "ps5")]
    {
        self_copy::wait();
        self_copy::start();
    }
    serve_on(listener, opts)
}

fn self_copy_status() -> String {
    #[cfg(target_env = "ps5")]
    return self_copy::status();
    #[cfg(not(target_env = "ps5"))]
    "none".into()
}

/// Never returns `Ok`.
pub fn serve_on(listener: TcpListener, opts: Options) -> io::Result<()> {
    let local = listener.local_addr()?;
    let interrupt = opts.interrupt;
    let server = Arc::new(api::Server::new(opts, local)?);
    server.wire_install();
    (server.opts.notify)(&started(&server.url, cfg!(target_env = "ps5")));
    #[cfg(target_env = "ps5")]
    eprintln!("ps5-dump-forge serve: listening on {}", server.url);
    #[cfg(target_env = "ps5")]
    tile::install(local.port());
    if let Some(flag) = interrupt {
        let server = server.clone();
        std::thread::Builder::new()
            .name("forge-signal".into())
            .spawn(move || {
                // ponytail: the signal handler only sets a flag; this polls it.
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                if server.stop() {
                    server.finish();
                }
            })?;
    }
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            // Out of descriptors, say: back off.
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        if active.load(Ordering::SeqCst) >= MAX_HANDLERS {
            http::busy(stream);
            continue;
        }
        ps5_dump_forge_flink::tune_buffers(&stream);
        active.fetch_add(1, Ordering::SeqCst);
        let slot = Slot(active.clone());
        let server = server.clone();
        let _ = std::thread::Builder::new()
            .name("forge-http".into())
            .spawn(move || {
                let _slot = slot;
                server.handle(stream);
            });
    }
    Ok(())
}

fn started(url: &str, ps5: bool) -> String {
    match ps5 {
        true => notice::started(notice::console_lang()).into(),
        false => format!("Dump Forge: open {url}"),
    }
}

struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The address the default route leaves from (a UDP `connect` sends no packet).
pub(crate) fn local_ip() -> Option<IpAddr> {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| s.connect((Ipv4Addr::new(1, 1, 1, 1), 80)).map(|()| s))
        .and_then(|s| s.local_addr())
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_unspecified())
}

/// A wildcard listener's address is the default route's.
// ponytail: `sceNetCtlGetInfo` is the hardware-tested alternative, if this proves unreliable.
fn console_url(local: SocketAddr, platform: Platform) -> String {
    let ip = if local.ip().is_unspecified() {
        local_ip()
    } else {
        Some(local.ip())
    };
    let host = match ip {
        Some(IpAddr::V4(ip)) => ip.to_string(),
        Some(IpAddr::V6(ip)) => format!("[{ip}]"),
        None if platform == Platform::Ps5 => "this PS5's IP address".into(),
        None => "this computer's IP address".into(),
    };
    format!("http://{host}:{}", local.port())
}

struct Peer {
    version: String,
    /// v0.1.1 and older lack it: they ignore `if_idle` and cancel their jobs.
    handover: bool,
}

fn running_forge(port: u16) -> Option<Peer> {
    let request =
        format!("GET /api/session HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let reply = local(port, &request, 64 * 1024).ok()?;
    let v: serde_json::Value = String::from_utf8_lossy(&reply)
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str(body).ok())?;
    (v["app"] == "ps5-dump-forge").then(|| Peer {
        version: v["version"].as_str().unwrap_or_default().to_string(),
        handover: v["handover"] == true,
    })
}

fn idle_by_jobs(port: u16) -> bool {
    let request =
        format!("GET /api/jobs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let Ok(reply) = local(port, &request, 16 << 20) else {
        return false;
    };
    String::from_utf8_lossy(&reply)
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|v| {
            v["jobs"]
                .as_array()
                .map(|jobs| jobs.iter().all(|j| !j["done"].is_null()))
        })
        .unwrap_or(false)
}

/// Within 3 s overall.
fn local(port: u16, request: &str, max: usize) -> io::Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, left(deadline)?)?;
    ask(stream, request, deadline, max)
}

fn left(deadline: Instant) -> io::Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    (!left.is_zero())
        .then_some(left)
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
}

/// All by `deadline`: a peer that trickles bytes can't stall the caller.
fn ask(mut stream: TcpStream, request: &str, deadline: Instant, max: usize) -> io::Result<Vec<u8>> {
    stream.set_write_timeout(Some(left(deadline)?))?;
    stream.write_all(request.as_bytes())?;
    let (mut reply, mut buf) = (Vec::new(), [0u8; 4096]);
    while reply.len() < max {
        stream.set_read_timeout(Some(left(deadline)?))?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => reply.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(reply)
}

pub(crate) fn status_of(reply: &[u8]) -> Option<u16> {
    let line = reply.split(|&b| b == b'\r').next()?;
    let mut parts = std::str::from_utf8(line).ok()?.split(' ');
    parts.next()?.starts_with("HTTP/1.").then_some(())?;
    parts.next()?.parse().ok()
}

/// `a` against `b`: `x.y.z` numerically, then a release above its pre-releases
/// (`0.0.1` > `0.0.1-pre3`), and pre-releases by their digit runs as numbers (`pre10` > `pre3`).
pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    /// Digits `(false, n, "")` or text `(true, 0, text)`: numbers sort below text.
    type Run<'a> = (bool, u64, &'a str);
    fn runs(s: &str) -> Vec<Run<'_>> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(c) = rest.chars().next() {
            let digit = c.is_ascii_digit();
            let end = rest
                .find(|c: char| c.is_ascii_digit() != digit)
                .unwrap_or(rest.len());
            let (run, tail) = rest.split_at(end);
            out.push(match digit {
                true => (false, run.parse().unwrap_or(u64::MAX), ""),
                false => (true, 0, run),
            });
            rest = tail;
        }
        out
    }
    fn split(v: &str) -> (Vec<u64>, Option<Vec<Run<'_>>>) {
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(runs(pre))),
            None => (v, None),
        };
        (
            core.split('.').map(|n| n.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    let ((a_core, a_pre), (b_core, b_pre)) = (split(a), split(b));
    a_core.cmp(&b_core).then(match (a_pre, b_pre) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => a.cmp(&b),
    })
}

fn random_hex(n: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
