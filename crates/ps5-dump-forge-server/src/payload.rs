//! The PS5's ELF loader (elfldr) as the desktop app's Dump Forge on PS5 card types it: the
//! address Install on PS5 and Send a payload connect to. The downloading and sending live in
//! `ps5-dump-forge-updater`.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

use ps5_dump_forge_updater::PORT;

/// `host`, `host:port` or `[v6]:port` (port 9021 by default) as the address to connect to, an
/// IPv4 one first.
// ponytail: one address per name; a host name whose IPv4 address doesn't answer isn't tried
// over IPv6 (a console is typed by its IP).
pub fn address(text: &str) -> io::Result<SocketAddr> {
    let (host, port) = crate::nas::parse_host(text, None, PORT)?;
    let addrs: Vec<SocketAddr> = (host.as_str(), port).to_socket_addrs()?.collect();
    (addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or(addrs.first())
        .copied())
    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address")))
}
