//! Forge discovery on UDP 8094: a passive responder that is always on, and a 10 s active
//! search (the Discover button). A query is a broadcast; a reply names the device (name,
//! platform, FLINK port, key id) and never the magic code. A reply from a paired key id is
//! handed to `Connect::discovered` (its address, for liveness).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const PORT: u16 = 8094;
const LIMIT: usize = 1024;
const CYCLE: Duration = Duration::from_secs(5);
const WINDOW: Duration = Duration::from_secs(3);
pub const SEARCH_TIME: Duration = Duration::from_secs(10);
const MAX_FOUND: usize = 64;
const MAGIC: &str = "forge-discovery";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a reply says about its device (never the code).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    /// Its FLINK port.
    pub port: u16,
}

impl Node {
    fn valid(&self) -> bool {
        self.key_id.len() == 32
            && self.key_id.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.name.is_empty()
            && self.name.len() <= 128
            && !self.platform.is_empty()
            && self.platform.len() <= 32
            && self.platform.bytes().all(|b| b.is_ascii_alphanumeric())
            && self.port != 0
    }
}

/// A Forge device a search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    pub key_id: String,
    pub name: String,
    pub platform: String,
    /// `ip:port` (its FLINK listener).
    pub address: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Scan {
    pub searching: bool,
    pub error: Option<String>,
    pub devices: Vec<Found>,
}

#[derive(Serialize, Deserialize)]
struct Packet {
    magic: String,
    version: u8,
    kind: String,
    scan_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    node: Option<Node>,
}

impl Packet {
    fn query(scan_id: String) -> Self {
        Self {
            magic: MAGIC.into(),
            version: 1,
            kind: "query".into(),
            scan_id,
            node: None,
        }
    }

    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > LIMIT {
            return None;
        }
        let packet: Self = serde_json::from_slice(bytes).ok()?;
        (packet.magic == MAGIC
            && packet.version == 1
            && !packet.scan_id.is_empty()
            && packet.scan_id.len() <= 128)
            .then_some(packet)
    }
}

struct Snapshot {
    node: Node,
    active_until: Option<Instant>,
    error: Option<String>,
    devices: HashMap<String, Found>,
}

/// Called with (key id, its FLINK address) for every accepted reply.
pub type OnReply = Arc<dyn Fn(&str, SocketAddr) + Send + Sync>;

struct Worker {
    cancel: Arc<AtomicBool>,
    join: JoinHandle<()>,
    address: SocketAddr,
}

/// Owned by the Connect service; the worker owns only the snapshot and the callback.
pub struct Discovery {
    shared: Arc<Mutex<Snapshot>>,
    worker: Mutex<Option<Worker>>,
    bind: SocketAddrV4,
    /// Tests: where queries go instead of the broadcast addresses.
    targets: Option<Vec<SocketAddrV4>>,
    on_reply: OnReply,
}

