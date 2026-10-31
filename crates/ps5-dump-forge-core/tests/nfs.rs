//! An NFS export as a game source and as an output. Runs only with
//! `FORGE_NFS_TEST=nfs://host[:port]/path/to/out`: a writable folder whose parent holds the
//! generated game `game-nfs` (`tests/scripts/nfs.sh up` prints one line per server: unfs3 for
//! NFSv3, NFS-Ganesha for NFSv4.1). Everything is checked over NFS itself (Ganesha's export is a Docker
//! volume, not a folder here). `FORGE_NFS_TEST_LOCAL` (the `out` folder on this machine) or
//! `FORGE_NFS_TEST_CONTAINER` (the server's container: `docker exec rm -rf`) removes the run's
//! folder afterwards (the client removes files only).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;

use ps5_dump_forge_core::remote::FlinkPath;
use ps5_dump_forge_core::{ConvertRequest, Event, Format, JobReport, Jobs, KrakenLevel, list_dir};

fn request(source: &Path, format: Format, output: &Path) -> ConvertRequest {
    ConvertRequest {
        source: source.to_path_buf(),
        format,
        output: output.to_path_buf(),
        compression_threads: None,
        inner: None,
        remove_backport: false,
        full_verify: true,
        kraken_level: KrakenLevel::Fast,
        ffpfsc_level: 6,
        lz4: None,
        zstd_pack: false,
        zstd_level: 5,
        zstd_unpack: false,
        lz4_level: 0,
        lz4_runtime: Default::default(),
        edits: Vec::new(),
        extract: Vec::new(),
        transfer: Vec::new(),
        replace: false,
        edit_rebuild: false,
        install: None,
        delete_source: false,
        archive_password: None,
    }
}

fn jobs() -> (Jobs, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let jobs = Jobs::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    (jobs, rx)
}

fn run(jobs: &Jobs, rx: &mpsc::Receiver<Event>, req: ConvertRequest) -> Result<JobReport, String> {
    jobs.start(req);
    loop {
        if let Event::Done { result, .. } = rx.recv().unwrap() {
            return result;
        }
    }
}

/// Every file under a local folder by its `/`-separated path, junk left out.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            let name = e.file_name().into_string().unwrap();
            if name == ".DS_Store" || name.starts_with("._") {
                continue;
            }
            let path = e.path();
            if e.file_type().unwrap().is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let rel = rel.to_str().unwrap().replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// A local folder of this run, empty, removed when dropped.
struct Local(PathBuf);

