//! LZ4 asset packs in a conversion: unpacking a packed source, tracing, and the logical tree
//! the `Lz4` target packs (`ps5-dump-forge-lz4` does the bytes; this is the job's glue).
//!
//! A source no LZ4 option touches converts as before: packs travel as plain files. Otherwise
//! the tree the job writes is an overlay: the packs decoded ([`open`]), the journal, logs and
//! old path index left out, the requested runtime installed and `ampr_emu.index` rebuilt for
//! the final file set ([`prepare`]).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::sync::atomic::AtomicBool;

use anyhow::Context;
use ps5_dump_forge_lz4::runtime::{self, RuntimeKind};
use ps5_dump_forge_lz4::writer::{PackFile, PackOutput};
use ps5_dump_forge_lz4::{
    AutoLoose, CRC_SIDECAR, INDEX, JOURNAL, LOGS, MANIFEST, PROFILE, Profile, RUNTIME,
    RuntimeProfile, Selection,
};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::jobs::Ctx;
use crate::preflight::listing;
use crate::{ConvertRequest, Format, Lz4Mode};

/// A runtime is a few hundred KiB; a bigger file is not read to be recognized.
const MAX_RUNTIME: u64 = 64 << 20;
/// The largest `ampr_emu.index` read for traces (2 M records and their paths fit).
const MAX_INDEX: u64 = 512 << 20;

/// Which runtime `tree` carries at `fakelib/libSceAmpr.sprx` (`None` without one).
pub(crate) fn runtime_kind(tree: &mut dyn SourceTree) -> RuntimeKind {
    let Some(size) = size_of(tree, RUNTIME) else {
        return RuntimeKind::None;
    };
    if size > MAX_RUNTIME {
        return RuntimeKind::Other;
    }
    match tree.read_range(RUNTIME, 0, size as usize) {
        Ok(bytes) if bytes.len() as u64 == size => runtime::classify(&bytes),
        _ => RuntimeKind::Other,
    }
}

fn size_of(tree: &dyn SourceTree, path: &str) -> Option<u64> {
    tree.files().iter().find(|f| f.path == path).map(|f| f.size)
}

/// What [`open`] found and did.
#[derive(Debug, Default)]
pub(crate) struct Packs {
    /// The source holds packs (a root `ampr_assets.index` starting `AMPRPAK4`).
    pub packed: bool,
    /// ... and the tree [`open`] returned has them decoded.
    pub unpacked: bool,
    /// For the `Lz4` target without a profile: the files the traces saw, resolved against the
    /// source's whole tree (before the backport is left out: the index the game ran with lists
    /// it). `None`: no usable traces.
    pub observed: Option<BTreeSet<String>>,
    /// What reading the traces refused.
    pub findings: Vec<String>,
}

/// Whether the job packs: the `Lz4` target (a packed folder), or [`Lz4Mode::Pack`] into any
/// `format`.
pub(crate) fn packing(req: &ConvertRequest) -> bool {
    req.format == Format::Lz4 || req.lz4 == Some(Lz4Mode::Pack)
}

/// Whether the job decodes packs: unpack, trace (not in place), or repack. Unpatch and an
/// in-place job refuse a packed source instead.
fn wants_unpack(req: &ConvertRequest) -> bool {
    let in_place = req.lz4_in_place || req.lz4 == Some(Lz4Mode::Unpatch);
    (req.lz4.is_some() && !in_place) || packing(req)
}

/// The findings of [`ConvertRequest::lz4_in_place`]: an `.exfat`/`.ffpkg` source (`kind`)
/// converted to its own format, patched (`Trace`) or unpatched, not packed. Empty when the
/// request is not in place.
pub(crate) fn in_place_findings(
    req: &ConvertRequest,
    kind: crate::convert::Kind,
    packed: bool,
) -> Vec<String> {
    use crate::convert::Kind;
    let mut findings = Vec::new();
    if !req.lz4_in_place {
        return findings;
    }
    let own = match kind {
        Kind::Exfat => Some(Format::Exfat),
        Kind::Ffpkg => Some(Format::Ffpkg),
        _ => None,
    };
    match kind {
        Kind::Folder => findings.push(
            "a game folder is patched in place directly (lz4_patch, lz4_unpatch), not by a \
             conversion in place"
                .into(),
        ),
        Kind::Exfat | Kind::Ffpkg => {}
        other => findings.push(format!(
            "a .{} can't be patched in place: the game's /app0 is read-only in it; only an \
             .exfat or .ffpkg image is replaced by a patched copy",
            other.name()
        )),
    }
    if let Some(own) = own
        && req.format != own
    {
        findings.push(format!(
            "patching in place keeps the image's format: the target must be .{}, not {}",
            kind.name(),
            target_name(req.format)
        ));
    }
    let verb = match req.lz4 {
        Some(Lz4Mode::Trace) => "patch",
        Some(Lz4Mode::Unpatch) => "unpatch",
        _ => {
            findings.push(
                "in place goes only with an LZ4 patch (lz4: trace) or unpatch (lz4: unpatch)"
                    .into(),
            );
            return findings;
        }
    };
    if packed && own.is_some() {
        findings.push(format!(
            "this image holds LZ4 packs ({MANIFEST}): unpack it first (Unpack LZ4), then {verb} \
             the unpacked image"
        ));
    }
    findings
}

