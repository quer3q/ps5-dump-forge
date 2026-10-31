//! Install (a request with [`ConvertRequest::install`]): a `.pkg` on this device or another
//! one handed to the PS5's installer. Sony's installer accepts the package by URL and then
//! pulls it over HTTP Range requests (several connections, keep-alive, HEAD too, sometimes a
//! `.crc` sidecar), so the job serves the package on a 127.0.0.1 port of its own until every
//! byte has been served at least once. Nothing is written here.
// ponytail: done means every byte was fetched once; whether the PS5 then finishes the install
// (its own checks, its own download queue) is not seen.

use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use ps5upload_fpkg::ReadSeek;

use crate::convert::SourceStamp;
use crate::extract_paths::preflight_failed;
use crate::jobs::Ctx;
use crate::remote::is_remote;
use crate::{ConvertRequest, JobReport, VerifyMode, VerifySummary};

/// The stages an install reports, in order.
pub(crate) const STAGES: [&str; 2] = ["preflight", "install"];

/// No request and no byte served for this long: the PS5 gave up on the download.
const STALL: Duration = Duration::from_secs(5 * 60);

/// Once every byte is served, this long with no request ends the job (a response still on its
/// way, a range asked for again).
const SETTLE: Duration = Duration::from_secs(2);

/// Sony's MetaInfo holds the URL in 128 bytes.
const URL_MAX: usize = 127;

/// The bytes a response body is sent in.
const SEND_CHUNK: usize = 1 << 20;

/// At most this many connections served at once (ps5upload's loopback takes 16); more get 503.
const MAX_CONNS: usize = 16;

/// A package on another device is read ahead at most this much per connection.
// ponytail: 16 connections x (old + new 4 MiB window + 1 MiB send buffer) = 144 MiB ceiling.
const REMOTE_WINDOW: usize = 4 << 20;

/// A connection's kept reader checks the package's stamp again once this has passed.
const RECHECK: Duration = Duration::from_secs(5);

/// The longest request head read.
const HEAD_MAX: usize = 8 << 10;

/// The `Last-Modified` every response carries: fixed and in the past (ps5upload found a fresh
/// one refused on some firmware).
const LAST_MODIFIED: &str = "Wed, 01 Jan 2025 00:00:00 GMT";

/// The cancel error: the PS5 may keep the download listed.
const CANCELLED: &str = "cancelled; if the PS5 still lists the download, delete it there";

/// The installer: `(url, name)` accepted, or Sony's error code.
pub(crate) type Installer<'a> = &'a dyn Fn(&str, &str) -> Result<(), u32>;

/// What Sony's error code means for the user, where it is known.
fn hint(code: u32) -> Option<&'static str> {
    Some(match code {
        0x80A3_0004 | 0x80A3_000A => "install the base game first",
        0x80A3_000F => "this patch is for a different game",
        0x80A3_0011 => "the base game is the wrong version for this patch",
        0x80A3_0019 => "the patch package is invalid or corrupt",
        0x80A3_0017 => "this DLC needs its base content installed",
        0x80A3_000D => "the console firmware is too old for this package",
        0x80A3_000C => "close the game before installing",
        0x80A3_0002 => "not enough free space",
        0x80A3_0008 | 0x80A3_0009 => "the package is broken or the wrong type",
        _ => return None,
    })
}

/// The job's error for Sony's refusal `code`.
fn refused(code: u32) -> String {
    match hint(code) {
        Some(h) => format!("the PS5 refused the package: 0x{code:08X} ({h})"),
        None => format!("the PS5 refused the package: 0x{code:08X}"),
    }
}

/// The PS5's installer (`ps5/installer.c`).
#[cfg(target_env = "ps5")]
fn ps5_install(url: &str, name: &str) -> Result<(), u32> {
    use std::ffi::{CString, c_char};
    unsafe extern "C" {
        fn forge_pkg_install(url: *const c_char, name: *const c_char, code: *mut u32) -> i32;
    }
    // Neither holds a NUL: the URL is ASCII we made, the name a hint with NULs dropped.
    let url = CString::new(url).map_err(|_| 0u32)?;
    let name = CString::new(name.replace('\0', "")).map_err(|_| 0u32)?;
    let mut code = 0u32;
    // SAFETY: both strings are NUL-terminated and outlive the call; `code` is a valid u32.
    let rc = unsafe { forge_pkg_install(url.as_ptr(), name.as_ptr(), &mut code) };
    if rc == 0 { Ok(()) } else { Err(code) }
}

