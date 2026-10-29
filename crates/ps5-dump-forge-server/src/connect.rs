//! Connect: Forge ↔ Forge over FLINK (the `ps5_dump_forge_flink` crate). This part is the
//! device's identity: the data folder, the node secret, the paired peers and the device name,
//! and the service (`Connect`): the FLINK listener, pairing (asked of the other device's user:
//! `Approvals` here, `Outgoing` for this device's own request), saved devices and their online
//! state.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use ps5_dump_forge_core::{ConvertRequest, Event, JobId};

use ps5_dump_forge_flink::{
    Approver, Budget, Conn, Control, DEAD_AFTER, Frame, Handshake, MAX_BODY, MAX_CONTROL_BODY,
    MAX_RPC_BODY, Me, NOT_PAIRED, PairRequest, Receiver, Request, Response, Sender, Session,
    check_path, client_handshake, hex, kind2, refusal, server_handshake, server_join, unhex,
};

/// The PS5's Connect folder (Forge's home is `/data/dumpforge`).
pub const PS5_DIR: &str = "/data/dumpforge/connect";
const SECRET_FILE: &str = "node.key";
const PEERS_FILE: &str = "peers.json";

/// Where `serve` keeps Connect's files when nobody names a folder: the PS5's `PS5_DIR`, else
/// the OS's per-user data folder + `ps5-dump-forge/connect`. The desktop app passes its own
/// app data folder instead.
pub fn default_dir() -> Option<PathBuf> {
    if cfg!(target_env = "ps5") {
        return Some(PathBuf::from(PS5_DIR));
    }
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")?
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else {
        var("XDG_DATA_HOME").or_else(|| Some(var("HOME")?.join(".local/share")))?
    };
    Some(base.join("ps5-dump-forge").join("connect"))
}

/// This device's FLINK identity: a random 32-byte secret kept in the data folder, and its key
/// id (the first 16 bytes of BLAKE3(secret), 32 hex digits), which peers know it by.
#[derive(Clone)]
pub struct Identity {
    pub secret: [u8; 32],
    pub key_id: String,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

impl Identity {
    pub fn from_secret(secret: [u8; 32]) -> Identity {
        let key_id = hex(&blake3::hash(&secret).as_bytes()[..16]);
        Identity { secret, key_id }
    }

    /// Reads `node.key` from `dir`, or makes one (the folder too) when there is none. A file
    /// of the wrong length is an error, never replaced: the peers' pairings hang on it.
    pub fn load_or_create(dir: &Path) -> io::Result<Identity> {
        let path = dir.join(SECRET_FILE);
        match fs::read(&path) {
            Ok(b) => {
                let secret: [u8; 32] = b.as_slice().try_into().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{} is not a Forge node key", path.display()),
                    )
                })?;
                return Ok(Identity::from_secret(secret));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret)
            .map_err(|e| io::Error::other(format!("no randomness: {e}")))?;
        fs::create_dir_all(dir)?;
        // ponytail: two processes creating the key at once both rename; the last one wins
        // and the other's pairings made before its restart are lost.
        write_atomic(&path, &secret)?;
        Ok(Identity::from_secret(secret))
    }
}

/// Writes `bytes` to `path` through a temporary name beside it (synced, then renamed over the
/// old file; 0600 on unix), so a reader sees the old file or the new one, never half of one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!(".{name}.forge-{}.tmp", std::process::id()));
    let mut o = fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let result = (|| {
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent()
            && let Ok(d) = fs::File::open(parent)
        {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// A paired device, known by its key id (never by address: its `address` is the last one it
/// was seen at, updated whenever it is seen elsewhere).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    /// `ip:port` of its FLINK listener, as last seen.
    pub address: Option<String>,
    /// Unix seconds; 0 = never.
    pub last_seen: u64,
    /// The pair secret, 64 hex digits.
    pub secret: String,
}

impl Peer {
    pub fn pair_secret(&self) -> Option<[u8; 32]> {
        unhex::<32>(&self.secret)
    }
}

/// `peers.json`: one entry per key id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peers {
    pub peers: Vec<Peer>,
}

impl Peers {
    /// The saved peers; none when the file isn't there.
    pub fn load(dir: &Path) -> io::Result<Peers> {
        let path = dir.join(PEERS_FILE);
        match fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Peers::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        write_atomic(&dir.join(PEERS_FILE), &json)
    }

    pub fn get(&self, key_id: &str) -> Option<&Peer> {
        self.peers.iter().find(|p| p.key_id == key_id)
    }

    /// Adds `peer`, or replaces the entry with its key id (pairing again updates that one).
    /// Other entries are never touched, even one last seen at the same address.
    pub fn upsert(&mut self, peer: Peer) {
        match self.peers.iter_mut().find(|p| p.key_id == peer.key_id) {
            Some(p) => *p = peer,
            None => self.peers.push(peer),
        }
    }

    /// A known peer seen at `address` (a handshake or a discovery reply): its address and
    /// last-seen time updated. False when the key id isn't paired.
    pub fn seen(&mut self, key_id: &str, address: &str, now: u64) -> bool {
        let Some(p) = self.peers.iter_mut().find(|p| p.key_id == key_id) else {
            return false;
        };
        p.address = Some(address.to_string());
        p.last_seen = now;
        true
    }

    /// Removes the pairing on this side. False when there was none.
    pub fn forget(&mut self, key_id: &str) -> bool {
        let n = self.peers.len();
        self.peers.retain(|p| p.key_id != key_id);
        self.peers.len() != n
    }
}

/// What this device calls itself to peers: the PS5's console name (Settings → System →
/// Console Information), else the host name, else "PS5" / "PC"; at most 32 characters.
pub fn device_name() -> String {
    #[cfg(target_env = "ps5")]
    if let Some(name) = console_nickname() {
        return name.chars().take(32).collect();
    }
    #[cfg(unix)]
    {
        let mut bytes = [0u8; 256];
        // SAFETY: gethostname writes at most the supplied buffer; termination is found below.
        if unsafe { libc::gethostname(bytes.as_mut_ptr().cast(), bytes.len()) } == 0 {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            if let Ok(name) = std::str::from_utf8(&bytes[..end])
                && !name.trim().is_empty()
            {
                return name.chars().take(32).collect();
            }
        }
    }
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|n| !n.trim().is_empty())
        .map(|n| n.chars().take(32).collect())
        .unwrap_or_else(|| {
            if cfg!(target_env = "ps5") {
                "PS5"
            } else {
                "PC"
            }
            .into()
        })
}

/// The console's name is a registry value, not kern.hostname.
#[cfg(target_env = "ps5")]
fn console_nickname() -> Option<String> {
    struct Module(*mut libc::c_void);
    impl Drop for Module {
        fn drop(&mut self) {
            // SAFETY: this handle came from our successful dlopen and is closed once.
            unsafe { libc::dlclose(self.0) };
        }
    }
    // Loaded on demand: an unavailable registry service must not stop the payload booting.
    // SAFETY: constant NUL-terminated path; the SDK implements dlopen/dlsym for SPRX modules.
    let handle = unsafe {
        libc::dlopen(
            c"/system/common/lib/libSceRegMgr.sprx".as_ptr(),
            libc::RTLD_LAZY,
        )
    };
    if handle.is_null() {
        return None;
    }
    let module = Module(handle);
    // SAFETY: the module stays loaded through the lookup and call.
    let symbol = unsafe { libc::dlsym(module.0, c"sceRegMgrGetStr".as_ptr()) };
    if symbol.is_null() {
        return None;
    }
    type GetString = unsafe extern "C" fn(libc::c_int, *mut libc::c_char, usize) -> libc::c_int;
    // SAFETY: sceRegMgrGetStr's SDK ABI is (int key, char *out, size_t capacity) -> int.
    let get_string = unsafe { std::mem::transmute::<*mut libc::c_void, GetString>(symbol) };
    const SYSTEM_NICKNAME: libc::c_int = 0x0205_0000;
    let mut bytes = [0u8; 256];
    // SAFETY: the writable buffer outlives the call and its full capacity is supplied.
    let status = unsafe { get_string(SYSTEM_NICKNAME, bytes.as_mut_ptr().cast(), bytes.len()) };
    if status != 0 {
        return None;
    }
    let end = bytes.iter().position(|b| *b == 0)?;
    let name = std::str::from_utf8(&bytes[..end]).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

// ---- The service ------------------------------------------------------------------------------
//
// One FLINK listener (a thread per inbound conn, at most `MAX_CONNS`), one prober (every
// `PROBE_EVERY` an `info` RPC to each paired device on a fresh conn) and the shared state behind
// `Connect`. A device is online while a handshake with it, either way, succeeded within
// `DEAD_AFTER`. ponytail: a fresh conn per probe instead of a held control link with keepalive.

/// FLINK's TCP port.
pub const PORT: u16 = 9883;
/// Saved devices shown (the UI's 3×3 grid).
pub const MAX_SHOWN: usize = 9;
/// Inbound conns at once: 8 lanes per remote read or write, several peers, the probes.
const MAX_CONNS: usize = 64;
const PROBE_EVERY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// This device on the Connect card.
#[derive(Clone, Debug, Serialize)]
pub struct ThisDevice {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    pub port: u16,
    /// Why the FLINK listener isn't running (port in use, ...); `None` while it is.
    pub listen_error: Option<String>,
    /// The address the default route leaves from (`crate::local_ip`); `None` when unknown.
    pub ip: Option<String>,
}

/// A saved device as the UI lists it (no secret).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Device {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    pub address: Option<String>,
    pub last_seen: u64,
    pub online: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub device: ThisDevice,
    pub devices: Vec<Device>,
}

/// An inbound connection, attributed after authentication so Forget closes its control
/// session and every lane. The session id lets a lane find its authenticated peer.
struct Inbound {
    stream: TcpStream,
    peer: Option<(String, String)>,
}

