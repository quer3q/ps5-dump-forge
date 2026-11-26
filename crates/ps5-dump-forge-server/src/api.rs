//! The routes, each mirroring a Tauri command (same core call, same `{e:#}` error text) plus limits.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{Read, Seek, Write};
use std::net::{SocketAddr, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ps5_dump_forge_core::{
    ArchivePassword, ConvertRequest, DeleteError, Event, Format, JobId, Jobs, ListEntry,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::http::{self, Head, Response};
use crate::paths::Roots;
use crate::table::{self, Table};
use crate::{Options, Platform, random_hex};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

const MAX_JOBS: i64 = 8;
const MAX_INSPECTIONS: usize = 2;
/// Bounded so waiting reads never hold the handlers Cancel and Stop need.
const MAX_WAITING: usize = 4;
const SLOT_WAIT: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(100)
} else {
    std::time::Duration::from_secs(30)
};
const IDLE_WAIT: Duration = Duration::from_secs(1);
/// Per read, not for the whole body: a GiB takes minutes.
const UPLOAD_IDLE: Duration = Duration::from_secs(30);
const STALE_MAX: usize = 1_000;

type Reply = Result<Response, Response>;

/// Pure reads: identical bodies share one run and its reply.
const SHARED_READS: [&str; 7] = [
    "inspect",
    "default_output",
    "generated_output",
    "package_scan",
    "host_tree",
    "pack_estimate",
    "edit_check",
];

pub(crate) struct Server {
    pub opts: Options,
    pub url: String,
    roots: Arc<Roots>,
    instance: String,
    jobs: Arc<Jobs>,
    table: Arc<Mutex<Table>>,
    /// Admitted minus finished jobs; only raised under `admission`, lowered by `Done`.
    running: Arc<AtomicI64>,
    /// Held from the admission checks until the job is in the table.
    admission: Mutex<()>,
    stopping: AtomicBool,
    /// Deletes and moves running outside `admission`; `finish` waits for them.
    deleting: AtomicUsize,
    inspecting: AtomicUsize,
    waiting: AtomicUsize,
    reads: crate::shared::Reads,
    updating: AtomicBool,
    /// Staged files being uploaded: one per file, and no discard of their folder meanwhile.
    uploads: Mutex<HashSet<PathBuf>>,
    cores: usize,
    /// `Err` names why Connect isn't running.
    connect: Result<Arc<crate::connect::Connect>, String>,
    notices: Arc<crate::notice::JobNotices>,
    /// Tests widen the gap between `Jobs::start` returning and the table admitting the job.
    #[cfg(test)]
    admit_pause: std::time::Duration,
}

/// Cores a default job leaves to the server (and on the PS5, the console's browser).
const SPARE_CORES: usize = if cfg!(target_env = "ps5") { 2 } else { 1 };

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, Response> {
    serde_json::from_slice(body).map_err(|e| Response::error(400, format!("bad request: {e}")))
}

fn core(e: impl std::fmt::Display) -> Response {
    Response::error(500, format!("{e:#}"))
}

/// A folder or regular file (links followed): a FIFO or device would block its handler on open.
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

fn deleted(e: DeleteError) -> Response {
    let status = match e {
        DeleteError::Refused(_) => 400,
        DeleteError::Missing(_) => 404,
        DeleteError::Busy(_) => 409,
        DeleteError::Failed(_) => 500,
    };
    Response::error(status, e.to_string())
}

/// No empty, `.` or `..` component, no `\` or NUL (on Windows no `:`: `push` reads a drive).
// ponytail: core's `Edit` path rule restated by hand; core checks the job's paths again anyway.
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

/// `<source>.forge-upload-<pid>.part`: a `.part`, so `stale_parts` lists a leftover.
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

/// CE-107750-0: the game needs 0777, set explicitly past the umask. A no-op off FreeBSD.
fn open_up(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(target_os = "freebsd")]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o777))?;
    }
    let _ = file;
    Ok(())
}

/// A client that ends early or stalls is `UnexpectedEof` (a 400); a write error keeps its kind.
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

struct Uploading<'a>(&'a Mutex<HashSet<PathBuf>>, PathBuf);

impl Drop for Uploading<'_> {
    fn drop(&mut self) {
        lock(self.0).remove(&self.1);
    }
}

static UPLOAD_SEQ: AtomicU64 = AtomicU64::new(0);

/// A chunked upload by client id: one `.part`, renamed once `size` bytes came; one idle for
/// `CHUNK_IDLE` is dropped by the next chunk. ponytail: a server exit leaves its `.part` behind.
struct Chunked {
    part: PathBuf,
    target: PathBuf,
    size: u64,
    replace: bool,
    offsets: HashSet<u64>,
    got: u64,
    touched: Instant,
}

static CHUNKED: std::sync::LazyLock<Mutex<HashMap<String, Chunked>>> =
    std::sync::LazyLock::new(Mutex::default);
/// Ended ids, so a chunk still in flight doesn't start a new `.part`. ponytail: the last 256.
static CHUNK_ENDED: Mutex<std::collections::VecDeque<String>> =
    Mutex::new(std::collections::VecDeque::new());
/// `CHUNK` in http-client.ts.
const UPLOAD_CHUNK: u64 = 8 << 20;
const CHUNK_IDLE: Duration = Duration::from_secs(600);

fn end_chunked(all: &mut HashMap<String, Chunked>, id: &str) {
    if let Some(c) = all.remove(id) {
        let _ = std::fs::remove_file(c.part);
    }
    let mut ended = lock(&CHUNK_ENDED);
    if !ended.iter().any(|e| e == id) {
        if ended.len() >= 256 {
            ended.pop_front();
        }
        ended.push_back(id.to_string());
    }
}

struct Inspecting<'a>(&'a AtomicUsize);

impl Drop for Inspecting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Flag<'a>(&'a AtomicBool);

struct Deleting<'a>(&'a AtomicUsize);

impl Drop for Deleting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Drop for Flag<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Checked before a takeover too, so a bad start never stops a good server.
pub(crate) fn usable_roots(opts: &Options) -> std::io::Result<(Roots, Vec<String>)> {
    let (roots, notes) = Roots::new(opts.platform, &opts.roots);
    if opts.platform == Platform::Host && roots.current().is_empty() {
        return Err(std::io::Error::other("no usable --root folder"));
    }
    Ok((roots, notes))
}

/// Never deleted, moved or replaced, nor a folder holding it: the roots and, on the PS5, Forge's files.
fn keep_list(roots: &Roots, configured: &[PathBuf]) -> Vec<PathBuf> {
    let mut keep = roots.current();
    keep.extend(configured.iter().cloned());
    #[cfg(target_env = "ps5")]
    keep.extend(
        [
            crate::self_copy::TARGET,
            crate::self_copy::TARGET_LOCK,
            crate::tile::OWNER_FILE,
        ]
        .map(PathBuf::from),
    );
    keep
}

#[derive(Deserialize)]
struct PathArg {
    path: PathBuf,
}

#[derive(Deserialize)]
struct MoveArg {
    from: PathBuf,
    to: PathBuf,
    #[serde(default)]
    replace: bool,
}

#[derive(Deserialize)]
struct InspectArg {
    path: PathBuf,
    #[serde(default)]
    password: Option<ArchivePassword>,
}

#[derive(Deserialize)]
struct PackEstimateArg {
    source: PathBuf,
    kind: ps5_dump_forge_core::EstimateKind,
    level: u8,
    #[serde(default)]
    password: Option<ArchivePassword>,
    /// Kraken or zlib of a packed game whose packs the build decodes.
    #[serde(default)]
    unpack: bool,
}

