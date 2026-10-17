//! The in-place editor's volume reader against Forge-built images.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ps5_dump_forge_exfat::edit::{Volume, plan};
use ps5_dump_forge_exfat::{Layout, Options};
use ps5_dump_forge_patch::{Data, Op, Syncs};
use ps5upload_fpkg::exfat::ExFatSource;
use ps5upload_fpkg::source::{FolderSource, SourceTree};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ps5-dump-forge-exfat-edit-{tag}-{}-{n}",
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

fn bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31) ^ seed)
        .collect()
}

const LONG: &str = "a-file-name-longer-than-fifteen-units.bin";

/// A small game tree in `<tmp>/src`, built into `<tmp>/game.exfat`.
fn build(tmp: &TempDir, opts: &Options) -> (PathBuf, Layout) {
    build_with(tmp, opts, &[])
}

/// [`build`] with `extra` empty files in the tree.
fn build_with(tmp: &TempDir, opts: &Options, extra: &[String]) -> (PathBuf, Layout) {
    let src = tmp.0.join("src");
    for rel in extra {
        let p = src.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }
    let files: [(&str, usize); 5] = [
        ("eboot.bin", 100_000),
        ("sce_sys/param.json", 300),
        ("data/big.bin", 200_000),
        ("data/sub/x.txt", 0),
        ("data/Ünïcode.txt", 5),
    ];
    for (i, (rel, len)) in files.iter().enumerate() {
        let p = src.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes(i as u8, *len)).unwrap();
    }
    std::fs::write(src.join(LONG), b"long").unwrap();
    std::fs::create_dir_all(src.join("empty")).unwrap();
    let image = tmp.0.join("game.exfat");
    let mut tree = FolderSource::open(&src).unwrap();
    let cancel = AtomicBool::new(false);
    let layout = ps5_dump_forge_exfat::plan(&tree, opts, &cancel).unwrap();
    let mut out = File::create(&image).unwrap();
    ps5_dump_forge_exfat::write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {}).unwrap();
    (image, layout)
}

fn open(image: &Path) -> (File, Volume) {
    let mut f = File::open(image).unwrap();
    let v = Volume::open(&mut f).unwrap();
    (f, v)
}

fn read_byte(f: &mut File, at: u64) -> u8 {
    use std::io::{Read, Seek, SeekFrom};
    let mut b = [0u8];
    f.seek(SeekFrom::Start(at)).unwrap();
    f.read_exact(&mut b).unwrap();
    b[0]
}

#[test]
fn corrupt_boot_checksum_is_an_error() {
    use std::io::{Seek, SeekFrom, Write};
    let tmp = TempDir::new("checksum");
    let (image, _) = build(&tmp, &Options::default());
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(&image)
        .unwrap();
    f.seek(SeekFrom::Start(100)).unwrap(); // VolumeSerialNumber, under the checksum
    f.write_all(&[0x5A]).unwrap();
    drop(f);
    let err = Volume::open(&mut File::open(&image).unwrap()).unwrap_err();
    assert!(err.to_string().contains("boot checksum"), "{err}");
}

#[test]
fn not_exfat_is_an_error() {
    let tmp = TempDir::new("notexfat");
    let p = tmp.0.join("zero.exfat");
    std::fs::write(&p, vec![0u8; 8192]).unwrap();
    assert!(Volume::open(&mut File::open(&p).unwrap()).is_err());
}

/// Byte `at` of test source `id`: `bytes(id, n)` is its first `n` bytes.
fn src_byte(id: u32, at: u64) -> u8 {
    (at as u8).wrapping_mul(31) ^ id as u8
}

/// Plans `ops`, applies and commits them; the image's free bytes before and after.
fn edit(image: &Path, ops: &[Op]) -> (u64, u64) {
    let before = open(image).1.free_bytes();
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .unwrap();
    let writes = plan(&mut f, ops).unwrap();
    // Free-space writes (new directory clusters) come first, then the live ones, bracketed
    // by the boot sector: VolumeDirty set first, cleared last.
    let free = writes.iter().take_while(|w| !w.live).count();
    let writes = &writes[free..];
    assert!(writes.iter().all(|w| w.live));
    let flags = |w: &ps5_dump_forge_patch::Write| match &w.data {
        Data::Bytes(b) if w.offset == 0 && b.len() == 512 => b[106] & 0x2,
        _ => panic!("not a boot sector write: {w:?}"),
    };
    assert_eq!(flags(&writes[0]), 0x2);
    assert_eq!(flags(writes.last().unwrap()), 0);
    let cancel = AtomicBool::new(false);
    let mut source = |id: u32, at: u64, buf: &mut [u8]| -> std::io::Result<()> {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = src_byte(id, at + i as u64);
        }
        Ok(())
    };
    let writes = plan(&mut f, ops).unwrap();
    ps5_dump_forge_patch::apply(&mut f, image, &writes, &mut source, &cancel, Syncs::STD)
        .unwrap()
        .commit(Syncs::STD)
        .unwrap();
    drop(f);
    (before, open(image).1.free_bytes())
}

