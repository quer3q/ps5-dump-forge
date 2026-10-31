//! LZ4 asset packs in a conversion: unpacking a packed source, and the logical tree
//! the `Lz4` target packs (`ps5-dump-forge-lz4` does the bytes; this is the job's glue).
//!
//! Without an LZ4 option packs travel as plain files, and an AMPR title gets the release
//! runtime where its own is missing, older or unknown, and an `ampr_emu.index` that fits
//! ([`plain`]). Otherwise the tree the job writes is
//! an overlay: the packs decoded ([`open`]), the journal, logs and old path index left out, the
//! requested runtime installed and `ampr_emu.index` rebuilt for the final file set
//! ([`prepare`]).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
#[cfg(target_os = "freebsd")]
use std::io;
use std::sync::atomic::AtomicBool;

use anyhow::Context;
use ps5_dump_forge_lz4::runtime::{self, RuntimeKind};
use ps5_dump_forge_lz4::writer::{PackFile, PackOutput};
use ps5_dump_forge_lz4::{CRC_SIDECAR, INDEX, JOURNAL, LOGS, MANIFEST, PROFILE, RUNTIME};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::jobs::Ctx;
use crate::preflight::listing;
use crate::{ConvertRequest, Format, Lz4Mode, Lz4Runtime};

/// A runtime is a few hundred KiB; a bigger file is not read to be recognized.
const MAX_RUNTIME: u64 = 64 << 20;
/// The largest `ampr_emu.index` read whole to be compared with the files written.
// ponytail: 64 MiB is about 600,000 records; a bigger index is taken as damaged and rebuilt
// (its journal and logs left out) rather than read whole into a job's 512 MiB.
const MAX_INDEX_CHECK: u64 = 64 << 20;

/// The runtime at `fakelib/libSceAmpr.sprx`, as [`Runtime::action`] judges it.
pub(crate) struct Runtime {
    /// Which one it is ([`RuntimeKind::None`] without one).
    pub kind: RuntimeKind,
    /// The ampr_emu version it names itself with, when it does ([`runtime::version_of`]).
    pub version: Option<String>,
    /// It holds the pack format ([`runtime::reads_packs`]).
    pub packs: bool,
}

/// What a conversion does to the source's runtime (by user decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeAction {
    /// An AMPR title without one: the release runtime goes in.
    Install,
    /// Forge's trace runtime, ampr-emu-ng (outside packing and a zstd-packed source), or another
    /// one older than the bundled ampr_emu or naming no version (into packs also one that reads
    /// none): the release runtime replaces it.
    Replace,
    /// Forge's release runtime, or ampr_emu at the bundled version or newer: left as it is.
    Keep,
    /// Not an AMPR title (and not Forge's trace runtime): nothing to do.
    Untouched,
}

impl Runtime {
    /// The action for a title that imports libSceAmpr (`ampr`, [`imports_ampr`]) or not, into
    /// an output that holds LZ4 packs (`packs`: Pack, or a packed source converted as it is)
    /// or not: the one rule the jobs and Inspect share.
    pub(crate) fn action(&self, ampr: bool, packs: bool) -> RuntimeAction {
        let current = self.version.as_deref().is_some_and(runtime::not_older);
        match self.kind {
            RuntimeKind::ForgeTrace => RuntimeAction::Replace,
            _ if !ampr => RuntimeAction::Untouched,
            RuntimeKind::None => RuntimeAction::Install,
            RuntimeKind::ForgeRelease => RuntimeAction::Keep,
            // ampr-emu-ng goes into a game only by packing (by user decision): any other job
            // puts the release runtime in its place. A zstd-packed source keeps it
            // ([`Self::zstd_action`]).
            RuntimeKind::Zstd => RuntimeAction::Replace,
            RuntimeKind::Other if current && (self.packs || !packs) => RuntimeAction::Keep,
            RuntimeKind::Other => RuntimeAction::Replace,
        }
    }

    /// The action for LZ4 Pack with the runtime the request chose: ampr-emu-ng goes in unless
    /// it is there; ampr_emu follows [`Self::action`] into packs (ampr-emu-ng replaced too).
    pub(crate) fn pack_action(&self, choice: Lz4Runtime) -> RuntimeAction {
        match (choice, self.kind) {
            (Lz4Runtime::Ng, RuntimeKind::Zstd) => RuntimeAction::Keep,
            (Lz4Runtime::Ng, RuntimeKind::None) => RuntimeAction::Install,
            (Lz4Runtime::Ng, _) => RuntimeAction::Replace,
            (Lz4Runtime::AmprEmu, _) => self.action(true, true),
        }
    }

