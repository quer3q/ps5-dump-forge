//! zstd packs in a conversion (`docs/PAK.md`): the tree the job writes (the zstd
//! runtime installed, a fresh `ampr_emu.index` listing every file, loose and packed), which
//! files go into the packs, and the packing (`ps5-dump-forge-zstd` does the bytes; this is the
//! job's glue). Into a folder, or straight into an image through a measure pass
//! ([`measure`], then `ps5_dump_forge_zstd::tree::PackedTree`); no keep-loose list.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use ps5_dump_forge_lz4::runtime::{self, RuntimeKind};
use ps5_dump_forge_lz4::{INDEX, JOURNAL, LOGS, RUNTIME};
use ps5_dump_forge_zstd::writer::PackOutput;
use ps5_dump_forge_zstd::{CHUNK_SIZE, INDEX_HEADER_SIZE, VOLUME_HEADER_SIZE, VOLUME_LIMIT};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::jobs::Ctx;
use crate::lz4::{Packs, Prepared};
use crate::preflight::listing;
use crate::{ConvertRequest, Format};

/// The zstd index at the game root.
pub(crate) const PACK_INDEX: &str = ps5_dump_forge_zstd::reader::INDEX;

/// The zstd index's first bytes.
pub(crate) const INDEX_MAGIC: &[u8; 8] = b"AMPRZPK1";

/// Whether `tree` is zstd packed: a root `ampr_zstd.index` that starts with [`INDEX_MAGIC`]
/// (Inspect's and the Library's label). An index that can't be read is not.
pub(crate) fn packed(tree: &mut dyn SourceTree) -> bool {
    let listed = (tree.files().iter()).any(|f| f.path == PACK_INDEX && f.size >= 8);
    listed
        && tree
            .read_range(PACK_INDEX, 0, 8)
            .is_ok_and(|b| b == INDEX_MAGIC)
}

/// Whether `path` is a name the zstd writer puts at the root (`ampr_zstd.index`, `ampr_zstd-*.pak`),
/// as a case-insensitive destination sees it.
pub(crate) fn is_pack_name(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    !p.contains('/') && (p == PACK_INDEX || (p.starts_with("ampr_zstd-") && p.ends_with(".pak")))
}

/// Whether `path` stays loose whatever the rules say: the `always_loose` list (eboot,
/// executables, `sce_sys/`, `sce_module/`, `fakelib*/`, the ampr_emu artifacts) plus
/// `ampr_zstd.index`, `ampr_zstd-*.pak` and `ampr_emu.index`.
pub(crate) fn loose(path: &str) -> bool {
    ps5_dump_forge_lz4::always_loose(path) || is_pack_name(path) || path == INDEX
}

/// The files to pack and how.
pub(crate) struct Plan {
    /// The packed files, as the prepared tree lists them.
    pub files: Vec<SourceFile>,
    /// zstd level, 1..=22.
    pub level: u8,
    /// Compression threads: the request's `compression_threads`, else every core (the pak
    /// writer uses at most 8).
    pub threads: usize,
}

impl Plan {
    /// The packed files' paths, for a loose-or-packed test per file.
    pub(crate) fn packed_paths(&self) -> HashSet<&str> {
        self.files.iter().map(|f| f.path.as_str()).collect()
    }

    /// The packed files' logical bytes.
    pub(crate) fn packed_bytes(&self) -> u64 {
        self.files
            .iter()
            .fold(0u64, |sum, f| sum.saturating_add(f.size))
    }

    /// Volumes the packer may need: each holds at least `VOLUME_LIMIT` less its header and
    /// one chunk that did not fit.
    fn max_volumes(&self) -> u64 {
        let per = VOLUME_LIMIT - VOLUME_HEADER_SIZE as u64 - CHUNK_SIZE;
        self.packed_bytes().div_ceil(per).max(1)
    }

