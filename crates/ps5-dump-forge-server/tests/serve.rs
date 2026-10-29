//! Black-box checks of `serve_on` over real sockets on 127.0.0.1, one server per test with
//! its own root, a captured notify and an exit hook that only records.
// Unix: symlinks, FIFOs and `/`-rooted paths.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ps5_dump_forge_server::{Options, Platform, serve, serve_on};
use serde_json::{Value, json};

type Log<T> = Arc<Mutex<Vec<T>>>;

struct Srv {
    addr: SocketAddr,
    root: PathBuf,
    notes: Log<String>,
    exits: Log<i32>,
}

fn dir(name: &str) -> PathBuf {
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("srv-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A minimal game folder.
fn game(root: &Path) {
    write(root, "eboot.bin", b"\x7fELF fake eboot");
    write(
        root,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    write(root, "data/a.bin", &[7u8; 100_000]);
}

fn mkfifo(path: &Path) {
    let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: a NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
}

fn options(root: &Path, notes: &Log<String>, exits: &Log<i32>) -> Options {
    let sink = notes.clone();
    let mut opts = Options::new(
        0,
        vec![root.to_path_buf()],
        Platform::Host,
        move |l: &str| sink.lock().unwrap().push(l.to_string()),
    );
    let exits = exits.clone();
    opts.exit = Arc::new(move |code| exits.lock().unwrap().push(code));
    opts
}

fn start_with(name: &str, tune: impl FnOnce(&mut Options)) -> Srv {
    let root = dir(name).join("root");
    std::fs::create_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (notes, exits) = (Log::default(), Log::default());
    let mut opts = options(&root, &notes, &exits);
    tune(&mut opts);
    std::thread::spawn(move || serve_on(listener, opts));
    let srv = Srv {
        addr,
        root,
        notes,
        exits,
    };
    wait(|| !srv.notes.lock().unwrap().is_empty());
    srv
}

fn start(name: &str) -> Srv {
    start_with(name, |_| {})
}

fn wait(mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Status, head and body of one raw exchange.
fn exchange(addr: SocketAddr, request: &[u8]) -> (u16, String, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let _ = s.write_all(request);
    let mut reply = Vec::new();
    let _ = s.read_to_end(&mut reply);
    let reply = String::from_utf8(reply).unwrap();
    let (head, body) = reply.split_once("\r\n\r\n").unwrap_or((&reply, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, head.to_string(), body.to_string())
}

fn raw_post(path: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

impl Srv {
    fn get(&self, path: &str) -> (u16, Value) {
        let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n");
        let (status, _, body) = exchange(self.addr, req.as_bytes());
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let (status, _, body) = exchange(self.addr, raw_post(path, &body.to_string()).as_bytes());
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn jobs(&self) -> Value {
        let (status, v) = self.get("/api/jobs");
        assert_eq!(status, 200, "{v}");
        v
    }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn request(source: &Path, output: &Path) -> Value {
    json!({ "request": {
        "source": s(source), "format": "exfat", "output": s(output),
        "compression_threads": null, "inner": null, "remove_backport": false,
    }})
}

#[test]
fn session_and_headers() {
    let srv = start("session");
    let note = srv.notes.lock().unwrap()[0].clone();
    assert_eq!(note, format!("Dump Forge: open http://{}", srv.addr));
    // No Host or Origin rules: any Host, any Origin, no token, no content type.
    let req = "GET /api/session HTTP/1.1\r\nHost: evil.example\r\nOrigin: null\r\n\r\n";
    let (status, head, body) = exchange(srv.addr, req.as_bytes());
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["app"], "ps5-dump-forge");
    assert_eq!(v["platform"], "host");
    assert_eq!(v["stopping"], false);
    assert_eq!(v["self_copy"], "none"); // a host build carries no copy of itself
    assert_eq!(v["handover"], true); // it takes quit {"if_idle": true}
    assert_eq!(v["instance"].as_str().unwrap().len(), 16);
    assert_eq!(v["separator"], std::path::MAIN_SEPARATOR_STR);
    assert_eq!(v["url"], format!("http://{}", srv.addr));
    // The url as a QR code for a phone: an SVG, dark on white.
    let (qr_status, qr_head, svg) = exchange(srv.addr, b"GET /api/qr HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(qr_status, 200);
    assert!(qr_head.contains("Content-Type: image/svg+xml"), "{qr_head}");
    assert!(
        svg.starts_with("<svg") && svg.contains("fill=\"#000\" d=\"M"),
        "{svg}"
    );
    // `?connect=1`: the Connect tab's address (a longer text, so another code).
    let (status, _, connect_svg) = exchange(
        srv.addr,
        b"GET /api/qr?connect=1 HTTP/1.1\r\nHost: x\r\n\r\n",
    );
    assert_eq!(status, 200);
    assert!(connect_svg.starts_with("<svg") && connect_svg != svg);
    assert!(v.get("paired").is_none());
    assert!(head.contains("Cache-Control: no-store"), "{head}");
    for gone in [
        "nosniff",
        "Referrer-Policy",
        "Content-Security-Policy",
        "Access-Control",
    ] {
        assert!(!head.contains(gone), "{head}");
    }
    // The page: HTML, rechecked on every load (so a reloaded payload shows its own UI), no
    // security headers.
    let (status, head, _) = exchange(srv.addr, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(status, 200);
    assert!(head.contains("Content-Type: text/html"), "{head}");
    assert!(head.contains("Cache-Control: no-cache"), "{head}");
    assert!(!head.contains("Content-Security"), "{head}");
    assert_eq!(srv.get("/nope.js").0, 404);
    assert_eq!(srv.post("/api/nope", json!({})).0, 404);
    // Bodies must still be JSON.
    let (status, _, body) = exchange(srv.addr, raw_post("/api/list_dir", "path=/").as_bytes());
    assert_eq!(status, 400);
    assert!(serde_json::from_str::<Value>(&body).unwrap()["error"].is_string());
    assert_eq!(
        exchange(srv.addr, raw_post("/api/quit", "").as_bytes()).0,
        400
    );
    assert!(srv.exits.lock().unwrap().is_empty());
}

#[test]
fn parser_limits() {
    let srv = start_with("parse", |o| o.deadline = Duration::from_millis(300));
    let refused = [
        (
            "POST /api/quit HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
                .to_string(),
            400,
        ),
        (
            "POST /api/quit HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}"
                .to_string(),
            400,
        ),
        (
            "GET / HTTP/1.1\r\nHost: x\r\nHost: x\r\n\r\n".to_string(),
            400,
        ),
        (
            format!(
                "GET /api/session HTTP/1.1\r\nHost: x\r\nX-Pad: {}\r\n\r\n",
                "a".repeat(17 * 1024)
            ),
            413,
        ),
        (
            format!(
                "POST /api/quit HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n",
                2 << 20
            ),
            413,
        ),
        ("DELETE / HTTP/1.1\r\nHost: x\r\n\r\n".to_string(), 400),
        ("GET http://x/ HTTP/1.1\r\nHost: x\r\n\r\n".to_string(), 400),
    ];
    for (req, code) in refused {
        let (status, _, body) = exchange(srv.addr, req.as_bytes());
        assert_eq!(status, code, "{}", &req[..60.min(req.len())]);
        assert!(serde_json::from_str::<Value>(&body).unwrap()["error"].is_string());
    }
    assert!(srv.exits.lock().unwrap().is_empty());
    // Slowloris: the deadline is absolute, however the bytes trickle.
    let start = Instant::now();
    let mut s = TcpStream::connect(srv.addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    for b in b"GET / HTTP/1.1\r\nHo" {
        let _ = s.write_all(&[*b]);
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut reply = String::new();
    let _ = s.read_to_string(&mut reply);
    assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
    assert!(start.elapsed() < Duration::from_secs(3));
    // A body that never comes.
    let mut s = TcpStream::connect(srv.addr).unwrap();
    s.write_all(b"POST /api/quit HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\n\r\n{")
        .unwrap();
    let mut reply = String::new();
    let _ = s.read_to_string(&mut reply);
    assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
}

#[test]
fn list_dir_shape() {
    let srv = start("list");
    let root = &srv.root;
    write(root, "b.bin", b"abc");
    write(root, "a.bin", b"");
    std::fs::create_dir(root.join("z")).unwrap();
    std::fs::create_dir(root.join("y")).unwrap();
    symlink("/nonexistent/forge", root.join("dangling")).unwrap();
    let list = |path: Value| srv.post("/api/list_dir", json!({ "path": path }));

    let (status, v) = list(Value::Null);
    assert_eq!(status, 200);
    let entry = json!({ "name": s(root), "path": s(root), "dir": true, "size": null, "link": false, "mounted": false });
    let roots = json!({ "path": null, "parent": null, "entries": [entry], "truncated": false });
    assert_eq!(v, roots);
    let (_, v) = list(json!(s(root)));
    assert_eq!(v["path"], s(root));
    assert_eq!(
        (&v["parent"], &v["truncated"]),
        (&Value::Null, &json!(false))
    ); // a root
    let entries = &v["entries"];
    let names: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["y", "z", "a.bin", "b.bin", "dangling"]);
    assert_eq!(entries[4]["path"], s(&root.join("dangling"))); // a broken link as itself
    assert_eq!(entries[4]["link"], true);
    let y = json!({ "name": "y", "path": s(&root.join("y")), "dir": true, "size": null, "link": false, "mounted": false });
    assert_eq!(entries[0], y);
    assert_eq!(entries[3]["size"], 3);
    let (_, v) = list(json!(s(&root.join("y"))));
    assert_eq!(v["parent"], s(root));
    assert_eq!(list(json!(s(&root.join("b.bin")))).0, 500); // not a folder
    // The Storage explorer: the Library's roots (no /mnt/shadowmnt, which a computer has
    // none of), `parent` null at one; off the PS5 the same listing.
    let explore =
        |path: Value| srv.post("/api/list_dir", json!({ "path": path, "explorer": true }));
    assert_eq!(explore(Value::Null), (200, roots));
    let (status, e) = explore(json!(s(root)));
    assert_eq!(status, 200);
    assert_eq!(e, list(json!(s(root))).1);
    assert_eq!(e["entries"][3]["mounted"], false);
    assert_eq!(explore(json!(s(&root.join("y")))).1["parent"], s(root));
}

#[test]
fn special_sources_are_refused() {
    let srv = start("fifo");
    // A FIFO would block its handler (or job) on open; refused before anything opens it.
    let fifo = srv.root.join("x.ffpfs");
    mkfifo(&fifo);
    let link = srv.root.join("link.ffpfs");
    symlink(&fifo, &link).unwrap(); // followed
    let naming = |source: &Path| json!({ "source": s(source), "format": "exfat", "dir": s(&srv.root), "taken": [] });
    for source in [&fifo, &link] {
        for (route, body) in [
            ("/api/inspect", json!({ "path": s(source) })),
            ("/api/default_output", naming(source)),
            ("/api/generated_output", naming(source)),
            ("/api/start_job", request(source, &srv.root.join("o.exfat"))),
        ] {
            let (status, v) = srv.post(route, body);
            assert_eq!(status, 400, "{route}: {v}");
            let text = v["error"].as_str().unwrap();
            assert!(text.contains("not a folder or a regular file"), "{text}");
        }
    }
    assert!(srv.jobs()["jobs"].as_array().unwrap().is_empty());
}

#[test]
fn job_end_to_end() {
    let srv = start("job");
    let game = srv.root.join("My Game");
    self::game(&game);
    let out = json!({ "source": s(&game), "format": "exfat", "dir": s(&srv.root), "taken": [] });
    let (status, output) = srv.post("/api/generated_output", out);
    assert_eq!(status, 200, "{output}");
    let output = PathBuf::from(output.as_str().unwrap());
    assert_eq!(output.parent(), Some(srv.root.as_path()));
    let (status, id) = srv.post("/api/start_job", request(&game, &output));
    assert_eq!(status, 200, "{id}");
    let id = id.as_u64().unwrap();
    let mut job = Value::Null;
    wait(|| {
        let snap = srv.jobs();
        job = snap["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .unwrap_or_default();
        !job["done"].is_null()
    });
    assert_eq!(job["done"]["kind"], "done");
    assert_eq!(job["done"]["result"]["Ok"]["output"], s(&output), "{job}");
    assert_eq!(job["request"]["source"], s(&game));
    assert!(job["request"]["compression_threads"].as_u64().unwrap() >= 1);
    // A request without the LZ4 fields (an older page) gets core's defaults.
    assert_eq!(job["request"]["lz4"], Value::Null);
    assert_eq!(job["request"]["lz4_level"], 0);
    assert_eq!(job["progress"]["kind"], "progress");
    let total = job["log_total"].as_u64().unwrap();
    assert!(total >= 1);
    assert_eq!(job["log"].as_array().unwrap().len() as u64, total.min(200));
    assert!(output.is_file());
    assert_eq!(
        srv.post("/api/cancel_job", json!({ "id": id })),
        (200, Value::Null)
    );
}

/// `POST /api/delete_path`: `null` once gone; refusals 400, missing 404. Links inside a
/// deleted folder go, what they point at stays.
#[test]
fn delete_path_route() {
    let srv = start("delete");
    let outside = dir("delete-outside");
    write(&outside, "keep.bin", b"keep");
    game(&srv.root.join("game"));
    symlink(outside.join("keep.bin"), srv.root.join("game/link")).unwrap();
    write(&srv.root, "a.exfat", b"image");
    symlink(srv.root.join("a.exfat"), srv.root.join("alias.exfat")).unwrap();
    let del = |path: &Path| srv.post("/api/delete_path", json!({ "path": s(path) }));
    let refused = |path: &Path| {
        let (status, v) = del(path);
        assert_eq!(status, 400, "{}: {v}", path.display());
        assert!(v["error"].is_string(), "{v}");
    };
    refused(&srv.root); // a browse root
    refused(srv.root.parent().unwrap()); // a folder holding one
    refused(Path::new("/"));
    refused(Path::new(""));
    refused(&srv.root.join("alias.exfat")); // a link itself
    assert_eq!(del(&srv.root.join("nope")).0, 404);
    assert_eq!(
        exchange(srv.addr, raw_post("/api/delete_path", "{}").as_bytes()).0,
        400
    );
    assert_eq!(del(&srv.root.join("a.exfat")), (200, Value::Null));
    assert_eq!(del(&srv.root.join("game")), (200, Value::Null));
    assert!(!srv.root.join("a.exfat").exists() && !srv.root.join("game").exists());
    assert_eq!(std::fs::read(outside.join("keep.bin")).unwrap(), b"keep");
    std::fs::remove_dir_all(&outside).unwrap();
}

/// `POST /api/move_path`: `null` once moved; refusals 400 (a browse root, a link, an existing
/// target, into itself), a missing source or target folder 404, a bad body 400.
#[test]
fn move_path_route() {
    let srv = start("move");
    game(&srv.root.join("game"));
    std::fs::create_dir(srv.root.join("into")).unwrap();
    write(&srv.root, "a.exfat", b"image");
    write(&srv.root, "b.exfat", b"other");
    symlink(srv.root.join("a.exfat"), srv.root.join("alias.exfat")).unwrap();
    let mv = |from: &Path, to: &Path| {
        srv.post("/api/move_path", json!({ "from": s(from), "to": s(to) }))
    };
    let refused = |from: &Path, to: &Path| {
        let (status, v) = mv(from, to);
        assert_eq!(status, 400, "{} -> {}: {v}", from.display(), to.display());
        assert!(v["error"].is_string(), "{v}");
    };
    refused(&srv.root, &srv.root.join("into/root")); // a browse root
    refused(&srv.root.join("alias.exfat"), &srv.root.join("c.exfat")); // a link itself
    refused(&srv.root.join("a.exfat"), &srv.root.join("b.exfat")); // never replaced
    refused(&srv.root.join("game"), &srv.root.join("game/inner")); // into itself
    assert_eq!(mv(&srv.root.join("nope"), &srv.root.join("x")).0, 404);
    assert_eq!(
        mv(&srv.root.join("a.exfat"), &srv.root.join("none/a.exfat")).0,
        404
    );
    assert_eq!(
        exchange(
            srv.addr,
            raw_post("/api/move_path", "{\"from\": \"/x\"}").as_bytes()
        )
        .0,
        400
    );
    assert_eq!(std::fs::read(srv.root.join("b.exfat")).unwrap(), b"other");
    let renamed = srv.root.join("renamed.exfat");
    assert_eq!(mv(&srv.root.join("a.exfat"), &renamed), (200, Value::Null));
    assert_eq!(std::fs::read(&renamed).unwrap(), b"image");
    let moved = srv.root.join("into/game");
    assert_eq!(mv(&srv.root.join("game"), &moved), (200, Value::Null));
    assert!(!srv.root.join("a.exfat").exists() && !srv.root.join("game").exists());
    assert!(moved.join("sce_sys/param.json").is_file());
}

#[test]
fn quit_closes_admission() {
    let srv = start("quit");
    game(&srv.root.join("game"));
    assert_eq!(srv.post("/api/quit", json!({})), (200, json!({})));
    wait(|| !srv.exits.lock().unwrap().is_empty());
    assert_eq!(*srv.exits.lock().unwrap(), [0]);
    assert!(
        srv.notes
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.contains("stopped"))
    );
    assert_eq!(srv.get("/api/session").1["stopping"], true);
    let req = request(&srv.root.join("game"), &srv.root.join("x.exfat"));
    let (status, v) = srv.post("/api/start_job", req);
    assert_eq!(status, 409, "{v}");
    let (status, v) = srv.post(
        "/api/delete_path",
        json!({ "path": s(&srv.root.join("game")) }),
    );
    assert_eq!(status, 409, "{v}");
    let (status, v) = srv.post(
        "/api/move_path",
        json!({ "from": s(&srv.root.join("game")), "to": s(&srv.root.join("moved")) }),
    );
    assert_eq!(status, 409, "{v}");
    assert!(srv.root.join("game").exists());
    assert_eq!(srv.post("/api/quit", json!({})), (200, json!({})));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(srv.exits.lock().unwrap().len(), 1);
}

#[test]
fn port_in_use() {
    let root = dir("inuse");
    let (notes, exits) = (Log::default(), Log::default());
    // Another copy of the server is taken over (`handover`); anything else: an error.
    let other = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = other.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in other.incoming().flatten() {
            let _ = s.shutdown(Shutdown::Both);
        }
    });
    let mut opts = options(&root, &notes, &exits);
    opts.port = port;
    assert!(serve(opts).is_err());
    let last = notes.lock().unwrap().last().unwrap().clone();
    assert!(
        last.contains(&format!("port {port} is used by another program")),
        "{last}"
    );
    // A peer that trickles a byte at a time: the probe gives up after 3 s overall.
    let trickle = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = trickle.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in trickle.incoming().flatten() {
            std::thread::spawn(move || {
                for _ in 0..200 {
                    if s.write_all(b"x").is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
        }
    });
    let mut opts = options(&root, &notes, &exits);
    opts.port = port;
    let start = Instant::now();
    assert!(serve(opts).is_err());
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

/// One request head (and its `Content-Length` body) off a raw connection.
fn read_request(s: &mut TcpStream) -> String {
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let (mut got, mut buf) = (Vec::new(), [0u8; 4096]);
    loop {
        let text = String::from_utf8_lossy(&got).into_owned();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .map_or(0, |n| n.trim().parse().unwrap());
            if body.len() >= length {
                return text;
            }
        }
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return text,
            Ok(n) => got.extend_from_slice(&buf[..n]),
        }
    }
}

/// A fake elfldr: answers every connection with `reply` and records each request.
fn fake_elfldr(reply: &'static str) -> (SocketAddr, Log<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log = Log::default();
    let seen = log.clone();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            seen.lock().unwrap().push(read_request(&mut s));
            let _ = s.write_all(reply.as_bytes());
        }
    });
    (addr, log)
}

/// `GET /api/elfldr` says whether elfldr answers; `POST /api/self_update` refuses an old or
/// malformed version before anything is downloaded or sent, and answers 503 while elfldr is down.
/// Offline: with elfldr up a newer version would be downloaded from GitHub
/// (ps5-dump-forge-updater's tests cover the download and the send).
#[test]
fn self_update_through_elfldr() {
    let newer = json!({ "version": "999.0.0" });
    let srv = start("update-host");
    assert_eq!(srv.post("/api/self_update", newer.clone()).0, 404);
    assert_eq!(srv.get("/api/elfldr").0, 404);

    let ps5 = |name: &str, elfldr: SocketAddr| {
        start_with(name, move |o| {
            o.platform = Platform::Ps5;
            o.elfldr = elfldr;
        })
    };
    // Nothing on the port.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = closed.local_addr().unwrap();
    drop(closed);
    let srv = ps5("update-none", addr);
    assert_eq!(srv.get("/api/elfldr"), (200, json!({ "up": false })));
    // elfldr is probed before anything is downloaded, so this stays offline.
    let (status, v) = srv.post("/api/self_update", newer.clone());
    assert_eq!(status, 503, "{v}");
    assert_eq!(
        v["error"],
        format!(
            "elfldr isn't running on port {}: load it, then try again",
            addr.port()
        )
    );

    // Something listening is up (the backlog accepts; nothing is read or sent).
    let listening = TcpListener::bind("127.0.0.1:0").unwrap();
    let srv = ps5("update-up", listening.local_addr().unwrap());
    assert_eq!(srv.get("/api/elfldr"), (200, json!({ "up": true })));

    // Old or malformed versions never reach elfldr, and the server keeps serving.
    let (addr, log) = fake_elfldr("HTTP/1.1 200 OK\r\n\r\n");
    let srv = ps5("update-old", addr);
    for bad in ["0.0.1", "999.0", "v999.0.0"] {
        let (status, v) = srv.post("/api/self_update", json!({ "version": bad }));
        assert_eq!(status, 400, "{bad}: {v}");
    }
    assert!(log.lock().unwrap().is_empty());
    assert_eq!(srv.get("/api/session").1["stopping"], false);
    assert!(srv.exits.lock().unwrap().is_empty());
}

/// A fake older copy of the server on a port of its own: `GET /api/session` gets `session`
/// without its `jobs`, `GET /api/jobs` gets `{jobs}`, `POST /api/quit` gets `quit` (200: it
/// then closes its port, as an exiting server does).
fn old_forge(session: Value, quit: u16) -> (u16, Log<String>) {
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log = Log::default();
    let seen = log.clone();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let request = read_request(&mut s);
            seen.lock().unwrap().push(request.clone());
            let (status, body) = if request.starts_with("POST /api/quit ") {
                (quit, json!({}))
            } else if request.starts_with("GET /api/jobs ") {
                (200, json!({ "jobs": session["jobs"] }))
            } else {
                let mut session = session.clone();
                session.as_object_mut().unwrap().remove("jobs");
                (200, session)
            };
            let body = body.to_string();
            let _ = write!(
                s,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            drop(s);
            if status == 200 && request.starts_with("POST /api/quit ") {
                break; // the listener closes with the thread
            }
        }
    });
    (port, log)
}

/// A copy started while another (any version) serves: it takes the port over when that one is
/// idle (`if_idle`, or for one without `handover` its job list); else it leaves it serving, with
/// a notice.
#[test]
fn handover() {
    let root = dir("handover");
    let current = env!("CARGO_PKG_VERSION");
    // `jobs` is what the fake answers `GET /api/jobs` with (left out of its session).
    let old = |handover: bool| {
        let mut v = json!({ "app": "ps5-dump-forge", "version": "0.0.1", "jobs": [] });
        if handover {
            v["handover"] = json!(true);
        }
        v
    };
    let run = |session: Value, quit: u16| {
        let (port, log) = old_forge(session, quit);
        let (notes, exits) = (Log::default(), Log::default());
        let mut opts = options(&root, &notes, &exits);
        opts.port = port;
        (port, log, notes, opts)
    };
    let posts = |log: &Log<String>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("POST "))
            .cloned()
            .collect::<Vec<_>>()
    };

    // Older and idle: asked to quit if idle, then this one serves on its port.
    let (port, log, notes, opts) = run(old(true), 200);
    std::thread::spawn(move || serve(opts));
    wait(|| {
        notes
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.contains("open http://"))
    });
    let posted = posts(&log);
    assert_eq!(posted.len(), 1);
    assert!(
        posted[0].starts_with("POST /api/quit HTTP/1.1\r\n"),
        "{}",
        posted[0]
    );
    assert!(
        posted[0].ends_with("\r\n\r\n{\"if_idle\":true}"),
        "{}",
        posted[0]
    );
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let (status, _, body) = exchange(addr, b"GET /api/session HTTP/1.1\r\nHost: x\r\n\r\n");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (status, &v["version"], &v["handover"]),
        (200, &json!(current), &json!(true))
    );

    // Older and busy: it keeps serving.
    let (_, _, notes, opts) = run(old(true), 409);
    serve(opts).unwrap();
    let last = notes.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        last,
        "Can't close Dump Forge - some jobs are still running."
    );

    // Older without `handover` (it would cancel its jobs): asked to quit (plain) only when its
    // job list shows every job finished, then this one serves on its port.
    // A long finished history (past the 64 KiB other replies are held to) is still read whole.
    let mut idle = old(false);
    idle["jobs"] = json!([{ "id": 1, "done": { "Ok": null }, "log": ["x".repeat(200 * 1024)] }]);
    let (port, log, notes, opts) = run(idle, 200);
    std::thread::spawn(move || serve(opts));
    wait(|| {
        notes
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.contains("open http://"))
    });
    let posted = posts(&log);
    assert_eq!(posted.len(), 1);
    assert!(posted[0].ends_with("\r\n\r\n{}"), "{}", posted[0]);
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("GET /api/jobs "))
    );
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let (status, _, _) = exchange(addr, b"GET /api/session HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(status, 200);

    // ... but a job still running (no `done`), or no job list: never asked, it keeps serving.
    for jobs in [json!([{ "id": 1, "done": null }]), json!(null)] {
        let mut busy = old(false);
        busy["jobs"] = jobs;
        let (_, log, notes, opts) = run(busy, 200);
        serve(opts).unwrap();
        let last = notes.lock().unwrap().last().unwrap().clone();
        assert_eq!(
            last,
            "Can't close Dump Forge - some jobs are still running."
        );
        assert!(posts(&log).is_empty());
    }

    // The same version (a copy pushed again) and a newer one: taken over too.
    for version in [current, "999.0.0"] {
        let mut running = old(true);
        running["version"] = json!(version);
        let (port, log, notes, opts) = run(running, 200);
        std::thread::spawn(move || serve(opts));
        wait(|| {
            notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.contains("open http://"))
        });
        assert_eq!(posts(&log).len(), 1);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let (status, _, _) = exchange(addr, b"GET /api/session HTTP/1.1\r\nHost: x\r\n\r\n");
        assert_eq!(status, 200);
    }

    // A start that couldn't serve (no usable --root) never stops the running copy.
    let (_, log, _, mut opts) = run(old(true), 200);
    opts.roots = vec![root.join("missing")];
    assert!(serve(opts).is_err());
    assert!(posts(&log).is_empty());

    // A quit that fails otherwise: an error, and the notice.
    let (_, _, notes, opts) = run(old(true), 500);
    assert!(serve(opts).is_err());
    let last = notes.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        last,
        format!("Dump Forge v{current} could not take over from v0.0.1")
    );
}
