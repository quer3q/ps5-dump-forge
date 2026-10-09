//! Every integrity check we understand, as a named list.

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cnt::{self, EntryDigest};
use crate::crypto::sha3;
use crate::crypto::{derive_ekpfs, derive_xts_keys};
use crate::outer::{self, BlockKind};
use crate::xts::{Xts, SIGNED_SECTOR_FLAG};
use crate::{fih, flt, le32, si, PkgFile, Result};
use crate::{format_err, BLOCK};

#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug)]
pub struct Report {
    pub content_id: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    fn push(&mut self, name: impl Into<String>, ok: bool, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            ok,
            detail: detail.into(),
        });
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.content_id)?;
        for c in &self.checks {
            let mark = if c.ok { "ok  " } else { "FAIL" };
            writeln!(f, "  [{mark}] {} {}", c.name, c.detail)?;
        }
        Ok(())
    }
}

/// A dinode's file bytes, gathered from the plaintext blocks it points at.
fn file_data(img: &outer::OuterImage, node: Option<&outer::Dinode>) -> Vec<u8> {
    node.map(|n| img.file_data(n)).unwrap_or_default()
}

/// Does the inner image expand, through its descriptor, to a mount whose metadata walks?
///
/// The descriptor is the authority on every length here: the image must be the block count it
/// declares, and what the image stores from the metadata base on must expand to the rest of the
/// mount it declares. The codec comes from the descriptor's own `compType` — a zlib image stores a
/// container there and everything else stores the region itself — so this reads both without
/// being told which it is looking at.
fn inner_mount_ok(image_len: u64, image: &[u8], layout: &crate::naps::Layout) -> (bool, String) {
    let mount_size = layout.mount_size();
    let mut detail = format!(
        "{} stored block(s), {} declared, mount {mount_size:#x}, comp {}",
        image_len.div_ceil(BLOCK),
        layout.num_outer_blocks,
        layout.compression_type
    );
    if image_len != u64::from(layout.num_outer_blocks) * BLOCK {
        detail.push_str(" (the image is not the length the descriptor declares)");
        return (false, detail);
    }
    let Some(meta_base) = layout.fidx.len().checked_sub(2).map(|i| layout.fidx[i].0) else {
        detail.push_str(" (no metadata-base fidx face)");
        return (false, detail);
    };
    let region = mount_size.saturating_sub(meta_base);
    // A stored region fills the mount, so the image is exactly the mount's length; anything
    // shorter has had its region compressed into the space above the base.
    let stored = image_len == mount_size;
    if !stored && layout.compression_type != crate::naps::COMP_ZLIB as u8 {
        // A compressed region in a codec this build does not decode — a real package's Kraken,
        // typically. The geometry above still holds; the region is left to a Kraken reader.
        detail.push_str(&format!(
            " (comp {} is not decoded here, so the region is unchecked)",
            layout.compression_type
        ));
        return (true, detail);
    }
    let Some(tail) = image.get(meta_base as usize..) else {
        detail.push_str(" (the image ends before its metadata base)");
        return (false, detail);
    };
    let plain = if stored {
        match tail.get(..region as usize) {
            Some(slice) => slice.to_vec(),
            None => {
                detail.push_str(" (the image does not reach the mount's end)");
                return (false, detail);
            }
        }
    } else {
        match crate::pfsc::parse(tail).and_then(|c| c.decompress()) {
            Ok(plain) => plain,
            Err(e) => {
                detail.push_str(&format!(" (the metadata region does not expand: {e})"));
                return (false, detail);
            }
        }
    };
    if plain.len() as u64 != region {
        detail.push_str(&format!(
            " (its metadata is {} bytes against a {region}-byte region)",
            plain.len()
        ));
        return (false, detail);
    }
    // The data region is not read: the walk needs the metadata and the inode table, both of which
    // live above the base, so a zero prefix stands in for it.
    let mut mount = vec![0u8; meta_base as usize];
    mount.extend_from_slice(&plain);
    let walked = crate::inner::read(&mount, meta_base);
    match walked {
        Ok(m) if m.flt_ok && !m.files.is_empty() => (true, detail),
        Ok(m) => {
            detail.push_str(&format!(
                " (the metadata walks to {} file(s), flat-path table ok: {})",
                m.files.len(),
                m.flt_ok
            ));
            (false, detail)
        }
        Err(e) => {
            detail.push_str(&format!(" (its metadata does not walk: {e})"));
            (false, detail)
        }
    }
}