    /// `ampr_zstd.index`'s size: header, 32-byte file records, 8-byte chunk and volume records,
    /// the paths.
    fn index_bytes(&self) -> u64 {
        let chunks = self.files.iter().fold(0u64, |sum, f| {
            sum.saturating_add(f.size.div_ceil(CHUNK_SIZE))
        });
        let paths = self
            .files
            .iter()
            .fold(0u64, |sum, f| sum.saturating_add(f.path.len() as u64));
        (INDEX_HEADER_SIZE as u64)
            .saturating_add(32u64.saturating_mul(self.files.len() as u64))
            .saturating_add(8u64.saturating_mul(chunks))
            .saturating_add(8u64.saturating_mul(self.max_volumes()))
            .saturating_add(paths)
    }

    /// Every byte the packer may write: every chunk stored raw (a zstd chunk is strictly
    /// smaller), the volume headers and the index.
    pub(crate) fn worst_case_bytes(&self) -> u64 {
        self.packed_bytes()
            .saturating_add(self.max_volumes() * VOLUME_HEADER_SIZE as u64)
            .saturating_add(self.index_bytes())
    }

    /// The biggest file the packer writes, for the destination's file size limit: a volume
    /// (4 GiB less 64 KiB at most) or the index.
    pub(crate) fn largest_file(&self) -> u64 {
        let volume = self
            .packed_bytes()
            .saturating_add(VOLUME_HEADER_SIZE as u64)
            .min(VOLUME_LIMIT);
        volume.max(self.index_bytes())
    }
}

/// What the request's own options make of zstd packing.
fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if !matches!(
        req.format,
        Format::Folder | Format::Exfat | Format::Ffpkg | Format::Ffpfs | Format::Ffpfsc
    ) {
        findings.push(format!(
            "zstd packs go into a folder or an exfat, ffpkg, ffpfs or ffpfsc image, not {}",
            crate::lz4::target_name(req.format)
        ));
    }
    if req.lz4.is_some() {
        findings.push("zstd packing takes no LZ4 option (lz4)".into());
    }
    if req.zstd_unpack {
        findings.push("zstd pack and zstd unpack can't go together".into());
    }
    if !(1..=22).contains(&req.zstd_level) {
        findings.push(format!(
            "the zstd level is 1 through 22, not {}",
            req.zstd_level
        ));
    }
    findings
}