/// Every file of the image with its bytes, read back by ps5upload's reader.
fn read_back(image: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut src = ExFatSource::open(image).unwrap();
    let paths: Vec<String> = src.files().iter().map(|f| f.path.clone()).collect();
    paths
        .into_iter()
        .map(|p| {
            let b = src.read(&p).unwrap();
            (p, b)
        })
        .collect()
}

fn write_at(image: &Path, at: u64, data: &[u8]) {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(image).unwrap();
    f.seek(SeekFrom::Start(at)).unwrap();
    f.write_all(data).unwrap();
}

/// Two free clusters, not adjacent, from the end of the heap.
fn two_free(v: &Volume) -> (u32, u32) {
    let last = v.cluster_count + 1;
    let (a, b) = (last, last - 2);
    assert!(!v.in_use(a) && !v.in_use(b));
    (a, b)
}

#[test]
fn fat_chains_fragmented_short_long_and_cyclic() {
    use ps5_dump_forge_exfat::edit::Stream;
    let tmp = TempDir::new("chains");
    let (image, _) = build(&tmp, &Options::default());
    let (_, mut v) = open(&image);
    let cs = v.cluster_size();
    let (a, b) = two_free(&v);
    write_at(&image, v.cluster_offset(a), &[0xAA; 65536]);
    write_at(&image, v.cluster_offset(b), &[0xBB; 65536]);
    v.fat[a as usize] = b;
    v.fat[b as usize] = u32::MAX;
    let s = |clusters: u64| Stream {
        first_cluster: a,
        len: clusters * cs - 10,
        no_fat_chain: false,
    };
    assert_eq!(v.chain(s(2)).unwrap(), [a, b]);
    // The list is reserved at its exact bound, never regrown past it.
    assert_eq!(v.chain(s(2)).unwrap().capacity(), 2);
    let data = v
        .read_stream(&mut File::open(&image).unwrap(), s(2))
        .unwrap();
    assert!(data[..65536].iter().all(|&x| x == 0xAA) && data[65536..].iter().all(|&x| x == 0xBB));
    assert!(v.chain(s(1)).is_err(), "chain longer than the stream");
    assert!(v.chain(s(3)).is_err(), "chain ends early");
    v.fat[b as usize] = a;
    assert!(v.chain(s(3)).is_err(), "loop a -> b -> a");
    v.fat[a as usize] = a;
    assert!(v.chain(s(2)).is_err(), "FAT[a] = a");
    v.fat[a as usize] = 0;
    assert!(v.chain(s(2)).is_err(), "free entry before the end");

    // On disk: the root directory's chain pointing at itself.
    let root_slot = v.fat_offset + u64::from(v.root_cluster) * 4;
    write_at(&image, root_slot, &v.root_cluster.to_le_bytes());
    let err = Volume::open(&mut File::open(&image).unwrap()).unwrap_err();
    assert!(err.to_string().contains("loops"), "{err}");
}

#[test]
fn dirty_image_is_refused_by_plan() {
    let tmp = TempDir::new("dirty");
    let (image, _) = build(&tmp, &Options::default());
    write_at(&image, 106, &[0x02]); // VolumeFlags.VolumeDirty, outside the boot checksum
    let mut f = File::open(&image).unwrap();
    assert!(Volume::open(&mut f).unwrap().dirty);
    let findings = plan(&mut f, &[]).unwrap_err();
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].contains("VolumeDirty") && findings[0].contains("rebuild"),
        "{findings:?}"
    );
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

