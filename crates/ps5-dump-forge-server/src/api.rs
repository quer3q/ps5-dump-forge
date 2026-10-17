//! The routes. Each mirrors a Tauri command in `app/src-tauri/src/main.rs`: the same core
//! call on the paths as given and the same `format!("{e:#}")` error text. The server adds
//! only limits: on jobs, concurrent inspections and listing sizes.

use std::collections::{BTreeSet, HashSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ps5_dump_forge_core::{
    ArchivePassword, ConvertRequest, DeleteError, Event, Format, JobId, Jobs,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::http::{self, Head, Response};
use crate::paths::{Roots, Storage};
use crate::table::{self, Table};
use crate::{Options, Platform, random_hex};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

const MAX_JOBS: i64 = 8;
/// Inspecting or naming reads a source's metadata (a whole image's, for an inspection); more
/// at once only slows the console down.
const MAX_INSPECTIONS: usize = 2;
/// How long a read waits for one of those slots before its 429: picking a source starts its
/// inspection and its naming together, and an archive's take a while. At most [`MAX_WAITING`]
/// wait at once (a 429 at once past that), so waiting reads never hold the handlers that
/// Cancel and Stop need.
const MAX_WAITING: usize = 4;
const SLOT_WAIT: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(100)
} else {
    std::time::Duration::from_secs(30)
};
/// How long `quit {"if_idle": true}` waits for admission before answering busy.
const IDLE_WAIT: Duration = Duration::from_secs(1);
const LIST_MAX: usize = 10_000;
/// How long an upload may wait for its next bytes (no limit on the whole: a GiB takes minutes).
const UPLOAD_IDLE: Duration = Duration::from_secs(30);
const STALE_MAX: usize = 1_000;

type Reply = Result<Response, Response>;

pub(crate) struct Server {
    pub opts: Options,
    /// `http://<address>:<port>` as shown to the user.
    pub url: String,
    roots: Roots,
    instance: String,
    jobs: Jobs,
    table: Arc<Mutex<Table>>,
    /// Admitted minus finished jobs; only raised under `admission`, lowered by `Done`.
    running: Arc<AtomicI64>,
    /// Held from the admission checks until the job is in the table, so admitting and
    /// stopping never interleave.
    admission: Mutex<()>,
    stopping: AtomicBool,
    inspecting: AtomicUsize,
    /// Reads waiting in [`Server::capped`] for a slot.
    waiting: AtomicUsize,
    /// Trace downloads being streamed (`quit {"if_idle": true}` waits for none).
    streaming: Arc<AtomicUsize>,
    /// A `self_update` is waiting on elfldr.
    updating: AtomicBool,
    /// Staged files being uploaded (`edit_upload`): one upload per file at a time, and no
    /// discard of a staging folder with one in flight.
    uploads: Mutex<HashSet<PathBuf>>,
    cores: usize,
    /// Tests widen the gap between `Jobs::start` returning and the table admitting the job.
    #[cfg(test)]
    admit_pause: std::time::Duration,
}

/// `POST /api/library_scan`'s reply: core's scan, plus `storage` on the PS5 only.
#[derive(serde::Serialize)]
struct ScanReply<'a> {
    #[serde(flatten)]
    scan: &'a ps5_dump_forge_core::LibraryScan,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<Storage>,
}

/// Cores a default job leaves to the server (and on the PS5, the console's browser).
const SPARE_CORES: usize = if cfg!(target_env = "ps5") { 2 } else { 1 };

/// A lock that survives a panic elsewhere (the data stays consistent per call).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, Response> {
    serde_json::from_slice(body).map_err(|e| Response::error(400, format!("bad request: {e}")))
}

/// Core's `anyhow` error with its causes, as the Tauri commands return it.
fn core(e: impl std::fmt::Display) -> Response {
    Response::error(500, format!("{e:#}"))
}

/// What an inspection or a naming request may read: a folder or a regular file, symlinks
/// followed. A FIFO or device would block its handler (or a job, and quit with it) on open.
/// Anything missing is left for core to report.
// ponytail: checked by path, then opened by core by path; swapping in a FIFO between the
// two needs write access to the storage.
fn readable(path: &Path) -> Result<(), Response> {
    match std::fs::metadata(path) {
        Ok(meta) if !meta.is_dir() && !meta.is_file() => Err(Response::error(
            400,
            format!("{}: not a folder or a regular file", path.display()),
        )),
        _ => Ok(()),
    }
}

/// Core's delete refusal as its status: 400, 404, 409 or 500.
fn deleted(e: DeleteError) -> Response {
    let status = match e {
        DeleteError::Refused(_) => 400,
        DeleteError::Missing(_) => 404,
        DeleteError::Busy(_) => 409,
        DeleteError::Failed(_) => 500,
    };
    Response::error(status, e.to_string())
}

/// A game path's components: relative, `/`-separated, no empty, `.` or `..` component, no `\`
/// or NUL (and on Windows no `:`, which `PathBuf::push` would read as a drive).
// ponytail: core's `Edit` path rule, restated (core keeps its check private); kept in step by
// hand, and core checks the edit job's paths again anyway.
fn game_path(path: &str) -> Result<Vec<&str>, Response> {
    let parts: Vec<&str> = path.split('/').collect();
    let bad = |c: &str| {
        matches!(c, "" | "." | "..")
            || c.contains(['\\', '\0'])
            || (cfg!(windows) && c.contains(':'))
    };
    match parts.iter().any(|c| bad(c)) {
        true => Err(Response::error(400, format!("bad game path: {path:?}"))),
        false => Ok(parts),
    }
}

/// `<source>.forge-upload-<pid>.part` beside an absolute source: a `.part`, so `stale_parts`
/// lists a leftover and the Library skips it.
fn staging_dir(source: &Path) -> Result<PathBuf, Response> {
    let (Some(parent), Some(name)) = (source.parent(), source.file_name()) else {
        return Err(Response::error(
            400,
            "bad request: source must be a file or folder path",
        ));
    };
    if !source.is_absolute() {
        return Err(Response::error(
            400,
            "bad request: source must be an absolute path",
        ));
    }
    let mut name = name.to_os_string();
    name.push(format!(".forge-upload-{}.part", std::process::id()));
    Ok(parent.join(name))
}

/// One folder of the staging tree, made unless it is there (0777 on the PS5).
fn make_dir(dir: &Path) -> Result<(), Response> {
    match std::fs::create_dir(dir) {
        // By path: opening a folder fails on Windows (and is only needed on FreeBSD).
        #[cfg(target_os = "freebsd")]
        Ok(()) => {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777))
        }
        #[cfg(not(target_os = "freebsd"))]
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            match std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
                true => Ok(()),
                false => {
                    let text = format!("{}: not a folder", dir.display());
                    return Err(Response::error(409, text));
                }
            }
        }
        Err(e) => Err(e),
    }
    .map_err(|e| core(format!("{}: {e}", dir.display())))
}

/// CE-107750-0: what the game reads on the PS5 is 0777, set explicitly (the create mode is
/// masked by the umask), as core's `finalize::open_up`. A no-op off FreeBSD.
fn open_up(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(target_os = "freebsd")]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o777))?;
    }
    let _ = file;
    Ok(())
}

/// Exactly `length` bytes of `body` into `file`. A client that ends early or stalls (any read
/// error) is `UnexpectedEof` (a 400); a failed write keeps its own kind (a 500).
fn copy_body(body: &mut dyn Read, file: &mut std::fs::File, length: u64) -> std::io::Result<()> {
    let eof = |text: String| std::io::Error::new(std::io::ErrorKind::UnexpectedEof, text);
    let mut buf = vec![0u8; 1024 * 1024];
    let mut left = length;
    while left > 0 {
        let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let n = match body.read(&mut buf[..want]) {
            Ok(0) => return Err(eof("the upload ended early".into())),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(eof(format!("reading the upload: {e}"))),
        };
        file.write_all(&buf[..n])?;
        left -= n as u64;
    }
    Ok(())
}

/// One upload in [`Server::uploads`]; frees its file when dropped, also on unwind.
struct Uploading<'a>(&'a Mutex<HashSet<PathBuf>>, PathBuf);

impl Drop for Uploading<'_> {
    fn drop(&mut self) {
        lock(self.0).remove(&self.1);
    }
}

/// Makes each upload's temporary name unique within this process.
static UPLOAD_SEQ: AtomicU64 = AtomicU64::new(0);

/// One running inspection; frees its place even when core panics.
struct Inspecting<'a>(&'a AtomicUsize);

