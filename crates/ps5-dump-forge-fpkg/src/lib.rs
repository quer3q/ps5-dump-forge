//! Streaming reader for debug FPKG packages (`.pkg`): the package's game tree as a
//! [`SourceTree`], so `.pkg` converts to a folder or any image without staging.
//!
//! Frozen interface (forge-core depends on it).
//!
//! How a read reaches the bytes: a file of the inner filesystem has a logical (mount) offset;
//! for a Kraken image the mount is tiled by blocks of up to 256 KiB, so a range maps to the
//! blocks it overlaps, each decoded from its stored bytes in `pfs_image.dat`; those stored bytes
//! map, 64 KiB at a time, through the outer dinode's block map to outer blocks, each read from
//! the package, decrypted when the image is native, and checked against its `imagedigs` digest.
//! A flat image stores each file raw, so its range is read straight from `pfs_image.dat`. Small
//! rings keep the last outer and decoded blocks; nothing is proportional to the package except
//! the block maps (4 B per outer block, ~32 B per Kraken block).
//!
//! Errors are [`ps5upload_fpkg::Error::Format`] messages that start with one of the prefixes
//! below, so a caller can tell the cases apart without parsing the rest.

mod mount;
mod pfs;
mod tree;

use std::path::Path;

use ps5upload_fpkg::cnt::ids;
use ps5upload_fpkg::cnt_write::PRESENTATION;
use ps5upload_fpkg::crypto::{
    DEFAULT_PASSCODE, derive_ekpfs, derive_pfs_key, derive_xts_keys, sha3,
};
use ps5upload_fpkg::outer::{Dinode, OuterImage, parse_dinodes};
use ps5upload_fpkg::source::{SourceFile, SourceTree};
use ps5upload_fpkg::xts::Xts;
use ps5upload_fpkg::{BLOCK, Error, ReadSeek, Result, naps};

use crate::mount::Layout;
use crate::pfs::{Outer, OuterFile, Pkg};
use crate::tree::InnerFile;

/// The file is not a PS5 package at all (no `\x7FFIH` header, or a PS4 package).
pub const NOT_PS5: &str = "not a PS5 package";
/// A retail package: the header's signed byte is not the debug `0x00`.
pub const RETAIL: &str = "retail package";
/// A native (AES-XTS) image whose container key check fails for the passcode given.
pub const WRONG_PASSCODE: &str = "wrong passcode";
/// A native image that does not decrypt although nothing says the passcode is wrong.
pub const NATIVE_UNDECODABLE: &str = "native-encrypted image this reader cannot decode";
/// A compression type, or a Kraken block mode, this reader does not decode.
pub const UNSUPPORTED_CODEC: &str = "unsupported codec";
/// Anything else: truncated, damaged, or structurally inconsistent.
pub const CORRUPT: &str = "corrupt package";

/// The most [`SourceTree::read`] returns whole; larger files go through `read_range`.
pub const MAX_WHOLE_READ: u64 = 256 << 20;
/// The most one `read_range` call returns (the trait allows short reads; callers loop).
const MAX_RANGE: usize = 64 << 20;

pub(crate) fn err(kind: &str, msg: impl std::fmt::Display) -> Error {
    Error::Format(format!("{kind}: {msg}"))
}

/// The container entries that carry a game file: the presentation set the builder copies into
/// the container (icons, `pic*`, `snd0.at9`, `save_data.png`, trophy and UDS data). Everything
/// else in a container is the package's own metadata (keys, digests, license, PlayGo tables,
/// `param.json`'s install copy, the protected NP entries) and is not a file of the game.
fn container_paths() -> impl Iterator<Item = (u32, &'static str)> {
    [
        (ids::ICON0_PNG, "sce_sys/icon0.png"),
        (ids::ICON0_DDS, "sce_sys/icon0.dds"),
    ]
    .into_iter()
    .chain(PRESENTATION.iter().map(|(id, path, _)| (*id, *path)))
}

/// Where a file's bytes are.
enum Where {
    /// In the inner image: a mount offset (Kraken) or an on-disk one (flat).
    Inner(u64),
    /// A container entry, at this absolute package offset.
    Container(u64),
}

/// The layers a read goes through, kept apart so each can borrow the others.
pub(crate) struct Reader {
    outer: Outer,
    image: OuterFile,
    layout: Layout,
}

