//! Byte images of the superblock, cylinder group headers and inodes: FreeBSD `struct fs`
//! (1376 B), `struct cg` (168 B) and `struct ufs2_dinode`, values as newfs writes them.

use crate::geom::{BSIZE, CBLKNO, CG_HDR, CONTIGSUMSIZE, Geom, IBLKNO, INOPB, NINDIR, SBLKNO};

pub const FS_UFS2_MAGIC: u32 = 0x1954_0119;
pub const CG_MAGIC: u32 = 0x0009_0255;
pub const SBLOCK_UFS2: u64 = 65536;
pub const SBLOCKSIZE: usize = 8192;
pub const VOLNAME_AT: usize = 680;
pub const VOLNAME_LEN: usize = 32;
/// `fs_old_flags`: flags live in `fs_flags` (FS_FLAGS_UPDATED).
const FS_FLAGS_UPDATED: u8 = 0x80;
/// newfs switches to space optimisation when minfree < 8%.
const FS_OPTSPACE: i32 = 1;
/// All on-disk times. Fixed, so the same tree gives the same bytes (newfs -R uses it too).
pub const TIME: i64 = 1_000_000_000;

pub const IFDIR: u16 = 0o040000;
pub const IFREG: u16 = 0o100000;

/// Per-group summary (`struct csum`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Csum {
    pub ndir: u64,
    pub nbfree: u64,
    pub nifree: u64,
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// The superblock (8192 bytes). `actual` is `fs_sblockactualloc`, this copy's byte offset.
/// The in-place editor builds one too, to check an image's geometry against the writer's.
pub fn superblock(
    g: &Geom,
    total: &Csum,
    fs_id: u32,
    volname: &[u8; VOLNAME_LEN],
    actual: u64,
) -> Vec<u8> {
    let mut b = vec![0u8; SBLOCKSIZE];
    let s32 = |b: &mut [u8], at: usize, v: i64| put32(b, at, v as i32 as u32);
    s32(&mut b, 8, SBLKNO as i64); // fs_sblkno
    s32(&mut b, 12, CBLKNO as i64); // fs_cblkno
    s32(&mut b, 16, IBLKNO as i64); // fs_iblkno
    s32(&mut b, 20, g.dblkno as i64); // fs_dblkno
    s32(&mut b, 44, g.ncg as i64); // fs_ncg
    s32(&mut b, 48, BSIZE as i64); // fs_bsize
    s32(&mut b, 52, BSIZE as i64); // fs_fsize
    s32(&mut b, 56, 1); // fs_frag
    s32(&mut b, 60, 0); // fs_minfree
    s32(&mut b, 72, !(BSIZE as i64 - 1)); // fs_bmask
    s32(&mut b, 76, !(BSIZE as i64 - 1)); // fs_fmask
    s32(&mut b, 80, 16); // fs_bshift
    s32(&mut b, 84, 16); // fs_fshift
    s32(&mut b, 88, CONTIGSUMSIZE as i64); // fs_maxcontig
    s32(&mut b, 92, NINDIR as i64); // fs_maxbpg = MAXBLKPG(bsize)
    s32(&mut b, 96, 0); // fs_fragshift
    s32(&mut b, 100, 7); // fs_fsbtodb: log2(fsize / DEV_BSIZE), DEV_BSIZE = 512
    s32(&mut b, 104, SBLOCKSIZE as i64); // fs_sbsize
    s32(&mut b, 116, NINDIR as i64); // fs_nindir
    s32(&mut b, 120, INOPB as i64); // fs_inopb
    s32(&mut b, 128, FS_OPTSPACE as i64); // fs_optim
    s32(&mut b, 144, crate::ondisk::TIME); // fs_id[0]
    put32(&mut b, 148, fs_id); // fs_id[1]
    s32(&mut b, 156, (g.csblocks * BSIZE) as i64); // fs_cssize
    s32(&mut b, 160, BSIZE as i64); // fs_cgsize = fragroundup(CGSIZE)
    s32(&mut b, 184, g.ipg as i64); // fs_ipg
    s32(&mut b, 188, g.fpg as i64); // fs_fpg
    b[209] = 1; // fs_clean
    b[VOLNAME_AT..VOLNAME_AT + VOLNAME_LEN].copy_from_slice(volname); // fs_volname
    b[211] = FS_FLAGS_UPDATED; // fs_old_flags
    s32(&mut b, 860, BSIZE as i64); // fs_maxbsize
    put64(&mut b, 872, g.size); // fs_providersize
    put64(&mut b, 992, actual); // fs_sblockactualloc
    put64(&mut b, 1000, SBLOCK_UFS2); // fs_sblockloc
    put64(&mut b, 1008, total.ndir); // fs_cstotal.cs_ndir
    put64(&mut b, 1016, total.nbfree); // .cs_nbfree
    put64(&mut b, 1024, total.nifree); // .cs_nifree (cs_nffree, cs_numclusters stay 0)
    put64(&mut b, 1072, TIME as u64); // fs_time
    put64(&mut b, 1080, g.size); // fs_size
    put64(&mut b, 1088, g.dsize()); // fs_dsize
    put64(&mut b, 1096, g.dblkno); // fs_csaddr = cgdmin(0)
    put32(&mut b, 1196, 16384); // fs_avgfilesize (AVFILESIZ)
    put32(&mut b, 1200, 64); // fs_avgfpdir (AFPDIR)
    // fs_ckhash, fs_metackhash, fs_flags = 0: no soft updates, no check hashes.
    s32(&mut b, 1316, CONTIGSUMSIZE as i64); // fs_contigsumsize
    s32(&mut b, 1320, 120); // fs_maxsymlinklen = (NDADDR + NIADDR) * 8
    put64(&mut b, 1328, maxfilesize()); // fs_maxfilesize
    put64(&mut b, 1336, BSIZE - 1); // fs_qbmask
    put64(&mut b, 1344, BSIZE - 1); // fs_qfmask
    put32(&mut b, 1372, FS_UFS2_MAGIC); // fs_magic
    b
}