    /// The action for a zstd-packed source converted as it is: its packs need the zstd
    /// runtime, whatever `eboot.bin` says, so any other one is replaced and none installed.
    pub(crate) fn zstd_action(&self) -> RuntimeAction {
        match self.kind {
            RuntimeKind::Zstd => RuntimeAction::Keep,
            RuntimeKind::None => RuntimeAction::Install,
            _ => RuntimeAction::Replace,
        }
    }
}

/// Whether `tree`'s `eboot.bin` imports libSceAmpr.
pub(crate) fn imports_ampr(tree: &mut dyn SourceTree) -> bool {
    size_of(tree, "eboot.bin").is_some() && ps5upload_fpkg::source::imports_ampr(tree, "eboot.bin")
}

/// The runtime `tree` carries at `fakelib/libSceAmpr.sprx`.
pub(crate) fn runtime_of(tree: &mut dyn SourceTree) -> Runtime {
    let unknown = Runtime {
        kind: RuntimeKind::Other,
        version: None,
        packs: false,
    };
    let Some(size) = size_of(tree, RUNTIME) else {
        return Runtime {
            kind: RuntimeKind::None,
            ..unknown
        };
    };
    if size > MAX_RUNTIME {
        return unknown;
    }
    match tree.read_range(RUNTIME, 0, size as usize) {
        Ok(bytes) if bytes.len() as u64 == size => Runtime {
            kind: runtime::classify(&bytes),
            version: runtime::version_of(&bytes),
            packs: runtime::reads_packs(&bytes),
        },
        _ => unknown,
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
    /// For zstd packing of an LZ4-packed source: the paths its LZ4 packs held.
    pub lz4_packed: Option<BTreeSet<String>>,
    /// The tree [`open`] returned has the source's zstd packs decoded.
    pub zstd_unpacked: bool,
}

/// Whether the job packs: the `Lz4` target (a packed folder), or [`Lz4Mode::Pack`] into any
/// `format`.
pub(crate) fn packing(req: &ConvertRequest) -> bool {
    req.format == Format::Lz4 || req.lz4 == Some(Lz4Mode::Pack)
}

/// Whether the job decodes packs: unpack, or repack (LZ4 or zstd).
fn wants_unpack(req: &ConvertRequest) -> bool {
    req.lz4.is_some() || packing(req) || req.zstd_pack
}

/// The source with its packs decoded when the job asks for that. Before the backport is left
/// out: its files are records of the manifest. Packs that fail any check fail the job.
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
        if req.zstd_pack {
            packs.lz4_packed = Some(tree.packed_paths().map(str::to_owned).collect());
        }
        source = Box::new(tree);
        packs.unpacked = true;
    }
    // zstd packs are decoded to be unpacked, or packed again (zstd or LZ4).
    if (req.zstd_pack || req.zstd_unpack || packing(req))
        && ps5_dump_forge_zstd::reader::detect(source.as_ref())
    {
        let tree = ps5_dump_forge_zstd::reader::unpack(source).context("opening the zstd packs")?;
        ctx.log(format!("zstd: {}", tree.describe()));
        source = Box::new(tree);
        packs.zstd_unpacked = true;
    }
    Ok((source, packs))
}

/// The pack list and its options, for the `Lz4` target.
pub(crate) struct PackPlan {
    /// Every logical file in final `ampr_emu.index` record order; `spec` `None` stays loose.
    pub files: Vec<PackFile>,
    /// The LZ4 level every pass compresses at (`ConvertRequest::lz4_level`).
    pub level: u8,
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
    if !ps5_dump_forge_lz4::block::valid_level(req.lz4_level) {
        findings.push(format!(
            "LZ4 level {} is not 0 (fast) or 3 through 12 (HC)",
            req.lz4_level
        ));
    }
    if req.zstd_unpack && (req.lz4.is_some() || packing(req)) {
        findings.push("zstd unpack takes no LZ4 option: unpack it, then pack it".into());
    }
    findings
}

