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
use crate::preflight::{self, GameInfo};
use crate::scan::{JunkFiltered, ScannedFolder};
use crate::verify::{self, HashingTree};
use crate::{ConvertRequest, Format, JobReport};

/// Every SMP image size and cluster is a multiple of this.
const IMAGE_ALIGN: u64 = 64 * 1024;

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

fn reopen(e: ps5upload_fpkg::Error) -> anyhow::Error {
    anyhow::anyhow!("reading the output back: {e}")
}

/// What an image source file looked like when the job opened it. Checked again before
/// publishing, so an image rewritten mid-conversion fails the job; a folder source checks
/// each file on every read instead.
pub(crate) struct SourceStamp {
    path: PathBuf,
    seen: Option<Seen>,
}

/// (length, modification time, (device, inode)).
type Seen = (u64, Option<SystemTime>, Option<(u64, u64)>);

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

    fn seen(path: &Path) -> anyhow::Result<Seen> {
        let meta = std::fs::metadata(path).with_context(|| format!("{}", path.display()))?;
        #[cfg(unix)]
        let id = {
            use std::os::unix::fs::MetadataExt;
            Some((meta.dev(), meta.ino()))
        };
        // ponytail: Windows file ids need GetFileInformationByHandle; length and mtime only.
        #[cfg(not(unix))]
        let id = None;
        Ok((meta.len(), meta.modified().ok(), id))
    }

    pub(crate) fn check(&self) -> anyhow::Result<()> {
        if let Some(seen) = &self.seen
            && Self::seen(&self.path).ok().as_ref() != Some(seen)
        {
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
    /// Lays `format`'s image out for `tree`; a refusal is a preflight finding.
    fn plan(
        format: Format,
        tree: &dyn SourceTree,
        info: &GameInfo,
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
                    free_bytes: 0,
                    label: label.into(),
                };
                ps5_dump_forge_exfat::plan(tree, &opts, cancel)
                    .map(Self::Exfat)
                    .map_err(|e| format!("exFAT layout: {e}"))
            }
            Format::Ffpkg => {
                let opts = ps5_dump_forge_ufs2::Options { free_bytes: 0 };
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
    Image(Image),
    /// The image inside, and its name in the container.
    Ffpfsc(Image, String),
}

pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    ctx.progress("scan", 0, 1);
    let kind = Kind::of(&req.source)?;
    // Before the open: a change after this point, however early, fails the publish.
    let stamp = SourceStamp::take(&req.source, kind)?;
    let mut source = open_source(&req.source, kind, ctx.cancel)?;
    ctx.log(format!("source: {}", source.describe()));
    ctx.progress("scan", 1, 1);
    ctx.check()?;

    ctx.progress("preflight", 0, 1);
    let out = preflight::output_path(&req.output)?;
    let (info, mut findings) = preflight::input(source.as_mut());
    if let Some(id) = &info.title_id {
        ctx.log(format!("title id: {id}"));
    }
    findings.extend(preflight::output(&req.source, &out, req.format));
    if req.format == Format::Pkg {
        return crate::package::run(req, ctx, source.as_mut(), &info, findings, &out, &stamp);
    }
    if req.format != Format::Folder {
        findings.extend(preflight::too_deep(source.as_ref()));
    }
    let mut need = preflight::estimate(source.as_ref());
    let mut largest = need;
    let planned = |format: Format, findings: &mut Vec<String>| {
        Image::plan(format, source.as_ref(), &info, ctx.cancel)
            .map_err(|e| findings.push(e))
            .ok()
    };
    let plan = match req.format {
        Format::Folder => {
            let paths: Vec<String> = source.files().iter().map(|f| f.path.clone()).collect();
            findings.extend(crate::extract::extraction_findings(
                &paths,
                source.empty_dirs(),
                crate::extract::CASE_INSENSITIVE,
            ));
            largest = source.files().iter().map(|f| f.size).max().unwrap_or(0);
            Some(Plan::Folder)
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
    let (space, notes) = preflight::destination(dir, need, largest);
    findings.extend(space);
    for note in notes {
        ctx.log(note);
    }
    let Some(plan) = plan.filter(|_| findings.is_empty()) else {
        bail!("preflight failed:\n  {}", findings.join("\n  "));
    };
    ctx.progress("preflight", 1, 1);
    // Write reads every source byte once, verify reads them back once.
    let file_bytes = source
        .files()
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    ctx.expect_rest(file_bytes.saturating_mul(2));

    let part_path = part_path(&out, ctx.job);
    let mut hashing = HashingTree::new(source.as_mut());
    // The image `.part` stays open from creation to verification, so what is verified is
    // the file this job wrote, whatever happens to its name meanwhile.
    let (part, image, bytes, wrapped) = match &plan {
        Plan::Folder => {
            let part = Part::create_dir(&part_path)?;
            let bytes = crate::extract::write(&mut hashing, &part, ctx)?;
            (part, None, bytes, None)
        }
        Plan::Image(image) => {
            let (part, mut file) = Part::create_file(&part_path)?;
            let (size, line) = image.write(&mut hashing, &mut file, ctx)?;
            ctx.log(format!("wrote {line}"));
            (part, Some(file), size, None)
        }
        Plan::Ffpfsc(image, name) => {
            let (part, mut file) = Part::create_file(&part_path)?;
            let opts = WrapOptions {
                threads: req.compression_threads.unwrap_or(0),
                ..WrapOptions::default()
            };
            let (line, report) = ps5_dump_forge_pfs::wrap(
                name,
                image.size(),
                &mut file,
                &opts,
                ctx.cancel,
                |stream| Ok(image.write(&mut hashing, stream, ctx)?.1),
            )?;
            ctx.log(format!(
                "wrote .ffpfsc: {} bytes holding {name} ({line}); {} of {} blocks compressed",
                report.image_size, report.compressed_blocks, report.blocks
            ));
            (part, Some(file), report.image_size, Some(report))
        }
    };
    if let Some(file) = &image {
        file.sync_all().context("syncing the image")?;
    }
    ctx.check()?;

    ctx.progress("verify", 0, 1);
    let expected = hashing.expected(ctx)?;
    ctx.check()?;
    let label = part.path().display().to_string();
    let mut checks = match (&plan, image) {
        (Plan::Folder, _) => {
            let mut tree = ScannedFolder::scan(part.path(), ctx.cancel)?;
            // `scan` resolves its root; a `.part` swapped for a link resolves elsewhere.
            // ponytail: verification reads by path below that root. Someone who can rename
            // entries in the output folder could swap it mid-read, but could as well swap
            // the published output a moment later; that writer is outside the threat model.
            if tree.root() != part.path() {
                bail!("{} was replaced while verifying", part.path().display());
            }
            verify::compare(&expected, &mut tree, ctx)?
        }
        (Plan::Image(Image::Exfat(_)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            let len = file.metadata()?.len();
            let volume = ExFat::from_file(PkgFile::from_reader(Box::new(file), len), &label)
                .map_err(reopen)?;
            checks.push(exfat_geometry(&volume)?);
            ctx.check()?;
            let mut tree =
                ExFatSource::from_volume(volume, format!("exfat {label}")).map_err(reopen)?;
            checks.extend(verify::compare(&expected, &mut tree, ctx)?);
            checks
        }
        (Plan::Image(Image::Ffpkg(layout)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            checks.extend(ufs2_geometry(&mut &file, layout)?);
            let mut tree = Ufs2Source::from_reader(Box::new(file), format!("ffpkg {label}"))
                .map_err(reopen)?;
            checks.push(format!("reader: {}", tree.describe()));
            checks.extend(verify::compare(&expected, &mut tree, ctx)?);
            checks
        }
        (Plan::Image(Image::Ffpfs(_)), Some(file)) => {
            let mut checks = image_checks(&file, bytes)?;
            let mut tree =
                PfsSource::from_reader(Box::new(file), format!("ffpfs {label}")).map_err(reopen)?;
            checks.push(pfs_geometry(tree.header())?);
            checks.extend(verify::compare(&expected, &mut tree, ctx)?);
            checks
        }
        (Plan::Ffpfsc(inner, name), Some(file)) => {
            let report = wrapped
                .as_ref()
                .expect("a .ffpfsc job keeps its wrap report");
            let mut checks = image_checks(&file, bytes)?;
            let copy = file.try_clone().context("reading the output back")?;
            let (mut tree, info) =
                ps5_dump_forge_pfs::open_ffpfsc(Box::new(copy), &label).map_err(reopen)?;
            checks.push(pfs_geometry(&info.outer)?);
            checks.push(container_check(&info, name, inner.size(), report)?);
            // The inner image's own layout, read through the container like SMP reads it.
            let outer = PfsSource::from_reader(Box::new(file), label.clone()).map_err(reopen)?;
            let nested = Nested::new(outer, name)?;
            let geometry = image_geometry(inner, Box::new(nested), &format!("{label} ({name})"))?;
            checks.extend(geometry.into_iter().map(|c| format!("inner image {c}")));
            ctx.check()?;
            checks.extend(verify::compare(&expected, tree.as_mut(), ctx)?);
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
    part.publish(&out)?;
    ctx.progress("finalize", 1, 1);
    checks.push(format!("published {}", out.display()));
    Ok(JobReport {
        output: out,
        bytes,
        files: expected.files.len() as u64,
        checks,
    })
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
    use ps5upload_fpkg::source::SourceFile;

    struct Mem(Vec<SourceFile>);

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
        let opts = ps5_dump_forge_ufs2::Options { free_bytes: 0 };
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

    #[cfg(unix)]
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
        assert!(stamp.check().is_err());
        // A folder source is checked per file by the scanner instead.
        SourceStamp::take(&dir, Kind::Folder)
            .unwrap()
            .check()
            .unwrap();
    }
}
