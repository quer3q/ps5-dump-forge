//! `prepare` and `write_package`: a build from an open source tree into a file the caller owns,
//! and the effective manifest that says what the package carries.

use std::path::{Path, PathBuf};

use ps5upload_fpkg::build::{self, BuildControl, BuildRequest, Origin};
use ps5upload_fpkg::crypto::DEFAULT_PASSCODE;
use ps5upload_fpkg::source::{FolderSource, SourceTree};
use ps5upload_fpkg::{cnt, fih, inner, kraken_image, outer, PkgFile};

const CONTENT_ID: &str = "UP0000-PPSA01234_00-TESTGAME00000000";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fpkg-prepared-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// A small libSceAmpr app: a module that names the library, artwork the container carries, a
/// stale packaging leftover, one empty directory.
fn write_tree(root: &Path) {
    let put = |path: &str, data: Vec<u8>| {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, &data).unwrap();
    };
    let mut eboot: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
    eboot[1000..1014].copy_from_slice(b"libSceAmpr.prx");
    put("eboot.bin", eboot);
    put(
        "data/small.bin",
        (0..100u32).map(|i| (i % 13) as u8).collect(),
    );
    put("data/large.bin", vec![0xAB; 600 * 1024]);
    put(
        "sce_sys/param.json",
        format!(
            "{{\"contentId\":\"{CONTENT_ID}\",\"contentVersion\":\"01.001.000\",\"titleId\":\"PPSA01234\",\"applicationDrmType\":\"free\"}}"
        )
        .into_bytes(),
    );
    put("sce_sys/icon0.png", vec![0x89; 2048]);
    put("sce_sys/pic0.png", vec![0x50; 1024]);
    put("sce_sys/playgo-chunk.dat", vec![1; 64]);
    std::fs::create_dir_all(root.join("data/empty")).unwrap();
}

fn request(source: &Path) -> BuildRequest {
    BuildRequest {
        time: Some((1_700_000_000, 0)),
        ..BuildRequest::new(source, source)
    }
}

/// One file of the built package's outer image.
fn outer_file(pkg: &Path, name: &str) -> Vec<u8> {
    let mut file = PkgFile::open(pkg).unwrap();
    let head = file.read_at(0, fih::HEADER_LEN).unwrap();
    let parsed = fih::parse(&head).unwrap();
    let container = cnt::read(&mut file, parsed.cnt_offset).unwrap();
    let image = outer::open(&mut file, &parsed, &container, DEFAULT_PASSCODE).unwrap();
    let nodes = image.dinodes();
    let uroot = nodes.get(2).unwrap();
    let ino = image
        .dirents(uroot)
        .into_iter()
        .find(|d| d.name == name)
        .unwrap()
        .ino as usize;
    image.file_data(&nodes[ino])
}

/// The inner mount of a Kraken package, decoded through its descriptor.
fn mount_of(pkg: &Path) -> Vec<u8> {
    let naps = outer_file(pkg, "naps_pkg_layout.dat");
    let image = outer_file(pkg, "pfs_image.dat");
    let blocks = kraken_image::describe(&naps).unwrap();
    let size = blocks.last().map(|b| b.logical + b.len).unwrap();
    let mut mount = vec![0u8; size as usize];
    for b in &blocks {
        let bytes = kraken_image::decode_described(&image, b).unwrap();
        mount[b.logical as usize..(b.logical + b.len) as usize].copy_from_slice(&bytes);
    }
    mount
}

