//! An in-place image edit (a request with [`ConvertRequest::edits`]): scan, preflight and the
//! format editor's plan of the writes, then `patch::apply` (undo journal first), a check of
//! the image through its reader, and commit; any failure after the apply rolls it back.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5_dump_forge_lz4::{CRC_SIDECAR, INDEX, JOURNAL, LOGS, MANIFEST, PROFILE, RUNTIME, reader};
use ps5_dump_forge_patch::{Applied, Data, Op, Recovery, Syncs, Write};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::convert::{Kind, SourceStamp, open_source};
use crate::edit::{Edited, HostFile, Opened};
use crate::finalize::{followed_id, handle_id};
use crate::jobs::Ctx;
use crate::lz4::{IndexState, RuntimeAction};
use crate::preflight;
use crate::verify::{self, Mode};
use crate::{ConvertRequest, EditCheck, Format, JobReport};

/// The `Progress` stages of an edit job, in order.
pub(crate) const STAGES: [&str; 5] = ["scan", "preflight", "write", "verify", "finalize"];

/// How the undo journal and the image are made durable: on the PS5 with ps5upload's retries
/// (a sync that only succeeded on a retry fails), elsewhere std's.
#[cfg(target_os = "freebsd")]
pub(crate) const SYNCS: Syncs = Syncs {
    file: sync_file,
    dir: crate::finalize::fsync_dir,
};
#[cfg(not(target_os = "freebsd"))]
pub(crate) const SYNCS: Syncs = Syncs::STD;

#[cfg(target_os = "freebsd")]
fn sync_file(file: &File) -> io::Result<()> {
    static NO_CANCEL: AtomicBool = AtomicBool::new(false);
    match crate::durable::sync_retry(file, false, &NO_CANCEL)? {
        crate::durable::Synced::Retried => Err(crate::durable::retried_sync()),
        _ => Ok(()),
    }
}

/// Room beyond the journal's own size its folder must have: what the filesystem spends on
/// the new file.
const JOURNAL_MARGIN: u64 = 1 << 20;

/// What the request asks for that an edit doesn't take.
pub(crate) fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if crate::net::is_network(&req.source) {
        findings.push(
            "a game on a NAS or another device can be browsed but not edited; convert it first"
                .into(),
        );
    }
    // A paired device's files (`flink://`) are added like this device's; a NAS's (SMB, NFS,
    // FTP) aren't.
    let from_nas: Vec<String> = (req.edits.iter())
        .filter_map(|e| match e {
            crate::Edit::Add { from, .. }
                if crate::remote::is_remote(from) && !crate::remote::is_flink(from) =>
            {
                Some(from.display().to_string())
            }
            _ => None,
        })
        .collect();
    if !from_nas.is_empty() {
        findings.push(format!(
            "files are added from this device or a paired one, not a NAS: {}",
            from_nas.join(", ")
        ));
    }
    if req.lz4.is_some() {
        findings
            .push("an edit takes no LZ4 option (lz4): edit the game, then use the Pack tab".into());
    }
    if req.zstd_pack || req.zstd_unpack {
        findings.push(
            "an edit takes no zstd option (zstd_pack, zstd_unpack): edit the game, then pack \
             it"
            .into(),
        );
    }
    if req.remove_backport {
        findings.push(
            "an edit does not remove the backport (remove_backport): delete its files instead"
                .into(),
        );
    }
    findings
}

/// Why `kind` can't be edited in place, or rebuilt with the edits when `rebuild` (`None`: it
/// can).
fn kind_finding(kind: Kind, rebuild: bool) -> Option<String> {
    match kind {
        Kind::Archive(_) => Some("archives can be browsed but not edited; convert it first".into()),
        Kind::Exfat | Kind::Ffpkg | Kind::Ffpfs | Kind::Ffpfsc | Kind::Pkg if rebuild => None,
        Kind::Folder if rebuild => Some("a folder is changed directly, never rebuilt".into()),
        Kind::Exfat | Kind::Ffpkg | Kind::Ffpfs => None,
        Kind::Ffpfsc => Some("a .ffpfsc can only be rebuilt, not edited in place".into()),
        Kind::Pkg => Some("a .pkg can only be rebuilt, not edited in place".into()),
        // ponytail: a folder is changed directly; that job is a later step.
        Kind::Folder => Some("editing a folder is not supported yet".into()),
    }
}

/// The format a `kind` of image is written as.
fn format_of(kind: Kind) -> Format {
    match kind {
        Kind::Exfat => Format::Exfat,
        Kind::Ffpkg => Format::Ffpkg,
        Kind::Ffpfs => Format::Ffpfs,
        Kind::Ffpfsc => Format::Ffpfsc,
        Kind::Pkg => Format::Pkg,
        other => unreachable!("{other:?} is not an image"),
    }
}

/// The format an edit request rebuilds its image as: the source's own, when the request has
/// edits and [`ConvertRequest::edit_rebuild`] and the source is an image (`None` otherwise: an
/// in-place edit, a folder's included, or an archive the edit job refuses). Such a request is
/// a conversion of the edited image to that format, replacing the source.
pub(crate) fn rebuild_format(req: &ConvertRequest) -> Option<Format> {
    if req.edits.is_empty() || !req.edit_rebuild {
        return None;
    }
    match Kind::of(&req.source).ok()? {
        Kind::Folder | Kind::Archive(_) => None,
        image => Some(format_of(image)),
    }
}

/// The format of the image inside a `.ffpfsc`, by its name's extension (SMP picks the inner
/// driver by it): what a rebuild writes inside the new container.
pub(crate) fn inner_format(image: &Path) -> anyhow::Result<Format> {
    let (_, info) = crate::convert::open_ffpfsc(image)?;
    let ext = Path::new(&info.inner_name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("exfat") => Ok(Format::Exfat),
        Some("ffpkg") => Ok(Format::Ffpkg),
        Some("ffpfs") => Ok(Format::Ffpfs),
        _ => bail!(
            "{}: its inner image {} is not an .exfat, .ffpkg or .ffpfs",
            image.display(),
            info.inner_name
        ),
    }
}

/// Why the image can't be edited because ShadowMount+ has it mounted (PS5 only; `None`
/// elsewhere, where there is no ShadowMount+). Checked in preflight and again before apply.
#[cfg(target_os = "freebsd")]
pub(crate) fn smp_finding(image: &Path) -> Option<String> {
    smp_inside(image).or_else(|| smp_mount(image, &mut smp_list, &mut smp_is_mount))
}

/// Why `path` can't be edited: it is ShadowMount+'s mount folder or inside it (as typed or
/// resolved), so it belongs to a mounted game, changed only through the image it was
/// mounted from. Folder edits, the LZ4 folder patch and Extract's destination check it too
/// (PS5 only).
pub(crate) fn smp_inside(path: &Path) -> Option<String> {
    crate::listing::smp_hidden(path).then(|| {
        format!(
            "{} is inside ShadowMount+'s mount folder, a mounted game: stop the game and change \
             the image it was mounted from instead",
            path.display()
        )
    })
}

#[cfg(not(target_os = "freebsd"))]
pub(crate) fn smp_finding(_: &Path) -> Option<String> {
    None
}

/// A folder's entry names, for [`smp_mounts`] on the real filesystem. A name that is not
/// UTF-8 is an error (fails closed): changed to fit, it would be stat'ed as another path.
#[cfg(target_os = "freebsd")]
pub(crate) fn smp_list(dir: &Path) -> io::Result<Vec<String>> {
    std::fs::read_dir(dir)?
        .map(|e| {
            e?.file_name().into_string().map_err(|name| {
                let e = format!("{}: a name that is not UTF-8", dir.join(name).display());
                io::Error::new(io::ErrorKind::InvalidData, e)
            })
        })
        .collect()
}

/// Whether `entry` is a mount point: its `st_dev` differs from its folder's.
#[cfg(target_os = "freebsd")]
pub(crate) fn smp_is_mount(entry: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(entry)?;
    let parent = entry.parent().unwrap_or(Path::new("/"));
    Ok(meta.is_dir() && meta.dev() != std::fs::metadata(parent)?.dev())
}

/// ShadowMount+'s mount folders (ps5upload v7.0.24 `smp::collect_status`).
#[cfg(any(target_os = "freebsd", test))]
const SMP_DIRS: [&str; 2] = ["/mnt/shadowmnt", "/mnt/shadowmnt/pfsc"];

/// The image extensions ShadowMount+ mounts, lower case.
#[cfg(any(target_os = "freebsd", test))]
const SMP_EXTENSIONS: [&str; 4] = ["exfat", "ffpkg", "ffpfs", "ffpfsc"];

/// The stem SMP names `name`'s mount by (the name without its extension), when `name` has an
/// extension SMP mounts (any case).
#[cfg(any(target_os = "freebsd", test))]
pub(crate) fn smp_stem(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    SMP_EXTENSIONS
        .iter()
        .any(|e| e.eq_ignore_ascii_case(ext))
        .then_some(stem)
}