impl Drop for Inspecting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One trace download being streamed; frees its place once the body is written or dropped.
struct Streaming(Arc<AtomicUsize>);

impl Streaming {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for Streaming {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Clears its flag when dropped, also when the handler unwinds.
struct Flag<'a>(&'a AtomicBool);

impl Drop for Flag<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// `X.Y.Z`, each part 1 to 9 digits.
fn plain_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Every byte but the unreserved ones as `%XX`.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Asks elfldr at `addr` to download v`version`'s PS5 ELF from its GitHub release and start it
/// with `serve`: `GET /?uri=<that URL>`, `args` and `pipe` inside the URL's own query (elfldr
/// reads them from there). elfldr answers 200 at once, then downloads and spawns, writing any
/// failure into the body; with `pipe=0` it closes once spawned. The new process takes the port
/// over from this one (`serve`'s handover) once it is idle.
// ponytail: unverified on hardware: elfldr's sceHttp has to follow GitHub's 302
// to its asset host and GitHub to ignore the extra query; elfldr ignores certificate errors and
// nothing checks a signature (anyone who can intercept the console's traffic can serve an ELF);
// it reads the body once without checking its length, so a short read could start a cut ELF.
fn elfldr_load(addr: SocketAddr, version: &str) -> Result<(), Response> {
    let uri = format!(
        "https://github.com/quer3q/ps5-dump-forge/releases/download/v{version}/\
         ps5-dump-forge-{version}-ps5.elf?args=serve&pipe=0"
    );
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|_| {
        Response::error(
            503,
            format!(
                "elfldr isn't running on port {}: load it, then try again",
                addr.port()
            ),
        )
    })?;
    let request = format!(
        "GET /?uri={} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n",
        percent_encode(&uri)
    );
    let failed = |why: &str| {
        Response::error(
            502,
            format!("elfldr couldn't download or start v{version}: {why}"),
        )
    };
    // The download (~11 MB) happens inside this reply; within the page's minute.
    let reply = crate::ask(
        stream,
        &request,
        Instant::now() + Duration::from_secs(55),
        64 * 1024,
    )
    .map_err(|e| failed(&e.to_string()))?;
    if crate::status_of(&reply) != Some(200) {
        return Err(Response::error(502, "elfldr refused the request"));
    }
    let text = String::from_utf8_lossy(&reply);
    let body = text.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    match body.lines().find(|l| l.contains("Error")) {
        Some(line) => Err(failed(line.trim())),
        None => Ok(()),
    }
}

/// The browse roots and the notes about candidates left out; an error on a computer with no
/// usable `--root` (also checked before a takeover, so a bad start never stops a good server).
pub(crate) fn usable_roots(opts: &Options) -> std::io::Result<(Roots, Vec<String>)> {
    let (roots, notes) = Roots::new(opts.platform, &opts.roots);
    if opts.platform == Platform::Host && roots.current().is_empty() {
        return Err(std::io::Error::other("no usable --root folder"));
    }
    Ok((roots, notes))
}

/// A JSON object whose optional `root` is a string (else 400).
fn optional_root(body: &[u8]) -> Result<Option<PathBuf>, Response> {
    match parse::<serde_json::Value>(body)? {
        serde_json::Value::Object(o) => match o.get("root") {
            None => Ok(None),
            Some(serde_json::Value::String(s)) => Ok(Some(PathBuf::from(s))),
            Some(_) => Err(Response::error(400, "bad request: root must be a string")),
        },
        _ => Err(Response::error(400, "bad request: expected a JSON object")),
    }
}

#[derive(Deserialize)]
struct PathArg {
    path: PathBuf,
}

/// `/api/inspect`: `password` for an archive that needs one (never logged or echoed).
#[derive(Deserialize)]
struct InspectArg {
    path: PathBuf,
    #[serde(default)]
    password: Option<ArchivePassword>,
}

/// `/api/default_output`, `/api/generated_output`: `password` for an archive that needs one
/// (never logged or echoed).
#[derive(Deserialize)]
struct OutputArg {
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    #[serde(default)]
    taken: Vec<PathBuf>,
    #[serde(default)]
    password: Option<ArchivePassword>,
}

#[derive(Deserialize)]
struct StartArg {
    request: ConvertRequest,
}

#[derive(Deserialize)]
struct SourceArg {
    source: PathBuf,
}

#[derive(Deserialize)]
struct VersionArg {
    version: String,
}

#[derive(Deserialize)]
struct IdArg {
    id: JobId,
}

#[derive(Deserialize)]
struct DirsArg {
    dirs: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct ListArg {
    #[serde(default)]
    path: Option<String>,
}

impl Server {
    pub fn new(opts: Options, local: SocketAddr) -> std::io::Result<Self> {
        let (roots, notes) = usable_roots(&opts)?;
        for line in &notes {
            eprintln!("ps5-dump-forge serve: {line}");
        }
        let table = Arc::new(Mutex::new(Table::default()));
        let running = Arc::new(AtomicI64::new(0));
        let jobs = {
            let (table, running) = (table.clone(), running.clone());
            Jobs::new(move |event| {
                if let Event::Done { .. } = event {
                    running.fetch_sub(1, Ordering::SeqCst);
                }
                lock(&table).record(&event);
            })
        };
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        Ok(Self {
            url: crate::console_url(local, opts.platform),
            instance: random_hex(8)?,
            roots,
            jobs,
            table,
            running,
            admission: Mutex::new(()),
            stopping: AtomicBool::new(false),
            inspecting: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
            streaming: Arc::new(AtomicUsize::new(0)),
            updating: AtomicBool::new(false),
            uploads: Mutex::new(HashSet::new()),
            cores,
            opts,
            #[cfg(test)]
            admit_pause: std::time::Duration::ZERO,
        })
    }

    pub fn handle(self: &Arc<Self>, mut stream: TcpStream) {
        let deadline = Instant::now() + self.opts.deadline;
        let mut response =
            match catch_unwind(AssertUnwindSafe(|| self.respond(&mut stream, deadline))) {
                Ok(r) => r,
                Err(_) => Response::error(500, "internal error"),
            };
        let after = response.after.take();
        http::send(&mut stream, &mut response, self.opts.write_timeout);
        if let Some(after) = after {
            after();
        }
    }

    fn respond(self: &Arc<Self>, stream: &mut TcpStream, deadline: Instant) -> Response {
        let mut head = match http::read_head(stream, deadline) {
            Ok(head) => head,
            Err(r) => return r,
        };
        if head.method == "POST" && head.path == http::UPLOAD {
            let length = head.header("content-length").map(|_| head.length as u64);
            let query = head.query.clone();
            let mut body = http::BodyReader::new(stream, &mut head, UPLOAD_IDLE);
            return self
                .edit_upload(&query, length, &mut body)
                .unwrap_or_else(|r| r);
        }
        match http::read_body(stream, &mut head, deadline) {
            Ok(body) => self.route(&head, &body).unwrap_or_else(|r| r),
            Err(r) => r,
        }
    }