#[test]
fn a_prepared_build_writes_into_the_callers_file_and_its_manifest_is_the_image() {
    let src = TempDir::new("src");
    let out = TempDir::new("out");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    let request = request(src.path());
    let prepared = build::prepare(&mut tree, &request, &mut BuildControl::default()).unwrap();
    assert_eq!(prepared.content_id(), CONTENT_ID);
    assert!(prepared.estimated_size() > 600 * 1024);

    let origin = |path: &str| {
        prepared
            .manifest()
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.origin)
    };
    assert_eq!(origin("eboot.bin"), Some(Origin::Source));
    assert_eq!(origin("data/large.bin"), Some(Origin::Source));
    // The DRM type "free" is rewritten to "standard".
    assert_eq!(origin("sce_sys/param.json"), Some(Origin::Rewritten));
    let param = prepared.generated_bytes("sce_sys/param.json").unwrap();
    assert!(std::str::from_utf8(param).unwrap().contains("standard"));
    assert_eq!(origin("ampr_emu.index"), Some(Origin::Generated));
    assert_eq!(origin("sce_sys/keystone"), Some(Origin::Generated));
    assert_eq!(
        prepared.generated_bytes("sce_sys/keystone").unwrap().len(),
        96
    );
    assert!(prepared.generated_bytes("eboot.bin").is_none());
    // The stale leftover is left out, and so is the artwork the container carries instead.
    assert_eq!(origin("sce_sys/playgo-chunk.dat"), None);
    assert_eq!(origin("sce_sys/icon0.png"), None);
    assert_eq!(origin("sce_sys/pic0.png"), None);
    let container_only: Vec<(&str, u64)> = prepared
        .container_only()
        .iter()
        .map(|e| (e.path.as_str(), e.size))
        .collect();
    assert_eq!(
        container_only,
        vec![("sce_sys/icon0.png", 2048), ("sce_sys/pic0.png", 1024)]
    );
    assert!(prepared
        .container_only()
        .iter()
        .all(|e| e.origin == Origin::Source));
    assert_eq!(prepared.empty_dirs(), ["data/empty".to_string()]);
    assert!(prepared.log().iter().any(|l| l.contains("leaving out")));
    let paths: Vec<&str> = prepared
        .manifest()
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "the manifest is sorted by path");

    // The caller names, creates and keeps the file; nothing else appears beside it.
    let path = out.path().join("game.42.part");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let report = build::write_package(
        &prepared,
        &mut tree,
        &mut file,
        &path,
        &mut |_| {},
        &mut BuildControl::default(),
    )
    .unwrap();
    drop(file);
    assert!(report.verify.ok(), "{}", report.verify);
    assert_eq!(report.path, path);
    assert_eq!(report.size, std::fs::metadata(&path).unwrap().len());
    let names: Vec<_> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("game.42.part")]);

    // The image holds exactly the manifest, each file as `read_range` serves it.
    let mount = mount_of(&path);
    let walked = inner::read(&mount, prepared.plan().meta_base).unwrap();
    let mut in_image: Vec<&str> = walked.files.iter().map(|f| f.path.as_str()).collect();
    in_image.sort();
    assert_eq!(in_image, paths);
    for entry in prepared.manifest() {
        let f = walked.files.iter().find(|f| f.path == entry.path).unwrap();
        assert_eq!(f.size, entry.size, "{}", entry.path);
        let expected = prepared
            .read_range(&mut tree, &entry.path, 0, entry.size as usize)
            .unwrap();
        assert_eq!(
            &mount[f.offset as usize..(f.offset + f.size) as usize],
            &expected[..],
            "{}",
            entry.path
        );
    }
    assert!(prepared
        .read_range(&mut tree, "sce_sys/icon0.png", 0, 16)
        .is_err());
}

#[test]
fn write_package_refuses_a_file_that_is_not_empty() {
    let src = TempDir::new("full-src");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let path = src.path().join("taken.part");
    std::fs::write(&path, b"someone else's").unwrap();
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let error = build::write_package(
        &prepared,
        &mut tree,
        &mut file,
        &path,
        &mut |_| {},
        &mut BuildControl::default(),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("not empty"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), b"someone else's");
}

/// A tree whose metadata claims a terabyte-sized artwork file, as a corrupt image can.
struct Lying(FolderSource, Vec<ps5upload_fpkg::source::SourceFile>);

impl SourceTree for Lying {
    fn files(&self) -> &[ps5upload_fpkg::source::SourceFile] {
        &self.1
    }
    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        assert_ne!(
            path, "sce_sys/pic0.png",
            "read whole before its size was checked"
        );
        self.0.read(path)
    }
    fn describe(&self) -> String {
        self.0.describe()
    }
}

/// What is read whole into memory is bounded by its declared size before any of it is read.
#[test]
fn a_huge_container_payload_is_refused_before_it_is_read() {
    let src = TempDir::new("huge-src");
    write_tree(src.path());
    let folder = FolderSource::open(src.path()).unwrap();
    let mut files = folder.files().to_vec();
    files
        .iter_mut()
        .find(|f| f.path == "sce_sys/pic0.png")
        .unwrap()
        .size = 1 << 40;
    let mut tree = Lying(folder, files);
    let error = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("holds in memory"), "{error}");
}