#[test]
fn huge_allocation_tables_are_refused() {
    // 1 TiB of 4 KiB clusters: a 1 GiB FAT. The file holds only the boot region.
    let cc: u32 = 1 << 28;
    let fat_sectors = ((u64::from(cc) + 2) * 4).div_ceil(512) as u32;
    let heap = 128 + fat_sectors;
    let mut r = vec![0u8; 12 * 512];
    r[3..11].copy_from_slice(b"EXFAT   ");
    r[72..80].copy_from_slice(&(u64::from(heap) + u64::from(cc) * 8).to_le_bytes());
    put32(&mut r, 80, 128);
    put32(&mut r, 84, fat_sectors);
    put32(&mut r, 88, heap);
    put32(&mut r, 92, cc);
    put32(&mut r, 96, 2);
    (r[108], r[109], r[110]) = (9, 3, 1);
    r[510..512].copy_from_slice(&[0x55, 0xAA]);
    let sum = r[..11 * 512]
        .iter()
        .enumerate()
        .filter(|(i, _)| !matches!(i, 106 | 107 | 112))
        .fold(0u32, |s, (_, &b)| {
            s.rotate_right(1).wrapping_add(u32::from(b))
        });
    for at in (11 * 512..12 * 512).step_by(4) {
        put32(&mut r, at, sum);
    }
    let tmp = TempDir::new("huge");
    let p = tmp.0.join("huge.exfat");
    std::fs::write(&p, &r).unwrap();
    let err = Volume::open(&mut File::open(&p).unwrap()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::OutOfMemory);
    assert!(
        err.to_string()
            .contains("too big to edit in place (1057 MiB); rebuild it instead"),
        "{err}"
    );
    let findings = plan(&mut File::open(&p).unwrap(), &[]).unwrap_err();
    assert!(
        findings[0].contains("allocation tables are too big"),
        "{findings:?}"
    );
}

/// The byte just past the root directory's last entry set.
fn root_end(image: &Path) -> u64 {
    let (mut f, v) = open(image);
    v.list(&mut f, "")
        .unwrap()
        .iter()
        .map(|n| *n.entries.last().unwrap())
        .max()
        .unwrap()
        + 32
}

/// Fills in the SetChecksum of an entry set.
fn seal(set: &mut [u8]) {
    let sum = set
        .iter()
        .enumerate()
        .filter(|(i, _)| !matches!(i, 2 | 3))
        .fold(0u16, |s, (_, &b)| {
            s.rotate_right(1).wrapping_add(u16::from(b))
        });
    set[2..4].copy_from_slice(&sum.to_le_bytes());
}

/// A file set named `name` (255 units at most) with no data, plus a Vendor Extension and a
/// Vendor Allocation entry owning `alloc_len` bytes at cluster `alloc` (FAT-chained).
fn vendor_set(name: &str, alloc: u32, alloc_len: u64) -> Vec<u8> {
    let names = name.encode_utf16().count().div_ceil(15);
    let sc = 1 + names + 2;
    let mut set = vec![0u8; (1 + sc) * 32];
    (set[0], set[1]) = (0x85, sc as u8);
    set[4] = 0x20; // archive
    (set[32], set[33], set[35]) = (0xC0, 0x01, name.encode_utf16().count() as u8);
    for (i, u) in name.encode_utf16().enumerate() {
        let e = 64 + i / 15 * 32;
        set[e] = 0xC1;
        set[e + 2 + 2 * (i % 15)..e + 4 + 2 * (i % 15)].copy_from_slice(&u.to_le_bytes());
    }
    set[(2 + names) * 32] = 0xE0; // Vendor Extension
    let at = (3 + names) * 32;
    (set[at], set[at + 1]) = (0xE1, 0x01); // Vendor Allocation, AllocationPossible
    put32(&mut set, at + 20, alloc);
    set[at + 24..at + 32].copy_from_slice(&alloc_len.to_le_bytes());
    seal(&mut set);
    set
}

/// Marks cluster `c` in use on disk, its FAT entry `next`.
fn use_cluster(image: &Path, v: &Volume, c: u32, next: u32) {
    write_at(image, v.fat_offset + u64::from(c) * 4, &next.to_le_bytes());
    let i = u64::from(c - 2);
    let at = v.cluster_offset(v.bitmap_stream.first_cluster) + i / 8;
    let b = read_byte(&mut File::open(image).unwrap(), at) | 1 << (i % 8);
    write_at(image, at, &[b]);
}

/// Deletes `name` from the root with every check: free bytes grew by `clusters`, each FAT
/// entry of `freed` is 0, the other files are unchanged.
fn delete_and_check(image: &Path, name: &str, clusters: u64, freed: &[u32]) {
    let mut want = read_back(image);
    want.remove(name);
    let (before, after) = edit(image, &[Op::Delete { path: name.into() }]);
    let (mut f, v) = open(image);
    assert_eq!(after - before, clusters * v.cluster_size());
    assert_eq!(v.lookup(&mut f, name).unwrap(), None);
    for &c in freed {
        assert_eq!(v.fat[c as usize], 0, "FAT[{c}]");
        assert!(!v.in_use(c), "{c}");
    }
    assert_eq!(read_back(image), want);
}

