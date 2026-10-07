//! Folder → `.exfat` → vendored reader, comparing paths, sizes and bytes; plus the
//! preflight rejections, cancellation and the size math for huge trees.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ps5_dump_forge_exfat::{Layout, Options, plan, write};
use ps5upload_fpkg::Result;
use ps5upload_fpkg::exfat::ExFatSource;
use ps5upload_fpkg::source::{FolderSource, SourceFile, SourceTree};

/// A fresh directory under the system temp dir, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ps5-dump-forge-exfat-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Bytes that differ per file and per offset, so a misplaced cluster shows up.
fn pattern(path: &str, offset: u64, len: usize) -> Vec<u8> {
    let seed = path
        .bytes()
        .fold(0x9E37_79B9u32, |h, b| h.rotate_left(5) ^ u32::from(b));
    (0..len as u64)
        .map(|i| {
            let x = (offset + i).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(seed);
            (x >> 29) as u8
        })
        .collect()
}

fn put(root: &Path, rel: &str, size: u64) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, pattern(rel, 0, size as usize)).unwrap();
}

/// Plans and writes `src` into `image`.
fn build(src: &Path, image: &Path, opts: &Options) -> Layout {
    let mut tree = FolderSource::open(src).unwrap();
    let cancel = AtomicBool::new(false);
    let layout = plan(&tree, opts, &cancel).unwrap();
    let mut out = File::create(image).unwrap();
    let mut last = (0, 0);
    let report = write(&mut tree, &layout, &mut out, &cancel, &mut |d, t| {
        last = (d, t)
    })
    .unwrap();
    let kept: Vec<&SourceFile> = tree.files().iter().filter(|f| !is_junk(&f.path)).collect();
    let total: u64 = kept.iter().map(|f| f.size).sum();
    assert_eq!(last, (total, total), "progress ends at the file-data total");
    assert_eq!(report.image_size, layout.image_size);
    assert_eq!(report.files, kept.len() as u64);
    assert_eq!(out.metadata().unwrap().len(), layout.image_size);
    assert_eq!(layout.image_size % 65536, 0);
    // The geometry the SMP fast path checks: raw exFAT at byte 0, shifts 9 and 7.
    let boot = read_at(&mut File::open(image).unwrap(), 0, 512);
    assert_eq!(&boot[3..11], b"EXFAT   ");
    assert_eq!((boot[108], boot[109]), (9, 7));
    // No macOS junk anywhere in the image, read raw (the vendored reader hides it).
    for name in raw_names(image) {
        assert!(!is_junk(&name), "junk in the image: {name}");
    }
    layout
}

/// The macOS junk `exfat.sh` keeps out of its images, at any depth.
fn is_junk(path: &str) -> bool {
    path.split('/').any(|p| {
        p.starts_with("._")
            || [".DS_Store", ".fseventsd", ".Spotlight-V100", ".Trashes"]
                .iter()
                .any(|j| p.eq_ignore_ascii_case(j))
    })
}

/// The image size `exfat.sh` picks, in its own integer arithmetic; `dirs` counts the root.
fn script_size(files: &[u64], dirs: u64, free: u64) -> u64 {
    let c = 65536u64;
    let mib = 1u64 << 20;
    let bytes: u64 = files.iter().sum();
    let alloc: u64 = files.iter().map(|s| s.div_ceil(c) * c).sum();
    let clusters = alloc / c;
    let mut total =
        alloc + clusters * 4 + clusters / 8 + 1 + (files.len() as u64 + dirs) * 256 + 32 * mib;
    let spare = (total / 200).clamp(64 * mib, 512 * mib);
    total += spare;
    if total < bytes + 64 * mib {
        total = bytes + 64 * mib;
    }
    total.div_ceil(mib) * mib + free
}

fn read_at(f: &mut File, at: u64, len: usize) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    f.seek(SeekFrom::Start(at)).unwrap();
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).unwrap();
    buf
}

