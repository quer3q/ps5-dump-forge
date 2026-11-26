//! LZ4 asset packs in a conversion: unpacking a packed source, and the logical tree
//! the `Lz4` target packs, and the AMPR runtime and index rules (AGENTS.md); `ps5-dump-forge-lz4` does the bytes.

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
// ponytail: 64 MiB is about 600,000 records; a bigger index is taken as damaged and rebuilt.
const MAX_INDEX_CHECK: u64 = 64 << 20;

/// The runtime at `fakelib/libSceAmpr.sprx`, as [`Runtime::action`] judges it.
pub(crate) struct Runtime {
    /// Which one it is ([`RuntimeKind::None`] without one).
    pub kind: RuntimeKind,
    /// Its self-named version (ampr_emu's or ampr-emu-ng's); an ampr-emu-ng's decides Keep or Replace.
    pub version: Option<String>,
    /// It holds the pack format ([`runtime::reads_packs`]).
    pub packs: bool,
    /// It is the bundled ampr-emu-ng byte for byte ([`runtime::zstd`]).
    pub bundled: bool,
}

/// What a job does to the source's runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeAction {
    /// An AMPR title without one: the bundled ampr-emu-ng goes in.
    Install,
    /// Forge's trace runtime, an older or unversioned ampr-emu-ng, or (with `replace_ampr_emu`) ampr_emu: replaced.
    Replace,
    /// ampr-emu-ng at the bundled version or newer, or an ampr_emu build the request keeps.
    Keep,
    /// A `.pkg` output: no AMPR runtime goes in, the one there is left out; the console's loader provides it.
    Omit,
    /// Not an AMPR title: nothing to do.
    Untouched,
}

impl Runtime {
    /// ampr-emu-ng at the bundled version or newer (the bundled build byte for byte included).
    fn ng_current(&self) -> bool {
        self.kind == RuntimeKind::Zstd
            && (self.bundled || self.version.as_deref().is_some_and(runtime::ng_not_older))
    }

    /// The one rule jobs and Inspect share, for an AMPR title (`ampr`) or not, into an output holding LZ4 packs (`packs`) or not; not for a `.pkg`.
    pub(crate) fn action(&self, ampr: bool, packs: bool, replace_ampr_emu: bool) -> RuntimeAction {
        match self.kind {
            RuntimeKind::ForgeTrace => RuntimeAction::Replace,
            _ if !ampr => RuntimeAction::Untouched,
            RuntimeKind::None => RuntimeAction::Install,
            RuntimeKind::Zstd if self.ng_current() => RuntimeAction::Keep,
            RuntimeKind::Zstd => RuntimeAction::Replace,
            // An ampr_emu build (Forge's old release one, another, or a file that doesn't read).
            RuntimeKind::ForgeRelease | RuntimeKind::Other
                if !replace_ampr_emu && (self.packs || !packs) =>
            {
                RuntimeAction::Keep
            }
            RuntimeKind::ForgeRelease | RuntimeKind::Other => RuntimeAction::Replace,
        }
    }

    /// Where only ampr-emu-ng serves the output (Pack, a zstd-packed source as it is): a current one kept, none installed, anything else replaced (#59).
    pub(crate) fn ng_action(&self) -> RuntimeAction {
        match self.kind {
            _ if self.ng_current() => RuntimeAction::Keep,
            RuntimeKind::None => RuntimeAction::Install,
            _ => RuntimeAction::Replace,
        }
    }

