//! Extract (a request with [`ConvertRequest::extract`]): files and folders of any source
//! copied into an existing folder on this computer or a paired one (`flink://`), each under
//! its own last component. Every
//! check runs before the first byte is written; nothing there is replaced unless the request
//! says [`ConvertRequest::replace`] (a file onto a file, a folder merged into a folder, each
//! replaced file refused as Delete refuses it; on another device the device judges what it
//! may replace, [`crate::remote::Remote::rename_replace`]). Each file goes through an
//! exclusive temporary name beside its target, synced, then renamed without replacing (over
//! the file it replaces); a selected folder is created exclusively first (one merged into is
//! kept). BLAKE3 while writing; verify checks each file's size, and with full verification
//! re-reads and re-hashes it.
// ponytail: no rollback: a failure part way leaves the files already extracted (the error
// says how many); each file itself is atomic. No U5 sync cadence on the PS5: each file is
// synced once, whole.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::convert::{Kind, SourceStamp, open_source};
use crate::finalize::{fsync_dir, make_dir_open};
use crate::jobs::Ctx;
use crate::prefetch::Prefetch;
use crate::preflight;
use crate::remote::RemoteDir;
use crate::temp_file::{write_temp, write_temp_new};
use crate::{ConvertRequest, Format, JobReport, VerifyMode, VerifySummary};

/// The stages an extract reports, in order.
pub(crate) const STAGES: [&str; 4] = ["scan", "preflight", "write", "verify"];
/// One read of a copy or of the check.
pub(crate) const CHUNK: u64 = 8 << 20;
/// Room beyond the copied bytes the destination must have.
const MARGIN: u64 = 1 << 20;

/// One selected path, resolved against the source's listing.
#[derive(Debug, PartialEq)]
pub(crate) struct Selection {
    /// The game path as selected.
    pub(crate) path: String,
    /// Its last component: the name it gets in the destination.
    pub(crate) name: String,
    /// A folder (else one file, `files[0]`).
    pub(crate) dir: bool,
    /// The files to copy, by game path.
    pub(crate) files: Vec<SourceFile>,
    /// Folders inside a selected folder, relative to it, parents first.
    pub(crate) dirs: Vec<String>,
}

impl Selection {
    /// `game` (a game path inside this selection) as a destination-relative path.
    fn target(&self, game: &str) -> String {
        match game.strip_prefix(&self.path) {
            Some(rest) if self.dir => format!("{}{rest}", self.name),
            _ => self.name.clone(),
        }
    }
}

/// Why `path` is not a plain game path (`/`-separated, relative, no empty, `.` or `..`
/// component, no `\`).
fn bad_path(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("an empty path");
    }
    if path.contains('\\') {
        return Some("a backslash in the path");
    }
    let bad = |c: &str| c.is_empty() || c == "." || c == "..";
    path.split('/')
        .any(bad)
        .then_some("an empty, '.' or '..' component")
}

