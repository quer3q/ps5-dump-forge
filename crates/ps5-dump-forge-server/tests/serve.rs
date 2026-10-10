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
    assert_eq!(note, format!("PS5 Dump Forge: open http://{}", srv.addr));
    // No Host or Origin rules: any Host, any Origin, no token, no content type.
    let req = "GET /api/session HTTP/1.1\r\nHost: evil.example\r\nOrigin: null\r\n\r\n";
    let (status, head, body) = exchange(srv.addr, req.as_bytes());
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["app"], "ps5-dump-forge");
    assert_eq!(v["platform"], "host");
    assert_eq!(v["stopping"], false);
    assert_eq!(v["self_copy"], "none"); // a host build carries no copy of itself
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
fn handler_cap() {
    let srv = start_with("cap", |o| o.deadline = Duration::from_secs(3));
    let idle: Vec<TcpStream> = (0..16)
        .map(|_| TcpStream::connect(srv.addr).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(300)); // all 16 accepted and waiting
    let (status, _, body) = exchange(srv.addr, b"");
    assert_eq!(status, 503, "{body}");
    drop(idle);
}

#[test]
fn paths_go_to_core_as_given() {
    let srv = start("paths");
    // Outside the root, through a symlink: no confinement.
    let outside = srv.root.parent().unwrap().join("outside");
    game(&outside.join("game"));
    symlink(&outside, srv.root.join("link")).unwrap();
    let (status, v) = srv.post("/api/inspect", json!({ "path": s(&outside.join("game")) }));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["title_id"], "PPSA01234");
    let via_link = srv.root.join("link/game");
    assert_eq!(
        srv.post("/api/inspect", json!({ "path": s(&via_link) })).0,
        200
    );
    let (status, v) = srv.post("/api/inspect", json!({ "path": s(&outside.join("gone")) }));
    assert_eq!(status, 500);
    assert!(v["error"].as_str().unwrap().contains("gone"), "{v}");
    let out = json!({ "source": s(&via_link), "format": "exfat", "dir": s(&outside) });
    let (status, v) = srv.post("/api/default_output", out);
    assert_eq!(
        (status, v),
        (200, json!(s(&outside.join("PPSA01234.exfat"))))
    );
    // The listing follows the link and works on any folder.
    let (status, v) = srv.post("/api/list_dir", json!({ "path": s(&outside) }));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["parent"], s(outside.parent().unwrap()));
    let (_, v) = srv.post("/api/list_dir", json!({ "path": s(&srv.root) }));
    assert_eq!(v["entries"][0]["name"], "link");
    assert_eq!(v["entries"][0]["dir"], true);
    assert_eq!(v["entries"][0]["path"], s(&outside)); // resolved
    let (status, _) = srv.post("/api/list_dir", json!({ "path": s(&outside.join("gone")) }));
    assert_eq!(status, 500);
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
    let entry = json!({ "name": s(root), "path": s(root), "dir": true, "size": null });
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
    let y = json!({ "name": "y", "path": s(&root.join("y")), "dir": true, "size": null });
    assert_eq!(entries[0], y);
    assert_eq!(entries[3]["size"], 3);
    let (_, v) = list(json!(s(&root.join("y"))));
    assert_eq!(v["parent"], s(root));
    assert_eq!(list(json!(s(&root.join("b.bin")))).0, 500); // not a folder
}

/// 10,001 files with long names: a listing of about 4 MB, cut at 10,000 entries.
fn big_folder(root: &Path) -> PathBuf {
    let dir = root.join("big");
    std::fs::create_dir(&dir).unwrap();
    for i in 0..10_001 {
        std::fs::write(dir.join(format!("{i:05}-{}", "n".repeat(150))), b"").unwrap();
    }
    dir
}