/// The source with its packs decoded when the job asks for that, and its traces when the
/// `Lz4` target picks by them. Before the backport is left out: its files are records of the
/// manifest and of the traced index. Packs that fail any check fail the job.
pub(crate) fn open(
    req: &ConvertRequest,
    mut source: Box<dyn SourceTree>,
    ctx: &Ctx,
) -> anyhow::Result<(Box<dyn SourceTree>, Packs)> {
    let mut packs = Packs {
        packed: ps5_dump_forge_lz4::reader::detect(source.as_mut())
            .context("looking for LZ4 packs")?,
        ..Packs::default()
    };
    if packs.packed && wants_unpack(req) {
        let tree = ps5_dump_forge_lz4::reader::unpack(source).context("opening the LZ4 packs")?;
        ctx.log(format!("LZ4: {}", tree.describe()));
        source = Box::new(tree);
        packs.unpacked = true;
    }
    if packing(req) && req.lz4_profile.is_none() {
        packs.observed = traces(
            source.as_mut(),
            req.lz4_traces.as_deref(),
            ctx,
            &mut packs.findings,
        )?;
    }
    Ok((source, packs))
}

/// The pack list and its options, for the `Lz4` target.
pub(crate) struct PackPlan {
    /// Every logical file in final `ampr_emu.index` record order; `spec` `None` stays loose.
    pub files: Vec<PackFile>,
    pub runtime: Option<RuntimeProfile>,
    /// Where the rules came from, for the job log.
    pub rules: String,
}

/// What [`prepare`] made of the tree.
pub(crate) struct Prepared {
    pub tree: Box<dyn SourceTree>,
    pub findings: Vec<String>,
    /// For the `Lz4` target.
    pub pack: Option<PackPlan>,
}

/// The findings the request's LZ4 options make on their own (ruling 3 of the design).
fn combinations(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    let trace = req.lz4 == Some(Lz4Mode::Trace);
    if trace && !matches!(req.format, Format::Folder | Format::Exfat | Format::Ffpkg) {
        findings.push(format!(
            "LZ4 traces are recorded only in a folder, .exfat or .ffpkg (the game writes them \
             into its own /app0, which is read-only from a {})",
            target_name(req.format)
        ));
    }
    if req.lz4 == Some(Lz4Mode::Unpack) && req.format == Format::Lz4 {
        findings.push("unpacking LZ4 is redundant: the LZ4 target always unpacks first".into());
    }
    if req.lz4 == Some(Lz4Mode::Pack) && req.format == Format::Pkg {
        findings.push(
            "LZ4 packs go into a folder, .exfat, .ffpkg, .ffpfs or .ffpfsc; packing into a .pkg \
             is not supported yet"
                .into(),
        );
    }
    if req.lz4 == Some(Lz4Mode::Unpatch) && req.format == Format::Lz4 {
        findings.push(
            "unpatching LZ4 is redundant: the LZ4 target always installs the release runtime"
                .into(),
        );
    }
    if req.lz4_profile.is_some() && !packing(req) {
        findings.push("an LZ4 profile only goes with packing (LZ4 Pack)".into());
    }
    if req.lz4_traces.is_some() && !packing(req) {
        findings.push("LZ4 traces only go with packing (LZ4 Pack)".into());
    }
    if req.lz4_traces.is_some() && req.lz4_profile.is_some() {
        findings.push("choose a profile or traces, not both".into());
    }
    if trace && !(64..=1024).contains(&req.lz4_trace_space_mib)
        || !req.lz4_trace_space_mib.is_multiple_of(64)
    {
        findings.push(format!(
            "the LZ4 trace space is 64 MiB to 1 GiB, in 64 MiB steps, not {} MiB",
            req.lz4_trace_space_mib
        ));
    }
    findings
}

fn target_name(format: Format) -> String {
    match format {
        Format::Lz4 => "LZ4 packed folder".into(),
        other => crate::preflight::extension(other).map_or("folder".into(), |e| format!(".{e}")),
    }
}