/// `pack` and `naming` for generated names only (`naming` absent: the default template).
#[derive(Deserialize)]
struct OutputArg {
    source: PathBuf,
    format: Format,
    dir: PathBuf,
    #[serde(default)]
    taken: Vec<PathBuf>,
    #[serde(default)]
    password: Option<ArchivePassword>,
    #[serde(default)]
    pack: Option<ps5_dump_forge_core::PackKind>,
    #[serde(default)]
    naming: Option<String>,
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
    /// The Storage explorer: core's `list_explorer`.
    #[serde(default)]
    explorer: bool,
}

impl Server {
    pub fn new(opts: Options, local: SocketAddr) -> std::io::Result<Self> {
        let (roots, notes) = usable_roots(&opts)?;
        let roots = Arc::new(roots);
        for line in &notes {
            eprintln!("ps5-dump-forge serve: {line}");
        }
        let table = Arc::new(Mutex::new(Table::default()));
        let running = Arc::new(AtomicI64::new(0));
        let connect = match &opts.connect_dir {
            None => Err("Connect is off in this server".to_string()),
            Some(dir) => crate::connect::Connect::start(dir, opts.connect_port)
                .map(Arc::new)
                .map_err(|e| format!("Connect couldn't start: {}: {e}", dir.display())),
        };
        if let Err(e) = &connect
            && opts.connect_dir.is_some()
        {
            eprintln!("ps5-dump-forge serve: {e}");
        }
        let notices = Arc::new(crate::notice::JobNotices::new(
            opts.settings_dir.clone(),
            (opts.platform == Platform::Ps5).then(|| opts.notify.clone()),
        ));
        let jobs = {
            let (table, running, notices) = (table.clone(), running.clone(), notices.clone());
            let connect = connect.as_ref().ok().map(Arc::downgrade);
            Arc::new(Jobs::new(move |event| {
                if let Event::Done { .. } = event {
                    running.fetch_sub(1, Ordering::SeqCst);
                }
                let ended = lock(&table).record(&event);
                notices.send(ended);
                if let Some(c) = connect.as_ref().and_then(std::sync::Weak::upgrade) {
                    c.job_event(&event);
                }
            }))
        };
        {
            let (roots, configured) = (roots.clone(), opts.roots.clone());
            jobs.set_protected(Arc::new(move || keep_list(&roots, &configured)));
        }
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        if let Ok(connect) = &connect {
            connect.set_busy(jobs.uses_hook());
            jobs.set_external_busy(connect.writing_hook());
            let weak = Arc::downgrade(&jobs);
            connect.set_replace_check(Arc::new(move |path| match weak.upgrade() {
                Some(jobs) => jobs.replace_check(path).map_err(|e| e.to_string()),
                None => Err("Dump Forge is stopping".to_string()),
            }));
            let weak = Arc::downgrade(&jobs);
            connect.set_cancel(Arc::new(move |id| {
                if let Some(jobs) = weak.upgrade() {
                    jobs.cancel(id);
                }
            }));
        }
        Ok(Self {
            url: crate::console_url(local, opts.platform),
            instance: random_hex(8)?,
            roots,
            jobs,
            table,
            running,
            admission: Mutex::new(()),
            stopping: AtomicBool::new(false),
            deleting: AtomicUsize::new(0),
            inspecting: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
            reads: Default::default(),
            updating: AtomicBool::new(false),
            uploads: Mutex::new(HashSet::new()),
            cores,
            connect,
            notices,
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
        if head.method == "POST" && head.path == http::CONNECT_UPLOAD {
            // U8: auto rest mode waits while an upload streams in.
            let _awake = ps5_dump_forge_core::awake();
            let length = head.header("content-length").map(|_| head.length as u64);
            let query = head.query.clone();
            let mut body = http::BodyReader::new(stream, &mut head, UPLOAD_IDLE);
            return self
                .connect_upload(&query, length, &mut body)
                .unwrap_or_else(|r| r);
        }
        if head.method == "POST" && head.path == http::UPLOAD {
            let _awake = ps5_dump_forge_core::awake();
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
        #[cfg(target_env = "ps5")]
        if crate::debug::ENABLED && !post && head.path == "/probe" {
            return Ok(Response::asset("probe.html", crate::debug::PROBE_PAGE));
        }
        let Some(api) = head.path.strip_prefix("/api/") else {
            return if post {
                Err(Response::error(404, "unknown route"))
            } else {
                self.asset(head)
            };
        };
        match (post, api) {
            (false, "session") => Ok(self.session()),
            (false, "qr") => self.qr(&head.query),
            (false, "elfldr") => self.elfldr(),
            (true, api) if SHARED_READS.contains(&api) => {
                let retry = head.query.split('&').any(|q| q == "retry=1");
                (self.reads).run(api, body, retry, &self.stopping, || self.read(api, body))
            }
            (true, "start_job") => self.start_job(body),
            (true, "cancel_job") => {
                self.jobs.cancel(parse::<IdArg>(body)?.id);
                Ok(Response::json(&()))
            }
            (true, "stale_parts") => self.stale_parts(body),
            (true, "list_dir") => self.list_dir(body),
            (true, "delete_path") => self.delete_path(body),
            (true, "move_path") => self.move_path(body),
            (true, "edit_discard") => self.edit_discard(body),
            (true, "storage") => Ok(Response::json(&self.roots.storage())),
            (true, "free_space") => {
                let path = parse::<PathArg>(body)?.path;
                (self.roots.confine(&path)).map_err(|e| Response::error(400, e))?;
                Ok(Response::json(&ps5_dump_forge_core::free_space(&path)))
            }
            (true, "existing_targets") => self.existing_targets(body),
            #[cfg(target_env = "ps5")]
            (true, "debug_input") if crate::debug::ENABLED => {
                crate::debug::input_push(body);
                Ok(Response::json(&()))
            }
            #[cfg(target_env = "ps5")]
            (false, "debug_input") if crate::debug::ENABLED => {
                Ok(Response::json(&crate::debug::input_lines()))
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
            (true, "notifications") => self.set_notifications(body),
            (false, "connect/state") => Ok(Response::json(&self.connect()?.snapshot())),
            (false, "connect/download") => self.connect_download(&head.query),
            (true, route) if route.starts_with("connect/") => {
                self.connect_route(&route["connect/".len()..], body)
            }
            _ => Err(Response::error(404, "unknown route")),
        }
    }

    fn asset(&self, head: &Head) -> Reply {
        let name = match head.path.as_str() {
            "/" => "index.html",
            path => &path[1..],
        };
        let (name, gzip, bytes) = ASSETS
            .iter()
            .find(|(n, ..)| *n == name)
            .ok_or_else(|| Response::error(404, "not found"))?;
        Ok(match gzip {
            true => Response::gzip_asset(name, bytes, head.header("accept-encoding")),
            false => Response::asset(name, bytes),
        })
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
            "handover": true,
            "bundled_ng_version": ps5_dump_forge_lz4::runtime::NG_VERSION,
            "cli_asset": match self.opts.platform {
                Platform::Ps5 => None,
                Platform::Host => ps5_dump_forge_updater::cli_asset(),
            },
            "separator": std::path::MAIN_SEPARATOR_STR,
            "url": self.url,
            "connect_url": format!("{}/#connect", self.url),
            "debug": cfg!(target_env = "ps5") && option_env!("FORGE_DEBUG_API").is_some(),
            "notifications": self.notices.on(),
        }))
    }

    fn set_notifications(&self, body: &[u8]) -> Reply {
        #[derive(Deserialize)]
        struct Arg {
            on: bool,
        }
        let on = parse::<Arg>(body)?.on;
        self.notices.set(on).map_err(|e| Response::error(500, e))?;
        Ok(Response::json(&json!({ "on": self.notices.on() })))
    }

    /// 404 when the server couldn't tell its IP (its url then names it in words).
    fn qr(&self, query: &str) -> Reply {
        if self.url.contains(' ') {
            return Err(Response::error(404, "no address to show"));
        }
        let url = match http::query_value(query, "connect") {
            Some(_) => format!("{}/#connect", self.url),
            None => self.url.clone(),
        };
        let code = qrcode::QrCode::with_error_correction_level(&url, qrcode::EcLevel::L)
            .map_err(|e| Response::error(500, format!("internal error: {e}")))?;
        Ok(Response::svg(qr_svg(&code)))
    }

    fn read(&self, api: &str, body: &[u8]) -> Reply {
        match api {
            "inspect" => self.inspect(body),
            "default_output" => {
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
            "generated_output" => {
                let arg: OutputArg = parse(body)?;
                let naming = arg.naming.as_deref();
                if let Some(why) = naming.and_then(ps5_dump_forge_core::naming_problem) {
                    return Err(Response::error(400, why));
                }
                readable(&arg.source)?;
                let password = arg.password.as_ref().map(ArchivePassword::as_str);
                let out = self.capped(|| {
                    ps5_dump_forge_core::generated_output_with(
                        &arg.source,
                        arg.format,
                        &arg.dir,
                        &arg.taken,
                        password,
                        arg.pack,
                        naming,
                    )
                })?;
                Ok(Response::json(&out))
            }
            "package_scan" => self.package_scan(body),
            "host_tree" => self.host_tree(body),
            "pack_estimate" => self.pack_estimate(body),
            "edit_check" => self.edit_check(body),
            _ => Err(Response::error(404, "unknown route")),
        }
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

    fn package_scan(&self, body: &[u8]) -> Reply {
        parse::<serde_json::Map<String, serde_json::Value>>(body)?;
        self.refuse_if_stopping()?;
        if self.opts.platform != Platform::Ps5 {
            return Err(Response::error(400, "packages install only on a PS5"));
        }
        let found = self.capped(|| ps5_dump_forge_core::scan_packages(&self.roots.current()))?;
        Ok(Response::json(&found))
    }

    /// 404 by core's own read, so a path gone while waiting for a slot is a 404 too.
    fn host_tree(&self, body: &[u8]) -> Reply {
        let arg: PathArg = parse(body)?;
        self.roots
            .confine(&arg.path)
            .map_err(|e| Response::error(400, e))?;
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

    fn pack_estimate(&self, body: &[u8]) -> Reply {
        let arg: PackEstimateArg = parse(body)?;
        readable(&arg.source)?;
        let found = self.capped(|| {
            Ok::<_, std::convert::Infallible>(ps5_dump_forge_core::pack_estimate(
                &arg.source,
                arg.kind,
                arg.level,
                arg.password.as_ref(),
                arg.unpack,
            ))
        })?;
        found
            .map(|e| Response::json(&e))
            .map_err(|e| Response::error(400, format!("{e:#}")))
    }

    fn edit_check(&self, body: &[u8]) -> Reply {
        let request = parse::<StartArg>(body)?.request;
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

    /// Capped: it may ask a peer or a NAS.
    fn existing_targets(&self, body: &[u8]) -> Reply {
        let request = parse::<StartArg>(body)?.request;
        if request.extract.is_empty() && request.transfer.is_empty() {
            return Err(Response::error(
                400,
                "bad request: only an extract or a transfer has targets to check",
            ));
        }
        let targets = self.capped(|| self.jobs.existing_targets(&request))?;
        Ok(Response::json(&targets))
    }

    /// At most [`MAX_INSPECTIONS`] at once, [`MAX_WAITING`] waiting up to [`SLOT_WAIT`], else 429.
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
        let id = self.start(parse::<StartArg>(body)?.request)?;
        Ok(Response::json(&id))
    }

    fn start(&self, mut request: ConvertRequest) -> Result<JobId, Response> {
        readable(&request.source)?;
        for p in request.transfer.iter().chain(&request.install) {
            readable(p)?;
        }
        // PS5: two cores left, its browser shares the payload's 5 CPUs.
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
        // The table (the page's job list) keeps no password.
        request.archive_password = None;
        if let Ok(connect) = &self.connect {
            connect.job_started(id, &request);
        }
        // Done may have come first: the notice goes once, here or from the callback.
        let ended = lock(&self.table).admit(id, request);
        drop(_admission);
        self.notices.send(ended);
        Ok(id)
    }

    /// A peer's Delete, Move to and (PS5) `install` run as this server's own routes do.
    pub(crate) fn wire_install(self: &Arc<Self>) {
        let Ok(connect) = &self.connect else { return };
        let gone = || DeleteError::Refused("Dump Forge is stopping".to_string());
        let weak = Arc::downgrade(self);
        let delete: crate::connect_fs::DeleteHook = Arc::new(move |path| {
            let server = weak.upgrade().ok_or_else(gone)?;
            let _deleting = server.admit_delete().map_err(|_| gone())?;
            server.jobs.delete_path(path, &server.keep())
        });
        let weak = Arc::downgrade(self);
        let move_: crate::connect_fs::MoveHook = Arc::new(move |from, to, replace| {
            let server = weak.upgrade().ok_or_else(gone)?;
            let _deleting = server.admit_delete().map_err(|_| gone())?;
            server.jobs.move_path(from, to, replace, &server.keep())
        });
        connect.set_delete_move(delete, move_);
        if self.opts.platform != Platform::Ps5 {
            return;
        }
        let weak = Arc::downgrade(self);
        connect.set_install(Arc::new(move |path| {
            let server = weak.upgrade().ok_or("Dump Forge is stopping")?;
            let request = serde_json::from_value(json!({
                "source": "", "format": "folder", "output": "", "install": path,
            }))
            .map_err(|e| e.to_string())?;
            server.start(request).map_err(|r| r.error_text())
        }));
    }

    fn keep(&self) -> Vec<PathBuf> {
        keep_list(&self.roots, &self.opts.roots)
    }

    fn delete_path(&self, body: &[u8]) -> Reply {
        let arg: PathArg = parse(body)?;
        let _deleting = self.admit_delete()?;
        let keep = self.keep();
        self.jobs.delete_path(&arg.path, &keep).map_err(deleted)?;
        Ok(Response::json(&()))
    }

    fn move_path(&self, body: &[u8]) -> Reply {
        let arg: MoveArg = parse(body)?;
        let _deleting = self.admit_delete()?;
        let keep = self.keep();
        self.jobs
            .move_path(&arg.from, &arg.to, arg.replace, &keep)
            .map_err(deleted)?;
        Ok(Response::json(&()))
    }

    fn admit_delete(&self) -> Result<Deleting<'_>, Response> {
        let _admission = lock(&self.admission);
        if self.stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        self.deleting.fetch_add(1, Ordering::SeqCst);
        Ok(Deleting(&self.deleting))
    }

    /// Call under admission, so none starts meanwhile.
    fn idle_of_deletes(&self) -> Result<(), Response> {
        if self.deleting.load(Ordering::SeqCst) > 0 {
            return Err(Response::error(
                409,
                "Dump Forge is busy: a game is being deleted",
            ));
        }
        Ok(())
    }

    /// `?source=&path=`: the body staged at `<staging>/<path>`, replacing an earlier one only
    /// once the whole body is in; `{"path"}`, an edit's `Add.from`.
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
        // Canonical, so two spellings of one source share one staging tree.
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
        let _uploading = {
            let mut uploads = lock(&self.uploads);
            if !uploads.insert(staged.clone()) {
                let text = format!("{path}: another upload of it is in flight");
                return Err(Response::error(409, text));
            }
            Uploading(&self.uploads, staged.clone())
        };
        // Not a file a job uses, nor one inside a folder an unfinished edit adds (core matches
        // a path not there yet only by spelling). Under admission: a started job is in both.
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

    fn edit_discard(&self, body: &[u8]) -> Reply {
        let arg: SourceArg = parse(body)?;
        staging_dir(&arg.source)?;
        // As `edit_upload` names it; a gone source by its canonical folder.
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
        let keep = self.keep();
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

    /// Core's `stale_parts` match, keeping only the first 1,000 by path while listing.
    fn stale_parts(&self, body: &[u8]) -> Reply {
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
        // After the listing and under admission: a job whose `.part` was listed is in the table.
        let (running, adds) = {
            let _admission = lock(&self.admission);
            let table = lock(&self.table);
            (table.unfinished(), table.adds())
        };
        let theirs = table::running_parts(&running);
        // A staging folder is not stale while an edit adds from it or an upload is in flight.
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

    /// `null` lists the roots; on the PS5 only a folder at or inside a root (else 400).
    fn list_dir(&self, body: &[u8]) -> Reply {
        let arg = parse::<ListArg>(body)?;
        let roots = self.roots.current();
        let Some(path) = arg.path else {
            let entries: Vec<ListEntry> = roots
                .iter()
                .filter_map(|r| r.to_str())
                .map(|r| ListEntry {
                    name: r.into(),
                    path: r.into(),
                    dir: true,
                    size: None,
                    link: false,
                    mounted: false,
                })
                .collect();
            return Ok(Response::json(&json!({
                "path": null, "parent": null, "entries": entries, "truncated": false,
            })));
        };
        let failed = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::InvalidInput => Response::error(400, format!("{path}: {e}")),
            _ => Response::error(500, format!("{path}: {e}")),
        };
        (self.roots.confine(Path::new(&path))).map_err(|e| Response::error(400, e))?;
        let list = match arg.explorer {
            true => ps5_dump_forge_core::list_explorer,
            false => ps5_dump_forge_core::list_dir,
        };
        let mut listing = list(Path::new(&path)).map_err(failed)?;
        // A link's entry carries its target's path: one outside the roots is left out.
        (listing.entries).retain(|e| !e.link || self.roots.confine(&e.path).is_ok());
        if roots.contains(&listing.path) {
            listing.parent = None;
        }
        Ok(Response::json(&listing))
    }

    /// Once `{}` is sent, cancels every job, waits for its cleanup and exits.
    fn quit(self: &Arc<Self>) -> Response {
        let mut response = Response::json(&json!({}));
        if self.stop() {
            let server = self.clone();
            response.after = Some(Box::new(move || server.finish()));
        }
        response
    }

    /// Admission within `IDLE_WAIT`, else a 409: a late yes would act after the asker gave up.
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
                        "Dump Forge is busy: a game is being deleted",
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
            self.idle_of_deletes()?;
            if self.running.load(Ordering::SeqCst) > 0 {
                return Err(Response::error(
                    409,
                    "Dump Forge is busy: a conversion is queued or running",
                ));
            }
            if self.connect.as_ref().is_ok_and(|c| c.peers_active()) {
                return Err(Response::error(
                    409,
                    "Dump Forge is busy: a paired device is reading or writing files here",
                ));
            }
            self.stopping.store(true, Ordering::SeqCst);
        }
        let mut response = Response::json(&json!({}));
        let server = self.clone();
        response.after = Some(Box::new(move || server.finish()));
        Ok(response)
    }

    // ponytail: connect and close; elfldr writes "Unknown payload format" to its own log, nothing on screen.
    fn elfldr(&self) -> Reply {
        if self.opts.platform != Platform::Ps5 {
            return Err(Response::error(404, "unknown route"));
        }
        let up = TcpStream::connect_timeout(&self.opts.elfldr, Duration::from_secs(1)).is_ok();
        Ok(Response::json(&json!({ "up": up })))
    }

    /// Downloads the release ELF here (elfldr can't follow GitHub's redirect) and streams it to
    /// elfldr; this server keeps serving until the new one takes over.
    fn self_update(&self, body: &[u8]) -> Reply {
        if self.opts.platform != Platform::Ps5 {
            return Err(Response::error(404, "unknown route"));
        }
        let version = parse::<VersionArg>(body)?.version;
        let current = env!("CARGO_PKG_VERSION");
        if !ps5_dump_forge_updater::plain_version(&version)
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
            self.idle_of_deletes()?;
            if self.running.load(Ordering::SeqCst) > 0 {
                return Err(Response::error(
                    409,
                    "a conversion is queued or running; update once it has finished",
                ));
            }
        }
        #[cfg(target_env = "ps5")]
        let spill = Some(std::path::Path::new(crate::self_copy::TARGET));
        #[cfg(not(target_env = "ps5"))]
        let spill = None;
        ps5_dump_forge_updater::install(self.opts.elfldr, &version, spill).map_err(|e| match e
            .kind()
        {
            std::io::ErrorKind::NotConnected => Response::error(
                503,
                format!(
                    "elfldr isn't running on port {}: load it, then try again",
                    self.opts.elfldr.port()
                ),
            ),
            _ => Response::error(502, e.to_string()),
        })?;
        Ok(Response::json(&json!({})))
    }

    /// `true` for the call that closed admission; every admitted job is in the table by then.
    pub fn stop(&self) -> bool {
        let _admission = lock(&self.admission);
        !self.stopping.swap(true, Ordering::SeqCst)
    }

    pub fn finish(&self) {
        // Cut short, a folder would be left half gone.
        // ponytail: no bound; a peer's delete ends at FLINK's silence limit.
        while self.deleting.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        self.jobs.cancel_all_and_wait();
        if let Ok(connect) = &self.connect {
            connect.shutdown();
        }
        #[cfg(target_env = "ps5")]
        crate::self_copy::wait();
        (self.opts.notify)(crate::notice::stopped(crate::notice::console_lang()));
        (self.opts.exit)(0);
    }
}

/// `/api/connect/*` bodies: each route reads the fields it needs.
#[derive(Deserialize)]
struct ConnectArg {
    #[serde(default)]
    address: String,
    #[serde(default)]
    key_id: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    allow: bool,
    #[serde(default)]
    path: PathBuf,
    #[serde(default)]
    run: String,
}

#[derive(Deserialize)]
struct RemoteJobArg {
    key_id: String,
    job: JobId,
}

#[derive(Deserialize)]
struct NasId {
    id: String,
}

fn plain_name(name: &str) -> bool {
    !(name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains(':')))
}