#[test]
fn delete_a_fragmented_fat_chained_file() {
    let tmp = TempDir::new("delfrag");
    let (image, _) = build(&tmp, &Options::default());
    let v = open(&image).1;
    let (a, b) = two_free(&v);
    write_at(&image, v.cluster_offset(a), &[0xAA; 65536]);
    write_at(&image, v.cluster_offset(b), &[0xBB; 100]);
    use_cluster(&image, &v, a, b);
    use_cluster(&image, &v, b, u32::MAX);
    let len = 65536u64 + 100;
    let mut set = vec![0u8; 3 * 32];
    (set[0], set[1], set[4]) = (0x85, 2, 0x20);
    (set[32], set[33], set[35]) = (0xC0, 0x01, 4); // FAT-chained (no NoFatChain)
    set[40..48].copy_from_slice(&len.to_le_bytes());
    put32(&mut set, 52, a);
    set[56..64].copy_from_slice(&len.to_le_bytes());
    set[64] = 0xC1;
    for (i, u) in "frag".encode_utf16().enumerate() {
        set[66 + 2 * i..68 + 2 * i].copy_from_slice(&u.to_le_bytes());
    }
    seal(&mut set);
    write_at(&image, root_end(&image), &set);
    let (mut f, v) = open(&image);
    let n = v.lookup(&mut f, "frag").unwrap().unwrap();
    assert_eq!(v.chain(n.stream).unwrap(), [a, b]);
    assert_eq!(read_back(&image)["frag"].len() as u64, len);
    delete_and_check(&image, "frag", 2, &[a, b]);
}

fn ren(from: &str, to: &str) -> Op {
    Op::Rename {
        from: from.into(),
        to: to.into(),
    }
}

/// The free-space writes `ops` would plan.
fn free_writes(image: &Path, ops: &[Op]) -> usize {
    let writes = plan(&mut File::open(image).unwrap(), ops).unwrap();
    writes.iter().filter(|w| !w.live).count()
}

#[test]
fn delete_a_folder_tree() {
    let tmp = TempDir::new("deldir");
    let (image, _) = build(&tmp, &Options::default());
    let mut want = read_back(&image);
    let cs = open(&image).1.cluster_size();
    let (before, after) = edit(
        &image,
        &[Op::Delete {
            path: "data".into(),
        }],
    );
    // data and data/sub (a cluster each), big.bin (4), Ünïcode.txt (1), x.txt (none).
    assert_eq!(after - before, 7 * cs);
    want.retain(|p, _| !p.starts_with("data/"));
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    for p in ["data", "data/sub", "data/big.bin"] {
        assert_eq!(v.lookup(&mut f, p).unwrap(), None, "{p}");
    }
    assert!(v.lookup(&mut f, "empty").unwrap().unwrap().is_dir());
    // An empty folder: its one cluster.
    let (before, after) = edit(
        &image,
        &[Op::Delete {
            path: "EMPTY".into(),
        }],
    );
    assert_eq!(after - before, cs);
    assert_eq!(read_back(&image), want);
}

/// `want` with `from` (a file, or every file under the folder) renamed to `to`.
fn moved(mut want: BTreeMap<String, Vec<u8>>, from: &str, to: &str) -> BTreeMap<String, Vec<u8>> {
    let keys: Vec<String> = want
        .keys()
        .filter(|p| *p == from || p.starts_with(&format!("{from}/")))
        .cloned()
        .collect();
    assert!(!keys.is_empty(), "{from}");
    for k in keys {
        let b = want.remove(&k).unwrap();
        want.insert(format!("{to}{}", &k[from.len()..]), b);
    }
    want
}

#[test]
fn move_into_another_folder() {
    let tmp = TempDir::new("renmove");
    let (image, _) = build(&tmp, &Options::default());
    let want = read_back(&image);
    let ops = [
        ren("data/sub", "empty/sub2"),
        ren("data/big.bin", "big.bin"),
        ren("data/Ünïcode.txt", "empty/sub2/Ünïcode-moved.txt"),
    ];
    let (before, after) = edit(&image, &ops);
    assert_eq!(before, after);
    let want = moved(want, "data/sub", "empty/sub2");
    let want = moved(want, "data/big.bin", "big.bin");
    let want = moved(want, "data/Ünïcode.txt", "empty/sub2/Ünïcode-moved.txt");
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    assert!(v.list(&mut f, "data").unwrap().is_empty());
    assert_eq!(v.list(&mut f, "empty/sub2").unwrap().len(), 2);
}

/// `n` empty files `<dir>/fNNN` (3 entries each).
fn fillers(dir: &str, n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{dir}f{i:03}")).collect()
}

