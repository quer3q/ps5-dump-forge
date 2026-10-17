//! The in-place editor's read side: a Forge-built image opens with the writer's free counts;
//! another tool's geometry, an unclean image and damaged counts are findings, never a panic.

mod common;

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use common::{Synth, build};
use ps5_dump_forge_ufs2::edit::{Kind, Volume};
use ps5_dump_forge_ufs2::{Layout, Options};

const B: u64 = 65536;
const SB: u64 = 65536;

fn open(path: &Path) -> Result<Volume, Vec<String>> {
    Volume::open(&mut File::open(path).unwrap()).unwrap()
}

fn game(name: &str, opts: &Options) -> (std::path::PathBuf, Layout) {
    let mut src = Synth::new(&[
        ("eboot.bin", 3 * B + 17),
        ("sce_sys/param.json", 812),
        ("sce_module/libc.prx", 12 * B + 1),
        ("data/a/b/c.bin", 5),
        ("data/empty.bin", 0),
    ]);
    let (img, layout, _) = build(name, &mut src, opts);
    (img, layout)
}

/// Writes `bytes` at `at`, runs `f` on the image, then puts the old bytes back.
fn patched(img: &Path, at: u64, bytes: &[u8], f: impl FnOnce(Result<Volume, Vec<String>>)) {
    let mut file = OpenOptions::new().read(true).write(true).open(img).unwrap();
    let mut old = vec![0u8; bytes.len()];
    file.seek(SeekFrom::Start(at)).unwrap();
    file.read_exact(&mut old).unwrap();
    file.seek(SeekFrom::Start(at)).unwrap();
    file.write_all(bytes).unwrap();
    f(open(img));
    file.seek(SeekFrom::Start(at)).unwrap();
    file.write_all(&old).unwrap();
}

fn le32(f: &mut File, at: u64) -> u32 {
    let mut b = [0u8; 4];
    f.seek(SeekFrom::Start(at)).unwrap();
    f.read_exact(&mut b).unwrap();
    u32::from_le_bytes(b)
}

#[track_caller]
fn finding(r: Result<Volume, Vec<String>>, want: &str) {
    let f = r.expect_err("opened");
    assert!(f.len() == 1 && f[0].contains(want), "{f:?} lacks {want:?}");
}

