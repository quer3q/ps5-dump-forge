//! A strict HTTP/1.1 subset: one request per connection, origin-form targets, `GET` and
//! `POST`, bodies by `Content-Length` only. Anything else is refused rather than guessed at.
//! A reply is a body in memory, or a download its own writer streams after the head.

use std::borrow::Cow;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

pub(crate) const HEAD_MAX: usize = 16 * 1024;
pub(crate) const BODY_MAX: usize = 1024 * 1024;

/// A request line and its headers (names lowercased), with the bytes read past them.
pub(crate) struct Head {
    pub method: String,
    /// The target without its query.
    pub path: String,
    /// The query, still percent-encoded (`""` without one).
    pub query: String,
    headers: Vec<(String, String)>,
    pub length: usize,
    rest: Vec<u8>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) struct Response {
    pub status: u16,
    kind: &'static str,
    body: Cow<'static, [u8]>,
    pub retry_after: Option<u64>,
    /// A static file's `Cache-Control` (API replies always get `no-store`).
    cache: Option<&'static str>,
    /// Runs once the response is sent (quit).
    pub after: Option<Box<dyn FnOnce() + Send>>,
    /// A download's file name and bytes, streamed after the head instead of `body`.
    /// Boxed: most replies carry none, and every `Err(Response)` stays small.
    download: Option<Box<Download>>,
}

/// Writes a download's body: exactly its `len` bytes into the writer it is given.
pub(crate) type Body = Box<dyn FnOnce(&mut dyn Write) -> std::io::Result<()> + Send>;

struct Download {
    name: String,
    len: u64,
    body: Option<Body>,
}

impl Response {
    pub fn json(value: &impl serde::Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self::new(200, JSON, Cow::Owned(body)),
            Err(e) => Self::error(500, format!("internal error: {e}")),
        }
    }

    /// An SVG drawn per request (`GET /api/qr`).
    pub fn svg(body: String) -> Self {
        Self::new(200, "image/svg+xml", Cow::Owned(body.into_bytes()))
    }

    /// `{"error": text}`, the shape of every refusal.
    pub fn error(status: u16, text: impl std::fmt::Display) -> Self {
        let body = serde_json::json!({ "error": text.to_string() }).to_string();
        Self::new(status, JSON, Cow::Owned(body.into_bytes()))
    }

    /// Vite names `assets/*` by content hash, so a new build never reuses a name: those are
    /// cached for good. Everything else (`index.html`, the AppCache manifest `forge.appcache`)
    /// is checked on every load, so reloading the payload always shows its own UI (found on
    /// the PS5 browser, which kept an old page).
    pub fn asset(name: &str, bytes: &'static [u8]) -> Self {
        let mut r = Self::new(200, content_type(name), Cow::Borrowed(bytes));
        r.cache = Some(if name.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        });
        r
    }

    /// `len` bytes, written by `body` after the head, as an attachment named `name` (any
    /// text: [`disposition`] makes the header safe).
    pub fn download(kind: &'static str, name: &str, len: u64, body: Body) -> Self {
        let mut r = Self::new(200, kind, Cow::Borrowed(b""));
        r.download = Some(Box::new(Download {
            name: name.to_string(),
            len,
            body: Some(body),
        }));
        r
    }

    pub fn limited(text: &str, retry_after: u64) -> Self {
        let mut r = Self::error(429, text);
        r.retry_after = Some(retry_after);
        r
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn new(status: u16, kind: &'static str, body: Cow<'static, [u8]>) -> Self {
        Self {
            status,
            kind,
            body,
            retry_after: None,
            cache: None,
            after: None,
            download: None,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            409 => "Conflict",
            413 => "Content Too Large",
            429 => "Too Many Requests",
            503 => "Service Unavailable",
            _ => "Internal Server Error",
        };
        let len = match &self.download {
            Some(d) => d.len,
            None => self.body.len() as u64,
        };
        let mut head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {len}\r\n\
             Connection: close\r\n",
            self.status, self.kind,
        );
        if let Some(d) = &self.download {
            head += &format!("Content-Disposition: {}\r\n", disposition(&d.name));
        }
        // API replies: the page must never see a stale one.
        if self.kind == JSON || self.download.is_some() {
            head += "Cache-Control: no-store\r\n";
        } else if let Some(cache) = self.cache {
            head += &format!("Cache-Control: {cache}\r\n");
        }
        if let Some(secs) = self.retry_after {
            head += &format!("Retry-After: {secs}\r\n");
        }
        head += "\r\n";
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

const JSON: &str = "application/json";

fn bad(text: &str) -> Response {
    Response::error(400, text)
}

fn content_type(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "appcache" => "text/cache-manifest",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// One read that gives up at `deadline`, however slowly the bytes trickle in.
fn read_some(stream: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Result<usize, Response> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(bad("the request took too long"));
        }
        stream
            .set_read_timeout(Some(left))
            .map_err(|e| bad(&format!("reading the request: {e}")))?;
        return match stream.read(buf) {
            Ok(0) => Err(bad("the request ended early")),
            Ok(n) => Ok(n),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Err(bad("the request took too long"))
            }
            Err(e) => Err(bad(&format!("reading the request: {e}"))),
        };
    }
}