    fn route(self: &Arc<Self>, head: &Head, body: &[u8]) -> Reply {
        let post = head.method == "POST";
        let Some(api) = head.path.strip_prefix("/api/") else {
            return if post {
                Err(Response::error(404, "unknown route"))
            } else {
                self.asset(&head.path)
            };
        };
        match (post, api) {
            (false, "session") => Ok(self.session()),
            (false, "qr") => self.qr(),
            (false, "elfldr") => self.elfldr(),
            (true, "inspect") => self.inspect(body),
            (true, "default_output") => {
                let arg: OutputArg = parse(body)?;
                readable(&arg.source)?;
                let password = arg.password.as_ref().map(ArchivePassword::as_str);
                let out = self.capped(|| {
                    ps5_dump_forge_core::default_output_with(
                        &arg.source,
                        arg.format,
                        &arg.dir,
                        password,
                    )
                })?;
                Ok(Response::json(&out))
            }
            (true, "generated_output") => {
                let arg: OutputArg = parse(body)?;
                readable(&arg.source)?;
                let password = arg.password.as_ref().map(ArchivePassword::as_str);
                let out = self.capped(|| {
                    ps5_dump_forge_core::generated_output_with(
                        &arg.source,
                        arg.format,
                        &arg.dir,
                        &arg.taken,
                        password,
                    )
                })?;
                Ok(Response::json(&out))
            }
            (true, "start_job") => self.start_job(body),
            (true, "cancel_job") => {
                self.jobs.cancel(parse::<IdArg>(body)?.id);
                Ok(Response::json(&()))
            }
            (true, "stale_parts") => self.stale_parts(body),
            (true, "lz4_patch") => self.lz4_patch(body, false),
            (true, "lz4_unpatch") => self.lz4_patch(body, true),
            (false, "lz4_traces") => self.lz4_traces(&head.query),
            (true, "list_dir") => self.list_dir(body),
            (true, "delete_path") => self.delete_path(body),
            (true, "edit_discard") => self.edit_discard(body),
            (true, "library_scan") => self.library_scan(body),
            (true, "library_cover") => self.library_cover(body),
            (true, "host_tree") => self.host_tree(body),
            (true, "edit_check") => self.edit_check(body),
            #[cfg(target_env = "ps5")]
            (true, "debug_bench") if crate::debug::ENABLED => {
                #[derive(Deserialize)]
                struct Q {
                    source: String,
                    dir: String,
                    mib: u64,
                }
                let q: Q = parse(body)?;
                Ok(Response::json(&crate::debug::bench(
                    &q.source, &q.dir, q.mib,
                )))
            }
            #[cfg(target_env = "ps5")]
            (true, "debug") if crate::debug::ENABLED => {
                #[derive(Deserialize)]
                struct Q {
                    path: String,
                }
                let q: Q = parse(body)?;
                Ok(Response::json(&crate::debug::probe(&q.path)))
            }
            (false, "jobs") => Ok(Response::json(&json!({
                "instance": self.instance,
                "jobs": lock(&self.table).snapshot(),
            }))),
            (true, "quit") => match parse::<serde_json::Value>(body)?["if_idle"] == true {
                true => self.quit_if_idle(),
                false => Ok(self.quit()),
            },
            (true, "self_update") => self.self_update(body),
            _ => Err(Response::error(404, "unknown route")),
        }
    }