/// Every check that needs only the header and the container — the two things both
/// verifiers read whole.
fn container_report(fih_block: &[u8], fih: &fih::Fih, cnt: &cnt::Cnt) -> Report {
    let mut r = Report {
        content_id: cnt.content_id.clone(),
        checks: Vec::new(),
    };
    r.push(
        "fih debug image",
        fih.is_debug(),
        format!("signed byte {:#04x}", fih.signed_byte),
    );
    r.push(
        "fih format version 3",
        fih.format_version == 3,
        fih.format_version.to_string(),
    );
    r.push("cnt package digest", cnt.package_digest_ok(), "");
    r.push("cnt digest-table digest", cnt.digest_table_digest_ok(), "");
    r.push("cnt header rollup digest", cnt.header_rollup_ok(), "");
    r.push("cnt second rollup digest", cnt.sc_entries2_ok(), "");
    r.push("cnt body digest", cnt.body_digest_ok(), "");
    r.push(
        "cnt finalized-image digest",
        cnt.fih_digest_ok(fih_block),
        "",
    );
    r.push("cnt descriptor pairs", cnt.descriptor_ok(), "");
    r.push(
        "cnt image-key digest",
        cnt.entry_digest_at(cnt::ids::IMAGE_KEY, 0x520),
        "",
    );
    r.push(
        "cnt imagedigs digest",
        cnt.entry_digest_at(cnt::ids::IMAGE_DIGESTS, 0x540),
        "",
    );
    for (id, verdict) in cnt.entry_digests() {
        r.push(
            format!("cnt entry {id:#06x} digest"),
            verdict != EntryDigest::Mismatch,
            format!("{verdict:?}"),
        );
    }
    if let Some(e) = cnt.entry(cnt::ids::PARAM_JSON) {
        let param = sha3(cnt.payload(e));
        let general = cnt
            .entry(cnt::ids::GENERAL_DIGESTS)
            .map(|g| cnt.payload(g))
            .unwrap_or(&[]);
        r.push(
            "param.json digest in GeneralDigests",
            general.windows(32).any(|w| w == param),
            "",
        );
    }
    for (name, ok) in cnt.general_digests(&fih.game_digest) {
        r.push(name, ok, "");
    }

    r
}