impl Discovery {
    /// The responder on `0.0.0.0:port` (`PORT`; 0 = any, for tests). A port in use is
    /// reported in `scan().error` and retried by `start`.
    pub fn new(node: Node, port: u16, on_reply: OnReply) -> Self {
        Self::new_on(
            node,
            SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port),
            None,
            on_reply,
        )
    }

    pub(crate) fn new_on(
        node: Node,
        bind: SocketAddrV4,
        targets: Option<Vec<SocketAddrV4>>,
        on_reply: OnReply,
    ) -> Self {
        let discovery = Self {
            shared: Arc::new(Mutex::new(Snapshot {
                node,
                active_until: None,
                error: None,
                devices: HashMap::new(),
            })),
            worker: Mutex::new(None),
            bind,
            targets,
            on_reply,
        };
        let _ = discovery.ensure_worker(&mut lock(&discovery.worker));
        discovery
    }

    /// The responder's address (tests: queries go there).
    pub fn address(&self) -> Option<SocketAddr> {
        lock(&self.worker).as_ref().map(|w| w.address)
    }

    pub fn scan(&self) -> Scan {
        let state = lock(&self.shared);
        let mut devices: Vec<Found> = state.devices.values().cloned().collect();
        devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.key_id.cmp(&b.key_id)));
        Scan {
            searching: state
                .active_until
                .is_some_and(|until| Instant::now() < until),
            error: state.error.clone(),
            devices,
        }
    }

    /// Starts a `SEARCH_TIME` search with an empty list (already searching: unchanged).
    pub fn start(&self) -> Result<(), String> {
        let mut owner = lock(&self.worker);
        self.ensure_worker(&mut owner)?;
        let mut state = lock(&self.shared);
        // One past its time counts as over, though the worker hasn't cleared it yet.
        if state
            .active_until
            .is_none_or(|until| Instant::now() >= until)
        {
            state.devices.clear();
            state.active_until = Some(Instant::now() + SEARCH_TIME);
        }
        Ok(())
    }

    /// Ends the search; what it found stays listed until the next start.
    pub fn stop(&self) {
        lock(&self.shared).active_until = None;
    }

    /// Stops and joins the passive responder as well.
    pub fn shutdown(&self) {
        if let Some(worker) = lock(&self.worker).take() {
            worker.cancel.store(true, Ordering::Release);
            let _ = worker.join.join();
        }
        lock(&self.shared).active_until = None;
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Discovery {
    fn ensure_worker(&self, owner: &mut Option<Worker>) -> Result<(), String> {
        if owner.as_ref().is_some_and(|w| !w.join.is_finished()) {
            return Ok(());
        }
        if let Some(worker) = owner.take() {
            let _ = worker.join.join();
        }
        let sockets = (|| -> io::Result<_> {
            let responder = UdpSocket::bind(self.bind)?;
            responder.set_nonblocking(true)?;
            responder.set_broadcast(true)?;
            let replies = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            replies.set_nonblocking(true)?;
            replies.set_broadcast(true)?;
            Ok((responder.local_addr()?, responder, replies))
        })();
        let (address, responder, replies) = match sockets {
            Ok(s) => s,
            Err(e) => return Err(self.fail(format!("discovery on UDP {}: {e}", self.bind))),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let shared = self.shared.clone();
        let targets = self.targets.clone();
        let on_reply = self.on_reply.clone();
        lock(&shared).error = None;
        let join = thread::Builder::new()
            .name("forge-discovery".into())
            .spawn(move || {
                if let Err(e) = run(&shared, &signal, &responder, &replies, targets, &on_reply) {
                    let mut state = lock(&shared);
                    state.active_until = None;
                    state.error = Some(format!("discovery: {e}"));
                }
            })
            .map_err(|e| self.fail(format!("discovery worker: {e}")))?;
        *owner = Some(Worker {
            cancel,
            join,
            address,
        });
        Ok(())
    }

    fn fail(&self, error: String) -> String {
        let mut state = lock(&self.shared);
        state.active_until = None;
        state.error = Some(error.clone());
        error
    }
}

/// Asks one device's responder (`at`, by unicast: the query Discover broadcasts) who it is,
/// waiting up to `wait`: its node, or `None` when no valid reply from that address came.
pub fn ask(at: SocketAddrV4, wait: Duration) -> io::Result<Option<Node>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    let scan_id = random_id()?;
    socket.send_to(&serde_json::to_vec(&Packet::query(scan_id.clone()))?, at)?;
    let deadline = Instant::now() + wait;
    let mut buffer = [0; LIMIT + 1];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        socket.set_read_timeout(Some(left.max(Duration::from_millis(1))))?;
        let (len, source) = match socket.recv_from(&mut buffer) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            // An ICMP port unreachable, or a signal: wait on.
            Err(_) => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        };
        if source.ip() != std::net::IpAddr::V4(*at.ip()) {
            continue;
        }
        if let Some(packet) = Packet::parse(&buffer[..len])
            && packet.kind == "reply"
            && packet.scan_id == scan_id
            && let Some(node) = packet.node.filter(Node::valid)
        {
            return Ok(Some(node));
        }
    }
    Ok(None)
}