pub(crate) struct Inner {
    dir: PathBuf,
    me: Me,
    listen_error: Option<String>,
    peers: Mutex<Peers>,
    /// `peers`' generation: bumped (under its lock) by each change a save must carry.
    peers_gen: AtomicU64,
    /// Held while peers.json is written (`save_peers`), never with `peers` waiting on it from
    /// a lane: saves run one at a time, outside the `peers` lock. Holds the generation last
    /// written, so an older snapshot never overwrites a newer one.
    peers_saved: Mutex<u64>,
    /// A save outside the lock failed: the next handshake saves again.
    peers_dirty: AtomicBool,
    /// Saved NAS devices (`nas.rs`) and the ids that answered the last TCP probe.
    nas: Mutex<crate::nas::NasList>,
    nas_online: Mutex<HashSet<String>>,
    /// Id → unix seconds of its last successful probe (memory only; 0 = not since start).
    nas_seen: Mutex<HashMap<String, u64>>,
    budget: Mutex<Budget>,
    /// Other devices' pairing requests, waiting for this device's user.
    approvals: Approvals,
    /// This device's own pairing request, while it runs and after (until the next one).
    outgoing: Mutex<Option<Outgoing>>,
    next_pair: AtomicU64,
    /// Key id → the last successful handshake with it.
    alive: Mutex<HashMap<String, Instant>>,
    /// Key id → the newest address a discovery reply gave for it.
    found: Mutex<HashMap<String, SocketAddr>>,
    /// Inbound conns, for the bound and for `shutdown`.
    conns: Mutex<HashMap<u64, Inbound>>,
    next_conn: AtomicU64,
    stop: AtomicBool,
    /// What paired peers may list, read and write (canonical; on the PS5 only `/data` and the
    /// drives plugged in within it); `None`: nothing.
    share: Option<crate::connect_fs::Share>,
    /// Lane nonces taken (`serve_lane`).
    lane_nonces: Mutex<HashSet<[u8; 16]>>,
    /// Files peers read in place (`connect_fs`).
    reads: crate::connect_fs::Reads,
    /// Files peers write here (`connect_fs`).
    writes: crate::connect_fs::Writes,
    /// Jobs mirrored both ways (`connect_jobs`).
    jobs: crate::connect_jobs::Mirror,
    /// How a peer's `install` starts a job here (the PS5's server sets it); `None`: refused.
    install: Mutex<Option<InstallHook>>,
}

/// Starts an install job of a package (a `flink://<peer>/...` path): the job's id, or why not.
pub type InstallHook = Arc<dyn Fn(PathBuf) -> Result<JobId, String> + Send + Sync>;

/// A peer's `install` with no hook set: this device isn't a PS5.
pub const CANT_INSTALL: &str = "this device can't install packages";

pub struct Connect {
    inner: Arc<Inner>,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    /// One per sleeping thread (prober, job pusher); dropped to stop them.
    wake: Mutex<Vec<mpsc::Sender<()>>>,
    /// Forge discovery (UDP 8094): the passive responder and the Forge part of a search.
    discovery: crate::discovery::Discovery,
    /// The NAS (mDNS) part of a search.
    lan: Arc<Lan>,
    lan_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A Connect error as the UI gets it: a refusal's code (`denied`, `timed_out`,
/// `too_many_attempts`, `not_paired`, ...), which it translates, else the error's text.
pub fn error_text(e: &io::Error) -> String {
    refusal(e).map_or_else(|| e.to_string(), str::to_string)
}

pub fn platform() -> &'static str {
    if cfg!(target_env = "ps5") {
        "ps5"
    } else {
        std::env::consts::OS
    }
}

impl Connect {
    /// Loads (or makes) the identity and peers in `dir` and starts the listener on `port` (0 =
    /// any, for tests) and the prober. A listener that can't bind is reported in `state()`, not
    /// an error; only the data folder's files are.
    pub fn start(dir: &Path, port: u16) -> io::Result<Connect> {
        // The PS5 (by user request): peers reach only `/data` and the drives plugged in.
        let reach = cfg!(target_env = "ps5").then(|| {
            let roots = crate::PS5_ROOTS.map(PathBuf::from);
            crate::paths::Roots::new(crate::Platform::Ps5, &roots).0
        });
        Connect::start_confined(dir, port, default_share_root(), reach)
    }

    /// `start` with this device's share root (tests use a temporary folder).
    pub fn start_rooted(dir: &Path, port: u16, share_root: Option<PathBuf>) -> io::Result<Connect> {
        Connect::start_confined(dir, port, share_root, None)
    }