pub(crate) fn target_name(format: Format) -> String {
    match format {
        Format::Lz4 => "LZ4 packed folder".into(),
        other => crate::preflight::extension(other).map_or("folder".into(), |e| format!(".{e}")),
    }
}

/// The tree the job writes, per the request's LZ4 options (`packs` from [`open`], the
/// backport already left out). `mtime` is the job's one timestamp (integer seconds) for a
/// rebuilt `ampr_emu.index` and the pack manifest. Without an LZ4 option, [`plain`] decides.
/// Untouched sources come back as they were.
pub(crate) fn prepare(
    req: &ConvertRequest,
    mut tree: Box<dyn SourceTree>,
    packs: Packs,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<Prepared> {
    let mut findings = combinations(req);
    let target = packing(req);
    let rt = runtime_of(tree.as_mut());
    let kind = rt.kind;
    let ampr = imports_ampr(tree.as_mut());
    if req.lz4 == Some(Lz4Mode::Unpack) && !packs.packed {
        ctx.log("unpack LZ4: nothing to unpack, the source holds no LZ4 packs");
    }
    if req.zstd_unpack && !packs.zstd_unpacked {
        findings.push("zstd unpack: the source holds no zstd packs (ampr_zstd.index)".into());
    }
    if target && size_of(tree.as_ref(), "eboot.bin").is_some() && !ampr {
        findings.push(
            "eboot.bin does not import libSceAmpr: LZ4 packs only work for a title that uses \
             AMPR"
                .into(),
        );
    }
    if !findings.is_empty() {
        return Ok(Prepared {
            tree,
            findings,
            pack: None,
        });
    }
    if !(target || packs.unpacked || packs.zstd_unpacked) {
        return plain(req, tree, packs.packed, &rt, ampr, mtime, ctx);
    }
    let ng = target && req.lz4_runtime == Lz4Runtime::Ng;
    if target && !ng {
        ctx.log(runtime::WARNING);
    }
    // Pack installs the chosen runtime whatever `eboot.bin` says (the check above refuses a
    // non-AMPR one). Unpacked, the zstd runtime has nothing left to read: the release one
    // replaces it.
    let action = match kind {
        _ if target => rt.pack_action(req.lz4_runtime),
        RuntimeKind::Zstd if packs.zstd_unpacked => RuntimeAction::Replace,
        _ => rt.action(ampr, false),
    };
    let installed: Option<&'static [u8]> = match action {
        RuntimeAction::Install | RuntimeAction::Replace if ng => {
            ctx.log(format!("LZ4: installing ampr-emu-ng at {RUNTIME}"));
            Some(runtime::zstd())
        }
        RuntimeAction::Install | RuntimeAction::Replace => {
            ctx.log(release_log(kind));
            Some(runtime::release())
        }
        RuntimeAction::Keep => {
            keep_log(&rt, ctx);
            None
        }
        RuntimeAction::Untouched => None,
    };
    // Into a `.pkg` the builder writes the index from exactly what it packages (as in
    // [`plain`]); a non-AMPR title's own stays listed so that the builder rebuilds it too.
    let pkg = req.format == Format::Pkg;
    let dropped = |p: &str| p == JOURNAL || LOGS.contains(&p) || (p == INDEX && (ampr || !pkg));
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
    if pkg {
        return Ok(Prepared {
            tree: overlay(tree, files, generated),
            findings,
            pack: None,
        });
    }
    // The `Lz4` target always writes a new one: the space bound counts exactly that. So does
    // zstd unpack: the zstd runtime's index listed its size.
    let keep = !target && !packs.zstd_unpacked;
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
    let pack = if target {
        Some(plan(tree.as_mut(), &records, req, ctx, &mut findings)?)
    } else {
        None
    };
    Ok(Prepared {
        tree: overlay(tree, files, generated),
        findings,
        pack,
    })
}

/// The job log's line for the release runtime going in where `kind` was.
fn release_log(kind: RuntimeKind) -> String {
    match kind {
        RuntimeKind::ForgeTrace => {
            format!("LZ4: swapping the tracing runtime at {RUNTIME} for the release one")
        }
        RuntimeKind::Other => format!(
            "LZ4: replacing the game's runtime at {RUNTIME} with the bundled ampr_emu {}",
            runtime::VERSION
        ),
        RuntimeKind::Zstd => format!(
            "LZ4: replacing ampr-emu-ng at {RUNTIME} with the bundled ampr_emu {}",
            runtime::VERSION
        ),
        _ => format!("LZ4: installing the release runtime at {RUNTIME}"),
    }
}