/// The tree the job writes, per the request's LZ4 options (`packs` from [`open`], the
/// backport already left out). `mtime` is the job's one timestamp (integer seconds) for a
/// rebuilt `ampr_emu.index` and the pack manifest. Untouched sources come back as they were.
pub(crate) fn prepare(
    req: &ConvertRequest,
    mut tree: Box<dyn SourceTree>,
    mut packs: Packs,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<Prepared> {
    let mut findings = combinations(req);
    findings.append(&mut packs.findings);
    let target = packing(req);
    let trace = req.lz4 == Some(Lz4Mode::Trace);
    let unpatch = req.lz4 == Some(Lz4Mode::Unpatch);
    let kind = runtime_kind(tree.as_mut());
    // In place, `in_place_findings` says it once.
    if unpatch && packs.packed && !req.lz4_in_place {
        findings.push(format!(
            "this dump holds LZ4 packs ({MANIFEST}): unpack it first (Unpack LZ4), then unpatch \
             the unpacked copy"
        ));
    }
    if packs.packed && !packs.unpacked && kind == RuntimeKind::ForgeTrace && !unpatch {
        findings.push(
            "this packed dump carries Forge's LZ4 trace runtime: unpack it (Unpack LZ4) or repack \
             it (the LZ4 packed folder target); swapping the runtime in place would break its \
             manifest"
                .into(),
        );
    }
    if req.lz4 == Some(Lz4Mode::Unpack) && !packs.packed {
        ctx.log("unpack LZ4: nothing to unpack, the source holds no LZ4 packs");
    }
    if (trace || target || unpatch)
        && size_of(tree.as_ref(), "eboot.bin").is_some()
        && !ps5upload_fpkg::source::imports_ampr(tree.as_mut(), "eboot.bin")
    {
        findings.push(
            "eboot.bin does not import libSceAmpr: LZ4 traces and packs only work for a title \
             that uses AMPR"
                .into(),
        );
    }
    let swap = kind == RuntimeKind::ForgeTrace && !packs.packed;
    if !(trace || target || unpatch || packs.unpacked || swap) || !findings.is_empty() {
        return Ok(Prepared {
            tree,
            findings,
            pack: None,
        });
    }
    if trace || target || unpatch {
        ctx.log(runtime::WARNING);
    }
    let profile = match (&req.lz4_profile, target) {
        (Some(path), true) => match ps5_dump_forge_lz4::load_profile(path) {
            Ok(p) => Some(p),
            Err(found) => {
                findings.extend(found);
                None
            }
        },
        _ => None,
    };
    let observed = packs.observed.take();

    let installed: Option<&'static [u8]> = if trace {
        ctx.log(format!(
            "LZ4 trace: installing the tracing runtime at {RUNTIME}"
        ));
        Some(runtime::TRACE)
    } else if unpatch {
        ctx.log(match kind {
            RuntimeKind::ForgeTrace => {
                format!(
                    "LZ4 unpatch: replacing the tracing runtime at {RUNTIME} with the release one"
                )
            }
            RuntimeKind::ForgeRelease => {
                format!("LZ4 unpatch: the release runtime at {RUNTIME} is written again")
            }
            RuntimeKind::Other => {
                format!("LZ4 unpatch: replacing another runtime at {RUNTIME} with the release one")
            }
            RuntimeKind::None => {
                format!("LZ4 unpatch: installing the release runtime at {RUNTIME}")
            }
        });
        Some(runtime::RELEASE)
    } else if target || kind == RuntimeKind::ForgeTrace {
        ctx.log(match kind {
            RuntimeKind::ForgeTrace => {
                format!("LZ4: swapping the tracing runtime at {RUNTIME} for the release one")
            }
            _ => format!("LZ4: installing the release runtime at {RUNTIME}"),
        });
        Some(runtime::RELEASE)
    } else {
        None
    };
    let dropped = |p: &str| p == JOURNAL || p == INDEX || LOGS.contains(&p);
    let mut files: Vec<SourceFile> = Vec::with_capacity(tree.files().len() + 2);
    for f in tree.files() {
        if !dropped(&f.path) {
            files.push(f.clone());
        } else if f.path != INDEX {
            ctx.log(format!("LZ4: leaving out {}", f.path));
        }
    }
    let mut generated: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    if let Some(bytes) = installed {
        put(&mut files, RUNTIME, bytes.len() as u64);
        generated.insert(RUNTIME.into(), Cow::Borrowed(bytes));
    }
    // The `Lz4` target always writes a new one: the space bound counts exactly that. Unpatch
    // promises a fresh one.
    let keep = !(target || unpatch);
    let Some(index) = final_index(tree.as_mut(), &files, keep, mtime, ctx, &mut findings)? else {
        return Ok(Prepared {
            tree,
            findings,
            pack: None,
        });
    };
    let records = ps5_dump_forge_lz4::index::read_index(&index)
        .context("reading back the new ampr_emu.index")?
        .records;
    put(&mut files, INDEX, index.len() as u64);
    generated.insert(INDEX.into(), Cow::Owned(index));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut dirs = HashSet::new();
    for f in &files {
        let mut at = 0;
        while let Some(i) = f.path[at..].find('/') {
            dirs.insert(&f.path[..at + i]);
            at += i + 1;
        }
    }
    let empty_dirs: Vec<String> = tree
        .empty_dirs()
        .iter()
        .filter(|d| !dirs.contains(d.as_str()))
        .cloned()
        .collect();
    let pack = if target {
        Some(plan(
            tree.as_mut(),
            &records,
            profile.as_ref(),
            observed.as_ref(),
            req,
            ctx,
            &mut findings,
        )?)
    } else {
        None
    };
    if trace && req.format == Format::Folder {
        // ponytail: the growth figure is the design's workload assumption (1,000 submits/s at
        // 96-byte headers and ~24 bytes per read), not a measurement; recalibrate after a real
        // play session.
        ctx.log(
            "LZ4 trace: the journal and logs grow while the game runs (about 1 GB per hour, an \
             estimate) and nothing caps them; keep free space where the folder will be played",
        );
    }
    Ok(Prepared {
        tree: Box::new(Overlay {
            inner: tree,
            files,
            empty_dirs,
            generated,
        }),
        findings,
        pack,
    })
}