/// The estimate covers what the container carries: here 8 MiB of artwork that does not
/// compress, beside a game of a few kilobytes.
#[test]
fn the_estimate_covers_container_payloads() {
    let src = TempDir::new("estimate-src");
    let out = TempDir::new("estimate-out");
    write_tree(src.path());
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let noise: Vec<u8> = (0..8 << 20)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    std::fs::write(src.path().join("sce_sys/pic0.png"), &noise).unwrap();
    std::fs::write(src.path().join("data/large.bin"), b"small").unwrap();
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let path = out.path().join("estimate.part");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let report = build::write_package(
        &prepared,
        &mut tree,
        &mut file,
        &path,
        &mut |_| {},
        &mut BuildControl::default(),
    )
    .unwrap();
    assert!(
        prepared.estimated_size() >= report.size,
        "estimated {} for a package of {}",
        prepared.estimated_size(),
        report.size
    );
    assert!(report.size > 8 << 20);
}

/// A dump's own `ampr_emu.index` lists the folder as it was; the package carries one generated
/// from what it packages, so the manifest says Generated, not Source.
#[test]
fn a_dumps_own_ampr_index_is_generated_anew() {
    let src = TempDir::new("index-src");
    write_tree(src.path());
    std::fs::write(src.path().join("ampr_emu.index"), b"stale").unwrap();
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let entry = prepared
        .manifest()
        .iter()
        .find(|e| e.path == "ampr_emu.index")
        .unwrap();
    assert_eq!(entry.origin, Origin::Generated);
    let bytes = prepared.generated_bytes("ampr_emu.index").unwrap();
    assert_ne!(bytes, b"stale");
    assert_eq!(bytes.len() as u64, entry.size);
    assert!(prepared.log().iter().any(|l| l.contains("regenerating")));
}

/// A caller that already scanned `eboot.bin` (`readiness`) passes the answer on, and `prepare`
/// takes it instead of scanning again: told "no", an AMPR module gets no generated index.
#[test]
fn a_known_ampr_answer_is_taken_instead_of_a_scan() {
    let src = TempDir::new("ampr-known");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    assert!(ps5upload_fpkg::source::readiness(&mut tree).imports_ampr);
    let has_index = |imports_ampr: Option<bool>, tree: &mut FolderSource| {
        let request = BuildRequest {
            imports_ampr,
            ..request(src.path())
        };
        let prepared = build::prepare(tree, &request, &mut BuildControl::default()).unwrap();
        prepared
            .manifest()
            .iter()
            .any(|e| e.path == "ampr_emu.index")
    };
    assert!(has_index(None, &mut tree));
    assert!(has_index(Some(true), &mut tree));
    assert!(!has_index(Some(false), &mut tree));
}