#[test]
fn a_full_folder_gets_a_new_cluster() {
    let tmp = TempDir::new("rengrow");
    // 682 sets of 3 entries leave 2 of a 64 KiB cluster's 2048 unused.
    let (image, _) = build_with(&tmp, &Options::default(), &fillers("full/", 682));
    let want = read_back(&image);
    let (mut f, v) = open(&image);
    let cs = v.cluster_size();
    let full = v.lookup(&mut f, "full").unwrap().unwrap();
    assert_eq!(full.stream.len, cs);
    let ops = [ren("data/big.bin", "full/big.bin")];
    assert_eq!(free_writes(&image, &ops), 1);
    let (before, after) = edit(&image, &ops);
    assert_eq!(before - after, cs);
    let (mut f, v) = open(&image);
    let full = v.lookup(&mut f, "full").unwrap().unwrap();
    assert_eq!(
        read_back(&image),
        moved(want, "data/big.bin", "full/big.bin")
    );
    assert_eq!((full.stream.len, full.valid_len), (2 * cs, 2 * cs));
    let chain = v.chain(full.stream).unwrap();
    assert_eq!(chain.len(), 2);
    // The set starts in the first cluster's two end-of-directory entries and spans into the
    // new one.
    let big = v.lookup(&mut f, "full/big.bin").unwrap().unwrap();
    assert_eq!(big.cluster, chain[0]);
    assert_eq!(big.entries[0], v.cluster_offset(chain[0]) + cs - 64);
    assert_eq!(big.entries[2], v.cluster_offset(chain[1]));
    assert_eq!(v.list(&mut f, "full").unwrap().len(), 683);
    // A second move lands in the new cluster's unused entries, no allocation.
    let ops = [ren("data/Ünïcode.txt", "full/u.txt")];
    assert_eq!(free_writes(&image, &ops), 0);
    edit(&image, &ops);
    assert_eq!(
        v.list(&mut File::open(&image).unwrap(), "full")
            .unwrap()
            .len(),
        684
    );
}

#[test]
fn renames_that_do_not_apply_are_findings() {
    let tmp = TempDir::new("renbad");
    let (image, _) = build_with(&tmp, &Options::default(), &fillers("full/", 682));
    let v = open(&image).1;
    write_at(&image, root_end(&image), &vendor_set("vend.bin", 0, 0));
    let mut f = File::open(&image).unwrap();
    let cases = [
        (ren("nope", "x"), "nope: not found"),
        (
            ren("data/big.bin", "EBOOT.BIN"),
            "EBOOT.BIN: already exists",
        ),
        (ren("data/big.bin", "data/sub/X.TXT"), "already exists"),
        (ren("data/big.bin", "a:b"), "forbidden character"),
        (ren("data/big.bin", "nope/x"), "folder nope not found"),
        (
            ren("data/big.bin", "eboot.bin/x"),
            "folder eboot.bin not found",
        ),
        (ren("data", "DATA/sub/data"), "moved into itself"),
        (ren("vend.bin", "v.bin"), "vendor entries"),
    ];
    for (op, want) in &cases {
        let findings = plan(&mut f, std::slice::from_ref(op)).unwrap_err();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains(want), "{op:?}: {findings:?}");
    }
    // No free cluster for the folder that must grow.
    let bitmap = v.cluster_offset(v.bitmap_stream.first_cluster);
    write_at(&image, bitmap, &vec![0xFF; v.bitmap.len()]);
    let findings = plan(&mut f, &[ren("eboot.bin", "full/e.bin")]).unwrap_err();
    assert_eq!(
        findings,
        ["the changes need 1 MB, the image has 0 MB free".to_string()]
    );
}

/// A set that would fill a folder's last entries exactly takes a new cluster instead: an
/// end-of-directory entry always stays after the last set (else a reader runs past the end).
#[test]
fn an_exact_fit_keeps_an_end_of_directory_entry() {
    let tmp = TempDir::new("renexact");
    // 2045 root entries in use, 3 unused: exactly a 3-entry set.
    let (image, _) = build_with(&tmp, &Options::default(), &fillers("", 675));
    let want = read_back(&image);
    let ops = [ren("data/big.bin", "big.bin")];
    assert_eq!(free_writes(&image, &ops), 1);
    let (before, after) = edit(&image, &ops);
    let (mut f, v) = open(&image);
    assert_eq!(before - after, v.cluster_size());
    assert_eq!(read_back(&image), moved(want, "data/big.bin", "big.bin"));
    let next = v.fat[v.root_cluster as usize];
    assert_eq!(v.fat[next as usize], u32::MAX);
    let big = v.lookup(&mut f, "big.bin").unwrap().unwrap();
    // It still takes the three unused entries; the new cluster holds only end entries.
    let root_end = v.cluster_offset(v.root_cluster) + v.cluster_size();
    assert_eq!(big.entries[2], root_end - 32);
    assert_eq!(read_byte(&mut f, v.cluster_offset(next)), 0);
}

