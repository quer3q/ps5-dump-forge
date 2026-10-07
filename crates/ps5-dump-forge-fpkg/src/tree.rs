//! The inner filesystem's tree, walked through the mount the way the console walks it: the
//! superblock at the metadata base, the inode table one block after it (390 inodes to a block),
//! and one dirent stream per directory from `uroot` (inode 4) down.
//!
//! Everything here is untrusted: counts are capped before anything is sized by them, every
//! directory is entered once (a second visit is a cycle), and the walk stops at a fixed number
//! of entries and of path bytes.

use std::collections::HashSet;

use ps5upload_fpkg::inner::{INODE_LEN, inode_offset, inode_table_len};
use ps5upload_fpkg::plan::{DIRENT_DIR, DIRENT_DOT, DIRENT_DOTDOT, DIRENT_FILE, FIRST_DIR_INODE};
use ps5upload_fpkg::{BLOCK, Result};

use crate::pfs::{le32, le64};
use crate::{CORRUPT, Reader, err};

/// Inodes accepted, as the vendored reader caps them.
const MAX_INODES: u64 = 4_000_000;
/// Entries listed in all (files and directories), as the vendored UFS2/exFAT walkers cap them.
pub(crate) const MAX_ENTRIES: usize = 1_000_000;
/// Path bytes kept in all.
pub(crate) const MAX_PATH_BYTES: usize = 256 << 20;
/// One directory's dirent stream, read whole.
// ponytail: 64 MiB per directory (~1 M entries), read whole; stream it if a title ever needs more
const MAX_DIR_BYTES: u64 = 64 << 20;
const SUPERBLOCK_MAGIC: u64 = 20_130_315;

pub(crate) struct InnerFile {
    pub path: String,
    pub size: u64,
    /// Logical (mount) offset.
    pub logical: u64,
    /// The inode's afid (`db1`), which orders the files in a flat image.
    pub afid: i32,
}

pub(crate) struct InnerTree {
    pub files: Vec<InnerFile>,
    pub empty_dirs: Vec<String>,
    /// Every directory path (for collisions with container entries).
    pub dirs: Vec<String>,
}

struct Inode {
    mode: u16,
    size: u64,
    logical: u64,
    afid: i32,
}