    fn asset(&self, path: &str) -> Reply {
        let name = match path {
            "/" => "index.html",
            _ => &path[1..],
        };
        ASSETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, bytes)| Response::asset(n, bytes))
            .ok_or_else(|| Response::error(404, "not found"))
    }

    fn session(&self) -> Response {
        Response::json(&json!({
            "app": "ps5-dump-forge",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": match self.opts.platform {
                Platform::Ps5 => "ps5",
                Platform::Host => "host",
            },
            "instance": self.instance,
            "stopping": self.stopping.load(Ordering::SeqCst),
            "self_copy": crate::self_copy_status(),
            // It takes `quit {"if_idle": true}`: a newer copy started later takes over from it.
            "handover": true,
            // The page joins and splits paths with it ("\\" on a Windows host).
            "separator": std::path::MAIN_SEPARATOR_STR,
            // The page shows it: the console's own page is on 127.0.0.1.
            "url": self.url,
        }))
    }

    /// `GET /api/qr`: the session's `url` as a QR code (SVG) for a phone to open. 404 when the
    /// server couldn't tell its IP (its url then names it in words).
    fn qr(&self) -> Reply {
        if self.url.contains(' ') {
            return Err(Response::error(404, "no address to show"));
        }
        let code = qrcode::QrCode::with_error_correction_level(&self.url, qrcode::EcLevel::L)
            .map_err(|e| Response::error(500, format!("internal error: {e}")))?;
        Ok(Response::svg(qr_svg(&code)))
    }

    fn inspect(&self, body: &[u8]) -> Reply {
        let arg: InspectArg = parse(body)?;
        readable(&arg.path)?;
        let password = arg.password.as_ref().map(ArchivePassword::as_str);
        let found = self.capped(|| ps5_dump_forge_core::inspect_with(&arg.path, password))?;
        Ok(Response::json(&found))
    }

    fn refuse_if_stopping(&self) -> Result<(), Response> {
        if self.stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        Ok(())
    }

    /// The Library scan: the body's `root`, or on the PS5 the configured current roots. Capped
    /// like an inspection, and refused once stopping. On the PS5 the reply also carries the
    /// internal storage's free space (`storage`); a computer's has no such field.
    // ponytail: the request deadline covers receiving the request, not core's work.
    fn library_scan(&self, body: &[u8]) -> Reply {
        let root = optional_root(body)?;
        self.refuse_if_stopping()?;
        if root.is_none() && self.opts.platform == Platform::Host {
            return Err(Response::error(400, "choose a folder to scan"));
        }
        let found = self.capped(|| match &root {
            Some(r) => ps5_dump_forge_core::scan_library_folder(r),
            None => ps5_dump_forge_core::scan_library(&self.roots.library()),
        })?;
        Ok(Response::json(&ScanReply {
            scan: &found,
            storage: self.roots.storage(),
        }))
    }

    /// One Library cover, capped like an inspection, and refused once stopping.
    fn library_cover(&self, body: &[u8]) -> Reply {
        let arg: PathArg = parse(body)?;
        self.refuse_if_stopping()?;
        let cover = self.capped(|| ps5_dump_forge_core::library_cover(&arg.path))?;
        Ok(Response::json(&cover))
    }

    /// The Edit tab's preview of a host file or folder: core's listing, capped like an
    /// inspection. 404 when nothing is at `path` (checked by core's own read, so a path gone
    /// while waiting for a slot is a 404 too); stopping cancels a scan under way.
    fn host_tree(&self, body: &[u8]) -> Reply {
        let arg: PathArg = parse(body)?;
        let listed = self.capped(|| {
            Ok::<_, std::convert::Infallible>(ps5_dump_forge_core::host_tree(
                &arg.path,
                &self.stopping,
            ))
        })?;
        listed.map(|tree| Response::json(&tree)).map_err(|e| {
            let missing = e.chain().any(|c| {
                c.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
            });
            if missing {
                Response::error(404, format!("{e:#}"))
            } else {
                core(e)
            }
        })
    }

    /// `{request}`: which way the Edit tab's request can be applied, in place or by a rebuild
    /// (core's `Jobs::edit_check`, an `EditCheck`; read errors are findings in it). It reads
    /// the image, so it is capped like an inspection. 400 for a source or added file that is
    /// a FIFO or device.
    fn edit_check(&self, body: &[u8]) -> Reply {
        let request = parse::<StartArg>(body)?.request;
        // A FIFO or device would block core's open, holding a read slot for good.
        readable(&request.source)?;
        for edit in &request.edits {
            if let ps5_dump_forge_core::Edit::Add { from, .. } = edit {
                readable(from)?;
            }
        }
        let check =
            self.capped(|| Ok::<_, std::convert::Infallible>(self.jobs.edit_check(&request)))?;
        Ok(Response::json(&check))
    }

    /// Runs a core call that reads a source, at most [`MAX_INSPECTIONS`] at once; the rest
    /// wait for a slot (at most [`MAX_WAITING`] of them), up to [`SLOT_WAIT`], then get a 429.
    /// A 409 once stopping, before and after taking the slot.
    // ponytail: waiters poll, not a queue, so a newcomer can take a freed slot first; FIFO with
    // a Condvar if waits show up unfair on the console.
    fn capped<T, E: std::fmt::Display>(
        &self,
        call: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, Response> {
        let busy = || Response::limited("2 games are already being read; try again in a moment", 1);
        self.refuse_if_stopping()?;
        let start = Instant::now();
        let mut waits = None;
        while self.inspecting.fetch_add(1, Ordering::SeqCst) >= MAX_INSPECTIONS {
            self.inspecting.fetch_sub(1, Ordering::SeqCst);
            if waits.is_none() {
                if self.waiting.fetch_add(1, Ordering::SeqCst) >= MAX_WAITING {
                    self.waiting.fetch_sub(1, Ordering::SeqCst);
                    return Err(busy());
                }
                waits = Some(Inspecting(&self.waiting));
            }
            self.refuse_if_stopping()?;
            if start.elapsed() >= SLOT_WAIT {
                return Err(busy());
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _inspecting = Inspecting(&self.inspecting);
        drop(waits);
        self.refuse_if_stopping()?;
        call().map_err(core)
    }

    fn start_job(&self, body: &[u8]) -> Reply {
        let mut request = parse::<StartArg>(body)?.request;
        readable(&request.source)?;
        // Below the core count by default, so the server stays responsive; on the PS5 by two,
        // since its browser shares the payload's 5 CPUs (3 workers made a `.pkg` 4% slower
        // than 4, with the page usable again).
        request.compression_threads = Some(match request.compression_threads {
            None => self.cores.saturating_sub(SPARE_CORES).max(1),
            Some(n) => n.clamp(1, self.cores),
        });
        let _admission = lock(&self.admission);
        if self.stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        if self.running.load(Ordering::SeqCst) >= MAX_JOBS {
            return Err(Response::error(
                429,
                "8 jobs are already queued or running; wait for one to finish",
            ));
        }
        self.running.fetch_add(1, Ordering::SeqCst);
        // The table lock is not held here: the event callback needs it, and core can emit
        // (even `Done`) before `start` returns. The table makes an entry on the first event.
        let id =
            catch_unwind(AssertUnwindSafe(|| self.jobs.start(request.clone()))).map_err(|_| {
                self.running.fetch_sub(1, Ordering::SeqCst);
                Response::error(500, "the job runner failed to start the job")
            })?;
        #[cfg(test)]
        std::thread::sleep(self.admit_pause);
        // The job has the password; the table (the page's job list) keeps none.
        request.archive_password = None;
        lock(&self.table).admit(id, request);
        Ok(Response::json(&id))
    }

    /// Patches a game folder in place for LZ4 traces (`unpatch`: undoes it, core's
    /// `lz4_unpatch`). Refused while a job is queued or running (it could be reading the
    /// folder); admission is held throughout, so no job starts and no quit lands mid-patch.
    fn lz4_patch(&self, body: &[u8], unpatch: bool) -> Reply {
        let arg: SourceArg = parse(body)?;
        let _admission = lock(&self.admission);
        if self.stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        if self.running.load(Ordering::SeqCst) > 0 {
            return Err(Response::error(
                409,
                format!(
                    "a conversion is queued or running; {} once it has finished",
                    if unpatch { "unpatch" } else { "patch" }
                ),
            ));
        }
        let done = if unpatch {
            ps5_dump_forge_core::lz4_unpatch(&arg.source)
        } else {
            ps5_dump_forge_core::lz4_patch(&arg.source)
        }
        .map_err(core)?;
        Ok(Response::json(&done))
    }

    /// Deletes a file or folder for good (core's `Jobs::delete_path`; the page asks first).
    /// Never a browse root or a folder holding one, configured or mounted now, nor anything an
    /// unfinished job uses. Admission is held, so no quit lands mid-delete.
    fn delete_path(&self, body: &[u8]) -> Reply {
        let arg: PathArg = parse(body)?;
        let _admission = lock(&self.admission);
        if self.stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        let mut keep = self.roots.current();
        keep.extend(self.opts.roots.iter().cloned());
        self.jobs.delete_path(&arg.path, &keep).map_err(deleted)?;
        Ok(Response::json(&()))
    }

    /// `POST ?source=<abs source>&path=<game path>` (both percent-encoded), the raw file as the
    /// body (`Content-Length` required, no size limit): streamed into the source's staging
    /// folder `<source>.forge-upload-<pid>.part/<path>` (made on first use, 0777 on the PS5),
    /// replacing a file staged there before only once the whole body is in (a failed upload
    /// changes nothing). `{"path": <the staged file>}`, an edit's `Add.from`. 409 while
    /// another upload of the same file is in flight; refused once stopping.
    // ponytail: checked by path, then created by path; a link swapped into the staging folder
    // between the two needs write access to the storage.
    fn edit_upload(&self, query: &str, length: Option<u64>, body: &mut dyn Read) -> Reply {
        let Some(length) = length else {
            return Err(Response::error(411, "bad request: needs Content-Length"));
        };
        let arg = |key: &str| {
            http::query_value(query, key)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    let text = format!("bad request: needs {key}=, percent-encoded");
                    Response::error(400, text)
                })
        };
        let source = PathBuf::from(arg("source")?);
        let path = arg("path")?;
        let parts = game_path(&path)?;
        staging_dir(&source)?;
        // Every key and the staging folder come from the canonical source, so two spellings
        // of one source (a linked parent, macOS `/tmp`) share one staging tree.
        let Ok(source) = source.canonicalize() else {
            let text = format!("{}: no such file or folder", source.display());
            return Err(Response::error(404, text));
        };
        let mut dir = staging_dir(&source)?;
        self.refuse_if_stopping()?;
        let (name, parents) = parts.split_last().unwrap_or((&"", &[]));
        let staged = parents
            .iter()
            .fold(dir.clone(), |d, p| d.join(p))
            .join(name);
        // Claimed before any folder is made, so a discard waits for it or goes first.
        let _uploading = {
            let mut uploads = lock(&self.uploads);
            if !uploads.insert(staged.clone()) {
                let text = format!("{path}: another upload of it is in flight");
                return Err(Response::error(409, text));
            }
            Uploading(&self.uploads, staged.clone())
        };
        // Not a file an unfinished job uses (an edit's added file, by identity: a case alias
        // or a linked parent counts), nor a new file inside a folder an unfinished edit adds
        // (core's check matches a path not there yet only by spelling): replacing it would
        // fail that job, or change what a queued one adds. Under admission, so a job that has
        // started is in both tables.
        // ponytail: checked once, before the body; a job started while the body streams in
        // that adds this file sees it replaced (its unchanged-file check fails it).
        let used = {
            let _admission = lock(&self.admission);
            let canonical = |p: PathBuf| p.canonicalize().unwrap_or(p);
            self.jobs.uses(&staged)
                || (lock(&self.table).adds().into_iter())
                    .map(canonical)
                    .any(|a| staged.starts_with(a))
        };
        if used {
            let text = format!("{path}: an unfinished job is using it");
            return Err(Response::error(409, text));
        }
        make_dir(&dir)?;
        for part in parents {
            dir.push(part);
            make_dir(&dir)?;
        }
        if std::fs::symlink_metadata(&staged).is_ok_and(|m| !m.is_file()) {
            let text = format!("{path}: something other than a file is staged there");
            return Err(Response::error(409, text));
        }
        // Into an exclusive temporary name, renamed over the staged file once the whole body
        // is in, so a failed replacement keeps the file uploaded before.
        let seq = UPLOAD_SEQ.fetch_add(1, Ordering::Relaxed);
        // Short and apart from the target's name, so a 255-byte name still fits.
        let temp = dir.join(format!(".forge-upload-{}-{seq}.tmp", std::process::id()));
        let failed = |e: std::io::Error| core(format!("{}: {e}", staged.display()));
        let mut file = (std::fs::File::options().write(true).create_new(true))
            .open(&temp)
            .map_err(failed)?;
        let copied = (|| {
            open_up(&file)?;
            copy_body(body, &mut file, length)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, &staged)
        })();
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&temp);
            return Err(match e.kind() {
                std::io::ErrorKind::UnexpectedEof => Response::error(400, e),
                _ => failed(e),
            });
        }
        Ok(Response::json(&json!({ "path": staged })))
    }

    /// `{source}`: deletes the source's staging folder (`edit_upload`), as `delete_path`
    /// deletes (refused while an unfinished job uses it), 409 while an upload into it is in
    /// flight; `null`, also when there is none.
    fn edit_discard(&self, body: &[u8]) -> Reply {
        let arg: SourceArg = parse(body)?;
        staging_dir(&arg.source)?;
        // As `edit_upload` names it: from the canonical source, or a gone source's canonical
        // folder; with neither there is nothing to remove.
        let source = arg.source.canonicalize().ok().or_else(|| {
            let parent = arg.source.parent()?.canonicalize().ok()?;
            Some(parent.join(arg.source.file_name()?))
        });
        let Some(source) = source else {
            return Ok(Response::json(&()));
        };
        let staging = staging_dir(&source)?;
        let _admission = lock(&self.admission);
        self.refuse_if_stopping()?;
        let mut keep = self.roots.current();
        keep.extend(self.opts.roots.iter().cloned());
        // Held through the removal: an upload that comes later makes the folder afresh.
        let uploads = lock(&self.uploads);
        if uploads.iter().any(|p| p.starts_with(&staging)) {
            let text = format!("{}: an upload into it is in flight", staging.display());
            return Err(Response::error(409, text));
        }
        match self.jobs.delete_path(&staging, &keep) {
            Ok(()) | Err(DeleteError::Missing(_)) => Ok(Response::json(&())),
            Err(e) => Err(deleted(e)),
        }
    }

    /// `GET ?source=<path>` (percent-encoded): both LZ4 trace files at the root of a folder or
    /// image as one STORED zip, streamed (core's `Lz4Traces::write_zip`). Opening reads the
    /// source's metadata, so it counts as an inspection; the transfer doesn't. Jobs may run
    /// meanwhile: this only reads. A source that changes part way ends the body short.
    fn lz4_traces(&self, query: &str) -> Reply {
        let Some(source) = http::query_value(query, "source").filter(|v| !v.is_empty()) else {
            return Err(Response::error(
                400,
                "bad request: needs source=, percent-encoded",
            ));
        };
        let source = PathBuf::from(source);
        readable(&source)?;
        // Counted under `admission` after a `stopping` check, as `quit_if_idle` checks and sets,
        // so a download and a takeover never both go ahead.
        let streaming = {
            let _admission = lock(&self.admission);
            self.refuse_if_stopping()?;
            Streaming::new(&self.streaming)
        };
        let mut traces = self
            .capped(|| ps5_dump_forge_core::lz4_traces(&source))?
            .map_err(|missing| Response::error(404, missing))?;
        let (name, len) = (traces.zip_name().to_string(), traces.zip_len());
        let body = Box::new(move |out: &mut dyn std::io::Write| {
            let _streaming = streaming;
            traces.write_zip(out)
        });
        Ok(Response::download("application/zip", &name, len, body))
    }

    /// Leftover `.part` files in `dirs` (missing ones skip), each
    /// folder listed once, without the ones unfinished jobs are writing; the first 1,000 by
    /// path. Matches as core's `stale_parts` does, but keeps only what it returns while listing.
    fn stale_parts(&self, body: &[u8]) -> Reply {
        // Room for the `.part`s of every unfinished job, taken out below.
        let keep = STALE_MAX + MAX_JOBS as usize;
        let mut seen = Vec::new();
        let mut found = BTreeSet::new();
        for dir in parse::<DirsArg>(body)?
            .dirs
            .into_iter()
            .filter(|d| !d.as_os_str().is_empty())
        {
            let key = dir.canonicalize().unwrap_or(dir);
            if seen.contains(&key) {
                continue;
            }
            for entry in std::fs::read_dir(&key).into_iter().flatten().flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("part"))
                {
                    found.insert(path);
                    if found.len() > keep {
                        found.pop_last();
                    }
                }
            }
            seen.push(key);
        }
        // After the listing and under admission: a job whose `.part` was listed was started
        // before this, so it is in the table by now.
        let (running, adds) = {
            let _admission = lock(&self.admission);
            let table = lock(&self.table);
            (table.unfinished(), table.adds())
        };
        let theirs = table::running_parts(&running);
        // A staging folder (`edit_upload`) is a `.part` too: not stale while an unfinished
        // edit job adds from it or an upload into it is in flight.
        let mut busy: Vec<PathBuf> = (adds.into_iter())
            .map(|a| a.canonicalize().unwrap_or(a))
            .collect();
        busy.extend(lock(&self.uploads).iter().cloned());
        let parts: Vec<PathBuf> = found
            .into_iter()
            .filter(|p| !theirs.contains(p) && !busy.iter().any(|b| b.starts_with(p)))
            .take(STALE_MAX)
            .collect();
        Ok(Response::json(&parts))
    }

    /// Any folder; `null` lists the roots. At most 10,000 entries (`truncated` says so).
    fn list_dir(&self, body: &[u8]) -> Reply {
        let roots = self.roots.current();
        let Some(path) = parse::<ListArg>(body)?.path else {
            let entries: Vec<_> = roots
                .iter()
                .filter_map(|r| r.to_str())
                .map(|r| json!({ "name": r, "path": r, "dir": true, "size": null }))
                .collect();
            return Ok(Response::json(&json!({
                "path": null, "parent": null, "entries": entries, "truncated": false,
            })));
        };
        let failed = |e: std::io::Error| Response::error(500, format!("{path}: {e}"));
        let dir = PathBuf::from(&path).canonicalize().map_err(failed)?;
        let mut entries = Vec::new();
        let mut truncated = false;
        // ponytail: a truncated listing is the first 10,000 the folder yields, sorted, not the
        // first 10,000 by name.
        for entry in std::fs::read_dir(&dir).map_err(failed)?.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if entries.len() == LIST_MAX {
                truncated = true;
                break;
            }
            let mut full = dir.join(&name);
            // Symlinks are followed and come back resolved; a dangling one shows as itself.
            let meta = match std::fs::metadata(&full) {
                Ok(meta) => {
                    if entry.file_type().is_ok_and(|t| t.is_symlink()) {
                        full = full.canonicalize().unwrap_or(full);
                    }
                    Ok(meta)
                }
                Err(_) => entry.metadata(),
            };
            let Ok(meta) = meta else {
                continue;
            };
            entries.push((meta.is_dir(), name, full, meta.len()));
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let entries: Vec<_> = entries
            .into_iter()
            .map(|(dir, name, full, len)| {
                json!({ "name": name, "path": full, "dir": dir, "size": (!dir).then_some(len) })
            })
            .collect();
        let parent = match roots.contains(&dir) {
            true => None,
            false => dir.parent().map(PathBuf::from),
        };
        Ok(Response::json(&json!({
            "path": dir, "parent": parent, "entries": entries, "truncated": truncated,
        })))
    }

    /// Closes admission at once; once `{}` is sent, cancels every job, waits for its
    /// cleanup and exits. Repeated: `{}` and nothing more.
    fn quit(self: &Arc<Self>) -> Response {
        let mut response = Response::json(&json!({}));
        if self.stop() {
            let server = self.clone();
            response.after = Some(Box::new(move || server.finish()));
        }
        response
    }

    /// `quit {"if_idle": true}` (a newer copy taking over): quits as `quit` does, but only when
    /// no job is queued or running and no trace download is streaming; else 409.
    /// Admission within `IDLE_WAIT`, else a 409: a patch or delete holds it for long, and a
    /// late yes would act after the asker gave up (the newer copy's 3 s request, the page's
    /// minute).
    fn admission_soon(&self) -> Result<MutexGuard<'_, ()>, Response> {
        let start = Instant::now();
        loop {
            match self.admission.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) if start.elapsed() < IDLE_WAIT => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    return Err(Response::error(
                        409,
                        "Dump Forge is busy: a game is being patched or deleted",
                    ));
                }
            }
        }
    }

    fn quit_if_idle(self: &Arc<Self>) -> Reply {
        {
            let _admission = self.admission_soon()?;
            if self.stopping.load(Ordering::SeqCst) {
                return Err(Response::error(409, "Dump Forge is stopping"));
            }
            if self.running.load(Ordering::SeqCst) > 0 {
                return Err(Response::error(
                    409,
                    "Dump Forge is busy: a conversion is queued or running",
                ));
            }
            if self.streaming.load(Ordering::SeqCst) > 0 {
                return Err(Response::error(
                    409,
                    "Dump Forge is busy: a trace download is running",
                ));
            }
            self.stopping.store(true, Ordering::SeqCst);
        }
        let mut response = Response::json(&json!({}));
        let server = self.clone();
        response.after = Some(Box::new(move || server.finish()));
        Ok(response)
    }

    /// `GET /api/elfldr` (PS5 only): `{"up": bool}`, whether elfldr accepts a connection, so the
    /// page offers an update only when it can be installed.
    // ponytail: connect and close; elfldr writes "Unknown payload format" to its own log, nothing on screen.
    fn elfldr(&self) -> Reply {
        if self.opts.platform != Platform::Ps5 {
            return Err(Response::error(404, "unknown route"));
        }
        let up = TcpStream::connect_timeout(&self.opts.elfldr, Duration::from_secs(1)).is_ok();
        Ok(Response::json(&json!({ "up": up })))
    }

    /// `{"version": "X.Y.Z"}` (PS5 only): elfldr downloads that release and starts it; this
    /// server keeps serving, and the new one takes over once it is idle (`serve`). Refused for
    /// a version not newer than this one, while a job is queued or running, once stopping and
    /// while another update is waiting. Blocks while elfldr downloads (its own handler thread).
    fn self_update(&self, body: &[u8]) -> Reply {
        if self.opts.platform != Platform::Ps5 {
            return Err(Response::error(404, "unknown route"));
        }
        let version = parse::<VersionArg>(body)?.version;
        let current = env!("CARGO_PKG_VERSION");
        if !plain_version(&version)
            || crate::compare_versions(&version, current) != std::cmp::Ordering::Greater
        {
            return Err(Response::error(
                400,
                format!("bad request: version must be X.Y.Z, newer than {current}"),
            ));
        }
        if self.updating.swap(true, Ordering::SeqCst) {
            return Err(Response::error(409, "an update is already in progress"));
        }
        let _updating = Flag(&self.updating);
        {
            // Released before elfldr is asked: the new copy's `quit {"if_idle"}` needs it.
            let _admission = self.admission_soon()?;
            if self.stopping.load(Ordering::SeqCst) {
                return Err(Response::error(409, "Dump Forge is stopping"));
            }
            if self.running.load(Ordering::SeqCst) > 0 {
                return Err(Response::error(
                    409,
                    "a conversion is queued or running; update once it has finished",
                ));
            }
        }
        elfldr_load(self.opts.elfldr, &version)?;
        Ok(Response::json(&json!({})))
    }

    /// `true` for the call that closed admission. Once it returns, every admitted job is
    /// in the table.
    pub fn stop(&self) -> bool {
        let _admission = lock(&self.admission);
        !self.stopping.swap(true, Ordering::SeqCst)
    }

    pub fn finish(&self) {
        self.jobs.cancel_all_and_wait();
        #[cfg(target_env = "ps5")]
        crate::self_copy::wait();
        (self.opts.notify)(crate::notice::stopped(crate::notice::console_lang()));
        (self.opts.exit)(0);
    }
}