#[test]
fn list_dir_is_capped() {
    let srv = start("listcap");
    let big = big_folder(&srv.root);
    let (status, v) = srv.post("/api/list_dir", json!({ "path": s(&big) }));
    assert_eq!(status, 200);
    assert_eq!(v["entries"].as_array().unwrap().len(), 10_000);
    assert_eq!(v["truncated"], true);
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
    assert_eq!(job["request"]["lz4_profile"], Value::Null);
    assert_eq!(job["request"]["lz4_trace_space_mib"], 256);
    assert_eq!(job["request"]["lz4_in_place"], false);
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

/// The LZ4 fields reach core as sent (no route of their own): an LZ4 target is named like a
/// folder, and a trace on a title without `libSceAmpr` fails core's preflight.
#[test]
fn lz4_fields_pass_through() {
    let srv = start("lz4");
    let game = srv.root.join("game");
    self::game(&game);
    let out = json!({ "source": s(&game), "format": "lz4", "dir": s(&srv.root) });
    let (status, v) = srv.post("/api/default_output", out);
    assert_eq!((status, v), (200, json!(s(&srv.root.join("PPSA01234")))));
    let output = srv.root.join("traced");
    let mut req = request(&game, &output);
    req["request"]["format"] = json!("folder");
    req["request"]["lz4"] = json!("trace");
    req["request"]["lz4_trace_space_mib"] = json!(192);
    let (status, id) = srv.post("/api/start_job", req);
    assert_eq!(status, 200, "{id}");
    let mut job = Value::Null;
    wait(|| {
        job = srv.jobs()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .unwrap_or_default();
        !job["done"].is_null()
    });
    assert_eq!(job["request"]["lz4"], "trace");
    assert_eq!(job["request"]["lz4_trace_space_mib"], 192);
    let err = job["done"]["result"]["Err"].as_str().unwrap_or_default();
    assert!(err.contains("does not import libSceAmpr"), "{job}");
    assert!(!output.exists());

    // In place, on a folder: core refuses (folders are patched by their own route).
    let mut req = request(&game, &output);
    req["request"]["lz4"] = json!("unpatch");
    req["request"]["lz4_in_place"] = json!(true);
    let (status, id) = srv.post("/api/start_job", req);
    assert_eq!(status, 200, "{id}");
    wait(|| {
        job = srv.jobs()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .unwrap_or_default();
        !job["done"].is_null()
    });
    assert_eq!(job["request"]["lz4"], "unpatch");
    assert_eq!(job["request"]["lz4_in_place"], true);
    let err = job["done"]["result"]["Err"].as_str().unwrap_or_default();
    assert!(
        err.contains("a game folder is patched in place directly"),
        "{job}"
    );
}

/// `lz4_patch` changes the folder in place and answers with core's `Lz4Patch`; core's
/// refusals come back as its error text.
#[test]
fn lz4_patch_route() {
    let srv = start("lz4patch");
    let game = srv.root.join("game");
    self::game(&game);
    write(
        &game,
        "eboot.bin",
        b"\x7fELF fake eboot importing libSceAmpr.sprx",
    );
    write(&game, "ampr_commands.bin", b"last session");
    write(&game, "ampr_emu.log", b"last log");
    let (status, v) = srv.post("/api/lz4_patch", json!({ "source": s(&game) }));
    assert_eq!(status, 200, "{v}");
    // eboot.bin, param.json, data/a.bin and the runtime.
    assert_eq!(v["indexed"], 4);
    assert_eq!(v["removed"], json!(["ampr_commands.bin", "ampr_emu.log"]));
    assert!(v["warning"].as_str().unwrap().contains("ampr_emu"), "{v}");
    let runtime = std::fs::metadata(game.join("fakelib/libSceAmpr.sprx")).unwrap();
    assert_eq!(runtime.len(), 633_094, "the trace build");
    assert!(game.join("ampr_emu.index").is_file());
    assert!(!game.join("ampr_commands.bin").exists() && !game.join("ampr_emu.log").exists());

    // `lz4_unpatch` undoes it, with the same answer.
    write(&game, "ampr_commands.bin", b"this session");
    let (status, v) = srv.post("/api/lz4_unpatch", json!({ "source": s(&game) }));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["indexed"], 4);
    assert_eq!(v["removed"], json!(["ampr_commands.bin"]));
    let runtime = std::fs::metadata(game.join("fakelib/libSceAmpr.sprx")).unwrap();
    assert_eq!(runtime.len(), 423_350, "the release build");
    assert!(!game.join("ampr_commands.bin").exists());

    let plain = srv.root.join("plain");
    self::game(&plain);
    let (status, v) = srv.post("/api/lz4_patch", json!({ "source": s(&plain) }));
    assert_eq!(status, 500);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("does not import libSceAmpr"),
        "{v}"
    );
    assert!(!plain.join("fakelib").exists());
    assert_eq!(srv.post("/api/lz4_patch", json!({})).0, 400);
    let (status, v) = srv.post("/api/lz4_unpatch", json!({ "source": s(&plain) }));
    assert_eq!(status, 500);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("does not import libSceAmpr")
    );
    assert_eq!(srv.post("/api/lz4_unpatch", json!({})).0, 400);
}