/// Another tool's superblock, and an image not cleanly unmounted, are refused.
#[test]
fn foreign_or_unclean_superblock_is_refused() {
    let (img, _) = game("edit-foreign", &Options::default());
    let tool = "written by another tool";
    patched(&img, SB + 1372, &0x1954_0118u32.to_le_bytes(), |r| {
        finding(r, tool)
    });
    patched(&img, SB + 48, &32768u32.to_le_bytes(), |r| finding(r, tool)); // fs_bsize
    patched(&img, SB + 52, &8192u32.to_le_bytes(), |r| finding(r, tool)); // fs_fsize
    patched(&img, SB + 1312, &2u32.to_le_bytes(), |r| finding(r, tool)); // FS_DOSOFTDEP
    patched(&img, SB + 1308, &1u32.to_le_bytes(), |r| finding(r, tool)); // fs_metackhash
    patched(&img, SB + 188, &1000u32.to_le_bytes(), |r| finding(r, tool)); // fs_fpg
    // Every field the writer derives: address shifts and masks, fsbtodb, limits, layout.
    let u32s: [(u64, u32); 10] = [
        (100, 6),      // fs_fsbtodb (64 KiB blocks, 512 B sectors: 7)
        (80, 15),      // fs_bshift
        (84, 13),      // fs_fshift
        (72, !0x7FFF), // fs_bmask
        (96, 3),       // fs_fragshift
        (60, 8),       // fs_minfree
        (92, 2048),    // fs_maxbpg
        (156, 0),      // fs_cssize
        (20, 9),       // fs_dblkno
        (1320, 60),    // fs_maxsymlinklen
    ];
    for (at, v) in u32s {
        patched(&img, SB + at, &v.to_le_bytes(), |r| finding(r, tool));
    }
    patched(&img, SB + 1336, &0x7FFFu64.to_le_bytes(), |r| {
        finding(r, tool)
    }); // fs_qbmask
    patched(&img, SB + 1328, &(1u64 << 40).to_le_bytes(), |r| {
        finding(r, tool)
    }); // maxfilesize
    patched(&img, SB + 1096, &5u64.to_le_bytes(), |r| finding(r, tool)); // fs_csaddr
    // Flags that change what is on disk: SU journal, gjournal, check hashes; the same bits
    // in fs_old_flags count while FS_FLAGS_UPDATED is clear.
    for bit in [0x08u32, 0x40, 0x200] {
        patched(&img, SB + 1312, &bit.to_le_bytes(), |r| finding(r, tool));
    }
    patched(&img, SB + 211, &[0x02], |r| finding(r, tool));
    // Bits the kernel may set on an image_rw= mount pass: TRIM, ACLs, multilabel, NFSv4 ACLs.
    for bit in [0x400u32, 0x10, 0x20, 0x100, 0x410] {
        patched(&img, SB + 1312, &bit.to_le_bytes(), |r| {
            r.unwrap_or_else(|f| panic!("{bit:#x}: {f:?}"));
        });
    }
    let unclean = "not unmounted cleanly (UFS2 fs_clean)";
    patched(&img, SB + 1312, &4u32.to_le_bytes(), |r| {
        finding(r, unclean)
    }); // FS_NEEDSFSCK
    patched(&img, SB + 209, &[0], |r| finding(r, unclean));
    patched(&img, SB + 1312, &1u32.to_le_bytes(), |r| {
        finding(r, unclean)
    }); // FS_UNCLEAN
    assert!(open(&img).is_ok(), "restored");
    let _ = std::fs::remove_file(&img);
}

/// Damaged counts are findings naming the group, and no byte of the metadata panics.
#[test]
fn damaged_counts_are_findings() {
    let (img, layout) = game("edit-damaged", &Options::default());
    let fpg = layout.blocks_per_group;
    let cg = |c: u64| (c * fpg + 3) * B;
    let mut f = File::open(&img).unwrap();
    let nbfree1 = le32(&mut f, cg(1) + 28);
    let nifree0 = le32(&mut f, cg(0) + 32);
    let dblkno = layout.metadata_blocks_per_group;
    let total_nbfree = le32(&mut f, SB + 1016);
    let summary1 = le32(&mut f, dblkno * B + 16 + 4);
    let freeoff0 = u64::from(le32(&mut f, cg(0) + 96));
    drop(f);
    let one_more = |v: u32| (v + 1).to_le_bytes();
    let cg1 = "the image is damaged (UFS2 cylinder group 1:";
    patched(&img, cg(1) + 28, &one_more(nbfree1), |r| finding(r, cg1));
    let cg0 = "the image is damaged (UFS2 cylinder group 0:";
    patched(&img, cg(0) + 32, &one_more(nifree0), |r| finding(r, cg0));
    patched(&img, cg(1) + 12, &7u32.to_le_bytes(), |r| finding(r, cg1)); // cg_cgx
    // cg_initediblk 0 under group 0's allocated root and game inodes.
    let past = "is allocated past cg_initediblk 0";
    patched(&img, cg(0) + 120, &0u32.to_le_bytes(), |r| finding(r, past));
    patched(&img, cg(0) + 4, &0u32.to_le_bytes(), |r| finding(r, cg0)); // cg_magic
    // Block 0 (the boot area) marked free in group 0's map.
    patched(&img, cg(0) + freeoff0, &[0x01], |r| {
        finding(r, "metadata block 0")
    });
    patched(&img, dblkno * B + 16 + 4, &one_more(summary1), |r| {
        finding(r, cg1)
    });
    let total = "the image is damaged (UFS2 fs_cstotal";
    patched(&img, SB + 1016, &one_more(total_nbfree), |r| {
        finding(r, total)
    });
    // Every byte of the superblock and of group 0's header block set to 0xFF in turn:
    // a finding or a clean open, never a panic.
    for at in (SB..SB + 8192).chain(cg(0)..cg(0) + 4096) {
        patched(&img, at, &[0xFF], |_| {});
    }
    patched(&img, cg(2), &vec![0xA5; B as usize], |r| {
        r.expect_err("garbage cylinder group");
    });
    assert!(open(&img).is_ok(), "restored");
    // Cut short: its blocks don't fit the file.
    OpenOptions::new()
        .write(true)
        .open(&img)
        .unwrap()
        .set_len(layout.image_size - B)
        .unwrap();
    finding(open(&img), "the file is shorter than its");
    let _ = std::fs::remove_file(&img);
}

