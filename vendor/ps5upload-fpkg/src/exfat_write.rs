//! An exFAT image writer: a game folder becomes one `.exfat` file ShadowMountPlus can mount.
//!
//! Written from the published exFAT specification (Microsoft, "exFAT file system
//! specification"), independently of any other tool's code. It writes a read-mostly volume in
//! one forward pass, with nothing held in memory but the layout:
//!
//! ```text
//! sectors 0..11    main boot region (boot sector, 8 extended, OEM, reserved, checksum)
//! sectors 12..23   backup boot region, identical
//! sector  128      the FAT (one, with a real chain for every allocation)
//! cluster heap     allocation bitmap, up-case table, root directory, every other directory,
//!                  then every file's data, each contiguous
//! ```
//!
//! Every allocation is contiguous AND has its FAT chain written, with `NoFatChain` left clear:
//! a driver that follows chains and one that trusts contiguity both read it.
//!
//! The volume keeps free space for a read-write mount (ShadowMountPlus `image_rw=`: saves,
//! small patches): 0.5% of the data, at least 64 MiB and at most 512 MiB, plus whatever the
//! caller asks for. Nothing is ever written behind the cursor, so the image can go straight
//! into a stream (a `.ffpfsc` container) as well as a file: [`plan_exfat`] lays it out and
//! checks every name without reading file data, [`write_exfat`] writes it in order. (Both ideas,
//! the spare and the plan/stream split, are PS5 Dump Forge's.)
//!
//! What is deliberately not done: no timestamps from the source (a fixed one, so the same
//! folder always gives the same image), no volume label, no `ampr_emu.index` generation here
//! (the image is the tree it is given; the engine's image build hands it a libSceAmpr game with
//! a regenerated index, `AmprIndexed`).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::source::SourceTree;
use crate::{format_err, Error, Result};

const SECTOR: u64 = 512;
/// Where the FAT starts, in sectors. The specification asks for at least 24 (after the two
/// boot regions); 128 leaves it on a 64 KiB boundary like common formatters do.
const FAT_OFFSET_SECTORS: u64 = 128;
const FIRST_CLUSTER: u32 = 2;
const FAT_END: u32 = 0xFFFF_FFFF;
/// The largest cluster count a volume may declare.
const MAX_CLUSTERS: u64 = 0xFFFF_FFF5;
const ENTRY: usize = 32;
const NAME_UNITS_PER_ENTRY: usize = 15;
const MAX_NAME_UNITS: usize = 255;
const COPY_CHUNK: usize = 1024 * 1024;
/// 2024-01-01 00:00:00, in the directory entry's packed form.
const FIXED_TIMESTAMP: u32 = ((2024 - 1980) << 25) | (1 << 21) | (1 << 16);
/// "the time is UTC": offset valid, zero quarter-hours.
const UTC_OFFSET: u8 = 0x80;
const ATTR_DIRECTORY: u16 = 0x10;
const ATTR_ARCHIVE: u16 = 0x20;

/// What a build produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExfatBuilt {
    /// Size of the image file.
    pub image_bytes: u64,
    pub files: usize,
    pub directories: usize,
    pub cluster_size: u32,
}

/// Free space a volume keeps for a read-write mount, beyond `extra`: 0.5% of the data, clamped
/// to 64..=512 MiB.
pub fn spare_bytes(data_bytes: u64, extra: u64) -> u64 {
    (data_bytes / 200)
        .clamp(64 << 20, 512 << 20)
        .saturating_add(extra)
}

/// The cluster size images get: 64 KiB, what every game image in circulation uses and the
/// block size ShadowMountPlus attaches an image with. (32 KiB also mounted on a CFI-1115A,
/// FW 13.60; there is no reason to differ from the images known to run games.)
pub const PS5_CLUSTER: u32 = 64 << 10;

/// The cluster size a volume holding `data_bytes` gets: 64 KiB, raised only if that many
/// clusters would not fit the format (past roughly 128 TiB).
pub fn cluster_size_for(data_bytes: u64) -> u32 {
    let mut size = u64::from(PS5_CLUSTER);
    while data_bytes / size > MAX_CLUSTERS / 2 {
        size *= 2;
    }
    size as u32
}