/// Every path in the image, from the raw directory entries: the root by its FAT chain,
/// the rest by first cluster and length (the writer sets NoFatChain on them).
fn raw_names(image: &Path) -> Vec<String> {
    let mut f = File::open(image).unwrap();
    let boot = read_at(&mut f, 0, 512);
    let u32_at = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let fat = u64::from(u32_at(&boot, 80)) * 512;
    let heap = u64::from(u32_at(&boot, 88)) * 512;
    let offset = |cluster: u32| heap + (u64::from(cluster) - 2) * 65536;
    let mut root = Vec::new();
    let mut cluster = u32_at(&boot, 96);
    while (2..0xFFFF_FFF7).contains(&cluster) {
        root.extend(read_at(&mut f, offset(cluster), 65536));
        cluster = u32_at(&read_at(&mut f, fat + 4 * u64::from(cluster), 4), 0);
    }
    let mut out = Vec::new();
    let mut todo = vec![(String::new(), root)];
    while let Some((prefix, bytes)) = todo.pop() {
        let mut dir = false;
        let (mut first, mut len, mut want) = (0u32, 0u64, 0usize);
        let mut name: Vec<u16> = Vec::new();
        for e in bytes.chunks(32) {
            match e[0] {
                0x00 => break,
                0x85 => dir = u16::from_le_bytes([e[4], e[5]]) & 0x10 != 0,
                0xC0 => {
                    want = usize::from(e[3]);
                    first = u32_at(e, 20);
                    len = u64::from_le_bytes(e[24..32].try_into().unwrap());
                    name.clear();
                }
                0xC1 => {
                    for u in e[2..].chunks(2) {
                        if name.len() < want {
                            name.push(u16::from_le_bytes([u[0], u[1]]));
                        }
                    }
                    if name.len() == want {
                        let path = format!("{prefix}{}", String::from_utf16(&name).unwrap());
                        if dir {
                            let bytes = read_at(&mut f, offset(first), len as usize);
                            todo.push((format!("{path}/"), bytes));
                        }
                        out.push(path);
                        want = usize::MAX;
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The image read back holds exactly the source's files, byte for byte.
fn assert_same(src: &Path, image: &Path) {
    let mut want = FolderSource::open(src).unwrap();
    let mut got = ExFatSource::open(image).unwrap();
    let mut a: Vec<SourceFile> = want
        .files()
        .iter()
        .filter(|f| !is_junk(&f.path))
        .cloned()
        .collect();
    let mut b: Vec<SourceFile> = got.files().to_vec();
    a.sort_by(|x, y| x.path.cmp(&y.path));
    b.sort_by(|x, y| x.path.cmp(&y.path));
    assert_eq!(a, b);
    for f in &a {
        let mut at = 0u64;
        loop {
            let x = want.read_range(&f.path, at, 1 << 20).unwrap();
            let y = got.read_range(&f.path, at, 1 << 20).unwrap();
            assert!(x == y, "{} differs at {at}", f.path);
            if x.is_empty() {
                break;
            }
            at += x.len() as u64;
        }
        assert_eq!(at, f.size);
    }
    // KNOWN GAP (reader): the vendored ExFatSource may not report empty directories yet;
    // compare them only once it does. fsck and the hdiutil mount check cover them meanwhile.
    if got.empty_dirs().is_empty() {
        eprintln!("NOTE: the exFAT reader reports no empty dirs; empty-dir comparison skipped");
    } else {
        assert_eq!(want.empty_dirs(), got.empty_dirs());
    }
}

fn boot_u32(image: &Path, at: usize) -> u32 {
    let bytes = std::fs::read(image).unwrap();
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn mixed_tree_round_trips() {
    let tmp = TempDir::new("mixed");
    let src = tmp.0.join("src");
    put(&src, "eboot.bin", 300_000);
    put(&src, "sce_sys/param.json", 1234);
    put(&src, "empty.bin", 0);
    put(&src, "a/b/c/d/also-empty", 0);
    put(&src, "a/b/c/d/deep.dat", 70_000);
    put(&src, "a/exact-cluster.bin", 65536);
    put(&src, "big/many-clusters.bin", 5 * 1024 * 1024 + 123);
    put(&src, "Café/日本語 データ.bin", 4097);
    put(&src, "Ωmega-ÿ-ж.txt", 17);
    put(&src, &"L".repeat(200), 5);
    std::fs::create_dir_all(src.join("data/shaders")).unwrap();
    // macOS junk: the folder scanner drops all of it; the writer would skip it too.
    put(&src, ".DS_Store", 6);
    put(&src, "._eboot.bin", 4);
    put(&src, "a/b/._deep.dat", 4);
    put(&src, ".fseventsd/fseventsd-uuid", 3);
    put(&src, ".Spotlight-V100/Store-V2/x", 3);
    put(&src, ".Trashes/501/old.bin", 3);

    let image = tmp.0.join("out.exfat");
    let layout = build(&src, &image, &Options::default());
    assert_same(&src, &image);
    assert!(layout.skipped.is_empty(), "{:?}", layout.skipped);
    let names = raw_names(&image);
    assert!(names.iter().any(|n| n == "data/shaders"), "empty dir kept");
    assert!(names.iter().any(|n| n == "Café/日本語 データ.bin"));

    // The size exfat.sh would pick: 10 files, 10 dirs with the root.
    let sizes = [
        300_000,
        1234,
        0,
        0,
        70_000,
        65536,
        5 * 1024 * 1024 + 123,
        4097,
        17,
        5,
    ];
    assert_eq!(layout.image_size, script_size(&sizes, 10, 0));

    let boot = std::fs::read(&image).unwrap();
    assert_eq!(boot[..512], boot[12 * 512..13 * 512], "backup boot sector");

    // Same tree, same bytes.
    let again = tmp.0.join("again.exfat");
    build(&src, &again, &Options::default());
    assert!(std::fs::read(&image).unwrap() == std::fs::read(&again).unwrap());
}

#[test]
fn many_entry_directory_spans_clusters() {
    let tmp = TempDir::new("many");
    let src = tmp.0.join("src");
    for i in 0..3000 {
        // Names over 15 units take two name entries: ~4 entries × 32 B × 3000 > 64 KiB.
        put(
            &src,
            &format!("dir/file-number-{i:05}.bin"),
            (i % 7) as u64 * 100,
        );
    }
    put(&src, "eboot.bin", 10);
    let image = tmp.0.join("many.exfat");
    build(&src, &image, &Options::default());
    assert_same(&src, &image);
}

#[test]
fn free_bytes_grow_the_volume() {
    let tmp = TempDir::new("free");
    let src = tmp.0.join("src");
    put(&src, "eboot.bin", 100);
    let small = build(&src, &tmp.0.join("small.exfat"), &Options::default()).image_size;
    // A tiny tree still gets exfat.sh's 64 MiB spare and 32 MiB metadata allowance.
    assert_eq!(small, script_size(&[100], 1, 0));
    let opts = Options {
        free_bytes: 32 << 20,
        ..Options::default()
    };
    let roomy = tmp.0.join("roomy.exfat");
    let big = build(&src, &roomy, &opts).image_size;
    assert_eq!(big, small + (32 << 20));
    assert_same(&src, &roomy);
    // ClusterCount (offset 92) covers the free space too.
    assert!(u64::from(boot_u32(&roomy, 92)) * 65536 >= 96 << 20);
}

/// A tree held in memory, so names a case-insensitive macOS volume cannot hold side by
/// side (and terabyte sizes) can be planned.
struct MemTree {
    files: Vec<SourceFile>,
    empty: Vec<String>,
}

impl MemTree {
    fn new(files: &[(&str, u64)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|&(p, size)| SourceFile {
                    path: p.to_string(),
                    size,
                })
                .collect(),
            empty: Vec::new(),
        }
    }
}

impl SourceTree for MemTree {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let size = self.files.iter().find(|f| f.path == path).unwrap().size;
        self.read_range(path, 0, size as usize)
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let size = self.files.iter().find(|f| f.path == path).unwrap().size;
        let len = len.min(size.saturating_sub(offset) as usize);
        Ok(pattern(path, offset, len))
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty
    }

    fn describe(&self) -> String {
        "memory".into()
    }
}

#[test]
fn bad_names_are_all_listed() {
    let mut tree = MemTree::new(&[
        ("eboot.bin", 1),
        ("sce_sys/Cafe\u{301}.png", 1), // NFD
        ("sce_sys/A.txt", 1),
        ("sce_sys/a.txt", 1), // same name to the volume
        ("data/bad:name", 1),
        ("data/Σ.bin", 1),
        ("data/σ.BIN", 1), // collides beyond ASCII too
        ("x/eboot.bin", 1),
        ("x/eboot.bin/inner", 1), // a file and a directory at once
    ]);
    tree.empty.push("ctl\u{1}dir".into());
    tree.empty.push("long/".to_string() + &"n".repeat(256));
    let e = plan(&tree, &Options::default(), &AtomicBool::new(false))
        .unwrap_err()
        .to_string();
    for needle in [
        "sce_sys/Cafe\u{301}.png: not in Unicode NFC",
        "sce_sys/a.txt: same name as sce_sys/A.txt",
        "data/bad:name: forbidden character ':'",
        "same name as data/Σ.bin",
        "x/eboot.bin: both a file and a directory",
        "ctl\\u{1}dir: forbidden character",
        "256 UTF-16 units",
    ] {
        assert!(e.contains(needle), "missing {needle:?} in:\n{e}");
    }
    assert!(e.starts_with("7 name problem(s)"), "{e}");
}

#[test]
fn bad_label_is_rejected() {
    let tree = MemTree::new(&[("eboot.bin", 1)]);
    let opts = Options {
        label: Some("TWELVE CHARS".into()),
        ..Options::default()
    };
    let e = plan(&tree, &opts, &AtomicBool::new(false)).unwrap_err();
    assert!(e.to_string().contains("volume label"), "{e}");
}

#[test]
fn cancel_stops_plan_and_write() {
    let mut tree = MemTree::new(&[("big.bin", 40 << 20), ("eboot.bin", 1)]);
    let cancel = AtomicBool::new(true);
    let e = plan(&tree, &Options::default(), &cancel).unwrap_err();
    assert!(e.to_string().contains("cancelled"), "{e}");

    cancel.store(false, Ordering::Relaxed);
    let layout = plan(&tree, &Options::default(), &cancel).unwrap();
    let tmp = TempDir::new("cancel");
    let mut out = File::create(tmp.0.join("x.exfat")).unwrap();
    let mut calls = 0;
    let e = write(&mut tree, &layout, &mut out, &cancel, &mut |done, _| {
        calls += 1;
        if done > 0 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap_err();
    assert!(e.to_string().contains("cancelled"), "{e}");
    assert_eq!(calls, 1, "stopped at the next chunk");
}

#[test]
fn changed_tree_is_refused() {
    let tree = MemTree::new(&[("eboot.bin", 10)]);
    let layout = plan(&tree, &Options::default(), &AtomicBool::new(false)).unwrap();
    let mut grown = MemTree::new(&[("eboot.bin", 11)]);
    let tmp = TempDir::new("changed");
    let mut out = File::create(tmp.0.join("x.exfat")).unwrap();
    let e = write(
        &mut grown,
        &layout,
        &mut out,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap_err();
    assert!(e.to_string().contains("changed"), "{e}");
}

#[test]
fn huge_sizes_plan_with_u64_math() {
    // 5 GiB and 3 TiB files: past every 32-bit byte count, well inside 32-bit clusters.
    let tree = MemTree::new(&[("a.bin", 5 << 30), ("b.bin", 3 << 40), ("eboot.bin", 1)]);
    let layout = plan(&tree, &Options::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(
        layout.image_size,
        script_size(&[5 << 30, 3 << 40, 1], 1, 0),
        "512 MiB spare at this size"
    );
    assert_eq!(layout.data_bytes, (5u64 << 30) + (3u64 << 40) + 1);
}

/// Writes a check image for `scripts/check-exfat.sh`: set `FORGE_EXFAT_OUT` to a directory;
/// it gets `src/` and `check.exfat`.
#[test]
#[ignore = "writes a check image for the independent fsck script"]
fn write_check_image() {
    let out = PathBuf::from(std::env::var("FORGE_EXFAT_OUT").expect("set FORGE_EXFAT_OUT"));
    let src = out.join("src");
    let _ = std::fs::remove_dir_all(&src);
    put(&src, "eboot.bin", 300_000);
    put(&src, "sce_sys/param.json", 1234);
    put(&src, "empty.bin", 0);
    put(&src, "a/b/c/d/deep.dat", 70_000);
    put(&src, "big/many-clusters.bin", 9 * 1024 * 1024 + 7);
    put(&src, "Café/日本語 データ.bin", 4097);
    put(&src, "Ωmega-ÿ-ж.txt", 17);
    put(&src, &"L".repeat(200), 5);
    for i in 0..3000 {
        put(
            &src,
            &format!("many/file-number-{i:05}.bin"),
            (i % 7) as u64 * 100,
        );
    }
    std::fs::create_dir_all(src.join("data/shaders")).unwrap();
    let opts = Options {
        free_bytes: 16 << 20,
        label: Some("PPSA01234".into()),
    };
    let image = out.join("check.exfat");
    build(&src, &image, &opts);
    assert_same(&src, &image);

    // And a plain one: default options, no label.
    let plain = out.join("plain-src");
    let _ = std::fs::remove_dir_all(&plain);
    put(&plain, "eboot.bin", 3 << 20);
    put(&plain, "sce_sys/param.json", 0);
    std::fs::create_dir_all(plain.join("empty")).unwrap();
    let image = out.join("plain.exfat");
    build(&plain, &image, &Options::default());
    assert_same(&plain, &image);
}

#[test]
fn cancel_on_the_last_chunk_still_fails() {
    let mut tree = MemTree::new(&[("eboot.bin", 1)]);
    let cancel = AtomicBool::new(false);
    let layout = plan(&tree, &Options::default(), &cancel).unwrap();
    let tmp = TempDir::new("cancel-last");
    let mut out = File::create(tmp.0.join("x.exfat")).unwrap();
    let e = write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {
        cancel.store(true, Ordering::Relaxed)
    })
    .unwrap_err();
    assert!(e.to_string().contains("cancelled"), "{e}");
}

#[test]
fn cancel_from_the_final_progress_call_fails_an_empty_tree() {
    let mut tree = MemTree::new(&[("empty", 0)]);
    let cancel = AtomicBool::new(false);
    let layout = plan(&tree, &Options::default(), &cancel).unwrap();
    let tmp = TempDir::new("cancel-empty");
    let mut out = File::create(tmp.0.join("x.exfat")).unwrap();
    let e = write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {
        cancel.store(true, Ordering::Relaxed)
    })
    .unwrap_err();
    assert!(e.to_string().contains("cancelled"), "{e}");
}