fn put(path: &str, data: Data) -> Op {
    Op::Put {
        path: path.into(),
        len: data.len(),
        data,
    }
}

fn source(id: u32, offset: u64, len: u64) -> Data {
    Data::Source { id, offset, len }
}

fn mkdir(path: &str) -> Op {
    Op::Mkdir { path: path.into() }
}

/// The raw bytes `at..at + len` of the image.
fn raw(image: &Path, at: u64, len: usize) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(image).unwrap();
    let mut b = vec![0u8; len];
    f.seek(SeekFrom::Start(at)).unwrap();
    f.read_exact(&mut b).unwrap();
    b
}

#[test]
fn put_new_files() {
    let tmp = TempDir::new("putnew");
    let (image, _) = build(&tmp, &Options::default());
    let mut want = read_back(&image);
    let (_, v) = open(&image);
    let cs = v.cluster_size();
    // Garbage in the free clusters a next-fit allocation takes first: the new file's last
    // cluster must come back with its tail zeroed.
    let first_free = (2..v.cluster_count + 2).find(|&c| !v.in_use(c)).unwrap();
    write_at(
        &image,
        v.cluster_offset(first_free),
        &vec![0xEE; 8 * cs as usize],
    );
    let big = bytes(9, 1000 + 150_000)[1000..].to_vec();
    let ops = [
        put("data/new.bin", source(9, 1000, 150_000)),
        put("small.txt", Data::Bytes(b"hello".to_vec())),
        put("empty/zero.bin", Data::Zero(0)),
        put("data/zeros.bin", Data::Zero(70_000)),
    ];
    let (before, after) = edit(&image, &ops);
    // 3 + 1 + 0 + 2 clusters; every folder had room for its new set.
    assert_eq!(before - after, 6 * cs);
    want.insert("data/new.bin".into(), big);
    want.insert("small.txt".into(), b"hello".to_vec());
    want.insert("empty/zero.bin".into(), Vec::new());
    want.insert("data/zeros.bin".into(), vec![0; 70_000]);
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    let new = v.lookup(&mut f, "data/new.bin").unwrap().unwrap();
    assert!(new.stream.no_fat_chain);
    assert_eq!(new.stream.first_cluster, first_free);
    assert_eq!((new.stream.len, new.valid_len), (150_000, 150_000));
    // The writer's FAT chain over a contiguous run too.
    let c = new.stream.first_cluster as usize;
    assert_eq!(v.fat[c..c + 3], [c as u32 + 1, c as u32 + 2, u32::MAX]);
    let tail = raw(
        &image,
        v.cluster_offset(first_free) + 150_000,
        (3 * cs - 150_000) as usize,
    );
    assert!(tail.iter().all(|&b| b == 0));
    let zero = v.lookup(&mut f, "empty/zero.bin").unwrap().unwrap();
    assert_eq!((zero.stream.first_cluster, zero.stream.len), (0, 0));
    assert!(!zero.stream.no_fat_chain);
    // A new set carries the writer's fixed timestamps and the archive attribute.
    let small = v.lookup(&mut f, "SMALL.TXT").unwrap().unwrap();
    let eboot = v.lookup(&mut f, "eboot.bin").unwrap().unwrap();
    assert_eq!(small.attributes, 0x20);
    assert_eq!(
        raw(&image, small.entries[0] + 8, 12),
        raw(&image, eboot.entries[0] + 8, 12)
    );
}

#[test]
fn replace_with_a_bigger_or_smaller_file() {
    let tmp = TempDir::new("putreplace");
    let (image, _) = build(&tmp, &Options::default());
    let mut want = read_back(&image);
    let (mut f, v) = open(&image);
    let cs = v.cluster_size();
    let old_eboot = v.lookup(&mut f, "eboot.bin").unwrap().unwrap();
    let old_big = v.lookup(&mut f, "data/big.bin").unwrap().unwrap();
    let old_clusters = [
        v.chain(old_eboot.stream).unwrap(),
        v.chain(old_big.stream).unwrap(),
    ];
    let times = raw(&image, old_eboot.entries[0] + 8, 12);
    // The path's case need not match: the file there keeps its name.
    let ops = [
        put("EBOOT.BIN", source(3, 0, 300_000)),
        put("data/big.bin", Data::Bytes(b"0123456789".to_vec())),
    ];
    let (before, after) = edit(&image, &ops);
    // eboot.bin 2 -> 5 clusters, big.bin 4 -> 1.
    assert_eq!((before as i64 - after as i64) / cs as i64, 3 - 3);
    want.insert("eboot.bin".into(), bytes(3, 300_000));
    want.insert("data/big.bin".into(), b"0123456789".to_vec());
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    let eboot = v.lookup(&mut f, "eboot.bin").unwrap().unwrap();
    let big = v.lookup(&mut f, "data/big.bin").unwrap().unwrap();
    // The sets are rewritten where they are, timestamps and attributes kept.
    assert_eq!(
        (eboot.name.as_str(), &eboot.entries),
        ("eboot.bin", &old_eboot.entries)
    );
    assert_eq!(big.entries, old_big.entries);
    assert_eq!(eboot.attributes, old_eboot.attributes);
    assert_eq!(raw(&image, eboot.entries[0] + 8, 12), times);
    assert_eq!((eboot.stream.len, eboot.valid_len), (300_000, 300_000));
    // New clusters only: the old data was never overwritten, and the old clusters are free.
    for c in old_clusters.iter().flatten() {
        assert!(!v.in_use(*c), "{c}");
        assert!(!v.chain(eboot.stream).unwrap().contains(c));
        assert!(!v.chain(big.stream).unwrap().contains(c));
    }
    let old_data = raw(&image, v.cluster_offset(old_clusters[0][0]), 100);
    assert_eq!(old_data, bytes(0, 100));
}