    /// `start_rooted` with the PS5's reach within the share root (`None`: all of it).
    pub(crate) fn start_confined(
        dir: &Path,
        port: u16,
        share_root: Option<PathBuf>,
        reach: Option<crate::paths::Roots>,
    ) -> io::Result<Connect> {
        let identity = Identity::load_or_create(dir)?;
        let peers = Peers::load(dir)?;
        let nas = crate::nas::NasList::load(dir)?;
        nas.publish();
        let (listener, listen_error) = match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => (Some(l), None),
            Err(e) => (None, Some(format!("can't listen on port {port}: {e}"))),
        };
        let port = listener
            .as_ref()
            .and_then(|l| l.local_addr().ok())
            .map_or(port, |a| a.port());
        let inner = Arc::new(Inner {
            dir: dir.to_path_buf(),
            me: Me {
                key_id: identity.key_id.clone(),
                name: device_name(),
                platform: platform().into(),
                port,
            },
            listen_error,
            peers: Mutex::new(peers),
            peers_gen: AtomicU64::new(0),
            peers_saved: Mutex::new(0),
            peers_dirty: AtomicBool::new(false),
            nas: Mutex::new(nas),
            nas_online: Mutex::default(),
            nas_seen: Mutex::default(),
            budget: Mutex::new(Budget::default()),
            approvals: Approvals::default(),
            outgoing: Mutex::default(),
            next_pair: AtomicU64::new(0),
            alive: Mutex::default(),
            found: Mutex::default(),
            conns: Mutex::default(),
            next_conn: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            share: (share_root.and_then(|r| dunce::canonicalize(r).ok()))
                .map(|root| crate::connect_fs::Share::new(root, reach)),
            lane_nonces: Mutex::default(),
            reads: Default::default(),
            writes: Default::default(),
            jobs: Default::default(),
            install: Mutex::default(),
        });
        crate::connect_fs::register(&inner);
        let mut threads = Vec::new();
        if let Some(l) = listener {
            let inner = inner.clone();
            threads.push(std::thread::spawn(move || accept_loop(&inner, l)));
        }
        let (wake, sleep) = mpsc::channel();
        {
            let inner = inner.clone();
            // The first round runs at start, so saved devices show online without the wait.
            threads.push(std::thread::spawn(move || {
                inner.probe_all();
                while let Err(mpsc::RecvTimeoutError::Timeout) = sleep.recv_timeout(PROBE_EVERY) {
                    inner.probe_all();
                }
            }));
        }
        let (push_wake, push_sleep) = mpsc::channel();
        {
            let inner = inner.clone();
            let every = crate::connect_jobs::PUSH_EVERY;
            threads.push(std::thread::spawn(move || {
                // U8: while a paired device reads or writes files here, auto rest mode waits
                // as during this device's own jobs.
                let mut awake: Option<ps5_dump_forge_core::Awake> = None;
                while let Err(mpsc::RecvTimeoutError::Timeout) = push_sleep.recv_timeout(every) {
                    inner.push_jobs(false);
                    // Files kept for a lost writer to resume, past their time.
                    inner.writes.sweep();
                    let active = inner.peers_active();
                    if active != awake.is_some() {
                        awake = active.then(ps5_dump_forge_core::awake);
                    }
                }
            }));
        }
        let node = crate::discovery::Node {
            key_id: inner.me.key_id.clone(),
            name: inner.me.name.clone(),
            platform: inner.me.platform.clone(),
            port: inner.me.port,
        };
        let found = inner.clone();
        // Port 0 (tests): an ephemeral responder too, so services on one machine don't meet.
        let udp = if port == 0 { 0 } else { crate::discovery::PORT };
        let discovery = crate::discovery::Discovery::new(
            node,
            udp,
            Arc::new(move |key_id, addr| found.discovered(key_id, addr)),
        );
        let connect = Connect {
            inner,
            threads: Mutex::new(threads),
            wake: Mutex::new(vec![wake, push_wake]),
            discovery,
            lan: Arc::default(),
            lan_thread: Mutex::default(),
        };
        // One search at every start (by user request; the desktop app and the PS5 alike): its
        // finds wait under Nearby, and a saved device that moved to a new address is found: a
        // peer's one entry once a handshake there succeeds (`discovered`), a NAS's when the
        // search ends (`migrate_nas`). Tests (port 0) search at start only with a saved device.
        let saved = !lock(&connect.inner.peers).peers.is_empty()
            || !lock(&connect.inner.nas).nas.is_empty();
        if port != 0 || saved {
            connect.discover_start();
        }
        Ok(connect)
    }

    pub fn me(&self) -> &Me {
        &self.inner.me
    }

    /// Asks the device at `addr` (`ip` or `ip:port`) to pair; its user allows or denies it there
    /// within `flink::PAIR_WAIT`. Runs on a thread of its own: `outgoing()` follows it (the check
    /// number both screens show, then the outcome). Pairing again with a known device updates
    /// its one entry. A new request replaces the last (a running one is cancelled).
    pub fn pair(&self, addr: &str) -> io::Result<()> {
        let addr = parse_addr(addr)?;
        if !is_local(addr.ip()) {
            return Err(not_local(addr.ip()));
        }
        self.pair_cancel();
        let id = self.inner.next_pair.fetch_add(1, Ordering::SeqCst) + 1;
        *lock(&self.inner.outgoing) = Some(Outgoing::new(id, addr));
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("forge-pair".into())
            .spawn(move || {
                let result = inner.pair_now(id, addr);
                inner.pair_done(id, result);
            })?;
        Ok(())
    }

    /// What answers at `address` (Pair by address, made smart: `probe.rs`).
    pub fn probe(&self, address: &str) -> io::Result<crate::probe::Probe> {
        crate::probe::probe(address, &self.inner.me)
    }

    /// Cancels this device's pairing request while it waits (its connection is shut).
    pub fn pair_cancel(&self) {
        if let Some(o) = lock(&self.inner.outgoing).as_mut()
            && o.running()
        {
            o.state = "cancelled";
            if let Some(stream) = o.stream.take() {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    /// This device's last pairing request.
    pub fn outgoing(&self) -> Option<Outgoing> {
        lock(&self.inner.outgoing).clone()
    }

    /// This device's user answers another device's pairing request `id`; false when it no
    /// longer waits (answered, timed out, or that device gave up).
    pub fn approve(&self, id: &str, allow: bool) -> bool {
        self.inner.approvals.answer(id, allow)
    }

    /// Other devices' pairing requests waiting for this device's user.
    pub fn pending(&self) -> Vec<PairRequest> {
        self.inner.approvals.waiting()
    }

    /// The saved devices, at most `MAX_SHOWN`, in pairing order.
    pub fn devices(&self) -> Vec<Device> {
        let alive = lock(&self.inner.alive).clone();
        let peers = lock(&self.inner.peers);
        peers
            .peers
            .iter()
            .take(MAX_SHOWN)
            .map(|p| Device {
                key_id: p.key_id.clone(),
                name: p.name.clone(),
                platform: p.platform.clone(),
                address: p.address.clone(),
                last_seen: p.last_seen,
                online: alive
                    .get(&p.key_id)
                    .is_some_and(|t| t.elapsed() < DEAD_AFTER),
            })
            .collect()
    }

    pub fn device(&self, key_id: &str) -> Option<Device> {
        self.devices().into_iter().find(|d| d.key_id == key_id)
    }

    /// Removes the pairing on this side. False when there was none.
    pub fn forget(&self, key_id: &str) -> io::Result<bool> {
        let mut peers = lock(&self.inner.peers);
        // Saved first: a pairing still in peers.json would be back after a restart.
        let mut next = peers.clone();
        let secret = peers.get(key_id).and_then(Peer::pair_secret);
        if !next.forget(key_id) {
            return Ok(false);
        }
        // Under the `peers` lock (rare, the user's): a failed save leaves it paired.
        let version = self.inner.peers_gen.load(Ordering::SeqCst) + 1;
        self.inner.save_peers(&next, version)?;
        self.inner.peers_gen.store(version, Ordering::SeqCst);
        *peers = next;
        // Under the same lock as connection attribution: a handshake or Join that finishes
        // during Forget cannot register a connection after this sweep.
        for c in lock(&self.inner.conns).values() {
            if c.peer.as_ref().is_some_and(|(key, _)| key == key_id) {
                let _ = c.stream.shutdown(std::net::Shutdown::Both);
            }
        }
        drop(peers);
        // Its unfinished files can't be resumed any more.
        if let Some(secret) = secret {
            self.inner.writes.forget(secret);
        }
        self.inner.jobs.forget(key_id);
        lock(&self.inner.alive).remove(key_id);
        lock(&self.inner.found).remove(key_id);
        Ok(true)
    }

    /// The saved NAS devices, in saving order (the UI shows peers first, then these, at most
    /// `MAX_SHOWN` in all).
    pub fn nas_devices(&self) -> Vec<crate::nas::NasDevice> {
        let online = lock(&self.inner.nas_online).clone();
        let seen = lock(&self.inner.nas_seen).clone();
        lock(&self.inner.nas)
            .nas
            .iter()
            .map(|n| crate::nas::NasDevice {
                id: n.id.clone(),
                name: n.name.clone(),
                host: n.host.clone(),
                port: n.port,
                protocol: n.protocol,
                user: n.user.clone(),
                has_password: !n.password.is_empty(),
                online: online.contains(&n.id),
                last_online: seen.get(&n.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Saves a NAS (one entry per host + port, updated in place) and probes it; its id.
    pub fn nas_save(&self, req: &crate::nas::NasSave) -> io::Result<String> {
        let mut list = lock(&self.inner.nas);
        let mut next = list.clone();
        let id = next.upsert(req)?;
        let before = list
            .nas
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.host.clone(), n.port));
        next.save(&self.inner.dir)?;
        next.publish();
        *list = next;
        let n = list.nas.iter().find(|n| n.id == id).cloned();
        drop(list);
        // A new address starts unseen: the old one's state isn't this one's.
        if before != n.as_ref().map(|n| (n.host.clone(), n.port)) {
            lock(&self.inner.nas_online).remove(&id);
            lock(&self.inner.nas_seen).remove(&id);
        }
        if let Some(n) = n
            && crate::nas::reachable(&n.host, n.port, CONNECT_TIMEOUT)
        {
            lock(&self.inner.nas_online).insert(id.clone());
            lock(&self.inner.nas_seen).insert(id.clone(), unix_now());
        }
        Ok(id)
    }

    /// Removes a saved NAS. False when there was none.
    pub fn nas_forget(&self, id: &str) -> io::Result<bool> {
        let mut list = lock(&self.inner.nas);
        let mut next = list.clone();
        if !next.forget(id) {
            return Ok(false);
        }
        next.save(&self.inner.dir)?;
        next.publish();
        *list = next;
        lock(&self.inner.nas_online).remove(id);
        lock(&self.inner.nas_seen).remove(id);
        Ok(true)
    }

    /// The saved NAS devices with their credentials, for the SMB source (never for a reply).
    pub fn nas_saved(&self) -> Vec<crate::nas::SavedNas> {
        lock(&self.inner.nas).nas.clone()
    }

    pub fn state(&self) -> State {
        let i = &self.inner;
        State {
            device: ThisDevice {
                key_id: i.me.key_id.clone(),
                name: i.me.name.clone(),
                platform: i.me.platform.clone(),
                port: i.me.port,
                listen_error: i.listen_error.clone(),
                ip: crate::local_ip().map(|ip| ip.to_string()),
            },
            devices: self.devices(),
        }
    }

    /// A discovery reply named `key_id` at `addr`: tried after the last address when probing.
    pub fn discovered(&self, key_id: &str, addr: SocketAddr) {
        self.inner.discovered(key_id, addr);
    }

    /// One probe round now (the prober runs one every `PROBE_EVERY`).
    pub fn refresh(&self) {
        self.inner.probe_all();
    }

    /// An RPC to a paired device on a fresh control conn.
    pub fn call(&self, key_id: &str, method: &str, body: Value) -> io::Result<Value> {
        let (conn, _) = self.inner.open(key_id)?;
        rpc(conn, method, body)
    }

    /// A control session with a paired device (its read handles live as long as it does).
    pub fn session(&self, key_id: &str) -> io::Result<(Rpc, Handshake)> {
        self.inner.session(key_id)
    }

    /// What says a path is in use by one of this device's unfinished jobs: peers' writes
    /// there are refused.
    pub fn set_busy(&self, busy: crate::connect_fs::Busy) {
        self.inner.writes.set_busy(busy);
    }

    /// What a peer's replacing rename here must pass first (core's `Jobs::replace_check`):
    /// unset, only the checks Connect makes itself.
    pub fn set_replace_check(&self, check: crate::connect_fs::ReplaceCheck) {
        self.inner.writes.set_replace_check(check);
    }

    /// What a paired device's Delete and Move to here run (this device's own
    /// `Jobs::delete_path`/`move_path` with its protected paths), after the paths are confined
    /// to the share; unset, both are refused.
    pub fn set_delete_move(
        &self,
        delete: crate::connect_fs::DeleteHook,
        move_: crate::connect_fs::MoveHook,
    ) {
        self.inner.writes.set_delete_move(delete, move_);
    }

    /// What says a peer is writing a path here (a file it created and hasn't renamed), for
    /// core's `Jobs::set_external_busy`. Holds the service weakly (Connect holds the jobs' hook).
    pub fn writing_hook(&self) -> crate::connect_fs::Busy {
        let inner = Arc::downgrade(&self.inner);
        Arc::new(move |p| inner.upgrade().is_some_and(|i| i.writes.writing(p)))
    }

    /// Whether a paired device is using files here: one it is writing (created, not renamed,
    /// kept for a reattach included) or reading. Quitting would cut its transfer short.
    pub fn peers_active(&self) -> bool {
        self.inner.peers_active()
    }

    /// `peers_active` as a hook, for the app's quit check. Holds the service weakly.
    pub fn activity_hook(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        let inner = Arc::downgrade(&self.inner);
        Arc::new(move || inner.upgrade().is_some_and(|i| i.peers_active()))
    }

    /// This service as core's `remote::Remote` (`start` registers one already).
    pub fn remote(&self) -> Arc<crate::connect_fs::ConnectRemote> {
        crate::connect_fs::ConnectRemote::new(&self.inner)
    }

    /// `Jobs::start` returned `id` for `request`: mirrored to the peers it uses
    /// (`connect_jobs`).
    pub fn job_started(&self, id: JobId, request: &ConvertRequest) {
        self.inner.jobs.started(id, request);
    }

    /// Each of this device's job events, for the mirror.
    pub fn job_event(&self, event: &Event) {
        self.inner.jobs.event(event);
    }

    /// How a peer's cancel reaches this device's jobs.
    pub fn set_cancel(&self, cancel: crate::connect_jobs::CancelHook) {
        self.inner.jobs.set_cancel(cancel);
    }

    /// How a paired peer's `install {path}` starts an install job here (the PS5 only).
    pub fn set_install(&self, install: InstallHook) {
        *lock(&self.inner.install) = Some(install);
    }

    /// Asks the paired PS5 `key_id` to install the package at `path` on this device (read by
    /// it over FLINK, so the job is mirrored here); the PS5's job id. `path` must be inside
    /// this device's share root.
    pub fn install(&self, key_id: &str, path: &Path) -> Result<JobId, String> {
        let root = (self.inner.share_root()).ok_or("this device shares no files")?;
        let rel = (path.to_str())
            .and_then(|s| crate::connect_jobs::relative(root, s))
            .filter(|r| !r.is_empty())
            .ok_or_else(|| format!("{}: not on this device's shared disk", path.display()))?;
        let reply = (self.call(key_id, "install", serde_json::json!({ "path": rel })))
            .map_err(|e| error_text(&e))?;
        reply["job"]
            .as_u64()
            .ok_or_else(|| "the device answered no job".to_string())
    }

    /// This device's user cancels a paired peer's job shown here; false when it isn't listed.
    pub fn cancel_remote_job(&self, key_id: &str, job: JobId) -> bool {
        self.inner.jobs.cancel_remote(key_id, job)
    }

    /// One push of this device's jobs now (the pusher runs one every `PUSH_EVERY`).
    pub fn push_jobs(&self) {
        self.inner.push_jobs(true);
    }

    /// Stops the listener, the prober and every inbound conn.
    pub fn shutdown(&self) {
        let i = &self.inner;
        self.discovery.shutdown();
        self.lan.stop.store(true, Ordering::SeqCst);
        if let Some(t) = lock(&self.lan_thread).take() {
            let _ = t.join();
        }
        i.stop.store(true, Ordering::SeqCst);
        // Before the sessions end: their unfinished files go at once, not after a grace.
        i.writes.stop();
        lock(&self.wake).clear();
        // Wakes the blocked accept.
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], i.me.port)),
            CONNECT_TIMEOUT,
        );
        for s in lock(&i.conns).values() {
            let _ = s.stream.shutdown(std::net::Shutdown::Both);
        }
        for t in lock(&self.threads).drain(..) {
            let _ = t.join();
        }
    }
}

impl Inner {
    /// A search's finds move a saved NAS that went offline to its new IP (`NasList::migrate`);
    /// the next probe round shows it online there.
    fn migrate_nas(&self, found: &[crate::mdns::Nas]) {
        let snapshot = lock(&self.nas).clone();
        let mut online = lock(&self.nas_online).clone();
        // One missed probe isn't enough: an entry moves only when its saved address still
        // doesn't answer now (checked without the list locked).
        for id in snapshot.clone().migrate(found, &online) {
            if let Some(n) = snapshot.nas.iter().find(|n| n.id == id)
                && crate::nas::reachable(&n.host, n.port, CONNECT_TIMEOUT)
            {
                online.insert(id);
            }
        }
        let mut next = snapshot.clone();
        let moved = next.migrate(found, &online);
        if moved.is_empty() {
            return;
        }
        let mut list = lock(&self.nas);
        // Saved or forgotten meanwhile: left for the next search.
        if *list != snapshot || next.save(&self.dir).is_err() {
            return;
        }
        next.publish();
        *list = next;
        drop(list);
        // A new address starts unseen and offline, as a save to one does.
        for id in &moved {
            lock(&self.nas_seen).remove(id);
            lock(&self.nas_online).remove(id);
        }
    }