impl Server {
    fn connect(&self) -> Result<&Arc<crate::connect::Connect>, Response> {
        self.connect.as_ref().map_err(|e| Response::error(503, e))
    }

    /// A failed call is 400 with `connect::error_text` (a refusal's code, else the text).
    fn connect_route(&self, route: &str, body: &[u8]) -> Reply {
        let c = self.connect()?;
        let failed = |e: std::io::Error| Response::error(400, crate::connect::error_text(&e));
        match route {
            "nas_save" => {
                let req: crate::nas::NasSave = parse(body)?;
                return Ok(Response::json(
                    &json!({ "id": c.nas_save(&req).map_err(failed)? }),
                ));
            }
            "nas_forget" => {
                let a: NasId = parse(body)?;
                return Ok(Response::json(&c.nas_forget(&a.id).map_err(failed)?));
            }
            "cancel_job" => {
                let a: RemoteJobArg = parse(body)?;
                return Ok(Response::json(&c.cancel_remote_job(&a.key_id, a.job)));
            }
            _ => {}
        }
        let a: ConnectArg = parse(body)?;
        match route {
            "pair" => {
                c.pair(&a.address).map_err(failed)?;
                Ok(Response::json(&()))
            }
            "probe" => Ok(Response::json(&c.probe(&a.address).map_err(failed)?)),
            "pair_cancel" => {
                c.pair_cancel();
                Ok(Response::json(&()))
            }
            "approve" => Ok(Response::json(&c.approve(&a.id, a.allow))),
            "forget" => Ok(Response::json(&c.forget(&a.key_id).map_err(failed)?)),
            "install" => {
                let job = c.install(&a.key_id, &a.path);
                Ok(Response::json(
                    &json!({ "job": job.map_err(|e| Response::error(400, e))? }),
                ))
            }
            "speed_test" => {
                self.refuse_if_stopping()?;
                let got = c
                    .speed_test(&a.key_id, &a.run)
                    .map_err(|e| Response::error(400, e))?;
                Ok(Response::json(&got))
            }
            "speed_test_cancel" => {
                c.speed_test_cancel(&a.key_id, &a.run);
                Ok(Response::json(&()))
            }
            "discover_start" => {
                c.discover_start();
                Ok(Response::json(&()))
            }
            "discover_stop" => {
                c.discover_stop();
                Ok(Response::json(&()))
            }
            _ => Err(Response::error(404, "unknown route")),
        }
    }

