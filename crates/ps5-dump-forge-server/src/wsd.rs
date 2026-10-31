//! Windows PCs (and Samba boxes running wsdd) by WS-Discovery: a SOAP Probe for `wsdp:Device` to
//! 239.255.255.250:3702 from an ephemeral port (responders answer us directly), keeping the
//! matches whose Types name `pub:Computer`; a match without XAddrs gets a Resolve. Each one's
//! name comes from one WS-Transfer Get to its XAddr (`http://<ip>:5357/<uuid>`, `pub:Computer`
//! `DESKTOP-AB12/Workgroup:WORKGROUP` → `DESKTOP-AB12`), else its IP. Listed as an SMB NAS on 445.
use crate::mdns::Nas;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 3702);
const MAX_FOUND: usize = 64;
const MAX_REPLY: u64 = 64 * 1024;
const RESEND: Duration = Duration::from_secs(3);
const GET_TIMEOUT: Duration = Duration::from_secs(2);
const ENVELOPE: &str = r#"<?xml version="1.0" encoding="utf-8"?><soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:wsd="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:wsdp="http://schemas.xmlsoap.org/ws/2006/02/devprof">"#;
const DISCOVERY: &str = "http://schemas.xmlsoap.org/ws/2005/04/discovery";

/// One ProbeMatch or ResolveMatch.
#[derive(Debug, PartialEq, Eq)]
pub struct Match {
    pub endpoint: String,
    pub computer: bool,
    pub xaddr: Option<XAddr>,
}

/// An `http://<IPv4>[:port]/path` XAddr.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XAddr {
    pub host: Ipv4Addr,
    pub port: u16,
    pub path: String,
}

/// A fresh `urn:uuid:` (v4).
fn message_id() -> io::Result<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(io::Error::other)?;
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

fn message(to: &str, action: &str, id: &str, body: &str) -> String {
    format!(
        "{ENVELOPE}<soap:Header><wsa:To>{}</wsa:To><wsa:Action>{action}</wsa:Action>\
         <wsa:MessageID>{id}</wsa:MessageID><wsa:ReplyTo><wsa:Address>\
         http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:Address>\
         </wsa:ReplyTo></soap:Header><soap:Body>{body}</soap:Body></soap:Envelope>",
        escape(to)
    )
}

pub fn probe(id: &str) -> String {
    let body = "<wsd:Probe><wsd:Types>wsdp:Device</wsd:Types></wsd:Probe>";
    message(
        "urn:schemas-xmlsoap-org:ws:2005:04:discovery",
        &format!("{DISCOVERY}/Probe"),
        id,
        body,
    )
}