pub(crate) fn read_head(stream: &mut TcpStream, deadline: Instant) -> Result<Head, Response> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            if end + 4 > HEAD_MAX {
                break;
            }
            let mut head = parse_head(&buf[..end])?;
            head.rest = buf.split_off(end + 4);
            return Ok(head);
        }
        if buf.len() >= HEAD_MAX {
            break;
        }
        let n = read_some(stream, &mut chunk, deadline)?;
        buf.extend_from_slice(&chunk[..n]);
    }
    Err(Response::error(413, "the request head is over 16 KiB"))
}

/// The head without its final blank line.
pub(crate) fn parse_head(bytes: &[u8]) -> Result<Head, Response> {
    let text = std::str::from_utf8(bytes).map_err(|_| bad("the request head is not UTF-8"))?;
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad("malformed request line"));
    };
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(bad("only HTTP/1.x is served"));
    }
    if !matches!(method, "GET" | "POST") {
        return Err(bad("only GET and POST are served"));
    }
    if !target.starts_with('/') || !target.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(bad("malformed request target"));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    let mut headers = Vec::new();
    for line in lines {
        // A stray CR or LF, or an obsolete folded continuation line.
        if line.contains(['\r', '\n']) || line.starts_with([' ', '\t']) {
            return Err(bad("malformed header line"));
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(bad("malformed header line"));
        };
        let value = value.trim_matches([' ', '\t']);
        if name.is_empty()
            || !name.bytes().all(tchar)
            || value.chars().any(|c| c.is_control() && c != '\t')
        {
            return Err(bad("malformed header line"));
        }
        headers.push((name.to_ascii_lowercase(), value.to_string()));
    }
    let count = |name: &str| headers.iter().filter(|(n, _)| n == name).count();
    if count("transfer-encoding") > 0 {
        return Err(bad(
            "Transfer-Encoding is not accepted; send Content-Length",
        ));
    }
    for name in ["host", "content-length"] {
        if count(name) > 1 {
            return Err(bad(&format!("duplicate {name} header")));
        }
    }
    let mut head = Head {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers,
        length: 0,
        rest: Vec::new(),
    };
    if let Some(length) = head.header("content-length") {
        if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad("malformed Content-Length"));
        }
        head.length = length.parse().unwrap_or(usize::MAX);
        if head.length > BODY_MAX {
            return Err(Response::error(413, "the request body is over 1 MiB"));
        }
    }
    Ok(head)
}