    /// Into a `.pkg`: [`RuntimeAction::Omit`] for an AMPR title or wherever a runtime is, else untouched.
    pub(crate) fn pkg_action(&self, ampr: bool) -> RuntimeAction {
        if ampr || self.kind != RuntimeKind::None {
            RuntimeAction::Omit
        } else {
            RuntimeAction::Untouched
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
        bundled: false,
    };
    let Some(size) = size_of(tree, RUNTIME) else {
        return Runtime {
            kind: RuntimeKind::None,
            ..unknown
        };
    };
    // An empty file is a runtime that doesn't read, not none: replaced, or left out of a `.pkg`.
    if size == 0 || size > MAX_RUNTIME {
        return unknown;
    }
    match tree.read_range(RUNTIME, 0, size as usize) {
        Ok(bytes) if bytes.len() as u64 == size => {
            let kind = runtime::classify(&bytes);
            Runtime {
                kind,
                version: if kind == RuntimeKind::Zstd {
                    runtime::ng_version_of(&bytes)
                } else {
                    runtime::version_of(&bytes)
                },
                packs: runtime::reads_packs(&bytes),
                bundled: bytes == runtime::zstd(),
            }
        }
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
    /// ... and [`open`] decoded them.
    pub unpacked: bool,
    /// The tree [`open`] returned has the source's zstd packs decoded.
    pub zstd_unpacked: bool,
}

/// Whether the job packs: the `Lz4` target, or [`Lz4Mode::Pack`] into any `format`.
pub(crate) fn packing(req: &ConvertRequest) -> bool {
    req.format == Format::Lz4 || req.lz4 == Some(Lz4Mode::Pack)
}

/// Whether the job decodes packs: unpack, or repack (LZ4 or zstd).
fn wants_unpack(req: &ConvertRequest) -> bool {
    req.lz4.is_some() || packing(req) || req.zstd_pack
}

/// A plain request taking a packed game into a `.pkg`, as the unpack it becomes (issue #73); additional content keeps its packs.
pub(crate) fn auto_unpack(
    req: &ConvertRequest,
    tree: &mut dyn SourceTree,
) -> anyhow::Result<Option<ConvertRequest>> {
    if req.format != Format::Pkg
        || req.zstd_pack
        || packing(req)
        || size_of(tree, "eboot.bin").is_none()
    {
        return Ok(None);
    }
    // zstd by the magic, as Inspect labels it: a stray file of that name is an ordinary file.
    let zstd = crate::zstd_pack::packed(tree);
    let lz4 = ps5_dump_forge_lz4::reader::detect(tree).context("looking for LZ4 packs")?;
    if zstd && lz4 {
        // ponytail: one unpack per job; a game holding both kinds is refused, not half unpacked.
        anyhow::bail!(
            "this game holds both zstd and LZ4 packs, and a .pkg carries none: unpack it into a \
             folder first (--zstd-unpack, then --lz4-unpack)"
        );
    }
    let mut auto = req.clone();
    if zstd && !lz4 && !req.zstd_unpack {
        // An LZ4 unpack asked of a zstd-packed game becomes the zstd one.
        auto.zstd_unpack = true;
        auto.lz4 = None;
    } else if lz4 && req.lz4.is_none() && !req.zstd_unpack {
        auto.lz4 = Some(Lz4Mode::Unpack);
    } else {
        return Ok(None);
    }
    Ok(Some(auto))
}

/// The source with its packs decoded when the job asks (before the backport goes: its files are manifest records).
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
        log_duplicates(ctx, "LZ4", tree.identical_paths(), tree.differing_paths());
        source = Box::new(tree);
        packs.unpacked = true;
    }
    // zstd packs are decoded to be unpacked, or packed again (zstd or LZ4).
    if (req.zstd_pack || req.zstd_unpack || packing(req))
        && ps5_dump_forge_zstd::reader::detect(source.as_ref())
    {
        let tree = ps5_dump_forge_zstd::reader::unpack(source).context("opening the zstd packs")?;
        ctx.log(format!("zstd: {}", tree.describe()));
        log_duplicates(ctx, "zstd", tree.identical_paths(), tree.differing_paths());
        source = Box::new(tree);
        packs.zstd_unpacked = true;
    }
    Ok((source, packs))
}