    fn discovered(&self, key_id: &str, addr: SocketAddr) {
        if lock(&self.peers).get(key_id).is_some() {
            lock(&self.found).insert(key_id.to_string(), addr);
        }
    }

    fn secret_of(&self, key_id: &str) -> Option<[u8; 32]> {
        lock(&self.peers).get(key_id).and_then(Peer::pair_secret)
    }

    /// The paired device's name, else its key id.
    pub(crate) fn peer_name(&self, key_id: &str) -> String {
        lock(&self.peers)
            .get(key_id)
            .map_or_else(|| key_id.to_string(), |p| p.name.clone())
    }

    pub(crate) fn paired_session(&self, key_id: &str, session: &Session) -> bool {
        !self.stop.load(Ordering::SeqCst) && self.secret_of(key_id) == Some(session.secret)
    }

    /// Attributes an authenticated inbound control connection or lane. A lane inherits its
    /// peer from the control session. A session from before Forget/re-pair is never revived.
    fn attribute(&self, id: u64, peer: Option<&str>, session: &Session) -> io::Result<String> {
        let peers = lock(&self.peers);
        let mut conns = lock(&self.conns);
        let sid = session.id();
        let key = peer.map(str::to_string).or_else(|| {
            conns.values().find_map(|c| {
                let (key, control) = c.peer.as_ref()?;
                (control == &sid).then(|| key.clone())
            })
        });
        let key = key
            .filter(|k| peers.get(k).and_then(Peer::pair_secret) == Some(session.secret))
            .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, NOT_PAIRED))?;
        let c = conns
            .get_mut(&id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "the connection ended"))?;
        c.peer = Some((key.clone(), sid));
        Ok(key)
    }

    fn peers_active(&self) -> bool {
        self.writes.active() || self.reads.active()
    }

    /// Writes `snapshot`, `peers` as of generation `version`, to peers.json, unless a newer one is
    /// written already: one save at a time, and never an older snapshot over a newer one.
    fn save_peers(&self, snapshot: &Peers, version: u64) -> io::Result<()> {
        let mut saved = lock(&self.peers_saved);
        if *saved >= version {
            return Ok(());
        }
        snapshot.save(&self.dir)?;
        *saved = version;
        Ok(())
    }

    /// A handshake succeeded, either way: a new pair secret is stored under the peer's key id
    /// (one entry per key id), a known peer's address, name and platform updated; saved when
    /// any of those changed or `last_seen` moved a minute. It is online from now. A pairing is
    /// saved before it counts, under the `peers` lock; any other change after the lock is
    /// released (every lane chunk and RPC takes it, and a sync can stall for seconds).
    fn record(&self, hs: &Handshake) -> io::Result<()> {
        let p = &hs.peer;
        let address = Some(SocketAddr::new(hs.remote.ip(), p.port).to_string());
        let now = unix_now();
        let mut peers = lock(&self.peers);
        let before = peers.clone();
        let save = match (
            hs.new_secret,
            peers.peers.iter_mut().find(|e| e.key_id == p.key_id),
        ) {
            (Some(s), _) => {
                peers.upsert(Peer {
                    key_id: p.key_id.clone(),
                    name: p.name.clone(),
                    platform: p.platform.clone(),
                    address,
                    last_seen: now,
                    secret: hex(&s),
                });
                true
            }
            (None, Some(e)) => {
                if e.pair_secret() != Some(hs.session.secret) {
                    return Err(io::Error::new(io::ErrorKind::PermissionDenied, NOT_PAIRED));
                }
                let changed = e.address != address || e.name != p.name || e.platform != p.platform;
                let stale = now.saturating_sub(e.last_seen) >= 60;
                (e.address, e.name, e.platform) = (address, p.name.clone(), p.platform.clone());
                e.last_seen = now;
                changed || stale
            }
            // Forgotten while the handshake ran.
            (None, None) => {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, NOT_PAIRED));
            }
        };
        if hs.new_secret.is_some() {
            let version = self.peers_gen.load(Ordering::SeqCst) + 1;
            if let Err(e) = self.save_peers(&peers, version) {
                // Memory keeps what peers.json says, or a restart would change who is paired.
                *peers = before;
                return Err(e);
            }
            self.peers_gen.store(version, Ordering::SeqCst);
            drop(peers);
        } else if save || self.peers_dirty.swap(false, Ordering::SeqCst) {
            let version = self.peers_gen.fetch_add(1, Ordering::SeqCst) + 1;
            let snapshot = peers.clone();
            drop(peers);
            // An address, name or time seen: a failure doesn't fail the handshake (who is
            // paired is unchanged); the next one saves again.
            if self.save_peers(&snapshot, version).is_err() {
                self.peers_dirty.store(true, Ordering::SeqCst);
            }
        } else {
            drop(peers);
        }
        lock(&self.alive).insert(p.key_id.clone(), Instant::now());
        Ok(())
    }

    /// A control conn to `addr` with a stored pair secret.
    fn dial(&self, addr: SocketAddr) -> io::Result<(Conn, Handshake)> {
        if !is_local(addr.ip()) {
            return Err(not_local(addr.ip()));
        }
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        let mut conn = Conn::new(stream, MAX_CONTROL_BODY, DEAD_AFTER)?;
        let hs = client_handshake(&mut conn, &self.me, &|k| self.secret_of(k), None)?;
        self.record(&hs)?;
        conn.set_max_body(MAX_RPC_BODY);
        Ok((conn, hs))
    }

    /// `Connect::pair`'s request `id` to `addr`: the handshake asking to pair (the check number
    /// goes to `outgoing` as soon as both sides have it), then one request, which the peer
    /// answers only once it has recorded the pairing.
    fn pair_now(&self, id: u64, addr: SocketAddr) -> io::Result<Device> {
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        {
            let mut out = lock(&self.outgoing);
            match out.as_mut() {
                Some(o) if o.id == id && o.running() => o.stream = Some(stream.try_clone()?),
                _ => return Err(io::Error::other("cancelled")),
            }
        }
        let mut conn = Conn::new(stream, MAX_CONTROL_BODY, DEAD_AFTER)?;
        let mut on_check = |check: &str| {
            if let Some(o) = lock(&self.outgoing)
                .as_mut()
                .filter(|o| o.id == id && o.running())
            {
                o.check = Some(check.to_string());
                o.state = "waiting";
            }
        };
        let hs = client_handshake(
            &mut conn,
            &self.me,
            &|k| self.secret_of(k),
            Some(&mut on_check),
        )?;
        self.record(&hs)?;
        conn.set_max_body(MAX_RPC_BODY);
        rpc(conn, "info", Value::Null)?;
        let peers = lock(&self.peers);
        let p = (peers.get(&hs.peer.key_id))
            .ok_or_else(|| io::Error::other("the pairing was not saved"))?;
        Ok(Device {
            key_id: p.key_id.clone(),
            name: p.name.clone(),
            platform: p.platform.clone(),
            address: p.address.clone(),
            last_seen: p.last_seen,
            online: true,
        })
    }

    /// Request `id`'s outcome into `outgoing` (unless a newer one replaced it, or it was
    /// cancelled and failed). A pairing that finished as Cancel came is saved on both sides, so
    /// it says `paired`, not `cancelled`.
    fn pair_done(&self, id: u64, result: io::Result<Device>) {
        let mut out = lock(&self.outgoing);
        let Some(o) = out.as_mut().filter(|o| o.id == id) else {
            return;
        };
        o.stream = None;
        if o.state == "cancelled" && result.is_err() {
            return;
        }
        match result {
            Ok(device) => {
                o.state = "paired";
                o.device = Some(device);
            }
            Err(e) => match refusal(&e) {
                Some(
                    code @ (ps5_dump_forge_flink::DENIED
                    | ps5_dump_forge_flink::TIMED_OUT
                    | ps5_dump_forge_flink::TOO_MANY_ATTEMPTS
                    | ps5_dump_forge_flink::BUSY
                    | ps5_dump_forge_flink::ALREADY_PAIRED),
                ) => o.state = code_state(code),
                _ => {
                    o.state = "failed";
                    o.error = Some(error_text(&e));
                }
            },
        }
    }

    /// Whether `key_id` is paired here and the service runs (`connect_fs` picks a service).
    pub(crate) fn serves(&self, key_id: &str) -> bool {
        !self.stop.load(Ordering::SeqCst) && lock(&self.peers).get(key_id).is_some()
    }

    /// A control session with a paired device; an error names the device.
    pub(crate) fn session(self: &Arc<Self>, key_id: &str) -> io::Result<(Rpc, Handshake)> {
        let (conn, hs) = self.open(key_id).map_err(|e| {
            match lock(&self.peers).get(key_id).map(|p| p.name.clone()) {
                Some(name) => io::Error::new(e.kind(), format!("{name} can't be reached: {e}")),
                None => e,
            }
        })?;
        let inner = Arc::downgrade(self);
        let key = key_id.to_string();
        let session = hs.session.clone();
        let mut rpc = Rpc::new(conn)?;
        rpc.allowed = Some(Box::new(move || {
            inner
                .upgrade()
                .is_some_and(|i| i.paired_session(&key, &session))
        }));
        Ok((rpc, hs))
    }

    /// A control conn to the paired device `key_id`: its last address, then the newest
    /// discovery address for it, then (saved on 9120, the port before 9883) its IP on `PORT`.
    /// Another device answering there is recorded as itself (its own entry, if paired) and the
    /// next address tried.
    fn open(&self, key_id: &str) -> io::Result<(Conn, Handshake)> {
        let last = lock(&self.peers)
            .get(key_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "that device isn't paired"))?
            .address
            .as_deref()
            .and_then(|a| a.parse::<SocketAddr>().ok());
        let found = lock(&self.found).get(key_id).copied();
        // Saved on the old port: a device updated since is found again where discovery can't
        // reach it (the handshake saves the new address). Only its refusal (`not_paired`, ...)
        // replaces the error, so a guess that finds nothing never hides the saved address's.
        let moved = last
            .filter(|a| a.port() == 9120)
            .map(|a| SocketAddr::new(a.ip(), PORT))
            .filter(|m| Some(*m) != found);
        let mut err = io::Error::new(io::ErrorKind::NotFound, "no address for that device");
        for addr in [last, found.filter(|f| Some(*f) != last), moved]
            .into_iter()
            .flatten()
        {
            match self.dial(addr) {
                Ok((conn, hs)) if hs.peer.key_id == key_id => return Ok((conn, hs)),
                Ok(_) => err = io::Error::other(format!("another device answers at {addr}")),
                Err(e) if Some(addr) != moved || refusal(&e).is_some() => err = e,
                Err(_) => {}
            }
        }
        Err(err)
    }

    /// An `info` RPC to every paired device at once. A device that answers `not_paired`
    /// (it forgot this one) is offline at once; others expire after `DEAD_AFTER`.
    fn probe_all(&self) {
        let keys: Vec<String> = lock(&self.peers)
            .peers
            .iter()
            .map(|p| p.key_id.clone())
            .collect();
        let nas: Vec<(String, String, u16)> = lock(&self.nas)
            .nas
            .iter()
            .map(|n| (n.id.clone(), n.host.clone(), n.port))
            .collect();
        std::thread::scope(|s| {
            for (id, host, port) in &nas {
                s.spawn(move || {
                    let up = !self.stop.load(Ordering::SeqCst)
                        && crate::nas::reachable(host, *port, CONNECT_TIMEOUT);
                    // An entry moved meanwhile (`migrate_nas`) isn't judged by its old address.
                    let same = |n: &crate::nas::SavedNas| {
                        n.id == *id && n.host == *host && n.port == *port
                    };
                    if !lock(&self.nas).nas.iter().any(same) {
                        return;
                    }
                    if up {
                        lock(&self.nas_seen).insert(id.clone(), unix_now());
                    }
                    let mut online = lock(&self.nas_online);
                    if up {
                        online.insert(id.clone());
                    } else {
                        online.remove(id);
                    }
                });
            }
            for k in &keys {
                s.spawn(move || {
                    if self.stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let r = self.open(k).and_then(|(c, _)| rpc(c, "info", Value::Null));
                    if r.as_ref().is_err_and(|e| refusal(e) == Some(NOT_PAIRED)) {
                        lock(&self.alive).remove(k);
                    }
                });
            }
        });
    }

    /// This device's jobs to the paired peers they use, each on its own thread, so a peer that
    /// can't be reached never holds back the others' pushes; a reply's cancels applied. A peer
    /// whose last push is still going is skipped this round. `wait`: until every push ends.
    fn push_jobs(self: &Arc<Self>, wait: bool) {
        let paired = |k: &str| lock(&self.peers).get(k).is_some();
        let root = self.share_root();
        let pushes = self.jobs.due(&self.me.key_id, root, &paired);
        let mut threads = Vec::new();
        for push in pushes {
            if !self.jobs.begin_push(&push.key_id) {
                continue;
            }
            let inner = self.clone();
            threads.push(std::thread::spawn(move || {
                if !inner.stop.load(Ordering::SeqCst) {
                    let r = (inner.open(&push.key_id))
                        .and_then(|(c, _)| rpc(c, "jobs", push.body.clone()));
                    if let Ok(reply) = r {
                        inner.jobs.replied(&push, &reply);
                    }
                }
                inner.jobs.end_push(&push.key_id);
            }));
        }
        if wait {
            threads.into_iter().for_each(|t| drop(t.join()));
        }
    }

    /// One inbound conn: a data lane when it opens with a `Join`, else a control conn: the
    /// handshake, then requests until the peer closes or goes silent for `DEAD_AFTER`.
    fn serve(&self, id: u64, stream: TcpStream) -> io::Result<()> {
        stream.set_read_timeout(Some(DEAD_AFTER))?;
        let lane = first_kind(&stream)? == kind2::JOIN;
        let mut conn = Conn::new(stream, MAX_CONTROL_BODY, DEAD_AFTER)?;
        if lane {
            return self.serve_lane(id, conn);
        }
        let hs = server_handshake(
            &mut conn,
            &self.me,
            &|k| self.secret_of(k),
            &self.approvals,
            &self.budget,
        )?;
        self.record(&hs)?;
        self.attribute(id, Some(&hs.peer.key_id), &hs.session)?;
        let _reads = self.reads.guard(&hs.session);
        let _writes = self.writes.guard(&hs.session);
        conn.set_max_body(MAX_RPC_BODY);
        let (tx, mut rx) = conn.split()?;
        let tx = Mutex::new(tx);
        loop {
            if let Control::Request(r) = rx.recv_control(&tx)? {
                // Pings while it works, so a long answer (a big folder's manifest) doesn't look
                // like a silent peer to the caller after `DEAD_AFTER`.
                // The answer comes back over a channel, so a quick one is sent at once (a
                // 50 ms poll here made every RPC, and so every small file, take 50 ms).
                let result = std::thread::scope(|s| {
                    let (done, finished) = mpsc::channel();
                    let (hs, r) = (&hs, &r);
                    let work = s.spawn(move || {
                        let _ = done.send(self.answer(hs, r));
                    });
                    let answer = loop {
                        match finished.recv_timeout(ps5_dump_forge_flink::PING_EVERY) {
                            Ok(answer) => break Some(answer),
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                let ping = Frame::new(kind2::PING, 0, Vec::new());
                                let _ = lock(&tx).send_frame(&ping);
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                        }
                    };
                    match (work.join(), answer) {
                        (Ok(()), Some(answer)) => answer,
                        _ => Err("the request failed".into()),
                    }
                });
                let answer = Response::new(r.id, result);
                lock(&tx).send_frame(&answer.frame()?)?;
            }
        }
    }

    /// A data lane: a peer's reads or writes (`connect_fs`), after its `Join`.
    fn serve_lane(&self, id: u64, mut conn: Conn) -> io::Result<()> {
        let (session, _, nonce) = server_join(&mut conn, &|id| self.session_of(id))?;
        let peer = self.attribute(id, None, &session)?;
        let allowed = || self.paired_session(&peer, &session);
        // A replayed Join would key the lane as before, so its recorded frames would pass again:
        // each lane nonce is taken once. ponytail: never pruned (16 bytes per lane).
        if !lock(&self.lane_nonces).insert(nonce) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "flink: a replayed lane",
            ));
        }
        conn.set_max_body(MAX_BODY);
        let sid = session.id();
        loop {
            let f = conn.recv_frame()?;
            match f.kind {
                kind2::PING => conn.send_frame(&Frame::new(kind2::PONG, 0, Vec::new()))?,
                kind2::REQUEST if crate::connect_fs::is_read(&f) => {
                    return self
                        .reads
                        .serve(&mut conn, &sid, self.share.as_ref(), f, &allowed);
                }
                kind2::REQUEST if crate::connect_fs::is_write(&f) => {
                    let root = self.share.as_ref();
                    return (self.writes).serve(
                        &mut conn,
                        &session,
                        root,
                        &self.me.name,
                        f,
                        &allowed,
                    );
                }
                k => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("flink: unexpected frame {k} on a lane"),
                    ));
                }
            }
        }
    }

    /// The control session a peer's open reads or writes were opened under, by its public id.
    fn session_of(&self, id: &str) -> Option<Session> {
        let session = self
            .reads
            .session_of(id)
            .or_else(|| self.writes.session_of(id))?;
        // Forget closes the sockets before their workers finish dropping the handles. A
        // Join during that cleanup must be refused before server_join sends JoinOk.
        let peers = lock(&self.peers);
        let paired = lock(&self.conns).values().any(|c| {
            c.peer.as_ref().is_some_and(|(key, sid)| {
                sid == id && peers.get(key).and_then(Peer::pair_secret) == Some(session.secret)
            })
        });
        paired.then_some(session)
    }

    /// The share root, canonical.
    fn share_root(&self) -> Option<&Path> {
        self.share.as_ref().map(|s| s.root.as_path())
    }

    /// The RPC methods this device answers.
    fn answer(&self, hs: &Handshake, r: &Request) -> Result<Value, String> {
        if !self.paired_session(&hs.peer.key_id, &hs.session) {
            return Err(NOT_PAIRED.into());
        }
        let root = self.share.as_ref();
        let (reads, writes, me) = (&self.reads, &self.writes, &self.me.name);
        if let Some(a) = crate::connect_fs::answer(root, reads, writes, me, hs, r) {
            return a;
        }
        match r.method.as_str() {
            "info" => Ok(serde_json::json!({
                "key_id": self.me.key_id,
                "name": self.me.name,
                "platform": self.me.platform,
                "port": self.me.port,
                "share_root": self.share_root().map(|p| p.to_string_lossy()),
            })),
            // The peer's jobs that use this device (`connect_jobs`); the ids to cancel.
            "jobs" => {
                let (me, root) = (&self.me.key_id, self.share_root());
                self.jobs.received(&hs.peer.key_id, &r.body, me, root)
            }
            // A package on the peer (`path` relative to its share root) installed here.
            "install" => {
                let path = (r.body["path"].as_str()).ok_or("install needs a \"path\" string")?;
                let hook = lock(&self.install).clone().ok_or(CANT_INSTALL)?;
                let job = hook(peer_package(&hs.peer.key_id, path)?)?;
                Ok(serde_json::json!({ "job": job }))
            }
            m => Err(format!("unknown method {m}")),
        }
    }
}