fn maxfilesize() -> u64 {
    let mut m = BSIZE * 12 - 1;
    let mut per = BSIZE;
    for _ in 0..3 {
        per *= NINDIR;
        m += per;
    }
    m
}

/// `struct fsrecovery`, the 20 bytes before the superblock: how fsck finds the backups.
pub fn fsrecovery(g: &Geom) -> [u8; 20] {
    let mut b = [0u8; 20];
    put32(&mut b, 0, FS_UFS2_MAGIC);
    put32(&mut b, 4, 7); // fsr_fsbtodb
    put32(&mut b, 8, SBLKNO as u32);
    put32(&mut b, 12, g.fpg as u32);
    put32(&mut b, 16, g.ncg as u32);
    b
}

/// The cylinder group header plus its maps, one block. The `used_inodes` and `used_blocks`
/// taken are always the group's first ones.
pub fn cylinder_group(g: &Geom, c: u64, used_inodes: u64, used_blocks: u64, cs: &Csum) -> Vec<u8> {
    let mut b = vec![0u8; BSIZE as usize];
    let ndblk = g.cg_blocks(c);
    let iusedoff = CG_HDR;
    let freeoff = iusedoff + g.ipg.div_ceil(8);
    let nextfree = freeoff + g.fpg.div_ceil(8);
    let clustersumoff = nextfree.next_multiple_of(4) - 4;
    let clusteroff = clustersumoff + (CONTIGSUMSIZE + 1) * 4;
    let nextfreeoff = clusteroff + g.fpg.div_ceil(8);

    put32(&mut b, 4, CG_MAGIC);
    put32(&mut b, 12, c as u32); // cg_cgx
    put32(&mut b, 20, ndblk as u32); // cg_ndblk
    put32(&mut b, 24, cs.ndir as u32); // cg_cs
    put32(&mut b, 28, cs.nbfree as u32);
    put32(&mut b, 32, cs.nifree as u32);
    put32(&mut b, 92, iusedoff as u32);
    put32(&mut b, 96, freeoff as u32);
    put32(&mut b, 100, nextfreeoff as u32);
    put32(&mut b, 104, clustersumoff as u32);
    put32(&mut b, 108, clusteroff as u32);
    put32(&mut b, 112, ndblk as u32); // cg_nclusterblks (frag = 1)
    put32(&mut b, 116, g.ipg as u32); // cg_niblk
    // cg_initediblk: like newfs, only what is in use, rounded to an inode block, at least two.
    let inited = used_inodes
        .next_multiple_of(INOPB)
        .max(2 * INOPB)
        .min(g.ipg);
    put32(&mut b, 120, inited as u32);
    put64(&mut b, 136, TIME as u64); // cg_time

    let mut set = |base: u64, bit: u64| {
        let at = (base + bit / 8) as usize;
        b[at] |= 1 << (bit % 8);
    };
    for i in 0..used_inodes {
        set(iusedoff, i);
    }
    // Free: the blocks in front of a backup superblock, then everything past the data.
    let first_free = g.cg_data_start(c) - c * g.fpg + used_blocks;
    let free_ranges = [(0, if c > 0 { SBLKNO } else { 0 }), (first_free, ndblk)];
    let mut sum = [0u32; CONTIGSUMSIZE as usize + 1];
    for (lo, hi) in free_ranges {
        for d in lo..hi {
            set(freeoff, d);
            set(clusteroff, d);
        }
        if hi > lo {
            sum[((hi - lo).min(CONTIGSUMSIZE)) as usize] += 1;
        }
    }
    // Entry 0 is never used: `cg_clustersumoff` points one word early, so it overlaps the
    // tail of the block map (newfs and fsck lay it out the same way). Don't write it.
    for (i, n) in sum.iter().enumerate().skip(1) {
        put32(&mut b, (clustersumoff + 4 * i as u64) as usize, *n);
    }
    b
}

/// One inode, 256 bytes.
pub struct Dinode {
    pub mode: u16,
    pub nlink: u16,
    pub size: u64,
    /// In DEV_BSIZE (512-byte) units.
    pub blocks: u64,
    pub db: [u64; 12],
    pub ib: [u64; 3],
    pub dirdepth: u32,
}

impl Dinode {
    pub fn write(&self, b: &mut [u8]) {
        b[0..2].copy_from_slice(&self.mode.to_le_bytes());
        b[2..4].copy_from_slice(&self.nlink.to_le_bytes());
        put64(b, 16, self.size);
        put64(b, 24, self.blocks);
        for at in [32, 40, 48, 56] {
            put64(b, at, TIME as u64); // atime, mtime, ctime, birthtime
        }
        for (i, p) in self.db.iter().enumerate() {
            put64(b, 112 + 8 * i, *p);
        }
        for (i, p) in self.ib.iter().enumerate() {
            put64(b, 208 + 8 * i, *p);
        }
        put32(b, 240, self.dirdepth);
    }
}