pub fn verify_package(path: &Path, passcode: &str) -> Result<Report> {
    let mut file = PkgFile::open(path)?;
    let fih_block = file.read_at(0, crate::BLOCK as usize)?;
    let fih = fih::parse(&fih_block)?;
    let cnt = cnt::read(&mut file, fih.cnt_offset)?;
    let mut r = container_report(&fih_block, &fih, &cnt);

    let img = outer::open(&mut file, &fih, &cnt, passcode)?;
    r.push(
        "outer superblock ICV",
        img.superblock.icv_ok,
        format!("block {}", img.superblock.index),
    );
    // Only the wording depends on the mode: a plaintext block is not decrypted, it has to be its
    // own digest exactly as it is stored.
    let plaintext = img.superblock.mode == crate::ImageMode::PlaintextNoAuth;
    for v in &img.verdicts {
        r.push(
            format!(
                "outer block {} {} its imagedigs entry",
                v.index,
                if plaintext { "matches" } else { "decrypts to" }
            ),
            v.kind.is_some(),
            match v.kind {
                Some(BlockKind::Data) => "data sector",
                Some(BlockKind::Signed) => "signed sector",
                Some(BlockKind::Superblock) => "plaintext superblock",
                Some(BlockKind::Plaintext) => "plaintext block",
                None if plaintext => "not its stored digest",
                None => "no sector matched",
            },
        );
    }
    let table_ok = img
        .plaintext
        .get(img.superblock.inode_table_block as usize)
        .is_some_and(|b| sha3(b) == img.superblock.inode_table_digest);
    r.push("outer inode table digest", table_ok, "");
    let nodes = img.dinodes();
    for (ino, n) in nodes.iter().enumerate() {
        let direct_ok = n
            .direct
            .iter()
            .take((n.blocks as usize).min(outer::DIRECT_SLOTS))
            .all(|d| {
                img.plaintext
                    .get(d.block as usize)
                    .is_some_and(|b| sha3(b) == d.digest)
            });
        // The indirect tables, level by level: a parent records its children's digests,
        // so every level is checked against the one above it.
        let used = n.indirect.iter().take_while(|t| t.block != 0).count();
        r.push(
            format!("outer inode {ino} block signatures"),
            direct_ok && img.indirect_ok(n),
            format!("{} block(s), {used} indirect level(s)", n.blocks),
        );
    }
    let uroot: Vec<String> = nodes
        .get(2)
        .map(|n| img.dirents(n).into_iter().map(|d| d.name).collect())
        .unwrap_or_default();
    r.push(
        "outer uroot holds pfs_image.dat and naps_pkg_layout.dat",
        uroot.iter().any(|n| n == "pfs_image.dat")
            && uroot.iter().any(|n| n == "naps_pkg_layout.dat"),
        uroot.join(", "),
    );

    // The inner image is what the console actually mounts, and nothing else in this report reads
    // it: the stored file has to be the length the descriptor declares, and its metadata region
    // has to expand to the rest of the mount and walk. A package can pass every outer check and
    // still fail here, which is the failure this exists to name.
    {
        let by_name = |name: &str| -> Vec<u8> {
            let ino = nodes
                .get(2)
                .map(|n| img.dirents(n))
                .unwrap_or_default()
                .into_iter()
                .find(|d| d.name == name)
                .map(|d| d.ino as usize);
            file_data(&img, ino.and_then(|i| nodes.get(i)))
        };
        let image = by_name("pfs_image.dat");
        let naps = by_name("naps_pkg_layout.dat");
        match crate::naps::parse(&naps) {
            // A descriptor our own reader cannot lay out is this crate's limitation, not the
            // package's — `naps::parse` reads the smallest sample exactly and the large ones not
            // at all — so it is reported without failing the package.
            Err(e) => r.push(
                "the inner image expands to the mount its descriptor fixes",
                true,
                format!("the descriptor is not parsed here: {e}"),
            ),
            Ok(layout) => {
                let (ok, detail) = inner_mount_ok(image.len() as u64, &image, &layout);
                r.push(
                    "the inner image expands to the mount its descriptor fixes",
                    ok,
                    detail,
                );
            }
        }
    }

    // The outer flat-path table (inode 1) hashes each uroot dirent name to its inode.
    {
        let flt = file_data(&img, nodes.get(1));
        let count = flt.get(0x2C..0x30).map(|b| le32(b, 0)).unwrap_or(0) as usize;
        let mut checked = 0usize;
        let mut ok = count > 0;
        for d in nodes.get(2).map(|n| img.dirents(n)).unwrap_or_default() {
            if d.name == "." || d.name == ".." {
                continue;
            }
            let want = flt::hash_path(&d.name);
            let found = (0..count).any(|i| {
                let e = 0x40 + i * 16;
                flt.get(e..e + 16).is_some_and(|rec| {
                    u64::from_le_bytes(rec[..8].try_into().unwrap()) == want
                        && (u64::from_le_bytes(rec[8..].try_into().unwrap()) & 0xFF_FFFF)
                            == d.ino as u64
                })
            });
            checked += 1;
            ok &= found;
        }
        r.push(
            "outer flat-path table hashes the uroot names",
            ok && checked > 0,
            format!("{checked} name(s), {count} table entries"),
        );
    }

    match si::read(&mut file)? {
        Some(s) => {
            let crc_name = format!("config/{}/playgo-chunk.crc", cnt.content_id);
            match s.members.iter().find(|m| m.name == crc_name) {
                Some(m) => {
                    let stored = file.read_at(m.offset, m.size as usize)?;
                    let expected = si::chunk_crc_table(&mut file, s.zip_start)?;
                    r.push(
                        "si playgo-chunk.crc",
                        stored == expected,
                        format!("{} bytes", m.size),
                    );
                }
                None => r.push("si playgo-chunk.crc", false, format!("{crc_name} missing")),
            }
        }
        None => r.push("si zip present", false, "no trailing STORED ZIP"),
    }
    Ok(r)
}