/// `flink://<key_id>/<path>` for a peer's package path: plain `/`-separated names, no `.`,
/// `..`, `\` or NUL (the peer confines it to its share root again when it is read).
fn peer_package(key_id: &str, path: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let bad = |p: &&str| *p == "." || *p == ".." || p.contains(['\\', '\0']);
    if parts.is_empty() || parts.iter().any(bad) {
        return Err(format!("install: {path:?} is not a file path"));
    }
    Ok(PathBuf::from(format!(
        "flink://{key_id}/{}",
        parts.join("/")
    )))
}

// ---- The share root ----------------------------------------------------------------------------
//
// A peer names paths relative to this device's share root (the disk's root); `connect_fs`
// reads and writes them.

/// What a paired peer may reach on this device (by user request, so a peer browses above the
/// home folder): `/` on the PS5, macOS and Linux; on Windows the root of the home folder's
/// drive. ponytail: one drive on Windows; a root listing the drives would reach the others.
pub fn default_share_root() -> Option<PathBuf> {
    if cfg!(windows) {
        let home = std::env::home_dir().filter(|h| h.is_absolute())?;
        home.ancestors().last().map(PathBuf::from)
    } else {
        Some(PathBuf::from("/"))
    }
}

/// `base` + a checked manifest path, one plain name at a time (a `C:` part can't replace it).
pub fn join_checked(base: &Path, rel: &str) -> Result<PathBuf, String> {
    check_path(rel)?;
    let mut p = base.to_path_buf();
    for part in rel.split('/') {
        let mut c = Path::new(part).components();
        if !matches!((c.next(), c.next()), (Some(Component::Normal(_)), None)) {
            return Err(format!("bad path {rel:?}"));
        }
        p.push(part);
    }
    Ok(p)
}