/// The job log's line for a runtime left as it is (another build, not older than ours).
fn keep_log(rt: &Runtime, ctx: &Ctx) {
    if let (RuntimeKind::Other, Some(v)) = (rt.kind, &rt.version) {
        ctx.log(format!(
            "LZ4: keeping the game's ampr_emu {v} at {RUNTIME} (the bundled one is {})",
            runtime::VERSION
        ));
    }
}

/// What a `.pkg` of an AMPR title leaves out of `all`, since the builder writes the index from
/// exactly what it packages: the journal and logs (their file ids follow the old index's
/// order), and when `ampr` (`eboot.bin` imports libSceAmpr) the old index too. The paths left
/// out come back, the index not among them.
pub(crate) fn leave_out_for_pkg(all: &mut Vec<SourceFile>, ampr: bool) -> Vec<String> {
    let mut gone = Vec::new();
    all.retain(|f| {
        let trace = f.path == JOURNAL || LOGS.contains(&f.path.as_str());
        if trace {
            gone.push(f.path.clone());
        }
        !(trace || (ampr && f.path == INDEX))
    });
    gone
}

/// A conversion without LZ4 options (by user decision). An AMPR title (`eboot.bin` imports
/// libSceAmpr) gets the release runtime per [`Runtime::action`] (installed where there is none,
/// replacing ampr-emu-ng and an older or unknown one, another as new as ours kept) and keeps its
/// `ampr_emu.index` only when it lists exactly the files written ([`index_state`]); else a new
/// one is written, and the journal and logs are left out with the old one: the journal's file
/// ids follow its record order. Into a `.pkg` the builder writes the index from the files it
/// packages, so the old one, the journal and logs are left out here. A packed source keeps its
/// index, journal and logs byte for byte. Forge's trace runtime is swapped whatever the title.
/// Anything else comes back untouched.
fn plain(
    req: &ConvertRequest,
    mut tree: Box<dyn SourceTree>,
    packed: bool,
    rt: &Runtime,
    ampr: bool,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<Prepared> {
    let untouched = |tree| {
        Ok(Prepared {
            tree,
            findings: Vec::new(),
            pack: None,
        })
    };
    // zstd packs travel as plain files too, read by the zstd runtime only. By the magic, as
    // Inspect labels it: a stray file of that name gets the plain repair.
    let zstd = !packed && crate::zstd_pack::packed(tree.as_mut());
    let action = if zstd {
        rt.zstd_action()
    } else {
        rt.action(ampr, packed)
    };
    let release = match action {
        RuntimeAction::Untouched => return untouched(tree),
        RuntimeAction::Keep => {
            keep_log(rt, ctx);
            false
        }
        RuntimeAction::Install if zstd => {
            ctx.log(format!("zstd: installing the zstd runtime at {RUNTIME}"));
            true
        }
        RuntimeAction::Replace if zstd => {
            ctx.log(format!(
                "zstd: replacing the runtime at {RUNTIME} with the zstd runtime"
            ));
            true
        }
        RuntimeAction::Install | RuntimeAction::Replace => {
            ctx.log(release_log(rt.kind));
            true
        }
    };
    let bytes = if zstd {
        runtime::zstd()
    } else {
        runtime::release()
    };
    let mut generated: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    let mut all = tree.files().to_vec();
    if release {
        generated.insert(RUNTIME.into(), Cow::Borrowed(bytes));
        put(&mut all, RUNTIME, bytes.len() as u64);
    }
    // The zstd index lists the packed files, which the folder does not hold as such.
    if packed || zstd {
        // The pack manifest's file ids are the index's records: never rebuilt.
        ctx.log(format!(
            "LZ4: keeping {INDEX} as it is: it must match the packs"
        ));
        return untouched(if release {
            overlay(tree, all, generated)
        } else {
            tree
        });
    }
    let traces = |f: &SourceFile| f.path == JOURNAL || LOGS.contains(&f.path.as_str());
    if req.format == Format::Pkg {
        // Built only after the builder leaves out SDK leftovers, from exactly what it packages
        // (an AMPR title's always; another's only when it has one, so that one stays).
        let before = all.len();
        for path in leave_out_for_pkg(&mut all, ampr) {
            ctx.log(format!("LZ4: leaving out {path}"));
        }
        if all.len() == before && !release {
            return untouched(tree);
        }
        return untouched(overlay(tree, all, generated));
    }
    let mut files = plain_listing(tree.files(), action);
    let state = index_state(tree.as_mut(), &files)?;
    match state {
        IndexState::Ok(_) => {
            return untouched(if release {
                overlay(tree, all, generated)
            } else {
                tree
            });
        }
        IndexState::Stale => ctx.log(format!(
            "LZ4: {INDEX} does not list exactly the files written (stale)"
        )),
        IndexState::Damaged => ctx.log(format!("LZ4: {INDEX} does not read (damaged)")),
        IndexState::Missing => ctx.log(format!("LZ4: no {INDEX} yet")),
    }
    for f in all.iter().filter(|f| traces(f)) {
        ctx.log(format!("LZ4: leaving out {}", f.path));
    }
    let mut findings = Vec::new();
    let Some(index) = final_index(tree.as_mut(), &files, false, mtime, ctx, &mut findings)? else {
        return Ok(Prepared {
            tree,
            findings,
            pack: None,
        });
    };
    put(&mut files, INDEX, index.len() as u64);
    generated.insert(INDEX.into(), Cow::Owned(index));
    untouched(overlay(tree, files, generated))
}

/// `listing` as written beside `ampr_emu.index` by a conversion without LZ4 options: without
/// the index, the journal and the logs, with the release runtime when `action` installs or
/// replaces one. What [`index_state`] compares with there.
pub(crate) fn plain_listing(listing: &[SourceFile], action: RuntimeAction) -> Vec<SourceFile> {
    let mut files: Vec<SourceFile> = listing
        .iter()
        .filter(|f| f.path != INDEX && f.path != JOURNAL && !LOGS.contains(&f.path.as_str()))
        .cloned()
        .collect();
    if matches!(action, RuntimeAction::Install | RuntimeAction::Replace) {
        put(&mut files, RUNTIME, runtime::release().len() as u64);
    }
    files
}

/// `inner` as `files`, the `generated` ones served from memory; its empty folders that now
/// hold a file are left out. `files` keeps the source's order (no sort): a solid archive is read
/// front to back, and an entry it already went past starts its decode again.
pub(crate) fn overlay(
    inner: Box<dyn SourceTree>,
    files: Vec<SourceFile>,
    generated: HashMap<String, Cow<'static, [u8]>>,
) -> Box<dyn SourceTree> {
    let mut dirs = HashSet::new();
    for f in &files {
        let mut at = 0;
        while let Some(i) = f.path[at..].find('/') {
            dirs.insert(&f.path[..at + i]);
            at += i + 1;
        }
    }
    let empty_dirs: Vec<String> = inner
        .empty_dirs()
        .iter()
        .filter(|d| !dirs.contains(d.as_str()))
        .cloned()
        .collect();
    Box::new(Overlay {
        inner,
        files,
        empty_dirs,
        generated,
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

/// How the source's `ampr_emu.index` fits the files written beside it.
pub(crate) enum IndexState {
    /// It lists exactly their paths and sizes (its bytes).
    Ok(Vec<u8>),
    /// It reads but lists other files or sizes, or is too big to list just these.
    Stale,
    /// It does not read, or is too big to be any index.
    Damaged,
    Missing,
}

/// The one comparison of the source's `ampr_emu.index` with `files` (every file written but the
/// index itself), for conversions and Inspect alike.
pub(crate) fn index_state(
    tree: &mut dyn SourceTree,
    files: &[SourceFile],
) -> anyhow::Result<IndexState> {
    Ok(index_states(tree, &[files])?.remove(0))
}

/// [`index_state`] against each of `listings`, the index read at most once (a remote source
/// keeps no read).
pub(crate) fn index_states(
    tree: &mut dyn SourceTree,
    listings: &[&[SourceFile]],
) -> anyhow::Result<Vec<IndexState>> {
    let Some(size) = size_of(tree, INDEX) else {
        return Ok(listings.iter().map(|_| IndexState::Missing).collect());
    };
    // Its bytes and records (`None`: it doesn't read), once a listing needs them.
    let mut read: Option<(Vec<u8>, Option<ps5_dump_forge_lz4::index::PathIndex>)> = None;
    let mut states = Vec::with_capacity(listings.len());
    let mut first_ok = None;
    for files in listings {
        // Bounded before it is read: one that lists exactly `files` is about a fresh one's size,
        // so anything bigger is `Stale`; one that could fit but is over the read cap is `Damaged`.
        if size > index_bound(files) {
            states.push(IndexState::Stale);
            continue;
        }
        if size > MAX_INDEX_CHECK {
            states.push(IndexState::Damaged);
            continue;
        }
        if read.is_none() {
            let old = read_all(tree, INDEX, size)?;
            let index = ps5_dump_forge_lz4::index::read_index(&old).ok();
            read = Some((old, index));
        }
        let Some((old, Some(index))) = &read else {
            states.push(IndexState::Damaged);
            continue;
        };
        let want: BTreeSet<(&str, u64)> = files.iter().map(|f| (f.path.as_str(), f.size)).collect();
        let have: BTreeSet<(&str, u64)> = (index.records.iter())
            .map(|(p, s)| (p.as_str(), *s))
            .collect();
        states.push(if have != want {
            IndexState::Stale
        } else if first_ok.is_none() {
            // Its bytes are moved in below, not copied.
            first_ok = Some(states.len());
            IndexState::Ok(Vec::new())
        } else {
            IndexState::Ok(old.clone())
        });
    }
    if let (Some(i), Some((old, _))) = (first_ok, read) {
        states[i] = IndexState::Ok(old);
    }
    Ok(states)
}

/// The largest `ampr_emu.index` that could list exactly `files`: four times a fresh one's size
/// (0x30-byte header, 24 bytes a record, the `/app0/` paths, 16-byte slots at twice the
/// records), plus 1 MiB, room for another tool's slot table or padding.
fn index_bound(files: &[SourceFile]) -> u64 {
    let n = files.len() as u64;
    let paths: u64 = files.iter().map(|f| f.path.len() as u64 + 7).sum();
    let fresh = 0x30 + 24 * n + paths + 16 + 16 * (2 * n).next_power_of_two().max(2);
    4 * fresh + (1 << 20)
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
    if keep && let IndexState::Ok(old) = index_state(tree, files)? {
        ctx.log(format!(
            "LZ4: keeping {INDEX}, it lists exactly these files"
        ));
        return Ok(Some(old));
    }
    let rows: Vec<(String, u64)> = files.iter().map(|f| (f.path.clone(), f.size)).collect();
    match ps5upload_fpkg::ampr_index::build(&rows, mtime) {
        Ok(index) => {
            ctx.log(format!(
                "LZ4: writing a new {INDEX} for {} files",
                rows.len()
            ));
            Ok(Some(index))
        }
        Err(e) => {
            findings.push(e);
            Ok(None)
        }
    }
}

/// All `size` bytes of `path`, in as many reads as the source takes (an `.pkg`'s serves at
/// most 64 MiB each); only one that returns nothing early is a short file.
fn read_all(tree: &mut dyn SourceTree, path: &str, size: u64) -> anyhow::Result<Vec<u8>> {
    let len = usize::try_from(size).context("too big")?;
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        let chunk = tree.read_range(path, bytes.len() as u64, len - bytes.len())?;
        if chunk.is_empty() {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != len {
        anyhow::bail!("{path}: read {} of {size} bytes", bytes.len());
    }
    Ok(bytes)
}

/// The pack list, in `records` (final index) order: the built-in guess (the keep-loose list and
/// root files stay loose), then auto-loose samples the large files left to pack (from `tree`).
fn plan(
    tree: &mut dyn SourceTree,
    records: &[(String, u64)],
    req: &ConvertRequest,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> anyhow::Result<PackPlan> {
    let mut files: Vec<PackFile> = records
        .iter()
        .map(|(path, size)| PackFile {
            path: path.clone(),
            size: *size,
            spec: ps5_dump_forge_lz4::select(path, *size),
        })
        .collect();
    auto_loose(tree, &mut files, req.lz4_level, ctx)?;
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
        "LZ4 rules: the built-in guess; {packed} files to pack, {} loose; level {}",
        files.len() - packed,
        req.lz4_level
    ));
    Ok(PackPlan {
        files,
        level: req.lz4_level,
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

/// Upstream's auto-loose over the pack list: every file it applies to is sampled (a few
/// blocks spread over it, encoded as the writer would) and kept loose when the sample barely
/// shrinks. Counted as preflight progress.
fn auto_loose(
    tree: &mut dyn SourceTree,
    files: &mut [PackFile],
    level: u8,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let picks: Vec<(usize, Vec<u64>)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let spec = f
                .spec
                .filter(|s| ps5_dump_forge_lz4::auto_loose_applies(f.size, s))?;
            Some((
                i,
                ps5_dump_forge_lz4::sample_indices(f.size, spec.block_shift),
            ))
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
    // A sliver of the job: every pass after it reads each file at least once.
    let rest = tree
        .files()
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    ctx.expect_rest(total.saturating_add(rest));
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
            level,
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
        if s.keeps_loose() {
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

    /// A `File` buffers nothing: syncing is all there is to finish (durable with the caller's
    /// `Dest::sync`, as for every extracted file).
    fn finish(&mut self, _: &str, w: File) -> ps5upload_fpkg::Result<()> {
        Ok(crate::extract::sync_file(&w)?)
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

/// The zstd packer writes through the same files as the LZ4 one.
impl ps5_dump_forge_zstd::writer::PackOutput for PartOutput<'_> {
    type W = <Self as PackOutput>::W;

    fn create(&mut self, name: &str) -> ps5upload_fpkg::Result<Self::W> {
        PackOutput::create(self, name)
    }

    fn reopen(&mut self, name: &str) -> ps5upload_fpkg::Result<Self::W> {
        PackOutput::reopen(self, name)
    }

    fn finish(&mut self, name: &str, w: Self::W) -> ps5upload_fpkg::Result<()> {
        PackOutput::finish(self, name, w)
    }
}

/// The biggest single file the `Lz4` target writes, for the destination's file size limit
/// (FAT32's 4 GiB): a loose file, the path index (`index` bytes), the manifest or a sidecar,
/// or a volume, which never passes `VOLUME_CAP`. Packed files' own sizes do not count: their
/// chunks are spread over volumes.
pub(crate) fn largest_file(files: &[PackFile], index: u64) -> u64 {
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
    let volume = worst_case_bytes(&packed, false).min(VOLUME_CAP);
    loose.max(manifest).max(crc).max(volume)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing whose `ampr_emu.index` serves zeros and fails any read past 1 MiB.
    struct Sized(Vec<SourceFile>);

    impl SourceTree for Sized {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            let size = size_of(self, path).unwrap_or(0);
            self.read_range(path, 0, size as usize)
        }
        fn read_range(&mut self, _: &str, _: u64, len: usize) -> ps5upload_fpkg::Result<Vec<u8>> {
            assert!(len <= 1 << 20, "a {len}-byte read");
            Ok(vec![0; len])
        }
        fn describe(&self) -> String {
            "sized".into()
        }
    }

    #[test]
    fn an_index_too_big_for_the_listing_is_not_read() {
        let file = |path: &str, size| SourceFile {
            path: path.into(),
            size,
        };
        let files = vec![file("eboot.bin", 100), file("data/a.bin", 5)];
        let mut tree = Sized(vec![file(INDEX, 200 << 20)]);
        assert!(matches!(
            index_state(&mut tree, &files).unwrap(),
            IndexState::Stale
        ));
        let mut tree = Sized(vec![file(INDEX, 4096)]);
        assert!(matches!(
            index_state(&mut tree, &files).unwrap(),
            IndexState::Damaged
        ));
        // 200,000 files of 60-byte paths: their index fits the bound.
        let many: Vec<SourceFile> = (0..200_000)
            .map(|i| file(&format!("data/{i:0>55}"), 1))
            .collect();
        let fresh = ps5upload_fpkg::ampr_index::build(
            &many
                .iter()
                .map(|f| (f.path.clone(), f.size))
                .collect::<Vec<_>>(),
            0,
        )
        .unwrap();
        assert!((fresh.len() as u64) < index_bound(&many) / 3);
        // Within that bound but past the whole-file cap: damaged, never read.
        assert!(index_bound(&many) > 100 << 20);
        let mut tree = Sized(vec![file(INDEX, 100 << 20)]);
        assert!(matches!(
            index_state(&mut tree, &many).unwrap(),
            IndexState::Damaged
        ));
    }
}