/// The tree a zstd pack job writes and its [`Plan`] (`packs` from [`crate::lz4::open`],
/// which decoded any LZ4 or zstd packs; the backport already left out). The zstd runtime
/// replaces whatever is at `fakelib/libSceAmpr.sprx`; the journal, logs and old index are left
/// out and a fresh `ampr_emu.index` (`mtime`: the job's timestamp) lists every file written,
/// loose and packed. `None` with findings.
pub(crate) fn prepare(
    req: &ConvertRequest,
    mut tree: Box<dyn SourceTree>,
    packs: Packs,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<(Prepared, Option<Plan>)> {
    let mut findings = request_findings(req);
    let eboot = tree.files().iter().any(|f| f.path == "eboot.bin");
    if eboot && !crate::lz4::imports_ampr(tree.as_mut()) {
        findings.push(
            "eboot.bin does not import libSceAmpr: zstd packs only work for a title that uses \
             AMPR"
                .into(),
        );
    }
    // Every root entry, a folder too (`ampr_zstd.index/x` or an empty one would block the file).
    let mut taken: Vec<String> = (tree.files().iter().map(|f| f.path.as_str()))
        .chain(tree.empty_dirs().iter().map(String::as_str))
        .map(|p| p.split('/').next().unwrap_or(p))
        .filter(|p| is_pack_name(p))
        .map(str::to_owned)
        .collect();
    taken.sort_unstable();
    taken.dedup();
    findings.extend(listing(
        "named like a zstd pack file the target writes",
        &taken,
    ));
    let refused = |tree, findings| Prepared {
        tree,
        findings,
        pack: None,
    };
    if !findings.is_empty() {
        return Ok((refused(tree, findings), None));
    }
    ctx.log(match crate::lz4::runtime_of(tree.as_mut()).kind {
        RuntimeKind::None => format!("zstd: installing the zstd runtime at {RUNTIME}"),
        RuntimeKind::Zstd => format!("zstd: the zstd runtime at {RUNTIME} is written again"),
        _ => format!("zstd: replacing the runtime at {RUNTIME} with the zstd runtime"),
    });
    let mut files: Vec<SourceFile> = Vec::with_capacity(tree.files().len() + 2);
    for f in tree.files() {
        if f.path == JOURNAL || LOGS.contains(&f.path.as_str()) {
            ctx.log(format!("zstd: leaving out {}", f.path));
        } else if f.path != INDEX && f.path != RUNTIME {
            files.push(f.clone());
        }
    }
    files.push(SourceFile {
        path: RUNTIME.into(),
        size: runtime::ZSTD.len() as u64,
    });
    let rows: Vec<(String, u64)> = files.iter().map(|f| (f.path.clone(), f.size)).collect();
    let index = match ps5upload_fpkg::ampr_index::build(&rows, mtime) {
        Ok(index) => index,
        Err(e) => return Ok((refused(tree, vec![e]), None)),
    };
    ctx.log(format!(
        "zstd: writing a new {INDEX} for {} files",
        rows.len()
    ));
    files.push(SourceFile {
        path: INDEX.into(),
        size: index.len() as u64,
    });
    // Pack what LZ4 packs: the game reads only AMPR files through the runtime and opens the
    // rest (configs, container indexes, media) with plain libc I/O the runtime does not
    // serve. An LZ4-packed source keeps its packed set; any other source takes LZ4's rules
    // (the built-in guess, then the keep-loose list).
    let rules = match &packs.lz4_packed {
        Some(set) => format!("the source's LZ4 packs ({} files)", set.len()),
        None => "LZ4's built-in guess".into(),
    };
    let packed: Vec<SourceFile> = (files.iter())
        .filter(|f| {
            !loose(&f.path)
                && match &packs.lz4_packed {
                    Some(set) => set.contains(&f.path),
                    None => ps5_dump_forge_lz4::select(&f.path, f.size).is_some(),
                }
        })
        .cloned()
        .collect();
    ctx.log(format!(
        "zstd rules: {rules}; {} files to pack, {} loose; zstd level {}, {} KiB chunks",
        packed.len(),
        files.len() - packed.len(),
        req.zstd_level,
        CHUNK_SIZE >> 10
    ));
    let mut generated: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    generated.insert(RUNTIME.into(), Cow::Borrowed(runtime::ZSTD));
    generated.insert(INDEX.into(), Cow::Owned(index));
    let plan = Plan {
        files: packed,
        level: req.zstd_level,
        threads: crate::convert::compression_threads(req),
    };
    let tree = crate::lz4::overlay(tree, files, generated);
    Ok((refused(tree, Vec::new()), Some(plan)))
}

/// Packs `plan`'s files from `tree` (the job's hashing tree) into `out`: the volumes, then
/// `ampr_zstd.index`. Compression runs chunk-parallel in bounded batches on a pool of
/// `plan.threads` (the pak crate).
pub(crate) fn pack<O: PackOutput>(
    tree: &mut dyn SourceTree,
    plan: &Plan,
    out: &mut O,
    ctx: &Ctx,
) -> anyhow::Result<ps5_dump_forge_zstd::Index> {
    let total = plan.packed_bytes().saturating_mul(2);
    ctx.progress("pack", 0, total);
    let started = std::time::Instant::now();
    let index = ps5_dump_forge_zstd::writer::pack_with(
        tree,
        &plan.files,
        plan.level,
        plan.threads,
        out,
        &mut |n| ctx.progress("pack", n.min(total), total),
        ctx.cancel,
    );
    ctx.check()?;
    let index = index?;
    ctx.progress("pack", total, total);
    let raw = plan.packed_bytes();
    let stored: u64 = (index.volume_sizes.iter())
        .map(|s| s - VOLUME_HEADER_SIZE as u64)
        .sum();
    let zstd = (index.chunks.iter())
        .filter(|c| c.codec == ps5_dump_forge_zstd::Codec::Zstd)
        .count();
    let secs = started.elapsed().as_secs_f64().max(1e-3);
    ctx.log(format!(
        "wrote zstd packs: {} files packed into {} volumes, {} stored raw; {} chunks, {zstd} \
         zstd; stored {}% of {raw} bytes; zstd level {}; {:.0} MB/s on {} threads",
        index.files.len(),
        index.volume_sizes.len(),
        index.files.iter().filter(|f| f.store_only).count(),
        index.chunks.len(),
        crate::convert::percent(stored, raw),
        plan.level,
        raw as f64 / 1e6 / secs,
        plan.threads
            .clamp(1, ps5_dump_forge_zstd::writer::MAX_THREADS)
    ));
    Ok(index)
}

/// The measure pass of zstd packs into an image: [`pack`]'s compression over `plan`'s files,
/// keeping only the index, so the image can be laid out from exact sizes. The write pass
/// reads the packed files again.
pub(crate) fn measure(
    tree: &mut dyn SourceTree,
    plan: &Plan,
    req: &ConvertRequest,
    ctx: &Ctx,
) -> anyhow::Result<ps5_dump_forge_zstd::Index> {
    ctx.log(
        "zstd into an image: packing straight into it, no temporary folder; the packed files \
         are read again to write them",
    );
    // Each file counts twice (the store-only test, then the compression).
    let total = plan.packed_bytes().saturating_mul(2);
    ctx.expect_rest(total.saturating_add(crate::convert::after_measure(tree, req)));
    ctx.progress("measure", 0, total);
    let started = std::time::Instant::now();
    let index = ps5_dump_forge_zstd::writer::measure(
        tree,
        &plan.files,
        plan.level,
        plan.threads,
        &mut |n| ctx.progress("measure", n.min(total), total),
        ctx.cancel,
    );
    ctx.check()?;
    let index = index?;
    ctx.progress("measure", total, total);
    let raw = plan.packed_bytes();
    let stored: u64 = (index.volume_sizes.iter())
        .map(|s| s - VOLUME_HEADER_SIZE as u64)
        .sum();
    ctx.log(format!(
        "measured zstd packs: {} files packed into {} volumes; {} chunks; stored {}% of {raw} \
         bytes; zstd level {}; {:.0} MB/s",
        index.files.len(),
        index.volume_sizes.len(),
        index.chunks.len(),
        crate::convert::percent(stored, raw),
        plan.level,
        raw as f64 / 1e6 / started.elapsed().as_secs_f64().max(1e-3),
    ));
    Ok(index)
}

/// Every check the runtime makes on the packs `tree` holds (the index and each volume header),
/// then the logical files read back through the packs against `expected`.
pub(crate) fn checks(
    expected: &crate::verify::Expected,
    tree: Box<dyn SourceTree>,
    ctx: &Ctx,
    mode: crate::verify::Mode,
) -> anyhow::Result<Vec<String>> {
    let mut tree = ps5_dump_forge_zstd::reader::unpack(tree)
        .map_err(|e| anyhow::Error::new(e).context("reading the zstd packs back"))?;
    let mut checks = vec![format!(
        "zstd packs: {PACK_INDEX} ({} packed files) and {} volume headers pass the reader's checks",
        tree.index().files.len(),
        tree.index().volume_sizes.len()
    )];
    checks.extend(crate::verify::compare(expected, &mut tree, ctx, mode)?);
    Ok(checks)
}