#[test]
fn lz4_plan_profile_route() {
    let srv = start("lz4planprofile");
    let game = srv.root.join("game");
    self::game(&game);
    write(
        &game,
        "eboot.bin",
        b"\x7fELF fake eboot importing libSceAmpr.sprx",
    );
    let body = |source: &Path, format: &str| {
        json!({ "request": {
            "source": s(source), "format": format, "output": "",
            "compression_threads": null, "inner": null, "remove_backport": false,
        }})
    };
    let (status, v) = srv.post("/api/lz4_plan_profile", body(&game, "lz4"));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["file_name"], "[PPSA01234]-lz4profile.toml");
    assert_eq!(
        (v["packed"].as_u64(), v["loose"].as_u64()),
        (Some(1), Some(3)),
        "{v}"
    );
    let toml = v["toml"].as_str().unwrap();
    assert!(toml.contains("\"data/a.bin\","), "{toml}");
    assert!(
        v["log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().starts_with("LZ4 rules"))
    );
    assert!(!game.join("fakelib").exists() && !game.join("ampr_emu.index").exists());
    let (status, v) = srv.post("/api/lz4_plan_profile", body(&game, "exfat"));
    assert_eq!(status, 500);
    assert!(v["error"].as_str().unwrap().contains("LZ4 Pack"), "{v}");
    assert_eq!(srv.post("/api/lz4_plan_profile", json!({})).0, 400);
}

/// `encodeURIComponent`: every byte but the unreserved ones as `%XX`.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn traces_request(source: &Path) -> String {
    format!(
        "GET /api/lz4_traces?source={} HTTP/1.1\r\nHost: x\r\n\r\n",
        encode(s(source))
    )
}

/// Status, head and raw body of `GET /api/lz4_traces` for `source`.
fn traces_zip(addr: SocketAddr, source: &Path) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream.write_all(traces_request(source).as_bytes()).unwrap();
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).unwrap();
    let end = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(reply[..end].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head, reply.split_off(end + 4))
}