/// Resolves `paths` against the source's `files` and `empty_dirs`: every unknown or malformed
/// path, two selections with one name, and a selection inside another are findings.
pub(crate) fn select(
    files: &[SourceFile],
    empty_dirs: &[String],
    paths: &[String],
) -> Result<Vec<Selection>, Vec<String>> {
    let mut findings = Vec::new();
    let mut out = Vec::new();
    for path in paths {
        if let Some(why) = bad_path(path) {
            findings.push(format!("extract: {path:?} is {why}"));
            continue;
        }
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        if let Some(file) = files.iter().find(|f| f.path == *path) {
            out.push(Selection {
                path: path.clone(),
                name,
                dir: false,
                files: vec![file.clone()],
                dirs: Vec::new(),
            });
            continue;
        }
        let prefix = format!("{path}/");
        let inside: Vec<SourceFile> = (files.iter())
            .filter(|f| f.path.starts_with(&prefix))
            .cloned()
            .collect();
        let empties: Vec<&String> = (empty_dirs.iter())
            .filter(|d| *d == path || d.starts_with(&prefix))
            .collect();
        if inside.is_empty() && empties.is_empty() {
            findings.push(format!("extract: {path} is not in the game"));
            continue;
        }
        // Every folder below the selected one: each file's parents and each empty folder with
        // its parents. Sorted, a parent comes before its children (it is their prefix).
        let mut dirs = BTreeSet::new();
        for rel in (inside.iter().map(|f| &f.path[prefix.len()..]))
            .map(|p| p.rsplit_once('/').map_or("", |(d, _)| d))
            .chain(empties.iter().filter_map(|d| d.strip_prefix(&prefix)))
        {
            let mut at = 0;
            while !rel.is_empty() {
                let end = rel[at..].find('/').map_or(rel.len(), |i| at + i);
                dirs.insert(rel[..end].to_string());
                if end == rel.len() {
                    break;
                }
                at = end + 1;
            }
        }
        out.push(Selection {
            path: path.clone(),
            name,
            dir: true,
            files: inside,
            dirs: dirs.into_iter().collect(),
        });
    }
    let mut names: HashMap<&str, &str> = HashMap::new();
    for s in &out {
        if let Some(other) = names.insert(&s.name, &s.path) {
            findings.push(format!(
                "extract: {other} and {} would both be named {}",
                s.path, s.name
            ));
        }
    }
    for a in &out {
        for b in out
            .iter()
            .filter(|b| a.dir && b.path.starts_with(&format!("{}/", a.path)))
        {
            findings.push(format!("extract: {} is inside {}", b.path, a.path));
        }
    }
    if findings.is_empty() {
        Ok(out)
    } else {
        Err(findings)
    }
}

/// What the request itself can't be, before anything is read.
fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if req.format != Format::Folder {
        findings.push("extract: the target format must be folder".to_string());
    }
    if !req.edits.is_empty()
        || req.edit_rebuild
        || req.lz4.is_some()
        || req.zstd_pack
        || req.zstd_unpack
    {
        findings
            .push("extract: a request to extract takes no edit, LZ4 or zstd options".to_string());
    }
    // Into a mounted game, written through the mount while the game may run.
    #[cfg(target_os = "freebsd")]
    findings.extend(crate::edit_job::smp_inside(&req.output).map(|why| format!("extract: {why}")));
    if crate::remote::is_remote(&req.output) {
        if let Err(e) = RemoteDir::open(&req.output) {
            findings.push(format!("extract: {e:#}"));
        }
        return findings;
    }
    if let Some(why) = crate::net::output_refusal(&req.output) {
        findings.push(format!("extract: {why}"));
        return findings;
    }
    match std::fs::metadata(&req.output) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => findings.push(format!("extract: {} is not a folder", req.output.display())),
        Err(e) => findings.push(format!("extract: {}: {e}", req.output.display())),
    }
    findings
}

/// Every check on the destination folder `root` (canonical): not inside a folder source,
/// nothing at any selection's name, the names it can hold, free space.
fn destination_findings(
    req: &ConvertRequest,
    kind: Kind,
    root: &Path,
    selections: &[Selection],
    ctx: &Ctx,
) -> (Vec<String>, Replacing) {
    let mut findings = Vec::new();
    // A device's paths compare as spelled (normalized, an SMB share's case folded: the device
    // resolves them), local ones canonical.
    let inside = if crate::remote::is_remote(root) || crate::remote::is_remote(&req.source) {
        use crate::remote::FlinkPath;
        match (FlinkPath::parse(root), FlinkPath::parse(&req.source)) {
            (Ok(root), Ok(source)) => root.within(&source),
            _ => false,
        }
    } else {
        dunce::canonicalize(&req.source).is_ok_and(|source| root.starts_with(source))
    };
    if kind == Kind::Folder && inside {
        findings.push(format!(
            "extract: the destination {} is inside the game folder",
            root.display()
        ));
    }
    let (found, replacing) = target_findings("extract", root, selections, req.replace, ctx);
    findings.extend(found);
    (findings, replacing)
}

/// What a request with [`ConvertRequest::replace`] replaces, as its preflight found it: the
/// paths [`write`] builds.
#[derive(Debug, Default)]
pub(crate) struct Replacing {
    /// Files renamed over.
    files: HashSet<PathBuf>,
    /// Folders merged into (kept, not made).
    dirs: HashSet<PathBuf>,
}

