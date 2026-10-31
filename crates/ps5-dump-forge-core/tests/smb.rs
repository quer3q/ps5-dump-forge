//! A NAS share as a game source and as an output. Runs only with
//! `FORGE_SMB_TEST=smb://[user[:pass]@]host[:port]/share` (the share holds `Stub Game`, see
//! `tests/scripts/nas.sh`); `FORGE_SMB_TEST_SHARE_DIR` (the share's folder on this machine, with a
//! writable `out`) also checks an `.exfat` image read over SMB and the outputs: an image, an
//! fpkg, an Extract and a transfer written onto the share and read back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;

use ps5_dump_forge_core::remote::{FlinkPath, host_port};
use ps5_dump_forge_core::smb::{SmbCredential, set_credentials};
use ps5_dump_forge_core::{ConvertRequest, Event, Format, JobReport, Jobs, KrakenLevel, inspect};

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

/// The share's URL without credentials (`smb://host[:port]/share`), after registering them.
fn share() -> Option<String> {
    let url = std::env::var("FORGE_SMB_TEST").ok()?;
    let rest = url.strip_prefix("smb://").expect("smb://");
    let (login, rest) = rest.rsplit_once('@').unwrap_or(("", rest));
    let (user, password) = login.split_once(':').unwrap_or((login, ""));
    let at = FlinkPath::parse(Path::new(&format!("smb://{rest}"))).unwrap();
    let (host, port) = host_port(&at.key_id, ps5_dump_forge_smb::DEFAULT_PORT);
    set_credentials(vec![SmbCredential {
        host: host.into(),
        port,
        user: user.into(),
        password: password.into(),
    }]);
    Some(at.url().display().to_string())
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

/// A folder of this run in the share's `out`, removed when dropped.
struct OutDir(PathBuf);

impl Drop for OutDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn outputs_onto_a_share_read_back_as_the_game() {
    let (Some(share), Ok(dir)) = (share(), std::env::var("FORGE_SMB_TEST_SHARE_DIR")) else {
        eprintln!("skipped: set FORGE_SMB_TEST and FORGE_SMB_TEST_SHARE_DIR");
        return;
    };
    let name = format!("core-{}", std::process::id());
    let here = OutDir(Path::new(&dir).join("out").join(&name));
    let _ = std::fs::remove_dir_all(&here.0);
    for sub in ["extract", "transfer"] {
        std::fs::create_dir_all(here.0.join(sub)).unwrap();
    }
    let there = format!("{share}/out/{name}");
    let at = |rel: &str| PathBuf::from(format!("{there}/{rel}"));
    let game = PathBuf::from(format!("{share}/Stub Game"));
    let local_game = Path::new(&dir).join("Stub Game");
    let want = files(&local_game);
    let local =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("smb-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&local);
    std::fs::create_dir_all(&local).unwrap();
    let (jobs, rx) = jobs();

    // An .exfat from the share onto the share, then read back over SMB.
    let image = at("stub.exfat");
    let t = std::time::Instant::now();
    let report = run(&jobs, &rx, request(&game, Format::Exfat, &image)).unwrap();
    eprintln!(
        "share -> .exfat on the share (full verify): {:?}",
        t.elapsed()
    );
    assert_eq!(report.output, image);
    assert_eq!(
        report.bytes,
        std::fs::metadata(here.0.join("stub.exfat")).unwrap().len()
    );
    let info = inspect(&image).unwrap();
    assert_eq!(info.title_id.as_deref(), Some("PPSA99999"));
    let back = local.join("exfat-back");
    run(&jobs, &rx, request(&image, Format::Folder, &back)).unwrap();
    assert_eq!(files(&back), want);

    // An fpkg streamed onto the share; it extracts back to the game.
    let pkg = at("PPSA99999.pkg");
    let t = std::time::Instant::now();
    let report = run(&jobs, &rx, request(&local_game, Format::Pkg, &pkg)).unwrap();
    eprintln!(
        "folder -> fpkg on the share (full verify): {:?}",
        t.elapsed()
    );
    let on_share = here.0.join("PPSA99999.pkg");
    assert_eq!(report.bytes, std::fs::metadata(&on_share).unwrap().len());
    assert!(std::fs::read(&on_share).unwrap().starts_with(b"\x7fFIH"));
    let back = local.join("pkg-back");
    run(&jobs, &rx, request(&on_share, Format::Folder, &back)).unwrap();
    // The build rewrites param.json and adds a keystone; every other file is the source's.
    let mut got = files(&back);
    assert!(got.remove("sce_sys/keystone").is_some());
    let param = got.remove("sce_sys/param.json").unwrap();
    assert!(String::from_utf8_lossy(&param).contains("PPSA99999"));
    let mut rest = want.clone();
    rest.remove("sce_sys/param.json");
    assert_eq!(got, rest);

    // Extract (from the share) and a transfer (from this device) into share folders.
    let mut req = request(&game, Format::Folder, &at("extract"));
    req.extract = vec!["eboot.bin".into(), "data".into()];
    run(&jobs, &rx, req).unwrap();
    let got = files(&here.0.join("extract"));
    let mut expect: BTreeMap<_, _> = want
        .iter()
        .filter(|(p, _)| p.as_str() == "eboot.bin" || p.starts_with("data/"))
        .map(|(p, b)| (p.clone(), b.clone()))
        .collect();
    assert_eq!(got, expect);
    let mut req = request(Path::new(""), Format::Folder, &at("transfer"));
    req.transfer = vec![local_game.clone(), local_game.join("eboot.bin")];
    run(&jobs, &rx, req).unwrap();
    let got = files(&here.0.join("transfer"));
    expect = want
        .iter()
        .map(|(p, b)| (format!("Stub Game/{p}"), b.clone()))
        .collect();
    expect.insert("eboot.bin".into(), want["eboot.bin"].clone());
    assert_eq!(got, expect);

    // The same again, replacing (the user confirmed it): files over files, the folder merged.
    let re = local.join("re");
    std::fs::create_dir_all(re.join("Stub Game")).unwrap();
    std::fs::write(re.join("Stub Game/eboot.bin"), b"new eboot").unwrap();
    std::fs::write(re.join("Stub Game/extra.bin"), b"extra").unwrap();
    std::fs::write(re.join("eboot.bin"), b"top").unwrap();
    let mut req = request(Path::new(""), Format::Folder, &at("transfer"));
    req.transfer = vec![re.join("Stub Game"), re.join("eboot.bin")];
    let e = run(&jobs, &rx, req.clone()).unwrap_err();
    assert!(e.contains("already exists (never replaced)"), "{e}");
    req.replace = true;
    run(&jobs, &rx, req).unwrap();
    expect.insert("Stub Game/eboot.bin".into(), b"new eboot".to_vec());
    expect.insert("Stub Game/extra.bin".into(), b"extra".to_vec());
    expect.insert("eboot.bin".into(), b"top".to_vec());
    assert_eq!(files(&here.0.join("transfer")), expect);

    // Nothing else left behind: no .part, no probe, no temporary name.
    let mut left: Vec<_> = std::fs::read_dir(&here.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["PPSA99999.pkg", "extract", "stub.exfat", "transfer"]);
    let _ = std::fs::remove_dir_all(&local);
}