pub(crate) fn read_body(
    stream: &mut TcpStream,
    head: &mut Head,
    deadline: Instant,
) -> Result<Vec<u8>, Response> {
    let mut body = std::mem::take(&mut head.rest);
    body.truncate(head.length);
    let mut chunk = [0u8; 8192];
    while body.len() < head.length {
        let want = chunk.len().min(head.length - body.len());
        let n = read_some(stream, &mut chunk[..want], deadline)?;
        body.extend_from_slice(&chunk[..n]);
    }
    Ok(body)
}

/// Writes the response within `timeout`, however slowly the client reads, and closes; a
/// download gets `timeout` for each write of its body instead (a GiB journal takes minutes, a
/// client that stops reading still loses its thread). A download whose body fails part way is
/// cut short: the client sees fewer bytes than `Content-Length` says, and marks it failed. Unread request bytes are
/// drained for a moment first, so the close doesn't reset the connection before the client
/// has read the response.
pub(crate) fn send(stream: &mut TcpStream, response: &mut Response, timeout: Duration) {
    if write_by(stream, &response.bytes(), Instant::now() + timeout).is_ok()
        && let Some(d) = response.download.as_mut()
        && let Err(e) = stream_body(stream, d, timeout)
    {
        eprintln!("ps5-dump-forge serve: sending {}: {e}", d.name);
    }
    let _ = stream.shutdown(Shutdown::Write);
    let until = Instant::now() + Duration::from_secs(1);
    let mut sink = [0u8; 8192];
    let mut drained = 0;
    while drained < BODY_MAX && Instant::now() < until {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
        match stream.read(&mut sink) {
            Ok(0) | Err(_) => break,
            Ok(n) => drained += n,
        }
    }
}

/// A download's body: its writer, held to exactly `len` bytes.
fn stream_body(stream: &mut TcpStream, d: &mut Download, timeout: Duration) -> std::io::Result<()> {
    let body = d.body.take().ok_or(ErrorKind::Other)?;
    let mut sink = Sink {
        stream,
        timeout,
        left: d.len,
    };
    body(&mut sink)?;
    match sink.left {
        0 => Ok(()),
        left => Err(std::io::Error::other(format!("{left} bytes short"))),
    }
}

/// The socket under a download: each write by its own deadline, never past `Content-Length`.
struct Sink<'a> {
    stream: &'a mut TcpStream,
    timeout: Duration,
    left: u64,
}

impl Write for Sink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() as u64 > self.left {
            return Err(std::io::Error::other("more bytes than Content-Length"));
        }
        write_by(self.stream, buf, Instant::now() + self.timeout)?;
        self.left -= buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `attachment; filename="..."` with an ASCII stand-in (`_` for anything else, a quote, a
/// backslash or a control character), plus `filename*=UTF-8''...` percent-encoded when the
/// name isn't plain ASCII (RFC 6266): no header injection whatever the name holds.
fn disposition(name: &str) -> String {
    let plain = |c: char| c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ';
    let ascii: String = name
        .chars()
        .map(|c| if plain(c) { c } else { '_' })
        .collect();
    let mut header = format!("attachment; filename=\"{ascii}\"");
    if ascii != name {
        let attr = |b: u8| b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b);
        let encoded: String = name
            .bytes()
            .map(|b| match attr(b) {
                true => (b as char).to_string(),
                false => format!("%{b:02X}"),
            })
            .collect();
        header += &format!("; filename*=UTF-8''{encoded}");
    }
    header
}

/// Decodes `%XX` escapes (`+` stays `+`, as `encodeURIComponent` writes a space `%20`);
/// `None` for a bad escape or text that isn't UTF-8.
pub(crate) fn percent_decode(text: &str) -> Option<String> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b != b'%' {
            out.push(b);
            continue;
        }
        let hex = [bytes.next()?, bytes.next()?];
        if !hex.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
    }
    String::from_utf8(out).ok()
}

/// The decoded value of `key` in a query (`a=1&b=2`); the first one wins.
pub(crate) fn query_value(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| percent_decode(v))
}