/// The install job, with this platform's installer (none off the PS5: a preflight finding).
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    #[cfg(target_env = "ps5")]
    return run_with(req, ctx, Some(&ps5_install), STALL);
    #[cfg(not(target_env = "ps5"))]
    run_with(req, ctx, None, STALL)
}

/// What the request itself can't be, before the package is read.
fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if !req.edits.is_empty()
        || !req.extract.is_empty()
        || !req.transfer.is_empty()
        || req.edit_rebuild
        || req.lz4.is_some()
        || req.zstd_pack
        || req.zstd_unpack
        || req.remove_backport
        || req.replace
    {
        findings.push(
            "install: an install takes no edit, extract, transfer, LZ4, zstd, backport or \
             replace options"
                .to_string(),
        );
    }
    findings
}

/// The package (or its sidecar) opened for reading, and its length: a regular file here (never a
/// link), or a file on another device read through [`crate::remote`].
fn open(path: &Path) -> anyhow::Result<(Box<dyn ReadSeek>, u64, SourceStamp)> {
    let shown = || path.display().to_string();
    if is_remote(path) {
        let r = crate::remote::reader(path)?.window_max(REMOTE_WINDOW);
        let len = r.len();
        let stamp = SourceStamp::of_file(path)?;
        return Ok((Box::new(r), len, stamp));
    }
    // ponytail: checked by name, then opened; a link swapped in between is followed.
    let meta = std::fs::symlink_metadata(path).with_context(shown)?;
    if meta.is_symlink() {
        anyhow::bail!("{} is a link (never followed)", shown());
    }
    let file = std::fs::File::open(path).with_context(shown)?;
    let meta = file.metadata().with_context(shown)?;
    if !meta.is_file() {
        anyhow::bail!("{} is not a file", shown());
    }
    let stamp = SourceStamp::of_handle(path, &file)?;
    Ok((Box::new(file), meta.len(), stamp))
}

/// The package being served.
struct Package {
    path: PathBuf,
    total: u64,
    stamp: SourceStamp,
    /// The last part of the URL: the file's own name when it is plain ASCII, else the content
    /// ID's (`<content id>.pkg`), else `package.pkg`.
    url_name: String,
    /// The name hint Sony gets: the file's name without `.pkg`.
    name: String,
    /// `<package>.crc` beside it, served when asked for and there.
    sidecar: PathBuf,
    /// The header's content ID (offset 0x40, 36 ASCII bytes), when it reads as one.
    content_id: Option<String>,
}