/// Lists `path` at `size`, replacing an entry of that path.
fn put(files: &mut Vec<SourceFile>, path: &str, size: u64) {
    files.retain(|f| f.path != path);
    files.push(SourceFile {
        path: path.into(),
        size,
    });
}

/// `ampr_emu.index` for `files` (every file but the index itself): with `keep`, the source's
/// own when it lists exactly these paths and sizes, else a new one. `None` with a finding when
/// the paths cannot be indexed.
fn final_index(
    tree: &mut dyn SourceTree,
    files: &[SourceFile],
    keep: bool,
    mtime: i64,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let rows: Vec<(String, u64)> = files.iter().map(|f| (f.path.clone(), f.size)).collect();
    if let Some(size) = size_of(tree, INDEX).filter(|&s| keep && s <= MAX_INDEX) {
        let old = read_all(tree, INDEX, size)?;
        if let Ok(index) = ps5_dump_forge_lz4::index::read_index(&old) {
            let want: BTreeSet<&(String, u64)> = rows.iter().collect();
            if index.records.iter().collect::<BTreeSet<_>>() == want {
                ctx.log(format!(
                    "LZ4: keeping {INDEX}, it lists exactly these files"
                ));
                return Ok(Some(old));
            }
        }
    }
    match ps5upload_fpkg::ampr_index::build(&rows, mtime) {
        Some(index) => {
            ctx.log(format!(
                "LZ4: writing a new {INDEX} for {} files",
                rows.len()
            ));
            Ok(Some(index))
        }
        None => {
            findings.push(format!(
                "two paths differ only in case, which {INDEX} cannot tell apart"
            ));
            Ok(None)
        }
    }
}

/// All `size` bytes of `path`.
fn read_all(tree: &mut dyn SourceTree, path: &str, size: u64) -> anyhow::Result<Vec<u8>> {
    let len = usize::try_from(size).context("too big")?;
    let bytes = tree.read_range(path, 0, len)?;
    if bytes.len() != len {
        anyhow::bail!("{path}: read {} of {size} bytes", bytes.len());
    }
    Ok(bytes)
}

