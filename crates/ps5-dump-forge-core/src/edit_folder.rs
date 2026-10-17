//! An edit of a plain game folder (a request with [`ConvertRequest::edits`] whose source is a
//! folder), applied straight into it by the user's explicit request (an exception to "never
//! touch the source", like the LZ4 folder patch). Every check runs before the first change:
//! the edits folded over the scanned folder ([`crate::edit_job::fold`], its AMPR index rule
//! included), the checks a conversion runs on the edited tree, the names a folder can hold and
//! free space for the added bytes. Then each op in order: a delete (`remove_file` /
//! `remove_dir_all`, never following a link), a rename ([`rename_no_replace`]), a new folder,
//! an add (each file through an exclusive temporary name beside its target, synced, renamed
//! over it). Verify re-reads every added file against what was copied, and the folder against
//! the planned listing; the touched folders are synced last.
// ponytail: no rollback for a folder: a failure part way leaves the earlier ops applied and says
// which; each file write itself is atomic (a temporary name renamed over the target).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use anyhow::{Context, bail};
use ps5_dump_forge_lz4::INDEX;
use ps5_dump_forge_patch::{Data, Op};
use ps5upload_fpkg::source::SourceTree;

use crate::edit::HostFile;
use crate::edit_job::{Folded, STAGES, fold, request_findings};
use crate::finalize::{FileId, followed_id, fsync_dir, handle_id, make_dir_open, path_id};
use crate::jobs::Ctx;
use crate::preflight;
use crate::scan::{ScannedFolder, Stamp, is_junk, open_nofollow};
use crate::{ConvertRequest, EditCheck, JobReport, VerifyMode, VerifySummary};

/// Room beyond the added bytes the folder must have: what the filesystem spends on them.
const MARGIN: u64 = 1 << 20;
/// One read of a copy or of the check.
const CHUNK: usize = 1 << 20;

/// A folder edit, checked: nothing changed yet.
pub(crate) struct Planned {
    /// The game folder, canonical (as the scan resolved it).
    root: PathBuf,
    /// What `root` named when it was planned (a link put there since is never followed).
    root_id: FileId,
    /// The ops in order, the fresh `ampr_emu.index` last when there is one.
    ops: Vec<Op>,
    /// The host files `Data::Source` ids name (by index).
    hosts: Vec<HostFile>,
    /// What the folder must list once edited: every (path, size) and the empty folders.
    files: Vec<(String, u64)>,
    empty_dirs: Vec<String>,
    notes: Vec<String>,
}

impl Planned {
    fn written(&self) -> u64 {
        (self.ops.iter())
            .map(|op| match op {
                Op::Put { len, .. } => *len,
                _ => 0,
            })
            .fold(0, u64::saturating_add)
    }
}