/// Walk the tree whose metadata starts at `meta_base` of a mount `mount_size` long. File data
/// must lie below `meta_base`.
pub(crate) fn walk(reader: &mut Reader, meta_base: u64, mount_size: u64) -> Result<InnerTree> {
    if meta_base.checked_add(BLOCK).is_none_or(|e| e > mount_size) {
        return Err(err(
            CORRUPT,
            format!("the metadata base {meta_base:#x} is past the mount's {mount_size:#x}"),
        ));
    }
    let mut sb = [0u8; 0x40];
    reader.mount(meta_base, &mut sb)?;
    if le64(&sb, 0) != 2 || le64(&sb, 8) != SUPERBLOCK_MAGIC {
        return Err(err(CORRUPT, "inner superblock version/magic mismatch"));
    }
    if u64::from(le32(&sb, 0x20)) != BLOCK {
        return Err(err(
            CORRUPT,
            format!("inner block size {:#x}", le32(&sb, 0x20)),
        ));
    }
    let inode_count = le64(&sb, 0x30);
    let table_at = meta_base + BLOCK;
    let table_fits = inode_count <= MAX_INODES
        && table_at
            .checked_add(inode_table_len(inode_count as usize))
            .is_some_and(|e| e <= mount_size);
    if inode_count <= u64::from(FIRST_DIR_INODE) || !table_fits {
        return Err(err(
            CORRUPT,
            format!("implausible inner inode count {inode_count}"),
        ));
    }
    let inode = |reader: &mut Reader, ino: u32| -> Result<Inode> {
        if u64::from(ino) >= inode_count {
            return Err(err(
                CORRUPT,
                format!("inode {ino} is past the table's {inode_count}"),
            ));
        }
        let mut raw = [0u8; INODE_LEN];
        reader.mount(table_at + inode_offset(ino as usize) as u64, &mut raw)?;
        Ok(Inode {
            mode: u16::from_le_bytes([raw[0], raw[1]]),
            size: le64(&raw, 8),
            logical: le64(&raw, 0x60),
            afid: le32(&raw, 0x68) as i32,
        })
    };

    let mut tree = InnerTree {
        files: Vec::new(),
        empty_dirs: Vec::new(),
        dirs: Vec::new(),
    };
    let mut entries = 0usize;
    let mut path_bytes = 0usize;
    // Directory streams live in the metadata region, each in its own blocks, so together they
    // cannot be longer than it; this stops a tree whose directories all point at one huge
    // stream from reading it once per directory.
    let mut dir_budget = mount_size - meta_base;
    let mut visited: HashSet<u32> = HashSet::new();
    let mut pending: Vec<(u32, String)> = vec![(FIRST_DIR_INODE, String::new())];
    while let Some((dir_ino, prefix)) = pending.pop() {
        if !visited.insert(dir_ino) {
            return Err(err(
                CORRUPT,
                format!("directory inode {dir_ino} is linked twice (a cycle)"),
            ));
        }
        let dir = inode(reader, dir_ino)?;
        if dir.mode & 0xF000 != 0x4000 {
            return Err(err(
                CORRUPT,
                format!("inode {dir_ino} ({}) is not a directory", shown(&prefix)),
            ));
        }
        if dir.size > MAX_DIR_BYTES
            || dir
                .logical
                .checked_add(dir.size)
                .is_none_or(|e| e > mount_size)
        {
            return Err(err(
                CORRUPT,
                format!(
                    "directory {} claims {} bytes at {:#x}",
                    shown(&prefix),
                    dir.size,
                    dir.logical
                ),
            ));
        }
        dir_budget = dir_budget.checked_sub(dir.size).ok_or_else(|| {
            err(
                CORRUPT,
                "the directories claim more bytes than the metadata region holds",
            )
        })?;
        let mut bytes = vec![0u8; dir.size as usize];
        reader.mount(dir.logical, &mut bytes)?;
        let mut children = 0usize;
        let mut o = 0usize;
        while o + 16 <= bytes.len() {
            let ino = le32(&bytes, o);
            let kind = le32(&bytes, o + 4) as i32;
            let name_len = le32(&bytes, o + 8) as usize;
            let ent_size = le32(&bytes, o + 12) as usize;
            if ent_size == 0 {
                break; // the zero padding after the last entry
            }
            if ent_size < 16 + name_len || o + 16 + name_len > bytes.len() {
                return Err(err(
                    CORRUPT,
                    format!("a directory entry of {} is malformed", shown(&prefix)),
                ));
            }
            let raw_name = &bytes[o + 16..o + 16 + name_len];
            o = o.saturating_add(ent_size);
            if kind == i32::from(DIRENT_DOT) || kind == i32::from(DIRENT_DOTDOT) {
                continue;
            }
            let name = std::str::from_utf8(raw_name)
                .ok()
                .filter(|n| !n.is_empty() && *n != "." && *n != ".." && !n.contains(['/', '\0']))
                .ok_or_else(|| {
                    err(
                        CORRUPT,
                        format!(
                            "{} holds a name that is not a valid path component ({:?})",
                            shown(&prefix),
                            String::from_utf8_lossy(raw_name)
                        ),
                    )
                })?;
            entries += 1;
            path_bytes = path_bytes.saturating_add(prefix.len() + 1 + name.len());
            if entries > MAX_ENTRIES || path_bytes > MAX_PATH_BYTES {
                return Err(err(
                    CORRUPT,
                    format!(
                        "the image lists more than {MAX_ENTRIES} entries or {MAX_PATH_BYTES} path bytes"
                    ),
                ));
            }
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            children += 1;
            if kind == i32::from(DIRENT_DIR) {
                tree.dirs.push(path.clone());
                pending.push((ino, path));
            } else if kind == i32::from(DIRENT_FILE) {
                let f = inode(reader, ino)?;
                if f.mode & 0xF000 != 0x8000 {
                    return Err(err(
                        CORRUPT,
                        format!("{path}: inode {ino} is not a regular file"),
                    ));
                }
                if f.logical.checked_add(f.size).is_none_or(|e| e > meta_base) {
                    return Err(err(
                        CORRUPT,
                        format!(
                            "{path}: {} bytes at {:#x} run past the data region",
                            f.size, f.logical
                        ),
                    ));
                }
                tree.files.push(InnerFile {
                    path,
                    size: f.size,
                    logical: f.logical,
                    afid: f.afid,
                });
            } else {
                return Err(err(
                    CORRUPT,
                    format!("{path}: unknown directory entry kind {kind}"),
                ));
            }
        }
        if children == 0 && !prefix.is_empty() {
            tree.empty_dirs.push(prefix);
        }
    }
    // A name listed twice in one directory (as two files, or a file and a directory) would
    // make two entries of one path.
    let mut all: Vec<&str> = tree
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(tree.dirs.iter().map(String::as_str))
        .collect();
    all.sort_unstable();
    if let Some(w) = all.windows(2).find(|w| w[0] == w[1]) {
        return Err(err(CORRUPT, format!("{} is listed twice", w[0])));
    }
    Ok(tree)
}

fn shown(prefix: &str) -> &str {
    if prefix.is_empty() { "uroot" } else { prefix }
}