/// The per-file passes of the check and plan stages are counted per file visited, those they
/// skip without a read included: from 0 to every file, a call per 256 files rather than one
/// per file, and no final count once stopped.
#[test]
fn the_check_and_plan_files_are_counted() {
    use build::Stage;
    use std::sync::atomic::{AtomicBool, Ordering};
    let src = TempDir::new("files-counted");
    write_tree(src.path());
    // Too short for a SELF header (check) and, half of them, for a module mark (plan).
    for i in 0..600 {
        let path = src.path().join(format!("data/many/{i:03}.bin"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![i as u8; if i % 2 == 0 { 2 } else { 8 }]).unwrap();
    }
    let calls = |seen: &[(Stage, u64, u64)], stage: Stage| -> Vec<(u64, u64)> {
        seen.iter()
            .filter(|(s, _, _)| *s == stage)
            .map(|&(_, done, total)| (done, total))
            .collect()
    };
    let mut tree = FolderSource::open(src.path()).unwrap();
    let mut seen: Vec<(Stage, u64, u64)> = Vec::new();
    let mut on_files = |stage: Stage, done: u64, total: u64| seen.push((stage, done, total));
    let mut control = BuildControl {
        files: Some(&mut on_files),
        ..BuildControl::default()
    };
    build::prepare(&mut tree, &request(src.path()), &mut control).unwrap();
    for stage in [Stage::Check, Stage::Plan] {
        let calls = calls(&seen, stage);
        let total = calls[0].1;
        assert!(total > 600, "{stage:?}: {calls:?}");
        let want: Vec<(u64, u64)> = (0..total)
            .step_by(256)
            .chain([total])
            .map(|done| (done, total))
            .collect();
        assert_eq!(calls, want, "{stage:?}");
    }
    let last_check = seen.iter().rposition(|(s, _, _)| *s == Stage::Check);
    let first_plan = seen.iter().position(|(s, _, _)| *s == Stage::Plan);
    assert!(last_check < first_plan);

    // Stopped during plan: the pass ends where it stopped, without its final count.
    let cancel = AtomicBool::new(false);
    let mut seen: Vec<(Stage, u64, u64)> = Vec::new();
    let mut on_files = |stage: Stage, done: u64, total: u64| {
        seen.push((stage, done, total));
        if stage == Stage::Plan && done == 256 {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    let mut control = BuildControl {
        files: Some(&mut on_files),
        cancel: Some(&cancel),
        ..BuildControl::default()
    };
    let result = build::prepare(&mut tree, &request(src.path()), &mut control);
    assert!(matches!(result, Err(ps5upload_fpkg::Error::Cancelled)));
    let plan = calls(&seen, Stage::Plan);
    assert_eq!(
        plan.iter().map(|c| c.0).collect::<Vec<_>>(),
        [0, 256],
        "{plan:?}"
    );
}

/// An LZ4-packed folder's own index also lists the files inside its packs, and ampr_emu checks
/// each record it opens against the pack manifest: the package carries it byte for byte. The
/// same folder without packs gets one generated anew; a packed one whose index is missing or
/// unreadable is refused, and readiness says so.
#[test]
fn a_packed_sources_own_index_is_packaged_as_is() {
    use ps5upload_fpkg::ampr_index::{self, Entry};
    let src = TempDir::new("packed-index-src");
    write_tree(src.path());
    let mut manifest = b"AMPRPAK4".to_vec();
    manifest.resize(64, 0);
    std::fs::write(src.path().join("ampr_assets.index"), &manifest).unwrap();
    std::fs::write(src.path().join("ampr_assets-000.pak"), [0u8; 128]).unwrap();
    // The folder as the packer saw it (pic0 included, which a package leaves out), plus the
    // packed files, which are not files of the folder.
    let records: Vec<Entry> = [
        ("eboot.bin", 4096),
        ("sce_sys/param.json", 120),
        ("sce_sys/pic0.png", 1024),
        ("data/packed/a.bin", 5_000_000),
        ("data/packed/b.bin", 77),
    ]
    .map(|(path, size)| Entry {
        path: path.into(),
        size,
        mtime: 42,
    })
    .to_vec();
    let own = ampr_index::build_entries(&records).unwrap();
    std::fs::write(src.path().join("ampr_emu.index"), &own).unwrap();
    let prepare = |src: &Path| {
        let mut tree = FolderSource::open(src).unwrap();
        build::prepare(&mut tree, &request(src), &mut BuildControl::default())
    };

    let prepared = prepare(src.path()).unwrap();
    let entry = prepared
        .manifest()
        .iter()
        .find(|e| e.path == "ampr_emu.index")
        .unwrap();
    assert_eq!(
        (entry.origin, entry.size),
        (Origin::Source, own.len() as u64)
    );
    assert!(prepared.generated_bytes("ampr_emu.index").is_none());
    assert!(prepared.log().iter().any(|l| l.contains("as it is")));
    let out = TempDir::new("packed-index-out");
    let report = build::build(
        &BuildRequest {
            output_dir: out.path().to_path_buf(),
            ..request(src.path())
        },
        &mut |_| {},
    )
    .unwrap();
    let mount = mount_of(&report.path);
    let walked = inner::read(&mount, prepared.plan().meta_base).unwrap();
    let f = walked
        .files
        .iter()
        .find(|f| f.path == "ampr_emu.index")
        .unwrap();
    assert_eq!(
        &mount[f.offset as usize..(f.offset + f.size) as usize],
        &own[..]
    );
    let mut tree = FolderSource::open(src.path()).unwrap();
    let readiness = ps5upload_fpkg::source::readiness(&mut tree);
    let check = |r: &ps5upload_fpkg::source::Readiness| {
        let c = r
            .checks
            .iter()
            .find(|c| c.name.starts_with("ampr_emu.index"))
            .unwrap();
        (c.ok, c.detail.clone())
    };
    let (ok, detail) = check(&readiness);
    assert!(ok && detail.contains("as it is"), "{detail}");

    // Not packed: generated anew from what the package holds.
    std::fs::write(src.path().join("ampr_assets.index"), b"NOTAPACK").unwrap();
    let prepared = prepare(src.path()).unwrap();
    assert_ne!(
        prepared.generated_bytes("ampr_emu.index").unwrap(),
        &own[..]
    );

    // Packed, with no index or an unreadable one: refused.
    std::fs::write(src.path().join("ampr_assets.index"), &manifest).unwrap();
    for own in [None, Some(&b"stale"[..])] {
        match own {
            None => std::fs::remove_file(src.path().join("ampr_emu.index")).unwrap(),
            Some(bytes) => std::fs::write(src.path().join("ampr_emu.index"), bytes).unwrap(),
        }
        let err = prepare(src.path()).err().unwrap();
        assert!(
            err.to_string().contains("no readable ampr_emu.index"),
            "{err}"
        );
        let mut tree = FolderSource::open(src.path()).unwrap();
        let (ok, detail) = check(&ps5upload_fpkg::source::readiness(&mut tree));
        assert!(
            !ok && detail.contains("no readable ampr_emu.index"),
            "{detail}"
        );
    }
}

/// A zstd-packed folder (a root `ampr_zstd.index` starting `AMPRZPK1`) keeps its own index too:
/// it lists the files inside the packs, which the package does not hold as files.
#[test]
fn a_zstd_packed_sources_own_index_is_packaged_as_is() {
    use ps5upload_fpkg::ampr_index::{self, Entry};
    let src = TempDir::new("zstd-index-src");
    write_tree(src.path());
    let mut index = b"AMPRZPK1".to_vec();
    index.resize(128, 0);
    std::fs::write(src.path().join("ampr_zstd.index"), &index).unwrap();
    std::fs::write(src.path().join("ampr_zstd-000.pak"), [0u8; 64]).unwrap();
    let records: Vec<Entry> = [("eboot.bin", 4096), ("data/packed/a.bin", 5_000_000)]
        .map(|(path, size)| Entry {
            path: path.into(),
            size,
            mtime: 42,
        })
        .to_vec();
    let own = ampr_index::build_entries(&records).unwrap();
    std::fs::write(src.path().join("ampr_emu.index"), &own).unwrap();
    let mut tree = FolderSource::open(src.path()).unwrap();
    assert!(ps5upload_fpkg::source::lz4_packed(&mut tree).unwrap());
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let entry = prepared
        .manifest()
        .iter()
        .find(|e| e.path == "ampr_emu.index")
        .unwrap();
    assert_eq!(
        (entry.origin, entry.size),
        (Origin::Source, own.len() as u64)
    );
    assert!(prepared.generated_bytes("ampr_emu.index").is_none());

    // Not the zstd pack magic: generated anew.
    std::fs::write(src.path().join("ampr_zstd.index"), [0u8; 128]).unwrap();
    let mut tree = FolderSource::open(src.path()).unwrap();
    assert!(!ps5upload_fpkg::source::lz4_packed(&mut tree).unwrap());
}

/// A listed `ampr_assets.index` whose first bytes cannot be read is an error, not "not packed":
/// that answer would regenerate the index and drop the records of the packed files.
#[test]
fn an_unreadable_pack_manifest_fails_the_build() {
    let src = TempDir::new("packed-unreadable-src");
    write_tree(src.path());
    let mut manifest = b"AMPRPAK4".to_vec();
    manifest.resize(64, 0);
    std::fs::write(src.path().join("ampr_assets.index"), &manifest).unwrap();
    let mut tree = FolderSource::open(src.path()).unwrap();
    std::fs::remove_file(src.path().join("ampr_assets.index")).unwrap();
    let err = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(err, ps5upload_fpkg::Error::Io(_)), "{err}");
    let readiness = ps5upload_fpkg::source::readiness(&mut tree);
    let check = readiness
        .checks
        .iter()
        .find(|c| c.name.starts_with("ampr_emu.index"))
        .unwrap();
    assert!(
        !check.ok && check.detail.contains("cannot be read"),
        "{}",
        check.detail
    );
}

/// A thread cap changes how long compression takes, never what it produces.
#[test]
fn a_thread_cap_builds_the_same_package() {
    let src = TempDir::new("threads-src");
    write_tree(src.path());
    let built = |threads: Option<usize>, name: &str| {
        let out = TempDir::new(name);
        let report = build::build(
            &BuildRequest {
                output_dir: out.path().to_path_buf(),
                threads,
                ..request(src.path())
            },
            &mut |_| {},
        )
        .unwrap();
        std::fs::read(report.path).unwrap()
    };
    assert!(built(Some(1), "threads-one") == built(None, "threads-all"));
}

/// A stop asked for at a stage's start ends the build with `Error::Cancelled`, at every
/// stage: preparing, compressing, writing and verifying.
#[test]
fn every_stage_stops_when_cancelled() {
    use build::Stage;
    use std::sync::atomic::{AtomicBool, Ordering};
    let src = TempDir::new("cancel-src");
    write_tree(src.path());
    for stop_at in [
        Stage::Check,
        Stage::Plan,
        Stage::Compress,
        Stage::Write,
        Stage::Verify,
    ] {
        let mut tree = FolderSource::open(src.path()).unwrap();
        let flag = AtomicBool::new(false);
        let mut on_stage = |s: Stage| {
            if s == stop_at {
                flag.store(true, Ordering::Relaxed);
            }
        };
        let mut control = BuildControl {
            cancel: Some(&flag),
            stage: Some(&mut on_stage),
            ..BuildControl::default()
        };
        let result =
            build::prepare(&mut tree, &request(src.path()), &mut control).and_then(|prepared| {
                let path = src.path().join(format!("{}.part", stop_at.id()));
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                build::write_package(
                    &prepared,
                    &mut tree,
                    &mut file,
                    &path,
                    &mut |_| {},
                    &mut control,
                )
            });
        match result {
            Err(ps5upload_fpkg::Error::Cancelled) => {}
            Err(e) => panic!("stopping at {stop_at:?}: {e}"),
            Ok(_) => panic!("stopping at {stop_at:?} still built a package"),
        }
    }
}

/// The verification reads the package through the caller's handle: what is checked is what
/// was written, whatever the path names by then (here, nothing at all).
#[test]
fn the_written_handle_is_what_gets_verified() {
    let src = TempDir::new("handle-src");
    let out = TempDir::new("handle-out");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let path = out.path().join("handle.part");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let moved = out.path().join("elsewhere.part");
    std::fs::rename(&path, &moved).unwrap();
    let report = build::write_package(
        &prepared,
        &mut tree,
        &mut file,
        &path,
        &mut |_| {},
        &mut BuildControl::default(),
    )
    .unwrap();
    assert!(report.verify.ok(), "{}", report.verify);
    assert_eq!(report.path, path);
    assert_eq!(report.size, std::fs::metadata(&moved).unwrap().len());
}

#[test]
fn a_sampled_self_check_runs_through_write_package() {
    let src = TempDir::new("sampled-src");
    let out = TempDir::new("sampled-out");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let path = out.path().join("sampled.part");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let report = build::write_package(
        &prepared,
        &mut tree,
        &mut file,
        &path,
        &mut |_| {},
        &mut BuildControl {
            sample: Some(7),
            ..BuildControl::default()
        },
    )
    .unwrap();
    assert!(report.verify.ok(), "{}", report.verify);
    let text = report.verify.to_string();
    assert!(
        text.contains("(a sample of") && text.contains("seed 7)"),
        "{text}"
    );
    assert!(text.contains("batches sampled"), "{text}");
}

/// An output that is not a file (Forge writes onto another device): clones share one buffer,
/// each with a position of its own. The package is the one a file gets, byte for byte, and its
/// self-check (and the in-place Kraken spool) reads it back through clones. A plaintext Kraken
/// build digests its blocks as it compresses them, so it reads none of its own output before
/// the self-check; an encrypted one still reads its image back to transform it.
#[test]
fn a_package_is_written_into_any_pkg_out() {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    struct Mem {
        data: Arc<Mutex<Vec<u8>>>,
        pos: u64,
        /// Set once the self-check starts; bytes read before that are counted in `early`.
        verifying: Arc<AtomicBool>,
        early: Arc<AtomicU64>,
    }
    impl Read for Mem {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let data = self.data.lock().unwrap();
            let at = (self.pos as usize).min(data.len());
            let n = buf.len().min(data.len() - at);
            buf[..n].copy_from_slice(&data[at..at + n]);
            self.pos += n as u64;
            if !self.verifying.load(Ordering::Relaxed) {
                self.early.fetch_add(n as u64, Ordering::Relaxed);
            }
            Ok(n)
        }
    }
    impl Write for Mem {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut data = self.data.lock().unwrap();
            let end = self.pos as usize + buf.len();
            if data.len() < end {
                data.resize(end, 0);
            }
            data[self.pos as usize..end].copy_from_slice(buf);
            self.pos = end as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Seek for Mem {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            let len = self.data.lock().unwrap().len() as i64;
            self.pos = match to {
                SeekFrom::Start(p) => p as i64,
                SeekFrom::End(d) => len + d,
                SeekFrom::Current(d) => self.pos as i64 + d,
            } as u64;
            Ok(self.pos)
        }
    }
    impl ps5upload_fpkg::PkgOut for Mem {
        fn try_clone_out(&self) -> std::io::Result<Box<dyn ps5upload_fpkg::PkgOut>> {
            Ok(Box::new(Mem {
                data: self.data.clone(),
                pos: 0,
                verifying: self.verifying.clone(),
                early: self.early.clone(),
            }))
        }
        fn out_len(&self) -> std::io::Result<u64> {
            Ok(self.data.lock().unwrap().len() as u64)
        }
        fn sync_out(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let src = TempDir::new("pkgout-src");
    write_tree(src.path());
    for (mode, name) in [
        (ps5upload_fpkg::ImageMode::PlaintextNoAuth, "plain"),
        (ps5upload_fpkg::ImageMode::Native, "native"),
    ] {
        let out = TempDir::new(&format!("pkgout-out-{name}"));
        let mut tree = FolderSource::open(src.path()).unwrap();
        let prepared = build::prepare(
            &mut tree,
            &BuildRequest {
                image_mode: mode,
                kraken: true,
                env_overrides: false,
                ..request(src.path())
            },
            &mut BuildControl::default(),
        )
        .unwrap();
        let path = out.path().join("file.part");
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let on_file = build::write_package(
            &prepared,
            &mut tree,
            &mut file,
            &path,
            &mut |_| {},
            &mut BuildControl::default(),
        )
        .unwrap();
        let mut mem = Mem {
            data: Arc::new(Mutex::new(Vec::new())),
            pos: 0,
            verifying: Arc::new(AtomicBool::new(false)),
            early: Arc::new(AtomicU64::new(0)),
        };
        let verifying = mem.verifying.clone();
        let mut on_stage = |s: build::Stage| {
            if s == build::Stage::Verify {
                verifying.store(true, Ordering::Relaxed);
            }
        };
        let in_mem = build::write_package(
            &prepared,
            &mut tree,
            &mut mem,
            Path::new("mem.part"),
            &mut |_| {},
            &mut BuildControl {
                stage: Some(&mut on_stage),
                ..BuildControl::default()
            },
        )
        .unwrap();
        assert!(in_mem.verify.ok(), "{name}: {}", in_mem.verify);
        assert!(mem.verifying.load(Ordering::Relaxed), "{name}");
        assert_eq!(in_mem.size, on_file.size, "{name}");
        assert!(
            *mem.data.lock().unwrap() == std::fs::read(&path).unwrap(),
            "{name}: the packages differ"
        );
        let early = mem.early.load(Ordering::Relaxed);
        match mode {
            ps5upload_fpkg::ImageMode::PlaintextNoAuth => {
                assert_eq!(early, 0, "a plaintext build read its own output back")
            }
            ps5upload_fpkg::ImageMode::Native => {
                assert!(early > 0, "an encrypted build reads its image back")
            }
        }
    }
}
