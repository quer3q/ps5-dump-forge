//! zstd packs in a conversion (`docs/zstd.md`): the tree the job writes, the packed set and the packing, into a folder or an image through [`measure`]; `ps5-dump-forge-zstd` does the bytes.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use ps5_dump_forge_lz4::runtime;
use ps5_dump_forge_lz4::{INDEX, JOURNAL, LOGS, RUNTIME};
use ps5_dump_forge_zstd::writer::PackOutput;
use ps5_dump_forge_zstd::{CHUNK_SIZE, INDEX_HEADER_SIZE, VOLUME_HEADER_SIZE, VOLUME_LIMIT};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::jobs::Ctx;
use crate::lz4::{Prepared, RuntimeAction};
use crate::preflight::listing;
use crate::{ConvertRequest, Format};

pub(crate) const PACK_INDEX: &str = ps5_dump_forge_zstd::reader::INDEX;

pub(crate) const INDEX_MAGIC: &[u8; 8] = b"AMPRZPK1";

/// Whether `tree` is zstd packed: a root `ampr_zstd.index` starting with [`INDEX_MAGIC`] (Inspect's label).
pub(crate) fn packed(tree: &mut dyn SourceTree) -> bool {
    let listed = (tree.files().iter()).any(|f| f.path == PACK_INDEX && f.size >= 8);
    listed
        && tree
            .read_range(PACK_INDEX, 0, 8)
            .is_ok_and(|b| b == INDEX_MAGIC)
}

/// Whether `path` is a root name the zstd writer puts there, case-insensitively.
pub(crate) fn is_pack_name(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    !p.contains('/') && (p == PACK_INDEX || (p.starts_with("ampr_zstd-") && p.ends_with(".pak")))
}

/// Whether `path` stays loose whatever the rules say: `always_loose`, the pack files and `ampr_emu.index`.
pub(crate) fn loose(path: &str) -> bool {
    ps5_dump_forge_lz4::always_loose(path) || is_pack_name(path) || path == INDEX
}

/// The files to pack and how.
pub(crate) struct Plan {
    pub files: Vec<SourceFile>,
    pub level: u8,
    /// Compression threads (the pak writer uses at most 8).
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

    /// Volumes the packer may need: each holds at least `VOLUME_LIMIT` less its header and one chunk.
    fn max_volumes(&self) -> u64 {
        let per = VOLUME_LIMIT - VOLUME_HEADER_SIZE as u64 - CHUNK_SIZE;
        self.packed_bytes().div_ceil(per).max(1)
    }

    /// `ampr_zstd.index`'s size: header, 32-byte file, 8-byte chunk and volume records, the paths.
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

    /// Every byte the packer may write: every chunk raw (a zstd chunk is strictly smaller), headers and index.
    pub(crate) fn worst_case_bytes(&self) -> u64 {
        self.packed_bytes()
            .saturating_add(self.max_volumes() * VOLUME_HEADER_SIZE as u64)
            .saturating_add(self.index_bytes())
    }