fn le64(f: &mut File, at: u64) -> u64 {
    let mut b = [0u8; 8];
    f.seek(SeekFrom::Start(at)).unwrap();
    f.read_exact(&mut b).unwrap();
    u64::from_le_bytes(b)
}

/// Nested folders, two empty ones, an empty file, files needing an indirect block, and a
/// folder whose entries fill more than one 64 KiB block.
fn nested(name: &str) -> PathBuf {
    let mut files: Vec<(String, u64)> = [
        ("eboot.bin", 3 * B + 17),
        ("sce_sys/param.json", 812),
        ("data/a/b/c.bin", 5),
        ("data/a/d.bin", 2 * B),
        ("data/empty.bin", 0),
        ("data/big.bin", 40 * B + 3),
        ("data/edge.bin", 12 * B + 1),
        ("Data/upper.bin", 9),
    ]
    .iter()
    .map(|(p, s)| (p.to_string(), *s))
    .collect();
    files.extend((0..1700).map(|i| (format!("many/file-{i:05}-padding-padding-more"), i % 3)));
    let refs: Vec<(&str, u64)> = files.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    let mut src = Synth::new(&refs);
    src.empty = vec!["data/void".into(), "z/y".into()];
    build(name, &mut src, &Options::default()).0
}

/// Lists `dir` and everything under it, each node also found by `lookup`.
fn walk(
    v: &Volume,
    f: &mut File,
    dir: &str,
    files: &mut Vec<(String, u64)>,
    empty: &mut Vec<String>,
) {
    let list = v.list(f, dir).unwrap();
    if list.is_empty() && !dir.is_empty() {
        empty.push(dir.to_string());
    }
    for (i, n) in list.into_iter().enumerate() {
        let path = match dir {
            "" => n.name.clone(),
            _ => format!("{dir}/{}", n.name),
        };
        // Every lookup lists each folder on its path whole: sample the 1700-entry one.
        if i < 50 || i % 100 == 0 {
            assert_eq!(v.lookup(f, &path).unwrap().as_ref(), Some(&n), "{path}");
        }
        match n.kind {
            Kind::Dir => walk(v, f, &path, files, empty),
            Kind::File => files.push((path, n.size)),
        }
    }
}