/// The pack list, in `records` (final index) order: the rules pick from the profile, else the
/// files the traces saw, else the built-in guess; the keep-loose list applies to all three, then
/// auto-loose samples the large files left to pack (from `tree`).
fn plan(
    tree: &mut dyn SourceTree,
    records: &[(String, u64)],
    profile: Option<&Profile>,
    observed: Option<&BTreeSet<String>>,
    req: &ConvertRequest,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> anyhow::Result<PackPlan> {
    let (selection, rules) = match (profile, observed) {
        (Some(p), _) => {
            for key in &p.ignored {
                ctx.log(format!("LZ4 profile: {key} is not used"));
            }
            let path = req
                .lz4_profile
                .as_deref()
                .unwrap_or(std::path::Path::new(""));
            (
                Selection::Profile(p),
                format!("the profile {}", path.display()),
            )
        }
        (None, Some(o)) => (
            Selection::Observed(o),
            match &req.lz4_traces {
                Some(path) => format!(
                    "the traces copied to {} ({} files the game read)",
                    path.display(),
                    o.len()
                ),
                None => format!("this dump's traces ({} files the game read)", o.len()),
            },
        ),
        (None, None) => (
            Selection::Fallback,
            "a built-in guess (no traces; less reliable)".to_string(),
        ),
    };
    let mut files: Vec<PackFile> = records
        .iter()
        .map(|(path, size)| PackFile {
            path: path.clone(),
            size: *size,
            spec: ps5_dump_forge_lz4::select(path, *size, &selection),
        })
        .collect();
    if let Some(p) = profile {
        let kept: Vec<String> = files
            .iter()
            .filter(|f| {
                f.spec.is_none()
                    && f.size > 0
                    && !ps5_dump_forge_lz4::always_loose(&f.path)
                    && ps5_dump_forge_lz4::profile_spec(&f.path, p).is_some()
            })
            .map(|f| f.path.clone())
            .collect();
        if !kept.is_empty() {
            ctx.log(format!(
                "LZ4 profile: {} files the profile packs stay loose (the keep-loose list: the \
                 game may read them before AMPR starts): {}",
                kept.len(),
                first_ten(&kept)
            ));
        }
    }
    let auto = profile.map_or_else(AutoLoose::default, |p| p.auto_loose);
    auto_loose(tree, &mut files, &auto, ctx)?;
    // Names the runtime cannot take, and names the pack files will take.
    let bad: Vec<String> = files
        .iter()
        .filter_map(|f| {
            ps5_dump_forge_lz4::format::rel_to_logical(&f.path)
                .err()
                .map(|e| format!("{} ({e})", f.path))
        })
        .collect();
    findings.extend(listing("not a path the LZ4 runtime takes", &bad));
    let taken: Vec<String> = files
        .iter()
        .filter(|f| is_pack_name(&f.path))
        .map(|f| f.path.clone())
        .collect();
    findings.extend(listing(
        "named like an LZ4 pack file the target writes",
        &taken,
    ));
    let packed = files.iter().filter(|f| f.spec.is_some()).count();
    ctx.log(format!(
        "LZ4 rules: {rules}; {packed} files to pack, {} loose",
        files.len() - packed
    ));
    Ok(PackPlan {
        files,
        runtime: profile.and_then(|p| p.runtime),
        rules,
    })
}

/// `paths` joined, the first ten and a count of the rest.
fn first_ten(paths: &[String]) -> String {
    let mut s = paths[..paths.len().min(10)].join(", ");
    if paths.len() > 10 {
        s.push_str(&format!(" and {} more", paths.len() - 10));
    }
    s
}

/// Upstream's auto-loose over the pack list: every file `auto` applies to is sampled (a few
/// blocks spread over it, encoded as the writer would) and kept loose when the sample barely
/// shrinks. Counted as preflight progress.
fn auto_loose(
    tree: &mut dyn SourceTree,
    files: &mut [PackFile],
    auto: &AutoLoose,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let picks: Vec<(usize, Vec<u64>)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let spec = f.spec.filter(|s| auto.applies(f.size, s))?;
            Some((i, auto.sample_indices(f.size, spec.block_shift)))
        })
        .collect();
    if picks.is_empty() {
        return Ok(());
    }
    let block = |i: usize, k: u64| {
        let f = &files[i];
        let b = 1u64 << f.spec.map_or(16, |s| s.block_shift);
        b.min(f.size.saturating_sub(k * b))
    };
    let total: u64 = picks
        .iter()
        .flat_map(|(i, ks)| ks.iter().map(move |&k| (*i, k)))
        .map(|(i, k)| block(i, k))
        .sum();
    let (mut base, mut loose, mut packed) = (0u64, Vec::new(), Vec::new());
    ctx.progress("preflight", 0, total);
    for (i, ks) in &picks {
        ctx.check()?;
        let f = &files[*i];
        let shift = f.spec.map_or(16, |s| s.block_shift);
        let s = ps5_dump_forge_lz4::writer::sample(
            tree,
            &f.path,
            f.size,
            shift,
            ks,
            &mut |n| ctx.progress("preflight", base + n, total),
            ctx.cancel,
        )
        .with_context(|| format!("auto-loose: sampling {}", f.path))?;
        base += s.raw_bytes;
        let line = format!(
            "{} (saves {:.1}%, {:.0}% RAW blocks)",
            f.path,
            s.savings() * 100.0,
            s.raw_ratio() * 100.0
        );
        if auto.keeps_loose(&s) {
            loose.push(line);
            files[*i].spec = None;
        } else {
            packed.push(line);
        }
    }
    ctx.progress("preflight", total, total);
    let stay = match packed.len() {
        0 => String::new(),
        n => format!("; {n} sampled stay packed: {}", first_ten(&packed)),
    };
    if loose.is_empty() {
        ctx.log(format!(
            "auto-loose: {} large files sampled, all stay packed: {}",
            packed.len(),
            first_ten(&packed)
        ));
    } else {
        ctx.log(format!(
            "auto-loose: {} large files kept loose (incompressible samples): {}{stay}",
            loose.len(),
            first_ten(&loose)
        ));
    }
    Ok(())
}