/// `write_all` with one absolute deadline: the timeout is what is left of it before each
/// write, so a client reading a trickle can't hold the thread write after write.
fn write_by(stream: &mut TcpStream, bytes: &[u8], deadline: Instant) -> std::io::Result<()> {
    write_until(stream, bytes, deadline, |s, left| {
        s.set_write_timeout(Some(left))
    })
}

/// [`write_by`] over any writer, `timeout` setting the per-write limit (unit-tested with a
/// writer that takes a byte at a time; a socket's kernel buffer would hide the deadline).
fn write_until<W: Write>(
    w: &mut W,
    mut bytes: &[u8],
    deadline: Instant,
    mut timeout: impl FnMut(&mut W, Duration) -> std::io::Result<()>,
) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(ErrorKind::TimedOut.into());
        }
        timeout(w, left)?;
        match w.write(bytes) {
            Ok(0) => return Err(ErrorKind::WriteZero.into()),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Over the handler cap: an immediate 503 from the accepting thread, never a blocking write.
pub(crate) fn busy(stream: TcpStream) {
    let mut stream = stream;
    let _ = stream.set_nonblocking(true);
    let _ = stream.write_all(&Response::error(503, "too many connections; try again").bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_is_rechecked_and_hashed_assets_are_cached() {
        let head = |r: Response| String::from_utf8_lossy(&r.bytes()).into_owned();
        assert!(
            head(Response::asset("index.html", b"<html>")).contains("Cache-Control: no-cache\r\n")
        );
        assert!(head(Response::asset("assets/index-1a2b.js", b"x")).contains("immutable"));
        let manifest = head(Response::asset("forge.appcache", b"CACHE MANIFEST\n"));
        assert!(
            manifest.contains("Content-Type: text/cache-manifest\r\n"),
            "{manifest}"
        );
        assert!(
            manifest.contains("Cache-Control: no-cache\r\n"),
            "{manifest}"
        );
        assert!(head(Response::json(&1)).contains("Cache-Control: no-store\r\n"));
    }

    /// Takes one byte per write, a little late each time: always progress, never done in time.
    struct Trickle(Vec<u8>);

    impl Write for Trickle {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(Duration::from_millis(5));
            self.0.push(buf[0]);
            Ok(1)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_write_deadline_is_absolute() {
        let mut w = Trickle(Vec::new());
        let mut limits = Vec::new();
        let start = Instant::now();
        let err = write_until(
            &mut w,
            &[7; 1000],
            start + Duration::from_millis(60),
            |_, left| {
                limits.push(left);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::TimedOut);
        // A per-write timeout would have let all 1000 bytes through (5 s); this stops near 60 ms.
        assert!(w.0.len() < 100, "{} bytes", w.0.len());
        assert!(start.elapsed() < Duration::from_millis(500));
        // Each write gets what is left of the one deadline, never a fresh allowance.
        assert!(limits.windows(2).all(|l| l[1] < l[0]));
        // A writer that keeps up finishes.
        let mut fast = Vec::new();
        write_until(
            &mut fast,
            b"done",
            Instant::now() + Duration::from_secs(1),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(fast, b"done");
    }

    #[test]
    fn queries_and_downloads() {
        assert_eq!(
            percent_decode("a%2Fb%20c+d%C3%A9").as_deref(),
            Some("a/b c+dé")
        );
        for bad in ["%", "%2", "%zz", "%+1", "%FF"] {
            assert_eq!(percent_decode(bad), None, "{bad}");
        }
        let q = "source=%2Fdata%2Fg&name=ampr_emu.index&name=x&flag";
        assert_eq!(query_value(q, "source").as_deref(), Some("/data/g"));
        assert_eq!(query_value(q, "name").as_deref(), Some("ampr_emu.index"));
        assert_eq!(query_value(q, "flag"), None);
        let head = parse("GET /api/x?a=1 HTTP/1.1").unwrap();
        assert_eq!((head.path.as_str(), head.query.as_str()), ("/api/x", "a=1"));

        let r = Response::download(
            "application/octet-stream",
            "t.bin",
            5,
            Box::new(|w| w.write_all(b"hello")),
        );
        let head = String::from_utf8(r.bytes()).unwrap();
        for line in [
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/octet-stream\r\n",
            "Content-Length: 5\r\n",
            "Content-Disposition: attachment; filename=\"t.bin\"\r\n",
            "Cache-Control: no-store\r\n",
        ] {
            assert!(head.contains(line), "{head}");
        }
        assert!(
            head.ends_with("\r\n\r\n"),
            "the body is streamed after the head"
        );
        assert_eq!(
            disposition("[Stellar Blade]-[PPSA13197]-amprtrace.zip"),
            "attachment; filename=\"[Stellar Blade]-[PPSA13197]-amprtrace.zip\""
        );
        assert_eq!(
            disposition("[ドラゴン é]-[PPSA1]-amprtrace.zip"),
            "attachment; filename=\"[____ _]-[PPSA1]-amprtrace.zip\"; \
             filename*=UTF-8''%5B%E3%83%89%E3%83%A9%E3%82%B4%E3%83%B3%20%C3%A9%5D-%5BPPSA1%5D-amprtrace.zip"
        );
        let hostile = disposition("a\"b\\c\r\nSet-Cookie: x=1.zip");
        assert!(!hostile.contains(['\r', '\n']), "{hostile}");
        assert!(hostile.starts_with("attachment; filename=\"a_b_c__Set-Cookie: x=1.zip\";"));
        assert!(
            hostile.ends_with("a%22b%5Cc%0D%0ASet-Cookie%3A%20x%3D1.zip"),
            "{hostile}"
        );
    }

    fn parse(text: &str) -> Result<Head, u16> {
        parse_head(text.as_bytes()).map_err(|r| r.status)
    }

    #[test]
    fn parses_strictly() {
        let head = parse("POST /api/jobs?x=1 HTTP/1.1\r\nHost: a:1\r\nContent-Length: 12").unwrap();
        assert_eq!(
            (head.method.as_str(), head.path.as_str()),
            ("POST", "/api/jobs")
        );
        assert_eq!((head.header("host"), head.length), (Some("a:1"), 12));
        for refused in [
            "PUT / HTTP/1.1\r\nHost: a:1",
            "GET http://a:1/ HTTP/1.1\r\nHost: a:1",
            "GET * HTTP/1.1\r\nHost: a:1",
            "GET  / HTTP/1.1\r\nHost: a:1",
            "GET / HTTP/2\r\nHost: a:1",
            "GET / HTTP/1.1\r\nHost: a:1\r\nHost: a:1",
            "GET / HTTP/1.1\r\nHost: a:1\r\nTransfer-Encoding: chunked",
            "GET / HTTP/1.1\r\nHost: a:1\r\nContent-Length: 1\r\nContent-Length: 1",
            "GET / HTTP/1.1\r\nHost: a:1\r\nContent-Length: -1",
            "GET / HTTP/1.1\r\nHost: a:1\r\nContent-Length: 1, 1",
            "GET / HTTP/1.1\r\nHost: a:1\r\n folded",
            "GET / HTTP/1.1\r\nHost: a:1\r\nBad Name: x",
            "GET / HTTP/1.1\r\nHost: a:1\r\nX: a\nY: b",
            "GET / HTTP/1.1\r\nHost: a:1\r\nnocolon",
        ] {
            assert_eq!(parse(refused).err(), Some(400), "{refused:?}");
        }
        let big = format!(
            "GET / HTTP/1.1\r\nHost: a:1\r\nContent-Length: {}",
            BODY_MAX + 1
        );
        assert_eq!(parse(&big).err(), Some(413));
        let huge = "GET / HTTP/1.1\r\nHost: a:1\r\nContent-Length: 99999999999999999999999";
        assert_eq!(parse(huge).err(), Some(413));
        assert!(parse("GET / HTTP/1.0").is_ok()); // no Host needed
    }
}