/// The job's timestamp in whole seconds, for a fresh `ampr_emu.index`.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Every check of a folder edit, changing nothing (on the PS5 the destination probe writes and
/// deletes its own probe file, as every write there does first). The inner `Err` lists every
/// finding; the outer one is a cancel or an error reading the folder.
pub(crate) fn plan(
    req: &ConvertRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<Result<Planned, Vec<String>>> {
    let mut findings = request_findings(req);
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    // Refuses links, special files and hard links anywhere in the folder.
    let tree = ScannedFolder::scan(&req.source, cancel)?;
    let root = tree.root().to_path_buf();
    let root_id = path_id(&root)
        .with_context(|| root.display().to_string())?
        .0;
    findings.extend(host_findings(&root, &req.edits));
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    let Folded {
        tree: mut expected,
        ops,
        hosts,
        mut notes,
        findings: folded,
    } = match fold(&root, Box::new(tree), req, now(), false, cancel)? {
        Ok(folded) => folded,
        Err(findings) => return Ok(Err(findings)),
    };
    findings.extend(folded);
    findings.extend(skipped_names(&ops));
    // The checks a conversion runs, on the edited tree: the game root, depth, names.
    let (_, input) = preflight::input(expected.as_mut());
    findings.extend(input);
    findings.extend(preflight::too_deep(expected.as_ref()));
    let paths: Vec<String> = expected.files().iter().map(|f| f.path.clone()).collect();
    #[cfg(not(target_os = "freebsd"))]
    findings.extend(crate::extract::extraction_findings(
        &paths,
        expected.empty_dirs(),
        crate::extract::CASE_INSENSITIVE,
    ));
    // A name an op makes that a later op takes away again is still made on the way.
    #[cfg(not(target_os = "freebsd"))]
    {
        let kept: BTreeSet<&str> = (paths.iter().chain(expected.empty_dirs()))
            .map(String::as_str)
            .collect();
        let gone: BTreeSet<String> = (new_paths(&ops).filter(|p| !kept.contains(p.as_str())))
            .cloned()
            .collect();
        let gone: Vec<String> = gone.into_iter().collect();
        findings.extend(crate::extract::extraction_findings(&[], &gone, false));
    }
    let (added, largest) = ops.iter().fold((0u64, 0u64), |(sum, max), op| match op {
        Op::Put { len, .. } => (sum.saturating_add(*len), max.max(*len)),
        _ => (sum, max),
    });
    let checked = preflight::destination(&root, added.saturating_add(MARGIN), largest, cancel);
    findings.extend(checked.findings);
    notes.extend(checked.notes);
    // U2: names as the folder's filesystem folds them; with no probe, as if it folds.
    #[cfg(target_os = "freebsd")]
    {
        let (folds, ascii_only) =
            (checked.dest.as_ref()).map_or((true, false), |d| (d.folds_names, d.ascii_only));
        findings.extend(preflight::folder_names(
            &paths,
            expected.empty_dirs(),
            folds,
            ascii_only,
        ));
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    if !findings.is_empty() {
        return Ok(Err(findings));
    }
    Ok(Ok(Planned {
        root,
        root_id,
        ops,
        hosts,
        files: (expected.files().iter())
            .map(|f| (f.path.clone(), f.size))
            .collect(),
        empty_dirs: expected.empty_dirs().to_vec(),
        notes,
    }))
}

/// Why an `Add.from` can't be copied into the game folder at `root`: it is inside it, or holds
/// it (compared by identity). A missing or linked `from` is left to [`crate::edit::Edited`].
fn host_findings(root: &Path, edits: &[crate::Edit]) -> Vec<String> {
    let Ok(game) = followed_id(root) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for edit in edits {
        let crate::Edit::Add { from, .. } = edit else {
            continue;
        };
        let linked = std::fs::symlink_metadata(from).map_or(true, |m| m.file_type().is_symlink());
        let Some(canon) = (!linked).then(|| from.canonicalize().ok()).flatten() else {
            continue;
        };
        let id = followed_id(&canon).ok();
        if canon
            .ancestors()
            .any(|a| followed_id(a).is_ok_and(|a| a == game))
        {
            findings.push(format!("{} is inside the game folder", from.display()));
        } else if id
            .is_some_and(|id| (root.ancestors()).any(|a| followed_id(a).is_ok_and(|a| a == id)))
        {
            findings.push(format!("{} holds the game folder", from.display()));
        }
    }
    findings
}

/// The new names a folder scan would skip (junk such as `.DS_Store`): written, they would
/// never read back as part of the game.
fn skipped_names(ops: &[Op]) -> Vec<String> {
    let mut findings: Vec<String> = new_paths(ops)
        .filter_map(|path| {
            let junk = path.split('/').find(|c| is_junk(c))?;
            Some(format!(
                "{path}: a game folder can't hold {junk} (Forge skips that name when it reads one)"
            ))
        })
        .collect();
    findings.dedup();
    findings
}

/// Every path an op makes (a file, a folder, a rename's target).
fn new_paths(ops: &[Op]) -> impl Iterator<Item = &String> {
    ops.iter().filter_map(|op| match op {
        Op::Put { path, .. } | Op::Mkdir { path } => Some(path),
        Op::Rename { to, .. } => Some(to),
        Op::Delete { .. } => None,
    })
}

/// One op, as the log and a failure name it.
fn label(op: &Op) -> String {
    match op {
        Op::Delete { path } => format!("deleted {path}"),
        Op::Rename { from, to } => format!("renamed {from} to {to}"),
        Op::Mkdir { path } => format!("made folder {path}"),
        Op::Put { path, len, .. } => format!("wrote {path} ({len} bytes)"),
    }
}

/// [`crate::Jobs::edit_check`] for a folder: it is always changed directly, so only the
/// in-place list can hold findings, and no sizes are given.
pub(crate) fn check(req: &ConvertRequest) -> EditCheck {
    let cancel = AtomicBool::new(false);
    let in_place = match plan(req, &cancel) {
        Ok(Ok(_)) => Vec::new(),
        Ok(Err(findings)) => findings,
        Err(e) => vec![format!("{e:#}")],
    };
    EditCheck {
        in_place,
        ..EditCheck::default()
    }
}

/// The folder edit job: scan + preflight ([`plan`]), the ops, their check, the folder syncs.
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    ctx.stages(&STAGES);
    ctx.progress("scan", 0, 1);
    let planned = plan(req, ctx.cancel)?
        .map_err(|f| anyhow::anyhow!("preflight failed:\n  {}", f.join("\n  ")))?;
    ctx.log(format!(
        "source: {} (a game folder, edited directly)",
        planned.root.display()
    ));
    ctx.progress("scan", 1, 1);
    ctx.progress("preflight", 0, 1);
    for note in &planned.notes {
        ctx.log(note.clone());
    }
    let written = planned.written();
    ctx.log(format!(
        "edit: {} changes, {written} bytes to write",
        planned.ops.len()
    ));
    ctx.progress("preflight", 1, 1);
    ctx.check()?;
    // Writing, then reading back what was written.
    ctx.expect_rest(written.saturating_mul(2));
    let mut work = Work {
        root: &planned.root,
        root_id: planned.root_id,
        partly: false,
        hosts: &planned.hosts,
        ctx,
        done: 0,
        total: written,
        added: BTreeMap::new(),
        touched: BTreeSet::new(),
    };
    ctx.progress("write", 0, written.max(1));
    let total = planned.ops.len();
    for (i, op) in planned.ops.iter().enumerate() {
        work.partly = false;
        let applied = ctx.check().and_then(|()| work.op(op));
        if let Err(e) = applied {
            let before: Vec<String> = planned.ops[..i].iter().map(label).collect();
            let what = match ctx.cancel.load(Ordering::Relaxed) {
                true => "cancelled".to_string(),
                false => format!("{e:#}"),
            };
            let mut applied = match (before.is_empty(), work.partly) {
                (true, false) => "nothing was changed".to_string(),
                (true, true) => "no change was applied before it".to_string(),
                (false, _) => format!(
                    "{i} of {total} changes were applied before it and stay: {}",
                    before.join("; ")
                ),
            };
            if work.partly {
                applied.push_str(
                    "; the failed change is partly applied (folders it made, or what it \
                     deleted of a folder, stay)",
                );
            }
            // What was written must still survive a power loss.
            let _ = work.sync_touched();
            bail!("{what}; {applied}");
        }
        ctx.log(format!("edit: {}", label(op)));
    }
    ctx.progress("write", written, written.max(1));
    // Past this point every change is in the folder: an error must say so.
    let after = |e: anyhow::Error| {
        let what = match ctx.cancel.load(Ordering::Relaxed) {
            true => "cancelled".to_string(),
            false => format!("{e:#}"),
        };
        anyhow::anyhow!("{what}; all {total} changes were applied and stay")
    };
    let checks = work.verify(&planned).map_err(after)?;
    ctx.progress("finalize", 0, 1);
    work.sync_touched().map_err(after)?;
    ctx.progress("finalize", 1, 1);
    let mut checks = checks;
    checks.push(format!("edited {} directly", planned.root.display()));
    Ok(JobReport {
        output: req.source.clone(),
        bytes: written,
        files: planned.files.len() as u64,
        checks,
        // Every added byte is read back.
        verify: VerifySummary {
            mode: VerifyMode::Full,
            checked_bytes: written,
            total_bytes: written,
            samples: 0,
            seed: 0,
        },
    })
}

/// The ops being applied, and what they left: each added file's final path with its length
/// and the hash of what was copied, and the folders whose entries changed.
struct Work<'a> {
    root: &'a Path,
    root_id: FileId,
    /// The current op changed something before it failed: a folder it made, or the start of
    /// a folder's delete.
    partly: bool,
    hosts: &'a [HostFile],
    ctx: &'a Ctx<'a>,
    /// Bytes written so far, of `total`.
    done: u64,
    total: u64,
    added: BTreeMap<String, (u64, blake3::Hash)>,
    touched: BTreeSet<PathBuf>,
}