/// Whether `path` is a name the `Lz4` target writes at the root, as a case-insensitive
/// destination sees it.
fn is_pack_name(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [MANIFEST, CRC_SIDECAR, PROFILE].contains(&p.as_str())
        || (p.starts_with("ampr_assets-") && p.ends_with(".pak") && !p.contains('/'))
}

/// Paths a trace session adds or replaces in the game folder (the PS5 patch, a traced copy),
/// and a killed patch's temporary files: left out of both sides of the membership check
/// (every other indexed file must be in the dump; the dump may hold more).
fn trace_artifact(path: &str) -> bool {
    path == JOURNAL
        || path == INDEX
        || path == RUNTIME
        || LOGS.contains(&path)
        || crate::lz4_patch::is_temp(path)
}

/// The files the game read in its last traced session, by path: the journal's file ids
/// through the path index it ran with. From `copied` (a journal with its index beside it)
/// when given, else from the dump's own. `None` (the built-in guess) without usable traces;
/// copied traces that cannot be used are a finding instead.
fn traces(
    tree: &mut dyn SourceTree,
    copied: Option<&std::path::Path>,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> anyhow::Result<Option<BTreeSet<String>>> {
    let mut copied_journal = None;
    let index = match copied {
        Some(path) => match crate::lz4_traces::open_copied(path, MAX_INDEX) {
            Ok((index, journal)) => {
                copied_journal = Some(journal);
                index
            }
            Err(e) => {
                findings.push(e);
                return Ok(None);
            }
        },
        None => {
            if size_of(tree, JOURNAL).is_none() {
                ctx.log(format!("LZ4 rules: no traces ({JOURNAL}) in this dump"));
                return Ok(None);
            }
            let Some(size) = size_of(tree, INDEX) else {
                ctx.log(format!(
                    "LZ4 rules: {JOURNAL} without {INDEX}, whose file ids it names; not using it"
                ));
                return Ok(None);
            };
            if size > MAX_INDEX {
                findings.push(format!("{INDEX} is {size} bytes, too big to be one"));
                return Ok(None);
            }
            read_all(tree, INDEX, size)?
        }
    };
    let index = match ps5_dump_forge_lz4::index::read_index(&index) {
        Ok(index) => index,
        Err(e) => {
            findings.push(format!("{INDEX}, which names the traced files: {e}"));
            return Ok(None);
        }
    };
    let listed: BTreeSet<&str> = index
        .records
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| !trace_artifact(p))
        .collect();
    let dump: BTreeSet<&str> = tree
        .files()
        .iter()
        .map(|f| f.path.as_str())
        .filter(|p| !trace_artifact(p))
        .collect();
    // Every indexed file must be in the dump: an index of files the dump lacks is another dump
    // (or version), and the journal's ids would name the wrong files. Files only in the dump
    // (a scene `.nfo` the traced copy lacked) were never seen by the game: unobserved, loose.
    let missing: Vec<String> = listed.difference(&dump).map(|p| p.to_string()).collect();
    if !missing.is_empty() {
        findings.push(format!(
            "the LZ4 traces do not belong to this dump: {INDEX}, through which {JOURNAL} names \
             the files the game read, lists files this dump does not have"
        ));
        findings.extend(listing("only in the index", &missing));
        return Ok(None);
    }
    let extra: Vec<&str> = dump.difference(&listed).copied().collect();
    if !extra.is_empty() {
        const SHOWN: usize = 10;
        let mut line = extra[..extra.len().min(SHOWN)].join(", ");
        if extra.len() > SHOWN {
            line += &format!(" and {} more", extra.len() - SHOWN);
        }
        ctx.log(format!(
            "LZ4 traces: {} files of this dump are not in {INDEX}, so the game can't have read \
             them; they stay loose: {line}",
            extra.len()
        ));
    }
    let count = u32::try_from(index.records.len()).unwrap_or(u32::MAX);
    let journal: Box<dyn Read + '_> = match copied_journal {
        Some(journal) => journal,
        None => {
            let size = size_of(tree, JOURNAL).unwrap_or(0);
            Box::new(TreeReader::new(tree, JOURNAL, size))
        }
    };
    let reader = io::BufReader::with_capacity(1 << 20, journal);
    let stats = ps5_dump_forge_lz4::journal::scan(reader, count, ctx.cancel)
        .with_context(|| format!("reading {JOURNAL}"))?;
    let observed: BTreeSet<String> = stats
        .observed
        .iter()
        .filter_map(|&id| index.records.get((id as usize).checked_sub(1)?))
        .map(|(p, _)| p.clone())
        .collect();
    ctx.log(format!(
        "LZ4 traces: {} records read, {} skipped (bad hash), {} sequence gaps, {} ended on an \
         unknown packet; {} files read by the game, {} bytes requested (requests, not bytes \
         delivered)",
        stats.records,
        stats.bad_hash,
        stats.gaps,
        stats.unknown,
        observed.len(),
        stats.requested_bytes
    ));
    if stats.unparsed_tail > 0 || stats.truncated_tail > 0 {
        ctx.log(format!(
            "LZ4 traces: the last {} bytes of {JOURNAL} could not be read as records",
            stats.unparsed_tail.saturating_add(stats.truncated_tail)
        ));
    }
    if observed.is_empty() {
        if let Some(path) = copied {
            findings.push(format!(
                "the LZ4 traces {} name no file the game read: play the traced copy (or patched \
                 folder) first",
                path.display()
            ));
            return Ok(None);
        }
        ctx.log("LZ4 rules: the traces name no file the game read; using the built-in guess");
        return Ok(None);
    }
    Ok(Some(observed))
}