/// A name that goes into the URL as it is.
fn url_safe(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && (name.bytes()).all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// The package at `path`, or why it can't be installed (a finding).
fn probe(path: &Path) -> Result<Package, String> {
    if path.as_os_str().is_empty() {
        return Err("install: no package given".to_string());
    }
    let (mut reader, total, stamp) = open(path).map_err(|e| format!("install: {e:#}"))?;
    if total == 0 {
        return Err(format!("install: {} is empty", path.display()));
    }
    let mut head = Vec::with_capacity(0x64);
    let read = (reader.as_mut().take(0x64)).read_to_end(&mut head);
    read.map_err(|e| format!("install: {}: {e}", path.display()))?;
    // The CNT header sits at 0 (a PS4-style package), at FIH's 0x58 (a PS5 package, Forge's
    // included) or at LIH's container pair (0x30, a patch package).
    let at = |o: usize| (head.get(o..o + 8)).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let cnt_at = match head.get(..4) {
        Some(b"\x7FCNT") => Some(0),
        Some(b"\x7FFIH") => at(0x58),
        Some(b"\x7FLIH") => at(0x30),
        _ => None,
    };
    let Some(cnt_at) = cnt_at.filter(|&o| o < total) else {
        return Err(format!(
            "install: {} is not a PS5 package (no \\x7FFIH, \\x7FCNT or \\x7FLIH header)",
            path.display()
        ));
    };
    if cnt_at != 0 {
        head.clear();
        let read = (reader.seek(SeekFrom::Start(cnt_at)))
            .and_then(|_| reader.as_mut().take(0x64).read_to_end(&mut head));
        read.map_err(|e| format!("install: {}: {e}", path.display()))?;
    }
    let content_id = (head.get(0x40..0x64))
        .filter(|_| head.starts_with(b"\x7FCNT"))
        .filter(|id| id.iter().all(|b| b.is_ascii_graphic()))
        .map(|id| String::from_utf8_lossy(id).into_owned());
    if !stamp.unchanged() {
        return Err(format!(
            "{} changed while it was being installed",
            path.display()
        ));
    }
    let file_name = crate::transfer::name_of(path).unwrap_or_default();
    // The URL holds at most `URL_MAX` bytes; `http://127.0.0.1:65535/<16 hex>/` takes 40.
    let fits = |n: &String| url_safe(n) && n.len() <= URL_MAX - 40;
    let url_name = Some(file_name.clone())
        .filter(|n| fits(n) && n.to_ascii_lowercase().ends_with(".pkg"))
        .or_else(|| {
            content_id
                .as_ref()
                .map(|id| format!("{id}.pkg"))
                .filter(fits)
        })
        .unwrap_or_else(|| "package.pkg".to_string());
    let name = match file_name.len().checked_sub(4) {
        Some(n) if file_name.to_ascii_lowercase().ends_with(".pkg") => file_name[..n].to_string(),
        _ => file_name,
    };
    Ok(Package {
        path: path.to_path_buf(),
        total,
        stamp,
        url_name,
        name,
        sidecar: path.with_extension("crc"),
        content_id,
    })
}

/// The byte ranges served at least once: sorted, disjoint, half-open, touching ones merged.
#[derive(Default)]
struct Coverage(Vec<(u64, u64)>);

impl Coverage {
    fn add(&mut self, start: u64, end: u64) {
        if start >= end {
            return;
        }
        let (mut s, mut e) = (start, end);
        self.0.retain(|&(a, b)| {
            let apart = b < s || a > e;
            if !apart {
                (s, e) = (s.min(a), e.max(b));
            }
            apart
        });
        let at = self.0.partition_point(|&(a, _)| a < s);
        self.0.insert(at, (s, e));
    }

    fn covered(&self) -> u64 {
        self.0.iter().map(|(a, b)| b - a).sum()
    }
}

/// What the server's threads and the job share.
struct Shared {
    coverage: Coverage,
    /// The last request or body chunk: the stall clock.
    last: Instant,
    /// Connections being served.
    conns: usize,
    /// Requests answered, for the first few logged.
    requests: u64,
    /// Lines for the job's log, taken by the job thread (the server's can't log).
    logs: Vec<String>,
    /// A package change found by a server thread; delivered as the job's error.
    failed: Option<String>,
    /// Lines logged so far.
    logged: usize,
}

/// Requests logged as they come; later only refusals are.
const LOG_FIRST: u64 = 8;

/// At most this many log lines from the server.
const LOG_MAX: usize = 50;

impl Shared {
    fn log(&mut self, line: String) {
        if self.logged < LOG_MAX {
            self.logged += 1;
            self.logs.push(line);
        }
    }
}

fn lock(state: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// An HTTP date (`Wed, 01 Jan 2025 00:00:00 GMT`) for `secs` since the epoch.
fn http_date(secs: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        DAYS[(days % 7) as usize],
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// One request's head, as far as serving needs it.
struct Head {
    method: String,
    target: String,
    range: Option<String>,
    /// The connection may carry another request: HTTP/1.1 unless `Connection: close`.
    keep: bool,
}

fn parse_head(head: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    let (mut range, mut connection) = (None, None);
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("connection") {
                connection = Some(value.to_ascii_lowercase());
            }
        }
    }
    let keep = match connection.as_deref() {
        Some(c) if c.contains("close") => false,
        Some(c) if c.contains("keep-alive") => true,
        _ => version == "HTTP/1.1",
    };
    Some(Head {
        method: method.to_string(),
        target: target.to_string(),
        range,
        keep,
    })
}

/// What a `Range` header asks of `total` bytes.
#[derive(Debug, PartialEq)]
enum Range {
    /// No usable range (none, malformed, several): the whole file.
    Whole,
    /// Bytes `start..=end`.
    Part(u64, u64),
    /// Nothing of the file: 416.
    Unsatisfiable,
}

fn parse_range(value: &str, total: u64) -> Range {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return Range::Whole;
    };
    let Some((a, b)) = spec.split_once('-').filter(|_| !spec.contains(',')) else {
        return Range::Whole;
    };
    let (a, b) = (a.trim(), b.trim());
    let (start, end) = if a.is_empty() {
        // `bytes=-n`: the last n bytes.
        let Ok(n) = b.parse::<u64>() else {
            return Range::Whole;
        };
        if n == 0 || total == 0 {
            return Range::Unsatisfiable;
        }
        (total - n.min(total), total - 1)
    } else {
        let Ok(start) = a.parse::<u64>() else {
            return Range::Whole;
        };
        let end = match b.parse::<u64>() {
            Ok(end) => end,
            Err(_) if b.is_empty() => u64::MAX,
            Err(_) => return Range::Whole,
        };
        (start, end)
    };
    if start >= total || start > end {
        return Range::Unsatisfiable;
    }
    Range::Part(start, end.min(total - 1))
}