fn random_id() -> io::Result<String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    Ok(random.iter().map(|b| format!("{b:02x}")).collect())
}

fn run(
    shared: &Mutex<Snapshot>,
    cancel: &AtomicBool,
    responder: &UdpSocket,
    replies: &UdpSocket,
    injected: Option<Vec<SocketAddrV4>>,
    on_reply: &OnReply,
) -> io::Result<()> {
    let mut next = Instant::now();
    let mut started = next;
    let mut scan_id = String::new();
    let mut rates: HashMap<Ipv4Addr, (Instant, u8)> = HashMap::new();
    let mut buffer = [0; LIMIT + 1];
    let mut failing = false;
    while !cancel.load(Ordering::Acquire) {
        let now = Instant::now();
        let active = {
            let mut state = lock(shared);
            if state.active_until.is_some_and(|until| now >= until) {
                state.active_until = None;
            }
            state.active_until.is_some()
        };
        if !active {
            scan_id.clear();
            next = now;
        } else if now >= next {
            next = now + CYCLE;
            // A failed id skips this cycle's query; the responder keeps answering.
            let Ok(id) = random_id() else {
                continue;
            };
            scan_id = id;
            started = now;
            let Ok(bytes) = serde_json::to_vec(&Packet::query(scan_id.clone())) else {
                continue;
            };
            let targets = injected.clone().unwrap_or_else(|| {
                // Some payload/libc combinations can't list interfaces; the limited broadcast
                // still goes out on the interface the OS picks.
                broadcast_targets()
                    .unwrap_or_else(|_| vec![SocketAddrV4::new(Ipv4Addr::BROADCAST, PORT)])
            });
            let mut last_error = None;
            let mut sent = false;
            for target in targets {
                match replies.send_to(&bytes, target) {
                    Ok(_) => sent = true,
                    Err(e) => last_error = Some(e),
                }
            }
            lock(shared).error = match (sent, last_error) {
                (false, Some(e)) => Some(format!("discovery broadcast: {e}")),
                _ => None,
            };
        }
        rates.retain(|_, (at, _)| now.duration_since(*at) < Duration::from_secs(1));
        // Bounded batches on nonblocking sockets keep a stop prompt even under a flood.
        for (socket, is_reply) in [(responder, false), (replies, true)] {
            for _ in 0..32 {
                if cancel.load(Ordering::Acquire) {
                    return Ok(());
                }
                let (len, source) = match socket.recv_from(&mut buffer) {
                    Ok(v) => {
                        failing = false;
                        v
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::ConnectionReset
                        ) =>
                    {
                        continue;
                    }
                    // A network going down and up (or the PS5's rest mode: ENETDOWN, ENOBUFS)
                    // must not end the responder: this device would stop being found.
                    // ponytail: the sockets are never bound again.
                    Err(e) => {
                        if !failing {
                            eprintln!("discovery: {e} (retrying every second)");
                        }
                        failing = true;
                        thread::sleep(Duration::from_secs(1));
                        break;
                    }
                };
                let SocketAddr::V4(source) = source else {
                    continue;
                };
                let ip = *source.ip();
                if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
                    continue;
                }
                let Some(mut packet) = Packet::parse(&buffer[..len]) else {
                    continue;
                };
                if is_reply {
                    if packet.kind != "reply"
                        || scan_id.is_empty()
                        || packet.scan_id != scan_id
                        || started.elapsed() >= WINDOW
                    {
                        continue;
                    }
                    let Some(node) = packet.node.take().filter(Node::valid) else {
                        continue;
                    };
                    let address = SocketAddr::from((ip, node.port));
                    {
                        let mut state = lock(shared);
                        if state.active_until.is_none()
                            || node.key_id == state.node.key_id
                            || (state.devices.len() >= MAX_FOUND
                                && !state.devices.contains_key(&node.key_id))
                        {
                            continue;
                        }
                        let found = Found {
                            key_id: node.key_id.clone(),
                            name: node.name,
                            platform: node.platform,
                            address: address.to_string(),
                        };
                        state.devices.insert(node.key_id.clone(), found);
                    }
                    on_reply(&node.key_id, address);
                } else {
                    if packet.kind != "query" || packet.node.is_some() {
                        continue;
                    }
                    if rates.len() >= 64 && !rates.contains_key(&ip) {
                        continue;
                    }
                    let rate = rates.entry(ip).or_insert((Instant::now(), 0));
                    if rate.1 >= 2 {
                        continue;
                    }
                    rate.1 += 1;
                    packet.kind = "reply".into();
                    packet.node = Some(lock(shared).node.clone());
                    let Ok(bytes) = serde_json::to_vec(&packet) else {
                        continue;
                    };
                    // A departing peer must not stop discovery for everyone.
                    if bytes.len() <= LIMIT
                        && let Err(e) = responder.send_to(&bytes, source)
                    {
                        lock(shared).error = Some(format!("discovery reply: {e}"));
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn broadcast_targets() -> io::Result<Vec<SocketAddrV4>> {
    let mut addresses = Vec::new();
    #[cfg(unix)]
    {
        let mut head = std::ptr::null_mut();
        // SAFETY: getifaddrs initializes head on success. The list remains alive
        // throughout traversal; address families are checked before sockaddr casts.
        unsafe {
            if libc::getifaddrs(&mut head) != 0 {
                return Err(io::Error::last_os_error());
            }
            struct Interfaces(*mut libc::ifaddrs);
            impl Drop for Interfaces {
                fn drop(&mut self) {
                    unsafe { libc::freeifaddrs(self.0) };
                }
            }
            let _interfaces = Interfaces(head);
            let mut current = head;
            while !current.is_null() {
                let interface = &*current;
                current = interface.ifa_next;
                let flags = interface.ifa_flags as libc::c_int;
                if flags & libc::IFF_UP == 0
                    || flags & libc::IFF_BROADCAST == 0
                    || flags & (libc::IFF_LOOPBACK | libc::IFF_POINTOPOINT) != 0
                    || interface.ifa_addr.is_null()
                    || interface.ifa_netmask.is_null()
                    || (*interface.ifa_addr).sa_family as libc::c_int != libc::AF_INET
                    || (*interface.ifa_netmask).sa_family as libc::c_int != libc::AF_INET
                {
                    continue;
                }
                let addr = &*interface.ifa_addr.cast::<libc::sockaddr_in>();
                let mask = &*interface.ifa_netmask.cast::<libc::sockaddr_in>();
                let ip = u32::from_be(addr.sin_addr.s_addr);
                let mask = u32::from_be(mask.sin_addr.s_addr);
                if Ipv4Addr::from(ip).is_loopback()
                    || ip == 0
                    || mask.count_ones() > 30
                    || mask == 0
                {
                    continue;
                }
                addresses.push(SocketAddrV4::new(Ipv4Addr::from(ip | !mask), PORT));
            }
        }
    }
    // ponytail: Windows uses limited broadcast on the OS-selected interface;
    // multi-interface subnet enumeration there awaits a native adapter API.
    addresses.push(SocketAddrV4::new(Ipv4Addr::BROADCAST, PORT));
    addresses.sort_unstable();
    addresses.dedup();
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(key: char, port: u16) -> Node {
        Node {
            key_id: key.to_string().repeat(32),
            name: format!("Forge {key}"),
            platform: "macos".into(),
            port,
        }
    }

    fn v4(a: SocketAddr) -> SocketAddrV4 {
        match a {
            SocketAddr::V4(a) => SocketAddrV4::new(Ipv4Addr::LOCALHOST, a.port()),
            SocketAddr::V6(_) => unreachable!(),
        }
    }

    #[test]
    fn lifecycle_on_loopback() {
        let remote = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let target = v4(remote.local_addr().unwrap());
        // A busy responder port is reported. ponytail: the retry once free isn't tested here: a freed
        // ephemeral port can be taken by a parallel test before the retry binds it.
        let busy = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let busy_at = v4(busy.local_addr().unwrap());
        let noop: OnReply = Arc::new(|_, _| {});
        let retry = Discovery::new_on(node('a', 9883), busy_at, Some(vec![]), noop);
        assert!(retry.scan().error.is_some());
        retry.shutdown();

        let heard = Arc::new(Mutex::new(Vec::new()));
        let h = heard.clone();
        let on: OnReply = Arc::new(move |k, a| lock(&h).push((k.to_string(), a)));
        let d = Discovery::new_on(
            node('a', 9883),
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
            Some(vec![target]),
            on,
        );
        let at = d.address().unwrap();
        remote
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // The passive responder answers a query without a search, naming no code.
        let query = serde_json::to_vec(&Packet::query("passive".into())).unwrap();
        remote.send_to(&query, at).unwrap();
        let mut bytes = [0; LIMIT + 1];
        let (len, _) = remote.recv_from(&mut bytes).unwrap();
        let reply = Packet::parse(&bytes[..len]).unwrap();
        assert_eq!(
            (reply.kind.as_str(), reply.node),
            ("reply", Some(node('a', 9883)))
        );
        assert!(!String::from_utf8_lossy(&bytes[..len]).contains("code"));
        assert!(!d.scan().searching);

        // A search queries the targets; a reply to another round is ignored.
        d.start().unwrap();
        let (len, source) = remote.recv_from(&mut bytes).unwrap();
        let mut packet = Packet::parse(&bytes[..len]).unwrap();
        assert_eq!(packet.kind, "query");
        let id = std::mem::replace(&mut packet.scan_id, "other".into());
        packet.kind = "reply".into();
        packet.node = Some(node('b', 9133));
        remote
            .send_to(&serde_json::to_vec(&packet).unwrap(), source)
            .unwrap();
        thread::sleep(Duration::from_millis(80));
        assert!(d.scan().devices.is_empty());
        // Its own key id is ignored too.
        packet.scan_id = id;
        packet.node = Some(node('a', 9883));
        remote
            .send_to(&serde_json::to_vec(&packet).unwrap(), source)
            .unwrap();
        packet.node = Some(node('b', 9133));
        remote
            .send_to(&serde_json::to_vec(&packet).unwrap(), source)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while d.scan().devices.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let scan = d.scan();
        assert_eq!(scan.devices.len(), 1);
        assert_eq!(scan.devices[0].address, "127.0.0.1:9133");
        assert_eq!(scan.devices[0].key_id, "b".repeat(32));
        let b = SocketAddr::from(([127, 0, 0, 1], 9133));
        assert_eq!(*lock(&heard), vec![("b".repeat(32), b)]);

        // The search ends by itself; the results stay; the responder keeps answering.
        lock(&d.shared).active_until = Some(Instant::now());
        thread::sleep(Duration::from_millis(80));
        assert!(!d.scan().searching);
        assert_eq!(d.scan().devices.len(), 1);
        d.stop();
        remote.send_to(&query, at).unwrap();
        assert!(remote.recv_from(&mut bytes).is_ok());
        // A new search starts an empty list.
        d.start().unwrap();
        assert!(d.scan().devices.is_empty());
        d.shutdown();
        remote.send_to(&query, at).unwrap();
        remote
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        while let Ok((len, _)) = remote.recv_from(&mut bytes) {
            // Only the search's own query may still be queued.
            assert_eq!(Packet::parse(&bytes[..len]).unwrap().kind, "query");
        }
    }
}