/// `list` and `lookup` see what the vendored reader sees, and the block maps read back the
/// reader's bytes.
#[test]
fn list_and_lookup_match_the_reader() {
    use ps5upload_fpkg::source::SourceTree;
    let img = nested("edit-list");
    let mut reader = ps5upload_fpkg::ufs2_source::Ufs2Source::open(&img).unwrap();
    let v = open(&img).unwrap();
    let mut f = File::open(&img).unwrap();
    let (mut files, mut empty) = (Vec::new(), Vec::new());
    walk(&v, &mut f, "", &mut files, &mut empty);
    files.sort();
    empty.sort();
    let mut want: Vec<(String, u64)> = reader
        .files()
        .iter()
        .map(|x| (x.path.clone(), x.size))
        .collect();
    want.sort();
    let mut want_empty = reader.empty_dirs().to_vec();
    want_empty.sort();
    assert_eq!(files, want);
    assert_eq!(empty, want_empty);
    assert_eq!(empty, ["data/void", "z/y"]);
    for (path, size) in &want {
        let n = v.lookup(&mut f, path).unwrap().unwrap();
        let inode = v.inode(&mut f, n.ino).unwrap();
        let map = v.block_map(&mut f, &inode).unwrap();
        // Up to 12 blocks direct; past that one single-indirect block.
        let blocks = size.div_ceil(B);
        assert_eq!(map.indirect.len(), usize::from(blocks > 12), "{path}");
        assert_eq!(map.owned(), blocks + map.indirect.len() as u64, "{path}");
        for lbn in 0..blocks {
            let p = map.physical(lbn).unwrap();
            let len = (size - lbn * B).min(B) as usize;
            let mut got = vec![0u8; len];
            f.seek(SeekFrom::Start(p * B)).unwrap();
            f.read_exact(&mut got).unwrap();
            assert!(
                got == reader.read_range(path, lbn * B, len).unwrap(),
                "{path} block {lbn}"
            );
        }
        assert_eq!(map.physical(blocks), None);
    }
    // The big folder spans two 64 KiB blocks; folder link counts are 2 + subfolders.
    let many = v.lookup(&mut f, "many").unwrap().unwrap();
    assert!(
        many.size > B && many.size.is_multiple_of(512),
        "{}",
        many.size
    );
    assert_eq!(v.list(&mut f, "many").unwrap().len(), 1700);
    assert_eq!(v.lookup(&mut f, "data").unwrap().unwrap().nlink, 4);
    assert_eq!(v.root(&mut f).unwrap().nlink, 2 + 5);
    // Case-sensitive, missing, through a file, bad paths.
    assert_eq!(v.lookup(&mut f, "EBOOT.BIN").unwrap(), None);
    assert!(v.lookup(&mut f, "Data/upper.bin").unwrap().is_some());
    assert_eq!(v.lookup(&mut f, "data/upper.bin").unwrap(), None);
    assert_eq!(v.lookup(&mut f, "nope").unwrap(), None);
    assert_eq!(v.lookup(&mut f, "eboot.bin/x").unwrap(), None);
    for bad in ["", "data/", "/data", "data//a"] {
        let e = v.lookup(&mut f, bad).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
    }
    let e = v.list(&mut f, "eboot.bin").unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::NotADirectory);
    assert_eq!(
        v.list(&mut f, "nope").unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let _ = std::fs::remove_file(&img);
}

/// Opens the patched image and runs `f` on the volume and the file (the patch must open).
fn edited(img: &Path, at: u64, bytes: &[u8], f: impl FnOnce(&Volume, &mut File)) {
    patched(img, at, bytes, |r| {
        let v = r.unwrap_or_else(|e| panic!("open at {at}: {e:?}"));
        f(&v, &mut File::open(img).unwrap())
    });
}

#[track_caller]
fn damage<T: std::fmt::Debug>(r: std::io::Result<T>, want: &str) {
    let e = r.expect_err("read");
    assert!(e.to_string().contains(want), "{e} lacks {want:?}");
}

