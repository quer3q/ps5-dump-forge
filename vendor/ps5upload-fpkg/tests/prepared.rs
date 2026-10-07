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

/// `build` is `prepare` + `write_package`: the same request gives the same bytes.
#[test]
fn build_and_a_prepared_write_give_the_same_package() {
    let src = TempDir::new("same-src");
    let out = TempDir::new("same-out");
    write_tree(src.path());
    let built = build::build(
        &BuildRequest {
            output_dir: out.path().to_path_buf(),
            ..request(src.path())
        },
        &mut |_| {},
    )
    .unwrap();

    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let path = out.path().join("mine.part");
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
        &mut BuildControl::default(),
    )
    .unwrap();
    assert!(std::fs::read(&built.path).unwrap() == std::fs::read(&path).unwrap());
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

/// ... and what grows with the tree: a thousand empty directories with long names, in a
/// package whose image is stored flat (each directory takes an image block, and a record in
/// the install metadata).
#[test]
fn the_estimate_covers_a_directory_heavy_tree() {
    let src = TempDir::new("dirs-src");
    let out = TempDir::new("dirs-out");
    write_tree(src.path());
    for i in 0..1000 {
        std::fs::create_dir_all(src.path().join(format!("d/{i:0>200}"))).unwrap();
    }
    let mut tree = FolderSource::open(src.path()).unwrap();
    let request = BuildRequest {
        kraken: false,
        ..request(src.path())
    };
    let prepared = build::prepare(&mut tree, &request, &mut BuildControl::default()).unwrap();
    let path = out.path().join("dirs.part");
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
}

/// The manifest serializes for a report.
#[test]
fn the_manifest_serializes() {
    let src = TempDir::new("serde-src");
    write_tree(src.path());
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(
        &mut tree,
        &request(src.path()),
        &mut BuildControl::default(),
    )
    .unwrap();
    let json = serde_json::to_string(prepared.manifest()).unwrap();
    assert!(json.contains(r#""origin":"generated""#), "{json}");
    assert!(json.contains(r#""path":"eboot.bin""#), "{json}");
    let _ = tree.describe();
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

/// The verifier on its own: a set flag stops it, a clear one changes nothing.
#[test]
fn a_verification_can_be_cancelled() {
    use ps5upload_fpkg::verify;
    use std::sync::atomic::{AtomicBool, Ordering};
    let src = TempDir::new("verify-cancel-src");
    let out = TempDir::new("verify-cancel-out");
    write_tree(src.path());
    let built = build::build(
        &BuildRequest {
            output_dir: out.path().to_path_buf(),
            ..request(src.path())
        },
        &mut |_| {},
    )
    .unwrap();
    let flag = AtomicBool::new(false);
    let report = verify::verify_streaming_controlled(
        &built.path,
        DEFAULT_PASSCODE,
        &mut |_, _| {},
        Some(&flag),
    )
    .unwrap();
    assert!(report.ok(), "{report}");
    assert_eq!(
        report.checks.len(),
        verify::verify_streaming(&built.path, DEFAULT_PASSCODE, &mut |_, _| {})
            .unwrap()
            .checks
            .len()
    );
    // Stopped from the progress callback, i.e. halfway through the block sweep.
    let mut stop = |_: u64, _: u64| flag.store(true, Ordering::Relaxed);
    flag.store(false, Ordering::Relaxed);
    let stopped =
        verify::verify_streaming_controlled(&built.path, DEFAULT_PASSCODE, &mut stop, Some(&flag));
    assert!(matches!(stopped, Err(ps5upload_fpkg::Error::Cancelled)));
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