    /// `?dir=&name=`: the body as `<dir>/<name>` through a `.part` and a no-replace rename
    /// (with `replace=1` over a file: [`Self::place_replacing`]); `id=` sends it in chunks.
    fn connect_upload(&self, query: &str, length: Option<u64>, body: &mut dyn Read) -> Reply {
        let Some(length) = length else {
            return Err(Response::error(411, "bad request: needs Content-Length"));
        };
        let arg = |key: &str| {
            http::query_value(query, key)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    Response::error(400, format!("bad request: needs {key}=, percent-encoded"))
                })
        };
        let (dir, name) = (PathBuf::from(arg("dir")?), arg("name")?);
        if !plain_name(&name) {
            return Err(Response::error(
                400,
                format!("{name:?} is not a plain file name"),
            ));
        }
        self.refuse_if_stopping()?;
        if self.opts.platform == Platform::Ps5
            && let Some(why) = ps5_dump_forge_core::out::ps5_refusal(&dir)
        {
            return Err(Response::error(400, why));
        }
        if !std::fs::metadata(&dir).is_ok_and(|m| m.is_dir()) {
            let text = format!("{}: not a folder", dir.display());
            return Err(Response::error(404, text));
        }
        let target = dir.join(&name);
        let replace = http::query_value(query, "replace").as_deref() == Some("1");
        if replace {
            self.replaceable(&target)?;
        }
        if let Some(id) = http::query_value(query, "id") {
            return self.connect_upload_chunk(&id, query, target, replace, length, body);
        }
        let exists = |t: &Path| format!("{}: already exists", t.display());
        if !replace && std::fs::symlink_metadata(&target).is_ok() {
            return Err(Response::error(409, exists(&target)));
        }
        let seq = UPLOAD_SEQ.fetch_add(1, Ordering::Relaxed);
        let part = dir.join(format!(".forge-connect-{}-{seq}.part", std::process::id()));
        let failed = |e: std::io::Error| core(format!("{}: {e}", target.display()));
        let mut file = (std::fs::File::options().write(true).create_new(true))
            .open(&part)
            .map_err(failed)?;
        let copied = (|| {
            open_up(&file)?;
            copy_body(body, &mut file, length)?;
            file.sync_all()?;
            drop(file);
            if replace {
                return Ok(());
            }
            ps5_dump_forge_core::rename_no_replace(&part, &target)
        })();
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&part);
            return Err(match e.kind() {
                std::io::ErrorKind::UnexpectedEof => Response::error(400, e),
                std::io::ErrorKind::AlreadyExists => Response::error(409, exists(&target)),
                _ => failed(e),
            });
        }
        if replace {
            self.place_replacing(&part, &target)?;
        }
        Ok(Response::json(&json!({ "path": target })))
    }

    /// Refused before the body is read: only a file goes over a file (400), not one a job uses
    /// (409). Core checks again at the rename.
    fn replaceable(&self, target: &Path) -> Result<(), Response> {
        let Ok(meta) = std::fs::symlink_metadata(target) else {
            return Ok(());
        };
        let (kind, shown) = (meta.file_type(), target.display());
        let why = if kind.is_symlink() {
            format!("{shown} is a link: nothing is put in its place")
        } else if kind.is_dir() {
            format!("{shown} is a folder: a file is not put in its place")
        } else if !kind.is_file() {
            format!("{shown} is not a file or folder: nothing is put in its place")
        } else if self.jobs.uses(target) {
            let why =
                format!("{shown} is in use by an unfinished job; wait for it or stop it first");
            return Err(Response::error(409, why));
        } else {
            return Ok(());
        };
        Err(Response::error(400, why))
    }

    /// Core's `move_path` with replace: refused as Delete refuses; under admission, so no job
    /// or quit lands between the check and the rename. The `.part` goes when it fails.
    fn place_replacing(&self, part: &Path, target: &Path) -> Result<(), Response> {
        let placed = {
            let _admission = lock(&self.admission);
            self.jobs.move_path(part, target, true, &self.keep())
        };
        placed.map_err(|e| {
            let _ = std::fs::remove_file(part);
            deleted(e)
        })
    }

    /// One chunk at `offset` of `size`; a chunk that fails drops the whole upload.
    fn connect_upload_chunk(
        &self,
        id: &str,
        query: &str,
        target: PathBuf,
        replace: bool,
        length: u64,
        body: &mut dyn Read,
    ) -> Reply {
        let bad = |what: String| Response::error(400, format!("bad request: {what}"));
        if id.is_empty() || id.len() > 32 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(bad("id= is 1-32 letters or digits".into()));
        }
        let drop_upload = || end_chunked(&mut lock(&CHUNKED), id);
        if http::query_value(query, "cancel").as_deref() == Some("1") {
            drop_upload();
            return Ok(Response::json(&json!({ "cancelled": true })));
        }
        let num = |key: &str| {
            http::query_value(query, key)
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| bad(format!("needs {key}= as a number")))
        };
        let (offset, size) = (num("offset")?, num("size")?);
        if offset.checked_add(length).is_none_or(|end| end > size) {
            return Err(bad(format!(
                "{length} bytes at {offset} pass the size {size}"
            )));
        }
        // Overlapping or gapped chunks could reach `size` with a hole left in the file.
        if !offset.is_multiple_of(UPLOAD_CHUNK) || length != UPLOAD_CHUNK.min(size - offset) {
            return Err(bad(format!(
                "a chunk is {UPLOAD_CHUNK} bytes at a multiple of it (the last one shorter)"
            )));
        }
        let exists = |t: &Path| format!("{}: already exists", t.display());
        let failed = |e: std::io::Error| core(format!("{}: {e}", target.display()));
        let part = {
            let mut all = lock(&CHUNKED);
            let idle: Vec<String> = (all.iter())
                .filter(|(_, c)| c.touched.elapsed() > CHUNK_IDLE)
                .map(|(k, _)| k.clone())
                .collect();
            for k in idle {
                end_chunked(&mut all, &k);
            }
            match all.get_mut(id) {
                Some(c) if c.target == target && c.size == size => {
                    c.touched = Instant::now();
                    c.part.clone()
                }
                Some(_) => {
                    return Err(Response::error(409, format!("upload {id} is another file")));
                }
                None => {
                    if lock(&CHUNK_ENDED).iter().any(|e| e == id) {
                        return Err(Response::error(409, format!("upload {id} has ended")));
                    }
                    if !replace && std::fs::symlink_metadata(&target).is_ok() {
                        return Err(Response::error(409, exists(&target)));
                    }
                    let dir = target.parent().unwrap_or(Path::new("."));
                    let part = dir.join(format!(".forge-connect-{}-{id}.part", std::process::id()));
                    let file = (std::fs::File::options().write(true).create_new(true))
                        .open(&part)
                        .map_err(failed)?;
                    open_up(&file).map_err(|e| {
                        let _ = std::fs::remove_file(&part);
                        failed(e)
                    })?;
                    all.insert(
                        id.to_string(),
                        Chunked {
                            part: part.clone(),
                            target: target.clone(),
                            size,
                            replace,
                            offsets: HashSet::new(),
                            got: 0,
                            touched: Instant::now(),
                        },
                    );
                    part
                }
            }
        };
        let written = (|| {
            let mut file = std::fs::File::options().write(true).open(&part)?;
            file.seek(std::io::SeekFrom::Start(offset))?;
            copy_body(body, &mut file, length)
        })();
        if let Err(e) = written {
            drop_upload();
            return Err(match e.kind() {
                std::io::ErrorKind::UnexpectedEof => Response::error(400, e),
                _ => failed(e),
            });
        }
        let complete = {
            let mut all = lock(&CHUNKED);
            let Some(c) = all.get_mut(id) else {
                return Err(Response::error(409, format!("upload {id} was cancelled")));
            };
            if c.offsets.insert(offset) {
                c.got += length;
            }
            if c.got >= c.size {
                all.remove(id)
            } else {
                None
            }
        };
        let Some(c) = complete else {
            return Ok(Response::json(&json!({ "done": false })));
        };
        let finished = (|| {
            // Writable: Windows refuses to flush a read-only handle ("Access is denied").
            let file = std::fs::File::options().write(true).open(&c.part)?;
            let got = file.metadata()?.len();
            if got != c.size {
                return Err(std::io::Error::other(format!(
                    "{got} bytes, not {}",
                    c.size
                )));
            }
            file.sync_all()?;
            drop(file);
            if c.replace {
                return Ok(());
            }
            ps5_dump_forge_core::rename_no_replace(&c.part, &c.target)
        })();
        if let Err(e) = finished {
            let _ = std::fs::remove_file(&c.part);
            return Err(match e.kind() {
                std::io::ErrorKind::AlreadyExists => Response::error(409, exists(&c.target)),
                _ => failed(e),
            });
        }
        if c.replace {
            self.place_replacing(&c.part, &c.target)?;
        }
        Ok(Response::json(&json!({ "path": c.target, "done": true })))
    }

    /// A file that shrinks meanwhile ends short.
    fn connect_download(&self, query: &str) -> Reply {
        let Some(path) = http::query_value(query, "path").filter(|v| !v.is_empty()) else {
            return Err(Response::error(
                400,
                "bad request: needs path=, percent-encoded",
            ));
        };
        let path = PathBuf::from(path);
        let not_a_file = || Response::error(400, format!("{}: not a file", path.display()));
        // Before the open: a FIFO would block it, and Windows can't open a folder.
        if std::fs::metadata(&path).is_ok_and(|m| !m.is_file()) {
            return Err(not_a_file());
        }
        let file = std::fs::File::open(&path)
            .map_err(|e| Response::error(404, format!("{}: {e}", path.display())))?;
        let meta = file.metadata().map_err(core)?;
        if !meta.is_file() {
            return Err(not_a_file());
        }
        let name = path
            .file_name()
            .map_or("download".into(), |n| n.to_string_lossy());
        let len = meta.len();
        let body = Box::new(move |out: &mut dyn Write| {
            // U8
            let _awake = ps5_dump_forge_core::awake();
            std::io::copy(&mut file.take(len), out).map(|_| ())
        });
        Ok(Response::download(
            "application/octet-stream",
            &name,
            len,
            body,
        ))
    }
}

