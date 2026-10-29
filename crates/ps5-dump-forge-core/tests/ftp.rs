//! Outputs on an FTP server (the ftp crate's in-process one): an fpkg goes back over what it
//! wrote, which the server's rewrite probe allows.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;

use ps5_dump_forge_core::{ConvertRequest, Event, Format, JobReport, Jobs, KrakenLevel};
use ps5_dump_forge_ftp::fake;

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn run(req: ConvertRequest) -> Result<JobReport, String> {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let jobs = Jobs::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    jobs.start(req);
    loop {
        if let Event::Done { result, .. } = rx.recv().unwrap() {
            return result;
        }
    }
}

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
        archive_password: None,
    }
}

#[test]
fn fpkg_onto_an_ftp_server_reads_back_as_the_game() {
    let root = dir("ftp-pkg");
    let src = root.join("src");
    write(&src, "eboot.bin", b"\x7fELF fake eboot");
    write(
        &src,
        "sce_sys/param.json",
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
    );
    let big: Vec<u8> = (0..3 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
    write(&src, "data/big.bin", &big);

    let server = fake::start();
    let pkg = PathBuf::from(format!("ftp://127.0.0.1:{}/PPSA01234.pkg", server.port));
    let report = run(request(&src, Format::Pkg, &pkg)).unwrap();
    let on_server = server.root.join("PPSA01234.pkg");
    assert_eq!(report.bytes, std::fs::metadata(&on_server).unwrap().len());
    assert!(std::fs::read(&on_server).unwrap().starts_with(b"\x7fFIH"));

    // The package converts back to the game.
    let back = root.join("back");
    run(request(&on_server, Format::Folder, &back)).unwrap();
    assert_eq!(std::fs::read(back.join("data/big.bin")).unwrap(), big);
    assert_eq!(
        std::fs::read_dir(&server.root).unwrap().count(),
        1,
        "no probe or part left behind"
    );
}

#[test]
fn a_session_the_server_closed_is_replaced_on_the_next_call() {
    let server = fake::start();
    write(&server.root, "game/eboot.bin", b"x");
    let at = PathBuf::from(format!("ftp://127.0.0.1:{}/game", server.port));
    assert_eq!(ps5_dump_forge_core::list_dir(&at).unwrap().entries.len(), 1);
    // The cached session dies as an idle one does; the call logs in again instead of failing.
    server.kick();
    assert_eq!(ps5_dump_forge_core::list_dir(&at).unwrap().entries.len(), 1);
}

/// A transfer onto an FTP server the user confirmed replacing: a file over a file, the folder
/// merged (what else is there kept); without `replace` refused as ever.
#[test]
fn a_transfer_onto_an_ftp_server_replaces_and_merges_when_asked() {
    let root = dir("ftp-replace");
    let src = root.join("src");
    write(&src, "f.bin", b"one file");
    write(&src, "g/zero.bin", b"");
    write(&src, "g/data/new.bin", b"new");
    let server = fake::start();
    write(&server.root, "out/f.bin", b"old");
    write(&server.root, "out/g/zero.bin", b"old zero");
    write(&server.root, "out/g/old.bin", b"keep");
    let out = PathBuf::from(format!("ftp://127.0.0.1:{}/out", server.port));
    let mut req = request(&src, Format::Folder, &out);
    req.transfer = vec![src.join("g"), src.join("f.bin")];

    let jobs = Jobs::new(|_| {});
    assert_eq!(
        jobs.existing_targets(&req).unwrap(),
        [out.join("g"), out.join("f.bin")]
    );
    let e = run(req.clone()).unwrap_err();
    assert!(e.contains("already exists (never replaced)"), "{e}");
    assert_eq!(
        std::fs::read(server.root.join("out/f.bin")).unwrap(),
        b"old"
    );

    req.replace = true;
    let report = run(req).unwrap();
    assert_eq!(report.files, 3);
    let at = |rel: &str| std::fs::read(server.root.join("out").join(rel)).unwrap();
    assert_eq!(at("f.bin"), b"one file");
    assert_eq!(at("g/zero.bin"), b"");
    assert_eq!(at("g/old.bin"), b"keep");
    assert_eq!(at("g/data/new.bin"), b"new");
    let left: Vec<_> = std::fs::read_dir(server.root.join("out/g"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(left.iter().all(|n| !n.ends_with(".tmp")), "{left:?}");
    let _ = std::fs::remove_dir_all(&root);
}
