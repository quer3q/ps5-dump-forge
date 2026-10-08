//! A strict HTTP/1.1 subset: one request per connection, origin-form targets, `GET` and
//! `POST`, bodies by `Content-Length` only. Anything else is refused rather than guessed at.

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
}

impl Response {
    pub fn json(value: &impl serde::Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self::new(200, JSON, Cow::Owned(body)),
            Err(e) => Self::error(500, format!("internal error: {e}")),
        }
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
        let mut head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Connection: close\r\n",
            self.status,
            self.kind,
            self.body.len()
        );
        // API replies: the page must never see a stale one.
        if self.kind == JSON {
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
    let path = target.split('?').next().unwrap_or_default();

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

/// Writes the response by `deadline`, however slowly the client reads, and closes. Unread
/// request bytes are drained for a moment first, so the close doesn't reset the connection
/// before the client has read the response.
pub(crate) fn send(stream: &mut TcpStream, response: &Response, deadline: Instant) {
    let _ = write_by(stream, &response.bytes(), deadline);
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