/// The destination folder `root`'s checks (`what` leads each finding): nothing at any
/// selection's name (with `replace`, only what [`replace_findings`] passes),
/// the names it can hold, free space.
pub(crate) fn target_findings(
    what: &str,
    root: &Path,
    selections: &[Selection],
    replace: bool,
    ctx: &Ctx,
) -> (Vec<String>, Replacing) {
    let mut findings = Vec::new();
    let mut replacing = Replacing::default();
    let peer = crate::remote::is_remote(root);
    let mut kinds = Kinds::default();
    for s in selections {
        let target = root.join(&s.name);
        if !crate::net::exists(&target) {
            continue;
        }
        if !replace {
            findings.push(format!(
                "{what}: {} already exists (never replaced)",
                target.display()
            ));
        } else {
            findings.extend(replace_findings(
                what,
                root,
                s,
                &mut replacing,
                &mut kinds,
                ctx,
            ));
        }
    }
    let files: Vec<String> = (selections.iter())
        .flat_map(|s| s.files.iter().map(|f| s.target(&f.path)))
        .collect();
    let dirs: Vec<String> = (selections.iter())
        .filter(|s| s.dir)
        .flat_map(|s| {
            let top = std::iter::once(s.name.clone());
            top.chain(s.dirs.iter().map(|d| format!("{}/{d}", s.name)))
        })
        .collect();
    let (total, largest) = (selections.iter().flat_map(|s| &s.files))
        .fold((0u64, 0u64), |(sum, max), f| {
            (sum.saturating_add(f.size), max.max(f.size))
        });
    let checked = if peer {
        crate::remote::destination(root, total.saturating_add(MARGIN))
    } else {
        preflight::destination(root, total.saturating_add(MARGIN), largest, ctx.cancel)
    };
    findings.extend(checked.findings);
    for note in checked.notes {
        ctx.log(note);
    }
    #[cfg(not(target_os = "freebsd"))]
    findings.extend(crate::extract::extraction_findings(
        &files,
        &dirs,
        // Another device's folding is unknown: as if it folds (an SMB share does).
        crate::extract::CASE_INSENSITIVE || peer,
    ));
    // U2: names as the folder's filesystem folds them; with no probe, as if it folds.
    #[cfg(target_os = "freebsd")]
    {
        let (folds, ascii_only) =
            (checked.dest.as_ref()).map_or((true, false), |d| (d.folds_names, d.ascii_only));
        findings.extend(preflight::folder_names(&files, &dirs, folds, ascii_only));
    }
    (findings, replacing)
}

/// What is at a target path, as [`replace_findings`] judges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    File,
    Dir,
    Link,
    /// A FIFO, socket or device (or, on another device, a name that isn't UTF-8).
    Other,
}

/// What is at paths here or on another device ([`Kinds::at`]); each remote folder is listed
/// once, not stat'ed per entry (a merged folder of 10,000 files is 10,000 round trips else).
#[derive(Default)]
struct Kinds {
    listed: HashMap<PathBuf, Result<Vec<crate::remote::RemoteEntry>, String>>,
}