impl Local {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("nfs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Local(dir)
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// This run's folder in the server's `out` (made by the jobs), removed when dropped.
struct OnServer {
    name: String,
    server_path: String,
}

impl Drop for OnServer {
    fn drop(&mut self) {
        if let Ok(local) = std::env::var("FORGE_NFS_TEST_LOCAL") {
            let _ = std::fs::remove_dir_all(Path::new(&local).join(&self.name));
        } else if let Ok(container) = std::env::var("FORGE_NFS_TEST_CONTAINER") {
            let _ = std::process::Command::new("docker")
                .args(["exec", &container, "rm", "-rf", &self.server_path])
                .status();
        } else {
            eprintln!("cleanup: {} left on the server", self.server_path);
        }
    }
}

/// The writable `out` folder and the game beside it, as `nfs://` paths.
fn out_and_game() -> Option<(PathBuf, PathBuf)> {
    let url = std::env::var("FORGE_NFS_TEST").ok()?;
    let out = FlinkPath::parse(Path::new(&url)).expect("nfs://host[:port]/path");
    let game = out
        .parent()
        .expect("a folder below an export")
        .join("game-nfs");
    Some((out.url(), game.url()))
}

#[test]
fn outputs_onto_an_export_read_back_as_the_game() {
    let Some((out, game)) = out_and_game() else {
        eprintln!("skipped: set FORGE_NFS_TEST");
        return;
    };
    let (jobs, rx) = jobs();
    let local = Local::new("out");
    let src = local.0.join("game");
    run(&jobs, &rx, request(&game, Format::Folder, &src)).unwrap();
    let want = files(&src);

    let name = format!("core-{}", std::process::id());
    let there = out.join(&name);
    let at = FlinkPath::parse(&there).unwrap();
    let server = OnServer {
        server_path: format!("/{}", at.path),
        name,
    };
    // An output folder's parent exists, as on a local disk.
    let (host, port) = ps5_dump_forge_core::remote::host_port(&at.key_id, 2049);
    let session = ps5_dump_forge_nfs::Session::connect(host, Some(port)).unwrap();
    eprintln!("{}", session.version());
    session.mkdir(&server.server_path).unwrap();

    // A folder onto the export, read back over NFS.
    let folder = there.join("folder");
    let t = std::time::Instant::now();
    run(&jobs, &rx, request(&src, Format::Folder, &folder)).unwrap();
    eprintln!("folder -> folder on NFS (full verify): {:?}", t.elapsed());
    let back = local.0.join("folder-back");
    run(&jobs, &rx, request(&folder, Format::Folder, &back)).unwrap();
    assert_eq!(files(&back), want);

    // An fpkg streamed onto the export; fetched back by a transfer, it extracts to the game.
    let pkg = there.join("PPSA99905.pkg");
    let t = std::time::Instant::now();
    let report = run(&jobs, &rx, request(&src, Format::Pkg, &pkg)).unwrap();
    eprintln!("folder -> fpkg on NFS (full verify): {:?}", t.elapsed());
    let fetched = local.0.join("fetched");
    std::fs::create_dir_all(&fetched).unwrap();
    let mut req = request(Path::new(""), Format::Folder, &fetched);
    req.transfer = vec![pkg.clone()];
    run(&jobs, &rx, req).unwrap();
    let local_pkg = fetched.join("PPSA99905.pkg");
    assert_eq!(report.bytes, std::fs::metadata(&local_pkg).unwrap().len());
    assert!(std::fs::read(&local_pkg).unwrap().starts_with(b"\x7fFIH"));
    let back = local.0.join("pkg-back");
    run(&jobs, &rx, request(&local_pkg, Format::Folder, &back)).unwrap();
    // The build rewrites param.json and adds a keystone; every other file is the source's.
    let mut got = files(&back);
    assert!(got.remove("sce_sys/keystone").is_some());
    let param = got.remove("sce_sys/param.json").unwrap();
    assert!(String::from_utf8_lossy(&param).contains("PPSA99905"));
    let mut rest = want.clone();
    rest.remove("sce_sys/param.json");
    assert_eq!(got, rest);

    // A transfer the user confirmed replacing: files over files, the folder merged.
    let re = local.0.join("re");
    std::fs::create_dir_all(re.join("folder")).unwrap();
    std::fs::write(re.join("folder/eboot.bin"), b"new eboot").unwrap();
    std::fs::write(re.join("folder/extra.bin"), b"extra").unwrap();
    let mut req = request(Path::new(""), Format::Folder, &there);
    req.transfer = vec![re.join("folder")];
    let e = run(&jobs, &rx, req.clone()).unwrap_err();
    assert!(e.contains("already exists (never replaced)"), "{e}");
    req.replace = true;
    run(&jobs, &rx, req).unwrap();
    // Fetched back by a transfer (which reads no file as a game's).
    let back = local.0.join("replaced-back");
    std::fs::create_dir_all(&back).unwrap();
    let mut req = request(Path::new(""), Format::Folder, &back);
    req.transfer = vec![folder.clone()];
    run(&jobs, &rx, req).unwrap();
    let mut expect = want.clone();
    expect.insert("eboot.bin".into(), b"new eboot".to_vec());
    expect.insert("extra.bin".into(), b"extra".to_vec());
    assert_eq!(files(&back.join("folder")), expect);

    // Nothing else left behind: no .part, no temporary name.
    let listing = list_dir(&there).unwrap();
    let mut left: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    left.sort();
    assert_eq!(left, ["PPSA99905.pkg", "folder"]);
    drop(server);
}
