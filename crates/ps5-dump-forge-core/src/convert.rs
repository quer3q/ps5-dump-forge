//! One conversion job: open the source, preflight, write a `.part`, verify it, publish it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5_dump_forge_fpkg::FpkgSource;
use ps5_dump_forge_pfs::{FfpfscInfo, PfsHeader, PfsSource, WrapOptions, WrapReport};
use ps5upload_fpkg::exfat::{ExFat, ExFatSource};
use ps5upload_fpkg::source::SourceTree;
use ps5upload_fpkg::ufs2_source::Ufs2Source;
use ps5upload_fpkg::{PkgFile, ReadSeek};

use crate::finalize::{Part, part_path};
use crate::jobs::Ctx;
use crate::prefetch::Prefetch;
use crate::preflight::{self, GameInfo};
use crate::scan::{JunkFiltered, ScannedFolder};
use crate::verify::{self, HashingTree, Mode};
use crate::{ConvertRequest, Format, JobReport, Lz4Mode};

/// Every SMP image size and cluster is a multiple of this.
const IMAGE_ALIGN: u64 = 64 * 1024;
/// The maker's mark, `PS5-FORGE-v<version>`: an `.exfat` holds it in an OEM Parameters
/// record, an `.ffpkg` as its `fs_volname` (31 bytes at most), a `.ffpfsc` in its inner image.
pub(crate) const MAKER_PREFIX: &str = "PS5-FORGE-v";
const _: () = assert!(
    MAKER_PREFIX.len() + env!("CARGO_PKG_VERSION").len() <= 31,
    "the maker's mark must fit UFS2 fs_volname (31 bytes)"
);

fn maker() -> Option<String> {
    Some(format!("{MAKER_PREFIX}{}", env!("CARGO_PKG_VERSION")))
}

/// What a source path is, by what it is on disk and then by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Folder,
    Exfat,
    Ffpkg,
    Ffpfs,
    Ffpfsc,
    Pkg,
}

impl Kind {
    pub(crate) fn of(path: &Path) -> anyhow::Result<Self> {
        if path.is_dir() {
            return Ok(Self::Folder);
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        Ok(match ext.as_str() {
            "exfat" => Self::Exfat,
            "ffpkg" | "ufs2" => Self::Ffpkg,
            "ffpfs" => Self::Ffpfs,
            "ffpfsc" => Self::Ffpfsc,
            "pkg" => Self::Pkg,
            _ => bail!(
                "{} is neither a folder nor a supported image (.exfat, .ffpkg, .ffpfs, .ffpfsc, .pkg)",
                path.display()
            ),
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Exfat => "exfat",
            Self::Ffpkg => "ffpkg",
            Self::Ffpfs => "ffpfs",
            Self::Ffpfsc => "ffpfsc",
            Self::Pkg => "pkg",
        }
    }
}

/// The source as a tree. Image readers are wrapped in [`JunkFiltered`], so every source
/// drops the same junk the folder scanner does.
pub(crate) fn open_source(
    path: &Path,
    kind: Kind,
    cancel: &AtomicBool,
) -> anyhow::Result<Box<dyn SourceTree>> {
    let with_path = |e: ps5upload_fpkg::Error| anyhow::anyhow!("{}: {e}", path.display());
    let image: Box<dyn SourceTree> = match kind {
        Kind::Folder => return Ok(Box::new(ScannedFolder::scan(path, cancel)?)),
        Kind::Exfat => Box::new(ExFatSource::open(path).map_err(with_path)?),
        Kind::Ffpkg => Box::new(Ufs2Source::open(path).map_err(with_path)?),
        Kind::Ffpfs => {
            let file = File::open(path).with_context(|| format!("{}", path.display()))?;
            Box::new(
                PfsSource::from_reader(Box::new(file), path.display().to_string())
                    .map_err(plain)?,
            )
        }
        Kind::Ffpfsc => open_ffpfsc(path)?.0,
        // The debug passcode: packages this app and ps5upload build use it.
        Kind::Pkg => Box::new(FpkgSource::open(path, None).map_err(with_path)?),
    };
    Ok(Box::new(JunkFiltered::new(image)))
}

/// The image inside a `.ffpfsc` (not junk-filtered) and the container's facts.
pub(crate) fn open_ffpfsc(path: &Path) -> anyhow::Result<(Box<dyn SourceTree>, FfpfscInfo)> {
    let file = File::open(path).with_context(|| format!("{}", path.display()))?;
    ps5_dump_forge_pfs::open_ffpfsc(Box::new(file), &path.display().to_string()).map_err(plain)
}

/// The PFS readers put the path in their own messages.
fn plain(e: ps5upload_fpkg::Error) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// The vendored error stays in the chain, so U7 finds its errno.
fn reopen(e: ps5upload_fpkg::Error) -> anyhow::Error {
    anyhow::Error::new(e).context("reading the output back")
}

/// What an image source file looked like when the job opened it. Checked again before
/// publishing, so an image rewritten mid-conversion fails the job; a folder source checks
/// each file on every read instead.
// ponytail: on FAT (FreeBSD's msdosfs) a file's id comes from its directory slot and mtimes
// have 2-second steps, so a same-size file renamed over the source within 2 s goes unnoticed.
// Upgrade: also hash a sample of the contents.
pub(crate) struct SourceStamp {
    path: PathBuf,
    seen: Option<Seen>,
}

/// (length, modification time, identity).
type Seen = (u64, Option<SystemTime>, crate::finalize::FileId);

impl SourceStamp {
    pub(crate) fn take(path: &Path, kind: Kind) -> anyhow::Result<Self> {
        let seen = match kind {
            Kind::Folder => None,
            _ => Some(Self::seen(path)?),
        };
        Ok(Self {
            path: path.to_path_buf(),
            seen,
        })
    }

    /// One file's stamp, whatever it holds (an LZ4 trace file in a folder).
    pub(crate) fn of_file(path: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            seen: Some(Self::seen(path)?),
        })
    }