/// ShadowMount+'s mounts whose stem `wanted` accepts, as (stem, mount point), over `list` (a
/// folder's entry names) and `is_mount` (an entry is a mount point): SMP mounts an image at
/// `<dir>/<stem>_<8 hex digits>`, the stem being its file name without the extension. Fails
/// closed: a listing or stat error other than "not found" is the `Err`, with where it was.
#[cfg(any(target_os = "freebsd", test))]
pub(crate) fn smp_mounts(
    list: &mut dyn FnMut(&Path) -> io::Result<Vec<String>>,
    is_mount: &mut dyn FnMut(&Path) -> io::Result<bool>,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Vec<(String, PathBuf)>, (PathBuf, io::Error)> {
    let stem_of = |entry: &str| {
        let (stem, hex) = entry.rsplit_once('_')?;
        (hex.len() == 8 && hex.bytes().all(|b| b.is_ascii_hexdigit()) && wanted(stem))
            .then(|| stem.to_string())
    };
    let mut found = Vec::new();
    for dir in SMP_DIRS.map(Path::new) {
        let entries = match list(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err((dir.to_path_buf(), e)),
        };
        for (entry, stem) in entries.iter().filter_map(|e| Some((e, stem_of(e)?))) {
            let at = dir.join(entry);
            match is_mount(&at) {
                Ok(false) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err((at, e)),
                Ok(true) => found.push((stem, at)),
            }
        }
    }
    Ok(found)
}

/// The rule behind [`smp_finding`]: the image's stem among [`smp_mounts`], compared exactly.
#[cfg(any(target_os = "freebsd", test))]
fn smp_mount(
    image: &Path,
    list: &mut dyn FnMut(&Path) -> io::Result<Vec<String>>,
    is_mount: &mut dyn FnMut(&Path) -> io::Result<bool>,
) -> Option<String> {
    let name = image.file_name().unwrap_or_default().to_string_lossy();
    let stem = name.rsplit_once('.').map_or(&*name, |(stem, _)| stem);
    match smp_mounts(list, is_mount, &|s| s == stem) {
        Ok(found) => found.first().map(|(_, at)| {
            format!(
                "ShadowMount+ has this image mounted at {}: stop the game, move the image out of \
                 ShadowMount+'s folders (or turn it off), then edit",
                at.display()
            )
        }),
        Err((at, e)) => Some(format!(
            "can't tell whether ShadowMount+ has this image mounted ({}: {e}); edit it once \
             that reads",
            at.display()
        )),
    }
}

/// The image as scan opened it.
pub(crate) struct Scanned {
    pub kind: Kind,
    pub image: PathBuf,
    /// Taken before the open, after any recovery: a change after it fails the apply.
    pub stamp: SourceStamp,
    pub tree: Box<dyn SourceTree>,
    /// What an earlier, interrupted edit left, now settled.
    pub recovery: Recovery,
}

/// Checks the request, the image's kind and name and that ShadowMount+ hasn't mounted it
/// ([`smp_finding`]); only then settles a journal an interrupted edit left
/// ([`ps5_dump_forge_patch::recover`]: it restores the image's own bytes, which any edit
/// needs first), stamps and opens the image. The inner `Err` lists why the request or the
/// image can't be edited in place (or rebuilt, when `rebuild`); the outer one is an error
/// reading it.
pub(crate) fn scan(
    req: &ConvertRequest,
    rebuild: bool,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<Scanned, Vec<String>>> {
    scan_with(req, rebuild, cancel, &smp_finding, true)
}

/// [`scan`] with its ShadowMount+ check given.
fn scan_with(
    req: &ConvertRequest,
    rebuild: bool,
    cancel: &AtomicBool,
    mounted: &dyn Fn(&Path) -> Option<String>,
    settle: bool,
) -> anyhow::Result<Result<Scanned, Vec<String>>> {
    let image = req.source.clone();
    let mut findings = request_findings(req);
    // A NAS or peer source is refused before it is asked anything (an offline one too).
    if crate::net::is_network(&image) {
        return Ok(Err(findings));
    }
    let kind = Kind::of(&image)?;
    findings.extend(kind_finding(kind, rebuild));
    if kind == Kind::Pkg && ps5_dump_forge_fpkg::is_patch(&image) {
        findings.push(preflight::PATCH_EDIT.into());
    }
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    // The name ShadowMount+ mounts it by, as a conversion's output name is checked.
    findings.extend(preflight::stem_finding(&image, format_of(kind)));
    // Before anything is written, recovery included.
    findings.extend(mounted(&image));
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    // A check reads only: a journal it finds is an edit under way (`settle` ran first).
    let journal = ps5_dump_forge_patch::journal_path(&image);
    let recovered = match settle {
        true => ps5_dump_forge_patch::recover(&image, SYNCS),
        false if std::fs::symlink_metadata(&journal).is_ok() => {
            return Ok(Err(vec![busy(&image)]));
        }
        false => Ok(Recovery::Clean),
    };
    let recovery = match recovered {
        Ok(r) => r,
        Err(e) => {
            return Ok(Err(vec![format!(
                "{}: an earlier edit's undo journal could not be settled: {e}",
                image.display()
            )]));
        }
    };
    let stamp = SourceStamp::take(&image, kind)?;
    let tree = open_source(&image, kind, cancel)?;
    Ok(Ok(Scanned {
        kind,
        image,
        stamp,
        tree,
        recovery,
    }))
}

/// An in-place edit, planned: nothing written yet.
pub(crate) struct Planned {
    pub kind: Kind,
    pub image: PathBuf,
    pub stamp: SourceStamp,
    /// The format editor's ops, the fresh `ampr_emu.index` last when there is one.
    pub ops: Vec<Op>,
    /// What applying them writes into the image.
    pub writes: Vec<Write>,
    /// The host files `Data::Source` ids name (by index).
    pub hosts: Vec<HostFile>,
    /// The tree the edited image must read back as.
    pub expected: Box<dyn SourceTree>,
    /// The undo journal's size (the live writes' old bytes and its framing).
    pub journal_bytes: u64,
    /// Lines for the job log.
    pub notes: Vec<String>,
}

/// Scan and preflight of an in-place edit. It writes nothing but a leftover journal's
/// recovery (see [`scan`]), which stays done when preflight refuses. The inner `Err` lists
/// every finding; the outer one is a cancel or an error reading.
#[cfg(test)]
pub(crate) fn plan_in_place(
    req: &ConvertRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<Planned, Vec<String>>> {
    match scan(req, false, cancel)? {
        Ok(scanned) => preflight(scanned, req, now(), cancel),
        Err(findings) => Ok(Err(findings)),
    }
}

/// Why a check can't plan an image whose undo journal is there: an edit is under way.
fn busy(image: &Path) -> String {
    format!(
        "an edit of {} is under way (its undo journal is beside it); check again once it ends",
        image.display()
    )
}

/// Settles a journal an interrupted edit left beside `req`'s image, as every edit does first
/// (not while ShadowMount+ has it mounted: the check says so), for [`check`]; the error says
/// why it couldn't be. The caller holds the job table, so no job is editing the image.
pub(crate) fn settle(req: &ConvertRequest) -> Option<String> {
    let image = &req.source;
    let journal = ps5_dump_forge_patch::journal_path(image);
    // A NAS or peer path is never edited (`request_findings`), and `smb://` / `flink://` also
    // spell a relative local path: never recover a journal there.
    if crate::net::is_network(image)
        || std::fs::symlink_metadata(&journal).is_err()
        || !matches!(Kind::of(image), Ok(k) if !matches!(k, Kind::Folder | Kind::Archive(_)))
        || smp_finding(image).is_some()
    {
        return None;
    }
    let settled = ps5_dump_forge_patch::recover(image, SYNCS);
    settled.err().map(|e| {
        format!(
            "{}: an earlier edit's undo journal could not be settled: {e}",
            image.display()
        )
    })
}

/// [`crate::Jobs::edit_check`] once [`settle`] ran (`settled`: its error): the in-place job's
/// scan and preflight, and the rebuild's scan, fold, checks and sizing, writing nothing.
pub(crate) fn check(req: &ConvertRequest, settled: Option<String>) -> EditCheck {
    if let Some(why) = settled {
        return EditCheck {
            in_place: vec![why.clone()],
            rebuild: vec![why],
            ..EditCheck::default()
        };
    }
    let cancel = AtomicBool::new(false);
    let flat = |r: anyhow::Result<Result<(), Vec<String>>>| match r {
        Ok(Ok(())) => Vec::new(),
        Ok(Err(findings)) => findings,
        Err(e) => vec![format!("{e:#}")],
    };
    let mut inside = None;
    let in_place = scan_with(req, false, &cancel, &smp_finding, false).and_then(|s| match s {
        Ok(scanned) => {
            preflight_sized(scanned, req, now(), &cancel, &mut inside).map(|p| p.map(drop))
        }
        Err(findings) => Ok(Err(findings)),
    });
    let mut check = EditCheck {
        in_place: flat(in_place),
        needed_inside: inside.map(|i| i.needed),
        free_inside: inside.map(|i| i.free),
        ..EditCheck::default()
    };
    check.rebuild = flat(check_rebuild(req, &cancel, &mut check));
    check
}

/// The rebuild half of [`check`]: what [`crate::convert::convert`]'s rebuild checks before it
/// writes, filling its sizes into `out`.
fn check_rebuild(
    req: &ConvertRequest,
    cancel: &AtomicBool,
    out: &mut EditCheck,
) -> anyhow::Result<Result<(), Vec<String>>> {
    let Scanned {
        kind, image, tree, ..
    } = match scan_with(req, true, cancel, &smp_finding, false)? {
        Ok(scanned) => scanned,
        Err(findings) => return Ok(Err(findings)),
    };
    let Folded {
        mut tree,
        mut findings,
        ..
    } = match fold(&image, tree, req, now(), kind == Kind::Pkg, cancel)? {
        Ok(folded) => folded,
        Err(findings) => return Ok(Err(findings)),
    };
    let (info, input) = preflight::input(tree.as_mut());
    findings.extend(input);
    let at = preflight::output_path(&image)?;
    let dir = at.parent().expect("output_path gives a parent");
    let sized = match kind {
        // As `package::run`: nothing is prepared once preflight has a finding.
        Kind::Pkg if !findings.is_empty() => None,
        Kind::Pkg => match crate::package::size(req, tree.as_mut(), &info, dir, cancel) {
            Ok(size) => Some((size, crate::package::room(size))),
            Err(f) => {
                findings.extend(f);
                None
            }
        },
        _ => {
            findings.extend(preflight::too_deep(tree.as_ref()));
            let size = rebuilt_size(req, kind, &image, tree.as_ref(), &info, cancel)?;
            size.map_err(|f| findings.push(f))
                .ok()
                .map(|size| (size, crate::convert::room(size)))
        }
    };
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        bail!("cancelled");
    }
    if let Some((size, need)) = sized {
        out.rebuild_needs = Some(need);
        let checked = preflight::destination(dir, need, size, cancel);
        out.free_beside = checked.free;
        findings.extend(checked.findings);
    }
    Ok(if findings.is_empty() {
        Ok(())
    } else {
        Err(findings)
    })
}

/// The size of the image a rebuild writes of `tree`: the source's own format, a `.ffpfsc`
/// at its worst case (every block raw) around an inner image of the source's inner format.
fn rebuilt_size(
    req: &ConvertRequest,
    kind: Kind,
    image: &Path,
    tree: &dyn SourceTree,
    info: &preflight::GameInfo,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<u64, String>> {
    if kind != Kind::Ffpfsc {
        return Ok(crate::convert::image_size(
            format_of(kind),
            tree,
            info,
            cancel,
        ));
    }
    if req.ffpfsc_level > 9 {
        return Ok(Err(format!(
            "the .ffpfsc zlib level is 0 through 9, not {}",
            req.ffpfsc_level
        )));
    }
    let inner = crate::convert::image_size(inner_format(image)?, tree, info, cancel);
    Ok(inner.and_then(|size| {
        ps5_dump_forge_pfs::container_size_max(size)
            .map_err(|e| format!("the .ffpfsc container: {e}"))
    }))
}

/// The job's timestamp in whole seconds, for a fresh `ampr_emu.index`.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The undo journal's framing (`ps5_dump_forge_patch`'s layout): its header and trailer, and
/// each entry's offset and length.
const JOURNAL_FRAME: u64 = 30 + 12;
const JOURNAL_ENTRY: u64 = 16;

/// Preflight of a scanned image: the edits folded over its tree ([`Edited`]), the checks a
/// conversion runs on the edited tree, a fresh `ampr_emu.index` when an AMPR title's file set
/// changes, the format editor's plan and room beside the image for the undo journal. `mtime` stamps a fresh index.
pub(crate) fn preflight(
    scanned: Scanned,
    req: &ConvertRequest,
    mtime: i64,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<Planned, Vec<String>>> {
    preflight_sized(scanned, req, mtime, cancel, &mut None)
}

/// The room inside an image an in-place plan counted, in bytes: what it allocates (data,
/// new and grown directories, indirect blocks; what it lacked room for too) and the free
/// space it started from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inside {
    pub needed: u64,
    pub free: u64,
}

/// [`preflight`], filling in `inside` once the format editor planned.
fn preflight_sized(
    scanned: Scanned,
    req: &ConvertRequest,
    mtime: i64,
    cancel: &AtomicBool,
    inside: &mut Option<Inside>,
) -> anyhow::Result<Result<Planned, Vec<String>>> {
    let Scanned {
        kind,
        image,
        stamp,
        tree,
        ..
    } = scanned;
    let Folded {
        tree: mut expected,
        ops,
        hosts,
        mut notes,
        mut findings,
    } = match fold(&image, tree, req, mtime, false, cancel)? {
        Ok(folded) => folded,
        Err(findings) => return Ok(Err(findings)),
    };
    // The checks a conversion runs, on the edited tree: the game root, depth, and the
    // writer's names (bad or colliding names, every offender listed).
    let (info, input) = preflight::input(expected.as_mut());
    findings.extend(input);
    findings.extend(preflight::too_deep(expected.as_ref()));
    let layout = crate::convert::image_size(format_of(kind), expected.as_ref(), &info, cancel);
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        bail!("cancelled");
    }
    findings.extend(layout.err());
    let mut file = File::open(&image).with_context(|| format!("{}", image.display()))?;
    let planned = match kind {
        Kind::Exfat => {
            let (planned, space) = ps5_dump_forge_exfat::edit::plan_sized(&mut file, &ops);
            *inside = space.map(|s| Inside {
                needed: s.needed,
                free: s.free,
            });
            planned
        }
        Kind::Ffpkg => {
            let (planned, space) = ps5_dump_forge_ufs2::edit::plan_sized(&mut file, &ops);
            *inside = space.map(|s| Inside {
                needed: s.needed,
                free: s.free,
            });
            planned
        }
        // PFS keeps no free count, and an in-place `.ffpfs` edit only overwrites runs, so
        // there are no sizes to report (`inside` stays `None`). Its "one run" refusal is
        // given once, not per op.
        Kind::Ffpfs => ps5_dump_forge_pfs::edit::plan(&mut file, &ops).map_err(|mut f| {
            let mut seen = false;
            f.retain(|f| {
                f != ps5_dump_forge_pfs::edit::NO_ROOM || !std::mem::replace(&mut seen, true)
            });
            f
        }),
        other => unreachable!("{other:?} is not edited in place"),
    };
    let writes = planned.unwrap_or_else(|f| {
        findings.extend(f);
        Vec::new()
    });
    let live = writes.iter().filter(|w| w.live && !w.data.is_empty());
    let journal_bytes = live.fold(JOURNAL_FRAME, |sum, w| {
        sum.saturating_add(JOURNAL_ENTRY + w.data.len())
    });
    let at = preflight::output_path(&image)?;
    let dir = at.parent().expect("output_path gives a parent");
    let need = journal_bytes.saturating_add(JOURNAL_MARGIN);
    let checked = preflight::destination(dir, need, journal_bytes, cancel);
    findings.extend(
        checked
            .findings
            .into_iter()
            .map(|f| format!("undo journal: {f}")),
    );
    notes.extend(checked.notes);
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    Ok(Ok(Planned {
        kind,
        image,
        stamp,
        ops,
        writes,
        hosts,
        expected,
        journal_bytes,
        notes,
    }))
}

/// The edits folded over a scanned image's tree, what an in-place edit and a rebuild both
/// write.
pub(crate) struct Folded {
    /// The edited tree ([`Edited`], with the fresh `ampr_emu.index` when there is one).
    pub tree: Box<dyn SourceTree>,
    /// The format editor's ops, the fresh index last when there is one.
    pub ops: Vec<Op>,
    /// The host files `Data::Source` ids name (by index).
    pub hosts: Vec<HostFile>,
    pub notes: Vec<String>,
    /// What the edited tree can't be written as (the edits themselves applied).
    pub findings: Vec<String>,
}

/// Folds `req`'s edits over `tree`, the scanned `image`: the edits must apply ([`Edited`];
/// the inner `Err` lists every offender), an add of the image itself is a finding, and an AMPR
/// title whose file set changes gets a fresh `ampr_emu.index` ([`fresh_index`]; the journal
/// and logs then go with the old one, the runtime is never swapped). An LZ4-packed game is
/// edited too, but its pack files and `ampr_emu.index` stay byte for byte and nothing may
/// appear at or under a packed path ([`Packs`]; the inner `Err` lists each offender); it gets
/// no fresh index, and a `.pkg` rebuild of it leaves nothing out (the builder keeps a packed
/// source's own index). `mtime` stamps a fresh index.
pub(crate) fn fold(
    image: &Path,
    mut tree: Box<dyn SourceTree>,
    req: &ConvertRequest,
    mtime: i64,
    pkg: bool,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<Folded, Vec<String>>> {
    let packs = if reader::detect(tree.as_mut()).context("looking for LZ4 packs")? {
        Some(Packs::of(tree.as_mut())?)
    } else if crate::zstd_pack::packed(tree.as_mut()) {
        Some(Packs::of_zstd(tree.as_mut())?)
    } else {
        None
    };
    let packs = match packs {
        Some(Err(findings)) => return Ok(Err(findings)),
        Some(Ok(packs)) => Some(packs),
        None => None,
    };
    let had_index = tree.files().iter().any(|f| f.path == INDEX);
    let before = sizes(&crate::lz4::plain_listing(
        tree.files(),
        RuntimeAction::Keep,
    ));
    let mut edited = match Edited::new(tree, &req.edits, None, cancel)? {
        Ok(edited) => edited,
        Err(findings) => return Ok(Err(findings)),
    };
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    if let Some(packs) = &packs {
        let locked = packs.findings(&edited);
        if !locked.is_empty() {
            return Ok(Err(locked));
        }
    }
    let mut ops = edited.ops().to_vec();
    let hosts = edited.hosts().to_vec();
    // An add of the image itself (or of a folder holding it) would read it while it changes.
    if let Ok(me) = crate::finalize::followed_id(image) {
        findings.extend(
            (hosts.iter().filter(|h| h.id == Some(me)))
                .map(|h| format!("{} is the image being edited", h.path.display())),
        );
    }
    let index = if let Some(packs) = &packs {
        // The runtime matches the index's records to the manifest's by file id: both stay.
        // The runtime, journal and logs aren't looked up through the index.
        let listing = crate::lz4::plain_listing(edited.files(), RuntimeAction::Keep);
        let own = |p: &str| p == RUNTIME || p == JOURNAL || LOGS.contains(&p);
        let after = sizes(&listing);
        if after.symmetric_difference(&before).any(|(p, _)| !own(p)) {
            notes.push(format!(
                "{0}: the game is packed; its packs and {INDEX} stay as they are, so a file \
                 added, resized or removed here keeps its old entry there (ampr_emu answers \
                 /app0 lookups from {INDEX}: a file it doesn't list is missing to the game)",
                packs.kind
            ));
        }
        None
    } else {
        fresh_index(
            &mut edited,
            had_index,
            &before,
            mtime,
            &mut notes,
            &mut findings,
        )?
    };
    let tree: Box<dyn SourceTree> = match index {
        Some(bytes) => {
            // The journal's file ids, and the logs, follow the old index's record order: a
            // fresh index takes them away, as a conversion does.
            let traces = |p: &str| p == JOURNAL || LOGS.contains(&p);
            let gone: Vec<String> = (edited.files().iter())
                .filter(|f| traces(&f.path))
                .map(|f| f.path.clone())
                .collect();
            if !gone.is_empty() {
                notes.push(format!(
                    "LZ4: removing {} with the old {INDEX} (their file ids follow its order)",
                    gone.join(", ")
                ));
            }
            ops.extend(gone.into_iter().map(|path| Op::Delete { path }));
            ops.push(Op::Put {
                path: INDEX.into(),
                len: bytes.len() as u64,
                data: Data::Bytes(bytes.clone()),
            });
            let mut files = edited.files().to_vec();
            files.retain(|f| f.path != INDEX && !traces(&f.path));
            let at = files.partition_point(|f| f.path.as_str() < INDEX);
            let size = bytes.len() as u64;
            files.insert(
                at,
                SourceFile {
                    path: INDEX.into(),
                    size,
                },
            );
            let generated = HashMap::from([(INDEX.to_string(), Cow::Owned(bytes))]);
            crate::lz4::overlay(Box::new(edited), files, generated)
        }
        None => Box::new(edited),
    };
    // Into a `.pkg` (a rebuild of one), an AMPR title's journal and logs always go, and its
    // old index: the builder writes a fresh one from what it packages, as for a conversion.
    // A packed game keeps all three: the builder keeps a packed source's own index.
    let tree = if pkg && packs.is_none() {
        for_pkg(tree, had_index, &mut notes)
    } else {
        tree
    };
    Ok(Ok(Folded {
        tree,
        ops,
        hosts,
        notes,
        findings,
    }))
}

/// [`fold`]'s edited tree as a `.pkg` takes it: for an AMPR title (`eboot.bin` imports
/// libSceAmpr, or the image held an index) what [`crate::lz4::leave_out_for_pkg`] leaves out,
/// each noted.
fn for_pkg(
    mut tree: Box<dyn SourceTree>,
    had_index: bool,
    notes: &mut Vec<String>,
) -> Box<dyn SourceTree> {
    let ampr = crate::lz4::imports_ampr(tree.as_mut());
    if !(ampr || had_index) {
        return tree;
    }
    let mut files = tree.files().to_vec();
    let before = files.len();
    for path in crate::lz4::leave_out_for_pkg(&mut files, ampr) {
        notes.push(format!("LZ4: leaving out {path}"));
    }
    if files.len() == before {
        return tree;
    }
    crate::lz4::overlay(tree, files, HashMap::new())
}

/// What an edit of an LZ4-packed game must leave as it is. Only the packs themselves are
/// locked: the runtime, the journal, logs, `eboot.bin` and every loose file may change (the
/// runtime reads a loose file from the disk as it is).
struct Packs {
    /// `LZ4` or `zstd`, for the findings.
    kind: &'static str,
    /// The manifest, its two sidecars, every volume it names and `ampr_emu.index` (its records
    /// match the manifest's by file id), each with its size in the source (`None`: absent).
    locked: Vec<(String, Option<u64>)>,
    /// Every packed record's path, ASCII lower-cased: the packs' own namespace.
    packed: HashSet<String>,
    /// Every folder above a packed path, ASCII lower-cased.
    dirs: HashSet<String>,
}

/// The folders above `path`, outermost first.
fn parents(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(move |(i, _)| &path[..i])
}

impl Packs {
    /// The packs of `tree`, which [`reader::detect`] found packed; a manifest that can't be
    /// read is a finding.
    fn of(tree: &mut dyn SourceTree) -> anyhow::Result<Result<Packs, Vec<String>>> {
        let size_of = |tree: &dyn SourceTree, path: &str| {
            (tree.files().iter())
                .find(|f| f.path == path)
                .map(|f| f.size)
        };
        let size = size_of(tree, MANIFEST).unwrap_or(0);
        if size > reader::MAX_MANIFEST_BYTES {
            return Ok(Err(vec![format!(
                "{MANIFEST} is {size} bytes, over {}",
                reader::MAX_MANIFEST_BYTES
            )]));
        }
        let bytes = (tree.read_range(MANIFEST, 0, size as usize))
            .with_context(|| format!("reading {MANIFEST}"))?;
        let manifest = match reader::open_manifest(&bytes) {
            Ok(manifest) => manifest,
            Err(e) => return Ok(Err(vec![format!("{MANIFEST} can't be read: {e}")])),
        };
        let names = [MANIFEST, CRC_SIDECAR, PROFILE, INDEX].map(String::from);
        let locked = (names.into_iter())
            .chain(manifest.packs.iter().map(|p| p.name.clone()))
            .map(|path| {
                let size = size_of(tree, &path);
                (path, size)
            })
            .collect();
        let packed: HashSet<String> = (manifest.files.iter())
            .filter(|f| f.packed())
            .map(|f| f.path.to_ascii_lowercase())
            .collect();
        let dirs = (packed.iter())
            .flat_map(|p| parents(p).map(String::from))
            .collect();
        Ok(Ok(Packs {
            kind: "LZ4",
            locked,
            packed,
            dirs,
        }))
    }

    /// The zstd packs of `tree`, which holds a root `ampr_zstd.index`: the index, every volume it
    /// names and `ampr_emu.index` (it lists the packed files) are locked. An index that can't
    /// be read is a finding.
    fn of_zstd(tree: &mut dyn SourceTree) -> anyhow::Result<Result<Packs, Vec<String>>> {
        use crate::zstd_pack::PACK_INDEX;
        // ponytail: the index is read whole; 256 MiB is about 8 million chunk records.
        const MAX_INDEX: u64 = 256 << 20;
        let size_of = |tree: &dyn SourceTree, path: &str| {
            (tree.files().iter())
                .find(|f| f.path == path)
                .map(|f| f.size)
        };
        let size = size_of(tree, PACK_INDEX).unwrap_or(0);
        if size > MAX_INDEX {
            return Ok(Err(vec![format!(
                "{PACK_INDEX} is {size} bytes, over {MAX_INDEX}"
            )]));
        }
        let bytes = (tree.read_range(PACK_INDEX, 0, size as usize))
            .with_context(|| format!("reading {PACK_INDEX}"))?;
        let index = match ps5_dump_forge_zstd::decode_index(&bytes) {
            Ok(index) => index,
            Err(e) => return Ok(Err(vec![format!("{PACK_INDEX} can't be read: {e}")])),
        };
        let volumes = (0..index.volume_sizes.len() as u32).map(ps5_dump_forge_zstd::volume_name);
        let locked = ([PACK_INDEX, INDEX].map(String::from).into_iter())
            .chain(volumes)
            .map(|path| {
                let size = size_of(tree, &path);
                (path, size)
            })
            .collect();
        let packed: HashSet<String> = (index.files.iter())
            .map(|f| f.path.to_ascii_lowercase())
            .collect();
        let dirs = (packed.iter())
            .flat_map(|p| parents(p).map(String::from))
            .collect();
        Ok(Ok(Packs {
            kind: "zstd",
            locked,
            packed,
            dirs,
        }))
    }

    /// What the edits do to the packs, one line per offender: an op naming a locked file, a
    /// locked file gone or resized (a folder deleted or renamed with it), and a file or empty
    /// folder at or under a packed path (or a file at a folder of one).
    fn findings(&self, edited: &Edited) -> Vec<String> {
        let locked = |p: &str| self.locked.iter().any(|(l, _)| l == p);
        let mut changed = BTreeSet::new();
        for op in edited.ops() {
            let named: &[&String] = match op {
                Op::Delete { path } | Op::Mkdir { path } | Op::Put { path, .. } => &[path],
                Op::Rename { from, to } => &[from, to],
            };
            changed.extend(named.iter().filter(|p| locked(p)).map(|p| p.to_string()));
        }
        let now: HashMap<&str, u64> = (edited.files().iter())
            .map(|f| (f.path.as_str(), f.size))
            .collect();
        changed.extend(
            (self.locked.iter())
                .filter(|(path, size)| now.get(path.as_str()).copied() != *size)
                .map(|(path, _)| path.clone()),
        );
        let mut out: Vec<String> = (changed.into_iter())
            .map(|p| {
                format!(
                    "{p} is part of the {} packs: edits can't change it",
                    self.kind
                )
            })
            .collect();
        let files = edited.files().iter().map(|f| (&f.path, true));
        for (path, file) in files.chain(edited.empty_dirs().iter().map(|d| (d, false))) {
            let key = path.to_ascii_lowercase();
            if self.packed.contains(&key) {
                out.push(format!(
                    "{path} is packed in the {} packs: edits can't add a file there",
                    self.kind
                ));
            } else if let Some(at) = parents(&key).find(|d| self.packed.contains(*d)) {
                out.push(format!(
                    "{path} is under {}, a file packed in the {} packs: edits can't add \
                     anything there",
                    &path[..at.len()],
                    self.kind
                ));
            } else if file && self.dirs.contains(&key) {
                out.push(format!(
                    "{path} is a folder in the {} packs: edits can't add a file there",
                    self.kind
                ));
            }
        }
        out
    }
}

/// Every (path, size) of a listing.
fn sizes(files: &[SourceFile]) -> BTreeSet<(String, u64)> {
    files.iter().map(|f| (f.path.clone(), f.size)).collect()
}

/// A fresh `ampr_emu.index` for an AMPR title (its `eboot.bin` imports libSceAmpr, or the
/// image held an index) whose file set or a size the edits change (`before`: the image's,
/// as [`crate::lz4::plain_listing`] lists it), unless its index already lists exactly the
/// edited files; and always when an op touches the index itself (deletes, replaces or renames
/// it: a replacement of the user's own is overwritten too). [`fold`] then removes the journal
/// and logs; the runtime is never swapped.
fn fresh_index(
    edited: &mut Edited,
    had_index: bool,
    before: &BTreeSet<(String, u64)>,
    mtime: i64,
    notes: &mut Vec<String>,
    findings: &mut Vec<String>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let listing = crate::lz4::plain_listing(edited.files(), RuntimeAction::Keep);
    let touched = edited.ops().iter().any(|op| match op {
        Op::Delete { path } | Op::Mkdir { path } | Op::Put { path, .. } => path == INDEX,
        Op::Rename { from, to } => from == INDEX || to == INDEX,
    });
    if !(touched || sizes(&listing) != *before) || !(had_index || crate::lz4::imports_ampr(edited))
    {
        return Ok(None);
    }
    if edited.files().iter().any(|f| f.path == INDEX) && touched {
        notes.push(format!(
            "LZ4: the edits change {INDEX}; a fresh one listing the edited files replaces it \
             (a stale index crashes the game)"
        ));
    }
    if !touched && let IndexState::Ok(_) = crate::lz4::index_state(edited, &listing)? {
        notes.push(format!(
            "LZ4: keeping {INDEX}, it lists exactly the edited files"
        ));
        return Ok(None);
    }
    let rows: Vec<(String, u64)> = listing.into_iter().map(|f| (f.path, f.size)).collect();
    match ps5upload_fpkg::ampr_index::build(&rows, mtime) {
        Ok(index) => {
            notes.push(format!(
                "LZ4: writing a new {INDEX} for {} files (the edits change them)",
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

/// The edit job: scan, preflight, then [`apply`] and its check.
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    // A game folder is changed directly (`edit_folder`).
    // A NAS or peer source goes to `scan`'s refusal unasked.
    if !crate::net::is_network(&req.source) && Kind::of(&req.source)? == Kind::Folder {
        return crate::edit_folder::run(req, ctx);
    }
    let planned = plan_job(req, ctx)?;
    let hooks = Hooks {
        sync: SYNCS,
        keep: Applied::commit,
        check: &|_| Ok(()),
    };
    apply(planned, req.full_verify, ctx, &hooks)
}

/// What [`apply`] calls that a test replaces: the syncs, the journal's removal at commit
/// ([`Applied::commit`], given syncs that leave the folder to [`Pending::commit`]) and a
/// check after the verification.
struct Hooks<'a> {
    sync: Syncs,
    keep: fn(Applied, Syncs) -> io::Result<()>,
    check: &'a dyn Fn(&Path) -> anyhow::Result<()>,
}

/// Says what [`scan`]'s recovery settled, if anything.
pub(crate) fn log_recovery(recovery: Recovery, ctx: &Ctx) {
    match recovery {
        Recovery::Clean => {}
        Recovery::Restored => ctx.log(
            "an earlier edit of this image was interrupted: its undo journal restored the image",
        ),
        Recovery::Discarded => ctx.log(
            "an earlier edit of this image was interrupted before it changed anything: its \
             incomplete undo journal was deleted",
        ),
    }
}

/// The scan and preflight stages, logged.
fn plan_job(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<Planned> {
    ctx.stages(&STAGES);
    ctx.progress("scan", 0, 1);
    let failed =
        |findings: Vec<String>| anyhow::anyhow!("preflight failed:\n  {}", findings.join("\n  "));
    let scanned = scan(req, false, ctx.cancel)?.map_err(failed)?;
    log_recovery(scanned.recovery, ctx);
    ctx.log(format!("source: {}", scanned.tree.describe()));
    ctx.progress("scan", 1, 1);
    ctx.check()?;
    ctx.progress("preflight", 0, 1);
    let planned = preflight(scanned, req, now(), ctx.cancel)?.map_err(failed)?;
    for note in &planned.notes {
        ctx.log(note.clone());
    }
    let live = planned.writes.iter().filter(|w| w.live).count();
    ctx.log(format!(
        "edit: {} changes, {} writes ({live} over live metadata, a {} byte undo journal)",
        planned.ops.len(),
        planned.writes.len(),
        planned.journal_bytes
    ));
    ctx.progress("preflight", 1, 1);
    Ok(planned)
}

/// What the job says when an applied edit is undone because of `e`.
const NOT_VERIFIED: &str = "the edit did not verify; the image was restored";

/// Writes the plan into the image and checks it. The expected tree is hashed first, from
/// the image as it is and the host files (what `full` or a fast sample compares), so a
/// write that damaged an untouched file is caught. Then the stamp and ShadowMount+ are
/// checked again and `patch::apply` writes, the image is reopened by its reader and compared
/// (plus `extra`, a test's own check), and the journal is committed. Any failure after the
/// apply rolls the image back.
fn apply(mut planned: Planned, full: bool, ctx: &Ctx, hooks: &Hooks) -> anyhow::Result<JobReport> {
    let sync = hooks.sync;
    let image = planned.image.clone();
    // The UFS2 indirect-block seam is sampled in a `.ffpkg`, as a conversion's.
    let mode = if full {
        Mode::Full
    } else {
        Mode::Fast {
            seed: verify::fresh_seed()?,
            seam: planned.kind == Kind::Ffpkg,
        }
    };
    let checked = verify::summary(planned.expected.files(), mode).checked_bytes;
    let written = planned.writes.iter().map(|w| w.data.len()).sum::<u64>();
    // Hashing, writing, reading back.
    ctx.expect_rest(checked.saturating_mul(2).saturating_add(written));
    ctx.progress("write", 0, 1);
    let expected = verify::expected_sampled(planned.expected.as_mut(), ctx, mode, "write")?;
    // The image and the hosts are read again by the apply; the scan's handle isn't needed.
    drop(planned.expected);
    ctx.check()?;
    let changed = || {
        anyhow::anyhow!(
            "{} changed after the edit was planned; nothing was written",
            image.display()
        )
    };
    if !planned.stamp.unchanged() {
        return Err(changed());
    }
    if let Some(mounted) = smp_finding(&image) {
        bail!("{mounted}");
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&image)
        .with_context(|| format!("opening {} to edit it", image.display()))?;
    if handle_id(&file)? != followed_id(&image)? {
        bail!("{} was replaced while it was opened", image.display());
    }
    if !planned.stamp.unchanged() {
        return Err(changed());
    }
    let mut hosts = Hosts::new(&planned.hosts);
    let applied = ps5_dump_forge_patch::apply(
        &mut file,
        &image,
        &planned.writes,
        &mut |id, offset, buf| hosts.read(id, offset, buf),
        ctx.cancel,
        sync,
    )
    .with_context(|| format!("editing {}", image.display()))?;
    drop(hosts);
    drop(file);
    // From here every way out but a commit restores the image, a panic included.
    let pending = Pending {
        applied: Some(applied),
        sync,
        keep: hooks.keep,
    };
    ctx.progress("write", written, written.max(2));
    let checked =
        check_edit(&image, planned.kind, &expected, ctx, mode, hooks.check).and_then(|c| {
            ctx.progress("finalize", 0, 1);
            // A cancel seen during the last read or the finalize event still undoes the edit.
            ctx.check()?;
            Ok(c)
        });
    let mut checks = match checked {
        Ok(checks) => checks,
        Err(e) => {
            let what = if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                "cancelled; the image was restored"
            } else {
                NOT_VERIFIED
            };
            return Err(match pending.rollback() {
                Ok(()) => anyhow::anyhow!("{what}:\n{e:#}"),
                Err(r) => anyhow::anyhow!(
                    "the edit was not kept ({e:#}), and restoring the image failed ({r}): its \
                     undo journal {} stays, and the next edit of the image restores it",
                    ps5_dump_forge_patch::journal_path(&image).display()
                ),
            });
        }
    };
    pending.commit(&image)?;
    checks.push(format!("edited {} in place", image.display()));
    ctx.progress("finalize", 1, 1);
    Ok(JobReport {
        output: image.clone(),
        bytes: std::fs::metadata(&image).map_or(0, |m| m.len()),
        files: expected.files.len() as u64,
        checks,
        verify: verify::summary(&expected.files, mode),
    })
}

/// An applied edit not yet kept: dropped without [`Pending::commit`] (an error or a panic on
/// the way) it rolls the image back, so the image is restored before the job ends and its
/// paths are released. (On the PS5 a panic aborts; the next edit's `recover` restores it.)
struct Pending {
    applied: Option<Applied>,
    sync: Syncs,
    keep: fn(Applied, Syncs) -> io::Result<()>,
}

impl Pending {
    fn rollback(mut self) -> io::Result<()> {
        let applied = self.applied.take().expect("taken only once");
        applied.rollback(self.sync)
    }

    /// Keeps the edit: the undo journal is deleted, then its folder synced. A journal that
    /// can't be deleted is still complete, so the image is restored from it; a failed sync
    /// after the delete can't be undone, and says the image holds the edit.
    fn commit(mut self, image: &Path) -> anyhow::Result<()> {
        let applied = self.applied.take().expect("taken only once");
        let sync = self.sync;
        // The folder is synced here, so its failure is told apart from the delete's.
        let no_dir = Syncs {
            file: sync.file,
            dir: |_| Ok(()),
        };
        if let Err(e) = (self.keep)(applied, no_dir) {
            return Err(match ps5_dump_forge_patch::recover(image, sync) {
                Ok(_) => anyhow::anyhow!("{e}; the image was restored"),
                Err(r) => anyhow::anyhow!(
                    "{e}, and restoring the image failed ({r}): its undo journal stays, and \
                     the next edit of the image restores it"
                ),
            });
        }
        let folder = folder_of(image);
        (sync.dir)(folder).map_err(|e| {
            anyhow::anyhow!(
                "the edit was applied and verified, but syncing {} failed: {e}; the image \
                 holds the edit",
                folder.display()
            )
        })
    }
}

/// The folder holding `image` (and its journal); `.` for a bare file name, whose parent is
/// empty (as the patch crate's own folder sync takes it).
fn folder_of(image: &Path) -> &Path {
    match image.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(applied) = self.applied.take() {
            let _ = applied.rollback(self.sync);
        }
    }
}

/// The host files an apply copies ([`Data::Source`] ids index `files`), each checked to be the
/// file the edit was planned with (identity, size, modification time) before it is read.
struct Hosts<'a> {
    files: &'a [HostFile],
    open: Option<(u32, Opened)>,
}

impl<'a> Hosts<'a> {
    fn new(files: &'a [HostFile]) -> Self {
        Self { files, open: None }
    }

    fn read(&mut self, id: u32, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let host = (self.files.get(id as usize))
            .ok_or_else(|| io::Error::other(format!("no host file {id}")))?;
        let with_path =
            |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", host.path.display()));
        if offset.saturating_add(buf.len() as u64) > host.size {
            return Err(io::Error::other(format!(
                "{}: a read past its {} bytes",
                host.path.display(),
                host.size
            )));
        }
        if self.open.as_ref().is_none_or(|(at, _)| *at != id) {
            self.open = None;
            self.open = Some((id, host.open().map_err(with_path)?));
        }
        let file = &mut self.open.as_mut().expect("opened above").1;
        if !file.same(host).map_err(with_path)? {
            return Err(io::Error::other(format!(
                "{} changed after it was added (replaced, or its size or modification time); \
                 add it again",
                host.path.display()
            )));
        }
        file.seek(SeekFrom::Start(offset)).map_err(with_path)?;
        file.read_exact(buf).map_err(with_path)
    }
}

/// The edited image, reopened by its reader, against `expected`.
fn check_edit(
    image: &Path,
    kind: Kind,
    expected: &verify::Expected,
    ctx: &Ctx,
    mode: Mode,
    extra: &dyn Fn(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<String>> {
    ctx.progress("verify", 0, 1);
    let mut tree = open_source(image, kind, ctx.cancel)?;
    let checks = verify::compare(expected, tree.as_mut(), ctx, mode)?;
    drop(tree);
    extra(image)?;
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Edit;
    use crate::scan::ScannedFolder;

    const PARAM: &[u8] =
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#;
    pub(super) const AMPR_EBOOT: &[u8] = b"\x7fELF fake eboot importing libSceAmpr.sprx";

    pub(super) fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A small game folder in `dir`/`name`-src, written straight into image `dir`/`name` by
    /// the writer its extension names (no conversion: nothing repaired or added).
    pub(super) fn image(dir: &Path, name: &str, eboot: &[u8], extra: &[(&str, &[u8])]) -> PathBuf {
        let src = dir.join(format!("{name}-src"));
        put(&src, "eboot.bin", eboot);
        put(&src, "sce_sys/param.json", PARAM);
        put(&src, "data/a.bin", &[7u8; 70_000]);
        put(&src, "data/sub/b.bin", b"bee");
        std::fs::create_dir_all(src.join("data/empty")).unwrap();
        for (rel, bytes) in extra {
            put(&src, rel, bytes);
        }
        let cancel = AtomicBool::new(false);
        let mut tree = ScannedFolder::scan(&src, &cancel).unwrap();
        let out = dir.join(name);
        let mut file = File::create(&out).unwrap();
        let progress = &mut |_, _| {};
        if name.ends_with(".exfat") {
            let layout = ps5_dump_forge_exfat::plan(&tree, &Default::default(), &cancel).unwrap();
            ps5_dump_forge_exfat::write(&mut tree, &layout, &mut file, &cancel, progress).unwrap();
        } else if name.ends_with(".ffpfs") {
            let layout = ps5_dump_forge_pfs::plan(&tree, &Default::default(), &cancel).unwrap();
            ps5_dump_forge_pfs::write(&mut tree, &layout, &mut file, &cancel, progress).unwrap();
        } else {
            let layout = ps5_dump_forge_ufs2::plan(&tree, &Default::default(), &cancel).unwrap();
            ps5_dump_forge_ufs2::write(&mut tree, &layout, &mut file, &cancel, progress).unwrap();
        }
        out
    }

    pub(super) fn request(source: &Path, edits: Vec<Edit>) -> ConvertRequest {
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": source, "format": "exfat", "output": source
        }))
        .unwrap();
        req.edits = edits;
        req
    }

    pub(super) fn plan(req: &ConvertRequest) -> Result<Planned, Vec<String>> {
        plan_in_place(req, &AtomicBool::new(false)).unwrap()
    }

    fn findings(req: &ConvertRequest) -> Vec<String> {
        plan(req).err().expect("refused")
    }

    fn mkdir(path: &str) -> Edit {
        Edit::Mkdir { path: path.into() }
    }

    #[test]
    fn refusals_per_kind() {
        let dir = crate::test_dir("edit-kinds");
        let cases = [
            ("game.zip", "archives can be browsed but not edited"),
            ("game.7z", "archives can be browsed but not edited"),
            ("game.ffpfsc", "a .ffpfsc can only be rebuilt"),
            ("game.pkg", "a .pkg can only be rebuilt"),
        ];
        for (name, want) in cases {
            let path = dir.join(name);
            std::fs::write(&path, b"not read").unwrap();
            let got = findings(&request(&path, vec![mkdir("x")]));
            assert!(
                got.len() == 1 && got[0].starts_with(want),
                "{name}: {got:?}"
            );
        }
        let folder = dir.join("folder");
        std::fs::create_dir(&folder).unwrap();
        let got = findings(&request(&folder, vec![mkdir("x")]));
        assert_eq!(got, ["editing a folder is not supported yet"]);
        // The request's own refusals come with the kind's.
        let mut req = request(&dir.join("game.pkg"), vec![mkdir("x")]);
        req.lz4 = Some(crate::Lz4Mode::Unpack);
        req.remove_backport = true;
        let got = findings(&req);
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got[0].contains("no LZ4 option (lz4)"), "{got:?}");
        assert!(got[1].contains("remove_backport"), "{got:?}");
        assert!(got[2].contains("can only be rebuilt"), "{got:?}");
    }

    fn paths(tree: &dyn SourceTree) -> Vec<String> {
        tree.files().iter().map(|f| f.path.clone()).collect()
    }

    /// The fresh index Put, if the plan has one (always last).
    fn index_put(planned: &Planned) -> Option<&[u8]> {
        match planned.ops.last() {
            Some(Op::Put {
                path,
                data: Data::Bytes(bytes),
                len,
            }) if path == INDEX => {
                assert_eq!(*len, bytes.len() as u64);
                Some(bytes)
            }
            _ => None,
        }
    }

    #[test]
    fn an_ampr_title_gets_a_fresh_index_when_its_files_change() {
        let dir = crate::test_dir("edit-ampr");
        let journal = (ps5_dump_forge_lz4::JOURNAL, &b"journal"[..]);
        let log = ("ampr_emu.log", &b"log"[..]);
        let img = image(&dir, "game.ffpkg", AMPR_EBOOT, &[journal, log]);
        let host = dir.join("added.bin");
        std::fs::write(&host, b"added").unwrap();
        let add = Edit::Add {
            from: host,
            to: "data/added.bin".into(),
        };
        let planned = plan(&request(&img, vec![add])).unwrap();
        let index = index_put(&planned).expect("a fresh index").to_vec();
        let records = ps5_dump_forge_lz4::index::read_index(&index)
            .unwrap()
            .records;
        let listed: Vec<&str> = records.iter().map(|(p, _)| p.as_str()).collect();
        // The journal and logs go with the old index, as a conversion leaves them out.
        let want = ["data/a.bin", "data/added.bin", "data/sub/b.bin"];
        assert_eq!(
            listed,
            [&want[..], &["eboot.bin", "sce_sys/param.json"]].concat()
        );
        let mut expected = planned.expected;
        let files = paths(expected.as_ref());
        assert!(
            files.contains(&INDEX.to_string()) && files.is_sorted(),
            "{files:?}"
        );
        assert!(!files.contains(&ps5_dump_forge_lz4::JOURNAL.to_string()));
        assert!(!files.contains(&"ampr_emu.log".to_string()));
        let n = planned.ops.len();
        assert_eq!(
            planned.ops[n - 3..n - 1],
            [
                Op::Delete {
                    path: ps5_dump_forge_lz4::JOURNAL.into()
                },
                Op::Delete {
                    path: "ampr_emu.log".into()
                },
            ]
        );
        assert!(
            planned
                .notes
                .iter()
                .any(|n| n.contains("removing ampr_commands.bin"))
        );
        assert_eq!(expected.read(INDEX).unwrap(), index);
        // A new folder changes no file: no index, the journal and logs stay.
        let planned = plan(&request(&img, vec![mkdir("more")])).unwrap();
        assert!(index_put(&planned).is_none());
        assert_eq!(
            planned.ops,
            [Op::Mkdir {
                path: "more".into()
            }]
        );
        let files = paths(planned.expected.as_ref());
        assert!(files.contains(&ps5_dump_forge_lz4::JOURNAL.to_string()));
        // Nor does any change of a title that is not AMPR.
        let plain = image(&dir, "plain.exfat", b"\x7fELF plain", &[]);
        let gone = Edit::Delete {
            path: "data/a.bin".into(),
        };
        assert!(index_put(&plan(&request(&plain, vec![gone])).unwrap()).is_none());
    }

    #[test]
    fn nothing_inside_smp_mount_folder_is_edited() {
        for path in [
            "/mnt/shadowmnt",
            "/mnt/shadowmnt/My Game_0a1b2c3d",
            "/mnt/shadowmnt/pfsc/My Game_0a1b2c3d/data/x.ffpkg",
        ] {
            let got = smp_inside(Path::new(path)).unwrap().replace('\\', "/");
            assert!(
                got.starts_with(&format!("{path} is inside ShadowMount+'s")),
                "{got}"
            );
        }
        for path in ["/mnt", "/mnt/shadowmnt2/x", "/data/games/My Game"] {
            assert_eq!(smp_inside(Path::new(path)), None, "{path}");
        }
    }

    #[test]
    fn smp_mount_points_are_found_by_name_and_device() {
        let image = Path::new("/data/games/My Game.v1.ffpkg");
        let listing = |names: &'static [&'static str]| {
            move |dir: &Path| -> io::Result<Vec<String>> {
                match dir.to_str() {
                    Some("/mnt/shadowmnt") => Ok(names.iter().map(|n| n.to_string()).collect()),
                    _ => Err(io::ErrorKind::NotFound.into()),
                }
            }
        };
        let mounts = &mut |at: &Path| -> io::Result<bool> {
            Ok(at.ends_with("My Game.v1_0a1B2c3D") || at.ends_with("My Game.v1_deadbeef"))
        };
        // Not ours: another stem, wrong hex length or digits, a folder that is no mount point.
        let others: &[&str] = &[
            "My Game_0a1b2c3d",
            "My Game.v1_0a1b2c3",
            "My Game.v1_0a1b2c3g",
            "My Game.v1_12345678",
        ];
        assert_eq!(smp_mount(image, &mut listing(others), mounts), None);
        let got = smp_mount(
            image,
            &mut listing(&["x_00000000", "My Game.v1_0a1B2c3D"]),
            mounts,
        )
        .unwrap()
        // Joined with the OS separator: `\` on Windows.
        .replace('\\', "/");
        assert!(got.starts_with("ShadowMount+ has this image mounted at /mnt/shadowmnt/My Game.v1_0a1B2c3D: stop the game"), "{got}");
        // The pfsc folder too.
        let pfsc = &mut |dir: &Path| -> io::Result<Vec<String>> {
            match dir.to_str() {
                Some("/mnt/shadowmnt") => Ok(Vec::new()),
                _ => Ok(vec!["My Game.v1_deadbeef".into()]),
            }
        };
        let got = smp_mount(image, pfsc, mounts).unwrap().replace('\\', "/");
        assert!(
            got.contains("/mnt/shadowmnt/pfsc/My Game.v1_deadbeef"),
            "{got}"
        );
        // Fails closed: a listing or stat error other than not-found.
        let denied = &mut |_: &Path| -> io::Result<Vec<String>> {
            Err(io::ErrorKind::PermissionDenied.into())
        };
        let got = smp_mount(image, denied, mounts).unwrap();
        assert!(
            got.starts_with("can't tell whether ShadowMount+ has this image mounted"),
            "{got}"
        );
        let stat =
            &mut |_: &Path| -> io::Result<bool> { Err(io::ErrorKind::PermissionDenied.into()) };
        assert!(smp_mount(image, &mut listing(&["My Game.v1_00000000"]), stat).is_some());
        let gone = &mut |_: &Path| -> io::Result<bool> { Err(io::ErrorKind::NotFound.into()) };
        assert_eq!(
            smp_mount(image, &mut listing(&["My Game.v1_00000000"]), gone),
            None
        );
    }

    /// An undo journal (`ps5_dump_forge_patch`'s layout) that puts `old` back at `offset`.
    fn journal(image: &Path, offset: u64, old: &[u8]) {
        let mut j = b"FORGEUNDO1".to_vec();
        j.extend(std::fs::metadata(image).unwrap().len().to_le_bytes());
        let total = JOURNAL_FRAME + JOURNAL_ENTRY + old.len() as u64;
        j.extend(total.to_le_bytes());
        j.extend(1u32.to_le_bytes());
        j.extend(offset.to_le_bytes());
        j.extend((old.len() as u64).to_le_bytes());
        j.extend(old);
        j.extend(b"FORGEEND");
        j.extend(crc32fast::hash(&j).to_le_bytes());
        std::fs::write(ps5_dump_forge_patch::journal_path(image), j).unwrap();
    }

    #[test]
    fn a_leftover_journal_is_restored_only_once_the_image_is_not_mounted() {
        let dir = crate::test_dir("edit-recover");
        let img = image(&dir, "game.exfat", b"\x7fELF plain", &[]);
        let original = std::fs::read(&img).unwrap();
        // As if an edit died part way through its live writes: the boot sector is gone.
        let mut damaged = original.clone();
        damaged[..512].fill(0);
        std::fs::write(&img, &damaged).unwrap();
        journal(&img, 0, &original[..512]);
        let at = ps5_dump_forge_patch::journal_path(&img);
        let req = request(&img, vec![mkdir("more")]);
        let cancel = AtomicBool::new(false);
        let mounted = |_: &Path| Some("ShadowMount+ has this image mounted at /mnt/x".to_string());
        let got = scan_with(&req, false, &cancel, &mounted, true)
            .unwrap()
            .err()
            .unwrap();
        assert_eq!(got, ["ShadowMount+ has this image mounted at /mnt/x"]);
        assert_eq!(std::fs::read(&img).unwrap(), damaged, "nothing written");
        assert!(at.exists());
        // Not mounted: the check restores the image first, then plans on it.
        plan(&req).unwrap();
        assert_eq!(std::fs::read(&img).unwrap(), original);
        assert!(!at.exists());
        // A refused edit leaves a recovered image recovered.
        std::fs::write(&img, &damaged).unwrap();
        journal(&img, 0, &original[..512]);
        let missing = Edit::Delete {
            path: "nope".into(),
        };
        findings(&request(&img, vec![missing]));
        assert_eq!(std::fs::read(&img).unwrap(), original);
    }
}

#[cfg(test)]
mod apply_tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;

    use super::tests::{image, put, request};
    use super::*;
    use crate::Edit;

    fn with_ctx<T>(f: impl FnOnce(&Ctx) -> T) -> T {
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|_| {});
        let cancel = AtomicBool::new(false);
        f(&Ctx::new(1, &emit, &cancel))
    }

    /// Every file's bytes and the empty dirs, through the image's reader.
    fn contents(img: &Path) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
        let mut tree = open_source(img, Kind::of(img).unwrap(), &AtomicBool::new(false)).unwrap();
        let files = (tree.files().to_vec().into_iter())
            .map(|f| (f.path.clone(), tree.read(&f.path).unwrap()))
            .collect();
        (files, tree.empty_dirs().to_vec())
    }

    /// An add, a folder add, a replace, a delete, a rename and a mkdir on `name`'s image.
    fn edits(dir: &Path) -> Vec<Edit> {
        let host = dir.join("host");
        put(&host, "new.bin", &[9u8; 200_000]);
        put(&host, "pack/x.txt", b"x");
        put(&host, "bee.bin", b"a new bee");
        std::fs::create_dir_all(host.join("pack/void")).unwrap();
        vec![
            Edit::Add {
                from: host.join("new.bin"),
                to: "data/new/new.bin".into(),
            },
            Edit::Add {
                from: host.join("pack"),
                to: "data/pack".into(),
            },
            Edit::Add {
                from: host.join("bee.bin"),
                to: "data/sub/b.bin".into(),
            },
            Edit::Delete {
                path: "data/empty".into(),
            },
            Edit::Rename {
                from: "data/a.bin".into(),
                to: "data/renamed.bin".into(),
            },
            Edit::Mkdir {
                path: "extra/deep".into(),
            },
        ]
    }

    fn applies(name: &str, full: bool) {
        let dir = crate::test_dir(&format!("edit-apply-{name}-{full}"));
        let img = image(&dir, name, b"\x7fELF plain eboot", &[]);
        let (before, _) = contents(&img);
        let mut req = request(&img, edits(&dir));
        req.full_verify = full;
        let report = with_ctx(|ctx| run(&req, ctx)).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        assert_eq!(report.output, img);
        assert_eq!(report.files, 6);
        assert_eq!(report.bytes, std::fs::metadata(&img).unwrap().len());
        assert!(
            report.checks.iter().any(|c| c.starts_with("blake3:")),
            "{report:?}"
        );
        assert_eq!(
            report.checks.last().unwrap(),
            &format!("edited {} in place", img.display())
        );
        assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
        let (files, empty) = contents(&img);
        let want: BTreeMap<String, Vec<u8>> = [
            ("data/new/new.bin", vec![9u8; 200_000]),
            ("data/pack/x.txt", b"x".to_vec()),
            ("data/renamed.bin", before["data/a.bin"].clone()),
            ("data/sub/b.bin", b"a new bee".to_vec()),
            ("eboot.bin", before["eboot.bin"].clone()),
            ("sce_sys/param.json", before["sce_sys/param.json"].clone()),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), b))
        .collect();
        assert_eq!(files, want, "{name}");
        assert_eq!(empty, ["data/pack/void", "extra/deep"], "{name}");
        // A second edit on the edited image.
        let again = request(
            &img,
            vec![Edit::Delete {
                path: "data/new".into(),
            }],
        );
        with_ctx(|ctx| run(&again, ctx)).unwrap();
        let (files, _) = contents(&img);
        assert!(!files.contains_key("data/new/new.bin"));
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn an_edit_is_applied_verified_and_committed() {
        for name in ["game.exfat", "game.ffpkg"] {
            applies(name, false);
            applies(name, true);
        }
    }

    /// The image's bytes outside the free-space writes of `writes`.
    fn used_bytes(img: &Path, writes: &[Write]) -> Vec<u8> {
        let bytes = std::fs::read(img).unwrap();
        let mut free: Vec<(usize, usize)> = (writes.iter().filter(|w| !w.live))
            .map(|w| (w.offset as usize, (w.offset + w.data.len()) as usize))
            .collect();
        free.sort();
        let (mut out, mut at) = (Vec::new(), 0);
        for (start, end) in free {
            out.extend_from_slice(&bytes[at..start.max(at)]);
            at = at.max(end);
        }
        out.extend_from_slice(&bytes[at..]);
        out
    }

    #[test]
    fn a_failed_check_restores_the_image() {
        for name in ["game.exfat", "game.ffpkg"] {
            let dir = crate::test_dir(&format!("edit-restore-{name}"));
            let img = image(&dir, name, b"\x7fELF plain eboot", &[]);
            let req = request(&img, edits(&dir));
            let writes = super::tests::plan(&req).unwrap().writes;
            assert!(writes.iter().any(|w| w.live) && writes.iter().any(|w| !w.live));
            let original = contents(&img);
            let used = used_bytes(&img, &writes);
            let seen = Cell::new(false);
            let err = with_ctx(|ctx| {
                let planned = plan_job(&req, ctx).unwrap();
                apply(
                    planned,
                    false,
                    ctx,
                    &checked(&|at| {
                        // The edit is in the image when the check fails.
                        seen.set(contents(at).0.contains_key("data/new/new.bin"));
                        bail!("simulated mismatch")
                    }),
                )
            })
            .unwrap_err()
            .to_string();
            assert!(seen.get(), "{name}: the edit was applied first");
            assert!(
                err.starts_with(NOT_VERIFIED) && err.contains("simulated mismatch"),
                "{err}"
            );
            assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
            assert_eq!(contents(&img), original, "{name}: same tree and bytes");
            assert!(
                used_bytes(&img, &writes) == used,
                "{name}: every used byte restored"
            );
        }
    }

    /// What a `.ffpfs` takes in place: a same-size and a smaller replace, a delete, a move.
    fn pfs_edits(dir: &Path) -> Vec<Edit> {
        let host = dir.join("host");
        put(&host, "bee.bin", b"BEE");
        put(&host, "a.bin", &[5u8; 50_000]);
        vec![
            Edit::Add {
                from: host.join("bee.bin"),
                to: "data/sub/b.bin".into(),
            },
            Edit::Add {
                from: host.join("a.bin"),
                to: "data/a.bin".into(),
            },
            Edit::Delete {
                path: "data/empty".into(),
            },
            Edit::Rename {
                from: "data/sub/b.bin".into(),
                to: "data/moved.bin".into(),
            },
        ]
    }

    #[test]
    fn a_failed_check_restores_a_ffpfs() {
        let dir = crate::test_dir("edit-restore-ffpfs");
        let img = image(&dir, "game.ffpfs", b"\x7fELF plain eboot", &[]);
        let req = request(&img, pfs_edits(&dir));
        let writes = super::tests::plan(&req).unwrap().writes;
        // Every PFS write is live: the restore brings back every byte.
        assert!(!writes.is_empty() && writes.iter().all(|w| w.live));
        let original = std::fs::read(&img).unwrap();
        let seen = Cell::new(false);
        let err = with_ctx(|ctx| {
            let planned = plan_job(&req, ctx).unwrap();
            apply(
                planned,
                false,
                ctx,
                &checked(&|at| {
                    let (files, _) = contents(at);
                    seen.set(files.get("data/moved.bin").is_some_and(|b| b == b"BEE"));
                    bail!("simulated mismatch")
                }),
            )
        })
        .unwrap_err()
        .to_string();
        assert!(seen.get(), "the edit was applied first");
        assert!(
            err.starts_with(NOT_VERIFIED) && err.contains("simulated mismatch"),
            "{err}"
        );
        assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
        assert!(std::fs::read(&img).unwrap() == original, "byte-identical");
        // Unhindered, the same edit applies and reads back.
        with_ctx(|ctx| run(&req, ctx)).unwrap();
        let (files, empty) = contents(&img);
        assert_eq!(files["data/moved.bin"], b"BEE");
        assert_eq!(files["data/a.bin"], vec![5u8; 50_000]);
        assert!(!files.contains_key("data/sub/b.bin"));
        assert_eq!(empty, ["data/sub"]);
    }

    #[test]
    fn a_change_after_the_plan_fails_with_nothing_written() {
        for name in ["game.exfat", "game.ffpkg"] {
            let dir = crate::test_dir(&format!("edit-changed-{name}"));
            let img = image(&dir, name, b"\x7fELF plain eboot", &[]);
            let req = request(&img, edits(&dir));
            let original = std::fs::read(&img).unwrap();
            // A host file rewritten after the plan.
            let err = with_ctx(|ctx| {
                let planned = plan_job(&req, ctx).unwrap();
                std::fs::write(dir.join("host/new.bin"), [8u8; 200_000]).unwrap();
                let past = SystemTime::now() + std::time::Duration::from_secs(5);
                File::options()
                    .write(true)
                    .open(dir.join("host/new.bin"))
                    .unwrap()
                    .set_modified(past)
                    .unwrap();
                apply(planned, false, ctx, &checked(&|_| Ok(())))
            })
            .unwrap_err();
            assert!(
                format!("{err:#}").contains("changed after it was added"),
                "{err:#}"
            );
            assert_eq!(std::fs::read(&img).unwrap(), original);
            // The image itself touched after the plan.
            let req = request(
                &img,
                vec![Edit::Mkdir {
                    path: "more".into(),
                }],
            );
            let err = with_ctx(|ctx| {
                let planned = plan_job(&req, ctx).unwrap();
                let later = SystemTime::now() + std::time::Duration::from_secs(60);
                File::options()
                    .write(true)
                    .open(&img)
                    .unwrap()
                    .set_modified(later)
                    .unwrap();
                apply(planned, false, ctx, &checked(&|_| Ok(())))
            })
            .unwrap_err();
            assert!(
                err.to_string()
                    .contains("changed after the edit was planned"),
                "{err}"
            );
            assert_eq!(std::fs::read(&img).unwrap(), original);
            assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
        }
    }

    fn checked<'a>(check: &'a dyn Fn(&Path) -> anyhow::Result<()>) -> Hooks<'a> {
        Hooks {
            sync: SYNCS,
            keep: Applied::commit,
            check,
        }
    }

    /// Runs `req`'s edit with `hooks` and an event sink `emit` given the job's cancel flag;
    /// returns the error, after checking the image reads as it did and no journal is left.
    fn refused(
        dir: &str,
        hooks: impl FnOnce(&AtomicBool) -> Hooks<'static>,
        emit: impl Fn(&crate::Event, &AtomicBool) + Send + Sync + 'static,
    ) -> String {
        let dir = crate::test_dir(dir);
        let img = image(&dir, "game.exfat", b"\x7fELF plain eboot", &[]);
        let req = request(&img, edits(&dir));
        let writes = super::tests::plan(&req).unwrap().writes;
        let (original, used) = (contents(&img), used_bytes(&img, &writes));
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(move |e| emit(&e, &flag));
        let ctx = Ctx::new(1, &emit, &cancel);
        let planned = plan_job(&req, &ctx).unwrap();
        let err = apply(planned, false, &ctx, &hooks(&cancel)).unwrap_err();
        assert_eq!(contents(&img), original, "{err:#}");
        assert!(used_bytes(&img, &writes) == used, "{err:#}");
        assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
        format!("{err:#}")
    }

    fn stage(e: &crate::Event, want: &str) -> bool {
        matches!(e, crate::Event::Progress { stage, .. } if stage == want)
    }

    #[test]
    fn a_cancel_after_the_apply_restores_the_image() {
        // Only the flag is set: during the verification, and on the finalize event.
        for at in ["verify", "finalize"] {
            let err = refused(
                &format!("edit-cancel-{at}"),
                |_| checked(&|_| Ok(())),
                move |e, cancel| {
                    if stage(e, at) {
                        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                },
            );
            assert!(
                err.starts_with("cancelled; the image was restored"),
                "{at}: {err}"
            );
        }
    }

    fn not_deleted(_: Applied, _: Syncs) -> io::Result<()> {
        Err(io::Error::other(
            "simulated: the journal could not be deleted",
        ))
    }

    #[test]
    fn a_journal_that_cannot_be_deleted_restores_the_image() {
        let err = refused(
            "edit-commit-delete",
            |_| Hooks {
                sync: SYNCS,
                keep: not_deleted,
                check: &|_| Ok(()),
            },
            |_, _| {},
        );
        assert!(
            err.starts_with("simulated: the journal could not be deleted; the image was restored"),
            "{err}"
        );
    }

    #[test]
    fn a_panic_after_the_apply_restores_the_image() {
        let dir = crate::test_dir("edit-panic");
        let img = image(&dir, "game.exfat", b"\x7fELF plain eboot", &[]);
        let req = request(&img, edits(&dir));
        let original = contents(&img);
        let emit: Box<dyn Fn(crate::Event) + Send + Sync> = Box::new(|e| {
            if stage(&e, "verify") {
                panic!("simulated panic in an event callback");
            }
        });
        let cancel = AtomicBool::new(false);
        let ctx = Ctx::new(1, &emit, &cancel);
        let planned = plan_job(&req, &ctx).unwrap();
        let hooks = checked(&|_| Ok(()));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply(planned, false, &ctx, &hooks)
        }));
        assert!(caught.is_err());
        assert_eq!(contents(&img), original);
        assert!(!ps5_dump_forge_patch::journal_path(&img).exists());
    }
}