#[test]
fn put_into_new_folders() {
    let tmp = TempDir::new("putdirs");
    let (image, _) = build(&tmp, &Options::default());
    let mut want = read_back(&image);
    let cs = open(&image).1.cluster_size();
    let ops = [
        mkdir("new/deep"),
        put("new/deep/f.bin", source(5, 0, 10)),
        // A Put makes its missing folders too.
        put("other/x/y.bin", source(6, 0, 70_000)),
        mkdir("data"),
        mkdir("new/empty"),
    ];
    let (before, after) = edit(&image, &ops);
    // Folders new, new/deep, new/empty, other, other/x; files 1 + 2 clusters.
    assert_eq!(before - after, (5 + 3) * cs);
    want.insert("new/deep/f.bin".into(), bytes(5, 10));
    want.insert("other/x/y.bin".into(), bytes(6, 70_000));
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    for d in ["new", "new/deep", "new/empty", "other", "other/x"] {
        let n = v.lookup(&mut f, d).unwrap().unwrap();
        assert!(n.is_dir() && n.stream.no_fat_chain, "{d}");
        assert_eq!((n.stream.len, n.valid_len), (cs, cs), "{d}");
    }
    assert!(v.list(&mut f, "new/empty").unwrap().is_empty());
    let mut names: Vec<String> = v
        .list(&mut f, "new")
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    names.sort();
    assert_eq!(names, ["deep", "empty"]);
    let src = ExFatSource::open(&image).unwrap();
    assert!(
        src.empty_dirs().iter().any(|d| d == "new/empty"),
        "{:?}",
        src.empty_dirs()
    );
}

/// Free space split by deletes: a file that fits no single run gets a FAT chain over the
/// lowest free clusters.
#[test]
fn fragmented_allocation_gets_a_fat_chain() {
    let tmp = TempDir::new("putfrag");
    let parts: Vec<String> = (0..10).map(|i| format!("parts/p{i}")).collect();
    let (image, _) = build_with(&tmp, &Options::default(), &[]);
    let cs = open(&image).1.cluster_size();
    let ops: Vec<Op> = parts
        .iter()
        .map(|p| put(p, Data::Bytes(vec![7; cs as usize])))
        .collect();
    edit(&image, &ops);
    // The rest of the free space taken by one filler: no run of 3 is left anywhere.
    let free = open(&image).1.free_bytes();
    edit(&image, &[put("filler.bin", Data::Zero(free))]);
    assert_eq!(open(&image).1.free_bytes(), 0);
    let (mut f, v) = open(&image);
    let mut owned: Vec<(u32, &String)> = parts
        .iter()
        .map(|p| {
            (
                v.lookup(&mut f, p).unwrap().unwrap().stream.first_cluster,
                p,
            )
        })
        .collect();
    owned.sort();
    // Every other part, in cluster order: no two freed clusters are adjacent.
    let gone: Vec<(u32, &String)> = owned.iter().step_by(2).take(3).cloned().collect();
    let dels: Vec<Op> = gone
        .iter()
        .map(|(_, p)| Op::Delete {
            path: p.to_string(),
        })
        .collect();
    edit(&image, &dels);
    let mut want = read_back(&image);
    let len = 2 * cs + 100;
    edit(&image, &[put("frag.bin", source(11, 0, len))]);
    want.insert("frag.bin".into(), bytes(11, len as usize));
    assert_eq!(read_back(&image), want);
    let (mut f, v) = open(&image);
    let frag = v.lookup(&mut f, "frag.bin").unwrap().unwrap();
    assert!(!frag.stream.no_fat_chain);
    let chain = v.chain(frag.stream).unwrap();
    assert_eq!(chain, gone.iter().map(|g| g.0).collect::<Vec<_>>());
    assert_eq!(v.fat[*chain.last().unwrap() as usize], u32::MAX);
    assert_eq!(v.free_bytes(), 0);
    // The last cluster's tail is zeroed (it held the deleted part's 7s).
    let tail = raw(
        &image,
        v.cluster_offset(chain[2]) + 100,
        (cs - 100) as usize,
    );
    assert!(tail.iter().all(|&b| b == 0));
    if let Some(keep) = std::env::var_os("FORGE_EXFAT_KEEP") {
        std::fs::copy(&image, PathBuf::from(keep).join("fragmented.exfat")).unwrap();
    }
}