    /// The stamp of the file `file` (opened from `path`) is, from the handle, so it describes
    /// exactly what is read; [`SourceStamp::unchanged`] then also says that `path` still names
    /// that file (identity, length, modification time).
    pub(crate) fn of_handle(path: &Path, file: &File) -> anyhow::Result<Self> {
        let shown = || format!("{}", path.display());
        let meta = file.metadata().with_context(shown)?;
        #[cfg(unix)]
        let id = crate::finalize::meta_id(&meta);
        #[cfg(windows)]
        let id = crate::win::raw_file_id(file).with_context(shown)?;
        #[cfg(not(any(unix, windows)))]
        let id = crate::finalize::handle_id(file).with_context(shown)?;
        Ok(Self {
            path: path.to_path_buf(),
            seen: Some((meta.len(), meta.modified().ok(), id)),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The file still has the length, modification time and identity it had when stamped
    /// (always true for a folder's stamp, which holds none).
    pub(crate) fn unchanged(&self) -> bool {
        self.seen
            .as_ref()
            .is_none_or(|seen| Self::seen(&self.path).ok().as_ref() == Some(seen))
    }

    fn seen(path: &Path) -> anyhow::Result<Seen> {
        let shown = || format!("{}", path.display());
        #[cfg(unix)]
        let (meta, id) = {
            let meta = std::fs::metadata(path).with_context(shown)?;
            let id = crate::finalize::meta_id(&meta);
            (meta, id)
        };
        // Identities come from a handle there; one handle answers both.
        #[cfg(not(unix))]
        let (meta, id) = {
            let file = File::open(path).with_context(shown)?;
            let meta = file.metadata().with_context(shown)?;
            // ponytail: a source on a volume that reports file id 0 (some NAS/SMB) is still read;
            // its stamp then catches a swap by length and mtime only. Ceiling: a same-size,
            // same-mtime replacement there goes unnoticed.
            #[cfg(windows)]
            let id = crate::win::raw_file_id(&file);
            #[cfg(not(windows))]
            let id = crate::finalize::handle_id(&file);
            (meta, id.with_context(shown)?)
        };
        Ok((meta.len(), meta.modified().ok(), id))
    }

    pub(crate) fn check(&self) -> anyhow::Result<()> {
        if !self.unchanged() {
            bail!(
                "{} changed while it was being converted; not publishing the output",
                self.path.display()
            );
        }
        Ok(())
    }
}

/// An image writer's layout, fixed in preflight.
enum Image {
    Exfat(ps5_dump_forge_exfat::Layout),
    Ffpkg(ps5_dump_forge_ufs2::Layout),
    Ffpfs(ps5_dump_forge_pfs::Layout),
}

impl Image {
    /// Lays `format`'s image out for `tree`, with `free_bytes` of free space beyond its own
    /// (an LZ4 trace's room); a refusal is a preflight finding.
    fn plan(
        format: Format,
        tree: &dyn SourceTree,
        info: &GameInfo,
        free_bytes: u64,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        match format {
            Format::Exfat => {
                // The volume label shows in Finder/Explorer; title ids are ASCII, 11 fit.
                let label: String = info
                    .title_id
                    .iter()
                    .flat_map(|id| id.chars())
                    .take(11)
                    .collect();
                // `.into()` accepts both `label: String` and the coming `label: Option<String>`.
                #[allow(clippy::useless_conversion)]
                let opts = ps5_dump_forge_exfat::Options {
                    free_bytes,
                    label: label.into(),
                    maker: maker(),
                };
                ps5_dump_forge_exfat::plan(tree, &opts, cancel)
                    .map(Self::Exfat)
                    .map_err(|e| format!("exFAT layout: {e}"))
            }
            Format::Ffpkg => {
                let opts = ps5_dump_forge_ufs2::Options {
                    free_bytes,
                    maker: maker(),
                };
                ps5_dump_forge_ufs2::plan(tree, &opts, cancel)
                    .map(Self::Ffpkg)
                    .map_err(|e| format!("UFS2 layout: {e}"))
            }
            Format::Ffpfs => ps5_dump_forge_pfs::plan(tree, &Default::default(), cancel)
                .map(Self::Ffpfs)
                .map_err(|e| format!("PFS layout: {e}")),
            other => unreachable!("{other:?} is not an image filesystem"),
        }
    }

    fn size(&self) -> u64 {
        match self {
            Self::Exfat(l) => l.image_size,
            Self::Ffpkg(l) => l.image_size,
            Self::Ffpfs(l) => l.image_size,
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::Exfat(_) => "exfat",
            Self::Ffpkg(_) => "ffpkg",
            Self::Ffpfs(_) => "ffpfs",
        }
    }

    /// Writes the image into `out` (a `.part`, or a `.ffpfsc`'s stream); returns its size
    /// and a log line.
    fn write<W: Write + Seek>(
        &self,
        tree: &mut dyn SourceTree,
        out: &mut W,
        ctx: &Ctx,
    ) -> ps5upload_fpkg::Result<(u64, String)> {
        let progress = &mut |d, t| ctx.progress("write", d, t);
        let (what, size, files, dirs) = match self {
            Self::Exfat(l) => {
                let r = ps5_dump_forge_exfat::write(tree, l, out, ctx.cancel, progress)?;
                ("exFAT", r.image_size, r.files, r.dirs)
            }
            Self::Ffpkg(l) => {
                let r = ps5_dump_forge_ufs2::write(tree, l, out, ctx.cancel, progress)?;
                ("UFS2", r.image_size, r.files, r.dirs)
            }
            Self::Ffpfs(l) => {
                let r = ps5_dump_forge_pfs::write(tree, l, out, ctx.cancel, progress)?;
                ("PFS", r.image_size, r.files, r.dirs)
            }
        };
        Ok((
            size,
            format!("{what} image: {size} bytes, {files} files, {dirs} dirs"),
        ))
    }
}

/// The writer's layout, fixed in preflight.
enum Plan {
    Folder,
    /// A folder of loose files and LZ4 packs.
    Lz4(crate::lz4::PackPlan),
    Image(Image),
    /// The image inside, and its name in the container.
    Ffpfsc(Image, String),
}

pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    // U7: a failure on a drive that went away says so.
    #[cfg(target_os = "freebsd")]
    ctx.watch(&req.source);
    #[cfg(target_os = "freebsd")]
    return convert(req, ctx).map_err(|e| {
        crate::durable::explain_removal(e, &ctx.mounts.borrow(), crate::dest::mount_state)
    });
    #[cfg(not(target_os = "freebsd"))]
    convert(req, ctx)
}

/// `--remove-backport`: `source` without fakelib's backport libraries, when they can go (else a
/// finding, or a log line when there are none).
fn leave_out_backport(
    req: &ConvertRequest,
    mut source: Box<dyn SourceTree>,
    info: &GameInfo,
    packs: &crate::lz4::Packs,
    ctx: &Ctx,
    findings: &mut Vec<String>,
) -> Box<dyn SourceTree> {
    if req.remove_backport {
        let fakelib = crate::backport::classify(source.as_mut());
        let kept: Vec<&str> = fakelib.emulators.iter().map(|e| e.name.as_str()).collect();
        if fakelib.libs.is_empty() {
            ctx.log("remove backport: nothing to remove, no backport library in fakelib");
        } else if packs.packed && !packs.unpacked {
            findings.push(
                "a packed dump lists its backport in its LZ4 manifest: unpack it (Unpack LZ4) to \
                 leave the backport out"
                    .into(),
            );
        } else if let Some(why) =
            crate::backport::blocked(source.as_mut(), info.param_json.as_ref())
        {
            findings.push(why);
        } else {
            for lib in &fakelib.libs {
                ctx.log(format!("remove backport: leaving out {lib}"));
            }
            source = Box::new(crate::backport::Without::new(source, &fakelib.libs));
        }
        if !kept.is_empty() {
            ctx.log(format!(
                "remove backport: keeping emulators {}",
                kept.join(", ")
            ));
        }
    }
    source
}

/// The pack list a Pack job of `req` would resolve, up to its LZ4 preparation, writing
/// nothing: the source opened, packs decoded, the backport left out on request, then the rules,
/// the keep-loose list and auto-loose. LZ4 findings are the error.
pub(crate) fn lz4_plan(
    req: &ConvertRequest,
    ctx: &Ctx,
) -> anyhow::Result<(crate::lz4::PackPlan, GameInfo)> {
    anyhow::ensure!(
        crate::lz4::packing(req),
        "a pack plan needs an LZ4 Pack request"
    );
    let kind = Kind::of(&req.source)?;
    let source = open_source(&req.source, kind, ctx.cancel)?;
    ctx.log(format!("source: {}", source.describe()));
    let (mut source, packs) = crate::lz4::open(req, source, ctx)?;
    let (info, _) = preflight::input(source.as_mut());
    let mut findings = Vec::new();
    let source = leave_out_backport(req, source, &info, &packs, ctx, &mut findings);
    let mtime = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let prepared = crate::lz4::prepare(req, source, packs, mtime, ctx)?;
    findings.extend(prepared.findings);
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    let pack = prepared.pack.context("no pack plan")?;
    Ok((pack, info))
}

/// U5 on FreeBSD: `write` puts every byte of the image through `SyncEvery`, sharing the
/// job's `cadence`; its finish is the image's sync.
#[cfg(target_os = "freebsd")]
fn synced<T>(
    file: &mut File,
    cadence: &mut crate::durable::Cadence,
    ctx: &Ctx,
    write: impl FnOnce(&mut crate::durable::SyncEvery<'_, &mut File>) -> ps5upload_fpkg::Result<T>,
) -> anyhow::Result<T> {
    let mut out = crate::durable::SyncEvery::new(file, cadence, ctx.cancel);
    let value = write(&mut out)?;
    out.finish().context("syncing the image")?;
    log_cadence(cadence, ctx);
    Ok(value)
}

/// Logs the last change of the sync cadence's N, if any (U5).
#[cfg(target_os = "freebsd")]
pub(crate) fn log_cadence(cadence: &mut crate::durable::Cadence, ctx: &Ctx) {
    if let Some((old, new)) = cadence.take_change() {
        let mib = |n: u64| n.div_ceil(1 << 20);
        ctx.log(format!(
            "sync cadence: every {} MiB (was {} MiB)",
            mib(new),
            mib(old)
        ));
    }
}

fn convert(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    ctx.progress("scan", 0, 1);
    let kind = Kind::of(&req.source)?;
    // Before the open: a change after this point, however early, fails the publish.
    let stamp = SourceStamp::take(&req.source, kind)?;
    let source = open_source(&req.source, kind, ctx.cancel)?;
    ctx.log(format!("source: {}", source.describe()));
    // The job's one timestamp, in whole seconds, for what it generates (an LZ4 manifest and
    // path index).
    let mtime = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    // LZ4 packs decoded first, when the job asks: the backport's files are manifest records.
    let (mut source, packs) = crate::lz4::open(req, source, ctx)?;
    ctx.progress("scan", 1, 1);
    ctx.check()?;

    ctx.progress("preflight", 0, 1);
    // In place the output is the source itself; its `.part` goes beside it.
    let in_place = req.lz4_in_place;
    let out = preflight::output_path(if in_place { &req.source } else { &req.output })?;
    let (info, mut findings) = preflight::input(source.as_mut());
    if let Some(id) = &info.title_id {
        ctx.log(format!("title id: {id}"));
    }
    // Before anything sizes or writes the source, so every target (`.pkg` included) and the
    // verification see the tree without it.
    let source = leave_out_backport(req, source, &info, &packs, ctx, &mut findings);
    findings.extend(crate::lz4::in_place_findings(req, kind, packs.packed));
    // The LZ4 options, before anything sizes or writes the tree (`.pkg` included).
    let prepared = crate::lz4::prepare(req, source, packs, mtime, ctx)?;
    findings.extend(prepared.findings);
    let mut source = prepared.tree;
    let mut pack = prepared.pack;
    if in_place {
        ctx.log(format!(
            "in place: writing {} copy beside {} (about the image's size of free space); it \
             replaces the source only once it verifies",
            if req.lz4 == Some(Lz4Mode::Unpatch) {
                "an unpatched"
            } else {
                "a patched"
            },
            out.display()
        ));
    } else {
        findings.extend(preflight::output(&req.source, &out, req.format));
    }
    if req.format == Format::Pkg {
        return crate::package::run(req, ctx, source.as_mut(), &info, findings, &out, &stamp);
    }
    if !matches!(req.format, Format::Folder | Format::Lz4) {
        findings.extend(preflight::too_deep(source.as_ref()));
    }
    let mut need = preflight::estimate(source.as_ref());
    let mut largest = need;
    let trace_space = match req.lz4 {
        Some(Lz4Mode::Trace) => u64::from(req.lz4_trace_space_mib) << 20,
        _ => 0,
    };
    if trace_space > 0 && matches!(req.format, Format::Exfat | Format::Ffpkg) {
        ctx.log(format!(
            "LZ4 trace: {} MiB of free space in the image for the game's trace",
            req.lz4_trace_space_mib
        ));
    }
    // LZ4 packs straight into an image: the measure pass gives every pack file's exact size
    // before anything lays the image out. Skipped when preflight has already failed.
    let image_target = matches!(
        req.format,
        Format::Exfat | Format::Ffpkg | Format::Ffpfs | Format::Ffpfsc
    );
    let measured = match &pack {
        Some(p) if image_target && findings.is_empty() => {
            Some(measure_packs(source.as_mut(), p, req, mtime, ctx)?)
        }
        _ => None,
    };
    let packed = measured.as_ref().zip(pack.as_ref());
    let mut planned = |format: Format, findings: &mut Vec<String>| {
        plan_image(
            format,
            source.as_mut(),
            packed,
            &info,
            trace_space,
            ctx.cancel,
        )
        .map_err(|e| findings.push(e))
        .ok()
    };
    let plan = match req.format {
        Format::Folder | Format::Lz4 => {
            // On FreeBSD the names are checked once the probe has seen the destination.
            #[cfg(not(target_os = "freebsd"))]
            {
                let paths: Vec<String> = source.files().iter().map(|f| f.path.clone()).collect();
                findings.extend(crate::extract::extraction_findings(
                    &paths,
                    source.empty_dirs(),
                    crate::extract::CASE_INSENSITIVE,
                ));
            }
            largest = source.files().iter().map(|f| f.size).max().unwrap_or(0);
            match pack.take() {
                // Every byte the packer may write, loose files and the path index included
                // (all chunks stored raw), with the estimate's margin; no volume passes the cap.
                Some(pack) => {
                    let bound = ps5_dump_forge_lz4::writer::worst_case_bytes(
                        &pack.files,
                        pack.runtime.is_some(),
                    );
                    need = bound
                        .saturating_add(bound / 50)
                        .saturating_add(64 * 1024 * 1024);
                    let index = source
                        .files()
                        .iter()
                        .find(|f| f.path == ps5_dump_forge_lz4::INDEX)
                        .map_or(0, |f| f.size);
                    largest = crate::lz4::largest_file(&pack.files, pack.runtime.is_some(), index);
                    Some(Plan::Lz4(pack))
                }
                // `prepare` said why.
                None if crate::lz4::packing(req) => None,
                None => Some(Plan::Folder),
            }
        }
        Format::Exfat | Format::Ffpkg | Format::Ffpfs => {
            if req.format == Format::Exfat
                && kind == Kind::Exfat
                && let Some(note) = crate::inspect::already_smp(&req.source)
            {
                ctx.log(note);
            }
            planned(req.format, &mut findings).map(Plan::Image)
        }
        Format::Ffpfsc if req.ffpfsc_level > 9 => {
            findings.push(format!(
                "the .ffpfsc zlib level is 0 through 9, not {}",
                req.ffpfsc_level
            ));
            None
        }
        Format::Ffpfsc => match req.inner.unwrap_or(Format::Exfat) {
            inner @ (Format::Exfat | Format::Ffpkg | Format::Ffpfs) => {
                planned(inner, &mut findings).map(|image| {
                    // SMP picks the inner image's driver by its extension.
                    let id = info.title_id.as_deref().unwrap_or("image");
                    let name = format!("{id}.{}", image.extension());
                    Plan::Ffpfsc(image, name)
                })
            }
            other => {
                findings.push(format!(
                    "a .ffpfsc holds an .exfat, .ffpkg or .ffpfs image, not {}",
                    preflight::extension(other).map_or("a folder".into(), |e| format!(".{e}"))
                ));
                None
            }
        },
        Format::Pkg => unreachable!("built by `package::run` above"),
    };
    ctx.check()?;
    // An image is the only thing written and its plan knows its exact size (the estimate
    // would refuse a destination that holds a tight image); 64 MiB more covers what the
    // destination's own file system spends on the new file. A `.ffpfsc` is sized as if no
    // block compressed.
    let output_size = match &plan {
        Some(Plan::Image(image)) => Some(image.size()),
        Some(Plan::Ffpfsc(image, _)) => {
            match ps5_dump_forge_pfs::container_size_max(image.size()) {
                Ok(size) => Some(size),
                Err(e) => {
                    findings.push(format!("the .ffpfsc container: {e}"));
                    None
                }
            }
        }
        _ => None,
    };
    if let Some(size) = output_size {
        need = size.saturating_add(64 * 1024 * 1024);
        largest = size;
    }
    let dir = out.parent().expect("output_path gives a parent");
    // U7: so a probe that fails because the drive went away says so.
    #[cfg(target_os = "freebsd")]
    ctx.watch(dir);
    let checked = preflight::destination(dir, need, largest, ctx.cancel);
    findings.extend(checked.findings);
    for note in checked.notes {
        ctx.log(note);
    }
    // U2: folder names as the destination folds them; with no probe, as if it folds.
    #[cfg(target_os = "freebsd")]
    if let Some(Plan::Folder | Plan::Lz4(_)) = &plan {
        let paths: Vec<String> = source.files().iter().map(|f| f.path.clone()).collect();
        let (folds, ascii_only) = checked
            .dest
            .as_ref()
            .map_or((true, false), |d| (d.folds_names, d.ascii_only));
        findings.extend(preflight::folder_names(
            &paths,
            source.empty_dirs(),
            folds,
            ascii_only,
        ));
    }
    let Some(plan) = plan.filter(|_| findings.is_empty()) else {
        bail!("preflight failed:\n  {}", findings.join("\n  "));
    };
    #[cfg(target_os = "freebsd")]
    let dest = checked.dest.context("the destination was not probed")?;
    ctx.progress("preflight", 1, 1);
    // The UFS2 indirect-block seam is sampled in a `.ffpkg`, alone or inside a `.ffpfsc`.
    let mode = if req.full_verify {
        Mode::Full
    } else {
        Mode::Fast {
            seed: verify::fresh_seed()?,
            seam: matches!(
                plan,
                Plan::Image(Image::Ffpkg(_)) | Plan::Ffpfsc(Image::Ffpkg(_), _)
            ),
        }
    };
    let summary = verify::summary(source.files(), mode);
    // Packed into an image, the image holds the pack files: they are written and compared as
    // written, then the logical files are compared through the packs.
    let physical_files = match (&measured, &pack) {
        (Some(m), Some(p)) => Some(
            ps5_dump_forge_lz4::tree::PackedTree::new(source.as_mut(), m, &p.files, 1, ctx.cancel)?
                .files()
                .to_vec(),
        ),
        _ => None,
    };
    let physical_check = physical_files
        .as_deref()
        .map_or(0, |f| verify::summary(f, mode).checked_bytes);
    // Write reads every source byte once, verify reads back what `mode` samples.
    let file_bytes = physical_files
        .as_deref()
        .unwrap_or(source.files())
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    ctx.expect_rest(
        file_bytes
            .saturating_add(summary.checked_bytes)
            .saturating_add(physical_check),
    );

    let part_path = part_path(&out, ctx.job);
    // Reads run one range ahead on their own thread; hashing stays here, and `HashingTree`
    // sees exactly the calls and bytes it would without read-ahead.
    let mut ahead = Prefetch::new(source).context("starting the read-ahead thread")?;
    let mut hashing = HashingTree::new(&mut ahead);
    #[cfg(target_os = "freebsd")]
    let mut cadence = crate::durable::Cadence::new();
    // Packed into an image: what the image must hold, as written.
    let mut physical: Option<verify::Expected> = None;
    // The image `.part` stays open from creation to verification, so what is verified is
    // the file this job wrote, whatever happens to its name meanwhile.
    let (part, image, bytes, wrapped) = match &plan {
        Plan::Folder => {
            #[cfg_attr(not(target_os = "freebsd"), allow(unused_mut))]
            let mut part = Part::create_dir(&part_path)?;
            #[cfg(target_os = "freebsd")]
            part.set_dest(&dest);
            let bytes = crate::extract::write(&mut hashing, &part, ctx)?;
            (part, None, bytes, None)
        }
        Plan::Lz4(pack) => {
            #[cfg_attr(not(target_os = "freebsd"), allow(unused_mut))]
            let mut part = Part::create_dir(&part_path)?;
            #[cfg(target_os = "freebsd")]
            part.set_dest(&dest);
            // The loose files through a view of the one hashing tree, then the packed ones:
            // each file is read once, front to back, and keeps its digest.
            let packed: std::collections::HashSet<&str> = pack
                .files
                .iter()
                .filter(|f| f.spec.is_some())
                .map(|f| f.path.as_str())
                .collect();
            let mut loose = crate::lz4::Only::new(&mut hashing, |p| !packed.contains(p));
            let loose = crate::extract::write(&mut loose, &part, ctx)?;
            let stored = write_packs(
                &mut hashing,
                pack,
                &part,
                req,
                mtime,
                ctx,
                #[cfg(target_os = "freebsd")]
                &mut cadence,
            )?;
            (part, None, loose.saturating_add(stored), None)
        }
        Plan::Image(_) | Plan::Ffpfsc(..) => {
            let (part, file, size, wrapped) = match (&measured, &pack) {
                // The image writer reads the packs as a tree made from the source; the files it
                // sees are hashed as written, the source's as the packs read them.
                (Some(m), Some(p)) => {
                    let threads = compression_threads(req);
                    let mut packs = ps5_dump_forge_lz4::tree::PackedTree::new(
                        &mut hashing,
                        m,
                        &p.files,
                        threads,
                        ctx.cancel,
                    )?;
                    let mut written = HashingTree::new(&mut packs);
                    let (part, file, size, wrapped) = write_image(
                        &plan,
                        &mut written,
                        &part_path,
                        req,
                        ctx,
                        #[cfg(target_os = "freebsd")]
                        &dest,
                        #[cfg(target_os = "freebsd")]
                        &mut cadence,
                    )?;
                    ctx.progress("verify", 0, 1);
                    // The source's files and their read-back through the packs come after.
                    ctx.reserve(summary.checked_bytes);
                    physical = Some(written.expected(ctx, mode)?);
                    (part, file, size, wrapped)
                }
                _ => write_image(
                    &plan,
                    &mut hashing,
                    &part_path,
                    req,
                    ctx,
                    #[cfg(target_os = "freebsd")]
                    &dest,
                    #[cfg(target_os = "freebsd")]
                    &mut cadence,
                )?,
            };
            (part, Some(file), size, wrapped)
        }
    };
    // On FreeBSD `synced` has synced it.
    #[cfg(not(target_os = "freebsd"))]
    if let Some(file) = &image {
        file.sync_all().context("syncing the image")?;
    }
    ctx.check()?;

    ctx.progress("verify", 0, 1);
    // Packed, the image's read-back as written comes first.
    ctx.reserve(physical_check);
    let expected = hashing.expected(ctx, mode)?;
    // The source is read no more: its handles close before the output may replace it
    // (Windows refuses to rename over an open file).
    drop(ahead);
    // Nor are the packs regenerated: their metadata goes before the reader builds its own.
    drop(measured);
    ctx.check()?;
    let runtime = pack.as_ref().and_then(|p| p.runtime);
    let label = part.path().display().to_string();
    let mut checks = match (&plan, image) {
        (Plan::Lz4(pack), _) => {
            let tree = ScannedFolder::scan(part.path(), ctx.cancel)?;
            // As for a folder, below.
            if tree.root() != part.path() {
                bail!("{} was replaced while verifying", part.path().display());
            }
            pack_checks(&expected, Box::new(tree), pack.runtime, ctx, mode)?
        }
        (Plan::Folder, _) => {
            let mut tree = ScannedFolder::scan(part.path(), ctx.cancel)?;
            // `scan` resolves its root; a `.part` swapped for a link resolves elsewhere.
            // ponytail: verification reads by path below that root. Someone who can rename
            // entries in the output folder could swap it mid-read, but could as well swap
            // the published output a moment later; that writer is outside the threat model.
            if tree.root() != part.path() {
                bail!("{} was replaced while verifying", part.path().display());
            }
            verify::compare(&expected, &mut tree, ctx, mode)?
        }
        (Plan::Image(Image::Exfat(_)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            let len = file.metadata()?.len();
            let volume = ExFat::from_file(PkgFile::from_reader(Box::new(file), len), &label)
                .map_err(reopen)?;
            checks.push(exfat_geometry(&volume)?);
            ctx.check()?;
            let tree =
                ExFatSource::from_volume(volume, format!("exfat {label}")).map_err(reopen)?;
            checks.extend(content(
                &expected,
                physical.as_ref(),
                runtime,
                Box::new(tree),
                ctx,
                mode,
            )?);
            checks
        }
        (Plan::Image(Image::Ffpkg(layout)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            checks.extend(ufs2_geometry(&mut &file, layout)?);
            let tree = Ufs2Source::from_reader(Box::new(file), format!("ffpkg {label}"))
                .map_err(reopen)?;
            checks.push(format!("reader: {}", tree.describe()));
            checks.extend(content(
                &expected,
                physical.as_ref(),
                runtime,
                Box::new(tree),
                ctx,
                mode,
            )?);
            checks
        }
        (Plan::Image(Image::Ffpfs(_)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            let tree =
                PfsSource::from_reader(Box::new(file), format!("ffpfs {label}")).map_err(reopen)?;
            checks.push(pfs_geometry(tree.header())?);
            checks.extend(content(
                &expected,
                physical.as_ref(),
                runtime,
                Box::new(tree),
                ctx,
                mode,
            )?);
            checks
        }
        (Plan::Ffpfsc(inner, name), Some(file)) => {
            let report = wrapped
                .as_ref()
                .expect("a .ffpfsc job keeps its wrap report");
            let mut checks = image_checks(&file, bytes)?;
            let copy = file.try_clone().context("reading the output back")?;
            let (tree, info) =
                ps5_dump_forge_pfs::open_ffpfsc(Box::new(copy), &label).map_err(reopen)?;
            checks.push(pfs_geometry(&info.outer)?);
            checks.push(container_check(&info, name, inner.size(), report)?);
            // The inner image's own layout, read through the container like SMP reads it.
            let outer = PfsSource::from_reader(Box::new(file), label.clone()).map_err(reopen)?;
            let nested = Nested::new(outer, name)?;
            let geometry = image_geometry(inner, Box::new(nested), &format!("{label} ({name})"))?;
            checks.extend(geometry.into_iter().map(|c| format!("inner image {c}")));
            ctx.check()?;
            checks.extend(content(
                &expected,
                physical.as_ref(),
                runtime,
                tree,
                ctx,
                mode,
            )?);
            checks
        }
        _ => unreachable!("image plans keep their file"),
    };
    for line in &checks {
        ctx.log(format!("check: {line}"));
    }

    ctx.expect_rest(0);
    ctx.progress("finalize", 0, 1);
    // Checked after the progress event, so a cancel sent in reaction to it still lands.
    ctx.check()?;
    stamp.check()?;
    if in_place {
        if let Some(backup) = part.replace(&out)? {
            ctx.log(format!(
                "in place: the original image is still at {}; delete it once the new one works",
                backup.display()
            ));
        }
        checks.push(format!("replaced {} with the verified copy", out.display()));
    } else {
        part.publish(&out)?;
        checks.push(format!("published {}", out.display()));
    }
    ctx.progress("finalize", 1, 1);
    Ok(JobReport {
        output: out,
        bytes,
        files: expected.files.len() as u64,
        checks,
        verify: summary,
    })
}

/// Compression threads: the request's cap, else every core.
fn compression_threads(req: &ConvertRequest) -> usize {
    match req.compression_threads.unwrap_or(0) {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    }
}

/// The measure pass of LZ4 packs into an image: `pack`'s compression over the packed files,
/// keeping only the metadata (no compressed data), so the image can be laid out from exact
/// sizes. The write pass reads the packed files a second time.
fn measure_packs(
    tree: &mut dyn SourceTree,
    pack: &crate::lz4::PackPlan,
    req: &ConvertRequest,
    mtime: i64,
    ctx: &Ctx,
) -> anyhow::Result<ps5_dump_forge_lz4::writer::Measured> {
    let total = pack
        .files
        .iter()
        .filter(|f| f.spec.is_some())
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    let threads = compression_threads(req);
    ctx.log(
        "LZ4 into an image: packing straight into it, no temporary folder; the packed files are \
         read twice (measure, then write)",
    );
    // Measuring never fills the bar: the write reads about every byte again (the packs are
    // at most their source's size, a little over for RAW-heavy ones), and the check reads
    // back about the same sample twice, as written and through the packs. The write pass
    // renews this from the plan.
    let bytes = tree
        .files()
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    let fast = Mode::Fast {
        seed: 0,
        seam: true,
    };
    let check = match req.full_verify {
        true => bytes,
        false => verify::summary(tree.files(), fast).checked_bytes,
    };
    ctx.expect_rest(
        total
            .saturating_add(bytes)
            .saturating_add(check.saturating_mul(2)),
    );
    ctx.progress("measure", 0, total);
    let started = std::time::Instant::now();
    let m = ps5_dump_forge_lz4::writer::measure(
        tree,
        &pack.files,
        mtime,
        pack.runtime.as_ref(),
        threads,
        &mut |n| ctx.progress("measure", n, total),
        ctx.cancel,
    )?;
    ctx.progress("measure", total, total);
    let r = m.report;
    let secs = started.elapsed().as_secs_f64().max(1e-3);
    let chunks = (m.crc.len() - ps5_dump_forge_lz4::format::CRC_HEADER) / 4;
    ctx.log(format!(
        "measured LZ4 packs: {} files packed into {} volumes, {} loose; stored {}% of {} bytes; \
         {:.0} MB/s on {threads} threads; {chunks} chunks, {} bytes of metadata in memory; \
         rules: {}",
        r.packed,
        r.volumes,
        r.loose,
        percent(r.stored_bytes, r.raw_bytes),
        r.raw_bytes,
        r.raw_bytes as f64 / 1e6 / secs,
        m.metadata_bytes(),
        pack.rules
    ));
    Ok(m)
}

/// [`Image::plan`] for the source, or, packing, for the packed tree made from it.
fn plan_image(
    format: Format,
    source: &mut dyn SourceTree,
    packed: Option<(&ps5_dump_forge_lz4::writer::Measured, &crate::lz4::PackPlan)>,
    info: &GameInfo,
    free_bytes: u64,
    cancel: &AtomicBool,
) -> Result<Image, String> {
    let Some((m, pack)) = packed else {
        return Image::plan(format, source, info, free_bytes, cancel);
    };
    let tree = ps5_dump_forge_lz4::tree::PackedTree::new(source, m, &pack.files, 1, cancel)
        .map_err(|e| format!("LZ4 packs: {e}"))?;
    Image::plan(format, &tree, info, free_bytes, cancel)
}

/// Writes `plan`'s image (alone or in a `.ffpfsc`) from `tree` into a new `.part`; returns the
/// `.part`, its open file, the image's size and, for a `.ffpfsc`, the container report.
#[allow(clippy::too_many_arguments)]
fn write_image(
    plan: &Plan,
    tree: &mut dyn SourceTree,
    part_path: &Path,
    req: &ConvertRequest,
    ctx: &Ctx,
    #[cfg(target_os = "freebsd")] dest: &crate::dest::Dest,
    #[cfg(target_os = "freebsd")] cadence: &mut crate::durable::Cadence,
) -> anyhow::Result<(Part, File, u64, Option<WrapReport>)> {
    // No shadowing: `file` must drop before `part`, which deletes the `.part` (U6).
    #[cfg_attr(not(target_os = "freebsd"), allow(unused_mut))]
    let (mut part, mut file) = Part::create_file(part_path)?;
    #[cfg(target_os = "freebsd")]
    part.set_dest(dest);
    match plan {
        Plan::Image(image) => {
            #[cfg(not(target_os = "freebsd"))]
            let (size, line) = image.write(tree, &mut file, ctx)?;
            #[cfg(target_os = "freebsd")]
            let (size, line) = synced(&mut file, cadence, ctx, |out| image.write(tree, out, ctx))?;
            ctx.log(format!("wrote {line}"));
            Ok((part, file, size, None))
        }
        Plan::Ffpfsc(image, name) => {
            let opts = WrapOptions {
                threads: req.compression_threads.unwrap_or(0),
                level: req.ffpfsc_level,
                ..WrapOptions::default()
            };
            ctx.log(format!(
                ".ffpfsc: zlib level {} of 0–9 (miniz_oxide), 64 KiB blocks, a block kept compressed \
                 when it saves at least {}%, {} compression threads",
                opts.level,
                opts.min_block_gain,
                compression_threads(req)
            ));
            let started = std::time::Instant::now();
            #[cfg(not(target_os = "freebsd"))]
            let (line, report) = ps5_dump_forge_pfs::wrap(
                name,
                image.size(),
                &mut file,
                &opts,
                ctx.cancel,
                |stream| Ok(image.write(tree, stream, ctx)?.1),
            )?;
            #[cfg(target_os = "freebsd")]
            let (line, report) = synced(&mut file, cadence, ctx, |out| {
                ps5_dump_forge_pfs::wrap(name, image.size(), out, &opts, ctx.cancel, |stream| {
                    Ok(image.write(tree, stream, ctx)?.1)
                })
            })?;
            let secs = started.elapsed().as_secs_f64().max(1e-3);
            ctx.log(format!(
                "wrote .ffpfsc: {} bytes holding {name} ({line}); {} of {} blocks compressed, \
                 {} raw; stored {:.1}% of {} bytes; {:.0} MB/s of image at zlib level {}",
                report.image_size,
                report.compressed_blocks,
                report.blocks,
                report.blocks - report.compressed_blocks,
                report.stored_size as f64 * 100.0 / report.raw_size.max(1) as f64,
                report.raw_size,
                report.raw_size as f64 / 1e6 / secs,
                opts.level
            ));
            Ok((part, file, report.image_size, Some(report)))
        }
        _ => unreachable!("only image plans are written as one file"),
    }
}

/// Every runtime check on the packs `tree` holds (the manifest, its sidecars and every volume
/// header), then the logical files read back through the packs against `expected`.
fn pack_checks(
    expected: &verify::Expected,
    tree: Box<dyn SourceTree>,
    runtime: Option<ps5_dump_forge_lz4::RuntimeProfile>,
    ctx: &Ctx,
    mode: Mode,
) -> anyhow::Result<Vec<String>> {
    let mut tree = ps5_dump_forge_lz4::reader::unpack(tree).map_err(reopen)?;
    // The reader takes packs without the optional sidecars; this job wrote them.
    if !tree.has_crc() {
        bail!(
            "verification failed: {} is missing",
            ps5_dump_forge_lz4::CRC_SIDECAR
        );
    }
    if tree.profile() != runtime {
        bail!(
            "verification failed: {} is {}, the job's profile asked for {:?}",
            ps5_dump_forge_lz4::PROFILE,
            tree.profile()
                .map_or("missing".into(), |p| format!("{p:?}")),
            runtime
        );
    }
    let mut checks = vec![format!(
        "packs: the manifest, its CRC sidecar{} and {} volume headers pass the runtime's checks",
        if runtime.is_some() {
            ", its runtime profile"
        } else {
            ""
        },
        tree.manifest().packs.len()
    )];
    checks.extend(verify::compare(expected, &mut tree, ctx, mode)?);
    Ok(checks)
}

/// An image's content: compared with `expected`, or, packed (`physical` given), compared as
/// written with `physical` and then through its packs with `expected`.
fn content(
    expected: &verify::Expected,
    physical: Option<&verify::Expected>,
    runtime: Option<ps5_dump_forge_lz4::RuntimeProfile>,
    mut tree: Box<dyn SourceTree>,
    ctx: &Ctx,
    mode: Mode,
) -> anyhow::Result<Vec<String>> {
    let Some(physical) = physical else {
        return verify::compare(expected, tree.as_mut(), ctx, mode);
    };
    ctx.reserve(verify::summary(&expected.files, mode).checked_bytes);
    let mut checks: Vec<String> = verify::compare(physical, tree.as_mut(), ctx, mode)?
        .into_iter()
        .map(|c| format!("as written, {c}"))
        .collect();
    ctx.reserve(0);
    checks.extend(pack_checks(expected, tree, runtime, ctx, mode)?);
    Ok(checks)
}

/// Packs `pack`'s packed files from `tree` (the hashing tree) into the `.part` folder: the
/// volumes, then the manifest and its sidecars. Returns the bytes the chunks take.
fn write_packs(
    tree: &mut dyn SourceTree,
    pack: &crate::lz4::PackPlan,
    part: &Part,
    req: &ConvertRequest,
    mtime: i64,
    ctx: &Ctx,
    #[cfg(target_os = "freebsd")] cadence: &mut crate::durable::Cadence,
) -> anyhow::Result<u64> {
    let total = pack
        .files
        .iter()
        .filter(|f| f.spec.is_some())
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    let threads = compression_threads(req);
    ctx.progress("pack", 0, total);
    let mut dest = crate::extract::Dest::open(part)?;
    let started = std::time::Instant::now();
    let report = {
        let mut out = crate::lz4::PartOutput::new(
            &mut dest,
            ctx.cancel,
            #[cfg(target_os = "freebsd")]
            &mut *cadence,
        );
        ps5_dump_forge_lz4::writer::pack(
            tree,
            &pack.files,
            mtime,
            pack.runtime.as_ref(),
            threads,
            &mut out,
            &mut |n| ctx.progress("pack", n, total),
            ctx.cancel,
        )?
    };
    #[cfg(target_os = "freebsd")]
    log_cadence(cadence, ctx);
    dest.sync(ctx)?;
    ctx.progress("pack", total, total);
    let secs = started.elapsed().as_secs_f64().max(1e-3);
    ctx.log(format!(
        "wrote LZ4 packs: {} files packed into {} volumes, {} loose; stored {}% of {} bytes; \
         {:.0} MB/s on {threads} threads; rules: {}",
        report.packed,
        report.volumes,
        report.loose,
        percent(report.stored_bytes, report.raw_bytes),
        report.raw_bytes,
        report.raw_bytes as f64 / 1e6 / secs,
        pack.rules
    ));
    Ok(report.stored_bytes)
}

/// The SMP exFAT layout, read from the volume's boot sector.
fn exfat_geometry(volume: &ExFat) -> anyhow::Result<String> {
    let geometry = volume.geometry();
    if geometry.volume_offset != 0
        || geometry.sector_size != 512
        || geometry.cluster_size != IMAGE_ALIGN
    {
        bail!("exFAT geometry is not the SMP layout: {geometry:?}");
    }
    Ok("geometry: raw volume at byte 0, 512 B sectors, 64 KiB clusters".into())
}

/// The PFS header this crate writes: PS5 (version 2), mode 0x8 (case-insensitive, unsigned,
/// 32-bit inodes, unencrypted), 64 KiB blocks, and as many blocks as the file holds.
fn pfs_geometry(h: &PfsHeader) -> anyhow::Result<String> {
    let mut bad = Vec::new();
    if h.version != 2 {
        bad.push(format!("version {}, not 2 (PS5)", h.version));
    }
    if h.mode != 0x8 {
        bad.push(format!("mode {:#x}, not 0x8", h.mode));
    }
    if u64::from(h.block_size) != ps5_dump_forge_pfs::BLOCK {
        bad.push(format!("block size {}, not 65536", h.block_size));
    }
    if h.ndblock.checked_mul(u64::from(h.block_size)) != Some(h.image_len) {
        bad.push(format!(
            "{} blocks of {} is not the image's {} bytes",
            h.ndblock, h.block_size, h.image_len
        ));
    }
    if !bad.is_empty() {
        bail!("PFS header is not the SMP layout:\n  {}", bad.join("\n  "));
    }
    Ok(format!(
        "geometry: PFS v2 at byte 0, mode 0x8, 64 KiB blocks, {} blocks = {} bytes, {} inodes",
        h.ndblock, h.image_len, h.inodes
    ))
}

/// The container holds the one image the job wrote, as written: its name, its length, every
/// 64 KiB block in the PFSC table, and the stored bytes the writer reported.
fn container_check(
    info: &FfpfscInfo,
    name: &str,
    raw: u64,
    report: &WrapReport,
) -> anyhow::Result<String> {
    let blocks = raw.div_ceil(ps5_dump_forge_pfs::BLOCK);
    let got = (
        info.inner_name.as_str(),
        info.raw_size,
        info.compressed,
        info.blocks,
        info.stored_size,
        info.compressed_blocks,
    );
    let want = (
        name,
        raw,
        true,
        blocks,
        report.stored_size,
        report.compressed_blocks,
    );
    if got != want {
        bail!(
            "the .ffpfsc container is not what was written: (name, length, compressed, blocks, \
             stored, compressed blocks) {got:?}, expected {want:?}"
        );
    }
    Ok(format!(
        "container: {name}, {raw} bytes in {blocks} PFSC blocks ({} compressed), stored in {} \
         bytes ({}%)",
        info.compressed_blocks,
        info.stored_size,
        percent(info.stored_size, raw)
    ))
}

/// `part` as a whole percentage of `whole`.
pub(crate) fn percent(part: u64, whole: u64) -> u64 {
    (u128::from(part) * 100)
        .checked_div(u128::from(whole))
        .map_or(0, |p| p as u64)
}

/// An image's own layout facts, read back from `r` (the image's bytes) rather than taken
/// from the writer's word.
fn image_geometry(
    image: &Image,
    mut r: Box<dyn ReadSeek>,
    label: &str,
) -> anyhow::Result<Vec<String>> {
    Ok(match image {
        Image::Exfat(_) => {
            let len = r.seek(SeekFrom::End(0))?;
            let volume = ExFat::from_file(PkgFile::from_reader(r, len), label).map_err(reopen)?;
            vec![exfat_geometry(&volume)?]
        }
        Image::Ffpkg(layout) => ufs2_geometry(&mut r, layout)?,
        Image::Ffpfs(_) => {
            let tree = PfsSource::from_reader(r, label.to_string()).map_err(reopen)?;
            vec![pfs_geometry(tree.header())?]
        }
    })
}

/// One file of a PFS image (the image inside a `.ffpfsc`) as `Read + Seek`, decoded through
/// the reader, so its own filesystem can be checked.
pub(crate) struct Nested {
    pfs: PfsSource,
    path: String,
    len: u64,
    pos: u64,
}

impl Nested {
    pub(crate) fn new(pfs: PfsSource, path: &str) -> anyhow::Result<Self> {
        let Some(len) = pfs.files().iter().find(|f| f.path == path).map(|f| f.size) else {
            bail!("{}: no {path} in the container", pfs.describe());
        };
        Ok(Self {
            pfs,
            path: path.to_string(),
            len,
            pos: 0,
        })
    }
}

impl Read for Nested {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let want = (buf.len() as u64).min(self.len.saturating_sub(self.pos)) as usize;
        if want == 0 {
            return Ok(0);
        }
        let bytes = self
            .pfs
            .read_range(&self.path, self.pos, want)
            .map_err(io::Error::other)?;
        let n = bytes.len().min(want);
        buf[..n].copy_from_slice(&bytes[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Nested {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::other("seek before the start"))?;
        Ok(self.pos)
    }
}

/// The image file is exactly as large as the writer said, and 64 KiB aligned.
fn image_checks(file: &File, size: u64) -> anyhow::Result<Vec<String>> {
    let len = file.metadata()?.len();
    if len != size {
        bail!("the image is {len} bytes, but the writer reported {size}");
    }
    if len % IMAGE_ALIGN != 0 {
        bail!("the image size {len} is not a multiple of 64 KiB");
    }
    Ok(vec![format!("image size: {len} bytes, 64 KiB aligned")])
}

/// The `.ffpkg` the writer made (`r`, the image's bytes) has the SMP geometry and the layout
/// it planned: read from its superblock and root inode, not taken from the writer's word.
fn ufs2_geometry<R: Read + Seek>(
    r: &mut R,
    layout: &ps5_dump_forge_ufs2::Layout,
) -> anyhow::Result<Vec<String>> {
    const SBLOCK: u64 = 65536;
    const FS_UFS2_MAGIC: u32 = 0x1954_0119;
    const BLOCK: u64 = 65536;
    const ROOT_INODE: u64 = 2;
    const INODE_SIZE: u64 = 256;
    const ROOT_MODE: u16 = 0o040777;
    let len = r.seek(SeekFrom::End(0)).context("sizing the image")?;
    let mut read_at = |at: u64, buf: &mut [u8]| -> io::Result<()> {
        r.seek(SeekFrom::Start(at))?;
        r.read_exact(buf)
    };
    let mut sb = [0u8; 1376];
    read_at(SBLOCK, &mut sb).context("reading the UFS2 superblock")?;
    let u32_at = |at: usize| u32::from_le_bytes(sb[at..at + 4].try_into().expect("4 bytes"));
    // The i32 fields read as u32: a negative one is far too large and fails below.
    let field = |at: usize| u64::from(u32_at(at));
    let magic = u32_at(1372);
    let (iblkno, dblkno, ncg) = (field(16), field(20), field(44));
    let (bsize, fsize, fpg) = (field(48), field(52), field(188));
    let size = u64::from_le_bytes(sb[1080..1088].try_into().expect("8 bytes"));
    let last = ncg
        .checked_sub(1)
        .and_then(|n| n.checked_mul(fpg))
        .and_then(|full| size.checked_sub(full));
    let mut bad = Vec::new();
    if magic != FS_UFS2_MAGIC {
        bad.push(format!("magic {magic:#x}, not UFS2's {FS_UFS2_MAGIC:#x}"));
    }
    if bsize != BLOCK || fsize != BLOCK {
        bad.push(format!(
            "block {bsize} / fragment {fsize}, not 65536 / 65536"
        ));
    }
    if size.checked_mul(fsize) != Some(len) {
        bad.push(format!(
            "fs_size {size} fragments of {fsize} is not the file's {len} bytes"
        ));
    }
    if (ncg, fpg, dblkno)
        != (
            layout.cylinder_groups,
            layout.blocks_per_group,
            layout.metadata_blocks_per_group,
        )
    {
        bad.push(format!(
            "{ncg} groups of {fpg} blocks with data from block {dblkno}, planned {} of {} from {}",
            layout.cylinder_groups, layout.blocks_per_group, layout.metadata_blocks_per_group
        ));
    }
    match last {
        Some(last) if last > dblkno && last == layout.last_group_blocks => {}
        _ => bad.push(format!(
            "the last group has {last:?} blocks, planned {}; it must hold more than its \
             {dblkno} metadata blocks",
            layout.last_group_blocks
        )),
    }
    let mut mode = [0u8; 2];
    let root = iblkno
        .checked_mul(fsize)
        .and_then(|at| at.checked_add(ROOT_INODE * INODE_SIZE))
        .filter(|at| at + INODE_SIZE <= len);
    match root {
        Some(at) => read_at(at, &mut mode).context("reading the root inode")?,
        None => bad.push(format!(
            "the inode table at block {iblkno} is outside the image"
        )),
    }
    let mode = u16::from_le_bytes(mode);
    if root.is_some() && mode != ROOT_MODE {
        bad.push(format!("root directory mode {mode:o}, not {ROOT_MODE:o}"));
    }
    if !bad.is_empty() {
        bail!(
            "UFS2 geometry is not the SMP layout:\n  {}",
            bad.join("\n  ")
        );
    }
    Ok(vec![
        format!(
            "geometry: UFS2 at byte 0, 64 KiB blocks and fragments, {size} blocks = file length, \
             {ncg} groups of {fpg}"
        ),
        format!(
            "last group: {} blocks, more than its {dblkno} metadata blocks",
            last.unwrap_or(0)
        ),
        "root directory: mode 0777".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use ps5upload_fpkg::source::SourceFile;

    /// Only the unix-only geometry test uses it.
    #[cfg(unix)]
    struct Mem(Vec<SourceFile>);

    #[cfg(unix)]
    impl SourceTree for Mem {
        fn files(&self) -> &[SourceFile] {
            &self.0
        }
        fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
            let size = self.0.iter().find(|f| f.path == path).map_or(0, |f| f.size);
            Ok(vec![7; size as usize])
        }
        fn describe(&self) -> String {
            "mem".into()
        }
    }

    #[cfg(unix)]
    #[test]
    fn ufs2_geometry_is_read_from_the_image() {
        let dir = crate::test_dir("ufs2-geometry");
        let file = |p: &str, size| SourceFile {
            path: p.into(),
            size,
        };
        let mut tree = Mem(vec![file("eboot.bin", 100), file("data/a.bin", 70_000)]);
        let cancel = AtomicBool::new(false);
        let opts = ps5_dump_forge_ufs2::Options::default();
        let layout = ps5_dump_forge_ufs2::plan(&tree, &opts, &cancel).unwrap();
        let path = dir.join("x.ffpkg");
        let mut out = crate::finalize::create_new(&path).unwrap();
        ps5_dump_forge_ufs2::write(&mut tree, &layout, &mut out, &cancel, &mut |_, _| {}).unwrap();
        let lines = ufs2_geometry(&mut &out, &layout).unwrap();
        assert_eq!(lines.len(), 3, "{lines:?}");

        // A root directory that is not 0777, then a wrong block size: both refused.
        let poke = |at: u64, bytes: &[u8]| {
            use std::os::unix::fs::FileExt;
            out.write_all_at(bytes, at).unwrap();
        };
        let root_mode = 4 * 65536 + 2 * 256;
        poke(root_mode, &0o040755u16.to_le_bytes());
        let err = ufs2_geometry(&mut &out, &layout).unwrap_err().to_string();
        assert!(err.contains("root directory mode 40755"), "{err}");
        poke(root_mode, &0o040777u16.to_le_bytes());
        poke(65536 + 48, &32768u32.to_le_bytes());
        let err = ufs2_geometry(&mut &out, &layout).unwrap_err().to_string();
        assert!(err.contains("block 32768"), "{err}");
    }

    #[test]
    fn a_ffpfsc_is_sized_as_if_no_block_compressed() {
        // An inner image FAT32 holds by itself, at the 4 GiB boundary, still makes a
        // container too big for it: preflight's `largest` is the container's worst case.
        let raw = crate::preflight::FAT_MAX_FILE + 1 - 512 * 1024;
        let max = ps5_dump_forge_pfs::container_size_max(raw).unwrap();
        assert!(raw < crate::preflight::FAT_MAX_FILE);
        assert!(max > crate::preflight::FAT_MAX_FILE, "{max}");
        assert_eq!(max % IMAGE_ALIGN, 0);
    }

    #[test]
    fn a_changed_image_source_is_caught() {
        let dir = crate::test_dir("stamp");
        let path = dir.join("game.exfat");
        std::fs::write(&path, b"image").unwrap();
        let stamp = SourceStamp::take(&path, Kind::Exfat).unwrap();
        stamp.check().unwrap();
        std::fs::write(&path, b"another image").unwrap();
        assert!(stamp.check().is_err());
        // Swapped for a file of the same length and time: a different inode.
        let stamp = SourceStamp::take(&path, Kind::Exfat).unwrap();
        let other = dir.join("other");
        std::fs::write(&other, b"another image").unwrap();
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        File::options()
            .write(true)
            .open(&other)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        std::fs::rename(&other, &path).unwrap();
        // Not on msdosfs: the renamed file takes the old slot's id (see `SourceStamp`).
        #[cfg(target_os = "freebsd")]
        let fat = crate::dest::fstype_of(&dir).unwrap() == "msdosfs";
        #[cfg(not(target_os = "freebsd"))]
        let fat = false;
        if !fat {
            assert!(stamp.check().is_err());
        }
        // A folder source is checked per file by the scanner instead.
        SourceStamp::take(&dir, Kind::Folder)
            .unwrap()
            .check()
            .unwrap();
    }
}
