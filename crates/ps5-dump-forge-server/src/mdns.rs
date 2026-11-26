//! NAS services by mDNS (RFC 6762): one PTR query for SMB, FTP and NFS to 224.0.0.251:5353
//! from an ephemeral port (legacy unicast: responders answer us directly), then PTR → SRV → A.
use serde::Serialize;
use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
pub const SERVICES: [(&str, &str); 3] = [
    ("_smb._tcp.local", "smb"),
    ("_ftp._tcp.local", "ftp"),
    ("_nfs._tcp.local", "nfs"),
];
const PTR: u16 = 12;
const SRV: u16 = 33;
const A: u16 = 1;
const MAX_FOUND: usize = 64;
/// Records kept per kind in one search: a flood of replies can't grow memory past this.
const MAX_RECORDS: usize = 512;
const RESEND: Duration = Duration::from_secs(3);

/// A NAS service: `name` is the instance's label (`My NAS`), `service` `smb`/`ftp`/`nfs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Nas {
    pub name: String,
    pub service: String,
    pub address: String,
    pub port: u16,
}

/// The records seen so far, by lower-cased owner name.
#[derive(Default)]
pub struct Records {
    ptr: Vec<(String, String)>,

    srv: HashMap<String, (String, u16)>,

    a: HashMap<String, Ipv4Addr>,
    /// Instance → the address its PTR came from (when no A record names one).
    from: HashMap<String, Ipv4Addr>,
}

pub fn query() -> Vec<u8> {
    let mut q = vec![0, 0, 0, 0, 0, SERVICES.len() as u8, 0, 0, 0, 0, 0, 0];
    for (name, _) in SERVICES {
        for label in name.split('.') {
            q.push(label.len() as u8);
            q.extend_from_slice(label.as_bytes());
        }
        q.extend_from_slice(&[0, 0, PTR as u8, 0, 1]);
    }
    q
}

/// A (possibly compressed) name at `at`; returns it and the offset after it in place.
fn name(msg: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut end = None;
    // Bounded jumps: a pointer loop can't spin.
    for _ in 0..64 {
        let len = *msg.get(at)? as usize;
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *msg.get(at + 1)? as usize;
            end.get_or_insert(at + 2);
            at = ptr;
            continue;
        }
        if len == 0 {
            return Some((out, end.unwrap_or(at + 1)));
        }
        let label = msg.get(at + 1..at + 1 + len)?;
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(&String::from_utf8_lossy(label));
        at += 1 + len;
    }
    None
}

fn u16_at(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*msg.get(at)?, *msg.get(at + 1)?]))
}

impl Records {
    pub fn add(&mut self, msg: &[u8], source: Ipv4Addr) -> Option<()> {
        let flags = u16_at(msg, 2)?;
        if flags & 0x8000 == 0 {
            return None; // a query
        }
        let questions = u16_at(msg, 4)?;
        let records = [6, 8, 10]
            .iter()
            .try_fold(0usize, |n, &o| Some(n + u16_at(msg, o)? as usize))?;
        let mut at = 12;
        for _ in 0..questions {
            at = name(msg, at)?.1 + 4;
        }
        for _ in 0..records {
            if [
                self.ptr.len(),
                self.srv.len(),
                self.a.len(),
                self.from.len(),
            ]
            .iter()
            .any(|&n| n >= MAX_RECORDS)
            {
                return Some(());
            }
            let (owner, next) = name(msg, at)?;
            let kind = u16_at(msg, next)?;
            let len = u16_at(msg, next + 8)? as usize;
            let data = next + 10;
            msg.get(data..data + len)?;
            let owner = owner.to_ascii_lowercase();
            match kind {
                PTR => {
                    let target = name(msg, data)?.0;
                    self.from.insert(target.to_ascii_lowercase(), source);
                    if !self.ptr.iter().any(|(o, t)| *o == owner && *t == target) {
                        self.ptr.push((owner, target));
                    }
                }
                SRV => {
                    let port = u16_at(msg, data + 4)?;
                    let host = name(msg, data + 6)?.0.to_ascii_lowercase();
                    self.srv.insert(owner, (host, port));
                }
                A if len == 4 => {
                    let ip = Ipv4Addr::new(msg[data], msg[data + 1], msg[data + 2], msg[data + 3]);
                    self.a.insert(owner, ip);
                }
                _ => {}
            }
            at = data + len;
        }
        Some(())
    }

    /// Every instance found; without SRV/A, the replying address and the service's usual port.
    pub fn candidates(&self) -> Vec<Nas> {
        let mut out = Vec::new();
        for (service, instance) in &self.ptr {
            let Some((_, short)) = SERVICES
                .iter()
                .find(|(s, _)| s.eq_ignore_ascii_case(service))
            else {
                continue;
            };
            let key = instance.to_ascii_lowercase();
            // The owner matched the service ignoring case, so its length is the suffix's.
            let label = instance
                .get(..instance.len().saturating_sub(service.len() + 1))
                .filter(|l| !l.is_empty())
                .unwrap_or(instance);
            let srv = self.srv.get(&key);
            let ip = srv
                .and_then(|(host, _)| self.a.get(host))
                .or_else(|| self.from.get(&key));
            let Some(ip) = ip else { continue };
            let port = srv.map_or(default_port(short), |(_, p)| *p);
            let nas = Nas {
                name: label.to_string(),
                service: short.to_string(),
                address: ip.to_string(),
                port,
            };
            if !out.contains(&nas) {
                out.push(nas);
            }
        }
        out.sort_by(|a, b| (&a.name, &a.service).cmp(&(&b.name, &b.service)));
        out.truncate(MAX_FOUND);
        out
    }
}