#[test]
fn no_room_is_one_finding() {
    let tmp = TempDir::new("putfull");
    let (image, _) = build(&tmp, &Options::default());
    let (_, v) = open(&image);
    let (cs, free) = (v.cluster_size(), v.free_clusters());
    let mut f = File::open(&image).unwrap();
    let mb = |clusters: u64| (clusters * cs).div_ceil(1_000_000);
    let have = free * cs / 1_000_000;
    // Data and new folder clusters are counted together.
    // 16 folders (a cluster each, about 1 MB) and a file in the last one.
    let deep = "d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s";
    let ops = [
        put("a.bin", Data::Zero(free * cs)),
        mkdir(deep),
        put(&format!("{deep}/b.bin"), Data::Zero(1)),
    ];
    let findings = plan(&mut f, &ops).unwrap_err();
    let msg = |n| {
        format!(
            "the changes need {} MB, the image has {have} MB free",
            mb(n)
        )
    };
    assert_ne!(mb(free + 17), mb(free + 2));
    assert_eq!(findings, [msg(free + 17)]);
    // What a plan frees is not reused: a delete does not make room for a replace.
    let ops = [
        Op::Delete {
            path: "data/big.bin".into(),
        },
        put("eboot.bin", Data::Zero((free + 1) * cs)),
    ];
    assert_eq!(plan(&mut f, &ops).unwrap_err(), [msg(free + 1)]);
    // Exactly the free space fits.
    assert!(plan(&mut f, &[put("eboot.bin", Data::Zero(free * cs))]).is_ok());
}

/// Every op in one plan. With `FORGE_EXFAT_KEEP=<dir>` the edited image is kept there as
/// `every-op.exfat`, its expected tree beside it in `every-op/` (for `scripts/check-exfat.sh`).
#[test]
fn every_op_in_one_plan() {
    let tmp = TempDir::new("everyop");
    let (image, _) = build(&tmp, &Options::default());
    let mut want = read_back(&image);
    let cs = open(&image).1.cluster_size();
    let ops = [
        Op::Delete { path: LONG.into() },
        Op::Delete {
            path: "data/sub".into(),
        },
        ren("data/Ünïcode.txt", "empty/Ünïcode-moved.txt"),
        ren("empty", "Empty-Renamed"),
        put("eboot.bin", source(30, 0, 3 * cs + 7)),
        put("data/big.bin", Data::Bytes(vec![0x42; 1000])),
        mkdir("new/a/b"),
        put("new/a/b/c.bin", source(31, 0, 2 * cs)),
        put("new/zero.bin", Data::Zero(0)),
        put("top.bin", source(32, 5, 12_345)),
    ];
    edit(&image, &ops);
    want.remove(LONG);
    want.remove("data/sub/x.txt");
    let want = moved(want, "data/Ünïcode.txt", "Empty-Renamed/Ünïcode-moved.txt");
    let mut want = want;
    want.insert("eboot.bin".into(), bytes(30, 3 * cs as usize + 7));
    want.insert("data/big.bin".into(), vec![0x42; 1000]);
    want.insert("new/a/b/c.bin".into(), bytes(31, 2 * cs as usize));
    want.insert("new/zero.bin".into(), Vec::new());
    want.insert("top.bin".into(), bytes(32, 5 + 12_345)[5..].to_vec());
    assert_eq!(read_back(&image), want);
    let (_, v) = open(&image);
    assert!(!v.dirty);
    let Some(keep) = std::env::var_os("FORGE_EXFAT_KEEP") else {
        return;
    };
    let keep = PathBuf::from(keep);
    let tree = keep.join("every-op");
    let _ = std::fs::remove_dir_all(&tree);
    for (p, b) in &want {
        let at = tree.join(p);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, b).unwrap();
    }
    std::fs::create_dir_all(tree.join("data")).unwrap();
    std::fs::copy(&image, keep.join("every-op.exfat")).unwrap();
}