/// One file of a tree as a stream.
struct TreeReader<'a> {
    tree: &'a mut dyn SourceTree,
    path: &'a str,
    pos: u64,
    size: u64,
}

impl<'a> TreeReader<'a> {
    fn new(tree: &'a mut dyn SourceTree, path: &'a str, size: u64) -> Self {
        Self {
            tree,
            path,
            pos: 0,
            size,
        }
    }
}

impl Read for TreeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let want = (self.size - self.pos).min(buf.len() as u64) as usize;
        if want == 0 {
            return Ok(0);
        }
        let bytes = self
            .tree
            .read_range(self.path, self.pos, want)
            .map_err(io::Error::other)?;
        if bytes.is_empty() || bytes.len() > want {
            return Err(io::Error::other(format!(
                "{}: read {} bytes at {} of {}",
                self.path,
                bytes.len(),
                self.pos,
                self.size
            )));
        }
        buf[..bytes.len()].copy_from_slice(&bytes);
        self.pos += bytes.len() as u64;
        Ok(bytes.len())
    }
}

/// The prepared tree: the inner tree's files less the diagnostics, plus the generated runtime
/// and index served from memory.
struct Overlay {
    inner: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    empty_dirs: Vec<String>,
    generated: HashMap<String, Cow<'static, [u8]>>,
}

impl SourceTree for Overlay {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        match self.generated.get(path) {
            Some(bytes) => Ok(bytes.to_vec()),
            None => self.inner.read(path),
        }
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let Some(bytes) = self.generated.get(path) else {
            return self.inner.read_range(path, offset, len);
        };
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start.saturating_add(len).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    fn empty_dirs(&self) -> &[String] {
        &self.empty_dirs
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// A borrowed view of `inner` holding only `files` (the loose ones), so they are extracted
/// through the same hashing tree the packer reads afterwards.
pub(crate) struct Only<'a> {
    inner: &'a mut dyn SourceTree,
    files: Vec<SourceFile>,
}

impl<'a> Only<'a> {
    pub(crate) fn new(inner: &'a mut dyn SourceTree, keep: impl Fn(&str) -> bool) -> Self {
        let files = inner
            .files()
            .iter()
            .filter(|f| keep(&f.path))
            .cloned()
            .collect();
        Self { inner, files }
    }
}

impl SourceTree for Only<'_> {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        self.inner.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        self.inner.empty_dirs()
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// The pack files, created at the root of the `.part` folder like extracted files (exclusive,
/// no link followed) and synced as each one is finished. On FreeBSD every byte goes through
/// the job's sync cadence (U5) and a retried sync fails the job.
pub(crate) struct PartOutput<'a> {
    dest: &'a mut crate::extract::Dest,
    #[cfg_attr(not(target_os = "freebsd"), allow(dead_code))]
    cancel: &'a AtomicBool,
    /// Lent to the one pack file open at a time.
    #[cfg(target_os = "freebsd")]
    cadence: Option<&'a mut crate::durable::Cadence>,
}

impl<'a> PartOutput<'a> {
    pub(crate) fn new(
        dest: &'a mut crate::extract::Dest,
        cancel: &'a AtomicBool,
        #[cfg(target_os = "freebsd")] cadence: &'a mut crate::durable::Cadence,
    ) -> Self {
        Self {
            dest,
            cancel,
            #[cfg(target_os = "freebsd")]
            cadence: Some(cadence),
        }
    }

    #[cfg(target_os = "freebsd")]
    fn wrap(&mut self, file: File) -> io::Result<crate::durable::SyncEvery<'a, File>> {
        let cadence = self
            .cadence
            .take()
            .ok_or_else(|| io::Error::other("two pack files open at once"))?;
        Ok(crate::durable::SyncEvery::new(file, cadence, self.cancel))
    }
}