/// The kind byte of the conn's first frame, without reading it.
fn first_kind(stream: &TcpStream) -> io::Result<u8> {
    let start = Instant::now();
    let mut b = [0u8; 3];
    loop {
        match stream.peek(&mut b)? {
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n if n >= b.len() => return Ok(b[2]),
            _ if start.elapsed() > DEAD_AFTER => return Err(io::ErrorKind::TimedOut.into()),
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn accept_loop(inner: &Arc<Inner>, listener: TcpListener) {
    for stream in listener.incoming() {
        if inner.stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else {
            // EMFILE and the like: don't spin.
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        // Local networks only (by user decision): FLINK doesn't encrypt.
        if !stream.peer_addr().is_ok_and(|a| is_local(a.ip())) {
            continue;
        }
        let id = inner.next_conn.fetch_add(1, Ordering::SeqCst);
        {
            // Checked under the lock `shutdown` takes after setting `stop`.
            let mut conns = lock(&inner.conns);
            if conns.len() >= MAX_CONNS || inner.stop.load(Ordering::SeqCst) {
                continue;
            }
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            conns.insert(
                id,
                Inbound {
                    stream: clone,
                    peer: None,
                },
            );
        }
        let inner = inner.clone();
        std::thread::spawn(move || {
            let _ = inner.serve(id, stream);
            lock(&inner.conns).remove(&id);
        });
    }
}

/// Requests one after another on a control conn; the conn is closed on drop.
pub struct Rpc {
    tx: Mutex<Sender>,
    rx: Receiver,
    next: u64,
    /// Outgoing sessions stop being usable when this device forgets the pairing too.
    allowed: Option<Box<dyn Fn() -> bool + Send + Sync>>,
}

impl Rpc {
    pub fn new(conn: Conn) -> io::Result<Rpc> {
        let (tx, rx) = conn.split()?;
        Ok(Rpc {
            tx: Mutex::new(tx),
            rx,
            next: 1,
            allowed: None,
        })
    }

    /// The peer's `err` comes back as `io::Error::other(err)`.
    pub fn call(&mut self, method: &str, body: Value) -> io::Result<Value> {
        if self.allowed.as_ref().is_some_and(|allowed| !allowed()) {
            lock(&self.tx).shutdown();
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, NOT_PAIRED));
        }
        let id = self.next;
        self.next += 1;
        let req = Request {
            id,
            method: method.into(),
            body,
        };
        lock(&self.tx).send_frame(&req.frame()?)?;
        loop {
            if let Control::Response(r) = self.rx.recv_control(&self.tx)?
                && r.id == id
            {
                return r.result().map_err(io::Error::other);
            }
        }
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        lock(&self.tx).shutdown();
    }
}

/// One request on a control conn, then the conn closed.
fn rpc(conn: Conn, method: &str, body: Value) -> io::Result<Value> {
    Rpc::new(conn)?.call(method, body)
}

/// `ip:port`, `ip` (FLINK's `PORT`), or a host name with or without a port.
/// A local network address (by user decision, FLINK pairs and talks only within one): private
/// and link-local IPv4, 100.64/10 (CGNAT, Tailscale's range), loopback, IPv6 ULA (Tailscale's
/// fd7a:115c:a1e0::/48 included) and link-local; an IPv4-mapped IPv6 counts as its IPv4.
pub fn is_local(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let [a, b, ..] = v.octets();
            v.is_private() || v.is_link_local() || v.is_loopback() || (a == 100 && b & 0xC0 == 64)
        }
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => is_local(IpAddr::V4(v4)),
            None => {
                let s = v.segments()[0];
                v.is_loopback() || s & 0xFE00 == 0xFC00 || s & 0xFFC0 == 0xFE80
            }
        },
    }
}

pub(crate) fn not_local(ip: std::net::IpAddr) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{ip} isn't on a local network: Connect pairs only within one (or over a VPN)"),
    )
}

fn parse_addr(s: &str) -> io::Result<SocketAddr> {
    let s = s.trim();
    if let Ok(a) = s.parse::<SocketAddr>() {
        return Ok(a);
    }
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, format!("no address in {s:?}"));
    if let Ok(ip) = s.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, PORT));
    }
    let mut addrs = match s.to_socket_addrs() {
        Ok(a) => a,
        Err(_) => (s, PORT).to_socket_addrs().map_err(|_| bad())?,
    };
    addrs.next().ok_or_else(bad)
}

/// Hosts Discover probes at once (each checks SMB, NFS and FTP at once: `probe::serves`).
const HOSTS_AT_ONCE: usize = 4;

/// How long after the search a new batch of probes may still start (each takes up to ~3 s), so
/// many hosts (up to 64 by mDNS and 64 by WS-Discovery) can't keep a search going for minutes.
const PROBE_GRACE: Duration = Duration::from_secs(3);

/// The NAS results of the last search.
#[derive(Default)]
struct Lan {
    running: AtomicBool,
    stop: AtomicBool,
    nas: Mutex<Vec<crate::mdns::Nas>>,
    /// Windows PCs by WS-Discovery (`wsd.rs`): hosts to probe, shown by what answers there.
    wsd: Mutex<Vec<crate::mdns::Nas>>,
    /// What a probe confirmed on each host the searches found (`mdns::merge`).
    probed: Mutex<crate::mdns::Probed>,
    error: Mutex<Option<String>>,
}

impl Lan {
    /// The mDNS results plus what the probes confirmed (`mdns::merge`).
    fn found(&self) -> Vec<crate::mdns::Nas> {
        crate::mdns::merge(&lock(&self.nas), &lock(&self.wsd), &lock(&self.probed))
    }

    /// Hosts found and not probed yet, each with the protocols mDNS announced there (trusted
    /// as announced, so not checked again).
    fn unprobed(&self) -> Vec<(String, Vec<crate::probe::Kind>)> {
        use crate::probe::Kind;
        let (nas, wsd) = (lock(&self.nas).clone(), lock(&self.wsd).clone());
        let probed = lock(&self.probed);
        let mut out: Vec<(String, Vec<Kind>)> = Vec::new();
        for n in nas.iter().chain(&wsd) {
            if probed.contains_key(&n.address) || out.iter().any(|(a, _)| *a == n.address) {
                continue;
            }
            let announced = (nas.iter())
                .filter(|m| m.address == n.address)
                .filter_map(|m| {
                    [Kind::Smb, Kind::Nfs, Kind::Ftp]
                        .into_iter()
                        .find(|k| k.as_str() == m.service)
                })
                .collect();
            out.push((n.address.clone(), announced));
        }
        out
    }

    /// Probes each host the searches find, `HOSTS_AT_ONCE` at a time, until the searches have
    /// ended (`searched`) and every host is done, or Stop, or `PROBE_GRACE` past `until` (a host
    /// left unprobed then shows as mDNS announced it; a Windows PC not at all).
    fn confirm_hosts(&self, searched: &AtomicBool, until: Instant) {
        loop {
            // Read before listing, so a host found just before the end is still probed.
            let done = searched.load(Ordering::Acquire);
            if self.stop.load(Ordering::Acquire) || Instant::now() >= until + PROBE_GRACE {
                return;
            }
            let mut todo = self.unprobed();
            if todo.is_empty() {
                if done {
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            todo.truncate(HOSTS_AT_ONCE);
            let mut probed = lock(&self.probed);
            for (address, _) in &todo {
                probed.insert(address.clone(), None);
            }
            drop(probed);
            std::thread::scope(|s| {
                for (address, skip) in todo {
                    s.spawn(move || {
                        let confirmed = (address.parse::<std::net::IpAddr>().ok())
                            .map(|ip| crate::probe::serves(ip, &skip))
                            .unwrap_or_default();
                        let confirmed = (confirmed.into_iter())
                            .map(|(kind, port)| (kind.as_str(), port))
                            .collect();
                        lock(&self.probed).insert(address, Some(confirmed));
                    });
                }
            });
        }
    }
}

/// A Forge device a search found; `paired` when it is a saved device.
#[derive(Clone, Debug, Serialize)]
pub struct ForgeFound {
    #[serde(flatten)]
    pub found: crate::discovery::Found,
    pub paired: bool,
}

/// What Discover shows: `searching` while either search runs.
#[derive(Clone, Debug, Serialize)]
pub struct Discovered {
    pub searching: bool,
    pub error: Option<String>,
    pub forge: Vec<ForgeFound>,
    pub nas: Vec<crate::mdns::Nas>,
}

impl Connect {
    /// Starts a 10 s search for Forge devices and NAS services with empty lists (already
    /// searching: unchanged); every NAS or Windows PC host found is probed for SMB, NFS and FTP
    /// meanwhile, so the search may run a few seconds past 10 s. No subnet sweep: only hosts
    /// that announce themselves. A failure shows in `discovery().error`.
    pub fn discover_start(&self) {
        let _ = self.discovery.start();
        let mut slot = lock(&self.lan_thread);
        // A search being stopped is waited for (one probe batch at most), then started again.
        if self.lan.running.load(Ordering::SeqCst) && !self.lan.stop.load(Ordering::SeqCst) {
            return;
        }
        if let Some(t) = slot.take() {
            let _ = t.join();
        }
        lock(&self.lan.nas).clear();
        lock(&self.lan.wsd).clear();
        lock(&self.lan.probed).clear();
        *lock(&self.lan.error) = None;
        self.lan.stop.store(false, Ordering::SeqCst);
        self.lan.running.store(true, Ordering::SeqCst);
        let (lan, inner) = (self.lan.clone(), self.inner.clone());
        *slot = Some(std::thread::spawn(move || {
            let until = Instant::now() + crate::discovery::SEARCH_TIME;
            // Both searches at once, each host they find probed meanwhile (`confirm_hosts`, up
            // to about 6 s past the search); the first failure is shown.
            let searched = AtomicBool::new(false);
            std::thread::scope(|scope| {
                scope.spawn(|| lan.confirm_hosts(&searched, until));
                let wsd = scope.spawn(|| {
                    let found = |list| *lock(&lan.wsd) = list;
                    if let Err(e) = crate::wsd::search(until, &lan.stop, &found) {
                        lock(&lan.error).get_or_insert(format!("Windows PC search: {e}"));
                    }
                });
                let found = |list| *lock(&lan.nas) = list;
                if let Err(e) = crate::mdns::search(until, &lan.stop, &found) {
                    lock(&lan.error).get_or_insert(format!("NAS search: {e}"));
                }
                let _ = wsd.join();
                searched.store(true, Ordering::Release);
            });
            inner.migrate_nas(&lan.found());
            lan.running.store(false, Ordering::SeqCst);
        }));
    }

    /// Everything the Connect tab polls: this device, saved devices and the last search
    /// (`GET /api/connect/state`, Tauri `connect_state`).
    pub fn snapshot(&self) -> Value {
        let state = self.state();
        let pending: Vec<Value> = (self.pending().into_iter())
            .map(|r| {
                serde_json::json!({
                    "id": r.id, "name": r.name, "platform": r.platform,
                    "ip": r.ip.to_string(), "check": r.check,
                })
            })
            .collect();
        serde_json::json!({
            "device": state.device,
            "devices": state.devices,
            "nas": self.nas_devices(),
            "discovery": self.discovery(),
            "pending": pending,
            "outgoing": self.outgoing(),
            "remote_jobs": self.inner.jobs.remote_jobs(&|key| {
                lock(&self.inner.peers).get(key).map(|p| p.name.clone())
            }),
        })
    }

    /// Ends the search; what it found stays listed until the next one.
    pub fn discover_stop(&self) {
        self.discovery.stop();
        self.lan.stop.store(true, Ordering::SeqCst);
    }

    pub fn discovery(&self) -> Discovered {
        let scan = self.discovery.scan();
        let peers = lock(&self.inner.peers);
        let forge = scan
            .devices
            .into_iter()
            .map(|found| ForgeFound {
                paired: peers.get(&found.key_id).is_some(),
                found,
            })
            .collect();
        drop(peers);
        Discovered {
            searching: scan.searching || self.lan.running.load(Ordering::SeqCst),
            error: scan.error.or_else(|| lock(&self.lan.error).clone()),
            forge,
            nas: self.lan.found(),
        }
    }
}

// ---- Pairing requests ---------------------------------------------------------------------------

/// Requests waiting at once (one per address): more are denied unasked.
const MAX_WAITING: usize = 4;

/// Other devices' pairing requests, waiting for this device's user (the handshake's `Approver`):
/// the UI lists them (`Connect::pending`) and answers one (`Connect::approve`), which wakes the
/// handshake waiting for it.
#[derive(Default)]
pub(crate) struct Approvals {
    waiting: Mutex<Vec<(PairRequest, Option<bool>)>>,
    changed: std::sync::Condvar,
}

impl Approvals {
    /// The user's answer to request `id`; false when it no longer waits.
    fn answer(&self, id: &str, allow: bool) -> bool {
        let mut w = lock(&self.waiting);
        let Some(entry) = w.iter_mut().find(|(r, a)| r.id == id && a.is_none()) else {
            return false;
        };
        entry.1 = Some(allow);
        self.changed.notify_all();
        true
    }

    fn waiting(&self) -> Vec<PairRequest> {
        (lock(&self.waiting).iter())
            .filter(|(_, a)| a.is_none())
            .map(|(r, _)| r.clone())
            .collect()
    }
}

impl Approver for Approvals {
    // One request per address and per claimed key id waiting at once: two claiming one id can't
    // both be allowed, the later replacing the first.
    // ponytail: a client that stops mid-handshake holds its connection until the read timeout
    // (`DEAD_AFTER`), as one that never sends its Hello does; the connection cap bounds both.
    fn admit(&self, req: &PairRequest) -> bool {
        let mut w = lock(&self.waiting);
        if w.len() >= MAX_WAITING
            || w.iter()
                .any(|(r, _)| r.ip == req.ip || r.key_id == req.key_id)
        {
            return false;
        }
        w.push((req.clone(), None));
        true
    }

    fn decide(&self, req: &PairRequest, wait: Duration) -> Option<bool> {
        let mut w = lock(&self.waiting);
        let deadline = Instant::now() + wait;
        loop {
            if let Some((_, Some(answer))) = w.iter().find(|(r, _)| r.id == req.id) {
                return Some(*answer);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            w = match self.changed.wait_timeout(w, left) {
                Ok((g, _)) => g,
                Err(e) => e.into_inner().0,
            };
        }
    }

    fn done(&self, req: &PairRequest) {
        lock(&self.waiting).retain(|(r, _)| r.id != req.id);
        self.changed.notify_all();
    }
}

/// This device's pairing request as the UI follows it. `state`: `asking` (connecting),
/// `waiting` (the other device's user decides; `check` is set), then `paired` (`device`),
/// `denied`, `timed_out`, `busy`, `already_paired`, `too_many_attempts`, `failed` (`error`) or
/// `cancelled`.
#[derive(Debug, Serialize)]
pub struct Outgoing {
    pub id: u64,
    pub address: String,
    pub check: Option<String>,
    pub state: &'static str,
    pub error: Option<String>,
    pub device: Option<Device>,
    /// The request's connection, to cancel it.
    #[serde(skip)]
    stream: Option<TcpStream>,
}

impl Clone for Outgoing {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            address: self.address.clone(),
            check: self.check.clone(),
            state: self.state,
            error: self.error.clone(),
            device: self.device.clone(),
            stream: None,
        }
    }
}

impl Outgoing {
    fn new(id: u64, addr: SocketAddr) -> Self {
        Self {
            id,
            address: addr.to_string(),
            check: None,
            state: "asking",
            error: None,
            device: None,
            stream: None,
        }
    }