/// A response head.
fn response(status: &str, len: u64, extra: &str, keep: bool) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "HTTP/1.1 {status}\r\nDate: {}\r\nLast-Modified: {LAST_MODIFIED}\r\n\
         Content-Type: application/octet-stream\r\nContent-Length: {len}\r\n\
         Accept-Ranges: bytes\r\n{extra}Connection: {}\r\n\r\n",
        http_date(now.as_secs()),
        if keep { "keep-alive" } else { "close" }
    )
}

/// A kept-alive connection with no request for this long is closed.
const CONN_IDLE: Duration = Duration::from_secs(60);

/// Accepts connections on `listener` (non-blocking) until `stop`, each served on its own thread.
fn serve(
    listener: &TcpListener,
    pkg: &Package,
    attempt: &str,
    state: &Mutex<Shared>,
    stop: &AtomicBool,
) {
    std::thread::scope(|s| {
        while !stop.load(Ordering::Relaxed) {
            let Ok((mut conn, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            };
            let full = {
                let mut st = lock(state);
                if st.conns >= MAX_CONNS {
                    true
                } else {
                    st.conns += 1;
                    false
                }
            };
            if full {
                let _ = conn.set_nonblocking(false);
                let _ = conn.set_write_timeout(Some(Duration::from_millis(200)));
                let _ = conn.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            s.spawn(move || {
                connection(conn, pkg, attempt, state, stop);
                lock(state).conns -= 1;
            });
        }
    });
}

/// Answers the requests of one connection, one after another, until it closes, idles, fails
/// or the job stops serving.
fn connection(
    mut conn: TcpStream,
    pkg: &Package,
    attempt: &str,
    state: &Mutex<Shared>,
    stop: &AtomicBool,
) {
    // Accepted from a non-blocking listener, it may be non-blocking too (BSD, macOS).
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(30)));
    let _ = conn.set_nodelay(true);
    let mut buf = Vec::new();
    let mut idle = Instant::now();
    let mut reader = Reader::default();
    while !stop.load(Ordering::Relaxed) {
        // Requests have no body: what follows one head is the next.
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head: Vec<u8> = buf.drain(..at + 4).collect();
            match answer(&mut conn, &mut reader, &head, pkg, attempt, state, stop) {
                Ok(true) => idle = Instant::now(),
                _ => return,
            }
            continue;
        }
        if buf.len() > HEAD_MAX {
            return;
        }
        let mut chunk = [0u8; 4096];
        match conn.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if idle.elapsed() >= CONN_IDLE {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// A 404, logged.
fn not_found(
    conn: &mut TcpStream,
    state: &Mutex<Shared>,
    keep: bool,
    line: &str,
) -> std::io::Result<bool> {
    lock(state).log(format!("served: {line}"));
    let close = if keep { "keep-alive" } else { "close" };
    let head =
        format!("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: {close}\r\n\r\n");
    conn.write_all(head.as_bytes()).map(|()| keep)
}

/// Answers one request; `Ok(true)` when the connection may carry another.
fn answer(
    conn: &mut TcpStream,
    reader: &mut Reader,
    head: &[u8],
    pkg: &Package,
    attempt: &str,
    state: &Mutex<Shared>,
    stop: &AtomicBool,
) -> std::io::Result<bool> {
    let line = String::from_utf8_lossy(head.split(|b| *b == b'\r').next().unwrap_or_default())
        .into_owned();
    let Some(h) = parse_head(head) else {
        return not_found(conn, state, false, &format!("{line} -> 404 (unparsed)"));
    };
    let head_only = h.method == "HEAD";
    if !head_only && h.method != "GET" {
        return not_found(conn, state, h.keep, &format!("{line} -> 404 (method)"));
    }
    let range = (h.range.as_deref()).map_or(Range::Whole, |r| parse_range(r, pkg.total));
    {
        let mut st = lock(state);
        st.last = Instant::now();
        st.requests += 1;
        if st.requests <= LOG_FIRST {
            let asked = h.range.as_deref().unwrap_or("-");
            st.log(format!("served: {line} range={asked}"));
        }
    }
    let prefix = format!("/{attempt}/");
    // The PS5 adds a query (`?product=0287&serverIpAddr=127.0.0.1&r=00000000`): the path decides.
    let path = h
        .target
        .split_once('?')
        .map_or(h.target.as_str(), |(p, _)| p);
    let Some(name) = path.strip_prefix(&prefix) else {
        return not_found(conn, state, h.keep, &format!("{line} -> 404 (path)"));
    };
    if name != pkg.url_name {
        let stem = pkg
            .url_name
            .rsplit_once('.')
            .map_or(pkg.url_name.as_str(), |(s, _)| s);
        if name == format!("{stem}.crc") {
            return sidecar(conn, pkg, head_only, &line, state);
        }
        return not_found(conn, state, h.keep, &format!("{line} -> 404 (path)"));
    }
    let (start, end) = match range {
        Range::Unsatisfiable => {
            let extra = format!("Content-Range: bytes */{}\r\n", pkg.total);
            let head = response("416 Range Not Satisfiable", 0, &extra, h.keep);
            conn.write_all(head.as_bytes())?;
            return Ok(h.keep);
        }
        Range::Part(start, end) => {
            let extra = format!("Content-Range: bytes {start}-{end}/{}\r\n", pkg.total);
            let head = response("206 Partial Content", end - start + 1, &extra, h.keep);
            conn.write_all(head.as_bytes())?;
            (start, end)
        }
        Range::Whole => {
            conn.write_all(response("200 OK", pkg.total, "", h.keep).as_bytes())?;
            (0, pkg.total - 1)
        }
    };
    if head_only {
        return Ok(h.keep);
    }
    body(conn, reader, pkg, start, end + 1, state, stop)?;
    Ok(h.keep)
}

/// Opens the same package seen at preflight, including its identity and modification time.
fn open_checked(pkg: &Package) -> anyhow::Result<Box<dyn ReadSeek>> {
    if !pkg.stamp.unchanged() {
        anyhow::bail!(changed(pkg));
    }
    let (r, len, stamp) = open(&pkg.path)?;
    if len != pkg.total || !pkg.stamp.same(&stamp) || !pkg.stamp.unchanged() {
        anyhow::bail!(changed(pkg));
    }
    Ok(r)
}

fn changed(pkg: &Package) -> String {
    format!(
        "{} changed while it was being installed",
        pkg.path.display()
    )
}

/// A connection's package reader: opened and checked at its first body, then kept for the
/// connection's later requests (each open and stamp is a round trip or more on another
/// device), its stamp checked again once [`RECHECK`] has passed.
#[derive(Default)]
struct Reader {
    open: Option<(Box<dyn ReadSeek>, Instant)>,
}

impl Reader {
    fn get(&mut self, pkg: &Package) -> anyhow::Result<&mut dyn ReadSeek> {
        match &mut self.open {
            Some((_, checked)) if checked.elapsed() < RECHECK => {}
            Some((_, checked)) => {
                if !pkg.stamp.unchanged() {
                    self.open = None;
                    anyhow::bail!(changed(pkg));
                }
                *checked = Instant::now();
            }
            None => self.open = Some((open_checked(pkg)?, Instant::now())),
        }
        Ok(self.open.as_mut().unwrap().0.as_mut())
    }
}

/// Sends bytes `start..end` of the package, marking each chunk sent as covered.
fn body(
    conn: &mut TcpStream,
    reader: &mut Reader,
    pkg: &Package,
    start: u64,
    end: u64,
    state: &Mutex<Shared>,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    let failed = |e: String| {
        let mut st = lock(state);
        st.log(format!("served: {}: {e}", pkg.path.display()));
        st.failed.get_or_insert_with(|| e.clone());
        std::io::Error::other(e)
    };
    let r = reader.get(pkg).map_err(|e| failed(format!("{e:#}")))?;
    r.seek(SeekFrom::Start(start))
        .map_err(|e| failed(e.to_string()))?;
    let mut buf = vec![0u8; SEND_CHUNK];
    let mut at = start;
    while at < end {
        if stop.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("stopped"));
        }
        let want = usize::try_from(end - at).map_or(SEND_CHUNK, |n| n.min(SEND_CHUNK));
        r.read_exact(&mut buf[..want])
            .map_err(|e| failed(e.to_string()))?;
        conn.write_all(&buf[..want])?;
        let mut st = lock(state);
        st.coverage.add(at, at + want as u64);
        st.last = Instant::now();
        at += want as u64;
    }
    Ok(())
}