    /// The biggest file the packer writes (a volume or the index), for the destination's file size limit.
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

/// The tree a zstd pack job writes and its [`Plan`] (`tree` from [`crate::lz4::open`], the backport out): ampr-emu-ng by `ng_action`, journal, logs and old index out, a fresh `ampr_emu.index` listing every file; `None` with findings.
pub(crate) fn prepare(
    req: &ConvertRequest,
    mut tree: Box<dyn SourceTree>,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<(Prepared, Option<Plan>)> {
    let mut findings = request_findings(req);
    let eboot = tree.files().iter().any(|f| f.path == "eboot.bin");
    // A title without AMPR is packed too: ShadowMount+ loads the injected ampr-emu-ng.
    if eboot && !crate::lz4::imports_ampr(tree.as_mut()) {
        ctx.log("zstd: eboot.bin does not import libSceAmpr; injecting ampr-emu-ng");
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
    //
    let rt = crate::lz4::runtime_of(tree.as_mut());
    let keep = rt.ng_action() == RuntimeAction::Keep;
    if keep {
        crate::lz4::keep_log("zstd", &rt, ctx);
    } else {
        ctx.log(crate::lz4::install_log("zstd", &rt));
    }
    let mut files: Vec<SourceFile> = Vec::with_capacity(tree.files().len() + 2);
    for f in tree.files() {
        if f.path == JOURNAL || LOGS.contains(&f.path.as_str()) {
            ctx.log(format!("zstd: leaving out {}", f.path));
        } else if f.path != INDEX && (keep || f.path != RUNTIME) {
            files.push(f.clone());
        }
    }
    if !keep {
        files.push(SourceFile {
            path: RUNTIME.into(),
            size: runtime::zstd().len() as u64,
        });
    }
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
    // The built-in strategy (`rules::select_everything`) or a profile's rules (`store` packs as compress: a chunk zstd can't shrink stays raw).
    let mut findings = Vec::new();
    let profile = crate::lz4::pack_profile(req, ctx, &mut findings);
    if !findings.is_empty() {
        return Ok((refused(tree, findings), None));
    }
    let packed: Vec<SourceFile> = (files.iter())
        .filter(|f| {
            !loose(&f.path)
                && ps5_dump_forge_lz4::select_everything(&f.path, f.size, profile.as_ref())
                    .is_some()
        })
        .cloned()
        .collect();
    let (n, loose_n) = (packed.len(), files.len() - packed.len());
    match (&req.pack_profile, &profile) {
        (Some(path), Some(p)) => ctx.log(format!(
            "pack rules: {}: {} rules; {n} files packed, {loose_n} loose; zstd level {}, {} KiB chunks",
            path.display(),
            p.rules().len(),
            req.zstd_level,
            CHUNK_SIZE >> 10
        )),
        _ => ctx.log(format!(
            "packing strategy: {}; {n} files to pack, {loose_n} loose; zstd level {}, {} KiB chunks",
            crate::lz4::EVERYTHING,
            req.zstd_level,
            CHUNK_SIZE >> 10
        )),
    }
    let set: HashSet<&str> = packed.iter().map(|f| f.path.as_str()).collect();
    let loose: Vec<(&str, u64, bool)> = (files.iter())
        .filter(|f| !set.contains(f.path.as_str()))
        .map(|f| (f.path.as_str(), f.size, false))
        .collect();
    ctx.log(crate::lz4::everything_loose_line(&loose, profile.as_ref()));
    let mut generated: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    if !keep {
        generated.insert(RUNTIME.into(), Cow::Borrowed(runtime::zstd()));
    }
    generated.insert(INDEX.into(), Cow::Owned(index));
    let plan = Plan {
        files: packed,
        level: req.zstd_level,
        threads: crate::convert::compression_threads(req),
    };
    let tree = crate::lz4::overlay(tree, files, generated);
    Ok((refused(tree, Vec::new()), Some(plan)))
}

/// Packs `plan`'s files from `tree` into `out`: the volumes, then `ampr_zstd.index`, chunk-parallel on `plan.threads`.
pub(crate) fn pack<O: PackOutput>(
    tree: &mut dyn SourceTree,
    plan: &Plan,
    out: &mut O,
    ctx: &Ctx,
) -> anyhow::Result<ps5_dump_forge_zstd::Index> {
    let total = plan.packed_bytes();
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
        "wrote zstd packs: {} files packed into {} volumes; {} chunks, {zstd} \
         zstd; stored {}% of {raw} bytes; zstd level {}; {:.0} MB/s on {} threads",
        index.files.len(),
        index.volume_sizes.len(),
        index.chunks.len(),
        crate::convert::percent(stored, raw),
        plan.level,
        raw as f64 / 1e6 / secs,
        plan.threads
            .clamp(1, ps5_dump_forge_zstd::writer::MAX_THREADS)
    ));
    Ok(index)
}

/// zstd's measure pass into an image: [`pack`]'s compression keeping only the index, so the image is laid out from exact sizes.
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
    let total = plan.packed_bytes();
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

/// Every runtime check on the packs `tree` holds, then the logical files read back through them against `expected`.
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