/// The zip's two entries: STORED, so each one's bytes follow its 30-byte local header and
/// name, and its 16-byte descriptor follows them. Python's zipfile (when installed) and, on
/// macOS, `unzip -t` and `ditto` check the whole archive too.
fn check_zip(zip: &[u8], files: &[(&str, &[u8])], tag: &str) {
    let mut at = 0;
    for (name, bytes) in files {
        assert_eq!(&zip[at..at + 4], b"PK\x03\x04", "{name}'s local header");
        let data = at + 30 + name.len();
        assert_eq!(&zip[at + 30..data], name.as_bytes());
        assert!(&zip[data..data + bytes.len()] == *bytes, "{name}'s bytes");
        at = data + bytes.len() + 16;
    }
    let dir = dir(&format!("zip-{tag}"));
    let path = dir.join("t.zip");
    std::fs::write(&path, zip).unwrap();
    let script = "import zipfile,sys\nz=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None\n\
                  for i in z.infolist(): print(i.filename, i.file_size, i.compress_type)";
    match std::process::Command::new("python3")
        .args(["-c", script])
        .arg(&path)
        .output()
    {
        Ok(out) => {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let want: String = files
                .iter()
                .map(|(n, b)| format!("{n} {} 0\n", b.len()))
                .collect();
            assert_eq!(String::from_utf8_lossy(&out.stdout), want);
        }
        Err(e) => eprintln!("python3 not run: {e}"),
    }
    if cfg!(target_os = "macos") {
        let run = |cmd: &mut std::process::Command| cmd.output().unwrap().status.success();
        assert!(run(std::process::Command::new("unzip")
            .arg("-tq")
            .arg(&path)));
        let x = dir.join("x");
        assert!(run(std::process::Command::new("ditto")
            .args(["-x", "-k"])
            .arg(&path)
            .arg(&x)));
        for (name, bytes) in files {
            assert!(
                std::fs::read(x.join(name)).unwrap() == *bytes,
                "ditto {name}"
            );
        }
    }
}

/// `GET /api/lz4_traces` streams both trace files of a folder or an image as one zip (a
/// journal over several 1 MiB chunks) named after the game; a source without them is a 404
/// naming what is missing, a source that can't be read core's error.
#[test]
fn lz4_traces_route() {
    let srv = start("traces");
    let game = srv.root.join("Traced Game");
    self::game(&game);
    write(
        &game,
        "sce_sys/param.json",
        r#"{"titleId":"PPSA01234","titleName":"Stellar: Blade/ドラ"}"#.as_bytes(),
    );
    let journal: Vec<u8> = (0..5 * 1024 * 1024 + 12_345u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let index = b"fake ampr_emu.index: the journal's file ids".to_vec();
    write(&game, "ampr_commands.bin", &journal);
    write(&game, "ampr_emu.index", &index);

    // The same game as a .ffpkg, the traces at its root as a traced image keeps them.
    let image = srv.root.join("traced.ffpkg");
    let mut req = request(&game, &image);
    req["request"]["format"] = json!("ffpkg");
    let (status, id) = srv.post("/api/start_job", req);
    assert_eq!(status, 200, "{id}");
    let mut job = Value::Null;
    wait(|| {
        job = srv.jobs()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .cloned()
            .unwrap_or_default();
        !job["done"].is_null()
    });
    assert!(job["done"]["result"]["Ok"].is_object(), "{job}");
    // What the page enables Download traces by.
    let (status, ins) = srv.post("/api/inspect", json!({ "path": s(&image) }));
    assert_eq!(status, 200, "{ins}");
    assert_eq!(ins["lz4"]["journal_bytes"], journal.len());

    let files: [(&str, &[u8]); 2] = [("ampr_commands.bin", &journal), ("ampr_emu.index", &index)];
    for (tag, source) in [("folder", &game), ("image", &image)] {
        let (status, head, body) = traces_zip(srv.addr, source);
        assert_eq!(status, 200, "{head}");
        for line in [
            "Content-Type: application/zip".to_string(),
            "Content-Disposition: attachment; filename=\"[Stellar Blade__]-[PPSA01234]-amprtrace.zip\"; \
             filename*=UTF-8''%5BStellar%20Blade%E3%83%89%E3%83%A9%5D-%5BPPSA01234%5D-amprtrace.zip"
                .to_string(),
            format!("Content-Length: {}", body.len()),
            "Cache-Control: no-store".to_string(),
        ] {
            assert!(head.contains(&line), "{line} in {head}");
        }
        check_zip(&body, &files, tag);
    }

    let plain = srv.root.join("plain");
    self::game(&plain);
    let (status, _, body) = traces_zip(srv.addr, &plain);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, 404, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("no ampr_commands.bin"),
        "{v}"
    );
    write(&plain, "ampr_commands.bin", b"journal");
    let (status, _, body) = traces_zip(srv.addr, &plain);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, 404, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("no ampr_emu.index beside"),
        "{v}"
    );
    assert_eq!(traces_zip(srv.addr, &srv.root.join("gone.ffpkg")).0, 500);
    assert_eq!(srv.get("/api/lz4_traces").0, 400);
    assert_eq!(
        srv.get("/api/lz4_trace_file?source=x&name=ampr_emu.index")
            .0,
        404
    );

    // The image written to meanwhile (a game relaunched on a writable mount): the next read
    // fails rather than hand out the old blocks.
    let mut t = ps5_dump_forge_core::lz4_traces(&image).unwrap().unwrap();
    let mut buf = vec![0u8; 1 << 20];
    assert_eq!(t.read(0, &mut buf).unwrap(), 1 << 20);
    let file = std::fs::File::options().write(true).open(&image).unwrap();
    let later = file.metadata().unwrap().modified().unwrap() + Duration::from_secs(2);
    file.set_modified(later).unwrap();
    let err = t.read(0, &mut buf).unwrap_err();
    assert!(
        err.to_string().contains("changed during the download"),
        "{err}"
    );
}