/// The checksum of the boot region's first eleven sectors (the twelfth holds it).
pub fn boot_checksum(sectors: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for (i, b) in sectors.iter().enumerate() {
        // VolumeFlags and PercentInUse change while a volume is in use, so they are left out.
        if i == 106 || i == 107 || i == 112 {
            continue;
        }
        sum = sum.rotate_right(1).wrapping_add(u32::from(*b));
    }
    sum
}

fn checksum16(sum: u16, b: u8) -> u16 {
    sum.rotate_right(1).wrapping_add(u16::from(b))
}

/// The up-case table the specification recommends, in its compressed on-disk form (a run of
/// units that map to themselves is `0xFFFF, count`). Its checksum is the one the
/// specification gives for it, `0xE619D30D`.
///
/// It has to be this table. A first version of this writer stored its own (the same idea,
/// generated from Unicode's current case mappings and packed tighter): macOS checked and
/// mounted that volume without complaint, and the PS5 (CFI-1115A, FW 13.60, ShadowMountPlus
/// 1.7) refused it at `nmount` with "Invalid argument". With this table and nothing else
/// changed, the same console mounts the image.
const UPCASE_TABLE: &[u8; 5836] = include_bytes!("exfat_upcase.bin");

/// [`UPCASE_TABLE`] expanded: the upper case of every UTF-16 unit.
fn upcase_map() -> &'static [u16] {
    static MAP: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let units: Vec<u16> = UPCASE_TABLE
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let mut out: Vec<u16> = Vec::with_capacity(0x10000);
        let mut i = 0;
        while i < units.len() && out.len() < 0x10000 {
            if units[i] == 0xFFFF && i + 1 < units.len() {
                let start = out.len();
                let run = usize::from(units[i + 1]).min(0x10000 - start);
                out.extend((start..start + run).map(|u| u as u16));
                i += 2;
            } else {
                out.push(units[i]);
                i += 1;
            }
        }
        // Units past the table's end map to themselves.
        let from = out.len();
        out.extend((from..0x10000).map(|u| u as u16));
        out
    })
}

/// Upper case of one UTF-16 unit, as this volume's up-case table defines it.
fn upcase_unit(u: u16) -> u16 {
    upcase_map()[usize::from(u)]
}

fn upcase_table() -> Vec<u8> {
    UPCASE_TABLE.to_vec()
}

fn table_checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |s, b| s.rotate_right(1).wrapping_add(u32::from(*b)))
}

fn name_hash(units: &[u16]) -> u16 {
    let mut h: u16 = 0;
    for u in units {
        for b in upcase_unit(*u).to_le_bytes() {
            h = checksum16(h, b);
        }
    }
    h
}

/// A name exFAT can hold, as UTF-16.
fn name_units(name: &str) -> Result<Vec<u16>> {
    let units: Vec<u16> = name.encode_utf16().collect();
    if units.is_empty() || units.len() > MAX_NAME_UNITS {
        return format_err(format!(
            "\"{name}\" cannot be stored: an exFAT name is 1 to 255 characters"
        ));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| (*c as u32) < 0x20 || "\"*/:<>?\\|".contains(*c))
    {
        return format_err(format!(
            "\"{name}\" cannot be stored: exFAT does not allow {bad:?} in a name"
        ));
    }
    // macOS hands out decomposed names; a game looks its files up by the composed spelling, and
    // exFAT compares units, not canonical equivalents. The UFS2 writer refuses these too.
    if !unicode_normalization::is_nfc(name) {
        return format_err(format!(
            "\"{name}\" cannot be stored: the name is decomposed (NFD, usually from a Mac), so \
             the game could not find the file. Rename it, or copy the game from another computer"
        ));
    }
    Ok(units)
}

/// Why exFAT cannot hold `name`, for a caller that wants to refuse it before planning an image
/// (the image browser's rename and new folder).
pub(crate) fn check_name(name: &str) -> Result<()> {
    name_units(name).map(|_| ())
}