impl Kinds {
    /// What is at `path` (`None`: nothing), or why that can't be said. On another device an
    /// entry whose name differs only in case is a refusal: the device may take the two names
    /// for one (an SMB share does) or not, so neither a merge nor a new name is safe.
    fn at(&mut self, path: &Path) -> Result<Option<Found>, String> {
        use crate::remote::FlinkPath;
        if !crate::remote::is_remote(path) {
            return match path.symlink_metadata() {
                Ok(meta) => {
                    let t = meta.file_type();
                    Ok(Some(if t.is_symlink() {
                        Found::Link
                    } else if t.is_dir() {
                        Found::Dir
                    } else if t.is_file() {
                        Found::File
                    } else {
                        Found::Other
                    }))
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{}: {e}", path.display())),
            };
        }
        let at = FlinkPath::parse(path).map_err(|e| format!("{e:#}"))?;
        let Some(parent) = at.parent() else {
            return Ok(Some(Found::Dir));
        };
        let name = at.path.rsplit('/').next().unwrap_or(&at.path);
        let entries = self.listed.entry(parent.url()).or_insert_with(|| {
            (crate::remote::get_for(&parent).and_then(|r| r.list(&parent)))
                .map_err(|e| format!("listing {}: {e}", parent.shown()))
        });
        let entries = entries.as_ref().map_err(Clone::clone)?;
        if let Some(e) = entries.iter().find(|e| e.name == name) {
            return Ok(Some(if e.link {
                Found::Link
            } else if e.special || e.bad_name {
                Found::Other
            } else if e.dir {
                Found::Dir
            } else {
                Found::File
            }));
        }
        match entries.iter().find(|e| e.name.eq_ignore_ascii_case(name)) {
            Some(e) => Err(format!(
                "{} is there as {}: the device may take the two names for one",
                path.display(),
                e.name
            )),
            None => Ok(None),
        }
    }
}

/// What selection `s` would replace in `root` (here or on another device), its target
/// existing: a file onto a file, a folder onto a folder (merged: every folder and file inside
/// checked too). A kind mismatch at any level is a finding, and so is a replaced file Delete
/// would refuse ([`Ctx::replace_refusal`]; on another device only what this device's jobs use:
/// a peer refuses the rest itself, at the rename); what passes goes into `replacing`.
fn replace_findings(
    what: &str,
    root: &Path,
    s: &Selection,
    replacing: &mut Replacing,
    kinds: &mut Kinds,
    ctx: &Ctx,
) -> Vec<String> {
    let mut findings = Vec::new();
    let top = root.join(&s.name);
    // The selection's folders (destination-relative) that are missing or not a folder there:
    // nothing below them exists to replace. Parents come first.
    let mut absent: HashSet<String> = HashSet::new();
    let parent_absent = |rel: &str, absent: &HashSet<String>| {
        rel.rsplit_once('/')
            .is_some_and(|(p, _)| absent.contains(p))
    };
    let mut entries: Vec<(String, PathBuf, bool)> = vec![(s.name.clone(), top.clone(), s.dir)];
    if s.dir {
        entries.extend(s.dirs.iter().map(|d| {
            let rel = format!("{}/{d}", s.name);
            (rel, top.join(d), true)
        }));
        entries.extend(s.files.iter().map(|f| {
            let rel = s.target(&f.path);
            let (parent, name) = rel.rsplit_once('/').unwrap_or(("", &rel));
            let path = root.join(parent).join(name);
            (rel, path, false)
        }));
    }
    for (rel, path, dir) in entries {
        if parent_absent(&rel, &absent) {
            if dir {
                absent.insert(rel);
            }
            continue;
        }
        let shown = path.display();
        let kind = match kinds.at(&path) {
            Ok(Some(kind)) => kind,
            found => {
                if let Err(e) = found {
                    findings.push(format!("{what}: {e}"));
                }
                if dir {
                    absent.insert(rel);
                }
                continue;
            }
        };
        let mismatch = match (kind, dir) {
            (Found::Link, _) => Some("is a link: nothing is put in its place"),
            (Found::Dir, false) => Some("is a folder: a file is not put in its place"),
            (Found::File, true) => Some("is a file: a folder is not put in its place"),
            (Found::Other, _) => Some("is not a file or folder: nothing is put in its place"),
            _ => None,
        };
        if let Some(why) = mismatch {
            findings.push(format!("{what}: {shown} {why}"));
            if dir {
                absent.insert(rel);
            }
            continue;
        }
        // Every folder merged into is checked too: the top one's job check covers those inside
        // it, but not ShadowMount+'s mount folder found further down (PS5).
        if let Err(e) = ctx.replace_refusal(&path, dir) {
            findings.push(format!("{what}: {e}"));
            continue;
        }
        if dir {
            replacing.dirs.insert(path);
        } else {
            replacing.files.insert(path);
        }
    }
    findings
}

/// One log line naming what was replaced (the first ten and a count of the rest).
pub(crate) fn log_replaced(replaced: &[PathBuf], ctx: &Ctx) {
    if replaced.is_empty() {
        return;
    }
    let shown: Vec<String> = (replaced.iter().take(10))
        .map(|p| p.display().to_string())
        .collect();
    let mut line = format!(
        "replace: {} file{} replaced: {}",
        replaced.len(),
        if replaced.len() == 1 { "" } else { "s" },
        shown.join(", ")
    );
    if replaced.len() > 10 {
        line += &format!(" and {} more", replaced.len() - 10);
    }
    ctx.log(line);
}

/// The targets of an extract or transfer request that exist (each selection's, top level).
pub(crate) fn existing_targets(req: &ConvertRequest) -> anyhow::Result<Vec<PathBuf>> {
    let names: Vec<String> = if !req.transfer.is_empty() {
        (req.transfer.iter())
            .filter_map(|s| crate::transfer::name_of(s))
            .collect()
    } else if !req.extract.is_empty() {
        (req.extract.iter())
            .filter(|p| bad_path(p).is_none())
            .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
            .collect()
    } else {
        bail!("only an extract or a transfer has targets to check");
    };
    // The folder itself first: a device that doesn't answer is an error, not "nothing there".
    let root = if crate::remote::is_remote(&req.output) {
        RemoteDir::open(&req.output)?.path().to_path_buf()
    } else {
        let meta = std::fs::metadata(&req.output)
            .with_context(|| format!("opening {}", req.output.display()))?;
        if !meta.is_dir() {
            bail!("{} is not a folder", req.output.display());
        }
        req.output.clone()
    };
    let mut seen = HashSet::new();
    Ok((names.into_iter())
        .filter(|n| seen.insert(n.clone()))
        .map(|n| root.join(n))
        .filter(|t| crate::net::exists(t))
        .collect())
}

pub(crate) fn preflight_failed(findings: Vec<String>) -> anyhow::Error {
    anyhow::anyhow!("preflight failed:\n  {}", findings.join("\n  "))
}

/// The extract job: scan, preflight, write, verify.
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    ctx.stages(&STAGES);
    ctx.progress("scan", 0, 1);
    let findings = request_findings(req);
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    let kind = Kind::of(&req.source)?;
    crate::convert::refuse_interrupted_edit(&req.source, kind)?;
    let stamp = SourceStamp::take(&req.source, kind)?;
    let (tree, stamp) = match kind {
        Kind::Archive(format) => crate::archive::open(req, ctx, format, stamp)?,
        _ => (open_source(&req.source, kind, ctx.cancel)?, stamp),
    };
    ctx.log(format!("source: {}", tree.describe()));
    ctx.progress("scan", 1, 1);
    ctx.check()?;

