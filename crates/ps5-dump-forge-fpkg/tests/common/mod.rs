//! Shared by the test binaries: a game tree, a package built from it by the vendored builder,
//! and the expected manifest (`Prepared::manifest()` ∪ `container_only()`) to compare with.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use ps5_dump_forge_fpkg::FpkgSource;
use ps5upload_fpkg::build::{self, BuildControl, BuildRequest};
use ps5upload_fpkg::source::{FolderSource, SourceTree};

pub const CONTENT_ID: &str = "UP0000-PPSA01234_00-TESTGAME00000000";
pub const BIG: &str = "data/big.bin";
pub const BIG_LEN: usize = 5 * 256 * 1024 + 12_345;

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "forge-fpkg-{}-{}-{name}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// xorshift64*: reproducible bytes and choices without a dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

/// A game folder: artwork the container carries, trophy data both carry, a multi-block file
/// that mixes compressible and random stretches, an empty file, empty directories, nesting
/// and a non-ASCII name. `big_random` adds that many random bytes as `data/random.bin`.
pub fn write_tree(root: &Path, big_random: usize) {
    let mut rng = Rng::new(7);
    let put = |path: &str, data: Vec<u8>| {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, &data).unwrap();
    };
    put(
        "eboot.bin",
        (0..70_000u32).map(|i| (i % 251) as u8).collect(),
    );
    put(
        "sce_sys/param.json",
        format!(
            "{{\"contentId\":\"{CONTENT_ID}\",\"contentVersion\":\"01.001.000\",\"titleId\":\"PPSA01234\"}}"
        )
        .into_bytes(),
    );
    put("sce_sys/icon0.png", rng.bytes(2048));
    put("sce_sys/pic0.png", rng.bytes(1024));
    put("sce_sys/snd0.at9", rng.bytes(3000));
    put("sce_sys/trophy2/trophy00.ucp", rng.bytes(500));
    let mut big = Vec::with_capacity(BIG_LEN);
    while big.len() < BIG_LEN {
        // Alternate a compressible stretch and a random one, so blocks take both kinds of half.
        let n = (BIG_LEN - big.len()).min(70_000);
        if (big.len() / 70_000).is_multiple_of(2) {
            big.extend((0..n).map(|i| (i / 7 % 13) as u8));
        } else {
            big.extend(rng.bytes(n));
        }
    }
    put(BIG, big);
    put("data/zero.bin", Vec::new());
    put("data/nested/deep/leaf.txt", b"leaf\n".to_vec());
    put("data/\u{fc}n\u{ef}c\u{f6}de.txt", b"unicode\n".to_vec());
    for i in 0..40 {
        put(&format!("data/many/f{i:02}.bin"), rng.bytes(10 + i * 37));
    }
    if big_random > 0 {
        put("data/random.bin", rng.bytes(big_random));
    }
    std::fs::create_dir_all(root.join("data/empty")).unwrap();
    std::fs::create_dir_all(root.join("emptytop/inner")).unwrap();
}

/// A built package and what a reader of it must see.
pub struct Built {
    pub path: PathBuf,
    /// Every file, sorted by path, with its packaged bytes.
    pub expected: Vec<(String, Vec<u8>)>,
    pub empty_dirs: Vec<String>,
    pub container_only: Vec<String>,
    _src: TempDir,
    _out: TempDir,
}

impl Built {
    pub fn bytes(&self, path: &str) -> &[u8] {
        &self
            .expected
            .iter()
            .find(|(p, _)| p == path)
            .unwrap_or_else(|| panic!("{path} not expected"))
            .1
    }

    pub fn package(&self) -> Vec<u8> {
        std::fs::read(&self.path).unwrap()
    }
}

/// Build a package from the test tree through `prepare` + `write_package` (which verifies it),
/// starting from `BuildRequest::production` and letting `tweak` change the request.
pub fn build(name: &str, big_random: usize, tweak: impl FnOnce(&mut BuildRequest)) -> Built {
    let src = TempDir::new(&format!("{name}-src"));
    let out = TempDir::new(&format!("{name}-out"));
    write_tree(src.path(), big_random);
    let mut request = BuildRequest::production(src.path(), out.path());
    request.time = Some((1_700_000_000, 0));
    request.threads = Some(2);
    tweak(&mut request);
    let mut tree = FolderSource::open(src.path()).unwrap();
    let prepared = build::prepare(&mut tree, &request, &mut BuildControl::default()).unwrap();
    let mut expected = Vec::new();
    for e in prepared.manifest() {
        let bytes = prepared
            .read_range(&mut tree, &e.path, 0, e.size as usize)
            .unwrap();
        assert_eq!(bytes.len() as u64, e.size, "{}", e.path);
        expected.push((e.path.clone(), bytes));
    }
    let mut container_only = Vec::new();
    for e in prepared.container_only() {
        expected.push((e.path.clone(), tree.read(&e.path).unwrap()));
        container_only.push(e.path.clone());
    }
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    let path = out.path().join("game.pkg");
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
    Built {
        path,
        expected,
        empty_dirs: prepared.empty_dirs().to_vec(),
        container_only,
        _src: src,
        _out: out,
    }
}

/// The reader lists exactly the expected files and empty directories and serves every byte,
/// whole and by odd-sized ranges.
pub fn assert_matches(source: &mut FpkgSource, built: &Built) {
    let listed: Vec<(String, u64)> = source
        .files()
        .iter()
        .map(|f| (f.path.clone(), f.size))
        .collect();
    let want: Vec<(String, u64)> = built
        .expected
        .iter()
        .map(|(p, b)| (p.clone(), b.len() as u64))
        .collect();
    assert_eq!(listed, want);
    assert_eq!(source.empty_dirs(), built.empty_dirs.as_slice());
    assert!(source.conflicts().is_empty(), "{:?}", source.conflicts());
    for (path, bytes) in &built.expected {
        assert!(source.read(path).unwrap() == *bytes, "{path} read whole");
        let mut got = Vec::new();
        loop {
            let chunk = source.read_range(path, got.len() as u64, 100_003).unwrap();
            if chunk.is_empty() {
                break;
            }
            got.extend_from_slice(&chunk);
        }
        assert!(got == *bytes, "{path} read by ranges");
    }
}

pub fn open_bytes(bytes: Vec<u8>, passcode: Option<&str>) -> ps5upload_fpkg::Result<FpkgSource> {
    let len = bytes.len() as u64;
    FpkgSource::from_reader(
        Box::new(std::io::Cursor::new(bytes)),
        len,
        "memory",
        passcode,
    )
}

pub fn error_of(result: ps5upload_fpkg::Result<FpkgSource>) -> String {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.to_string(),
    }
}

/// Where `imagedigs` sits in a package (read with the trusted vendored parser).
pub fn imagedigs_at(pkg: &[u8]) -> usize {
    let cnt_offset = u64::from_le_bytes(pkg[0x58..0x60].try_into().unwrap()) as usize;
    let cnt = ps5upload_fpkg::cnt::Cnt::from_bytes(pkg[cnt_offset..].to_vec()).unwrap();
    let e = cnt.entry(ps5upload_fpkg::cnt::ids::IMAGE_DIGESTS).unwrap();
    cnt_offset + e.offset as usize
}