fn resolve(id: &str, endpoint: &str) -> String {
    let body = format!(
        "<wsd:Resolve><wsa:EndpointReference><wsa:Address>{}</wsa:Address>\
         </wsa:EndpointReference></wsd:Resolve>",
        escape(endpoint)
    );
    message(
        "urn:schemas-xmlsoap-org:ws:2005:04:discovery",
        &format!("{DISCOVERY}/Resolve"),
        id,
        &body,
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape(s: &str) -> String {
    s.trim()
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text inside each `<prefix:local …>…</prefix:local>` (any prefix), in order. A tiny
/// extractor, not a parser: no CDATA, and a same-named element inside another ends it early.
pub fn elements<'a>(xml: &'a str, local: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        let tag = &rest[i + 1..];
        let end = tag
            .find(|c: char| c == '>' || c == '/' || c.is_whitespace())
            .unwrap_or(tag.len());
        let full = &tag[..end];
        rest = &tag[end..];
        if full.rsplit(':').next() != Some(local) {
            continue;
        }
        let Some(open) = rest.find('>') else { break };
        if rest[..open].ends_with('/') {
            out.push("");
            rest = &rest[open + 1..];
            continue;
        }
        let body = &rest[open + 1..];
        let close = format!("</{full}>");
        let Some(len) = body.find(&close) else { break };
        out.push(&body[..len]);
        rest = &body[len + close.len()..];
    }
    out
}

fn first(xml: &str, local: &str) -> Option<String> {
    elements(xml, local).first().map(|t| unescape(t))
}

/// `http://<IPv4>[:port]/path`; anything else (IPv6, a host name, https) is `None`.
pub fn xaddr(url: &str) -> Option<XAddr> {
    let rest = url.strip_prefix("http://")?;
    let (hostport, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (hostport, 80),
    };
    let path = if path.is_empty() { "/" } else { path };
    // It goes into a request line: no spaces or control characters.
    if path.bytes().any(|b| b <= b' ' || b == 0x7F) {
        return None;
    }
    Some(XAddr {
        host: host.parse().ok()?,
        port,
        path: path.to_string(),
    })
}

/// The matches of a ProbeMatches or ResolveMatches message answering one of `ours`.
pub fn matches(xml: &str, ours: &[String]) -> Vec<Match> {
    let related = first(xml, "RelatesTo");
    if !related.is_some_and(|r| ours.contains(&r)) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for kind in ["ProbeMatch", "ResolveMatch"] {
        for m in elements(xml, kind) {
            let Some(endpoint) = elements(m, "EndpointReference")
                .first()
                .and_then(|e| first(e, "Address"))
                .filter(|e| !e.is_empty() && e.len() <= 256)
            else {
                continue;
            };
            let types = first(m, "Types").unwrap_or_default();
            let computer = types
                .split_whitespace()
                .any(|t| t.rsplit(':').next() == Some("Computer"));
            let xaddr = first(m, "XAddrs")
                .unwrap_or_default()
                .split_whitespace()
                .find_map(xaddr);
            out.push(Match {
                endpoint,
                computer,
                xaddr,
            });
        }
    }
    out
}

/// The computer's name in a Get response: `pub:Computer` up to its `/Workgroup:` or `/Domain:`.
pub fn computer_name(xml: &str) -> Option<String> {
    let value = first(xml, "Computer")?;
    let name: String = value
        .split('/')
        .next()?
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// One WS-Transfer Get to `at`, done by `deadline` (a peer dripping its reply can't hold the
/// search past it) and at most 64 KiB read.
/// ponytail: a chunked reply is searched as it came; a chunk line inside the element would hide it.
fn get_name(at: &XAddr, endpoint: &str, deadline: Instant) -> Option<String> {
    let body = message(
        endpoint,
        "http://schemas.xmlsoap.org/ws/2004/09/transfer/Get",
        &message_id().ok()?,
        "",
    );
    let addr = SocketAddr::from((at.host, at.port));
    // Connect and send within what is left too: a host dropping the connection silently can't
    // hold the search (a reply may list 64 of them) past `deadline`.
    let left = || Some(deadline.saturating_duration_since(Instant::now())).filter(|d| !d.is_zero());
    let mut s = TcpStream::connect_timeout(&addr, left()?).ok()?;
    s.set_write_timeout(Some(left()?)).ok()?;
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/soap+xml\r\n\
         User-Agent: wsd\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        at.path,
        at.host,
        at.port,
        body.len()
    );
    s.write_all(head.as_bytes()).ok()?;
    s.write_all(body.as_bytes()).ok()?;
    let mut reply = Vec::new();
    let mut buf = [0u8; 4096];
    // A timeout or the deadline after part of the reply still leaves what came.
    while (reply.len() as u64) < MAX_REPLY {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || s.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => reply.extend_from_slice(&buf[..n]),
        }
    }
    let reply = String::from_utf8_lossy(&reply);
    if !reply.starts_with("HTTP/1.1 200") && !reply.starts_with("HTTP/1.0 200") {
        return None;
    }
    computer_name(reply.split_once("\r\n\r\n")?.1)
}

/// Probes until `until` (again every `RESEND`) or `stop`, calling `found` with the whole list
/// each time it grows.
pub fn search(until: Instant, stop: &AtomicBool, found: &dyn Fn(Vec<Nas>)) -> io::Result<()> {
    search_on(GROUP, until, stop, found)
}