/// Address → the services (`nas::Protocol` spelling, port) a probe confirmed there; `None`
/// while its probe runs.
pub type Probed = HashMap<String, Option<Vec<(&'static str, u16)>>>;

/// Every mDNS service as announced, plus what a probe confirmed on each host mDNS or WSD named
/// (mDNS's name first); one entry per host + protocol + port, sorted.
pub fn merge(mdns: &[Nas], wsd: &[Nas], probed: &Probed) -> Vec<Nas> {
    let mut out = mdns.to_vec();
    for (address, confirmed) in probed {
        let Some(confirmed) = confirmed else { continue };
        let Some(named) = mdns.iter().chain(wsd).find(|n| n.address == *address) else {
            continue;
        };
        for &(service, port) in confirmed {
            if !out
                .iter()
                .any(|n| n.address == *address && n.service == service && n.port == port)
            {
                out.push(Nas {
                    name: named.name.clone(),
                    service: service.to_string(),
                    address: address.clone(),
                    port,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        (&a.name, &a.service, &a.address, a.port).cmp(&(&b.name, &b.service, &b.address, b.port))
    });
    out
}

fn default_port(service: &str) -> u16 {
    match service {
        "smb" => 445,
        "ftp" => 21,
        _ => 2049,
    }
}

/// Queries until `until` or `stop`, calling `found` with the whole list each time it changes.
pub fn search(until: Instant, stop: &AtomicBool, found: &dyn Fn(Vec<Nas>)) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let _ = socket.set_multicast_ttl_v4(255);
    let q = query();
    let mut records = Records::default();
    // Published on every change: a later SRV or A record may correct an address or port.
    let mut shown = Vec::new();
    let mut next = Instant::now();
    let mut buf = [0u8; 9000];
    while Instant::now() < until && !stop.load(Ordering::Acquire) {
        if Instant::now() >= next {
            socket.send_to(&q, GROUP)?;
            next = Instant::now() + RESEND;
        }
        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::ConnectionReset
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        let std::net::SocketAddr::V4(from) = from else {
            continue;
        };
        if records.add(&buf[..len], *from.ip()).is_some() {
            let list = records.candidates();
            if list != shown {
                shown = list.clone();
                found(list);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_name(m: &mut Vec<u8>, name: &str) {
        for label in name.split('.') {
            m.push(label.len() as u8);
            m.extend_from_slice(label.as_bytes());
        }
        m.push(0);
    }

    fn record(m: &mut Vec<u8>, owner: &[u8], kind: u16, data: &[u8]) {
        m.extend_from_slice(owner);
        m.extend_from_slice(&kind.to_be_bytes());
        m.extend_from_slice(&[0x80, 1, 0, 0, 0x11, 0x94]);
        m.extend_from_slice(&(data.len() as u16).to_be_bytes());
        m.extend_from_slice(data);
    }

    #[test]
    fn query_and_answer_parse() {
        let q = query();
        assert_eq!(&q[4..6], &[0, 3]);
        assert!(name(&q, 12).unwrap().0 == "_smb._tcp.local");

        // SMB by PTR/SRV/A, FTP on 2121 by PTR + SRV, an FTP PTR with no SRV/A (the sender, 21).
        let named = |n: &str| {
            let mut o = Vec::new();
            put_name(&mut o, n);
            o
        };
        let mut m = vec![0, 0, 0x84, 0, 0, 0, 0, 6, 0, 0, 0, 0];
        let mut instance = vec![8];
        instance.extend_from_slice(b"Home NAS");
        instance.extend_from_slice(&(12u16 | 0xC000).to_be_bytes());
        record(&mut m, &named("_smb._tcp.local"), PTR, &instance);
        let instance_at = (m.len() - instance.len()) as u16 | 0xC000;
        let mut srv = vec![0, 0, 0, 0, 1, 0xBD];
        put_name(&mut srv, "nas.local");
        record(&mut m, &instance_at.to_be_bytes(), SRV, &srv);
        record(&mut m, &named("NAS.local"), A, &[192, 168, 1, 20]);
        let ftp = named("Home NAS._ftp._tcp.local");
        record(&mut m, &named("_ftp._tcp.local"), PTR, &ftp);
        let mut srv = vec![0, 0, 0, 0, 0x08, 0x49];
        put_name(&mut srv, "nas.local");
        record(&mut m, &ftp, SRV, &srv);
        let old = named("Old Mac._ftp._tcp.local");
        record(&mut m, &named("_FTP._tcp.local"), PTR, &old);

        let mut r = Records::default();
        r.add(&m, Ipv4Addr::new(192, 168, 1, 7)).unwrap();
        let nas = |name: &str, service: &str, address: &str, port| Nas {
            name: name.into(),
            service: service.into(),
            address: address.into(),
            port,
        };
        assert_eq!(
            r.candidates(),
            vec![
                nas("Home NAS", "ftp", "192.168.1.20", 2121),
                nas("Home NAS", "smb", "192.168.1.20", 445),
                nas("Old Mac", "ftp", "192.168.1.7", 21),
            ]
        );
        // Our own query (no response bit), a cut message and a pointer loop are refused.
        assert!(Records::default().add(&q, Ipv4Addr::LOCALHOST).is_none());
        assert!(
            Records::default()
                .add(&m[..m.len() - 3], Ipv4Addr::LOCALHOST)
                .is_none()
        );
        let looped = [0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0xC0, 12];
        assert!(
            Records::default()
                .add(&looped, Ipv4Addr::LOCALHOST)
                .is_none()
        );
    }
}