impl Reader {
    /// Exactly `buf.len()` bytes at `off`: of the mount for a Kraken image; of the stored image
    /// for a flat one, which is the mount from the metadata base on and where a file's `at` is
    /// already its on-disk offset (see [`flat_placements`]).
    pub(crate) fn mount(&mut self, off: u64, buf: &mut [u8]) -> Result<()> {
        match &mut self.layout {
            Layout::Kraken(k) => k.read(&mut self.outer, &self.image, off, buf),
            Layout::Flat(_) => self.image.read(&mut self.outer, off, buf),
        }
    }
}

/// An opened package. Reads decode only the blocks a range touches.
pub struct FpkgSource {
    files: Vec<SourceFile>,
    /// Parallel to `files`.
    places: Vec<Where>,
    empty_dirs: Vec<String>,
    conflicts: Vec<String>,
    details: Vec<String>,
    label: String,
    content_id: String,
    reader: Reader,
}

impl FpkgSource {
    /// Open `path` with `passcode` (the debug default when `None`). Errors say precisely what
    /// is wrong: not a PS5 package, wrong passcode, native-encrypted, unsupported codec, retail.
    pub fn open(path: &Path, passcode: Option<&str>) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        Self::from_reader(Box::new(file), len, path.display().to_string(), passcode)
    }

    /// [`FpkgSource::open`] over any seekable bytes of known length (a file, a view into one).
    pub fn from_reader(
        file: Box<dyn ReadSeek>,
        len: u64,
        label: impl Into<String>,
        passcode: Option<&str>,
    ) -> Result<Self> {
        let label = label.into();
        let passcode = passcode.unwrap_or(DEFAULT_PASSCODE);
        let mut pkg = Pkg::new(file, len);
        let h = pfs::header(&mut pkg)?;
        let cnt = pfs::container(&mut pkg, h.cnt_offset)?;
        let digests = cnt
            .entry(ids::IMAGE_DIGESTS)
            .ok_or_else(|| err(CORRUPT, "the container has no imagedigs entry"))?;
        if digests.size != h.outer_blocks * 32 {
            return Err(err(
                CORRUPT,
                format!(
                    "imagedigs holds {} bytes for {} outer blocks",
                    digests.size, h.outer_blocks
                ),
            ));
        }
        let mut outer = Outer::new(pkg, &h, digests.at, None);

        // The superblock is stored as it is in both modes; its digest is the header's.
        let sb_bytes = outer
            .try_load(h.sb_index)?
            .filter(|b| sha3(b) == h.game_digest)
            .ok_or_else(|| err(CORRUPT, "the outer superblock does not match its digests"))?;
        let sb = pfs::superblock(&sb_bytes)?;
        let mut facts = vec![
            format!("content id: {}", cnt.content_id),
            format!(
                "package: {len} bytes, debug, FIH format {}",
                h.format_version
            ),
        ];
        if sb.plaintext {
            facts.push("image mode: plaintext (PPRPLAIN-NOAUTH!), no passcode needed".into());
        } else {
            facts.push("image mode: native AES-XTS, keys from the passcode".into());
            outer.set_xts(Some(Xts::new(&derive_xts_keys(
                &derive_ekpfs(&cnt.content_id, passcode),
                &sb.seed,
            ))));
        }
        if sb.inode_table_block == h.sb_index {
            return Err(err(CORRUPT, "the outer inode table is the superblock"));
        }
        let table = match outer.try_load(sb.inode_table_block)? {
            Some(t) if sha3(&t) == sb.inode_table_digest => t,
            Some(_) => {
                return Err(err(
                    CORRUPT,
                    "the outer inode table does not match its record",
                ));
            }
            None if sb.plaintext => {
                return Err(err(
                    CORRUPT,
                    "the outer inode table does not match its digest",
                ));
            }
            None => return Err(native_failure(&mut outer, &cnt, passcode)?),
        };
        let nodes = parse_dinodes(&table, sb.dinode_count);
        let uroot = child(&mut outer, &nodes, 0, "uroot")?;
        let image_ino = child(&mut outer, &nodes, uroot, "pfs_image.dat")?;
        let naps_ino = child(&mut outer, &nodes, uroot, "naps_pkg_layout.dat")?;
        let image = OuterFile::map(&mut outer, &nodes[image_ino], "pfs_image.dat")?;
        if nodes[naps_ino].size > mount::MAX_DESCRIPTOR {
            return Err(err(
                CORRUPT,
                format!(
                    "the layout descriptor is {} bytes (at most {} read)",
                    nodes[naps_ino].size,
                    mount::MAX_DESCRIPTOR
                ),
            ));
        }
        let naps_file = OuterFile::map(&mut outer, &nodes[naps_ino], "naps_pkg_layout.dat")?;
        let mut blob = vec![0u8; naps_file.size as usize];
        naps_file.read(&mut outer, 0, &mut blob)?;
        let layout = Layout::parse(&blob, image.size)?;
        drop(blob);

        let meta_base = h.meta_base;
        let mount_size = match &layout {
            Layout::Kraken(k) => {
                facts.push(format!(
                    "layout: Kraken, {} blocks ({} stored raw)",
                    k.block_count(),
                    k.raw_blocks()
                ));
                k.mount_size()
            }
            Layout::Flat(l) => {
                facts.push("layout: flat, stored".into());
                // From the metadata base on, a flat image is its own mount.
                if l.mount_size() > image.size {
                    return Err(err(
                        CORRUPT,
                        format!(
                            "the flat image is {} bytes, short of its {}-byte mount",
                            image.size,
                            l.mount_size()
                        ),
                    ));
                }
                l.mount_size()
            }
        };
        facts.push(format!(
            "sizes: outer image {} bytes, inner image {} bytes stored, mount {mount_size} bytes, metadata base {meta_base:#x}",
            h.outer_blocks * BLOCK,
            image.size
        ));

        let mut reader = Reader {
            outer,
            image,
            layout,
        };
        let mut inner = tree::walk(&mut reader, meta_base, mount_size)?;
        if let Layout::Flat(l) = &reader.layout {
            flat_placements(&mut inner.files, l, meta_base)?;
        }

        let mut files: Vec<(SourceFile, Where)> = inner
            .files
            .into_iter()
            .map(|f| {
                (
                    SourceFile {
                        path: f.path,
                        size: f.size,
                    },
                    Where::Inner(f.logical),
                )
            })
            .collect();
        let mut empty_dirs = inner.empty_dirs;
        let dirs: std::collections::HashSet<String> = inner.dirs.into_iter().collect();

        // The container's copies of game files: added where the image has nothing at that path,
        // skipped (and noted when they disagree) where it does.
        let mut conflicts = Vec::new();
        let mut from_container = 0usize;
        for (id, path) in container_paths() {
            let Some(e) = cnt.entry(id) else { continue };
            // An empty slot is how the builder leaves an icon out; a protected (encrypted) entry
            // is not a plain copy of anything.
            if e.size == 0 || e.flags1 & 0x8000_0000 != 0 {
                continue;
            }
            if dirs.contains(path) {
                conflicts.push(format!(
                    "{path}: the container carries a file where the image has a directory; the image wins"
                ));
                continue;
            }
            // A file of the image where the entry needs a directory (`sce_sys` itself a file).
            let blocked = path
                .match_indices('/')
                .map(|(at, _)| &path[..at])
                .find(|dir| files.iter().any(|(f, _)| f.path == *dir));
            if let Some(dir) = blocked {
                conflicts.push(format!(
                    "{path}: the image has a file at {dir}, where the container's entry needs a directory; the image wins"
                ));
                continue;
            }
            if let Some((f, _)) = files.iter().find(|(f, _)| f.path == path) {
                if f.size != e.size {
                    conflicts.push(format!(
                        "{path}: the image holds {} bytes, the container {}; the image wins",
                        f.size, e.size
                    ));
                }
                continue;
            }
            empty_dirs.retain(|d| !path.starts_with(&format!("{d}/")));
            files.push((
                SourceFile {
                    path: path.to_string(),
                    size: e.size,
                },
                Where::Container(e.at),
            ));
            from_container += 1;
        }
        files.sort_by(|a, b| a.0.path.cmp(&b.0.path));
        empty_dirs.sort();
        facts.push(format!(
            "files: {} ({from_container} from the container only), {} empty directories, {} conflicts",
            files.len(),
            empty_dirs.len(),
            conflicts.len()
        ));
        let (files, places) = files.into_iter().unzip();
        Ok(Self {
            files,
            places,
            empty_dirs,
            conflicts,
            details: facts,
            label,
            content_id: cnt.content_id,
            reader,
        })
    }

    /// Format facts for `inspect` (content id, image mode, codec, sizes), one per line.
    pub fn details(&self) -> Vec<String> {
        self.details.clone()
    }

    /// Paths both the image and the container carry, with sizes that disagree; the image's
    /// copy is the one served.
    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }

    fn index(&self, path: &str) -> Result<usize> {
        self.files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .map_err(|_| Error::Format(format!("{path} is not in {}", self.label)))
    }
}