pub fn search_on(
    target: SocketAddrV4,
    until: Instant,
    stop: &AtomicBool,
    found: &dyn Fn(Vec<Nas>),
) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    // Each send has its own id: a responder drops a repeated MessageID.
    let mut ours = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut resolving: Vec<String> = Vec::new();
    let mut list: Vec<Nas> = Vec::new();
    let mut next = Instant::now();
    let mut buf = vec![0u8; 16 * 1024];
    while Instant::now() < until && !stop.load(Ordering::Acquire) {
        if Instant::now() >= next {
            let id = message_id()?;
            socket.send_to(probe(&id).as_bytes(), target)?;
            ours.push(id);
            next = Instant::now() + RESEND;
        }
        let len = match socket.recv_from(&mut buf) {
            Ok((len, _)) => len,
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
        let xml = String::from_utf8_lossy(&buf[..len]).into_owned();
        for m in matches(&xml, &ours) {
            if seen.contains(&m.endpoint) || list.len() >= MAX_FOUND {
                continue;
            }
            // A Resolve's match is a computer we already probed.
            if !m.computer && !resolving.contains(&m.endpoint) {
                continue;
            }
            let Some(at) = m.xaddr else {
                if !resolving.contains(&m.endpoint) && resolving.len() < MAX_FOUND {
                    let id = message_id()?;
                    socket.send_to(resolve(&id, &m.endpoint).as_bytes(), target)?;
                    ours.push(id);
                    resolving.push(m.endpoint);
                }
                continue;
            };
            seen.push(m.endpoint.clone());
            let address = at.host.to_string();
            if list.iter().any(|n| n.address == address) {
                continue;
            }
            // Not past the search's end or a Stop: the address names it then.
            let deadline = until.min(Instant::now() + GET_TIMEOUT);
            let name = (!stop.load(Ordering::Acquire))
                .then(|| get_name(&at, &m.endpoint, deadline))
                .flatten()
                .unwrap_or_else(|| address.clone());
            list.push(Nas {
                name,
                service: "smb".into(),
                address,
                port: 445,
            });
            list.sort_by(|a, b| (&a.name, &a.address).cmp(&(&b.name, &b.address)));
            found(list.clone());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const NS: &str = r#"xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:wsd="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:wsdp="http://schemas.xmlsoap.org/ws/2006/02/devprof" xmlns:pub="http://schemas.microsoft.com/windows/pub/2005/07""#;

    /// A reply as Windows' FDResPub sends it, answering `id`.
    fn reply(id: &str, action: &str, body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8" ?><soap:Envelope {NS}><soap:Header><wsa:To>http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:To><wsa:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/{action}</wsa:Action><wsa:MessageID>urn:uuid:9f1c2c6e-0d3a-4b1e-8a51-2f0f6f1d3e01</wsa:MessageID><wsa:RelatesTo>{id}</wsa:RelatesTo><wsd:AppSequence InstanceId="17" SequenceId="urn:uuid:0d8e3a4f-5a52-4c43-9f0e-4b1d2c3a4b5c" MessageNumber="4"></wsd:AppSequence></soap:Header><soap:Body>{body}</soap:Body></soap:Envelope>"#
        )
    }

    fn one(kind: &str, endpoint: &str, types: &str, xaddrs: &str) -> String {
        let xaddrs = if xaddrs.is_empty() {
            String::new()
        } else {
            format!("<wsd:XAddrs>{xaddrs}</wsd:XAddrs>")
        };
        format!(
            "<wsd:{kind}><wsa:EndpointReference><wsa:Address>{endpoint}</wsa:Address></wsa:EndpointReference><wsd:Types>{types}</wsd:Types>{xaddrs}<wsd:MetadataVersion>2</wsd:MetadataVersion></wsd:{kind}>"
        )
    }

    const EP: &str = "urn:uuid:4a6bd5e2-7c11-4f0e-9d2b-001122334455";

    fn get_response() -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?><soap:Envelope {NS} xmlns:wsx="http://schemas.xmlsoap.org/ws/2004/09/mex"><soap:Header><wsa:To>http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:To><wsa:Action>http://schemas.xmlsoap.org/ws/2004/09/transfer/GetResponse</wsa:Action><wsa:MessageID>urn:uuid:1</wsa:MessageID><wsa:RelatesTo>urn:uuid:2</wsa:RelatesTo></soap:Header><soap:Body><wsx:Metadata><wsx:MetadataSection Dialect="http://schemas.xmlsoap.org/ws/2006/02/devprof/ThisDevice"><wsdp:ThisDevice><wsdp:FriendlyName xml:lang="en-US">Microsoft Publication Service Device Host</wsdp:FriendlyName><wsdp:FirmwareVersion>1.0</wsdp:FirmwareVersion><wsdp:SerialNumber>20050718</wsdp:SerialNumber></wsdp:ThisDevice></wsx:MetadataSection><wsx:MetadataSection Dialect="http://schemas.xmlsoap.org/ws/2006/02/devprof/Relationship"><wsdp:Relationship Type="http://schemas.xmlsoap.org/ws/2006/02/devprof/host"><wsdp:Host><wsa:EndpointReference><wsa:Address>{EP}</wsa:Address></wsa:EndpointReference><wsdp:Types>pub:Computer</wsdp:Types><wsdp:ServiceId>{EP}</wsdp:ServiceId><pub:Computer>DESKTOP-AB12/Workgroup:WORKGROUP</pub:Computer></wsdp:Host></wsdp:Relationship></wsx:MetadataSection></wsx:Metadata></soap:Body></soap:Envelope>"#
        )
    }

    #[test]
    fn search_on_loopback() {
        let responder = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        responder
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let SocketAddr::V4(target) = responder.local_addr().unwrap() else {
            unreachable!()
        };
        let http = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let http_port = http.local_addr().unwrap().port();
        let list = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (l, s) = (list.clone(), stop.clone());
        let searcher = std::thread::spawn(move || {
            let found = |v: Vec<Nas>| {
                *l.lock().unwrap() = v;
                s.store(true, Ordering::Release);
            };
            search_on(target, Instant::now() + Duration::from_secs(8), &s, &found)
        });

        // The Probe: answered by garbage, a printer and a computer without XAddrs (as wsdd does).
        let mut buf = vec![0u8; 16 * 1024];
        let (len, from) = responder.recv_from(&mut buf).unwrap();
        let probe = String::from_utf8_lossy(&buf[..len]).into_owned();
        assert!(probe.contains("/discovery/Probe<") && probe.contains("wsdp:Device"));
        let id = first(&probe, "MessageID").unwrap();
        responder.send_to(b"\xff\x00 not xml", from).unwrap();
        let printer = one(
            "ProbeMatch",
            "urn:uuid:printer",
            "wsdp:Device wprt:PrintDeviceType",
            "http://127.0.0.1:1/x",
        );
        let pc = one("ProbeMatch", EP, "wsdp:Device pub:Computer", "");
        let body = format!("<wsd:ProbeMatches>{printer}{pc}</wsd:ProbeMatches>");
        responder
            .send_to(reply(&id, "ProbeMatches", &body).as_bytes(), from)
            .unwrap();

        // Its Resolve (a repeated Probe may come first), answered with the XAddr.
        let resolve = loop {
            let (len, _) = responder.recv_from(&mut buf).unwrap();
            let m = String::from_utf8_lossy(&buf[..len]).into_owned();
            if m.contains("/discovery/Resolve<") {
                break m;
            }
        };
        assert!(resolve.contains(EP));
        let rid = first(&resolve, "MessageID").unwrap();
        let xaddrs = format!("http://127.0.0.1:{http_port}/4a6bd5e2");
        let body = format!(
            "<wsd:ResolveMatches>{}</wsd:ResolveMatches>",
            one("ResolveMatch", EP, "wsdp:Device pub:Computer", &xaddrs)
        );
        responder
            .send_to(reply(&rid, "ResolveMatches", &body).as_bytes(), from)
            .unwrap();

        // The Get.
        let (mut conn, _) = http.accept().unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut req = Vec::new();
        let mut chunk = [0u8; 4096];
        while !String::from_utf8_lossy(&req).contains("</soap:Envelope>") {
            let n = conn.read(&mut chunk).unwrap();
            assert!(n > 0);
            req.extend_from_slice(&chunk[..n]);
        }
        let req = String::from_utf8_lossy(&req);
        assert!(req.starts_with("POST /4a6bd5e2 HTTP/1.1\r\n"));
        assert!(req.contains("transfer/Get<") && req.contains(EP));
        let body = get_response();
        write!(
            conn,
            "HTTP/1.1 200 \r\nContent-Type: application/soap+xml\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(conn);

        searcher.join().unwrap().unwrap();
        assert_eq!(
            *list.lock().unwrap(),
            vec![Nas {
                name: "DESKTOP-AB12".into(),
                service: "smb".into(),
                address: "127.0.0.1".into(),
                port: 445
            }]
        );
    }
}