impl Work<'_> {
    fn op(&mut self, op: &Op) -> anyhow::Result<()> {
        // The game folder itself replaced (by a link elsewhere, say) since it was planned.
        match path_id(self.root) {
            Ok((id, kind)) if id == self.root_id && kind.is_dir() => {}
            _ => bail!("{} was moved or replaced", self.root.display()),
        }
        match op {
            Op::Delete { path } => {
                let target = self.under(path)?;
                let meta = std::fs::symlink_metadata(&target)
                    .with_context(|| format!("deleting {}", target.display()))?;
                // A link is never followed (nor deleted: the scan saw none, so it is new).
                if meta.is_dir() {
                    self.partly = true;
                    std::fs::remove_dir_all(&target)
                } else if meta.is_file() {
                    std::fs::remove_file(&target)
                } else {
                    bail!("{} is no longer a file or folder", target.display());
                }
                .with_context(|| format!("deleting {}", target.display()))?;
                self.touch(&target);
                self.moved(path, None);
            }
            Op::Rename { from, to } => {
                let source = self.under(from)?;
                let meta = std::fs::symlink_metadata(&source)
                    .with_context(|| format!("renaming {}", source.display()))?;
                if !meta.is_file() && !meta.is_dir() {
                    bail!("{} is no longer a file or folder", source.display());
                }
                if let Some((dir, _)) = to.rsplit_once('/') {
                    self.make_dirs(dir)?;
                }
                let target = self.under(to)?;
                crate::finalize::rename_no_replace(&source, &target).with_context(|| {
                    format!("renaming {} to {}", source.display(), target.display())
                })?;
                self.touch(&source);
                self.touch(&target);
                self.moved(from, Some(to));
            }
            Op::Mkdir { path } => self.make_dirs(path)?,
            Op::Put { path, len, data } => self.put(path, *len, data)?,
        }
        Ok(())
    }

    /// `rel` in the game folder, every folder on the way checked to be a real folder (never a
    /// link) or not there yet.
    fn under(&self, rel: &str) -> anyhow::Result<PathBuf> {
        let mut at = self.root.to_path_buf();
        let mut parts = rel.split('/').peekable();
        while let Some(part) = parts.next() {
            push(&mut at, part)?;
            if parts.peek().is_none() {
                break;
            }
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => bail!("{} is no longer a folder", at.display()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| at.display().to_string()),
            }
        }
        Ok(at)
    }

    /// Folder `rel` and its parents, each made where missing (0777 on the PS5).
    fn make_dirs(&mut self, rel: &str) -> anyhow::Result<()> {
        let mut at = self.root.to_path_buf();
        for part in rel.split('/') {
            push(&mut at, part)?;
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.is_dir() => continue,
                Ok(_) => bail!("{} is not a folder", at.display()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| at.display().to_string()),
            }
            std::fs::create_dir(&at).with_context(|| format!("creating {}", at.display()))?;
            self.partly = true;
            make_dir_open(&at).with_context(|| format!("setting the mode of {}", at.display()))?;
            self.touch(&at);
        }
        Ok(())
    }

    /// Adds or replaces file `rel` through a temporary name beside it.
    fn put(&mut self, rel: &str, len: u64, data: &Data) -> anyhow::Result<()> {
        let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
        if !dir.is_empty() {
            self.make_dirs(dir)?;
        }
        let target = self.under(rel)?;
        let found = std::fs::symlink_metadata(&target);
        if found.as_ref().is_ok_and(|m| m.is_dir()) {
            bail!("{} is a folder now", target.display());
        }
        // On a volume that folds case or normalization another file can answer to `name`;
        // the replacing rename below would overwrite it.
        if found.is_ok() && !spelled(target.parent().expect("under the game folder"), name)? {
            bail!(
                "{}: another file in its folder answers to this name (the volume ignores case \
                 or normalization); it is not replaced",
                target.display()
            );
        }
        // An old ampr_emu.index's journal and logs, deleted before it, must stay deleted
        // beside the new one after a power loss: their ids would name its records.
        if rel == INDEX {
            self.sync_touched()?;
        }
        let folder = target
            .parent()
            .expect("under the game folder")
            .to_path_buf();
        let mut hasher = blake3::Hasher::new();
        let (hosts, ctx, total) = (self.hosts, self.ctx, self.total);
        let done = &mut self.done;
        crate::lz4_patch::write_temp(&folder, name, &mut |file| {
            hasher = blake3::Hasher::new();
            let mut emit = |chunk: &[u8], file: &mut File| -> io::Result<()> {
                if ctx.cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::other("cancelled"));
                }
                file.write_all(chunk)?;
                hasher.update(chunk);
                *done = done.saturating_add(chunk.len() as u64);
                ctx.progress("write", *done, total.max(1));
                Ok(())
            };
            match data {
                Data::Bytes(bytes) => bytes.chunks(CHUNK).try_for_each(|c| emit(c, file)),
                Data::Zero(n) => {
                    let zeros = vec![0u8; CHUNK.min(usize::try_from(*n).unwrap_or(CHUNK))];
                    let mut left = *n;
                    while left > 0 {
                        let step = zeros.len().min(usize::try_from(left).unwrap_or(CHUNK));
                        emit(&zeros[..step], file)?;
                        left -= step as u64;
                    }
                    Ok(())
                }
                Data::Source { id, offset, len } => {
                    let host = (hosts.get(*id as usize))
                        .ok_or_else(|| io::Error::other(format!("no host file {id}")))?;
                    copy_host(host, *offset, *len, &mut |c| emit(c, file))
                }
            }
        })?;
        self.touch(&target);
        self.added.insert(rel.to_string(), (len, hasher.finalize()));
        Ok(())
    }

    /// Records that `path`'s folder changed.
    fn touch(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.touched.insert(parent.to_path_buf());
        }
    }

    /// Follows the added files through a delete (`to` = `None`) or a rename of `from`.
    fn moved(&mut self, from: &str, to: Option<&str>) {
        let inside = format!("{from}/");
        let hit: Vec<String> = (self.added.keys())
            .filter(|k| *k == from || k.starts_with(&inside))
            .cloned()
            .collect();
        for key in hit {
            let entry = self.added.remove(&key).expect("listed above");
            if let Some(to) = to {
                self.added
                    .insert(format!("{to}{}", &key[from.len()..]), entry);
            }
        }
    }

    /// Syncs every folder whose entries changed (one a later op removed is skipped), the game
    /// folder last: on macOS its F_FULLFSYNC flushes the drive's cache for all of them.
    fn sync_touched(&mut self) -> anyhow::Result<()> {
        let root = self.root.to_path_buf();
        for dir in std::mem::take(&mut self.touched) {
            if dir == root {
                continue;
            }
            match fsync_dir(&dir) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                other => other.with_context(|| format!("syncing {}", dir.display()))?,
            }
        }
        fsync_dir(&root).with_context(|| format!("syncing {}", root.display()))
    }

    /// Every added file re-read against what was copied, then the folder against the planned
    /// listing. A failure leaves the changes applied (there is no rollback) and says so.
    fn verify(&mut self, planned: &Planned) -> anyhow::Result<Vec<String>> {
        let ctx = self.ctx;
        let failed =
            |e: anyhow::Error| anyhow::anyhow!("the edit did not verify (the changes stay): {e:#}");
        let total = self.total.max(1);
        let mut done = 0u64;
        ctx.progress("verify", 0, total);
        let mut buf = vec![0u8; CHUNK];
        for (rel, (len, hash)) in &self.added {
            ctx.check()?;
            let path = self.under(rel).map_err(failed)?;
            let mut read = || -> anyhow::Result<blake3::Hash> {
                let mut file = open_nofollow(&path)?;
                let meta = file.metadata()?;
                if !meta.is_file() || meta.len() != *len {
                    bail!("{} holds {} bytes, not {len}", path.display(), meta.len());
                }
                let mut hasher = blake3::Hasher::new();
                loop {
                    let n = file.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                    done = done.saturating_add(n as u64);
                    ctx.progress("verify", done, total);
                }
                Ok(hasher.finalize())
            };
            let got = read().with_context(|| format!("reading {}", path.display()));
            if got.map_err(failed)? != *hash {
                return Err(failed(anyhow::anyhow!(
                    "{rel} does not hold what was copied"
                )));
            }
        }
        let scanned = ScannedFolder::scan(self.root, ctx.cancel).map_err(failed)?;
        let files: Vec<(String, u64)> = (scanned.files().iter())
            .map(|f| (f.path.clone(), f.size))
            .collect();
        if files != planned.files || scanned.empty_dirs() != planned.empty_dirs {
            let want: BTreeSet<_> = planned.files.iter().collect();
            let got: BTreeSet<_> = files.iter().collect();
            let mut diff: Vec<String> = (want.difference(&got))
                .map(|(p, s)| format!("missing {p} ({s} bytes)"))
                .chain(
                    got.difference(&want)
                        .map(|(p, s)| format!("unexpected {p} ({s} bytes)")),
                )
                .take(10)
                .collect();
            if diff.is_empty() {
                diff.push("its empty folders differ".into());
            }
            return Err(failed(anyhow::anyhow!(
                "{} does not list what the edit planned: {}",
                self.root.display(),
                diff.join(", ")
            )));
        }
        ctx.progress("verify", total, total);
        Ok(vec![
            format!(
                "blake3: {} added files ({} bytes) read back and matched",
                self.added.len(),
                self.total
            ),
            format!(
                "the folder lists the {} files and {} empty folders planned",
                files.len(),
                planned.empty_dirs.len()
            ),
        ])
    }
}