/// Dark modules on white with a 2-module quiet zone (the page pads it further); one path,
/// crisp edges, scaled by CSS.
fn qr_svg(code: &qrcode::QrCode) -> String {
    let (w, q) = (code.width(), 2);
    let mut d = String::new();
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == qrcode::Color::Dark {
            d += &format!("M{},{}h1v1h-1z", i % w + q, i / w + q);
        }
    }
    let n = w + 2 * q;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {n} {n}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{n}\" height=\"{n}\" fill=\"#fff\"/><path fill=\"#000\" d=\"{d}\"/></svg>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn server() -> Server {
        let opts = Options::new(0, vec![std::env::temp_dir()], Platform::Host, |_: &str| {});
        Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap()
    }

    fn post(server: &Arc<Server>, route: &str, body: serde_json::Value) -> Response {
        let head = format!("POST /api/{route} HTTP/1.1\r\nHost: x");
        let head = http::parse_head(head.as_bytes()).unwrap_or_else(|_| unreachable!());
        server
            .route(&head, body.to_string().as_bytes())
            .unwrap_or_else(|r| r)
    }

    /// Until a `start_job` on another thread holds admission (in its test pause). A fixed sleep
    /// raced it on a loaded CI runner: the thread could still be on its way in, or past it.
    fn wait_admitting(server: &Server) {
        let start = Instant::now();
        while !matches!(
            server.admission.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ) {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "start_job never got in"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    // Only the Unix-only admission test needs a folder.
    #[cfg(unix)]
    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-api-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn library_server(root: &Path) -> Arc<Server> {
        let opts = Options::new(0, vec![root.to_path_buf()], Platform::Host, |_: &str| {});
        Arc::new(Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap())
    }

    /// Scan and cover: shapes, the configured roots, malformed bodies, the 2-at-once cap,
    /// stopping, a missing cover (`null`) and an unreadable source (core's message, 500).
    #[test]
    fn library() {
        // Under the build folder, not the temp dir: a macOS temp dir is a refused system folder.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("forge-api-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plain")).unwrap();
        let root = root.canonicalize().unwrap();
        let server = library_server(&root);

        // A computer has no automatic scan: no root is a 400; a system folder is refused.
        let r = post(&server, "library_scan", json!({}));
        assert_eq!(
            (r.status, r.text().contains("choose a folder to scan")),
            (400, true)
        );
        // FreeBSD (the PS5) has no system-folder list.
        if cfg!(any(target_os = "macos", target_os = "linux", windows)) {
            // Windows refuses only the system drive's root; "/" is the current drive's (D:\ on
            // GitHub's runners), which is allowed.
            let system_root = match cfg!(windows) {
                true => {
                    let drive = std::env::var("SystemDrive").ok();
                    let letter = drive.and_then(|d| d.chars().next()).unwrap_or('C');
                    format!("{letter}:\\")
                }
                false => "/".to_string(),
            };
            let r = post(&server, "library_scan", json!({ "root": system_root }));
            assert_eq!(r.status, 500, "{}", r.text());
            assert!(r.text().contains("system folder"), "{}", r.text());
        }

        let r = post(&server, "library_scan", json!({ "root": root }));
        assert_eq!(r.status, 200, "{}", r.text());
        let v: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
        assert_eq!(v["roots"], json!([root]));
        assert_eq!(v["entries"], json!([]));
        // A computer's reply has no free space.
        assert!(v.get("storage").is_none(), "{v}");
        assert_eq!(
            (&v["truncated"], &v["warnings"]),
            (&json!(false), &json!([]))
        );

        // Another root; a non-string root is a 400.
        let other = root.join("other");
        std::fs::create_dir_all(other.join("game/sce_sys")).unwrap();
        std::fs::write(other.join("game/eboot.bin"), b"x").unwrap();
        std::fs::write(
            other.join("game/sce_sys/param.json"),
            br#"{"titleId":"PPSA00001","contentVersion":"01.000.000"}"#,
        )
        .unwrap();
        let r = post(&server, "library_scan", json!({ "root": other }));
        assert_eq!(r.status, 200, "{}", r.text());
        let v: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
        assert_eq!(v["roots"], json!([other]));
        assert_eq!(v["entries"][0]["title_id"], json!("PPSA00001"), "{v}");
        assert_eq!(v["entries"][0]["lz4_packed"], json!(false), "{v}");
        assert_eq!(v["entries"].as_array().unwrap().len(), 1);
        assert_eq!(
            post(&server, "library_scan", json!({ "root": 5 })).status,
            400
        );

        // A folder with no icon: `null`. A missing path: core's error.
        let r = post(
            &server,
            "library_cover",
            json!({ "path": root.join("plain") }),
        );
        assert_eq!((r.status, r.text()), (200, "null".to_string()));
        let gone = root.join("gone.ffpkg");
        let r = post(&server, "library_cover", json!({ "path": gone }));
        assert_eq!(r.status, 500);
        let v: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
        assert!(
            v["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{}",
            r.text()
        );

        // Bodies: not an object; no path.
        assert_eq!(post(&server, "library_scan", json!("x")).status, 400);
        assert_eq!(post(&server, "library_scan", json!([])).status, 400);
        assert_eq!(post(&server, "library_cover", json!({})).status, 400);
        assert_eq!(
            post(&server, "library_cover", json!({ "path": 1 })).status,
            400
        );

        server.inspecting.store(MAX_INSPECTIONS, Ordering::SeqCst);
        for (route, body) in [
            ("library_scan", json!({ "root": root })),
            ("library_cover", json!({ "path": root })),
        ] {
            let r = post(&server, route, body);
            assert_eq!((r.status, r.retry_after), (429, Some(1)), "{route}");
        }
        server.inspecting.store(0, Ordering::SeqCst);

        assert!(server.stop());
        for (route, body) in [
            ("library_scan", json!({ "root": root })),
            ("library_cover", json!({ "path": root })),
        ] {
            let r = post(&server, route, body);
            assert_eq!(r.status, 409, "{route}");
            assert!(r.text().contains("stopping"), "{}", r.text());
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// No patch or unpatch while a job is queued or running, or once stopping.
    #[test]
    fn lz4_patch_waits_for_jobs() {
        for (route, verb) in [("lz4_patch", "patch"), ("lz4_unpatch", "unpatch")] {
            let server = Arc::new(server());
            let body = json!({ "source": "/nonexistent/game" });
            server.running.store(1, Ordering::SeqCst);
            let r = post(&server, route, body.clone());
            assert_eq!(r.status, 409);
            assert!(
                r.text()
                    .contains(&format!("a conversion is queued or running; {verb} once")),
                "{}",
                r.text()
            );
            server.running.store(0, Ordering::SeqCst);
            // Through to core, which finds no folder.
            assert_eq!(post(&server, route, body.clone()).status, 500);
            assert!(server.stop());
            assert_eq!(post(&server, route, body).status, 409);
        }
    }

    /// Admission is held until a job is in the table, and both `stop` and `stale_parts`
    /// wait for it, though core's events can come first. A job queued behind one blocked on
    /// a FIFO stays unfinished; its `.part` (made here, as a worker would) is not stale.
    #[cfg(unix)]
    #[test]
    fn admission_is_seen_whole() {
        let tmp = dir("admit");
        let fifo = tmp.join("block.ffpfs");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let mut server = server();
        server.admit_pause = Duration::from_millis(400);
        let server = Arc::new(server);
        // Job 1, past the API's source check: blocks opening the FIFO, holding the queue.
        server.jobs.start(ConvertRequest {
            source: fifo.clone(),
            format: Format::Exfat,
            output: tmp.join("blocked.exfat"),
            compression_threads: None,
            inner: None,
            remove_backport: false,
            full_verify: false,
            kraken_level: ps5_dump_forge_core::KrakenLevel::Fast,
            ffpfsc_level: 6,
            lz4: None,
            lz4_profile: None,
            lz4_traces: None,
            lz4_trace_space_mib: 256,
            lz4_in_place: false,
            edits: Vec::new(),
            extract: Vec::new(),
            edit_rebuild: false,
            archive_password: None,
        });
        let starter = {
            let (server, out) = (server.clone(), tmp.join("out.exfat"));
            std::thread::spawn(move || {
                let request = json!({ "request": {
                    "source": "/nonexistent/forge.exfat", "format": "exfat", "output": out,
                }});
                post(&server, "start_job", request).status
            })
        };
        wait_admitting(&server); // job 2 is inside the pause
        let pid = std::process::id();
        let live = tmp.join(format!("out.exfat.2-{pid}.part"));
        let old = tmp.join("old.exfat.1-1.part");
        std::fs::write(&live, b"").unwrap();
        std::fs::write(&old, b"").unwrap();
        let start = Instant::now();
        let r = post(&server, "stale_parts", json!({ "dirs": [tmp] }));
        // Job 2's `.part` isn't stale: `stale_parts` waited for its admission.
        assert_eq!(r.status, 200);
        assert_eq!(r.text(), json!([old]).to_string());
        // Nothing an unfinished job uses is deleted, nor a folder holding it; the rest is.
        for busy in [&live, &tmp] {
            let r = post(&server, "delete_path", json!({ "path": busy }));
            assert_eq!(r.status, 409, "{}", r.text());
        }
        let r = post(&server, "delete_path", json!({ "path": old }));
        assert_eq!((r.status, r.text()), (200, "null".to_string()));
        assert!(!old.exists() && live.exists());
        assert!(server.stop());
        let r = post(&server, "delete_path", json!({ "path": live }));
        assert_eq!(r.status, 409);
        assert!(r.text().contains("stopping"), "{}", r.text());
        assert!(
            lock(&server.table)
                .unfinished()
                .iter()
                .any(|(id, _)| *id == 2)
        );
        assert_eq!(starter.join().unwrap(), 200);

        server.jobs.cancel(2);
        // Opening the write end wakes job 1 to an empty file (non-blocking: it fails until
        // job 1 is waiting on its end).
        use std::os::unix::fs::OpenOptionsExt;
        let open = || {
            std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
        };
        while open().is_err() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(20));
        }
        server.jobs.cancel_all_and_wait();
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// `start_job` passes an edit job's `edits` (and `edit_rebuild`) through, never checks its
    /// `output`, and keys it by its source. While it is unfinished (queued behind a job blocked
    /// on a FIFO), a file it adds can't be uploaded again (409), its staging folder isn't a
    /// stale `.part`, and discard refuses it; once it is done, all three go through.
    #[cfg(unix)]
    #[test]
    fn edit_job_pass_through() {
        let tmp = dir("editjob");
        let game = tmp.join("game");
        std::fs::create_dir(&game).unwrap();
        let fifo = tmp.join("block.ffpfs");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let server = Arc::new(server());
        // Job 1, past the API's source check (a FIFO is refused): blocks opening it.
        let blocker = json!({
            "source": fifo, "format": "exfat", "output": tmp.join("blocked.exfat"),
        });
        server.jobs.start(serde_json::from_value(blocker).unwrap());
        let r = upload(&server, &game, "data/x.bin", b"new");
        assert_eq!(r.status, 200, "{}", r.text());
        let staged: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
        let staged = PathBuf::from(staged["path"].as_str().unwrap());
        let edits = json!([
            { "op": "add", "from": staged, "to": "data/x.bin" },
            { "op": "delete", "path": "old" },
        ]);
        // An output nobody could write: an edit job's is ignored.
        let request = json!({ "request": {
            "source": game, "format": "exfat", "output": "/nonexistent/no/out.exfat",
            "edits": edits, "edit_rebuild": true,
        }});
        let r = post(&server, "start_job", request);
        assert_eq!((r.status, r.text()), (200, "2".to_string()));
        let jobs = lock(&server.table).snapshot();
        let admitted = &jobs.iter().find(|j| j["id"] == 2).unwrap()["request"];
        assert_eq!(admitted["edits"], edits);
        assert_eq!(admitted["edit_rebuild"], true);
        assert!(
            lock(&server.table)
                .unfinished()
                .contains(&(2, game.clone()))
        );

        let r = upload(&server, &game, "data/x.bin", b"newer");
        assert_eq!(r.status, 409, "{}", r.text());
        assert!(
            r.text().contains("an unfinished job is using it"),
            "{}",
            r.text()
        );
        assert_eq!(std::fs::read(&staged).unwrap(), b"new");
        assert_eq!(upload(&server, &game, "data/y.bin", b"y").status, 200);
        let r = post(&server, "stale_parts", json!({ "dirs": [tmp] }));
        assert_eq!(r.text(), "[]");
        let r = post(&server, "edit_discard", json!({ "source": game }));
        assert_eq!(r.status, 409, "{}", r.text());

        server.jobs.cancel(2);
        let start = Instant::now();
        while !lock(&server.table).adds().is_empty() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(upload(&server, &game, "data/x.bin", b"newer").status, 200);
        let r = post(&server, "stale_parts", json!({ "dirs": [tmp] }));
        assert_eq!(
            r.text(),
            json!([staged.parent().unwrap().parent().unwrap()]).to_string()
        );
        let r = post(&server, "edit_discard", json!({ "source": game }));
        assert_eq!((r.status, r.text()), (200, "null".to_string()));
        assert!(!staged.exists() && game.exists());

        use std::os::unix::fs::OpenOptionsExt;
        let open = || {
            (std::fs::OpenOptions::new().write(true))
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
        };
        while open().is_err() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(20));
        }
        server.jobs.cancel_all_and_wait();
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// `quit {"if_idle": true}`: 409 while a job is queued or running, a trace download is
    /// streaming or once stopping; else `{}`, stopping, and quitting after the reply. A plain
    /// `quit` is unchanged.
    #[test]
    fn quit_if_idle() {
        let server = Arc::new(server());
        let idle = json!({ "if_idle": true });
        server.running.store(1, Ordering::SeqCst);
        let r = post(&server, "quit", idle.clone());
        assert_eq!(r.status, 409);
        assert!(
            r.text().contains("a conversion is queued or running"),
            "{}",
            r.text()
        );
        server.running.store(0, Ordering::SeqCst);
        let stream = Streaming::new(&server.streaming);
        let r = post(&server, "quit", idle.clone());
        assert_eq!(r.status, 409);
        assert!(
            r.text().contains("a trace download is running"),
            "{}",
            r.text()
        );
        drop(stream);
        // Admission held (a patch or delete): busy after a short wait, never stopping later.
        let held = lock(&server.admission);
        let r = post(&server, "quit", idle.clone());
        assert_eq!(r.status, 409);
        assert!(r.text().contains("patched or deleted"), "{}", r.text());
        drop(held);
        assert!(!server.stopping.load(Ordering::SeqCst));
        let r = post(&server, "quit", idle.clone());
        assert_eq!((r.status, r.text()), (200, "{}".to_string()));
        assert!(r.after.is_some() && server.stopping.load(Ordering::SeqCst));
        let r = post(&server, "quit", idle);
        assert_eq!(r.status, 409);
        assert!(r.text().contains("stopping"), "{}", r.text());
        // A plain quit once stopping: `{}` and nothing more.
        let r = post(&server, "quit", json!({}));
        assert_eq!((r.status, r.after.is_none()), (200, true));
    }

    /// `self_update`: the PS5's only; a newer `X.Y.Z` only; never while a job is queued or
    /// running, another update waits or the server is stopping (all before elfldr is asked).
    #[test]
    fn self_update_refusals() {
        let r = post(
            &Arc::new(server()),
            "self_update",
            json!({ "version": "999.0.0" }),
        );
        assert_eq!((r.status, r.text().contains("unknown route")), (404, true));

        let mut opts = Options::new(0, vec![], Platform::Ps5, |_: &str| {});
        // A closed port, in case a check below lets a request through.
        opts.elfldr = "127.0.0.1:1".parse().unwrap();
        let server = Arc::new(Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap());
        let current = env!("CARGO_PKG_VERSION");
        for bad in [
            json!({ "version": current }),
            json!({ "version": "0.0.1" }),
            json!({ "version": "999.0" }),
            json!({ "version": "999.0.0-pre1" }),
            json!({ "version": "v999.0.0" }),
            json!({ "version": "999.0.0/../x" }),
            json!({ "version": "9999999999.0.0" }),
            json!({ "version": 1 }),
            json!({}),
        ] {
            assert_eq!(
                post(&server, "self_update", bad.clone()).status,
                400,
                "{bad}"
            );
        }
        let newer = json!({ "version": "999.0.0" });
        server.running.store(1, Ordering::SeqCst);
        let r = post(&server, "self_update", newer.clone());
        assert_eq!(r.status, 409);
        assert!(
            r.text().contains("a conversion is queued or running"),
            "{}",
            r.text()
        );
        server.running.store(0, Ordering::SeqCst);
        // A patch or delete holding admission past 1 s: refused before elfldr is asked.
        {
            let _held = server.admission.lock().unwrap();
            let r = post(&server, "self_update", newer.clone());
            assert_eq!(r.status, 409);
            assert!(r.text().contains("patched or deleted"), "{}", r.text());
        }
        server.updating.store(true, Ordering::SeqCst);
        let r = post(&server, "self_update", newer.clone());
        assert_eq!(r.status, 409);
        assert!(r.text().contains("already in progress"), "{}", r.text());
        // The refusal leaves the other update's flag alone.
        assert!(server.updating.load(Ordering::SeqCst));
        server.updating.store(false, Ordering::SeqCst);
        assert!(server.stop());
        let r = post(&server, "self_update", newer);
        assert_eq!(r.status, 409);
        assert!(r.text().contains("stopping"), "{}", r.text());
        assert!(!server.updating.load(Ordering::SeqCst));
    }

    /// One request over a real socket, through `handle`; the raw reply.
    fn over_tcp(server: &Arc<Server>, request: &[u8]) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let s = server.clone();
        let t = std::thread::spawn(move || s.handle(listener.accept().unwrap().0));
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(request).unwrap();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        t.join().unwrap();
        reply
    }

    fn upload(server: &Server, source: &Path, path: &str, body: &[u8]) -> Response {
        let query = format!(
            "source={}&path={}",
            percent_encode(&source.to_string_lossy()),
            percent_encode(path)
        );
        (server.edit_upload(&query, Some(body.len() as u64), &mut &body[..])).unwrap_or_else(|r| r)
    }

    /// Upload (over the socket past `BODY_MAX`, and directly), replacing, refusals, the
    /// staging folder as a stale `.part`, and discard (twice: the second is a no-op).
    #[test]
    fn edit_upload_and_discard() {
        let root = std::env::temp_dir().join(format!("forge-api-upload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // Canonical, as the reply's paths are (a macOS temp dir is under a link).
        let root = root.canonicalize().unwrap();
        let source = root.join("game.ffpkg");
        std::fs::write(&source, b"image").unwrap();
        let server = Arc::new(server());
        let staging = root.join(format!(
            "game.ffpkg.forge-upload-{}.part",
            std::process::id()
        ));

        let big: Vec<u8> = (0..http::BODY_MAX as u32 * 2 + 7)
            .map(|i| i as u8)
            .collect();
        let target = format!(
            "/api/edit_upload?source={}&path=data%2Fa%20b.bin",
            percent_encode(&source.to_string_lossy())
        );
        let head = format!(
            "POST {target} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n",
            big.len()
        );
        let reply = over_tcp(&server, &[head.as_bytes(), &big].concat());
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        let staged = staging.join("data").join("a b.bin");
        let body = reply.split_once("\r\n\r\n").unwrap().1;
        let found: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(found["path"], json!(staged));
        assert_eq!(std::fs::read(&staged).unwrap(), big);

        // A second upload to the same path replaces the file.
        assert_eq!(upload(&server, &source, "data/a b.bin", b"new").status, 200);
        assert_eq!(std::fs::read(&staged).unwrap(), b"new");

        for bad in ["", "/abs", "a/../b", "a//b", "./a", "a/", "a\\b", "a\0b"] {
            assert_eq!(upload(&server, &source, bad, b"x").status, 400, "{bad:?}");
        }
        // A folder is in the way, then a file is.
        assert_eq!(upload(&server, &source, "data", b"x").status, 409);
        assert_eq!(upload(&server, &source, "data/a b.bin/c", b"x").status, 409);
        let missing = root.join("gone.exfat");
        assert_eq!(upload(&server, &missing, "a", b"x").status, 404);
        assert_eq!(
            upload(&server, Path::new("rel.exfat"), "a", b"x").status,
            400
        );
        let r = server.edit_upload("source=%2Fx&path=a", None, &mut &b""[..]);
        assert_eq!(r.unwrap_or_else(|r| r).status, 411);
        // A body that ends early leaves no file.
        let query = format!(
            "source={}&path=short.bin",
            percent_encode(&source.to_string_lossy())
        );
        let r = server.edit_upload(&query, Some(10), &mut &b"12345"[..]);
        assert_eq!(r.unwrap_or_else(|r| r).status, 400);
        assert!(!staging.join("short.bin").exists());
        // A 255-byte name: the temporary name doesn't grow with it.
        let long = "n".repeat(251) + ".bin";
        assert_eq!(upload(&server, &source, &long, b"long").status, 200);
        assert_eq!(std::fs::read(staging.join(&long)).unwrap(), b"long");
        // A replacement that fails part way keeps the file uploaded before, and no temp file.
        let query = format!(
            "source={}&path=data%2Fa%20b.bin",
            percent_encode(&source.to_string_lossy())
        );
        let r = server.edit_upload(&query, Some(10), &mut &b"12345"[..]);
        assert_eq!(r.unwrap_or_else(|r| r).status, 400);
        assert_eq!(std::fs::read(&staged).unwrap(), b"new");
        let names: Vec<_> = std::fs::read_dir(staging.join("data"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");

        let r = post(&server, "stale_parts", json!({ "dirs": [root] }));
        assert!(r.text().contains("forge-upload-"), "{}", r.text());
        let r = post(&server, "edit_discard", json!({ "source": source }));
        assert_eq!((r.status, r.text()), (200, "null".to_string()));
        assert!(!staging.exists() && source.exists());
        assert_eq!(
            post(&server, "edit_discard", json!({ "source": source })).status,
            200
        );
        assert_eq!(
            post(&server, "edit_discard", json!({ "source": "rel" })).status,
            400
        );
        assert_eq!(post(&server, "edit_discard", json!({})).status, 400);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn helpers() {
        assert!(plain_version("0.1.2") && plain_version("10.20.30"));
        assert!(!plain_version("0.1") && !plain_version("0.1.2.3") && !plain_version("0..2"));
        assert_eq!(
            percent_encode("https://x/a b?args=serve&pipe=0"),
            "https%3A%2F%2Fx%2Fa%20b%3Fargs%3Dserve%26pipe%3D0"
        );
        use std::cmp::Ordering::*;
        for (a, b, want) in [
            ("0.1.2", "0.1.1", Greater),
            ("0.1.10", "0.1.9", Greater),
            ("0.1.2", "0.1.2-pre4", Greater),
            ("0.1.2-pre10", "0.1.2-pre4", Greater),
            ("0.1.1", "0.1.1", Equal),
            ("", "0.0.1", Less),
        ] {
            assert_eq!(crate::compare_versions(a, b), want, "{a} vs {b}");
        }
        assert_eq!(crate::status_of(b"HTTP/1.1 200 OK\r\n\r\n"), Some(200));
        assert_eq!(crate::status_of(b"HTTP/1.0 409 Conflict"), Some(409));
        assert_eq!(crate::status_of(b"[elfldr.elf] Error"), None);
        assert_eq!(crate::status_of(b""), None);
    }

    /// The same without a blocked queue: a job whose `Done` came before `start` returned.
    #[test]
    fn stop_sees_every_admitted_job() {
        let mut server = server();
        server.admit_pause = Duration::from_millis(300);
        let server = Arc::new(server);
        let starter = {
            let server = server.clone();
            std::thread::spawn(move || {
                let body = json!({ "request": {
                    "source": "/nonexistent/forge.exfat", "format": "exfat",
                    "output": "/nonexistent/out.exfat",
                }});
                server.start_job(body.to_string().as_bytes()).is_ok()
            })
        };
        wait_admitting(&server); // inside the pause
        assert!(server.stop());
        assert!(lock(&server.table).all_admitted());
        assert_eq!(lock(&server.table).snapshot().len(), 1);
        assert!(starter.join().unwrap());
    }
}
