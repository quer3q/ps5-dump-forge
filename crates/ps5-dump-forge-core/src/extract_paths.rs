//! Extract (a request with [`ConvertRequest::extract`]): files and folders of any source
//! copied into an existing folder on this computer, each under its own last component. Every
//! check runs before the first byte is written; nothing there is ever replaced. Each file goes
//! through an exclusive temporary name beside its target, synced, then renamed without
//! replacing; a selected folder is created exclusively first. BLAKE3 while writing; verify
//! checks each file's size, and with full verification re-reads and re-hashes it.
// ponytail: no rollback: a failure part way leaves the files already extracted (the error
// says how many); each file itself is atomic. No U5 sync cadence on the PS5: each file is
// synced once, whole.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::convert::{Kind, SourceStamp, open_source};
use crate::finalize::{fsync_dir, make_dir_open};
use crate::jobs::Ctx;
use crate::lz4_patch::write_temp_new;
use crate::preflight;
use crate::{ConvertRequest, Format, JobReport, VerifyMode, VerifySummary};

/// The stages an extract reports, in order.
pub(crate) const STAGES: [&str; 4] = ["scan", "preflight", "write", "verify"];
/// One read of a copy or of the check.
const CHUNK: u64 = 8 << 20;
/// Room beyond the copied bytes the destination must have.
const MARGIN: u64 = 1 << 20;

/// One selected path, resolved against the source's listing.
#[derive(Debug, PartialEq)]
struct Selection {
    /// The game path as selected.
    path: String,
    /// Its last component: the name it gets in the destination.
    name: String,
    /// A folder (else one file, `files[0]`).
    dir: bool,
    /// The files to copy, by game path.
    files: Vec<SourceFile>,
    /// Folders inside a selected folder, relative to it, parents first.
    dirs: Vec<String>,
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
fn select(
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
    if !req.edits.is_empty() || req.edit_rebuild || req.lz4.is_some() || req.lz4_in_place {
        findings.push("extract: a request to extract takes no edit or LZ4 options".to_string());
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
) -> Vec<String> {
    let mut findings = Vec::new();
    if kind == Kind::Folder
        && let Ok(source) = req.source.canonicalize()
        && root.starts_with(&source)
    {
        findings.push(format!(
            "extract: the destination {} is inside the game folder",
            root.display()
        ));
    }
    for s in selections {
        let target = root.join(&s.name);
        if target.symlink_metadata().is_ok() {
            findings.push(format!(
                "extract: {} already exists (never replaced)",
                target.display()
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
    let checked = preflight::destination(root, total.saturating_add(MARGIN), largest, ctx.cancel);
    findings.extend(checked.findings);
    for note in checked.notes {
        ctx.log(note);
    }
    #[cfg(not(target_os = "freebsd"))]
    findings.extend(crate::extract::extraction_findings(
        &files,
        &dirs,
        crate::extract::CASE_INSENSITIVE,
    ));
    // U2: names as the folder's filesystem folds them; with no probe, as if it folds.
    #[cfg(target_os = "freebsd")]
    {
        let (folds, ascii_only) =
            (checked.dest.as_ref()).map_or((true, false), |d| (d.folds_names, d.ascii_only));
        findings.extend(preflight::folder_names(&files, &dirs, folds, ascii_only));
    }
    findings
}

fn preflight_failed(findings: Vec<String>) -> anyhow::Error {
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
    let (mut tree, stamp) = match kind {
        Kind::Archive(format) => crate::archive::open(req, ctx, format, stamp)?,
        _ => (open_source(&req.source, kind, ctx.cancel)?, stamp),
    };
    ctx.log(format!("source: {}", tree.describe()));
    ctx.progress("scan", 1, 1);
    ctx.check()?;

    ctx.progress("preflight", 0, 1);
    let selections =
        select(tree.files(), tree.empty_dirs(), &req.extract).map_err(preflight_failed)?;
    let root =
        (req.output.canonicalize()).with_context(|| format!("opening {}", req.output.display()))?;
    let findings = destination_findings(req, kind, &root, &selections, ctx);
    ctx.check()?;
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    #[cfg(target_os = "freebsd")]
    {
        ctx.watch(&req.source);
        ctx.watch(&root);
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
    let mut done = 0u64;
    let outcome = write(
        tree.as_mut(),
        &root,
        &selections,
        total,
        &mut done,
        &mut written,
        ctx,
    )
    .and_then(|()| {
        ctx.progress("write", total, total);
        ctx.progress("verify", 0, 1);
        if !stamp.unchanged() {
            bail!("{} changed while it was read", req.source.display());
        }
        verify(&written, req.full_verify, ctx)
    });
    // A failure (or cancel) after some files were finished: they stay, and the log says so
    // too, since a cancelled job's error is only "cancelled".
    let checked = match (outcome, written.len()) {
        (Ok(checked), _) => checked,
        (Err(e), 0) => return Err(e),
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

/// A new folder at `path`, exclusively (an existing one fails), 0777 on the PS5.
fn make_dir(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir(path).with_context(|| format!("creating {}", path.display()))?;
    make_dir_open(path).with_context(|| format!("opening up {}", path.display()))
}

/// Copies every selection into `root`; `written` gets each finished file (its path, size and
/// BLAKE3), `done` the bytes copied.
fn write(
    tree: &mut dyn SourceTree,
    root: &Path,
    selections: &[Selection],
    total: u64,
    done: &mut u64,
    written: &mut Vec<(PathBuf, u64, blake3::Hash)>,
    ctx: &Ctx,
) -> anyhow::Result<()> {
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
        let mut synced = vec![root.to_path_buf()];
        if s.dir {
            make_dir(&target)?;
            for dir in &s.dirs {
                make_dir(&target.join(dir))?;
            }
            synced.push(target.clone());
            synced.extend(s.dirs.iter().map(|d| target.join(d)));
        }
        for file in &s.files {
            let rel = s.target(&file.path);
            let (parent, name) = rel.rsplit_once('/').unwrap_or(("", &rel));
            let dir = root.join(parent);
            let mut hasher = blake3::Hasher::new();
            let mut fill = |out: &mut File| -> io::Result<()> {
                copy(tree, file, out, &mut hasher, total, done, ctx)
            };
            write_temp_new(&dir, name, &mut fill)?;
            written.push((dir.join(name), file.size, hasher.finalize()));
        }
        for dir in synced.iter().rev() {
            fsync_dir(dir).with_context(|| format!("syncing {}", dir.display()))?;
        }
    }
    Ok(())
}

/// Streams `file` of `tree` into `out` in chunks, hashing it, checking cancel between chunks.
fn copy(
    tree: &mut dyn SourceTree,
    file: &SourceFile,
    out: &mut File,
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
fn verify(written: &[(PathBuf, u64, blake3::Hash)], full: bool, ctx: &Ctx) -> anyhow::Result<u64> {
    let total = if full {
        written
            .iter()
            .fold(0u64, |n, (_, size, _)| n.saturating_add(*size))
    } else {
        0
    };
    let mut done = 0u64;
    for (path, size, hash) in written {
        ctx.check()?;
        let shown = || path.display().to_string();
        let len = std::fs::symlink_metadata(path).with_context(shown)?.len();
        if len != *size {
            bail!(
                "verify: {} holds {len} bytes, {size} were written",
                path.display()
            );
        }
        if !full {
            continue;
        }
        let mut file = File::open(path).with_context(shown)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; CHUNK as usize];
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