/// Whether `folder` lists an entry spelled exactly `name`.
fn spelled(folder: &Path, name: &str) -> anyhow::Result<bool> {
    let entries = std::fs::read_dir(folder).with_context(|| folder.display().to_string())?;
    for entry in entries {
        let entry = entry.with_context(|| folder.display().to_string())?;
        if entry.file_name() == std::ffi::OsStr::new(name) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `part` of a game path onto `at`, refused unless it is one plain name: a drive or a root
/// (`C:` on Windows) would replace `at` and lead out of the game folder.
fn push(at: &mut PathBuf, part: &str) -> anyhow::Result<()> {
    let mut parts = Path::new(part).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(name)), None) if name == std::ffi::OsStr::new(part) => {
            at.push(part);
            Ok(())
        }
        _ => bail!("{part} is not a plain name"),
    }
}

/// Copies `len` bytes at `offset` of host file `host` into `emit`, which takes each chunk;
/// `host` must still be the file the edit was planned with (identity, size, modification
/// time), before and after.
fn copy_host(
    host: &HostFile,
    offset: u64,
    len: u64,
    emit: &mut dyn FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let shown = host.path.display();
    let with_path = |e: io::Error| io::Error::new(e.kind(), format!("{shown}: {e}"));
    let changed = || {
        io::Error::other(format!(
            "{shown} changed after it was added (replaced, or its size or modification time); \
             add it again"
        ))
    };
    if offset.saturating_add(len) > host.size {
        return Err(io::Error::other(format!(
            "{shown}: a read past its {} bytes",
            host.size
        )));
    }
    let named = path_id(&host.path).map_err(with_path)?.0 == host.id;
    let mut file = open_nofollow(&host.path).map_err(with_path)?;
    let same = |file: &File| -> io::Result<bool> {
        let meta = file.metadata()?;
        Ok(handle_id(file)? == host.id && meta.is_file() && Stamp::of(&meta) == host.stamp)
    };
    if !named || !same(&file).map_err(with_path)? {
        return Err(changed());
    }
    file.seek(SeekFrom::Start(offset)).map_err(with_path)?;
    let mut buf = vec![0u8; CHUNK.min(usize::try_from(len).unwrap_or(CHUNK)).max(1)];
    let mut left = len;
    while left > 0 {
        let step = buf.len().min(usize::try_from(left).unwrap_or(CHUNK));
        file.read_exact(&mut buf[..step]).map_err(with_path)?;
        emit(&buf[..step])?;
        left -= step as u64;
    }
    // Nor while it was copied.
    if !same(&file).map_err(with_path)? {
        return Err(changed());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{Edit, Event};

    const PARAM: &[u8] =
        br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#;
    const AMPR_EBOOT: &[u8] = b"\x7fELF fake eboot importing libSceAmpr.sprx";

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A small game folder `dir/game`.
    fn game(dir: &Path, eboot: &[u8], extra: &[(&str, &[u8])]) -> PathBuf {
        let root = dir.join("game");
        put(&root, "eboot.bin", eboot);
        put(&root, "sce_sys/param.json", PARAM);
        put(&root, "data/a.bin", &[7u8; 70_000]);
        put(&root, "data/sub/b.bin", b"bee");
        std::fs::create_dir_all(root.join("data/empty")).unwrap();
        for (rel, bytes) in extra {
            put(&root, rel, bytes);
        }
        root
    }

    fn request(source: &Path, edits: Vec<Edit>) -> ConvertRequest {
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": source, "format": "exfat", "output": source.with_extension("exfat")
        }))
        .unwrap();
        req.edits = edits;
        req
    }

    /// Runs the job with `on` seeing every event (and the cancel flag).
    fn run_with(
        req: &ConvertRequest,
        on: impl Fn(&Event, &AtomicBool) + Send + Sync + 'static,
    ) -> anyhow::Result<JobReport> {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| on(&e, &flag));
        run(req, &Ctx::new(1, &emit, &cancel))
    }

    /// Every entry under `root` (hidden ones too, nothing skipped): files with their bytes,
    /// folders as `None`.
    fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
        let mut all = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if path.is_dir() {
                    all.insert(rel, None);
                    stack.push(path);
                } else {
                    all.insert(rel, Some(std::fs::read(&path).unwrap()));
                }
            }
        }
        all
    }

    fn mkdir(path: &str) -> Edit {
        Edit::Mkdir { path: path.into() }
    }

    fn add(from: &Path, to: &str) -> Edit {
        Edit::Add {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn every_op_is_applied_and_checked() {
        let dir = crate::test_dir("edit-folder-ops");
        let root = game(&dir, b"\x7fELF plain eboot", &[]);
        let host = dir.join("host");
        put(&host, "new.bin", &[9u8; 3 << 20]);
        put(&host, "pack/x.txt", b"x");
        put(&host, "bee.bin", b"a new bee");
        std::fs::create_dir_all(host.join("pack/void")).unwrap();
        let edits = vec![
            add(&host.join("new.bin"), "data/new/new.bin"),
            add(&host.join("pack"), "data/pack"),
            add(&host.join("bee.bin"), "data/sub/b.bin"),
            Edit::Delete {
                path: "data/empty".into(),
            },
            Edit::Rename {
                from: "data/a.bin".into(),
                to: "data/renamed.bin".into(),
            },
            // An added file followed through a rename of its folder.
            Edit::Rename {
                from: "data/new".into(),
                to: "data/moved".into(),
            },
            mkdir("extra/deep"),
        ];
        let req = request(&root, edits);
        let check = crate::jobs::Inner::new(Box::new(|_| {})).edit_check(&req);
        assert_eq!(check, EditCheck::default(), "{check:?}");
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = logs.clone();
        let report = run_with(&req, move |e, _| {
            if let Event::Log { line, .. } = e {
                seen.lock().unwrap().push(line.clone());
            }
        })
        .unwrap_or_else(|e| panic!("{e:#}"));
        assert_eq!(report.output, root);
        assert_eq!(report.files, 6);
        assert_eq!(report.bytes, (3 << 20) + 1 + 9);
        assert_eq!(report.verify.mode, VerifyMode::Full);
        assert!(
            report.checks[0].starts_with("blake3: 3 added files"),
            "{report:?}"
        );
        let logs = logs.lock().unwrap();
        for want in [
            "edit: deleted data/empty",
            "edit: renamed data/new to data/moved",
        ] {
            assert!(logs.iter().any(|l| l == want), "{want}: {logs:?}");
        }
        let mut want: BTreeMap<String, Option<Vec<u8>>> = [
            ("data", None),
            ("data/moved", None),
            ("data/moved/new.bin", Some(vec![9u8; 3 << 20])),
            ("data/pack", None),
            ("data/pack/void", None),
            ("data/pack/x.txt", Some(b"x".to_vec())),
            ("data/renamed.bin", Some(vec![7u8; 70_000])),
            ("data/sub", None),
            ("data/sub/b.bin", Some(b"a new bee".to_vec())),
            ("eboot.bin", Some(b"\x7fELF plain eboot".to_vec())),
            ("extra", None),
            ("extra/deep", None),
            ("sce_sys", None),
            ("sce_sys/param.json", Some(PARAM.to_vec())),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), b))
        .collect();
        // No temporary file is left.
        assert_eq!(snapshot(&root), want);
        // The host files are untouched.
        assert_eq!(std::fs::read(host.join("bee.bin")).unwrap(), b"a new bee");
        // A second edit on the edited folder.
        let again = request(
            &root,
            vec![Edit::Delete {
                path: "data/moved".into(),
            }],
        );
        run_with(&again, |_, _| {}).unwrap();
        want.remove("data/moved");
        want.remove("data/moved/new.bin");
        assert_eq!(snapshot(&root), want);
    }

    #[test]
    fn a_refusal_changes_nothing() {
        let dir = crate::test_dir("edit-folder-refused");
        let root = game(&dir, b"\x7fELF plain eboot", &[]);
        put(&dir, "host.bin", b"host");
        std::fs::create_dir_all(dir.join("hostdir")).unwrap();
        let slash = PathBuf::from(format!("{}/", dir.join("hostdir").display()));
        let before = snapshot(&root);
        #[allow(unused_mut)]
        let mut cases: Vec<(Vec<Edit>, &str)> = vec![
            // `link/` would follow a link at `link`.
            (
                vec![add(&slash, "x")],
                "does not end in a file or folder name",
            ),
            (
                vec![
                    add(&dir.join("host.bin"), "data/host.bin"),
                    Edit::Delete {
                        path: "nope".into(),
                    },
                ],
                "delete nope: nope is not in the game",
            ),
            (
                vec![add(&root.join("data/a.bin"), "copy.bin")],
                "is inside the game folder",
            ),
            (vec![add(&dir, "all")], "holds the game folder"),
            (
                vec![add(&dir.join("host.bin"), "data/.DS_Store")],
                "a game folder can't hold .DS_Store",
            ),
            (
                vec![Edit::Delete {
                    path: "sce_sys/param.json".into(),
                }],
                "sce_sys/param.json is missing",
            ),
            (
                vec![Edit::Rename {
                    from: "data/a.bin".into(),
                    to: "data/sub".into(),
                }],
                "data/sub already exists",
            ),
        ];
        // A name made on the way and deleted again is still checked (`C:` would leave the
        // game folder on Windows).
        #[cfg(not(target_os = "freebsd"))]
        cases.push((
            vec![mkdir("x:/y"), Edit::Delete { path: "x:".into() }],
            "x:/y (':' in name",
        ));
        for (edits, want) in cases {
            let req = request(&root, edits);
            let err = format!("{:#}", run_with(&req, |_, _| {}).unwrap_err());
            assert!(err.starts_with("preflight failed:"), "{err}");
            assert!(err.contains(want), "{want}: {err}");
            let check = check(&req);
            assert!(check.in_place.iter().any(|f| f.contains(want)), "{check:?}");
            assert!(check.rebuild.is_empty() && check.rebuild_needs.is_none());
            assert_eq!(snapshot(&root), before);
        }
        let mut req = request(&root, vec![mkdir("x")]);
        req.lz4_in_place = true;
        req.remove_backport = true;
        let err = format!("{:#}", run_with(&req, |_, _| {}).unwrap_err());
        assert!(
            err.contains("lz4_in_place") && err.contains("remove_backport"),
            "{err}"
        );
        assert_eq!(snapshot(&root), before);
    }

    #[test]
    fn a_cancel_mid_way_reports_the_applied_ops() {
        let dir = crate::test_dir("edit-folder-cancel");
        let root = game(&dir, b"\x7fELF plain eboot", &[]);
        // Between two ops.
        let req = request(&root, vec![mkdir("a"), mkdir("b"), mkdir("c")]);
        let err = run_with(&req, |e, cancel| {
            if matches!(e, Event::Log { line, .. } if line == "edit: made folder b") {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "cancelled; 2 of 3 changes were applied before it and stay: made folder a; made \
             folder b"
        );
        assert!(root.join("a").is_dir() && root.join("b").is_dir());
        assert!(!root.join("c").exists());
        // In the middle of a file: its temporary file goes, the target never appears.
        put(&dir, "big.bin", &[1u8; 5 << 20]);
        let before = snapshot(&root);
        let req = request(
            &root,
            vec![mkdir("d"), add(&dir.join("big.bin"), "data/big.bin")],
        );
        // ponytail: progress events are throttled, so a mid-copy cancel can't be aimed from a
        // test; this cancels once writing is done, during verify.
        let err = run_with(&req, |e, cancel| {
            if matches!(e, Event::Progress { stage, .. } if stage == "verify") {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "cancelled; all 2 changes were applied and stay"
        );
        assert!(root.join("d").is_dir() && root.join("data/big.bin").is_file());
        assert!(!snapshot(&root).keys().any(|k| k.contains(".forge-")));
        let _ = before;
    }

    #[cfg(unix)]
    #[test]
    fn a_game_folder_replaced_by_a_link_is_not_followed() {
        let dir = crate::test_dir("edit-folder-root-link");
        let root = game(&dir, b"\x7fELF plain eboot", &[]);
        let outside = dir.join("outside");
        put(&outside, "data/a.bin", b"outside");
        let req = request(
            &root,
            vec![
                mkdir("x"),
                Edit::Delete {
                    path: "data/a.bin".into(),
                },
            ],
        );
        let (from, aside, to) = (root.clone(), dir.join("aside"), outside.clone());
        let err = run_with(&req, move |e, _| {
            if matches!(e, Event::Log { line, .. } if line == "edit: made folder x") {
                std::fs::rename(&from, &aside).unwrap();
                std::os::unix::fs::symlink(&to, &from).unwrap();
            }
        })
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("was moved or replaced"),
            "{err:#}"
        );
        assert_eq!(
            std::fs::read(outside.join("data/a.bin")).unwrap(),
            b"outside"
        );
    }

    #[test]
    fn an_ampr_title_gets_a_fresh_index() {
        let dir = crate::test_dir("edit-folder-ampr");
        let journal = ps5_dump_forge_lz4::JOURNAL;
        let log = ps5_dump_forge_lz4::LOGS[0];
        let root = game(
            &dir,
            AMPR_EBOOT,
            &[(journal, b"old ids"), (log, b"old log")],
        );
        // A mkdir changes no file: no index, the journal and log stay.
        run_with(&request(&root, vec![mkdir("x")]), |_, _| {}).unwrap();
        assert!(!root.join(INDEX).exists());
        assert!(root.join(journal).exists() && root.join(log).exists());
        put(&dir, "host.bin", b"host");
        let req = request(&root, vec![add(&dir.join("host.bin"), "data/host.bin")]);
        let report = run_with(&req, |_, _| {}).unwrap_or_else(|e| panic!("{e:#}"));
        assert!(!root.join(journal).exists() && !root.join(log).exists());
        let mut tree = ScannedFolder::scan(&root, &AtomicBool::new(false)).unwrap();
        let listing = crate::lz4::plain_listing(tree.files(), crate::lz4::RuntimeAction::Keep);
        assert!(listing.iter().any(|f| f.path == "data/host.bin"));
        assert!(matches!(
            crate::lz4::index_state(&mut tree, &listing).unwrap(),
            crate::lz4::IndexState::Ok(_)
        ));
        // The index is written and checked like an added file.
        assert!(
            report.checks[0].starts_with("blake3: 2 added files"),
            "{report:?}"
        );
        // The runtime is never swapped: there is none, and none is installed.
        assert!(!root.join(ps5_dump_forge_lz4::RUNTIME).exists());
    }
}