    ctx.progress("preflight", 0, 1);
    let selections =
        select(tree.files(), tree.empty_dirs(), &req.extract).map_err(preflight_failed)?;
    // On a peer: the folder there, its path spelled as `FlinkPath` normalizes it.
    let mut peer = if crate::remote::is_remote(&req.output) {
        Some(RemoteDir::open(&req.output)?)
    } else {
        None
    };
    let root = match &peer {
        Some(dir) => dir.path().to_path_buf(),
        None => dunce::canonicalize(&req.output)
            .with_context(|| format!("opening {}", req.output.display()))?,
    };
    let (findings, replacing) = destination_findings(req, kind, &root, &selections, ctx);
    ctx.check()?;
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    #[cfg(target_os = "freebsd")]
    {
        ctx.watch(&req.source);
        if peer.is_none() {
            ctx.watch(&root);
        }
    }
    let total =
        (selections.iter().flat_map(|s| &s.files)).fold(0u64, |n, f| n.saturating_add(f.size));
    let count: usize = selections.iter().map(|s| s.files.len()).sum();
    ctx.progress("preflight", 1, 1);

    ctx.expect_rest(if req.full_verify {
        total.saturating_mul(2)
    } else {
        total
    });
    let mut written: Vec<(PathBuf, u64, blake3::Hash)> = Vec::with_capacity(count);
    let mut made = Vec::new();
    let mut replaced = Vec::new();
    let mut done = 0u64;
    let mut tree = read_ahead(tree, &selections)?;
    let wrote = write(
        &mut tree,
        &root,
        peer.as_mut(),
        &selections,
        &replacing,
        total,
        &mut done,
        &mut Written {
            files: &mut written,
            made: &mut made,
            replaced: &mut replaced,
        },
        ctx,
    );
    log_replaced(&replaced, ctx);
    let outcome = wrote.and_then(|()| {
        ctx.progress("write", total, total);
        ctx.progress("verify", 0, 1);
        if !stamp.unchanged() {
            bail!("{} changed while it was read", req.source.display());
        }
        verify(&written, req.full_verify, ctx)
    });
    // A failure (or cancel) after some files were finished: they stay, and the log says so
    // too, since a cancelled job's error may be only "cancelled".
    let checked = match (outcome, written.len()) {
        (Ok(checked), _) => checked,
        (Err(e), 0) => return Err(made_note(e, &made, "extract", ctx)),
        (Err(e), n) => {
            let kept = format!("{n} of {count} files were extracted before it and stay");
            ctx.log(format!("extract: {kept}"));
            return Err(anyhow::anyhow!("{e:#}; {kept}"));
        }
    };
    ctx.progress("verify", 1, 1);
    let mode = if req.full_verify {
        VerifyMode::Full
    } else {
        VerifyMode::Fast
    };
    let check = if req.full_verify {
        format!("extract: {count} files read back, BLAKE3 matches")
    } else {
        format!("extract: {count} files, sizes match")
    };
    Ok(JobReport {
        output: root,
        bytes: total,
        files: count as u64,
        checks: vec![check],
        verify: VerifySummary {
            mode,
            checked_bytes: checked,
            total_bytes: total,
            samples: 0,
            seed: 0,
        },
    })
}

