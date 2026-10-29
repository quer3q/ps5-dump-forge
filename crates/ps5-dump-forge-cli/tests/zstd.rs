//! End to end: `convert <folder> --to folder --zstd-pack` through the real binary, then the
//! output read back through `ps5-dump-forge-zstd`'s reader.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use ps5_dump_forge_core::ScannedFolder;
use ps5_dump_forge_lz4::runtime::ZSTD;
use ps5_dump_forge_zstd::{CHUNK_SIZE, Codec};
use ps5upload_fpkg::source::SourceTree;

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Deterministic incompressible bytes (xorshift64).
fn noise(len: usize, mut x: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// A small AMPR title: path -> bytes.
fn fixture(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let text: Vec<u8> = (0..700_000)
        .map(|i| b"compressible level data "[i % 24])
        .collect();
    let files: BTreeMap<String, Vec<u8>> = [
        (
            "eboot.bin",
            b"\x7fELF fake eboot importing libSceAmpr.sprx".to_vec(),
        ),
        (
            "sce_sys/param.json",
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#
                .to_vec(),
        ),
        ("data/level1.bin", text),
        ("data/noise.bin", noise(300_000, 7)),
        ("data/empty.bin", Vec::new()),
        (
            "readme.txt",
            b"a root file: LZ4's guess keeps it loose".to_vec(),
        ),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_string(), b))
    .collect();
    for (path, bytes) in &files {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    files
}

#[test]
fn zstd_pack_to_a_folder_reads_back() {
    let root = dir("zstd-pack");
    let src = root.join("src");
    let out = root.join("out");
    let files = fixture(&src);
    let run = Command::new(env!("CARGO_BIN_EXE_ps5-dump-forge"))
        .args(["convert".as_ref(), src.as_os_str()])
        .args(["--to", "folder", "--zstd-pack", "--zstd-level", "3", "-o"])
        .arg(&out)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");

    // JSON-lines events: the stages, `pack` progress, the packer's log line.
    let events: Vec<serde_json::Value> = String::from_utf8(run.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let stages = events.iter().find(|e| e["kind"] == "stages").unwrap();
    assert_eq!(
        stages["stages"],
        serde_json::json!(["scan", "preflight", "write", "pack", "verify", "finalize"])
    );
    assert!(
        events
            .iter()
            .any(|e| e["kind"] == "progress" && e["stage"] == "pack"),
        "{events:?}"
    );
    let logged = |want: &str| {
        events
            .iter()
            .any(|e| e["kind"] == "log" && e["line"].as_str().unwrap().contains(want))
    };
    assert!(
        logged("wrote zstd packs: 2 files packed into 1 volumes"),
        "{events:?}"
    );
    assert!(
        stderr.contains("ok: zstd packs: ampr_zstd.index (2 packed files)"),
        "{stderr}"
    );

    // Loose, as LZ4's built-in guess keeps them: eboot, sce_sys, root files, empty files, the
    // runtime (ZSTD bytes), the indexes and the volume.
    for loose in [
        "eboot.bin",
        "sce_sys/param.json",
        "readme.txt",
        "data/empty.bin",
        "ampr_zstd.index",
        "ampr_zstd-000.pak",
    ] {
        assert!(out.join(loose).is_file(), "{loose}");
    }
    assert!(std::fs::read(out.join("fakelib/libSceAmpr.sprx")).unwrap() == ZSTD);
    let packed = ["data/level1.bin", "data/noise.bin"];
    for path in packed {
        assert!(!out.join(path).exists(), "{path} is packed");
    }

    // Every packed file, read back through the pak reader, is the source's.
    let scanned = ScannedFolder::scan(&out, &AtomicBool::new(false)).unwrap();
    let mut tree = ps5_dump_forge_zstd::reader::unpack(Box::new(scanned)).unwrap();
    let index = tree.index().clone();
    let mut in_index: Vec<&str> = index.files.iter().map(|f| f.path.as_str()).collect();
    in_index.sort_unstable();
    let mut want = packed.to_vec();
    want.sort_unstable();
    assert_eq!(in_index, want);
    for path in packed {
        assert!(tree.read(path).unwrap() == files[path], "{path}");
    }
    // A ranged read across a chunk boundary.
    let at = CHUNK_SIZE - 10;
    assert!(
        tree.read_range("data/level1.bin", at, 20).unwrap()
            == files["data/level1.bin"][at as usize..at as usize + 20]
    );
    let record = |path: &str| index.files.iter().find(|f| f.path == path).unwrap();
    let level1 = record("data/level1.bin");
    assert!(!level1.store_only);
    let first = level1.first_chunk as usize;
    assert_eq!(
        index.chunks[first..first + 3]
            .iter()
            .filter(|c| c.codec == Codec::Zstd)
            .count(),
        3,
        "700 KB of text is three zstd chunks"
    );
    assert!(record("data/noise.bin").store_only);

    // ampr_emu.index lists every game file, loose and packed, and the runtime.
    let bytes = std::fs::read(out.join("ampr_emu.index")).unwrap();
    let mut listed = ps5_dump_forge_lz4::index::read_index(&bytes)
        .unwrap()
        .records;
    listed.sort();
    let mut expected: Vec<(String, u64)> = files
        .iter()
        .map(|(p, b)| (p.clone(), b.len() as u64))
        .chain([("fakelib/libSceAmpr.sprx".to_string(), ZSTD.len() as u64)])
        .collect();
    expected.sort();
    assert_eq!(listed, expected);
}