#[cfg(not(target_os = "freebsd"))]
impl PackOutput for PartOutput<'_> {
    type W = File;

    fn create(&mut self, name: &str) -> ps5upload_fpkg::Result<File> {
        Ok(self.dest.create_root(name)?)
    }

    fn reopen(&mut self, name: &str) -> ps5upload_fpkg::Result<File> {
        Ok(self.dest.reopen_root(name)?)
    }

    /// A `File` buffers nothing: syncing is all there is to finish.
    fn finish(&mut self, _: &str, w: File) -> ps5upload_fpkg::Result<()> {
        Ok(w.sync_all()?)
    }
}

/// A reopened volume starts a fresh `SyncEvery` at offset 0: the packer only seeks there and
/// rewrites the 64-byte header, so nothing past it is written or zeroed.
#[cfg(target_os = "freebsd")]
impl<'a> PackOutput for PartOutput<'a> {
    type W = crate::durable::SyncEvery<'a, File>;

    fn create(&mut self, name: &str) -> ps5upload_fpkg::Result<Self::W> {
        let file = self.dest.create_root(name)?;
        Ok(self.wrap(file)?)
    }

    fn reopen(&mut self, name: &str) -> ps5upload_fpkg::Result<Self::W> {
        let file = self.dest.reopen_root(name)?;
        Ok(self.wrap(file)?)
    }

    fn finish(&mut self, _: &str, w: Self::W) -> ps5upload_fpkg::Result<()> {
        let (_, cadence) = w.finish_parts()?;
        self.cadence = Some(cadence);
        Ok(())
    }
}

/// The biggest single file the `Lz4` target writes, for the destination's file size limit
/// (FAT32's 4 GiB): a loose file, the path index (`index` bytes), the manifest or a sidecar,
/// or a volume, which never passes `VOLUME_CAP`. Packed files' own sizes do not count: their
/// chunks are spread over volumes.
pub(crate) fn largest_file(files: &[PackFile], profile: bool, index: u64) -> u64 {
    use ps5_dump_forge_lz4::writer::{VOLUME_CAP, worst_case_bytes};
    let loose = files
        .iter()
        .filter(|f| f.spec.is_none())
        .fold(index, |max, f| max.max(f.size));
    let packed: Vec<PackFile> = files.iter().filter(|f| f.spec.is_some()).cloned().collect();
    if packed.is_empty() {
        return loose;
    }
    let chunks = packed.iter().fold(0u64, |sum, f| {
        let shift = f.spec.map_or(16, |s| s.block_shift);
        sum.saturating_add(f.size.div_ceil(1 << shift))
    });
    // Manifest: header, file, chunk and pack records, then every path (as `/app0/...`, NUL
    // ended) and up to the runtime's 1024 volume names.
    let strings = files.iter().fold(1024 * 32, |sum: u64, f| {
        sum.saturating_add(f.path.len() as u64 + 7)
    });
    let manifest = (128 + 32 * 1024u64)
        .saturating_add(48u64.saturating_mul(files.len() as u64))
        .saturating_add(12u64.saturating_mul(chunks))
        .saturating_add(strings);
    let crc = 48u64.saturating_add(4u64.saturating_mul(chunks));
    let volume = worst_case_bytes(&packed, profile).min(VOLUME_CAP);
    loose.max(manifest).max(crc).max(volume)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5_dump_forge_lz4::PackSpec;

    #[test]
    fn packed_files_are_not_the_largest_output_file() {
        const GIB: u64 = 1 << 30;
        let spec = Some(PackSpec {
            block_shift: 16,
            store: false,
            hot: false,
            random: false,
        });
        let file = |path: &str, size, spec| PackFile {
            path: path.into(),
            size,
            spec,
        };
        // A 5 GiB asset that is packed goes into volumes below 4 GiB.
        let files = [
            file("eboot.bin", 100, None),
            file("data/huge.bin", 5 * GIB, spec),
        ];
        let largest = largest_file(&files, false, 4096);
        assert!(
            largest <= ps5_dump_forge_lz4::writer::VOLUME_CAP,
            "{largest}"
        );
        assert!(largest > 4 * GIB - (1 << 20), "{largest}");
        // A loose one stays as big as it is; so does a big index.
        let files = [
            file("eboot.bin", 100, None),
            file("data/huge.bin", 5 * GIB, None),
        ];
        assert_eq!(largest_file(&files, false, 4096), 5 * GIB);
        assert_eq!(largest_file(&files[..1], false, 4096), 4096);
        // Small packs: the volume is about the data, not the cap.
        let files = [
            file("eboot.bin", 100, None),
            file("data/a.bin", 300_000, spec),
        ];
        assert!(largest_file(&files, false, 4096) < 1 << 20);
    }
}