/// `tree` read one range ahead on its own thread, so the next read overlaps writing and
/// hashing the last one (as convert's writers do). When each selected file is the next
/// non-empty one of `files()` after the one before it, the next file is read ahead too; else
/// a guess past a file's end could send an archive's decoder past what is asked next.
pub(crate) fn read_ahead(
    tree: Box<dyn SourceTree>,
    selections: &[Selection],
) -> anyhow::Result<Prefetch> {
    let rank: HashMap<&str, usize> = (tree.files().iter())
        .filter(|f| f.size > 0)
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();
    let mut order = (selections.iter().flat_map(|s| &s.files))
        .filter(|f| f.size > 0)
        .map(|f| rank.get(f.path.as_str()).copied());
    let first = order.next().flatten();
    let in_order = (order.try_fold(first, |at, next| match (at, next) {
        (Some(a), Some(b)) if b == a + 1 => Some(Some(b)),
        _ => None,
    }))
    .is_some();
    drop(rank);
    if in_order {
        Prefetch::sequential(tree, CHUNK as usize)
    } else {
        Prefetch::new(tree)
    }
    .context("starting the read-ahead thread")
}

/// A new folder at `path`, exclusively (an existing one fails), 0777 on the PS5.
fn make_dir(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir(path).with_context(|| format!("creating {}", path.display()))?;
    make_dir_open(path).with_context(|| format!("opening up {}", path.display()))
}

/// What [`write`] has done so far.
pub(crate) struct Written<'a> {
    /// Each finished file: its path, size and BLAKE3.
    pub(crate) files: &'a mut Vec<(PathBuf, u64, blake3::Hash)>,
    /// Each selection folder made.
    pub(crate) made: &'a mut Vec<PathBuf>,
    /// Each file renamed over one that was there ([`Replacing`]).
    pub(crate) replaced: &'a mut Vec<PathBuf>,
}

