//! Connect's smart Pair by address: what answers at `host` or `host:port`, each confirmed by its
//! own protocol, never by an open port alone. No port: a Forge (discovery by unicast on UDP 8094,
//! else a FLINK Hello on 9883), then SMB 445, NFS 2049, FTP 21. A port: an FTP banner, an NFS
//! NULL call, an SMB2 NEGOTIATE, then a FLINK Hello. Every check runs at once, each within
//! `WAIT` to connect and `WAIT` for the exchange (then its socket is shut down, so a server that
//! drips bytes can't hold it), so a probe ends within about 3 s, 4 s for a Forge on a local
//! address (discovery's 1 s first), plus the name lookup's `WAIT`.
// ponytail: one pick when several answer (the order above); the others aren't reported. A name
// is probed at one address (IPv4 preferred): a device reachable only at another isn't found.

use std::io::{self, BufRead, BufReader, Read};
use std::net::{IpAddr, Shutdown, SocketAddr, SocketAddrV4, TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

use ps5_dump_forge_flink::{Conn, MAX_CONTROL_BODY, Me, introduce};
use serde::Serialize;

const WAIT: Duration = Duration::from_millis(1500);
const ASK: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Forge,
    Smb,
    Nfs,
    Ftp,
}

/// What answered: `name` is a Forge's (from its discovery reply or Hello), `None` for a NAS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Probe {
    pub kind: Kind,
    pub host: String,
    pub port: u16,
    pub name: Option<String>,
}

/// The default ports (tests point them at loopback fakes).
pub(crate) struct Ports {
    pub discovery: u16,
    pub flink: u16,
    pub smb: u16,
    pub nfs: u16,
    pub ftp: u16,
}

const PORTS: Ports = Ports {
    discovery: crate::discovery::PORT,
    flink: crate::connect::PORT,
    smb: ps5_dump_forge_smb::DEFAULT_PORT,
    nfs: ps5_dump_forge_nfs::DEFAULT_PORT,
    ftp: crate::nas::FTP_PORT,
};

/// What answers at `address` (`host`, `host:port`, `[v6]:port`), or "nothing to connect to at
/// <address>". A Forge must be on a local network (pairing's rule and text); a NAS needn't.
pub fn probe(address: &str, me: &Me) -> io::Result<Probe> {
    probe_on(address, me, &PORTS)
}

pub(crate) fn probe_on(address: &str, me: &Me, ports: &Ports) -> io::Result<Probe> {
    // Port 0 = none given (`parse_host` refuses a typed 0).
    let (host, given) = crate::nas::parse_host(address, None, 0)?;
    let nothing = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("nothing to connect to at {}", address.trim()),
        )
    };
    // The address keeps an IPv6 scope (`fe80::1%en0`) for every check.
    let base = resolve(&host).ok_or_else(nothing)?;
    let ip = base.ip();
    let at = |port| {
        let mut a = base;
        a.set_port(port);
        a
    };
    let found = |kind, port| Probe {
        kind,
        host: host.clone(),
        port,
        name: None,
    };
    let order = if given == 0 {
        [
            (Kind::Smb, ports.smb),
            (Kind::Nfs, ports.nfs),
            (Kind::Ftp, ports.ftp),
        ]
    } else {
        [(Kind::Ftp, given), (Kind::Nfs, given), (Kind::Smb, given)]
    };
    let (forge, nas) = std::thread::scope(|s| {
        let forge = s.spawn(|| forge(base, given, me, ports));
        let checks: Vec<_> = order
            .iter()
            .map(|&(kind, port)| s.spawn(move || confirm(kind, at(port))))
            .collect();
        let nas: Vec<bool> = checks
            .into_iter()
            .map(|c| c.join().unwrap_or(false))
            .collect();
        (forge.join().ok().flatten(), nas)
    });
    let nas = order
        .iter()
        .zip(nas)
        .find(|(_, ok)| *ok)
        .map(|(&(k, p), _)| found(k, p));
    // Pairing is for a local network only: a Forge elsewhere yields to a NAS answering there.
    let far = forge.is_some() && !crate::connect::is_local(ip);
    let forge = forge.filter(|_| !far);
    // No port: a Forge first (its FLINK port is what Pair dials); a port: what answers there.
    let pick = match (given, forge, nas) {
        (0, Some(f), _) | (_, Some(f), None) => Probe {
            kind: Kind::Forge,
            port: f.0,
            name: Some(f.1),
            // The address checked, so pairing can't resolve the name to another one; a typed
            // IP (with its scope) as it is.
            host: if host.contains(':') || host.parse::<IpAddr>().is_ok() {
                host.clone()
            } else {
                ip.to_string()
            },
        },
        (_, _, Some(n)) => n,
        (_, None, None) if far => return Err(crate::connect::not_local(ip)),
        (_, None, None) => return Err(nothing()),
    };
    Ok(pick)
}