    fn running(&self) -> bool {
        matches!(self.state, "asking" | "waiting")
    }
}

fn code_state(code: &str) -> &'static str {
    match code {
        ps5_dump_forge_flink::DENIED => "denied",
        ps5_dump_forge_flink::TIMED_OUT => "timed_out",
        ps5_dump_forge_flink::BUSY => "busy",
        ps5_dump_forge_flink::ALREADY_PAIRED => "already_paired",
        _ => "too_many_attempts",
    }
}

/// `from` asks `to` (listening at `addr`) to pair; `to`'s user answers `allow` once both show
/// the same check number. The request's outcome.
#[cfg(test)]
impl Connect {
    /// The files peers write here.
    pub(crate) fn writes(&self) -> &crate::connect_fs::Writes {
        &self.inner.writes
    }
}

#[cfg(test)]
pub(crate) fn pair_for_test(from: &Connect, to: &Connect, addr: &str, allow: bool) -> Outgoing {
    from.pair(addr).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let o = from.outgoing().unwrap();
        if let (Some(check), Some(r)) = (&o.check, to.pending().first()) {
            assert_eq!(check, &r.check, "both screens show one check number");
            assert_eq!(r.key_id, from.me().key_id);
            assert!(to.approve(&r.id, allow));
            break;
        }
        assert!(o.running(), "{o:?}");
        assert!(
            Instant::now() < deadline,
            "no request reached the other device"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    pair_then(from, deadline)
}

/// `from`'s request's outcome, once it has one.
#[cfg(test)]
fn pair_then(from: &Connect, deadline: Instant) -> Outgoing {
    loop {
        let o = from.outgoing().unwrap();
        if !o.running() {
            return o;
        }
        assert!(Instant::now() < deadline, "the request never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-connect-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_pairing_done_as_cancel_came_says_paired() {
        let dir = temp_dir("cancel-race");
        let c = Connect::start(&dir, 0).unwrap();
        let addr: SocketAddr = "127.0.0.1:9883".parse().unwrap();
        let device = Device {
            key_id: "k".into(),
            name: "ps5".into(),
            platform: "ps5".into(),
            address: Some("127.0.0.1:9883".into()),
            last_seen: 0,
            online: true,
        };
        for (ok, state) in [(false, "cancelled"), (true, "paired")] {
            *lock(&c.inner.outgoing) = Some(Outgoing::new(1, addr));
            c.pair_cancel();
            let result = if ok {
                Ok(device.clone())
            } else {
                Err(io::Error::other("cancelled"))
            };
            c.inner.pair_done(1, result);
            assert_eq!(c.outgoing().unwrap().state, state);
        }
        c.shutdown();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_networks_only() {
        for (ip, local) in [
            ("192.168.99.230", true),
            ("10.1.2.3", true),
            ("172.16.0.1", true),
            ("172.32.0.1", false),
            ("169.254.1.1", true),
            ("100.64.0.1", true),
            ("100.127.255.255", true),
            ("100.128.0.1", false),
            ("127.0.0.1", true),
            ("8.8.8.8", false),
            ("::1", true),
            ("fd7a:115c:a1e0::1", true),
            ("fe80::1", true),
            ("2001:4860::8888", false),
            ("::ffff:192.168.1.2", true),
            ("::ffff:8.8.8.8", false),
        ] {
            assert_eq!(is_local(ip.parse().unwrap()), local, "{ip}");
        }
        let dir = temp_dir("public");
        let c = Connect::start(&dir, 0).unwrap();
        let e = c.pair("8.8.8.8").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        c.shutdown();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_start_stop_and_shutdown() {
        let dir = temp_dir("discover");
        let c = Connect::start_rooted(&dir, 0, None).unwrap();
        assert!(c.discovery().forge.is_empty());
        c.discover_start();
        assert!(c.discovery().searching);
        c.discover_start(); // already searching: unchanged
        c.discover_stop();
        let t = Instant::now();
        c.shutdown();
        // The LAN searches see the stop within a read timeout (a description fetch could
        // take longer on a real LAN).
        assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());
        assert!(!c.discovery().searching);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nas_saved_once_and_password_never_in_snapshot() {
        let dir = temp_dir("nas");
        let c = Connect::start_rooted(&dir, 0, None).unwrap();
        // A port nothing listens on: offline.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let host = format!("127.0.0.1:{port}");
        let save = |name: &str, password: Option<&str>| crate::nas::NasSave {
            name: name.into(),
            host: host.clone(),
            user: Some("guest-user".into()),
            password: password.map(Into::into),
            ..Default::default()
        };
        let id = c.nas_save(&save("Box", Some("s3cret-pw"))).unwrap();
        assert_eq!(c.nas_save(&save("Renamed", None)).unwrap(), id);
        let snap = c.snapshot();
        let text = snap.to_string();
        assert!(!text.contains("s3cret-pw"), "{text}");
        assert_eq!(snap["nas"].as_array().unwrap().len(), 1);
        assert_eq!(snap["nas"][0]["name"], "Renamed");
        assert_eq!(snap["nas"][0]["has_password"], true);
        assert_eq!(snap["nas"][0]["online"], false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(crate::nas::NAS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        c.shutdown();
        let c = Connect::start_rooted(&dir, 0, None).unwrap();
        assert_eq!(c.nas_saved()[0].password, "s3cret-pw");
        assert!(c.nas_forget(&id).unwrap());
        assert!(c.nas_devices().is_empty());
        c.shutdown();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn approvals_admit_one_per_address_and_key_id() {
        let req = |id: &str, ip: [u8; 4], key: &str| PairRequest {
            id: id.into(),
            key_id: key.into(),
            name: "x".into(),
            platform: "test".into(),
            ip: ip.into(),
            check: "123456".into(),
        };
        let a = Approvals::default();
        assert!(a.admit(&req("1", [10, 0, 0, 1], "k1")));
        assert!(!a.admit(&req("2", [10, 0, 0, 1], "k2")), "one per address");
        assert!(
            !a.admit(&req("3", [10, 0, 0, 2], "k1")),
            "one per claimed key id"
        );
        assert!(a.admit(&req("4", [10, 0, 0, 3], "k3")));
        assert!(a.answer("1", true));
        assert!(!a.answer("1", false), "answered once");
        assert_eq!(
            a.decide(&req("1", [10, 0, 0, 1], "k1"), Duration::ZERO),
            Some(true)
        );
        a.done(&req("1", [10, 0, 0, 1], "k1"));
        assert_eq!(a.waiting().len(), 1);
        assert!(
            a.admit(&req("5", [10, 0, 0, 1], "k1")),
            "its address is free again"
        );
    }

    #[test]
    fn identity_persists() {
        let dir = temp_dir("id");
        let a = Identity::load_or_create(&dir).unwrap();
        assert_eq!(a.key_id.len(), 32);
        let b = Identity::load_or_create(&dir).unwrap();
        assert_eq!(a.secret, b.secret);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(SECRET_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::write(dir.join(SECRET_FILE), b"short").unwrap();
        assert!(Identity::load_or_create(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Handshakes recorded at once from many threads, each from a new address: every one saves
    /// outside the `peers` lock, and peers.json ends as memory does (never an older snapshot
    /// written last). A failed save is tried again by the next handshake.
    #[test]
    fn concurrent_records_leave_the_newest_peers_json() {
        let dir = temp_dir("records");
        let c = Connect::start(&dir, 0).unwrap();
        let key = "k".repeat(32);
        lock(&c.inner.peers).upsert(Peer {
            key_id: key.clone(),
            name: "pc".into(),
            platform: "windows".into(),
            address: None,
            last_seen: 0,
            secret: hex(&[7; 32]),
        });
        let hs = |port: u16| Handshake {
            peer: ps5_dump_forge_flink::Hello {
                v: 2,
                key_id: key.clone(),
                name: "pc".into(),
                platform: "windows".into(),
                port,
                nonce: String::new(),
            },
            remote: "192.168.1.5:50000".parse().unwrap(),
            new_secret: None,
            session: ps5_dump_forge_flink::Session {
                secret: [7; 32],
                nonce_c: [0; 16],
                nonce_s: [0; 16],
            },
        };
        for round in 0..5u16 {
            std::thread::scope(|s| {
                for t in 0..8u16 {
                    let hs = hs(1000 + round * 8 + t);
                    let inner = &c.inner;
                    s.spawn(move || inner.record(&hs).unwrap());
                }
            });
            assert_eq!(Peers::load(&dir).unwrap(), *lock(&c.inner.peers));
        }
        // The race those threads may or may not hit, forced: a snapshot taken earlier, whose
        // save comes last, is not written over the newer one.
        let older = lock(&c.inner.peers).clone();
        let v = c.inner.peers_gen.load(Ordering::SeqCst);
        c.inner.record(&hs(999)).unwrap();
        c.inner.save_peers(&older, v).unwrap();
        assert_eq!(Peers::load(&dir).unwrap(), *lock(&c.inner.peers));
        // A save that fails (the folder is a file now) is written by the next handshake, even
        // one that changes nothing.
        fs::remove_dir_all(&dir).unwrap();
        fs::write(&dir, b"").unwrap();
        c.inner.record(&hs(1)).unwrap();
        fs::remove_file(&dir).unwrap();
        c.inner.record(&hs(1)).unwrap();
        assert_eq!(Peers::load(&dir).unwrap(), *lock(&c.inner.peers));
        c.shutdown();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn peers_round_trip() {
        let dir = temp_dir("peers");
        assert_eq!(Peers::load(&dir).unwrap(), Peers::default());
        let peer = |id: &str, addr: &str| Peer {
            key_id: id.repeat(32),
            name: format!("dev {id}"),
            platform: "ps5".into(),
            address: Some(addr.into()),
            last_seen: 10,
            secret: "ab".repeat(32),
        };
        let mut p = Peers::default();
        p.upsert(peer("a", "10.0.0.2:9883"));
        p.upsert(peer("b", "10.0.0.3:9883"));
        // Pairing again with a known key id updates that one entry.
        p.upsert(Peer {
            name: "renamed".into(),
            ..peer("a", "10.0.0.9:9883")
        });
        assert_eq!(p.peers.len(), 2);
        // Seen at b's old address: b keeps its own entry and address.
        assert!(p.seen(&"a".repeat(32), "10.0.0.3:9883", 20));
        assert!(!p.seen(&"c".repeat(32), "10.0.0.4:9883", 20));
        assert_eq!(
            p.get(&"b".repeat(32)).unwrap().address.as_deref(),
            Some("10.0.0.3:9883")
        );
        assert_eq!(
            p.get(&"a".repeat(32)).unwrap().pair_secret(),
            Some([0xab; 32])
        );
        p.save(&dir).unwrap();
        let q = Peers::load(&dir).unwrap();
        assert_eq!(q, p);
        assert_eq!(q.get(&"a".repeat(32)).unwrap().name, "renamed");
        assert_eq!(q.get(&"a".repeat(32)).unwrap().last_seen, 20);
        let mut q = q;
        assert!(q.forget(&"a".repeat(32)));
        assert!(!q.forget(&"a".repeat(32)));
        q.save(&dir).unwrap();
        assert_eq!(Peers::load(&dir).unwrap().peers.len(), 1);
        assert!(
            fs::read_dir(&dir).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn service_pairs_lists_and_forgets() {
        let dirs = ["svc-a", "svc-b", "svc-c"].map(temp_dir);
        let a = Connect::start(&dirs[0], 0).unwrap();
        let b = Connect::start(&dirs[1], 0).unwrap();
        let c = Connect::start(&dirs[2], 0).unwrap();
        let sa = a.state();
        assert!(sa.device.listen_error.is_none());
        assert_ne!(sa.device.port, 0);
        let addr_a = format!("127.0.0.1:{}", sa.device.port);
        let at = |s: &Connect| Some(format!("127.0.0.1:{}", s.me().port));

        // Denied on A: refused, nothing stored on either side.
        let o = pair_for_test(&c, &a, &addr_a, false);
        assert_eq!(o.state, "denied");
        assert!(a.devices().is_empty() && c.devices().is_empty());
        assert!(a.pending().is_empty());

        // Allowed on A: both list each other, online.
        let o = pair_for_test(&b, &a, &addr_a, true);
        assert_eq!(o.state, "paired", "{o:?}");
        let d = o.device.unwrap();
        assert_eq!(d.key_id, sa.device.key_id);
        assert!(d.online);
        let on_a = a.devices();
        assert_eq!(on_a.len(), 1);
        assert_eq!(on_a[0].key_id, b.me().key_id);
        assert!(on_a[0].online);
        assert_eq!(on_a[0].address, at(&b));
        let info = b.call(&a.me().key_id, "info", Value::Null).unwrap();
        assert_eq!(info["key_id"], a.me().key_id.as_str());

        // B comes back on another port: its next handshake moves A's one entry there (saved).
        b.shutdown();
        let b = Connect::start(&dirs[1], 0).unwrap();
        b.refresh();
        let on_a = a.devices();
        assert_eq!(on_a.len(), 1);
        assert_eq!(on_a[0].address, at(&b));
        // Saved just after memory changes (outside the `peers` lock).
        let wait = Instant::now() + Duration::from_secs(5);
        while Peers::load(&dirs[0]).unwrap().peers[0].address != at(&b) {
            assert!(Instant::now() < wait, "the new address was never saved");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(b.devices()[0].online);
        // Asking again while A has B saved: refused unasked (a claimed key id never replaces a
        // pairing); once A forgot B, pairing again keeps one entry on each side.
        b.pair(&addr_a).unwrap();
        let o = pair_then(&b, Instant::now() + Duration::from_secs(20));
        assert_eq!(o.state, "already_paired", "{o:?}");
        assert!(a.forget(&b.me().key_id).unwrap());
        assert_eq!(pair_for_test(&b, &a, &addr_a, true).state, "paired");
        // A request cancelled while it waits: nothing changes, and A's prompt goes.
        assert!(a.forget(&b.me().key_id).unwrap());
        b.pair(&addr_a).unwrap();
        let wait = Instant::now() + Duration::from_secs(10);
        while a.pending().is_empty() {
            assert!(Instant::now() < wait, "no request on A");
            std::thread::sleep(Duration::from_millis(20));
        }
        b.pair_cancel();
        assert_eq!(b.outgoing().unwrap().state, "cancelled");
        let wait = Instant::now() + Duration::from_secs(10);
        while !a.pending().is_empty() {
            assert!(
                Instant::now() < wait,
                "the cancelled request still waits on A"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(a.devices().is_empty());
        assert_eq!(pair_for_test(&b, &a, &addr_a, true).state, "paired");
        assert_eq!((a.devices().len(), b.devices().len()), (1, 1));

        // A forgets B: gone on A; B's next probe is refused and shows A offline.
        assert!(a.forget(&b.me().key_id).unwrap());
        assert!(!a.forget(&b.me().key_id).unwrap());
        assert!(a.devices().is_empty());
        assert!(Peers::load(&dirs[0]).unwrap().peers.is_empty());
        b.refresh();
        assert!(!b.devices()[0].online);
        let e = b.call(&a.me().key_id, "info", Value::Null).unwrap_err();
        assert_eq!(refusal(&e), Some(NOT_PAIRED));

        for s in [a, b, c] {
            s.shutdown();
        }
        for d in dirs {
            fs::remove_dir_all(d).unwrap();
        }
    }

    /// A peer's `install` is refused on a device with no install hook (not a PS5); with one,
    /// the job starts on the caller's package, spelled `flink://<caller>/...`.
    #[test]
    fn install_is_refused_without_a_hook() {
        let dirs = ["inst-a", "inst-b"].map(temp_dir);
        let a = Connect::start(&dirs[0], 0).unwrap();
        let b = Connect::start(&dirs[1], 0).unwrap();
        let addr_a = format!("127.0.0.1:{}", a.me().port);
        assert_eq!(pair_for_test(&b, &a, &addr_a, true).state, "paired");
        let pkg = dunce::canonicalize(&dirs[1]).unwrap().join("game.pkg");
        let ka = a.me().key_id.clone();

        assert_eq!(b.install(&ka, &pkg).unwrap_err(), CANT_INSTALL);
        let e = (b.call(&ka, "install", serde_json::json!({ "path": "x/../y.pkg" }))).unwrap_err();
        assert_eq!(error_text(&e), CANT_INSTALL, "the hook is asked for first");

        let seen = Arc::new(Mutex::new(Vec::new()));
        let got = seen.clone();
        a.set_install(Arc::new(move |p| {
            lock(&got).push(p);
            Ok(7)
        }));
        assert_eq!(b.install(&ka, &pkg), Ok(7));
        let rel =
            crate::connect_jobs::relative(b.inner.share_root().unwrap(), pkg.to_str().unwrap());
        let want = PathBuf::from(format!("flink://{}/{}", b.me().key_id, rel.unwrap()));
        assert_eq!(*lock(&seen), [want]);
        let e = (b.call(&ka, "install", serde_json::json!({ "path": "x/../y.pkg" }))).unwrap_err();
        assert!(error_text(&e).contains("is not a file path"), "{e}");
        assert_eq!(lock(&seen).len(), 1);

        for s in [a, b] {
            s.shutdown();
        }
        for d in dirs {
            fs::remove_dir_all(d).unwrap();
        }
    }
}