/// One line for the packed files also on disk (none: no line), the first 10 named.
fn log_duplicates(ctx: &Ctx, kind: &str, same: &[String], other: &[String]) {
    if same.is_empty() && other.is_empty() {
        return;
    }
    let mut parts = Vec::new();
    if !same.is_empty() {
        parts.push(format!("{} identical (plain copy kept)", same.len()));
    }
    if !other.is_empty() {
        parts.push(format!("{} different (packed copy used)", other.len()));
    }
    let paths: Vec<&str> = same
        .iter()
        .chain(other)
        .take(10)
        .map(String::as_str)
        .collect();
    ctx.log(format!(
        "{kind}: {} files both packed and on disk: {}: {}",
        same.len() + other.len(),
        parts.join(", "),
        paths.join(", ")
    ));
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

/// The findings the request's LZ4 options make on their own.
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

/// The tree the job writes per the request's LZ4 options (`packs` from [`open`]); `mtime` stamps a rebuilt index and the manifest; without an LZ4 option, [`plain`].
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
    let ampr = imports_ampr(tree.as_mut());
    if req.lz4 == Some(Lz4Mode::Unpack) && !packs.packed {
        ctx.log("unpack LZ4: nothing to unpack, the source holds no LZ4 packs");
    }
    if req.zstd_unpack && !packs.zstd_unpacked {
        findings.push("zstd unpack: the source holds no zstd packs (ampr_zstd.index)".into());
    }
    if target && req.lz4_runtime != Lz4Runtime::Ng {
        findings.push(
            "LZ4 packs get the ampr-emu-ng runtime (ng): ampr_emu is no longer bundled".into(),
        );
    } else if target && size_of(tree.as_ref(), "eboot.bin").is_some() && !ampr {
        // A title without AMPR is packed too: ShadowMount+ force-loads ampr-emu-ng.
        ctx.log("LZ4: eboot.bin does not import libSceAmpr; injecting ampr-emu-ng");
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
    // Into a `.pkg` no runtime goes; Pack installs ampr-emu-ng; unpacked, the plain conversion's rule.
    let pkg = req.format == Format::Pkg;
    // A zstd unpack's lines are zstd's; LZ4 unpack and LZ4 Pack keep the LZ4 tag.
    let tag = if packs.zstd_unpacked && !target {
        "zstd"
    } else {
        "LZ4"
    };
    let action = if pkg {
        rt.pkg_action(ampr)
    } else if target {
        rt.ng_action()
    } else {
        rt.action(ampr, false, req.replace_ampr_emu)
    };
    let installed: Option<&'static [u8]> = match action {
        RuntimeAction::Install | RuntimeAction::Replace => {
            ctx.log(install_log(tag, &rt));
            Some(runtime::zstd())
        }
        RuntimeAction::Keep => {
            keep_log(tag, &rt, ctx);
            None
        }
        RuntimeAction::Omit => {
            omit_log(tag, &rt, ctx);
            None
        }
        RuntimeAction::Untouched => None,
    };
    // Into a `.pkg` the builder indexes what it packages; a non-AMPR title's own index stays listed so it is rebuilt too.
    let dropped = |p: &str| {
        p == JOURNAL || LOGS.contains(&p) || (p == INDEX && (ampr || !pkg)) || (p == RUNTIME && pkg)
    };
    let mut files: Vec<SourceFile> = Vec::with_capacity(tree.files().len() + 2);
    for f in tree.files() {
        if !dropped(&f.path) {
            files.push(f.clone());
        } else if f.path != INDEX && f.path != RUNTIME {
            ctx.log(format!("{tag}: leaving out {}", f.path));
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
    // The `Lz4` target and zstd unpack always write a new index (the space bound counts it).
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

/// The log line for the bundled ampr-emu-ng going in where `rt` was.
pub(crate) fn install_log(tag: &str, rt: &Runtime) -> String {
    let ng = runtime::NG_VERSION;
    let was = match (rt.kind, &rt.version) {
        (RuntimeKind::None, _) => {
            return format!("{tag}: installing ampr-emu-ng {ng} at {RUNTIME}");
        }
        (RuntimeKind::ForgeTrace, _) => "the tracing runtime".to_string(),
        (RuntimeKind::Zstd, Some(v)) => format!("ampr-emu-ng {v}"),
        (RuntimeKind::Zstd, None) => "an ampr-emu-ng naming no version".to_string(),
        (_, Some(v)) => format!("the game's ampr_emu {v}"),
        (_, None) => "the game's runtime".to_string(),
    };
    format!("{tag}: replacing {was} at {RUNTIME} with the bundled ampr-emu-ng {ng}")
}

/// The log line for a runtime kept: a non-older ampr-emu-ng, or ampr_emu the request keeps.
pub(crate) fn keep_log(tag: &str, rt: &Runtime, ctx: &Ctx) {
    let v = rt
        .version
        .as_deref()
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    match rt.kind {
        RuntimeKind::Zstd if !rt.bundled => ctx.log(format!(
            "{tag}: keeping the game's ampr-emu-ng{v} at {RUNTIME} (the bundled one is {})",
            runtime::NG_VERSION
        )),
        RuntimeKind::Other | RuntimeKind::ForgeRelease => ctx.log(format!(
            "{tag}: keeping the game's ampr_emu{v} at {RUNTIME} (replace_ampr_emu is off)"
        )),
        _ => {}
    }
}

fn omit_log(tag: &str, rt: &Runtime, ctx: &Ctx) {
    ctx.log(if rt.kind == RuntimeKind::None {
        format!("{tag}: no AMPR runtime goes into a .pkg: the console's loader provides it")
    } else {
        format!(
            "{tag}: leaving out {RUNTIME}: a .pkg carries no AMPR runtime, the console's loader \
             provides it"
        )
    });
}

/// What a `.pkg` leaves out of `all`: the runtime always; with `traces` the journal and logs, and with `ampr` the old index. Returns the paths left out, bar the index and runtime.
pub(crate) fn leave_out_for_pkg(
    all: &mut Vec<SourceFile>,
    ampr: bool,
    traces: bool,
) -> Vec<String> {
    let mut gone = Vec::new();
    all.retain(|f| {
        let trace = traces && (f.path == JOURNAL || LOGS.contains(&f.path.as_str()));
        if trace {
            gone.push(f.path.clone());
        }
        !(trace || f.path == RUNTIME || (traces && ampr && f.path == INDEX))
    });
    gone
}

/// A conversion without LZ4 options: the runtime by [`Runtime::action`] and the index kept only when [`index_state`] says it fits (AGENTS.md's runtime and index rules).
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
    // zstd packs travel as plain files too; by the magic, as Inspect labels it.
    let zstd = !packed && crate::zstd_pack::packed(tree.as_mut());
    let pkg = req.format == Format::Pkg;
    let action = if pkg {
        rt.pkg_action(ampr)
    } else if zstd {
        rt.ng_action()
    } else {
        rt.action(ampr, packed, req.replace_ampr_emu)
    };
    let tag = if zstd { "zstd" } else { "LZ4" };
    let install = match action {
        RuntimeAction::Untouched => return untouched(tree),
        RuntimeAction::Keep => {
            keep_log(tag, rt, ctx);
            false
        }
        RuntimeAction::Omit => {
            omit_log(tag, rt, ctx);
            false
        }
        RuntimeAction::Install | RuntimeAction::Replace => {
            ctx.log(install_log(tag, rt));
            true
        }
    };
    let bytes = runtime::zstd();
    let mut generated: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    let mut all = tree.files().to_vec();
    if install {
        generated.insert(RUNTIME.into(), Cow::Borrowed(bytes));
        put(&mut all, RUNTIME, bytes.len() as u64);
    }
    let traces = |f: &SourceFile| f.path == JOURNAL || LOGS.contains(&f.path.as_str());
    if pkg {
        // The builder indexes what survives its SDK filter; a packed source's index is kept as it is.
        let before = all.len();
        for path in leave_out_for_pkg(&mut all, ampr, !(packed || zstd)) {
            ctx.log(format!("LZ4: leaving out {path}"));
        }
        if all.len() == before {
            return untouched(tree);
        }
        return untouched(overlay(tree, all, generated));
    }
    // The zstd index lists the packed files, which the folder does not hold as such.
    if packed || zstd {
        // The pack manifest's file ids are the index's records: never rebuilt.
        ctx.log(format!(
            "LZ4: keeping {INDEX} as it is: it must match the packs"
        ));
        return untouched(if install {
            overlay(tree, all, generated)
        } else {
            tree
        });
    }
    let mut files = plain_listing(tree.files(), action);
    let state = index_state(tree.as_mut(), &files)?;
    match state {
        IndexState::Ok(_) => {
            return untouched(if install {
                overlay(tree, all, generated)
            } else {
                tree
            });
        }
        IndexState::Stale => ctx.log(format!(
            "LZ4: {INDEX} does not list exactly the files written (stale)"
        )),
        IndexState::Damaged(why) => ctx.log(format!("LZ4: {INDEX} does not read (damaged): {why}")),
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

/// `listing` as a conversion without LZ4 options writes it beside `ampr_emu.index` (index, journal and logs out, the runtime as `action` puts it).
pub(crate) fn plain_listing(listing: &[SourceFile], action: RuntimeAction) -> Vec<SourceFile> {
    let mut files: Vec<SourceFile> = listing
        .iter()
        .filter(|f| f.path != INDEX && f.path != JOURNAL && !LOGS.contains(&f.path.as_str()))
        .cloned()
        .collect();
    if matches!(action, RuntimeAction::Install | RuntimeAction::Replace) {
        put(&mut files, RUNTIME, runtime::zstd().len() as u64);
    }
    files
}

/// `inner` as `files`, the `generated` ones served from memory; `files` keeps the source's order (a solid archive is read front to back).
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
    /// It does not read, or is too big to be any index (why, for the log).
    Damaged(String),
    Missing,
}

/// The one comparison of the source's `ampr_emu.index` with `files`, for conversions and Inspect alike.
pub(crate) fn index_state(
    tree: &mut dyn SourceTree,
    files: &[SourceFile],
) -> anyhow::Result<IndexState> {
    Ok(index_states(tree, &[files])?.remove(0))
}

/// [`index_state`] against each of `listings`, the index read at most once.
pub(crate) fn index_states(
    tree: &mut dyn SourceTree,
    listings: &[&[SourceFile]],
) -> anyhow::Result<Vec<IndexState>> {
    let Some(size) = size_of(tree, INDEX) else {
        return Ok(listings.iter().map(|_| IndexState::Missing).collect());
    };
    // Its bytes and records (`Err`: why it doesn't read), once a listing needs them.
    let mut read: Option<(
        Vec<u8>,
        Result<ps5_dump_forge_lz4::index::PathIndex, String>,
    )> = None;
    let mut states = Vec::with_capacity(listings.len());
    let mut first_ok = None;
    for files in listings {
        // Bounded before it is read: bigger than one listing exactly `files` could be is `Stale`, over the read cap `Damaged`.
        if size > index_bound(files) {
            states.push(IndexState::Stale);
            continue;
        }
        if size > MAX_INDEX_CHECK {
            states.push(IndexState::Damaged(format!(
                "{size} bytes, over the {MAX_INDEX_CHECK} bytes read for a check"
            )));
            continue;
        }
        if read.is_none() {
            let old = read_all(tree, INDEX, size)?;
            let index = ps5_dump_forge_lz4::index::read_index(&old).map_err(|e| e.to_string());
            read = Some((old, index));
        }
        let Some((old, index)) = &read else {
            unreachable!("read just above")
        };
        let index = match index {
            Ok(index) => index,
            Err(why) => {
                states.push(IndexState::Damaged(format!("{size} bytes, {why}")));
                continue;
            }
        };
        let want: BTreeSet<(&str, u64)> = files.iter().map(|f| (f.path.as_str(), f.size)).collect();
        let have: BTreeSet<(&str, u64)> = (index.records.iter())
            .map(|(p, s)| (p.as_str(), *s))
            .collect();
        states.push(if have != want {
            IndexState::Stale
        } else if first_ok.is_none() {
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

/// The largest index that could list exactly `files`: four times a fresh one's size plus 1 MiB.
fn index_bound(files: &[SourceFile]) -> u64 {
    let n = files.len() as u64;
    let paths: u64 = files.iter().map(|f| f.path.len() as u64 + 7).sum();
    let fresh = 0x30 + 24 * n + paths + 16 + 16 * (2 * n).next_power_of_two().max(2);
    4 * fresh + (1 << 20)
}

/// `ampr_emu.index` for `files`: with `keep` the source's own when it fits, else a new one; `None` with a finding when unindexable.
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

/// All `size` bytes of `path`, in as many reads as the source takes (a `.pkg`'s serves at most 64 MiB each).
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

/// The most a pack profile may hold.
const MAX_PROFILE: u64 = 1 << 20;

/// The request's pack profile, read and parsed: `None` without one or on failure (every problem a finding).
// ponytail: the profile and its pattern files are read once, not guarded as job paths.
pub(crate) fn pack_profile(
    req: &ConvertRequest,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> Option<ps5_dump_forge_lz4::Profile> {
    let path = req.pack_profile.as_deref()?;
    let shown = path.display();
    let text = match read_profile(path) {
        Ok(t) => t,
        Err(e) => {
            findings.push(format!("pack rules {shown}: {e:#}"));
            return None;
        }
    };
    let mut total = 0u64;
    let mut read =
        |name: &str| read_pattern_file(path, name, &mut total).map_err(|e| format!("{e:#}"));
    match ps5_dump_forge_lz4::parse_profile(&text, &mut read) {
        Ok(p) => {
            if !p.ignored.is_empty() {
                ctx.log(format!(
                    "pack rules: ignored keys: {}",
                    p.ignored.join(", ")
                ));
            }
            Some(p)
        }
        Err(errs) => {
            findings.extend(errs.iter().map(|e| format!("pack rules {shown}: {e}")));
            None
        }
    }
}

/// A rule's `include_from` / `exclude_from` file in the profile's folder (locally not out of it through a link); `total` caps all of one profile's.
fn read_pattern_file(
    profile: &std::path::Path,
    name: &str,
    total: &mut u64,
) -> anyhow::Result<String> {
    let over = || {
        anyhow::anyhow!(
            "pattern files: more than {} MiB in all",
            (16 * MAX_PROFILE) >> 20
        )
    };
    // past the cap nothing more is read
    if *total > 16 * MAX_PROFILE {
        return Err(over());
    }
    let dir = match profile.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => std::path::Path::new("."),
    };
    let path = if crate::net::is_network(dir) {
        // kept `/`-separated: `Path::join` would add a `\` on Windows
        let dir = dir.to_string_lossy();
        std::path::PathBuf::from(format!("{}/{name}", dir.trim_end_matches('/')))
    } else {
        let path = dir.join(name);
        let parent = path.parent().unwrap_or(dir);
        if !parent.canonicalize()?.starts_with(dir.canonicalize()?) {
            anyhow::bail!("leads outside the profile's folder");
        }
        path
    };
    let text = read_profile(&path)?;
    *total += text.len() as u64;
    if *total > 16 * MAX_PROFILE {
        return Err(over());
    }
    Ok(text)
}

/// A profile's text: a regular file (not a link) of at most [`MAX_PROFILE`] bytes, UTF-8.
fn read_profile(path: &std::path::Path) -> anyhow::Result<String> {
    let too_big = || anyhow::anyhow!("more than {} KiB", MAX_PROFILE >> 10);
    let bytes = if crate::net::is_network(path) {
        let (dir, name) = match (path.parent(), path.file_name().and_then(|n| n.to_str())) {
            (Some(dir), Some(name)) => (dir, name),
            _ => anyhow::bail!("not a file"),
        };
        crate::net::read_small(dir, name, MAX_PROFILE)?
            .ok_or_else(|| anyhow::anyhow!("not a regular file of at most 1 MiB (or missing)"))?
    } else {
        let meta = path.symlink_metadata()?;
        if meta.file_type().is_symlink() {
            anyhow::bail!("a link: give the file it points to");
        }
        if !meta.is_file() {
            anyhow::bail!("not a regular file");
        }
        if meta.len() > MAX_PROFILE {
            return Err(too_big());
        }
        // A link or FIFO swapped in after the lstat: opened without following or blocking, then judged by the handle.
        let file = crate::scan::open_nofollow(path)?;
        let opened = file.metadata()?;
        if !opened.is_file() {
            anyhow::bail!("not a regular file");
        }
        if opened.len() > MAX_PROFILE {
            return Err(too_big());
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_PROFILE + 1), &mut bytes)?;
        if bytes.len() as u64 > MAX_PROFILE {
            return Err(too_big());
        }
        bytes
    };
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("not UTF-8 text"))
}

/// The built-in packing strategy's name, as the logs give it.
pub(crate) const EVERYTHING: &str = "pack everything except system files";

/// The pack list in `records` order: a profile's rules (then its auto-loose sampling), else the built-in strategy.
fn plan(
    tree: &mut dyn SourceTree,
    records: &[(String, u64)],
    req: &ConvertRequest,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> anyhow::Result<PackPlan> {
    let profile = pack_profile(req, ctx, findings);
    if req.pack_profile.is_some() && profile.is_none() {
        // A profile that doesn't parse refuses the job: every file loose, nothing sampled.
        let files = (records.iter())
            .map(|(path, size)| PackFile {
                path: path.clone(),
                size: *size,
                spec: None,
            })
            .collect();
        return Ok(PackPlan {
            files,
            level: req.lz4_level,
        });
    }
    let mut files: Vec<PackFile> = records
        .iter()
        .map(|(path, size)| PackFile {
            path: path.clone(),
            size: *size,
            spec: ps5_dump_forge_lz4::select_everything(path, *size, profile.as_ref()),
        })
        .collect();
    let ruled: Vec<bool> = files.iter().map(|f| f.spec.is_some()).collect();
    if let Some(p) = &profile {
        auto_loose(tree, &mut files, req.lz4_level, &p.auto_loose, ctx)?;
    }
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
    match (&req.pack_profile, &profile) {
        (Some(path), Some(p)) => ctx.log(format!(
            "pack rules: {}: {} rules; {packed} files packed, {} loose; LZ4 level {}",
            path.display(),
            p.rules().len(),
            files.len() - packed,
            req.lz4_level
        )),
        _ => ctx.log(format!(
            "packing strategy: {EVERYTHING}; {packed} files to pack, {} loose; level {}",
            files.len() - packed,
            req.lz4_level
        )),
    }
    let loose: Vec<(&str, u64, bool)> = (files.iter().zip(&ruled))
        .filter(|(f, _)| f.spec.is_none())
        .map(|(f, r)| (f.path.as_str(), f.size, *r))
        .collect();
    ctx.log(match &profile {
        None => everything_loose_line(&loose, None),
        Some(_) => loose_line(&loose, true),
    });
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

/// Decimal GB with one decimal, MB under 1 GB.
fn gb(bytes: u64) -> String {
    match bytes as f64 {
        b if b >= 1e9 => format!("{:.1} GB", b / 1e9),
        b => format!("{:.1} MB", b / 1e6),
    }
}

/// A profile's "kept loose" line: `(path, size, auto)`, `auto` when auto-loose kept a file the rules would pack.
pub(crate) fn loose_line(loose: &[(&str, u64, bool)], auto_loose: bool) -> String {
    let (mut auto, mut sys, mut sys_n, mut prof, mut prof_n, mut empty) =
        (0u64, 0u64, 0, 0u64, 0, 0);
    for &(path, size, a) in loose {
        if a {
            auto += size;
        } else if ps5_dump_forge_lz4::always_loose(path) {
            (sys, sys_n) = (sys + size, sys_n + 1);
        } else if size == 0 {
            empty += 1;
        } else {
            (prof, prof_n) = (prof + size, prof_n + 1);
        }
    }
    let total: u64 = loose.iter().map(|l| l.1).sum();
    let mut s = format!(
        "packing strategy: kept loose {} files, {}: the profile {prof_n} files, {}, system and \
         runtime files (always_loose) {sys_n} files, {}",
        loose.len(),
        gb(total),
        gb(prof),
        gb(sys)
    );
    if auto_loose {
        s.push_str(&format!(", auto-loose {}", gb(auto)));
    }
    if empty > 0 {
        s.push_str(&format!(", {empty} empty files"));
    }
    s
}

/// The "kept loose" line of the built-in strategy: only `always_loose` and empty files stay loose.
pub(crate) fn everything_loose_line(
    loose: &[(&str, u64, bool)],
    profile: Option<&ps5_dump_forge_lz4::Profile>,
) -> String {
    if profile.is_some() {
        return loose_line(loose, false);
    }
    let bytes: u64 = loose.iter().map(|l| l.1).sum();
    let empty = loose.iter().filter(|l| l.1 == 0).count();
    format!(
        "packing strategy: kept loose {} files, {}: system and runtime files (always_loose) {}, \
         {empty} empty files",
        loose.len(),
        gb(bytes),
        loose.len() - empty
    )
}

/// Upstream's auto-loose: each large file is sampled as the writer encodes and kept loose when it barely shrinks.
fn auto_loose(
    tree: &mut dyn SourceTree,
    files: &mut [PackFile],
    level: u8,
    rules: &ps5_dump_forge_lz4::AutoLoose,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let picks: Vec<(usize, Vec<u64>)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let spec = f.spec.filter(|s| rules.applies(f.size, s))?;
            Some((i, rules.sample_indices(f.size, spec.block_shift)))
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
        if rules.keeps_loose(&s) {
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

/// Whether `path` is a root name the `Lz4` target writes, case-insensitively.
fn is_pack_name(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [MANIFEST, CRC_SIDECAR, PROFILE].contains(&p.as_str())
        || (p.starts_with("ampr_assets-") && p.ends_with(".pak") && !p.contains('/'))
}

/// The prepared tree: the inner tree's files less the diagnostics, plus the generated ones from memory.
pub(crate) struct Overlay {
    pub(crate) inner: Box<dyn SourceTree>,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) empty_dirs: Vec<String>,
    pub(crate) generated: HashMap<String, Cow<'static, [u8]>>,
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

/// The pack files at the `.part` folder's root (exclusive, no link followed), synced as each is finished; on FreeBSD through the sync cadence (U5).
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
        Ok(crate::extract::sync_file(&w)?)
    }
}

/// A reopened volume starts a fresh `SyncEvery` at 0: the packer only rewrites its 64-byte header.
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

/// The biggest single file the `Lz4` target writes, for FAT32's 4 GiB limit (packed files are spread over volumes).
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
    // Manifest: header, records, then every `/app0/...` path NUL-ended and up to 1024 volume names.
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

    #[cfg(unix)]
    #[test]
    fn a_profile_pattern_file_through_a_link_out_of_its_folder_is_refused() {
        let root = std::env::temp_dir().join(format!("forge-profile-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("rules");
        std::fs::create_dir_all(dir.join("lists")).unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::fs::write(dir.join("lists/pack.txt"), "data/*\n").unwrap();
        std::fs::write(root.join("outside/x.txt"), "*\n").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), dir.join("out")).unwrap();
        let profile = dir.join("p.toml");
        let mut total = 0;
        let read = read_pattern_file(&profile, "lists/pack.txt", &mut total).unwrap();
        assert_eq!(read, "data/*\n");
        let e = read_pattern_file(&profile, "out/x.txt", &mut total).unwrap_err();
        assert!(
            format!("{e:#}").contains("outside the profile's folder"),
            "{e:#}"
        );
        std::fs::remove_dir_all(&root).unwrap();
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
        // Unreadable: the log line carries the parser's reason.
        match index_state(&mut tree, &files).unwrap() {
            IndexState::Damaged(why) => assert!(why.starts_with("4096 bytes, "), "{why}"),
            _ => panic!("expected Damaged"),
        }
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
            IndexState::Damaged(_)
        ));
    }
}