impl SourceTree for FpkgSource {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let size = self.files[self.index(path)?].size;
        if size > MAX_WHOLE_READ {
            return Err(Error::Format(format!(
                "{path} is {size} bytes, over the {MAX_WHOLE_READ} read whole; read it by range"
            )));
        }
        let mut out = Vec::with_capacity(size as usize);
        while (out.len() as u64) < size {
            let chunk = self.read_range(path, out.len() as u64, MAX_RANGE)?;
            if chunk.is_empty() {
                break;
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    fn read_range(&mut self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let i = self.index(path)?;
        let size = self.files[i].size;
        if offset >= size {
            return Ok(Vec::new());
        }
        let n = (size - offset).min(len.min(MAX_RANGE) as u64) as usize;
        let mut buf = vec![0u8; n];
        match self.places[i] {
            Where::Inner(at) => self.reader.mount(at + offset, &mut buf)?,
            Where::Container(at) => self.reader.outer.pkg.read_at(at + offset, &mut buf)?,
        }
        Ok(buf)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        format!("fpkg {} ({})", self.label, self.content_id)
    }
}

/// Why a native image's inode table did not decrypt. The container's entry-keys slot carries
/// `SHA3(k) ^ k` for each passcode-derived key `k` (slot 1 is the image key), which tests the
/// passcode without touching the image.
fn native_failure(outer: &mut Outer, cnt: &pfs::Container, passcode: &str) -> Result<Error> {
    let check = match cnt.entry(ids::ENTRY_KEYS).filter(|e| e.size >= 96) {
        Some(e) => {
            let mut stored = [0u8; 32];
            outer.pkg.read_at(e.at + 64, &mut stored)?;
            let key = derive_pfs_key(&cnt.content_id, passcode, 1);
            let mut want = sha3(&key);
            for (w, k) in want.iter_mut().zip(key) {
                *w ^= k;
            }
            Some(stored == want)
        }
        None => None,
    };
    Ok(match check {
        Some(false) => err(
            WRONG_PASSCODE,
            "the container's key check fails for this passcode",
        ),
        Some(true) => err(
            NATIVE_UNDECODABLE,
            "the passcode passes the container's key check, but the outer blocks do not \
             decrypt with the keys it derives",
        ),
        None => err(
            NATIVE_UNDECODABLE,
            "the outer blocks do not decrypt with this passcode, and the container has no key \
             check to tell a wrong passcode from another key scheme",
        ),
    })
}

/// The inode `name` names in directory dinode `dir` of the outer image.
fn child(outer: &mut Outer, nodes: &[Dinode], dir: usize, name: &str) -> Result<usize> {
    let node = nodes
        .get(dir)
        .ok_or_else(|| err(CORRUPT, format!("outer dinode {dir} is missing")))?;
    let block = outer.block(u64::from(node.direct[0].block))?;
    OuterImage::parse_dirents(block, node.size)
        .into_iter()
        .find(|d| d.name == name)
        .map(|d| d.ino as usize)
        .filter(|&ino| ino < nodes.len())
        .ok_or_else(|| err(CORRUPT, format!("the outer image has no {name}")))
}

/// A flat image stores each file at its own 64 KiB boundary, in afid order, where the mount packs
/// them; the descriptor's records do not say where (the vendored `naps::reconstruct` takes the
/// placements from the writer's plan). So they are re-derived the way the planner lays them out,
/// and checked against what the descriptor does carry: each file's logical start in afid order,
/// the data's on-disk end and the metadata base. Rewrites each file's `logical` to its on-disk
/// offset.
fn flat_placements(files: &mut [InnerFile], layout: &naps::Layout, meta_base: u64) -> Result<()> {
    let n = files.len();
    let refuse = |why: &str| {
        Err(err(
            CORRUPT,
            format!("a flat image whose file placement cannot be derived: {why}"),
        ))
    };
    let fidx = &layout.fidx;
    if fidx.len() != n + 3 {
        return refuse("the descriptor's file count differs from the tree's");
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| files[i].afid);
    let mut on_disk = 0u64;
    for (k, &i) in order.iter().enumerate() {
        if files[i].afid != k as i32 || fidx[k].0 != files[i].logical {
            return refuse("the afids do not match the descriptor's file starts");
        }
        files[i].logical = on_disk;
        match on_disk
            .checked_add(files[i].size)
            .and_then(|e| e.checked_next_multiple_of(BLOCK))
        {
            Some(next) if next <= meta_base => on_disk = next,
            _ => return refuse("the files run past the metadata base"),
        }
    }
    if fidx[n].0 != on_disk || fidx[n + 1].0 != meta_base {
        return refuse("the data end or metadata base differs from the descriptor's");
    }
    Ok(())
}