/// The package's `.crc` sidecar, whole, when there is one beside it (else 404); the connection
/// is closed after it.
fn sidecar(
    conn: &mut TcpStream,
    pkg: &Package,
    head_only: bool,
    line: &str,
    state: &Mutex<Shared>,
) -> std::io::Result<bool> {
    let Ok((r, len, _)) = open(&pkg.sidecar) else {
        return not_found(conn, state, true, &format!("{line} -> 404 (no sidecar)"));
    };
    lock(state).log(format!("served: {line} -> 200 (sidecar)"));
    conn.write_all(response("200 OK", len, "", false).as_bytes())?;
    if !head_only {
        let sent = std::io::copy(&mut r.take(len), conn)?;
        if sent != len {
            return Err(std::io::Error::other(
                "the sidecar changed while it was sent",
            ));
        }
    }
    Ok(false)
}

/// The install job with `installer` (none: this device can't install, a preflight finding),
/// failing once the PS5 fetches nothing for `stall`.
pub(crate) fn run_with(
    req: &ConvertRequest,
    ctx: &Ctx,
    installer: Option<Installer>,
    stall: Duration,
) -> anyhow::Result<JobReport> {
    ctx.stages(&STAGES);
    ctx.progress("preflight", 0, 1);
    let mut findings = request_findings(req);
    if installer.is_none() {
        findings.push("install: packages install only on a PS5".to_string());
    }
    let pkg = probe(req.install.as_deref().unwrap_or(Path::new("")));
    let (pkg, installer) = match (pkg, installer) {
        (Ok(pkg), Some(installer)) if findings.is_empty() => (pkg, installer),
        (pkg, _) => {
            findings.extend(pkg.err());
            return Err(preflight_failed(findings));
        }
    };
    let id = pkg.content_id.as_deref().unwrap_or("unreadable");
    ctx.log(format!(
        "package: {} ({} bytes, content ID {id})",
        pkg.path.display(),
        pkg.total
    ));
    ctx.progress("preflight", 1, 1);
    ctx.check()?;

    let listener = TcpListener::bind(("127.0.0.1", 0)).context("starting the package server")?;
    listener
        .set_nonblocking(true)
        .context("starting the package server")?;
    let port = listener
        .local_addr()
        .context("starting the package server")?
        .port();
    let mut token = [0u8; 8];
    getrandom::fill(&mut token).map_err(|e| anyhow::anyhow!("no randomness for the URL: {e}"))?;
    let attempt: String = token.iter().map(|b| format!("{b:02x}")).collect();
    let url = format!("http://127.0.0.1:{port}/{attempt}/{}", pkg.url_name);
    let state = Mutex::new(Shared {
        coverage: Coverage::default(),
        last: Instant::now(),
        conns: 0,
        requests: 0,
        logs: Vec::new(),
        failed: None,
        logged: 0,
    });
    let stop = AtomicBool::new(false);
    ctx.expect_rest(pkg.total);
    ctx.progress("install", 0, pkg.total);
    std::thread::scope(|s| {
        s.spawn(|| serve(&listener, &pkg, &attempt, &state, &stop));
        let result = deliver(ctx, &pkg, &url, installer, &state, stall);
        stop.store(true, Ordering::Relaxed);
        result
    })?;
    ctx.progress("install", pkg.total, pkg.total);
    Ok(JobReport {
        output: pkg.path.clone(),
        bytes: pkg.total,
        files: 1,
        checks: vec![format!("install: the PS5 fetched all {} bytes", pkg.total)],
        verify: VerifySummary {
            mode: VerifyMode::Fast,
            checked_bytes: 0,
            total_bytes: pkg.total,
            samples: 0,
            seed: 0,
        },
    })
}