/// A 2-module quiet zone (the page pads it further), one path, scaled by CSS.
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

    /// Until a `start_job` on another thread holds admission (a fixed sleep raced on CI).
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

    /// The PS5: listings and `host_tree` only at or inside a root, else 400; a computer unchanged.
    #[cfg(unix)]
    #[test]
    fn a_ps5_lists_only_its_roots() {
        // Not the temp dir: on macOS it is a refused system folder.
        let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("forge-api-reach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("user/data/games")).unwrap();
        std::fs::create_dir_all(tmp.join("system")).unwrap();
        let tmp = tmp.canonicalize().unwrap();
        let (data, real) = (tmp.join("data"), tmp.join("user/data"));
        std::os::unix::fs::symlink(&real, &data).unwrap();
        std::os::unix::fs::symlink(tmp.join("system"), real.join("out")).unwrap();
        std::os::unix::fs::symlink(real.join("games"), real.join("in")).unwrap();
        let opts = Options::new(0, vec![data.clone()], Platform::Ps5, |_: &str| {});
        let server = Arc::new(Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap());
        let list = |path: &Path, explorer: bool| {
            post(
                &server,
                "list_dir",
                json!({ "path": path, "explorer": explorer }),
            )
        };
        for explorer in [false, true] {
            let r = post(
                &server,
                "list_dir",
                json!({ "path": null, "explorer": explorer }),
            );
            let v: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
            assert_eq!(v["entries"][0]["path"], json!(real), "{v}");
            assert_eq!(v["entries"].as_array().unwrap().len(), 1, "{v}");
            let r = list(&data, explorer);
            assert_eq!(r.status, 200, "{}", r.text());
            let v: serde_json::Value = serde_json::from_str(&r.text()).unwrap();
            assert_eq!((&v["path"], &v["parent"]), (&json!(real), &json!(null)));
            let names: Vec<&str> = (v["entries"].as_array().unwrap().iter())
                .map(|e| e["name"].as_str().unwrap())
                .collect();
            assert!(
                names.contains(&"in") && !names.contains(&"out"),
                "{names:?}"
            );
            assert_eq!(list(&data.join("games"), explorer).status, 200);
            for p in [tmp.clone(), tmp.join("system"), data.join("../system")] {
                let r = list(&p, explorer);
                assert_eq!(r.status, 400, "{}: {}", p.display(), r.text());
                let line = "is not on this console's storage or a plugged-in drive";
                assert!(r.text().contains(line), "{}", r.text());
            }
        }
        let outside = json!({ "path": tmp.join("system") });
        assert_eq!(post(&server, "host_tree", outside).status, 400);
        let r = post(&server, "host_tree", json!({ "path": data.join("games") }));
        assert_eq!(r.status, 200, "{}", r.text());
        assert_ne!(list(Path::new("flink://00ff"), false).status, 400);

        let server = library_server(&real);
        let r = post(&server, "list_dir", json!({ "path": tmp.join("system") }));
        assert_eq!(r.status, 200, "{}", r.text());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

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
        // Admission held (a delete): busy after a short wait, never stopping later.
        let held = lock(&server.admission);
        let r = post(&server, "quit", idle.clone());
        assert_eq!(r.status, 409);
        assert!(r.text().contains("being deleted"), "{}", r.text());
        drop(held);
        assert!(!server.stopping.load(Ordering::SeqCst));
        let r = post(&server, "quit", idle.clone());
        assert_eq!((r.status, r.text()), (200, "{}".to_string()));
        assert!(r.after.is_some() && server.stopping.load(Ordering::SeqCst));
        let r = post(&server, "quit", idle);
        assert_eq!(r.status, 409);
        assert!(r.text().contains("stopping"), "{}", r.text());
        let r = post(&server, "quit", json!({}));
        assert_eq!((r.status, r.after.is_none()), (200, true));
    }

    #[test]
    fn quit_if_idle_waits_for_a_peer_writing_here() {
        let tmp = std::env::temp_dir().join(format!("forge-api-peer-quit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let share = tmp.join("share");
        std::fs::create_dir_all(&share).unwrap();
        let mut s = server();
        let here =
            crate::connect::Connect::start_rooted(&tmp.join("a"), 0, Some(share.clone())).unwrap();
        let (port, key) = (here.state().device.port, here.me().key_id.clone());
        s.connect = Ok(Arc::new(here));
        let server = Arc::new(s);
        let here = server.connect.as_ref().unwrap();
        let peer = crate::connect::Connect::start_rooted(&tmp.join("b"), 0, None).unwrap();
        let addr = format!("127.0.0.1:{port}");
        let o = crate::connect::pair_for_test(&peer, here, &addr, true);
        assert_eq!(o.state, "paired", "{o:?}");
        let (mut rpc, _hs) = peer.session(&key).unwrap();
        let h = rpc.call("create", json!({ "path": "f.part" })).unwrap()["handle"]
            .as_u64()
            .unwrap();

        let idle = json!({ "if_idle": true });
        let r = post(&server, "quit", idle.clone());
        assert_eq!(r.status, 409);
        assert!(
            r.text()
                .contains("a paired device is reading or writing files here"),
            "{}",
            r.text()
        );
        assert!(!server.stopping.load(Ordering::SeqCst));
        rpc.call("close", json!({ "handle": h })).unwrap();
        rpc.call("rename", json!({ "from": "f.part", "to": "f" }))
            .unwrap();
        let r = post(&server, "quit", idle);
        assert_eq!((r.status, r.text()), (200, "{}".to_string()));

        drop(rpc);
        peer.shutdown();
        here.shutdown();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Every refusal comes before elfldr is asked.
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
        // A delete holding admission past 1 s: refused before elfldr is asked.
        {
            let _held = server.admission.lock().unwrap();
            let r = post(&server, "self_update", newer.clone());
            assert_eq!(r.status, 409);
            assert!(r.text().contains("being deleted"), "{}", r.text());
        }
        // A delete running without admission (counted): refused too, at once.
        server.deleting.store(1, Ordering::SeqCst);
        let r = post(&server, "self_update", newer.clone());
        assert_eq!(r.status, 409);
        assert!(r.text().contains("being deleted"), "{}", r.text());
        let r = post(&server, "quit", json!({ "if_idle": true }));
        assert_eq!(r.status, 409);
        assert!(r.text().contains("being deleted"), "{}", r.text());
        server.deleting.store(0, Ordering::SeqCst);
        server.updating.store(true, Ordering::SeqCst);
        let r = post(&server, "self_update", newer.clone());
        assert_eq!(r.status, 409);
        assert!(r.text().contains("already in progress"), "{}", r.text());
        assert!(server.updating.load(Ordering::SeqCst));
        server.updating.store(false, Ordering::SeqCst);
        assert!(server.stop());
        let r = post(&server, "self_update", newer);
        assert_eq!(r.status, 409);
        assert!(r.text().contains("stopping"), "{}", r.text());
        assert!(!server.updating.load(Ordering::SeqCst));
    }

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

    /// Over the socket past `BODY_MAX`, replacing, refusals, stale `.part`, discard twice.
    #[test]
    fn edit_upload_and_discard() {
        let root = std::env::temp_dir().join(format!("forge-api-upload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
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

    /// A job whose `Done` came before `start` returned.
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

    /// Once stopping, no job, delete or move is admitted, nothing touched.
    #[test]
    fn quit_closes_admission() {
        let root = dir("quit-closes");
        std::fs::write(root.join("a.bin"), b"a").unwrap();
        let server = library_server(&root);
        let r = post(&server, "quit", json!({}));
        assert_eq!(r.status, 200, "{}", r.text());
        assert!(r.after.is_some() && server.stopping.load(Ordering::SeqCst));
        let start = json!({ "request": {
            "source": root.join("a.bin"), "format": "exfat", "output": root.join("out.exfat"),
        }});
        let delete = json!({ "path": root.join("a.bin") });
        let move_ = json!({ "from": root.join("a.bin"), "to": root.join("b.bin") });
        for (route, body) in [
            ("start_job", start),
            ("delete_path", delete),
            ("move_path", move_),
        ] {
            let r = post(&server, route, body);
            assert_eq!(r.status, 409, "{route}: {}", r.text());
            assert!(
                r.text().contains("Dump Forge is stopping"),
                "{route}: {}",
                r.text()
            );
        }
        assert_eq!(std::fs::read(root.join("a.bin")).unwrap(), b"a");
        assert!(!root.join("b.bin").exists() && !root.join("out.exfat").exists());
        assert!(lock(&server.table).snapshot().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A phone's upload past `BODY_MAX` and its download back, refusals.
    #[test]
    fn phone_uploads_refuse_mounted_games_before_reading_the_body() {
        struct Unread;
        impl Read for Unread {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("a refused upload must not read its body");
            }
        }
        let opts = Options::new(0, vec![], Platform::Ps5, |_: &str| {});
        let server = Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap();
        for dir in [
            "/mnt/shadowmnt",
            "/mnt/shadowmnt/Game_01234567/data",
            "/mnt/shadowmnt/pfsc/Game_01234567",
        ] {
            for extra in ["", "&id=mountrefusal&offset=0&size=1"] {
                let query = format!("dir={}&name=file.bin{extra}", percent_encode(dir));
                let err = server
                    .connect_upload(&query, Some(1), &mut Unread)
                    .err()
                    .unwrap();
                assert_eq!(err.status, 400);
                assert!(err.text().contains("mounted game"), "{}", err.text());
            }
        }
        assert!(!lock(&CHUNKED).contains_key("mountrefusal"));
    }

    #[test]
    fn connect_upload_and_download() {
        let root = std::env::temp_dir().join(format!("forge-api-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        let mut opts = Options::new(0, vec![root.clone()], Platform::Host, |_: &str| {});
        (opts.connect_dir, opts.connect_port) = (Some(root.join("data")), 0);
        let server = Arc::new(Server::new(opts, "127.0.0.1:1".parse().unwrap()).unwrap());
        let get = |path: &str| {
            over_tcp(
                &server,
                format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes(),
            )
        };
        let state = get("/api/connect/state");
        assert!(state.starts_with("HTTP/1.1 200"), "{state}");
        // Pairing is approved on the device: no code in the state.
        assert!(
            state.contains("\"pending\"") && !state.contains("\"code\""),
            "{state}"
        );
        assert!(state.contains("\"remote_jobs\":[]"), "{state}");
        let post = |route: &str, body: &str| {
            over_tcp(
                &server,
                format!(
                    "POST /api/connect/{route} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
        };
        let r = post("cancel_job", r#"{"key_id":"ab","job":3}"#);
        assert!(r.starts_with("HTTP/1.1 200") && r.ends_with("false"), "{r}");
        let r = post("cancel_job", r#"{"key_id":"ab"}"#);
        assert!(r.starts_with("HTTP/1.1 400"), "{r}");
        assert!(get("/api/session").contains("/#connect"));

        let big: Vec<u8> = (0..http::BODY_MAX as u32 * 2 + 7)
            .map(|i| i as u8)
            .collect();
        let dir = percent_encode(&root.to_string_lossy());
        let put_q = |name: &str, extra: &str, body: &[u8]| {
            let mut req = format!(
                "POST /api/connect/upload?dir={dir}&name={}{extra} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n",
                percent_encode(name),
                body.len()
            )
            .into_bytes();
            req.extend_from_slice(body);
            over_tcp(&server, &req)
        };
        let put = |name: &str, body: &[u8]| put_q(name, "", body);
        let reply = put("a b.bin", &big);
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert_eq!(std::fs::read(root.join("a b.bin")).unwrap(), big);
        assert!(put("a b.bin", b"again").starts_with("HTTP/1.1 409"));
        assert!(put("../x", b"x").starts_with("HTTP/1.1 400"));

        // In 8 MiB chunks, out of order (as parallel requests land): done on the last byte.
        let c = UPLOAD_CHUNK as usize;
        let file: Vec<u8> = (0..2 * c + 5).map(|i| (i % 251) as u8).collect();
        let n = file.len();
        let cuts = [(c, 2 * c), (0, c), (2 * c, n)];
        for (k, &(from, to)) in cuts.iter().enumerate() {
            let q = format!("&id=up1&offset={from}&size={n}");
            let reply = put_q("chunked.bin", &q, &file[from..to]);
            assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
            assert_eq!(reply.contains("\"done\":true"), k == 2, "{reply}");
        }
        assert_eq!(std::fs::read(root.join("chunked.bin")).unwrap(), file);
        // Past the size, or not a whole chunk at its place: refused.
        let past = put_q("x.bin", "&id=up2&offset=4&size=5", b"ab");
        assert!(past.starts_with("HTTP/1.1 400"), "{past}");
        let odd = put_q("x.bin", "&id=up2&offset=0&size=9", b"abc");
        assert!(odd.starts_with("HTTP/1.1 400"), "{odd}");
        // A cancelled upload leaves nothing, and a chunk still in flight can't start it again.
        let last = format!("&id=up3&offset={c}&size={}", c + 1);
        assert!(put_q("y.bin", &last, b"z").contains("\"done\":false"));
        assert!(put_q("y.bin", "&id=up3&cancel=1", b"").contains("cancelled"));
        let late = put_q("y.bin", &last, b"z");
        assert!(late.starts_with("HTTP/1.1 409"), "{late}");
        assert!(!root.join("y.bin").exists());
        assert!(
            !std::fs::read_dir(&root)
                .unwrap()
                .any(|e| { e.unwrap().file_name().to_string_lossy().ends_with(".part") })
        );

        let path = percent_encode(&root.join("a b.bin").to_string_lossy());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let s = server.clone();
        let t = std::thread::spawn(move || s.handle(listener.accept().unwrap().0));
        let mut client = TcpStream::connect(addr).unwrap();
        let req = format!("GET /api/connect/download?path={path} HTTP/1.1\r\nHost: x\r\n\r\n");
        client.write_all(req.as_bytes()).unwrap();
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).unwrap();
        t.join().unwrap();
        let split = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&reply[..split]);
        assert!(
            head.starts_with("HTTP/1.1 200") && head.contains("attachment"),
            "{head}"
        );
        assert!(
            head.contains(&format!("Content-Length: {}", big.len())),
            "{head}"
        );
        assert_eq!(reply[split + 4..], big[..]);
        assert!(get(&format!("/api/connect/download?path={dir}")).starts_with("HTTP/1.1 400"));
        server.connect.as_ref().unwrap().shutdown();
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Without `replace` nothing changes; with it a file goes over a file, a folder merges.
    #[test]
    fn move_path_replaces_only_when_asked() {
        let root = dir("move-replace");
        std::fs::create_dir_all(root.join("a/d")).unwrap();
        std::fs::create_dir_all(root.join("b/d")).unwrap();
        std::fs::write(root.join("a/x.bin"), b"new").unwrap();
        std::fs::write(root.join("b/x.bin"), b"old").unwrap();
        std::fs::write(root.join("a/d/y.bin"), b"y").unwrap();
        std::fs::write(root.join("b/d/z.bin"), b"z").unwrap();
        let server = library_server(&root);
        let arg = |from: &str, to: &str, replace: Option<bool>| {
            let mut v = json!({ "from": root.join(from), "to": root.join(to) });
            if let Some(r) = replace {
                v["replace"] = json!(r);
            }
            v
        };
        for replace in [None, Some(false)] {
            let r = post(&server, "move_path", arg("a/x.bin", "b/x.bin", replace));
            assert_eq!(r.status, 400, "{}", r.text());
            assert!(r.text().contains("nothing is replaced"), "{}", r.text());
        }
        assert_eq!(std::fs::read(root.join("b/x.bin")).unwrap(), b"old");
        let r = post(&server, "move_path", arg("a/x.bin", "b/x.bin", Some(true)));
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(std::fs::read(root.join("b/x.bin")).unwrap(), b"new");
        assert!(!root.join("a/x.bin").exists());
        // A folder onto a folder merges; a file onto a folder is refused.
        let r = post(&server, "move_path", arg("a/d", "b/d", Some(true)));
        assert_eq!(r.status, 200, "{}", r.text());
        assert!(root.join("b/d/y.bin").is_file() && root.join("b/d/z.bin").is_file());
        assert!(!root.join("a/d").exists());
        std::fs::write(root.join("a/d"), b"f").unwrap();
        let r = post(&server, "move_path", arg("a/d", "b/d", Some(true)));
        assert_eq!(r.status, 400, "{}", r.text());
        assert!(r.text().contains("is a folder"), "{}", r.text());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Single and chunked: a file replaced, a missing one made, a folder or link refused.
    #[test]
    fn phone_uploads_replace_a_file_only_when_asked() {
        let root = dir("upload-replace");
        std::fs::create_dir_all(root.join("d/folder")).unwrap();
        std::fs::write(root.join("d/x.bin"), b"old").unwrap();
        let server = library_server(&root);
        let send = |name: &str, extra: &str, body: &[u8]| {
            let dir = percent_encode(&root.join("d").to_string_lossy());
            let query = format!("dir={dir}&name={}{extra}", percent_encode(name));
            (server.connect_upload(&query, Some(body.len() as u64), &mut &body[..]))
                .unwrap_or_else(|r| r)
        };
        let read = |name: &str| std::fs::read(root.join("d").join(name)).unwrap();
        // Without replace: 409, nothing changed (single and chunked).
        assert_eq!(send("x.bin", "", b"new").status, 409);
        assert_eq!(
            send("x.bin", "&id=rep0&offset=0&size=3", b"new").status,
            409
        );
        assert_eq!(read("x.bin"), b"old");
        let r = send("x.bin", "&replace=1", b"new");
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(read("x.bin"), b"new");
        let r = send("fresh.bin", "&replace=1", b"f");
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(read("fresh.bin"), b"f");
        // Chunked: two whole chunks, then the short last one, the first asking to replace.
        let c = UPLOAD_CHUNK as usize;
        let file: Vec<u8> = (0..2 * c + 3).map(|i| (i % 249) as u8).collect();
        let n = file.len();
        for (from, to) in [(0, c), (2 * c, n), (c, 2 * c)] {
            let extra = format!("&id=rep1&offset={from}&size={n}&replace=1");
            let r = send("x.bin", &extra, &file[from..to]);
            assert_eq!(r.status, 200, "{}", r.text());
        }
        assert_eq!(read("x.bin"), file);
        // Refused onto a folder (400, its contents untouched) and onto a link.
        std::fs::write(root.join("d/folder/in.bin"), b"in").unwrap();
        for extra in ["&replace=1", "&replace=1&id=rep2&offset=0&size=1"] {
            let r = send("folder", extra, b"z");
            assert_eq!(r.status, 400, "{}", r.text());
            assert!(r.text().contains("is a folder"), "{}", r.text());
        }
        assert_eq!(read("folder/in.bin"), b"in");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("d/fresh.bin"), root.join("d/link")).unwrap();
            let r = send("link", "&replace=1", b"z");
            assert_eq!(r.status, 400, "{}", r.text());
            assert!(r.text().contains("is a link"), "{}", r.text());
            assert_eq!(read("fresh.bin"), b"f");
        }
        let names: Vec<String> = (std::fs::read_dir(root.join("d")).unwrap())
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.ends_with(".part")), "{names:?}");
        assert!(!lock(&CHUNKED).contains_key("rep2"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
