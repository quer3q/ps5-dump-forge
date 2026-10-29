//! `ps5-dump-forge serve`: the app's web UI over plain HTTP/1.1, for the PS5 payload (Tauri
//! can't run on the console) and for trying that UI on a computer. Std only, one request
//! per connection, and the page polls `GET /api/jobs` (no server push). The rules (limits,
//! routes) are in ps5/README.md, "Web server".
//!
//! No protections, by decision: no pairing, no Host/Origin checks, no path confinement (but
//! on the PS5 listings reach only `/data` and the drives plugged in, by user request). It
//! behaves like the Tauri commands over HTTP, for anyone (and any web page in a browser) that
//! can reach it on the network.

mod api;
// Connect (Forge ↔ Forge over FLINK, discovery, saved NAS devices): public for the desktop app, which runs
// the same service without the HTTP server.
pub mod connect;
pub mod connect_fs;
// Jobs mirrored to the paired peers they use (read only there; the peer may cancel them).
pub mod connect_jobs;
// PS5 bring-up only (and only with FORGE_DEBUG_API set): never built for a desktop target.
#[cfg(target_env = "ps5")]
mod debug;
// Connect's discovery: Forge (UDP 8094) and NAS (mDNS; Windows PCs: wsd.rs).
// A few helpers (`address`, `keepalive`, ...) only tests use yet.
#[allow(dead_code)]
mod discovery;
mod http;
mod mdns;
// Connect's saved NAS devices.
pub mod nas;
mod notice;
mod paths;
pub mod payload;
// Connect's smart Pair by address: what answers at an address (Forge, SMB, NFS or FTP).
pub mod probe;
// The payload's copy of its own ELF (`ps5/build.sh` stage 2).
#[cfg(any(all(test, unix), target_env = "ps5"))]
mod self_copy;
mod table;
// Connect's Discover: Windows PCs by WS-Discovery.
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

/// Forge's home folder on the PS5 (`ps5/entry.c`'s `FORGE_DIR`): the self copy, the tile's
/// owner record, Connect's data and the settings the console keeps.
pub const FORGE_DIR: &str = "/data/dumpforge";

/// Where the PS5's file browser starts: `/data`, and each drive while a filesystem is mounted
/// on it (re-checked each time, for hot-plugged drives). By user request they are all it
/// reaches: a listing, and a paired device, refuses any other path (`paths::Roots::confine`);
/// not `/mnt/shadowmnt` either.
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

/// `ps5-dump-forge-version:<version>\0`: scripts/release-ps5.sh wants exactly one in the PS5
/// ELF; the self copy reads the version of an ELF on disk from it. The CLI keeps it linked.
pub static VERSION_MARKER: &[u8] =
    concat!("ps5-dump-forge-version:", env!("CARGO_PKG_VERSION"), "\0").as_bytes();

const MAX_HANDLERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// The roots are [`PS5_ROOTS`]-like candidates, those that exist (`/mnt/...` drives: that
    /// are mounted), each served under its canonical path (dropped only when that is `/`).
    Ps5,
    /// The roots are folders named on the command line, pinned at startup.
    Host,
}

pub struct Options {
    pub port: u16,
    /// The file browser's starting points (`list_dir` with a `null` path); on the PS5 also
    /// all that listings and paired devices reach.
    pub roots: Vec<PathBuf>,
    pub platform: Platform,
    /// Shows the user a line: the URL, "can't close", "stopped".
    pub notify: Arc<dyn Fn(&str) + Send + Sync>,
    /// The absolute time a client has to send one whole request (10 s).
    pub deadline: Duration,
    /// The absolute time a response has to reach a client, however slowly it reads (30 s).
    pub write_timeout: Duration,
    /// Called once quit has cancelled every job and waited for its cleanup:
    /// `std::process::exit` unless a test replaces it.
    pub exit: Arc<dyn Fn(i32) + Send + Sync>,
    /// Set from a signal handler: the server then quits as `POST /api/quit` does.
    pub interrupt: Option<&'static AtomicBool>,
    /// The loader `POST /api/self_update` asks to fetch and start a release (elfldr on
    /// `127.0.0.1:9021`; tests point it at a fake).
    pub elfldr: SocketAddr,
    /// Connect's data folder (node secret, peers); `None` runs no Connect service (tests). The
    /// CLI's `serve` passes [`connect::default_dir`].
    pub connect_dir: Option<PathBuf>,
    /// Connect's FLINK port: 9883, or `FORGE_FLINK_PORT` (local tests of two copies; 0 = any).
    pub connect_port: u16,
    /// Where settings are kept (the job notifications' `notifications.off` marker); `None`
    /// keeps them in memory only (tests). The CLI's `serve` passes [`FORGE_DIR`] on the PS5.
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

/// Binds `0.0.0.0:<port>` and serves until quit. A port in use by another copy of this
/// server, of any version, is taken over when it is idle ([`take_over`]); a port in use by
/// anything else is an error.
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
    // Not serving (another copy is busy, or the bind failed): the self copy still finishes
    // before the process exits, so a newer payload loaded over an older one saves itself.
    #[cfg(target_env = "ps5")]
    self_copy::wait();
    result
}

fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
}

/// Another copy (any version) holds the port: asks it to quit if idle (no job queued or running), then binds once its port is free (every 200 ms, up to 30 s) and serves. A busy
/// one keeps serving; the notice says how to update.
fn take_over(opts: Options, peer: &Peer) -> io::Result<()> {
    // A start that can't serve (no usable root) must not stop the one that does.
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
    // The older copy finished its own save before it exited; saving again after this one's
    // startup save makes the newest version's the last, whatever order the two ran in.
    #[cfg(target_env = "ps5")]
    {
        self_copy::wait();
        self_copy::start();
    }
    serve_on(listener, opts)
}

/// The `self_copy` field of `GET /api/session`.
fn self_copy_status() -> String {
    #[cfg(target_env = "ps5")]
    return self_copy::status();
    #[cfg(not(target_env = "ps5"))]
    "none".into()
}

/// Serves on a listener the caller bound (tests bind port 0). Never returns `Ok`.
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
            // Out of descriptors, say: back off rather than spin.
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
        // A thread that can't start drops the closure, its slot and the socket with it.
        let _ = std::thread::Builder::new()
            .name("forge-http".into())
            .spawn(move || {
                let _slot = slot;
                server.handle(stream);
            });
    }
    Ok(())
}

/// The notice once serving. The console's own browser opens the page from the icon; the
/// address is in the page's header (and the log) for anyone else on the network.
fn started(url: &str, ps5: bool) -> String {
    match ps5 {
        true => notice::started(notice::console_lang()).into(),
        false => format!("Dump Forge: open {url}"),
    }
}

/// One handler thread; frees its place even when the handler unwinds.
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

/// `http://<ip>:<port>`: the listener's own address, or for a wildcard listener the
/// address the default route leaves from (a UDP `connect` sends no packet).
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

/// The copy of this app answering `GET /api/session` on `127.0.0.1:<port>`.
struct Peer {
    version: String,
    /// It takes `quit {"if_idle": true}` (session `handover`; v0.1.1 and older lack it, ignore
    /// the body and cancel their jobs).
    handover: bool,
}

/// Whether `127.0.0.1:<port>` answers `GET /api/session` as this app, and as which version.
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

/// Whether `GET /api/jobs` on `127.0.0.1:<port>` lists only finished jobs (each has `done`).
fn idle_by_jobs(port: u16) -> bool {
    let request =
        format!("GET /api/jobs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    // Up to 32 finished jobs with 200 log lines each: well past 64 KiB.
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

/// One request to `127.0.0.1:<port>` and its reply (at most `max` bytes), within 3 s overall.
fn local(port: u16, request: &str, max: usize) -> io::Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, left(deadline)?)?;
    ask(stream, request, deadline, max)
}

/// The time to `deadline`; an error once it has passed.
fn left(deadline: Instant) -> io::Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    (!left.is_zero())
        .then_some(left)
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
}

/// Sends `request` and reads the reply until the peer closes, at most `max` bytes, all by
/// `deadline` (a peer that trickles bytes can't stall the caller).
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

/// The status code of a raw HTTP/1.x reply.
pub(crate) fn status_of(reply: &[u8]) -> Option<u16> {
    let line = reply.split(|&b| b == b'\r').next()?;
    let mut parts = std::str::from_utf8(line).ok()?.split(' ');
    parts.next()?.starts_with("HTTP/1.").then_some(())?;
    parts.next()?.parse().ok()
}

/// `a` against `b`: `x.y.z` numerically, then a release above its pre-releases
/// (`0.0.1` > `0.0.1-pre3`), and pre-releases by their digit runs as numbers (`pre10` > `pre3`).
pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    /// A run of digits `(false, n, "")` or of anything else `(true, 0, text)`: numbers sort
    /// below text.
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

/// `n` random bytes as hex; fails closed when the system RNG does.
fn random_hex(n: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