#[derive(Default)]
struct Node {
    name: String,
    units: Vec<u16>,
    is_dir: bool,
    /// Path in the source, for a file.
    src: String,
    size: u64,
    /// Children by up-cased name, so order is stable and case clashes are caught.
    children: BTreeMap<Vec<u16>, usize>,
    first_cluster: u32,
    clusters: u64,
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn child(&mut self, parent: usize, name: &str, is_dir: bool) -> Result<usize> {
        let units = name_units(name)?;
        let key: Vec<u16> = units.iter().map(|u| upcase_unit(*u)).collect();
        if let Some(&at) = self.nodes[parent].children.get(&key) {
            let existing = &self.nodes[at];
            if existing.is_dir != is_dir || existing.name != name {
                return format_err(format!(
                    "\"{}\" and \"{name}\" differ only by letter case (or one is a folder): exFAT holds one of them",
                    existing.name
                ));
            }
            if !is_dir {
                return format_err(format!("\"{name}\" is listed twice"));
            }
            return Ok(at);
        }
        let at = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_string(),
            units,
            is_dir,
            ..Node::default()
        });
        self.nodes[parent].children.insert(key, at);
        Ok(at)
    }

    fn dir_for(&mut self, path: &str) -> Result<usize> {
        let mut at = 0;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            at = self.child(at, part, true)?;
        }
        Ok(at)
    }

    /// Bytes of directory entries `dir` needs.
    fn dir_bytes(&self, dir: usize) -> u64 {
        let own: usize = self.nodes[dir]
            .children
            .values()
            .map(|&c| (2 + self.nodes[c].units.len().div_ceil(NAME_UNITS_PER_ENTRY)) * ENTRY)
            .sum();
        // The root also carries the bitmap and up-case table entries.
        (own + if dir == 0 { 2 * ENTRY } else { 0 }) as u64
    }
}

fn clusters_for(bytes: u64, cluster: u64) -> u64 {
    bytes.div_ceil(cluster)
}

struct Layout {
    cluster: u64,
    cluster_count: u64,
    fat_sectors: u64,
    heap_sectors: u64,
    bitmap_clusters: u64,
    bitmap_bytes: u64,
    upcase: Vec<u8>,
    upcase_cluster: u32,
    /// Directories in the order their clusters are laid down (root first), then files.
    dirs: Vec<usize>,
    files: Vec<usize>,
}