impl Kind {
    /// The spelling of `nas::Protocol` (and of `mdns::Nas::service`).
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Forge => "forge",
            Kind::Smb => "smb",
            Kind::Nfs => "nfs",
            Kind::Ftp => "ftp",
        }
    }
}

/// Discover's check of a host it found: which of SMB, NFS and FTP answer on their default ports
/// (all at once, within about `2 * WAIT`), leaving out `skip` (what mDNS announced there).
pub fn serves(ip: IpAddr, skip: &[Kind]) -> Vec<(Kind, u16)> {
    serves_on(ip, skip, &PORTS)
}

pub(crate) fn serves_on(ip: IpAddr, skip: &[Kind], ports: &Ports) -> Vec<(Kind, u16)> {
    let checks = [
        (Kind::Smb, ports.smb),
        (Kind::Nfs, ports.nfs),
        (Kind::Ftp, ports.ftp),
    ];
    std::thread::scope(|s| {
        let running: Vec<_> = (checks.into_iter())
            .filter(|(kind, _)| !skip.contains(kind))
            .map(|(kind, port)| {
                let check = s.spawn(move || confirm(kind, SocketAddr::new(ip, port)));
                (kind, port, check)
            })
            .collect();
        (running.into_iter())
            .filter_map(|(kind, port, check)| check.join().unwrap_or(false).then_some((kind, port)))
            .collect()
    })
}

/// The host's first address, IPv4 preferred (discovery answers IPv4 only), within `WAIT`.
// ponytail: a lookup past `WAIT` is left running on its own thread (std has no timeout for it).
fn resolve(host: &str) -> Option<SocketAddr> {
    let (tx, rx) = mpsc::channel();
    let host = host.to_string();
    std::thread::spawn(move || {
        let addrs: Vec<SocketAddr> = (host.as_str(), 0)
            .to_socket_addrs()
            .map(Iterator::collect)
            .unwrap_or_default();
        let _ = tx.send(
            addrs
                .iter()
                .find(|a| a.is_ipv4())
                .or(addrs.first())
                .copied(),
        );
    });
    rx.recv_timeout(WAIT).ok().flatten()
}

/// Connects to `addr` within `WAIT` and runs `exchange` on the connection on a thread of its
/// own, given up on and shut down `WAIT` later if it is still running: per-read timeouts alone
/// let a slow server drip bytes, and on Windows a shutdown doesn't end a read that bytes keep
/// coming to.
// ponytail: there a given-up exchange lives on until its own read limits end it.
fn bounded<T: Send + 'static>(
    addr: SocketAddr,
    exchange: impl FnOnce(TcpStream) -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    let tcp = TcpStream::connect_timeout(&addr, WAIT)?;
    let watch = tcp.try_clone()?;
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(exchange(tcp));
    });
    wait.recv_timeout(WAIT).unwrap_or_else(|_| {
        let _ = watch.shutdown(Shutdown::Both);
        Err(io::Error::new(io::ErrorKind::TimedOut, "no answer in time"))
    })
}

/// Whether `kind`'s server answers at `addr`.
fn confirm(kind: Kind, addr: SocketAddr) -> bool {
    match kind {
        Kind::Smb => bounded(addr, |tcp| ps5_dump_forge_smb::probe(tcp, WAIT)).is_ok(),
        Kind::Nfs => bounded(addr, |tcp| ps5_dump_forge_nfs::probe(tcp, WAIT)).is_ok(),
        Kind::Ftp => ftp_banner(addr).is_ok_and(|l| l.starts_with("220")),
        Kind::Forge => false,
    }
}

/// An FTP server's first line (its greeting), at most 512 bytes.
fn ftp_banner(addr: SocketAddr) -> io::Result<String> {
    bounded(addr, |tcp| {
        tcp.set_read_timeout(Some(WAIT))?;
        let mut line = String::new();
        BufReader::new(tcp.take(512)).read_line(&mut line)?;
        Ok(line)
    })
}