/// Verify a package without holding it: every block is read and decrypted on demand, and
/// the data blocks are swept once against `imagedigs` instead of being kept. A 155 GB
/// package verifies in a few hundred megabytes this way, which is what the engine needs,
/// since the packages it builds are that size.
pub fn verify_streaming(
    path: &Path,
    passcode: &str,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Report> {
    verify_streaming_controlled(path, passcode, progress, None)
}

/// [`verify_streaming`], stopping with [`crate::Error::Cancelled`] once `cancel` is set. A
/// verification reads the whole package (twice: the block sweep and the CRC table), minutes
/// for a large game, so every block read checks the flag.
pub fn verify_streaming_controlled(
    path: &Path,
    passcode: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: Option<&AtomicBool>,
) -> Result<Report> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err(crate::Error::Cancelled);
    }
    verify_file_controlled(PkgFile::open(path)?, passcode, progress, cancel)
}

/// [`verify_streaming_controlled`] over a package already open, e.g. through the handle it was
/// just written with, so the bytes checked are that file's whatever its path now names.
/// Outer blocks the sweep reads per batch (8 MiB).
const SWEEP_BATCH: u64 = 128;

/// How many of `raw`'s blocks (outer blocks `first` on) reach no `digests` entry, checked on
/// every core: by decryption with either sector number under `xts`, as stored without it. The
/// superblock is skipped: it is not encrypted and was matched against the game digest.
fn failing_blocks(
    raw: &[u8],
    first: u64,
    sb_index: u64,
    xts: Option<&Xts>,
    digests: &[[u8; 32]],
) -> u64 {
    let matches = |block: &[u8], index: u64| match xts {
        None => sha3(block) == digests[index as usize],
        Some(xts) => [index, SIGNED_SECTOR_FLAG | index].iter().any(|&sector| {
            let mut pt = block.to_vec();
            xts.decrypt(sector, &mut pt);
            sha3(&pt) == digests[index as usize]
        }),
    };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let per = (raw.len() / BLOCK as usize).div_ceil(threads).max(1) * BLOCK as usize;
    std::thread::scope(|s| {
        let workers: Vec<_> = raw
            .chunks(per)
            .enumerate()
            .map(|(c, share)| {
                let matches = &matches;
                s.spawn(move || {
                    share
                        .chunks(BLOCK as usize)
                        .enumerate()
                        .filter(|&(k, block)| {
                            let index = first + ((c * per) / BLOCK as usize + k) as u64;
                            index != sb_index && !matches(block, index)
                        })
                        .count() as u64
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .sum()
    })
}

pub fn verify_file_controlled(
    file: PkgFile,
    passcode: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: Option<&AtomicBool>,
) -> Result<Report> {
    verify_file_sampled(file, passcode, progress, cancel, None)
}

/// Which of `batches` 8 MiB batches a sampled sweep reads, ascending: the first, the last and
/// a seeded 1% of the rest, at most 128 of them (1 GiB). The same for the same seed.
fn sampled_batches(batches: u64, seed: u64) -> Vec<u64> {
    let mut picked = vec![0, batches.saturating_sub(1)];
    let rest = batches.saturating_sub(2);
    // Ends included, at most 128 batches (1 GiB).
    let want = (rest / 100).max(u64::from(rest > 0)).min(126);
    // SplitMix64: small, and the same on every platform for the same seed.
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for _ in 0..want {
        picked.push(1 + next() % rest);
    }
    picked.sort_unstable();
    picked.dedup();
    picked.retain(|&b| b < batches);
    picked
}

/// [`verify_file_controlled`], with `sample: Some(seed)` reading only [`sampled_batches`] of
/// the outer blocks and of the `playgo-chunk.crc` table (every structural check still runs);
/// for a caller that checks the package's files itself.
pub fn verify_file_sampled(
    mut file: PkgFile,
    passcode: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: Option<&AtomicBool>,
    sample: Option<u64>,
) -> Result<Report> {
    let stop = || -> Result<()> {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(crate::Error::Cancelled);
        }
        Ok(())
    };
    stop()?;
    let fih_block = file.read_at(0, BLOCK as usize)?;
    let fih = fih::parse(&fih_block)?;
    let cnt = cnt::read(&mut file, fih.cnt_offset)?;
    let mut r = container_report(&fih_block, &fih, &cnt);

    if !fih.pfs_size.is_multiple_of(BLOCK) {
        return format_err("outer image size is not a whole number of blocks");
    }
    let count = fih.pfs_size / BLOCK;
    let digests = match cnt.image_digests() {
        Some(d) if d.len() as u64 == count => d,
        _ => {
            r.push(
                "outer imagedigs table",
                false,
                "missing or the wrong length",
            );
            return Ok(r);
        }
    };

    // The superblock is where the header says it is, and its plaintext is the game digest.
    let sb_absolute = crate::le64(&fih_block, 0x20);
    let sb_index = sb_absolute.saturating_sub(BLOCK) / BLOCK;
    let sb_block = if sb_absolute >= BLOCK && sb_index < count {
        file.read_at(fih.pfs_offset + sb_index * BLOCK, BLOCK as usize)?
    } else {
        return format_err("the header's superblock offset is outside the image");
    };
    r.push(
        "outer superblock is where the header says",
        sha3(&sb_block) == fih.game_digest,
        format!("block {sb_index}"),
    );
    let superblock = outer::parse_superblock(sb_index, &sb_block)?;
    r.push("outer superblock ICV", superblock.icv_ok, "");

    // A native image derives its keys here; a plaintext one stores its blocks as they are, so
    // there is nothing to derive and nothing to undo.
    let plaintext_mode = superblock.mode == crate::ImageMode::PlaintextNoAuth;
    let xts = (!plaintext_mode).then(|| {
        Xts::new(&derive_xts_keys(
            &derive_ekpfs(&cnt.content_id, passcode),
            &superblock.seed,
        ))
    });
    // The sector rule the samples follow: data blocks carry their own index, metadata
    // blocks set bit 47, and the superblock is not encrypted at all.
    let read_block = |file: &mut PkgFile, index: u64| -> Result<Vec<u8>> {
        stop()?;
        let raw = file.read_at(fih.pfs_offset + index * BLOCK, BLOCK as usize)?;
        if index == sb_index {
            return Ok(raw);
        }
        let Some(xts) = &xts else {
            return Ok(raw);
        };
        let mut pt = raw;
        let sector = if index < sb_index {
            index
        } else {
            SIGNED_SECTOR_FLAG | index
        };
        xts.decrypt(sector, &mut pt);
        Ok(pt)
    };

    let table = read_block(&mut file, superblock.inode_table_block as u64)?;
    r.push(
        "outer inode table digest",
        sha3(&table) == superblock.inode_table_digest,
        "",
    );
    let nodes = outer::parse_dinodes(&table, superblock.dinode_count as u64);
    for (ino, node) in nodes.iter().enumerate() {
        // The data blocks are checked by the sweep below; here it is the dinode's own
        // records and every level of its indirect tables.
        let ok = {
            let mut read = |index: u64| read_block(&mut file, index);
            outer::verify_dinode(node, false, &mut read)?
        };
        r.push(
            format!("outer inode {ino} block signatures"),
            ok,
            format!("{} block(s)", node.blocks),
        );
    }

    // The sweep: every data block must reach the digest `imagedigs` carries — by decryption in
    // the native mode, as stored in the plaintext one.
    // Read a batch at a time and checked on every core.
    let batches = count.div_ceil(SWEEP_BATCH);
    let swept: Vec<u64> = match sample {
        Some(seed) => sampled_batches(batches, seed),
        None => (0..batches).collect(),
    };
    let sweep_blocks: u64 = swept
        .iter()
        .map(|b| (count - b * SWEEP_BATCH).min(SWEEP_BATCH))
        .sum();
    let (mut bad, mut done) = (0u64, 0u64);
    for &b in &swept {
        stop()?;
        let first = b * SWEEP_BATCH;
        let n = (count - first).min(SWEEP_BATCH);
        let raw = file.read_at(fih.pfs_offset + first * BLOCK, (n * BLOCK) as usize)?;
        bad += failing_blocks(&raw, first, sb_index, xts.as_ref(), &digests);
        done += n;
        progress(done * BLOCK, sweep_blocks * BLOCK);
    }
    progress(done * BLOCK, sweep_blocks * BLOCK);
    let scope = match sample {
        Some(seed) => format!(" (a sample of {done}, seed {seed})"),
        None => String::new(),
    };
    r.push(
        if plaintext_mode {
            "outer blocks match their imagedigs entry"
        } else {
            "outer blocks decrypt to their imagedigs entry"
        },
        bad == 0,
        format!("{bad} of {count} failed{scope}"),
    );

    // The uroot dirents and the flat-path table, read on demand like everything else.
    let uroot_bytes = read_block(&mut file, nodes[2].direct[0].block as u64)?;
    let uroot = outer::OuterImage::parse_dirents(&uroot_bytes, nodes[2].size);
    let names: Vec<String> = uroot.iter().map(|d| d.name.clone()).collect();
    r.push(
        "outer uroot holds pfs_image.dat and naps_pkg_layout.dat",
        names.iter().any(|n| n == "pfs_image.dat")
            && names.iter().any(|n| n == "naps_pkg_layout.dat"),
        names.join(", "),
    );
    {
        // The flat-path table hashes each uroot name to the inode its dirent records.
        let flt = read_block(&mut file, nodes[1].direct[0].block as u64)?;
        let count = crate::le32(&flt, 0x2C) as usize;
        let mut checked = 0usize;
        let mut ok = count > 0;
        for entry in uroot.iter().filter(|d| !d.name.starts_with('.')) {
            let want = flt::hash_path(&entry.name);
            let found = (0..count).any(|i| {
                let at = 0x40 + i * 16;
                flt.get(at..at + 16).is_some_and(|rec| {
                    u64::from_le_bytes(rec[..8].try_into().unwrap()) == want
                        && (u64::from_le_bytes(rec[8..].try_into().unwrap()) & 0xFF_FFFF)
                            == u64::from(entry.ino)
                })
            });
            checked += 1;
            ok &= found;
        }
        r.push(
            "outer flat-path table hashes the uroot names",
            ok && checked > 0,
            format!("{checked} name(s), {count} table entries"),
        );
    }

    match si::read(&mut file)? {
        Some(s) => {
            let crc_name = format!("config/{}/playgo-chunk.crc", cnt.content_id);
            match s.members.iter().find(|m| m.name == crc_name) {
                Some(m) => {
                    let stored = file.read_at(m.offset, m.size as usize)?;
                    let (ok, detail) = match sample {
                        None => {
                            let expected =
                                si::chunk_crc_table_controlled(&mut file, s.zip_start, cancel)?;
                            (stored == expected, format!("{} bytes", m.size))
                        }
                        Some(seed) => {
                            let batches = s.zip_start.div_ceil(si::CRC_BATCH * BLOCK);
                            let picked = sampled_batches(batches, seed);
                            let mut ok = stored.len() as u64 == s.zip_start.div_ceil(BLOCK) * 4;
                            for &b in &picked {
                                if !ok {
                                    break;
                                }
                                stop()?;
                                let start = b * si::CRC_BATCH * BLOCK;
                                let len = (s.zip_start - start).min(si::CRC_BATCH * BLOCK);
                                let at = (b * si::CRC_BATCH * 4) as usize;
                                let crcs = si::batch_crcs(&mut file, start, len)?;
                                ok = crcs.iter().enumerate().all(|(k, c)| {
                                    stored.get(at + k * 4..at + k * 4 + 4) == Some(&c.to_le_bytes())
                                });
                            }
                            let n = picked.len();
                            (
                                ok,
                                format!("{} bytes, {n} of {batches} batches sampled", m.size),
                            )
                        }
                    };
                    r.push("si playgo-chunk.crc", ok, detail);
                }
                None => r.push("si playgo-chunk.crc", false, format!("{crc_name} missing")),
            }
        }
        None => r.push("si zip present", false, "no trailing STORED ZIP"),
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::sampled_batches;

    #[test]
    fn a_sample_holds_the_ends_and_one_percent() {
        assert_eq!(sampled_batches(1, 7), [0]);
        assert_eq!(sampled_batches(2, 7), [0, 1]);
        assert_eq!(sampled_batches(3, 7), [0, 1, 2]);
        let picked = sampled_batches(10_000, 42);
        assert_eq!(
            picked,
            sampled_batches(10_000, 42),
            "the same for the same seed"
        );
        assert_ne!(picked, sampled_batches(10_000, 43));
        assert!(picked.windows(2).all(|w| w[0] < w[1]));
        assert_eq!((picked[0], *picked.last().unwrap()), (0, 9_999));
        assert!((90..=102).contains(&picked.len()), "{}", picked.len());
        assert!(
            (0..64).all(|seed| sampled_batches(1_000_000, seed).len() <= 128),
            "at most 128 batches, 1 GiB"
        );
    }
}