/// Copies every selection into `root` (through `peer` when it is on one), replacing only what
/// `replacing` names; `out` records what was done, `done` the bytes copied.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write(
    tree: &mut dyn SourceTree,
    root: &Path,
    mut peer: Option<&mut RemoteDir>,
    selections: &[Selection],
    replacing: &Replacing,
    total: u64,
    done: &mut u64,
    out: &mut Written<'_>,
    ctx: &Ctx,
) -> anyhow::Result<()> {
    let Written {
        files: written,
        made,
        replaced,
    } = out;
    for s in selections {
        ctx.check()?;
        let target = root.join(&s.name);
        let bytes = s.files.iter().fold(0u64, |n, f| n.saturating_add(f.size));
        ctx.log(format!(
            "extract: {} -> {} ({} files, {} bytes)",
            s.path,
            target.display(),
            s.files.len(),
            bytes
        ));
        if let Some(dir) = peer.as_deref_mut() {
            // Each folder made exclusively (one merged into kept, parents first); the peer
            // syncs each folder a file is renamed into.
            if s.dir {
                if replacing.dirs.contains(&target) {
                    dir.keep(&s.name);
                } else {
                    dir.dir(&s.name)?;
                    made.push(target.clone());
                }
                for d in &s.dirs {
                    let rel = format!("{}/{d}", s.name);
                    if replacing.dirs.contains(&target.join(d)) {
                        dir.keep(&rel);
                    } else {
                        dir.dir(&rel)?;
                    }
                }
            }
            for file in &s.files {
                let rel = s.target(&file.path);
                let (parent, name) = rel.rsplit_once('/').unwrap_or(("", &rel));
                let path = root.join(parent).join(name);
                let replace = replacing.files.contains(&path);
                let mut hasher = blake3::Hasher::new();
                dir.write(&rel, replace, &mut |out| {
                    copy(tree, file, out, &mut hasher, total, done, ctx)
                })?;
                if replace {
                    replaced.push(path);
                }
                written.push((root.join(&rel), file.size, hasher.finalize()));
            }
            continue;
        }
        let mut synced = vec![root.to_path_buf()];
        if s.dir {
            // A folder merged into is kept; every other one is made exclusively.
            if !replacing.dirs.contains(&target) {
                make_dir(&target)?;
                made.push(target.clone());
            }
            for dir in s.dirs.iter().map(|d| target.join(d)) {
                if !replacing.dirs.contains(&dir) {
                    make_dir(&dir)?;
                }
            }
            synced.push(target.clone());
            synced.extend(s.dirs.iter().map(|d| target.join(d)));
        }
        let files = s.files.iter().try_for_each(|file| -> anyhow::Result<()> {
            let rel = s.target(&file.path);
            let (parent, name) = rel.rsplit_once('/').unwrap_or(("", &rel));
            let dir = root.join(parent);
            let mut hasher = blake3::Hasher::new();
            let mut fill = |out: &mut File| -> io::Result<()> {
                copy(tree, file, out, &mut hasher, total, done, ctx)
            };
            let path = dir.join(name);
            if replacing.files.contains(&path) {
                write_temp(&dir, name, &mut fill)?;
                replaced.push(path.clone());
            } else {
                write_temp_new(&dir, name, &mut fill)?;
            }
            written.push((path, file.size, hasher.finalize()));
            Ok(())
        });
        if files.is_err() {
            // The barrier for the files finished before the failure (they stay).
            let _ = fsync_dir(root);
        }
        files?;
        // The root last: on macOS its F_FULLFSYNC is the barrier for every plain fsync before.
        for (i, dir) in synced.iter().enumerate().rev() {
            let synced = if i == 0 {
                fsync_dir(dir)
            } else {
                sync_folder(dir)
            };
            synced.with_context(|| format!("syncing {}", dir.display()))?;
        }
    }
    Ok(())
}

/// A folder synced; on macOS a plain fsync, made durable by the F_FULLFSYNC of the
/// [`fsync_dir`] that follows it ([`crate::extract::sync_file`]).
fn sync_folder(dir: &Path) -> io::Result<()> {
    #[cfg(target_vendor = "apple")]
    return crate::extract::sync_file(&File::open(dir)?);
    #[cfg(not(target_vendor = "apple"))]
    fsync_dir(dir)
}

/// A failure before any file was finished: the folders `write` made stay (a retry finds them
/// taken), so the error (and the log) names them.
pub(crate) fn made_note(e: anyhow::Error, made: &[PathBuf], job: &str, ctx: &Ctx) -> anyhow::Error {
    let note = match made {
        [] => return e,
        [one] => format!("{} was created and stays", one.display()),
        _ => {
            let all: Vec<String> = made.iter().map(|p| p.display().to_string()).collect();
            format!("{} were created and stay", all.join(", "))
        }
    };
    ctx.log(format!("{job}: {note}"));
    anyhow::anyhow!("{e:#}; {note}")
}