/// A journal rewritten mid-download (the game relaunched) ends the transfer short of its
/// `Content-Length`, so the browser marks the download failed instead of keeping a mix.
#[test]
fn lz4_traces_stop_when_the_source_changes() {
    let srv = start("traces-changed");
    let game = srv.root.join("game");
    self::game(&game);
    // Far more than the socket buffers hold, so most of it is still unread when it changes.
    let journal = vec![0x5au8; 40 << 20];
    write(&game, "ampr_commands.bin", &journal);
    write(&game, "ampr_emu.index", b"idx");
    let mut stream = TcpStream::connect(srv.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream.write_all(traces_request(&game).as_bytes()).unwrap();
    let mut reply = vec![0u8; 64 * 1024];
    let n = stream.read(&mut reply).unwrap();
    reply.truncate(n);
    assert!(reply.starts_with(b"HTTP/1.1 200 OK\r\n"));
    let head = String::from_utf8_lossy(&reply).into_owned();
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // the server fills the buffers and blocks
    // The same length, other bytes: only the source check tells (an early end would not).
    write(&game, "ampr_commands.bin", &vec![0xa5u8; journal.len()]);
    stream.read_to_end(&mut reply).unwrap();
    let end = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let body = &reply[end..];
    assert!(body.len() < length, "{} of {length} bytes", body.len());
    assert!(
        body[30 + 17..].iter().all(|&b| b == 0x5a),
        "only the old session's bytes"
    );
}

#[test]
fn stale_parts_are_capped() {
    let srv = start("stalecap");
    for i in 0..1_005 {
        write(&srv.root, &format!("g{i:04}.exfat.1-1.part"), b"");
    }
    let (status, v) = srv.post("/api/stale_parts", json!({ "dirs": [s(&srv.root)] }));
    assert_eq!(status, 200);
    let parts = v.as_array().unwrap();
    assert_eq!(parts.len(), 1_000);
    assert_eq!(parts[0], s(&srv.root.join("g0000.exfat.1-1.part")));
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
    assert!(srv.root.join("game").exists());
    assert_eq!(srv.post("/api/quit", json!({})), (200, json!({})));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(srv.exits.lock().unwrap().len(), 1);
}

#[test]
fn port_in_use() {
    let root = dir("inuse");
    let (notes, exits) = (Log::default(), Log::default());
    // Another copy of the server: "already running", success.
    let forge = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = forge.local_addr().unwrap().port();
    let opts = options(&root, &notes, &exits);
    std::thread::spawn(move || serve_on(forge, opts));
    wait(|| !notes.lock().unwrap().is_empty());
    let mut opts = options(&root, &notes, &exits);
    opts.port = port;
    serve(opts).unwrap();
    let last = notes.lock().unwrap().last().unwrap().clone();
    assert!(last.contains("already running at http://"), "{last}");
    // Anything else: an error.
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