/// A damaged directory entry or block pointer is a finding naming the inode, not a panic.
#[test]
fn damaged_entries_and_pointers_are_findings() {
    let img = nested("edit-damaged-tree");
    let v = open(&img).unwrap();
    let mut f = File::open(&img).unwrap();
    let fs_size = le64(&mut f, SB + 1080);
    let root = v.inode_offset(2);
    let dir = le64(&mut f, root + 112) * B; // the root's first block: ".", "..", "Data", ...
    let big = v.lookup(&mut f, "data/big.bin").unwrap().unwrap().ino;
    let big = v.inode_offset(big);
    let small = v.inode_offset(v.lookup(&mut f, "data/a/b/c.bin").unwrap().unwrap().ino);
    let (db0, ib0) = (le64(&mut f, big + 112), le64(&mut f, big + 208));
    let b = v.lookup(&mut f, "data/a/b").unwrap().unwrap();
    assert_eq!(b.size, 512);
    let b_dir = le64(&mut f, v.inode_offset(b.ino) + 112) * B;
    drop(f);
    let entry = "the image is damaged (UFS2 inode 2: directory entry at byte 24";
    let list = |want: &'static str| move |v: &Volume, f: &mut File| damage(v.list(f, ""), want);
    edited(&img, dir + 24 + 4, &3u16.to_le_bytes(), list(entry)); // d_reclen not 4-aligned
    edited(&img, dir + 24 + 4, &4u16.to_le_bytes(), list(entry)); // shorter than the entry
    edited(&img, dir + 24 + 4, &512u16.to_le_bytes(), list(entry)); // past the chunk
    edited(&img, dir + 24 + 7, &[200], list(entry)); // d_namlen past d_reclen
    edited(&img, dir + 24 + 8, b"/", list(entry)); // a '/' in the name
    edited(&img, dir, &3u32.to_le_bytes(), list("'.' or '..' wrong"));
    edited(
        &img,
        dir + 12,
        &0u32.to_le_bytes(),
        list("'.' or '..' missing"),
    );
    edited(
        &img,
        dir + 24,
        &u32::MAX.to_le_bytes(),
        list("inode 4294967295: out of range"),
    );
    edited(&img, dir + 24 + 6, &[1], list("d_type 1"));
    edited(&img, dir + 24 + 8, &[0xFF], list("name not UTF-8"));
    // "." taking the whole 512-byte folder: ".." is never seen.
    edited(&img, b_dir + 4, &512u16.to_le_bytes(), |v, f| {
        damage(v.list(f, "data/a/b"), "'.' or '..' missing");
        damage(v.lookup(f, "data/a/b/c.bin"), "'.' or '..' missing");
    });
    // Two entries named "data" ("Data" renamed): the listing and every lookup in the folder
    // refuse it, the first match included.
    edited(&img, dir + 24 + 8, b"d", |v, f| {
        damage(v.list(f, ""), "\"data\" listed twice");
        damage(v.lookup(f, "data"), "listed twice");
        damage(v.lookup(f, "eboot.bin"), "listed twice");
    });
    // A bad record after the match: root entries are ".", "..", "Data", "data",
    // "eboot.bin", "many", "sce_sys", "z" (at byte 108).
    edited(&img, dir + 108 + 4, &3u16.to_le_bytes(), |v, f| {
        damage(v.lookup(f, "Data"), "directory entry at byte 108");
    });
    // Block pointers: past fs_size (direct and in the indirect block), metadata, free,
    // past the size, twice; di_blocks off by one block.
    let map = |want: &'static str, path: &'static str| {
        move |v: &Volume, f: &mut File| {
            let n = v.lookup(f, path).unwrap().unwrap();
            let i = v.inode(f, n.ino).unwrap();
            damage(v.block_map(f, &i), want);
        }
    };
    let big_bin = "data/big.bin";
    edited(
        &img,
        big + 112 + 24,
        &fs_size.to_le_bytes(),
        map("past fs_size", big_bin),
    );
    edited(
        &img,
        ib0 * B,
        &(fs_size + 5).to_le_bytes(),
        map("past fs_size", big_bin),
    );
    edited(
        &img,
        big + 112,
        &1u64.to_le_bytes(),
        map("is group metadata", big_bin),
    );
    edited(
        &img,
        big + 112,
        &(fs_size - 1).to_le_bytes(),
        map("free in the block map", big_bin),
    );
    edited(
        &img,
        big + 120,
        &db0.to_le_bytes(),
        map("mapped twice", big_bin),
    );
    edited(
        &img,
        small + 120,
        &db0.to_le_bytes(),
        map("past its size", "data/a/b/c.bin"),
    );
    let blocks = (41 + 1 + 1) * 128u64; // 41 data + 1 indirect, one too many
    edited(
        &img,
        big + 24,
        &blocks.to_le_bytes(),
        map("di_blocks", big_bin),
    );
    edited(&img, root + 112, &0u64.to_le_bytes(), |v, f| {
        damage(v.list(f, ""), "inode 2: di_blocks")
    });
    assert!(
        v.lookup(&mut File::open(&img).unwrap(), big_bin).is_ok(),
        "restored"
    );
    let _ = std::fs::remove_file(&img);
}
