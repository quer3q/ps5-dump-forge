//! `ps5-dump-forge serve`: the app's web UI over plain HTTP/1.1, for the PS5 payload (Tauri
//! can't run on the console) and for trying that UI on a computer. Std only, one request
//! per connection, and the page polls `GET /api/jobs` (no server push). The rules (limits,
//! routes) are in ps5/README.md, "Web server".
//!
//! No protections, by decision: no pairing, no Host/Origin checks, no path confinement. It
//! behaves like the Tauri commands over HTTP, for anyone (and any web page in a browser) that
//! can reach it on the network.

mod api;
// PS5 bring-up only (and only with FORGE_DEBUG_API set): never built for a desktop target.
#[cfg(target_env = "ps5")]
mod debug;
mod http;
mod paths;
// The payload's copy of its own ELF (`ps5/build.sh` stage 2).
#[cfg(any(all(test, unix), target_env = "ps5"))]
mod self_copy;
mod table;
// Unix: the owner file names the title folder by (st_dev, st_ino).
#[cfg(any(all(test, unix), target_env = "ps5"))]
mod tile;

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use std::path::PathBuf;

/// Where the PS5's file browser starts: `/data`, and each drive while a filesystem is mounted
/// on it (re-checked each time, for hot-plugged drives).
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
    /// The file browser's starting points (`list_dir` with a `null` path); they confine
    /// nothing.
    pub roots: Vec<PathBuf>,
    pub platform: Platform,
    /// Shows the user a line: the URL, "already running", "stopped".
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
        }
    }
}

/// Binds `0.0.0.0:<port>` and serves until quit. A port in use by another copy of this
/// server is "already running" (`Ok`); by anything else, an error.
pub fn serve(opts: Options) -> io::Result<()> {
    #[cfg(target_env = "ps5")]
    self_copy::start();
    let result = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, opts.port)) {
        Ok(listener) => serve_on(listener, opts),
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            let url = console_url(
                SocketAddr::from((Ipv4Addr::UNSPECIFIED, opts.port)),
                opts.platform,
            );
            if is_forge(opts.port) {
                (opts.notify)(&format!("PS5 Dump Forge: already running at {url}"));
                Ok(())
            } else {
                let line = format!(
                    "PS5 Dump Forge: port {} is used by another program",
                    opts.port
                );
                (opts.notify)(&line);
                Err(io::Error::new(io::ErrorKind::AddrInUse, line))
            }
        }
        Err(e) => Err(e),
    };
    // Not serving (another copy already is, or the bind failed): the self copy still finishes
    // before the process exits, so a newer payload loaded over an older one saves itself.
    #[cfg(target_env = "ps5")]
    self_copy::wait();
    result
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
    let or_tile = match cfg!(target_env = "ps5") {
        true => " or the PS5 Dump Forge tile",
        false => "",
    };
    (server.opts.notify)(&format!("PS5 Dump Forge: open {}{or_tile}", server.url));
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

/// One handler thread; frees its place even when the handler unwinds.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `http://<ip>:<port>`: the listener's own address, or for a wildcard listener the
/// address the default route leaves from (a UDP `connect` sends no packet).
// ponytail: `sceNetCtlGetInfo` is the hardware-tested alternative, if this proves unreliable.
fn console_url(local: SocketAddr, platform: Platform) -> String {
    let ip = if local.ip().is_unspecified() {
        UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
            .and_then(|s| s.connect((Ipv4Addr::new(1, 1, 1, 1), 80)).map(|()| s))
            .and_then(|s| s.local_addr())
            .ok()
            .map(|a| a.ip())
            .filter(|ip| !ip.is_unspecified())
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

/// Whether `127.0.0.1:<port>` answers `GET /api/session` as this app, within 3 s overall
/// (a peer that trickles bytes can't stall startup) and 64 KiB.
fn is_forge(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    let left = || {
        let left = deadline.saturating_duration_since(Instant::now());
        (!left.is_zero())
            .then_some(left)
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    };
    let ask = || -> io::Result<Vec<u8>> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let mut stream = TcpStream::connect_timeout(&addr, left()?)?;
        stream.set_write_timeout(Some(left()?))?;
        write!(
            stream,
            "GET /api/session HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )?;
        let (mut reply, mut buf) = (Vec::new(), [0u8; 4096]);
        while reply.len() < 64 * 1024 {
            stream.set_read_timeout(Some(left()?))?;
            match stream.read(&mut buf)? {
                0 => break,
                n => reply.extend_from_slice(&buf[..n]),
            }
        }
        Ok(reply)
    };
    let Ok(reply) = ask() else {
        return false;
    };
    String::from_utf8_lossy(&reply)
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(body).ok())
        .is_some_and(|v| v["app"] == "ps5-dump-forge")
}

/// `n` random bytes as hex; fails closed when the system RNG does.
fn random_hex(n: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