/// Streams `file` of `tree` into `out` in chunks, hashing it, checking cancel between chunks.
fn copy(
    tree: &mut dyn SourceTree,
    file: &SourceFile,
    out: &mut dyn Write,
    hasher: &mut blake3::Hasher,
    total: u64,
    done: &mut u64,
    ctx: &Ctx,
) -> io::Result<()> {
    let mut offset = 0u64;
    while offset < file.size {
        if ctx.check().is_err() {
            return Err(io::Error::other("cancelled"));
        }
        let want = (file.size - offset).min(CHUNK) as usize;
        let buf = (tree.read_range(&file.path, offset, want))
            .map_err(|e| io::Error::other(format!("reading {}: {e}", file.path)))?;
        if buf.is_empty() || buf.len() > want {
            return Err(io::Error::other(format!(
                "{}: the source gave {} bytes at {offset} of {} where {want} were asked",
                file.path,
                buf.len(),
                file.size
            )));
        }
        out.write_all(&buf)?;
        hasher.update(&buf);
        offset += buf.len() as u64;
        *done += buf.len() as u64;
        ctx.progress("write", *done, total);
    }
    Ok(())
}

/// Each written file's size; with `full`, its bytes re-read and re-hashed too. Returns the
/// bytes read back.
pub(crate) fn verify(
    written: &[(PathBuf, u64, blake3::Hash)],
    full: bool,
    ctx: &Ctx,
) -> anyhow::Result<u64> {
    let total = if full {
        written
            .iter()
            .fold(0u64, |n, (_, size, _)| n.saturating_add(*size))
    } else {
        0
    };
    let mut done = 0u64;
    let mut buf = Vec::new();
    let mut listed = Listed::default();
    for (path, size, hash) in written {
        ctx.check()?;
        let shown = || path.display().to_string();
        let peer = crate::remote::is_remote(path);
        // On another device: fast mode takes the size from its folder's one listing; the full
        // check reads back through the reader whose open gave the size.
        let mut reader = None;
        let len = match peer {
            true if !full && let Some(len) = listed.size(path) => len,
            true => reader.insert(crate::remote::reader(path)?).len(),
            false => std::fs::symlink_metadata(path).with_context(shown)?.len(),
        };
        if len != *size {
            bail!(
                "verify: {} holds {len} bytes, {size} were written",
                path.display()
            );
        }
        if !full {
            continue;
        }
        let mut file: Box<dyn Read> = if let Some(reader) = reader {
            Box::new(reader)
        } else {
            Box::new(File::open(path).with_context(shown)?)
        };
        let mut hasher = blake3::Hasher::new();
        buf.resize(CHUNK as usize, 0);
        loop {
            ctx.check()?;
            let n = file.read(&mut buf).with_context(shown)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            done += n as u64;
            ctx.progress("verify", done, total);
        }
        if hasher.finalize() != *hash {
            bail!("verify: {} differs from what was written", path.display());
        }
    }
    Ok(done)
}

/// Written files' folders on another device, each listed once by [`verify`]: one request per
/// folder, not one per file.
#[derive(Default)]
struct Listed(HashMap<PathBuf, HashMap<String, u64>>);

impl Listed {
    /// The size of the regular file at `path` by its folder's listing; `None` when the listing
    /// fails or doesn't show it as one (then a stat, as before, says why).
    fn size(&mut self, path: &Path) -> Option<u64> {
        let (dir, name) = (path.parent()?, path.file_name()?.to_str()?);
        let sizes = self.0.entry(dir.to_path_buf()).or_insert_with(|| {
            let listing = crate::remote::FlinkPath::parse(dir)
                .ok()
                .and_then(|at| crate::remote::get_for(&at).ok()?.list(&at).ok());
            (listing.into_iter().flatten())
                .filter(|e| !e.dir && !e.link && !e.special && !e.bad_name)
                .map(|e| (e.name, e.size))
                .collect()
        });
        sizes.get(name).copied()
    }
}