fn layout(tree: &mut Tree, cluster_override: Option<u32>, spare: u64) -> Result<Layout> {
    let data_bytes: u64 = tree.nodes.iter().map(|n| n.size).sum();
    let cluster = u64::from(cluster_override.unwrap_or_else(|| cluster_size_for(data_bytes)));
    if !cluster.is_power_of_two() || !(SECTOR..=32 << 20).contains(&cluster) {
        return format_err(format!("{cluster} is not a usable exFAT cluster size"));
    }
    let upcase = upcase_table();

    // Directories in pre-order (a folder before what is inside it), then files in the same
    // order, so the volume's metadata is at its front.
    let mut dirs: Vec<usize> = Vec::new();
    let mut files: Vec<usize> = Vec::new();
    let mut stack = vec![0usize];
    while let Some(at) = stack.pop() {
        dirs.push(at);
        let kids: Vec<usize> = tree.nodes[at].children.values().copied().collect();
        for &k in kids.iter().rev() {
            if tree.nodes[k].is_dir {
                stack.push(k);
            }
        }
        files.extend(kids.iter().copied().filter(|&k| !tree.nodes[k].is_dir));
    }

    let upcase_clusters = clusters_for(upcase.len() as u64, cluster);
    let mut used = upcase_clusters;
    for &d in &dirs {
        let c = clusters_for(tree.dir_bytes(d), cluster).max(1);
        tree.nodes[d].clusters = c;
        tree.nodes[d].size = c * cluster;
        used += c;
    }
    for &f in &files {
        let c = clusters_for(tree.nodes[f].size, cluster);
        tree.nodes[f].clusters = c;
        used += c;
    }

    let free = clusters_for(spare, cluster);
    // The bitmap covers every cluster, its own included.
    let mut bitmap_clusters = 1u64;
    loop {
        let need = clusters_for((used + free + bitmap_clusters).div_ceil(8), cluster).max(1);
        if need == bitmap_clusters {
            break;
        }
        bitmap_clusters = need;
    }
    let mut cluster_count = used + free + bitmap_clusters;
    // The smallest volume the format allows is 1 MiB; free clusters make up the rest.
    let fat_sectors_for = |count: u64| ((count + 2) * 4).div_ceil(SECTOR);
    let heap_for = |count: u64| {
        (FAT_OFFSET_SECTORS + fat_sectors_for(count)).next_multiple_of(cluster / SECTOR)
    };
    while (heap_for(cluster_count) * SECTOR) + cluster_count * cluster < 1 << 20 {
        cluster_count += 1;
    }
    if cluster_count > MAX_CLUSTERS {
        return format_err(format!(
            "{cluster_count} clusters of {cluster} bytes is more than exFAT can address"
        ));
    }
    // More clusters need a longer bitmap only past a cluster boundary, which the loop above
    // already settled for `used`; the padding clusters are few and stay inside it.
    let bitmap_bytes = cluster_count.div_ceil(8);
    if clusters_for(bitmap_bytes, cluster) > bitmap_clusters {
        return format_err("exFAT bitmap sizing did not settle");
    }

    // Hand out cluster numbers in the order things are written.
    let mut next = u64::from(FIRST_CLUSTER) + bitmap_clusters;
    let upcase_cluster = next as u32;
    next += upcase_clusters;
    for &at in dirs.iter().chain(files.iter()) {
        let n = &mut tree.nodes[at];
        n.first_cluster = if n.clusters == 0 { 0 } else { next as u32 };
        next += n.clusters;
    }

    Ok(Layout {
        cluster,
        cluster_count,
        fat_sectors: fat_sectors_for(cluster_count),
        heap_sectors: heap_for(cluster_count),
        bitmap_clusters,
        bitmap_bytes,
        upcase,
        upcase_cluster,
        dirs,
        files,
    })
}