/// A Forge at `base` as (FLINK port, name): a FLINK Hello on the port its discovery reply names
/// (which must be `port` when one is given), else on `port` or 9883. Off the local network no
/// Hello is sent (it names this device): the discovery reply alone, for the not-local error.
fn forge(base: SocketAddr, port: u16, me: &Me, ports: &Ports) -> Option<(u16, String)> {
    let ip = base.ip();
    let asked = match ip {
        IpAddr::V4(v4) => crate::discovery::ask(SocketAddrV4::new(v4, ports.discovery), ASK)
            .ok()
            .flatten()
            .filter(|n| n.key_id != me.key_id && (port == 0 || n.port == port)),
        IpAddr::V6(_) => None,
    };
    if !crate::connect::is_local(ip) {
        return asked.map(|n| (n.port, n.name));
    }
    let port = asked.map_or(if port == 0 { ports.flink } else { port }, |n| n.port);
    let mut at = base;
    at.set_port(port);
    let me = me.clone();
    let hello = bounded(at, move |tcp| {
        introduce(&mut Conn::new(tcp, MAX_CONTROL_BODY, WAIT)?, &me)
    });
    Some((port, hello.ok()?.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{Ipv4Addr, TcpListener};

    fn me() -> Me {
        Me {
            key_id: "a".repeat(32),
            name: "Prober".into(),
            platform: "macos".into(),
            port: 9883,
        }
    }

    /// A port nothing listens on (TCP and UDP alike, as far as the test goes).
    fn closed() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    }

    fn none_but(ftp: u16, smb: u16, nfs: u16, flink: u16, discovery: u16) -> Ports {
        Ports {
            discovery,
            flink,
            smb,
            nfs,
            ftp,
        }
    }

    #[test]
    fn a_dripping_server_is_cut_off() {
        // One byte every 300 ms: no read ever times out, yet the probe ends on time.
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    while s.write_all(b"x").is_ok() {
                        std::thread::sleep(Duration::from_millis(300));
                    }
                });
            }
        });
        let none = none_but(closed(), closed(), closed(), closed(), closed());
        let start = std::time::Instant::now();
        assert!(probe_on(&format!("127.0.0.1:{port}"), &me(), &none).is_err());
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn a_forge_by_its_hello_or_its_discovery_reply() {
        let dir = std::env::temp_dir().join(format!("forge-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = crate::connect::Connect::start(&dir, 0).unwrap();
        let port = c.me().port;
        let none = none_but(closed(), closed(), closed(), closed(), closed());
        let p = probe_on(&format!("127.0.0.1:{port}"), &me(), &none).unwrap();
        assert_eq!(p.kind, Kind::Forge);
        assert_eq!(
            (p.port, p.name.as_deref()),
            (port, Some(c.me().name.as_str()))
        );
        // Asking itself: its Hello is refused, nothing else answers.
        assert!(probe_on(&format!("127.0.0.1:{port}"), c.me(), &none).is_err());

        // No port: the discovery reply names the FLINK port, whose Hello confirms the device.
        let mut node = crate::discovery::Node {
            key_id: "b".repeat(32),
            name: "Forge B".into(),
            platform: "ps5".into(),
            port,
        };
        let noop: crate::discovery::OnReply = std::sync::Arc::new(|_, _| {});
        let at = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
        let d = crate::discovery::Discovery::new_on(node.clone(), at, Some(vec![]), noop.clone());
        let udp = d.address().unwrap().port();
        let ports = none_but(closed(), closed(), closed(), closed(), udp);
        let p = probe_on("127.0.0.1", &me(), &ports).unwrap();
        assert_eq!((p.kind, p.port), (Kind::Forge, port));
        assert_eq!(p.name.as_deref(), Some(c.me().name.as_str()));
        assert_eq!(p.host, "127.0.0.1");
        // Another port given: the reply doesn't name it, and no Forge answers there.
        assert!(probe_on(&format!("127.0.0.1:{}", closed()), &me(), &ports).is_err());
        d.shutdown();
        // A reply naming a port no Forge listens on (its FLINK bind failed): not a Forge.
        node.port = closed();
        let d = crate::discovery::Discovery::new_on(node, at, Some(vec![]), noop);
        let ports = none_but(
            closed(),
            closed(),
            closed(),
            closed(),
            d.address().unwrap().port(),
        );
        assert!(probe_on("127.0.0.1", &me(), &ports).is_err());
        d.shutdown();
        c.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