/// Hands `url` to the installer, then waits until every byte was served and requests settle,
/// the PS5 stalls, or the job is cancelled.
fn deliver(
    ctx: &Ctx,
    pkg: &Package,
    url: &str,
    installer: Installer,
    state: &Mutex<Shared>,
    stall: Duration,
) -> anyhow::Result<()> {
    ctx.log(format!("serving it at {url}"));
    if url.len() > URL_MAX {
        anyhow::bail!("the package's URL is longer than {URL_MAX} bytes: {url}");
    }
    if let Err(code) = installer(url, &pkg.name) {
        // What the PS5 asked for before refusing (a 0x80B22404 is its pre-flight of the URL).
        for line in std::mem::take(&mut lock(state).logs) {
            ctx.log(line);
        }
        anyhow::bail!(refused(code));
    }
    ctx.log("the PS5 accepted the package and fetches it now");
    lock(state).last = Instant::now();
    let mut quarter = 0u128;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let (covered, idle, logs, failed) = {
            let mut st = lock(state);
            (
                st.coverage.covered(),
                st.last.elapsed(),
                std::mem::take(&mut st.logs),
                st.failed.clone(),
            )
        };
        for line in logs {
            ctx.log(line);
        }
        if let Some(failed) = failed {
            anyhow::bail!(failed);
        }
        ctx.progress("install", covered, pkg.total);
        while quarter < 4 && u128::from(covered) * 4 >= u128::from(pkg.total) * (quarter + 1) {
            quarter += 1;
            ctx.log(format!("served {}% of the package", quarter * 25));
        }
        if ctx.cancel.load(Ordering::Relaxed) {
            anyhow::bail!(CANCELLED);
        }
        if covered >= pkg.total && idle >= SETTLE {
            return Ok(());
        }
        if idle >= stall {
            anyhow::bail!("the PS5 stopped fetching the package");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &[u8; 36] = b"UP0000-PPSA00000_00-0000000000000000";

    /// A package-looking file of `len` bytes (a CNT header, a content ID, varied bytes).
    fn package(path: &Path, len: usize) -> Vec<u8> {
        let mut data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
        data[..4].copy_from_slice(b"\x7FCNT");
        data[0x40..0x64].copy_from_slice(ID);
        std::fs::write(path, &data).unwrap();
        data
    }

    fn request(path: &Path) -> ConvertRequest {
        serde_json::from_value(serde_json::json!({
            "source": "", "format": "folder", "output": "", "install": path
        }))
        .unwrap()
    }

    fn run(
        req: &ConvertRequest,
        installer: Installer,
        cancel: &AtomicBool,
    ) -> Result<JobReport, String> {
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
        let ctx = Ctx::new(1, &emit, cancel);
        run_with(req, &ctx, Some(installer), Duration::from_secs(5)).map_err(|e| format!("{e:#}"))
    }

    /// One request on a kept-alive connection: status and body.
    fn ask(conn: &mut TcpStream, method: &str, path: &str, range: Option<&str>) -> (u16, Vec<u8>) {
        let range = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
        let req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{range}\r\n");
        conn.write_all(req.as_bytes()).unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8];
        while !head.ends_with(b"\r\n\r\n") {
            conn.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8(head).unwrap();
        let status = head[9..12].parse().unwrap();
        let len: usize = (head.lines())
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .map_or(0, |n| n.trim().parse().unwrap());
        let mut body = vec![0u8; if method == "HEAD" { 0 } else { len }];
        conn.read_exact(&mut body).unwrap();
        (status, body)
    }

    /// The host and path of `url`.
    fn split(url: &str) -> (String, String) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        (host.to_string(), format!("/{path}"))
    }

    #[test]
    fn install_serves_every_byte_by_ranges_on_kept_alive_connections() {
        let dir = crate::test_dir("install-ok");
        let path = dir.join("My Game (v1).pkg");
        let data = package(&path, (3 << 20) + 12_345);
        std::fs::write(dir.join("My Game (v1).crc"), b"crc!").unwrap();
        let installer = |url: &str, name: &str| -> Result<(), u32> {
            assert_eq!(name, "My Game (v1)");
            // Not plain ASCII: the URL names the content ID.
            let (host, path) = split(url);
            assert!(path.ends_with(&format!("/{}.pkg", std::str::from_utf8(ID).unwrap())));
            let mut conn = TcpStream::connect(&host).unwrap();
            let (status, body) = ask(&mut conn, "HEAD", &path, None);
            assert_eq!((status, body.len()), (200, 0));
            let crc = path.replace(".pkg", ".crc");
            assert_eq!(ask(&mut conn, "GET", &crc, None), (200, b"crc!".to_vec()));
            // The PS5 asks with a query of its own (as measured on a console).
            let asked = format!("{path}?product=0287&serverIpAddr=127.0.0.1&r=00000000");
            let mut fresh = TcpStream::connect(&host).unwrap();
            let (status, body) = ask(&mut fresh, "GET", &asked, Some("bytes=0-15"));
            assert_eq!((status, body), (206, data[..16].to_vec()));
            let mut conn = TcpStream::connect(&host).unwrap();
            assert_eq!(ask(&mut conn, "GET", "/elsewhere/x.pkg", None).0, 404);
            // Two connections, ranges taken in turn, each kept alive.
            let mut other = TcpStream::connect(&host).unwrap();
            let step = (1 << 20) + 7;
            for (k, start) in (0..data.len()).step_by(step).enumerate() {
                let end = (start + step).min(data.len()) - 1;
                let c = if k % 2 == 0 { &mut conn } else { &mut other };
                let (status, body) = ask(c, "GET", &path, Some(&format!("bytes={start}-{end}")));
                assert_eq!(status, 206);
                assert_eq!(body, data[start..=end]);
            }
            Ok(())
        };
        let report = run(&request(&path), &installer, &AtomicBool::new(false)).unwrap();
        assert_eq!(report.bytes, data.len() as u64);
        assert_eq!(report.output, path);
    }

    #[test]
    fn install_detects_same_size_package_replacement_on_reopen() {
        let dir = crate::test_dir("install-replaced");
        let path = dir.join("game.pkg");
        let data = package(&path, 4096);
        let pkg = probe(&path).unwrap();
        let mut r = open_checked(&pkg).unwrap();
        let mut first = [0; 100];
        r.read_exact(&mut first).unwrap();
        assert_eq!(first, data[..100]);
        let replacement = dir.join("replacement.pkg");
        std::fs::write(&replacement, &data).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        let err = open_checked(&pkg).err().unwrap().to_string();
        assert_eq!(
            err,
            format!("{} changed while it was being installed", path.display())
        );
    }

    #[test]
    fn install_cancelled_stops_serving_and_says_what_stays() {
        let dir = crate::test_dir("install-cancel");
        let path = dir.join("game.pkg");
        package(&path, 1 << 20);
        let cancel = AtomicBool::new(false);
        let url = Mutex::new(String::new());
        let installer = |u: &str, _: &str| -> Result<(), u32> {
            let (host, path) = split(u);
            let mut conn = TcpStream::connect(&host).unwrap();
            assert_eq!(ask(&mut conn, "GET", &path, Some("bytes=0-99")).0, 206);
            *url.lock().unwrap() = u.to_string();
            cancel.store(true, Ordering::Relaxed);
            Ok(())
        };
        let err = run(&request(&path), &installer, &cancel).unwrap_err();
        assert_eq!(err, CANCELLED);
        // The server is gone with the job.
        let (host, _) = split(&url.lock().unwrap());
        assert!(TcpStream::connect(&host).is_err());
    }

    #[test]
    fn install_preflight_refuses_what_is_not_a_package_and_other_options() {
        let dir = crate::test_dir("install-preflight");
        let path = dir.join("game.pkg");
        std::fs::write(&path, b"PK\x03\x04 not a package").unwrap();
        let mut req = request(&path);
        req.zstd_pack = true;
        let installer = |_: &str, _: &str| -> Result<(), u32> { panic!("not called") };
        let err = run(&req, &installer, &AtomicBool::new(false)).unwrap_err();
        assert!(err.contains("an install takes no edit"), "{err}");
        assert!(err.contains("is not a PS5 package"), "{err}");
        std::fs::write(&path, b"").unwrap();
        let err = run(&request(&path), &installer, &AtomicBool::new(false)).unwrap_err();
        assert!(err.contains("is empty"), "{err}");
        // Off the PS5 there is no installer.
        #[cfg(not(target_env = "ps5"))]
        {
            package(&path, 4096);
            let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
            let cancel = AtomicBool::new(false);
            let err = super::run(&request(&path), &Ctx::new(1, &emit, &cancel)).unwrap_err();
            assert!(
                format!("{err:#}").contains("packages install only on a PS5"),
                "{err:#}"
            );
        }
    }
}