fn boot_region(l: &Layout, root_cluster: u32, used_clusters: u64) -> Vec<u8> {
    let mut r = vec![0u8; 12 * SECTOR as usize];
    let volume_sectors = l.heap_sectors + l.cluster_count * (l.cluster / SECTOR);
    {
        let b = &mut r[..SECTOR as usize];
        b[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        b[3..11].copy_from_slice(b"EXFAT   ");
        b[72..80].copy_from_slice(&volume_sectors.to_le_bytes());
        b[80..84].copy_from_slice(&(FAT_OFFSET_SECTORS as u32).to_le_bytes());
        b[84..88].copy_from_slice(&(l.fat_sectors as u32).to_le_bytes());
        b[88..92].copy_from_slice(&(l.heap_sectors as u32).to_le_bytes());
        b[92..96].copy_from_slice(&(l.cluster_count as u32).to_le_bytes());
        b[96..100].copy_from_slice(&root_cluster.to_le_bytes());
        // A serial that follows from the layout, so the same folder gives the same image.
        let serial = (l.cluster_count as u32) ^ ((l.cluster as u32) << 4) ^ 0x5055_5035;
        b[100..104].copy_from_slice(&serial.to_le_bytes());
        b[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        b[108] = SECTOR.trailing_zeros() as u8;
        b[109] = (l.cluster / SECTOR).trailing_zeros() as u8;
        b[110] = 1; // one FAT
        b[111] = 0x80;
        b[112] = ((used_clusters * 100) / l.cluster_count).min(100) as u8;
        b[510] = 0x55;
        b[511] = 0xAA;
    }
    for sector in 1..=8usize {
        let end = (sector + 1) * SECTOR as usize;
        r[end - 4..end].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
    }
    let sum = boot_checksum(&r[..11 * SECTOR as usize]).to_le_bytes();
    for chunk in r[11 * SECTOR as usize..].as_chunks_mut::<4>().0 {
        *chunk = sum;
    }
    r
}

/// One file or folder's directory entries: File, Stream Extension, then its name.
fn entry_set(n: &Node) -> Vec<u8> {
    let name_entries = n.units.len().div_ceil(NAME_UNITS_PER_ENTRY);
    let mut set = vec![0u8; (2 + name_entries) * ENTRY];
    set[0] = 0x85;
    set[1] = (1 + name_entries) as u8;
    let attr = if n.is_dir {
        ATTR_DIRECTORY
    } else {
        ATTR_ARCHIVE
    };
    set[4..6].copy_from_slice(&attr.to_le_bytes());
    for at in [8, 12, 16] {
        set[at..at + 4].copy_from_slice(&FIXED_TIMESTAMP.to_le_bytes());
    }
    set[22] = UTC_OFFSET;
    set[23] = UTC_OFFSET;
    set[24] = UTC_OFFSET;

    let s = &mut set[ENTRY..2 * ENTRY];
    s[0] = 0xC0;
    s[1] = 0x01; // allocation possible; the FAT chain is real, so NoFatChain stays clear
    s[3] = n.units.len() as u8;
    s[4..6].copy_from_slice(&name_hash(&n.units).to_le_bytes());
    s[8..16].copy_from_slice(&n.size.to_le_bytes());
    s[20..24].copy_from_slice(&n.first_cluster.to_le_bytes());
    s[24..32].copy_from_slice(&n.size.to_le_bytes());

    for (i, chunk) in n.units.chunks(NAME_UNITS_PER_ENTRY).enumerate() {
        let e = &mut set[(2 + i) * ENTRY..(3 + i) * ENTRY];
        e[0] = 0xC1;
        for (j, u) in chunk.iter().enumerate() {
            e[2 + j * 2..4 + j * 2].copy_from_slice(&u.to_le_bytes());
        }
    }

    let mut sum: u16 = 0;
    for (i, b) in set.iter().enumerate() {
        if i == 2 || i == 3 {
            continue;
        }
        sum = checksum16(sum, *b);
    }
    set[2..4].copy_from_slice(&sum.to_le_bytes());
    set
}

fn directory_bytes(tree: &Tree, dir: usize, l: &Layout) -> Vec<u8> {
    let mut out = Vec::with_capacity(tree.nodes[dir].size as usize);
    if dir == 0 {
        let mut bitmap = [0u8; ENTRY];
        bitmap[0] = 0x81;
        bitmap[20..24].copy_from_slice(&FIRST_CLUSTER.to_le_bytes());
        bitmap[24..32].copy_from_slice(&l.bitmap_bytes.to_le_bytes());
        out.extend_from_slice(&bitmap);
        let mut up = [0u8; ENTRY];
        up[0] = 0x82;
        up[4..8].copy_from_slice(&table_checksum(&l.upcase).to_le_bytes());
        up[20..24].copy_from_slice(&l.upcase_cluster.to_le_bytes());
        up[24..32].copy_from_slice(&(l.upcase.len() as u64).to_le_bytes());
        out.extend_from_slice(&up);
    }
    for &c in tree.nodes[dir].children.values() {
        out.extend_from_slice(&entry_set(&tree.nodes[c]));
    }
    // The rest of the directory's clusters are zero: an entry type of 0 ends the listing.
    out.resize(tree.nodes[dir].size as usize, 0);
    out
}

fn zeros(w: &mut impl Write, mut n: u64) -> std::io::Result<()> {
    let block = [0u8; 64 * 1024];
    while n > 0 {
        let take = n.min(block.len() as u64) as usize;
        w.write_all(&block[..take])?;
        n -= take as u64;
    }
    Ok(())
}

/// An exFAT image laid out and checked, before any file data is read.
pub struct ExfatPlan {
    tree: Tree,
    l: Layout,
    used: u64,
    /// The image's exact size.
    pub image_bytes: u64,
    /// File-data bytes `write_exfat` copies (its progress total).
    pub data_bytes: u64,
    /// Free space in the image, for a read-write mount.
    pub free_bytes: u64,
    pub files: usize,
    pub directories: usize,
    pub cluster_size: u32,
}

/// The most names one error lists; the rest are counted.
const NAME_ERRORS_SHOWN: usize = 50;

/// Lays out `source` as an exFAT image and checks every name, without reading file data.
/// Every name exFAT cannot hold is reported at once, not just the first. `extra_free` is free
/// space wanted beyond the default spare; `cluster_size` overrides the size chosen from the
/// data (tests use small ones).
pub fn plan_exfat(
    source: &dyn SourceTree,
    cluster_size: Option<u32>,
    extra_free: u64,
) -> Result<ExfatPlan> {
    let mut tree = Tree {
        nodes: vec![Node {
            is_dir: true,
            ..Node::default()
        }],
    };
    let mut bad: Vec<String> = Vec::new();
    for f in source.files() {
        let path = f.path.trim_matches('/');
        let (dir, name) = match path.rsplit_once('/') {
            Some((d, n)) => (d, n),
            None => ("", path),
        };
        let placed = tree
            .dir_for(dir)
            .and_then(|parent| tree.child(parent, name, false));
        match placed {
            Ok(at) => {
                tree.nodes[at].src = path.to_string();
                tree.nodes[at].size = f.size;
            }
            Err(e) => bad.push(format!("{path}: {e}")),
        }
    }
    for dir in source.empty_dirs() {
        if let Err(e) = tree.dir_for(dir.trim_matches('/')) {
            bad.push(format!("{dir}: {e}"));
        }
    }
    if !bad.is_empty() {
        let more = bad.len().saturating_sub(NAME_ERRORS_SHOWN);
        let mut msg = format!(
            "{} name{} cannot go into an exFAT image:\n{}",
            bad.len(),
            if bad.len() == 1 { "" } else { "s" },
            bad[..bad.len().min(NAME_ERRORS_SHOWN)].join("\n")
        );
        if more > 0 {
            msg.push_str(&format!("\n… and {more} more"));
        }
        return format_err(msg);
    }

    let data_bytes: u64 = tree
        .nodes
        .iter()
        .filter(|n| !n.is_dir)
        .map(|n| n.size)
        .sum();
    let l = layout(&mut tree, cluster_size, spare_bytes(data_bytes, extra_free))?;
    let used: u64 = l.bitmap_clusters
        + clusters_for(l.upcase.len() as u64, l.cluster)
        + l.dirs
            .iter()
            .chain(l.files.iter())
            .map(|&n| tree.nodes[n].clusters)
            .sum::<u64>();
    Ok(ExfatPlan {
        image_bytes: l.heap_sectors * SECTOR + l.cluster_count * l.cluster,
        data_bytes: l.files.iter().map(|&f| tree.nodes[f].size).sum(),
        free_bytes: (l.cluster_count - used) * l.cluster,
        files: l.files.len(),
        directories: l.dirs.len() - 1,
        cluster_size: l.cluster as u32,
        tree,
        l,
        used,
    })
}

/// Writes the planned image into `w`, front to back (a file, or a `.ffpfsc` stream).
///
/// `on_progress(done, total)` is called with file-data bytes as they are written, and
/// `cancelled()` is asked between chunks. Returns the image's size.
pub fn write_exfat<W: Write>(
    plan: &ExfatPlan,
    source: &mut dyn SourceTree,
    w: &mut W,
    on_progress: &mut dyn FnMut(u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<u64> {
    write_exfat_after(plan, source, w, 0, on_progress, cancelled)
}

/// [`write_exfat`] for a writer that drops the image's first `skip` bytes because they are on disk
/// already: a file that ends before them isn't read, only stands in as zeros, so going on after a
/// crash costs what is left. The image is the same bytes: its layout and serial follow from the
/// files alone.
pub fn write_exfat_after<W: Write>(
    plan: &ExfatPlan,
    source: &mut dyn SourceTree,
    w: &mut W,
    skip: u64,
    on_progress: &mut dyn FnMut(u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<u64> {
    let (tree, l, used) = (&plan.tree, &plan.l, plan.used);
    let total = plan.data_bytes;
    let boot = boot_region(l, tree.nodes[0].first_cluster, used);
    w.write_all(&boot)?;
    w.write_all(&boot)?; // the backup region
    zeros(w, (FAT_OFFSET_SECTORS - 24) * SECTOR)?;

    // The FAT: two reserved entries, then a chain per allocation in cluster order.
    let mut fat_written = 8u64;
    w.write_all(&0xFFFF_FFF8u32.to_le_bytes())?;
    w.write_all(&FAT_END.to_le_bytes())?;
    let mut chain = |w: &mut W, first: u64, n: u64| {
        for c in first..first + n {
            let next = if c + 1 == first + n {
                FAT_END
            } else {
                (c + 1) as u32
            };
            w.write_all(&next.to_le_bytes())?;
        }
        fat_written += n * 4;
        Ok::<(), std::io::Error>(())
    };
    let mut next = u64::from(FIRST_CLUSTER);
    chain(w, next, l.bitmap_clusters)?;
    next += l.bitmap_clusters;
    let upcase_clusters = clusters_for(l.upcase.len() as u64, l.cluster);
    chain(w, next, upcase_clusters)?;
    next += upcase_clusters;
    for &n in l.dirs.iter().chain(l.files.iter()) {
        let c = tree.nodes[n].clusters;
        if c > 0 {
            chain(w, next, c)?;
            next += c;
        }
    }
    // Free clusters (only the padding of a very small volume) and the tail of the FAT.
    zeros(
        w,
        l.heap_sectors * SECTOR - FAT_OFFSET_SECTORS * SECTOR - fat_written,
    )?;

    // The heap. Bitmap first: one bit per cluster, set for everything allocated.
    let mut bitmap = vec![0u8; (l.bitmap_clusters * l.cluster) as usize];
    for bit in 0..used {
        bitmap[(bit / 8) as usize] |= 1 << (bit % 8);
    }
    w.write_all(&bitmap)?;
    w.write_all(&l.upcase)?;
    zeros(w, upcase_clusters * l.cluster - l.upcase.len() as u64)?;
    for &d in &l.dirs {
        w.write_all(&directory_bytes(tree, d, l))?;
    }

    let mut done = 0u64;
    on_progress(0, total);
    for &f in &l.files {
        let (src, size) = (tree.nodes[f].src.clone(), tree.nodes[f].size);
        let mut at = 0u64;
        if skip > 0 && size > 0 {
            let first = u64::from(tree.nodes[f].first_cluster);
            let start = l.heap_sectors * SECTOR + (first - u64::from(FIRST_CLUSTER)) * l.cluster;
            if start + tree.nodes[f].clusters * l.cluster <= skip {
                zeros(w, tree.nodes[f].clusters * l.cluster)?;
                done += size;
                on_progress(done, total);
                continue;
            }
        }
        while at < size {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let want = (size - at).min(COPY_CHUNK as u64) as usize;
            let chunk = source.read_range(&src, at, want)?;
            if chunk.len() != want {
                return format_err(format!(
                    "{src} ended at {} of its {size} bytes while it was being read",
                    at + chunk.len() as u64
                ));
            }
            w.write_all(&chunk)?;
            at += want as u64;
            done += want as u64;
            on_progress(done, total);
        }
        zeros(w, tree.nodes[f].clusters * l.cluster - size)?;
    }
    zeros(w, (l.cluster_count - used) * l.cluster)?;
    w.flush()?;
    Ok(plan.image_bytes)
}

/// Writes `source` as an exFAT image at `out` ([`plan_exfat`] then [`write_exfat`]).
///
/// A cancelled or failed build removes the partial file.
pub fn build_exfat(
    source: &mut dyn SourceTree,
    out: &Path,
    cluster_size: Option<u32>,
    on_progress: &mut dyn FnMut(u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<ExfatBuilt> {
    let plan = plan_exfat(source, cluster_size, 0)?;
    let result = (|| -> Result<u64> {
        let mut w = std::io::BufWriter::with_capacity(4 << 20, std::fs::File::create(out)?);
        write_exfat(&plan, source, &mut w, on_progress, cancelled)
    })();
    match result {
        Ok(image_bytes) => Ok(ExfatBuilt {
            image_bytes,
            files: plan.files,
            directories: plan.directories,
            cluster_size: plan.cluster_size,
        }),
        Err(e) => {
            let _ = std::fs::remove_file(out);
            Err(e)
        }
    }
}
