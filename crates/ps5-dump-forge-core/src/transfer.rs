//! Transfer (a request with [`ConvertRequest::transfer`]): files and folders of this device, a
//! paired one (`flink://`) or a NAS (`smb://`) copied into a folder on this device or a paired
//! one, each under its own name, as Extract copies (`extract_paths`): every check before the
//! first byte, nothing replaced (unless [`ConvertRequest::replace`], as Extract replaces), each
//! file through an exclusive temporary name, synced, renamed without replacing; folders
//! recreated, empty ones too; BLAKE3 while writing, sizes checked
//! (fast) or every file re-read and re-hashed (full). Peer to peer is relayed through here.
// ponytail: no rollback, as Extract: a failure part way leaves the files already copied (the
// error says how many).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use ps5upload_fpkg::ReadSeek;
use ps5upload_fpkg::source::{SourceFile, SourceTree};

use crate::convert::{Kind, SourceStamp, open_source};
use crate::extract_paths::{
    CHUNK, Selection, Written, log_replaced, made_note, preflight_failed, select, target_findings,
    verify, write,
};
use crate::jobs::Ctx;
use crate::remote::{FlinkPath, RemoteDir, is_remote};
use crate::{ConvertRequest, JobReport, VerifyMode, VerifySummary};

/// The stages a transfer reports, in order.
pub(crate) const STAGES: [&str; 4] = crate::extract_paths::STAGES;

/// The name `path` gets in the destination: its last component (on a peer or a share, of the
/// path there).
pub(crate) fn name_of(path: &Path) -> Option<String> {
    let inner = if is_remote(path) {
        PathBuf::from(FlinkPath::parse(path).ok()?.path)
    } else {
        path.to_path_buf()
    };
    inner.file_name()?.to_str().map(str::to_string)
}

/// Where each source of `req` lands (the output itself for one with no name).
pub(crate) fn targets(req: &ConvertRequest) -> Vec<PathBuf> {
    (req.transfer.iter())
        .map(|s| name_of(s).map_or_else(|| req.output.clone(), |n| req.output.join(n)))
        .collect()
}

/// What the request itself can't be, before anything is read.
fn request_findings(req: &ConvertRequest) -> Vec<String> {
    let mut findings = Vec::new();
    if !req.edits.is_empty()
        || !req.extract.is_empty()
        || req.edit_rebuild
        || req.lz4.is_some()
        || req.zstd_pack
        || req.zstd_unpack
    {
        findings
            .push("transfer: a transfer takes no edit, extract, LZ4 or zstd options".to_string());
    }
    #[cfg(target_os = "freebsd")]
    findings.extend(crate::edit_job::smp_inside(&req.output).map(|why| format!("transfer: {why}")));
    if is_remote(&req.output) {
        if let Err(e) = RemoteDir::open(&req.output) {
            findings.push(format!("transfer: {e:#}"));
        }
    } else if let Some(why) = crate::net::output_refusal(&req.output) {
        findings.push(format!("transfer: {why}"));
    } else {
        match std::fs::metadata(&req.output) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => findings.push(format!(
                "transfer: {} is not a folder",
                req.output.display()
            )),
            Err(e) => findings.push(format!("transfer: {}: {e}", req.output.display())),
        }
    }
    for source in &req.transfer {
        if name_of(source).is_none() {
            findings.push(format!(
                "transfer: {} can't be copied as a whole: choose what is inside it",
                source.display()
            ));
        }
        // Local: never a link or a special file (the folder scanner refuses those inside).
        if !crate::net::is_network(source) {
            match std::fs::symlink_metadata(source) {
                Ok(m) if m.is_symlink() => findings.push(format!(
                    "transfer: {} is a link (never followed)",
                    source.display()
                )),
                Ok(m) if !m.is_file() && !m.is_dir() => findings.push(format!(
                    "transfer: {} is not a file or folder",
                    source.display()
                )),
                Ok(_) => {}
                Err(e) => findings.push(format!("transfer: {}: {e}", source.display())),
            }
        }
        if inside(&req.output, source) {
            findings.push(format!(
                "transfer: the destination {} is inside {}",
                req.output.display(),
                source.display()
            ));
        }
    }
    findings
}

/// Whether `output` is at or inside `source`: on one device as spelled (normalized), locally
/// by the canonical paths; never across devices.
fn inside(output: &Path, source: &Path) -> bool {
    if is_remote(output) || is_remote(source) {
        return match (FlinkPath::parse(output), FlinkPath::parse(source)) {
            (Ok(o), Ok(s)) => o.within(&s),
            _ => false,
        };
    }
    match (dunce::canonicalize(output), dunce::canonicalize(source)) {
        (Ok(o), Ok(s)) => o.starts_with(s),
        _ => false,
    }
}

/// A source folder's tree with every path under `<name>/`, so the folder is one selection.
struct Rooted {
    tree: Box<dyn SourceTree>,
    prefix: String,
    files: Vec<SourceFile>,
    dirs: Vec<String>,
}

impl Rooted {
    fn new(tree: Box<dyn SourceTree>, name: &str) -> Self {
        let prefix = format!("{name}/");
        let files = (tree.files().iter())
            .map(|f| SourceFile {
                path: format!("{prefix}{}", f.path),
                size: f.size,
            })
            .collect();
        // The folder itself, so an empty one is selected too.
        let dirs = std::iter::once(name.to_string())
            .chain(tree.empty_dirs().iter().map(|d| format!("{prefix}{d}")))
            .collect();
        Self {
            tree,
            prefix,
            files,
            dirs,
        }
    }

    fn inner<'a>(&self, path: &'a str) -> &'a str {
        path.strip_prefix(&self.prefix).unwrap_or(path)
    }
}

impl SourceTree for Rooted {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let path = self.inner(path);
        self.tree.read(path)
    }

    fn read_range(
        &mut self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> ps5upload_fpkg::Result<Vec<u8>> {
        let path = self.inner(path);
        self.tree.read_range(path, offset, len)
    }

    fn empty_dirs(&self) -> &[String] {
        &self.dirs
    }

    fn describe(&self) -> String {
        self.tree.describe()
    }
}

/// One source file as a tree of its own, named `<name>`. Its handle is opened at the first read
/// and closed at the file's end, so a transfer of many files holds one at a time.
struct OneFile {
    files: Vec<SourceFile>,
    path: PathBuf,
    /// The scan's stamp: the handle opened later must be of that same file.
    stamp: SourceStamp,
    reader: Option<Box<dyn ReadSeek>>,
    shown: String,
}

impl OneFile {
    /// The tree and the file's stamp (a local file's from the opened handle, so it describes
    /// exactly what is read). The file is opened (so a folder or link is refused) and closed.
    fn open(path: &Path, name: &str) -> anyhow::Result<(Self, SourceStamp)> {
        let (_, size, stamp) = Self::handle(path)?;
        let tree = Self {
            files: vec![SourceFile {
                path: name.to_string(),
                size,
            }],
            path: path.to_path_buf(),
            stamp: stamp.clone(),
            reader: None,
            shown: path.display().to_string(),
        };
        Ok((tree, stamp))
    }

    fn handle(path: &Path) -> anyhow::Result<(Box<dyn ReadSeek>, u64, SourceStamp)> {
        let shown = || path.display().to_string();
        if is_remote(path) {
            let stamp = SourceStamp::of_file(path)?;
            let r = crate::remote::reader(path)?;
            let size = r.len();
            Ok((Box::new(r), size, stamp))
        } else {
            let f = std::fs::File::open(path).with_context(shown)?;
            let meta = f.metadata().with_context(shown)?;
            if !meta.is_file() {
                bail!("{} is not a file", shown());
            }
            let stamp = SourceStamp::of_handle(path, &f)?;
            Ok((Box::new(f), meta.len(), stamp))
        }
    }
}

impl SourceTree for OneFile {
    fn files(&self) -> &[SourceFile] {
        &self.files
    }

    fn read(&mut self, path: &str) -> ps5upload_fpkg::Result<Vec<u8>> {
        let size = usize::try_from(self.files[0].size).unwrap_or(usize::MAX);
        self.read_range(path, 0, size)
    }

    fn read_range(&mut self, _: &str, offset: u64, len: usize) -> ps5upload_fpkg::Result<Vec<u8>> {
        let size = self.files[0].size;
        let reader = match &mut self.reader {
            Some(r) => r,
            None => {
                let (r, now_size, now) = Self::handle(&self.path)
                    .map_err(|e| std::io::Error::other(format!("{e:#}")))?;
                if now_size != size || !now.same(&self.stamp) {
                    return Err(std::io::Error::other(format!(
                        "{} changed while it was read",
                        self.shown
                    ))
                    .into());
                }
                self.reader.insert(r)
            }
        };
        reader.seek(SeekFrom::Start(offset))?;
        let mut buf = Vec::with_capacity(len.min(CHUNK as usize));
        reader.take(len as u64).read_to_end(&mut buf)?;
        if offset.saturating_add(buf.len() as u64) >= size {
            self.reader = None;
        }
        Ok(buf)
    }

    fn describe(&self) -> String {
        format!("file {}", self.shown)
    }
}

/// The transfer job: scan, preflight, write, verify.
pub(crate) fn run(req: &ConvertRequest, ctx: &Ctx) -> anyhow::Result<JobReport> {
    ctx.stages(&STAGES);
    ctx.progress("scan", 0, 1);
    let findings = request_findings(req);
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    let mut trees: Vec<Box<dyn SourceTree>> = Vec::new();
    let mut selections: Vec<Selection> = Vec::new();
    let mut stamps = Vec::new();
    for path in &req.transfer {
        ctx.check()?;
        let name = name_of(path).unwrap_or_default();
        let net = crate::net::is_network(path);
        let tree: Box<dyn SourceTree> = if net && crate::net::is_dir(path)? || !net && path.is_dir()
        {
            Box::new(Rooted::new(
                open_source(path, Kind::Folder, ctx.cancel)?,
                &name,
            ))
        } else {
            let (tree, stamp) = OneFile::open(path, &name)?;
            stamps.push(stamp);
            Box::new(tree)
        };
        ctx.log(format!("source: {}", tree.describe()));
        let mut selected = select(tree.files(), tree.empty_dirs(), std::slice::from_ref(&name))
            .map_err(preflight_failed)?;
        selections.push(selected.remove(0));
        trees.push(tree);
    }
    ctx.progress("scan", 1, 1);
    ctx.check()?;

    ctx.progress("preflight", 0, 1);
    let mut peer = if is_remote(&req.output) {
        Some(RemoteDir::open(&req.output)?)
    } else {
        None
    };
    let root = match &peer {
        Some(dir) => dir.path().to_path_buf(),
        None => dunce::canonicalize(&req.output)
            .with_context(|| format!("opening {}", req.output.display()))?,
    };
    let mut findings = Vec::new();
    let mut names = std::collections::HashMap::new();
    for (path, s) in req.transfer.iter().zip(&selections) {
        if let Some(other) = names.insert(&s.name, path) {
            findings.push(format!(
                "transfer: {} and {} would both be named {}",
                other.display(),
                path.display(),
                s.name
            ));
        }
    }
    // Checked as one: the room for every source and the names beside each other.
    let (found, replacing) = target_findings("transfer", &root, &selections, req.replace, ctx);
    findings.extend(found);
    ctx.check()?;
    if !findings.is_empty() {
        return Err(preflight_failed(findings));
    }
    #[cfg(target_os = "freebsd")]
    {
        let local = req.transfer.iter().filter(|s| !crate::net::is_network(s));
        for path in local.chain(peer.is_none().then_some(&root)) {
            ctx.watch(path);
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
    let mut out = Written {
        files: &mut written,
        made: &mut made,
        replaced: &mut replaced,
    };
    let wrote = (trees.iter_mut().zip(&selections)).try_for_each(|(tree, s)| {
        write(
            tree.as_mut(),
            &root,
            peer.as_mut(),
            std::slice::from_ref(s),
            &replacing,
            total,
            &mut done,
            &mut out,
            ctx,
        )
    });
    log_replaced(&replaced, ctx);
    let outcome = wrote.and_then(|()| {
        ctx.progress("write", total, total);
        ctx.progress("verify", 0, 1);
        if let Some(changed) = stamps.iter().find(|s| !s.unchanged()) {
            bail!("{} changed while it was read", changed.path().display());
        }
        verify(&written, req.full_verify, ctx)
    });
    let checked = match (outcome, written.len()) {
        (Ok(checked), _) => checked,
        (Err(e), 0) => return Err(made_note(e, &made, "transfer", ctx)),
        (Err(e), n) => {
            let kept = format!("{n} of {count} files were copied before it and stay");
            ctx.log(format!("transfer: {kept}"));
            return Err(anyhow::anyhow!("{e:#}; {kept}"));
        }
    };
    ctx.progress("verify", 1, 1);
    let (mode, check) = if req.full_verify {
        (
            VerifyMode::Full,
            format!("transfer: {count} files read back, BLAKE3 matches"),
        )
    } else {
        (
            VerifyMode::Fast,
            format!("transfer: {count} files, sizes match"),
        )
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use crate::jobs::Ctx;
    use crate::remote::tests::peer;
    use crate::{ConvertRequest, Event, JobReport, Jobs};

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// `src/g` (a file longer than a read, an empty file, an empty folder, junk) and
    /// `src/f.bin`; the long file's bytes.
    fn sources(src: &Path) -> Vec<u8> {
        let big: Vec<u8> = (0..9 * 1024 * 1024 + 7).map(|i| (i % 251) as u8).collect();
        put(src, "g/data/big.bin", &big);
        put(src, "g/zero.bin", b"");
        std::fs::create_dir_all(src.join("g/data/empty")).unwrap();
        put(src, "g/.DS_Store", b"junk");
        put(src, "f.bin", b"one file");
        big
    }

    fn request(sources: &[PathBuf], output: &Path) -> ConvertRequest {
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": sources[0], "format": "folder", "output": output
        }))
        .unwrap();
        req.transfer = sources.to_vec();
        req.full_verify = true;
        req
    }

    fn run(req: &ConvertRequest) -> anyhow::Result<JobReport> {
        let cancel = AtomicBool::new(false);
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(|_| {});
        crate::convert::run(req, &Ctx::new(1, &emit, &cancel))
    }

    fn has(e: impl std::fmt::Display, what: &str) {
        assert!(e.to_string().contains(what), "{e}");
    }

    #[test]
    fn files_and_folders_are_copied_between_devices() {
        let dir = crate::test_dir("transfer-copy");
        let src = dir.join("src");
        let big = sources(&src);
        let both = [src.join("g"), src.join("f.bin")];

        // Here to here: each under its name, the empty folder too, junk left out.
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let report = run(&request(&both, &out)).unwrap();
        assert_eq!(report.files, 3);
        assert_eq!(std::fs::read(out.join("g/data/big.bin")).unwrap(), big);
        assert_eq!(std::fs::read(out.join("g/zero.bin")).unwrap(), b"");
        assert!(out.join("g/data/empty").is_dir());
        assert!(!out.join("g/.DS_Store").exists());
        assert_eq!(std::fs::read(out.join("f.bin")).unwrap(), b"one file");
        // A taken name, two sources with one name, a destination inside a source, other
        // options: refused before anything is written.
        has(run(&request(&both, &out)).unwrap_err(), "already exists");
        std::fs::create_dir_all(src.join("x")).unwrap();
        put(&src, "x/f.bin", b"other");
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        has(
            run(&request(&[src.join("f.bin"), src.join("x/f.bin")], &other)).unwrap_err(),
            "would both be named f.bin",
        );
        has(
            run(&request(std::slice::from_ref(&src), &src.join("x"))).unwrap_err(),
            "is inside",
        );
        let mut req = request(&[src.join("f.bin")], &other);
        req.extract = vec!["eboot.bin".into()];
        has(
            run(&req).unwrap_err(),
            "takes no edit, extract, LZ4 or zstd options",
        );
        assert_eq!(std::fs::read_dir(&other).unwrap().count(), 0);

        // Here to a peer, a peer to here, and a peer to another peer (relayed here).
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        let (a, fa) = peer(&dir.join("a"));
        let (b, fb) = peer(&dir.join("b"));
        run(&request(&both, &a.url())).unwrap();
        assert_eq!(fa.file("g/data/big.bin").unwrap(), big);
        assert!(fa.paths().contains(&"g/data/empty".to_string()));
        assert_eq!(fa.file("f.bin").unwrap(), b"one file");
        has(
            run(&request(&both, &a.url())).unwrap_err(),
            "already exists",
        );
        let back = dir.join("back");
        std::fs::create_dir_all(&back).unwrap();
        let on_a = [a.join("g").url(), a.join("f.bin").url()];
        run(&request(&on_a, &back)).unwrap();
        assert_eq!(std::fs::read(back.join("g/data/big.bin")).unwrap(), big);
        assert!(back.join("g/data/empty").is_dir());
        assert_eq!(std::fs::read(back.join("f.bin")).unwrap(), b"one file");
        run(&request(&on_a, &b.url())).unwrap();
        assert_eq!(fb.file("g/data/big.bin").unwrap(), big);
        assert_eq!(fb.file("f.bin").unwrap(), b"one file");
        has(
            run(&request(&[a.join("g").url()], &a.join("g/data").url())).unwrap_err(),
            "is inside",
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_transfer_leaves_no_temporary_file() {
        let dir = crate::test_dir("transfer-cancel");
        let src = dir.join("src");
        sources(&src);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        // The log line's target is a host path: `\g` on Windows.
        let folder = format!("{}g (2 files", std::path::MAIN_SEPARATOR);
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| {
            // As the folder starts: its first file's temporary name is made, then cut.
            if let Event::Log { line, .. } = e
                && line.contains(&folder)
                && line.contains(" -> ")
            {
                flag.store(true, Ordering::Relaxed);
            }
        });
        // The small file first: it is finished and stays; the folder's first file is cut and
        // its temporary file removed.
        let req = request(&[src.join("f.bin"), src.join("g")], &out);
        let e = crate::convert::run(&req, &Ctx::new(1, &emit, &cancel)).unwrap_err();
        has(format!("{e:#}"), "cancel");
        has(
            format!("{e:#}"),
            "1 of 3 files were copied before it and stay",
        );
        assert_eq!(std::fs::read(out.join("f.bin")).unwrap(), b"one file");
        let mut left = Vec::new();
        let mut todo = vec![out.clone()];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(at).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    todo.push(path.clone());
                }
                left.push(path);
            }
        }
        assert!(
            left.iter().all(|p| !p.to_string_lossy().ends_with(".tmp")),
            "{left:?}"
        );
        assert!(!out.join("g/data/big.bin").exists());

        // Cut before any file is finished: the folders made stay, and the error says so.
        cancel.store(false, Ordering::Relaxed);
        let out = dir.join("out2");
        std::fs::create_dir_all(&out).unwrap();
        let req = request(&[src.join("g")], &out);
        let e = crate::convert::run(&req, &Ctx::new(1, &emit, &cancel)).unwrap_err();
        has(format!("{e:#}"), "cancel");
        has(format!("{e:#}"), "g was created and stays");
        assert!(out.join("g").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    type Done = (u64, Result<JobReport, String>);

    #[test]
    fn a_file_source_holds_a_handle_only_while_it_is_read() {
        use ps5upload_fpkg::source::SourceTree;
        let dir = crate::test_dir("transfer-onefile");
        put(&dir, "f.bin", b"0123456789");
        let (mut one, _) = super::OneFile::open(&dir.join("f.bin"), "f.bin").unwrap();
        assert!(one.reader.is_none(), "closed after the scan");
        assert_eq!(one.read_range("f.bin", 0, 4).unwrap(), b"0123");
        assert!(one.reader.is_some());
        assert_eq!(one.read_range("f.bin", 4, 100).unwrap(), b"456789");
        assert!(one.reader.is_none(), "closed at its end");
        // Replaced since the scan (same length): the next open refuses it.
        put(&dir, "g.bin", b"abcdefghij");
        std::fs::rename(dir.join("g.bin"), dir.join("f.bin")).unwrap();
        has(
            one.read_range("f.bin", 0, 4).unwrap_err(),
            "changed while it was read",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Jobs whose job 1 waits at its stages until `release` is sent; `done` gets each Done.
    fn held() -> (Jobs, mpsc::Sender<()>, mpsc::Receiver<Done>) {
        let (release, wait) = mpsc::channel::<()>();
        let wait = Mutex::new(wait);
        let (send, done) = mpsc::channel();
        let send = Mutex::new(send);
        let jobs = Jobs::new(move |e| match e {
            Event::Stages { job: 1, .. } => {
                let _ = wait.lock().unwrap().recv();
            }
            Event::Done { job, result } => {
                let _ = send.lock().unwrap().send((job, result));
            }
            _ => {}
        });
        (jobs, release, done)
    }

    fn next(done: &mpsc::Receiver<Done>) -> Done {
        done.recv_timeout(Duration::from_secs(60)).unwrap()
    }

    fn convert(source: &Path, output: &Path) -> ConvertRequest {
        serde_json::from_value(serde_json::json!({
            "source": source, "format": "folder", "output": output
        }))
        .unwrap()
    }

    #[test]
    fn a_transfer_runs_beside_a_heavy_job_and_conflicts_are_refused() {
        let dir = crate::test_dir("transfer-slots");
        let src = dir.join("src");
        sources(&src);
        put(&dir, "game/eboot.bin", b"\x7fELF fake eboot");
        put(
            &dir,
            "game/sce_sys/param.json",
            br#"{"titleId":"PPSA01234","contentId":"UP0000-PPSA01234_00-TESTTESTTESTTEST"}"#,
        );
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        // A heavy job held running; a transfer still runs, in its own slot.
        let (jobs, release, done) = held();
        assert_eq!(jobs.start(convert(&dir.join("game"), &dir.join("conv"))), 1);
        let t = jobs.start(request(&[src.join("f.bin")], &out));
        let (job, result) = next(&done);
        assert_eq!(job, t);
        result.unwrap();
        // A transfer from or into the heavy job's paths is refused, nothing written.
        let t = jobs.start(request(&[dir.join("game")], &out));
        let (job, result) = next(&done);
        assert_eq!(job, t);
        has(result.unwrap_err(), "in use by another job");
        jobs.start(request(&[src.join("g")], &dir.join("game")));
        has(next(&done).1.unwrap_err(), "in use by another job");
        assert!(!dir.join("game/g").exists());
        release.send(()).unwrap();
        assert_eq!(next(&done).0, 1);

        // A transfer held running: a heavy job reading what it writes is refused.
        let (jobs, release, done) = held();
        assert_eq!(jobs.start(request(&[src.join("g")], &out)), 1);
        let c = jobs.start(convert(&out.join("g"), &dir.join("conv2")));
        let (job, result) = next(&done);
        assert_eq!(job, c);
        has(result.unwrap_err(), "in use by another job");
        // A path another device is writing refuses any job.
        let busy = dir.join("game");
        jobs.set_external_busy(Arc::new(move |p| p.starts_with(&busy)));
        jobs.start(convert(&dir.join("game"), &dir.join("conv3")));
        has(
            next(&done).1.unwrap_err(),
            "being written by another device",
        );
        release.send(()).unwrap();
        let (job, result) = next(&done);
        assert_eq!(job, 1);
        result.unwrap();
        assert!(out.join("g/data/empty").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn transfers_queue_in_order_and_a_cancelled_one_leaves_the_queue() {
        let dir = crate::test_dir("transfer-fifo");
        let src = dir.join("src");
        sources(&src);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let (jobs, release, done) = held();
        assert_eq!(jobs.start(request(&[src.join("f.bin")], &out)), 1);
        let second = jobs.start(request(&[src.join("g")], &out));
        let third = jobs.start(request(&[src.join("g/zero.bin")], &out));
        // Queued behind the first: cancelled, it ends at once and writes nothing.
        jobs.cancel(third);
        let (job, result) = next(&done);
        assert_eq!((job, result.unwrap_err()), (third, "cancelled".to_string()));
        assert!(!out.join("zero.bin").exists() && !out.join("g").exists());
        release.send(()).unwrap();
        let (job, result) = next(&done);
        assert_eq!(job, 1);
        result.unwrap();
        let (job, result) = next(&done);
        assert_eq!(job, second);
        result.unwrap();
        assert!(out.join("g/data/big.bin").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Runs `req` as `run` does, with its log lines.
    fn run_logged(req: &ConvertRequest) -> (anyhow::Result<JobReport>, Vec<String>) {
        let cancel = AtomicBool::new(false);
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let emit: Box<dyn Fn(Event) + Send + Sync> = Box::new(move |e| {
            if let Event::Log { line, .. } = e {
                sink.lock().unwrap().push(line);
            }
        });
        let result = crate::convert::run(req, &Ctx::new(1, &emit, &cancel));
        let lines = lines.lock().unwrap().clone();
        (result, lines)
    }

    /// Every path under `root` with a file's bytes, sorted.
    fn listing(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
        let mut out = Vec::new();
        let mut todo = vec![root.to_path_buf()];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(at).unwrap() {
                let path = entry.unwrap().path();
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if path.is_dir() {
                    todo.push(path);
                    out.push((rel, None));
                } else {
                    out.push((rel, Some(std::fs::read(&path).unwrap())));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn replace_puts_files_over_files_and_merges_folders() {
        let dir = crate::test_dir("transfer-replace");
        let src = dir.join("src");
        let big = sources(&src);
        put(&src, "new.bin", b"new");
        let all = [src.join("g"), src.join("f.bin"), src.join("new.bin")];
        let out = dir.join("out");
        put(&out, "f.bin", b"old");
        put(&out, "g/old.bin", b"keep");
        put(&out, "g/zero.bin", b"old zero");

        // Asked first: only the targets there, at the top level.
        let jobs = Jobs::new(|_| {});
        let req = request(&all, &out);
        let canon = |p: &str| out.join(p);
        assert_eq!(
            jobs.existing_targets(&req).unwrap(),
            [canon("g"), canon("f.bin")]
        );
        // Without replace: refused as ever, nothing written.
        let before = listing(&out);
        let e = run(&req).unwrap_err();
        has(&e, "f.bin already exists (never replaced)");
        has(&e, "g already exists (never replaced)");
        assert_eq!(listing(&out), before);

        // With it: a file over a file, the folder merged (its own file kept), new ones made.
        let mut req = request(&all, &out);
        req.replace = true;
        let (report, log) = run_logged(&req);
        assert_eq!(report.unwrap().files, 4);
        assert_eq!(std::fs::read(out.join("f.bin")).unwrap(), b"one file");
        assert_eq!(std::fs::read(out.join("g/old.bin")).unwrap(), b"keep");
        assert_eq!(std::fs::read(out.join("g/zero.bin")).unwrap(), b"");
        assert_eq!(std::fs::read(out.join("g/data/big.bin")).unwrap(), big);
        assert!(out.join("g/data/empty").is_dir());
        assert_eq!(std::fs::read(out.join("new.bin")).unwrap(), b"new");
        let line = log.iter().find(|l| l.starts_with("replace: ")).unwrap();
        assert!(line.starts_with("replace: 2 files replaced: "), "{line}");
        assert!(listing(&out).iter().all(|(p, _)| !p.ends_with(".tmp")));

        // A kind mismatch at any level: refused, nothing written.
        let other = dir.join("other");
        std::fs::create_dir_all(other.join("f.bin")).unwrap();
        put(&other, "g/data", b"a file where a folder goes");
        let before = listing(&other);
        let mut req = request(&[src.join("g"), src.join("f.bin")], &other);
        req.replace = true;
        let e = format!("{:#}", run(&req).unwrap_err());
        has(&e, "f.bin is a folder: a file is not put in its place");
        has(&e, "data is a file: a folder is not put in its place");
        assert_eq!(listing(&other), before);

        // On a peer the same: asked first, then a file over a file (small ones in calls of
        // their own, a large one streamed), the folder merged.
        let at_peer = dir.join("a");
        put(&at_peer, "f.bin", b"old");
        put(&at_peer, "g/old.bin", b"keep");
        put(&at_peer, "g/zero.bin", b"old zero");
        put(&at_peer, "g/data/big.bin", b"old big");
        let (a, fa) = peer(&at_peer);
        let mut req = request(&all, &a.url());
        assert_eq!(
            jobs.existing_targets(&req).unwrap(),
            [a.join("g").url(), a.join("f.bin").url()]
        );
        has(
            run(&req).unwrap_err(),
            "f.bin already exists (never replaced)",
        );
        req.replace = true;
        let (report, log) = run_logged(&req);
        assert_eq!(report.unwrap().files, 4);
        assert_eq!(fa.file("f.bin").unwrap(), b"one file");
        assert_eq!(fa.file("g/old.bin").unwrap(), b"keep");
        assert_eq!(fa.file("g/zero.bin").unwrap(), b"");
        assert_eq!(fa.file("g/data/big.bin").unwrap(), big);
        assert_eq!(fa.file("new.bin").unwrap(), b"new");
        assert!(fa.paths().contains(&"g/data/empty".to_string()));
        assert!(
            fa.paths().iter().all(|p| !p.ends_with(".tmp")),
            "{:?}",
            fa.paths()
        );
        let line = log.iter().find(|l| l.starts_with("replace: ")).unwrap();
        assert!(line.starts_with("replace: 3 files replaced: "), "{line}");

        // A kind mismatch there, a link, or a name the device may fold into another: refused,
        // nothing written.
        let at_other = dir.join("b");
        std::fs::create_dir_all(at_other.join("f.bin")).unwrap();
        put(&at_other, "g/data", b"a file where a folder goes");
        put(&at_other, "g/zero.bin", b"a link");
        put(&at_other, "g/Old.bin", b"other case");
        let (b, fb) = peer(&at_other);
        fb.links.lock().unwrap().push("g/zero.bin".into());
        let before = fb.paths();
        let mut req = request(
            &[src.join("g"), src.join("f.bin"), src.join("new.bin")],
            &b.url(),
        );
        req.replace = true;
        put(&src, "g/old.bin", b"new old");
        let e = format!("{:#}", run(&req).unwrap_err());
        std::fs::remove_file(src.join("g/old.bin")).unwrap();
        has(&e, "f.bin is a folder: a file is not put in its place");
        has(&e, "data is a file: a folder is not put in its place");
        has(&e, "zero.bin is a link: nothing is put in its place");
        has(&e, "old.bin is there as Old.bin");
        assert_eq!(fb.paths(), before);
        assert_eq!(fb.file("g/Old.bin").unwrap(), b"other case");

        // An extract onto a peer replaces and merges the same way.
        let at_x = dir.join("x");
        put(&at_x, "g/zero.bin", b"old zero");
        put(&at_x, "g/kept.bin", b"kept");
        let (x, fx) = peer(&at_x);
        let mut req = convert(&src, &x.url());
        req.extract = vec!["g".into()];
        has(run(&req).unwrap_err(), "already exists (never replaced)");
        req.replace = true;
        let (report, log) = run_logged(&req);
        report.unwrap();
        assert_eq!(fx.file("g/zero.bin").unwrap(), b"");
        assert_eq!(fx.file("g/kept.bin").unwrap(), b"kept");
        assert_eq!(fx.file("g/data/big.bin").unwrap(), big);
        assert!(
            log.iter()
                .any(|l| l.starts_with("replace: 1 file replaced: "))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A job never replaces what it copies from: a file sent into its own folder, or a folder
    /// merged into itself, is refused with nothing changed.
    #[test]
    fn replace_refuses_the_jobs_own_source() {
        let dir = crate::test_dir("transfer-replace-own");
        put(&dir, "f.bin", b"mine");
        put(&dir, "g/a.bin", b"a");
        for source in [dir.join("f.bin"), dir.join("g")] {
            let (jobs, release, done) = held();
            let mut req = request(std::slice::from_ref(&source), &dir);
            req.replace = true;
            assert_eq!(jobs.start(req), 1);
            release.send(()).unwrap();
            let (_, result) = next(&done);
            has(result.unwrap_err(), "is what this job copies from");
        }
        assert_eq!(std::fs::read(dir.join("f.bin")).unwrap(), b"mine");
        assert_eq!(std::fs::read(dir.join("g/a.bin")).unwrap(), b"a");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replace_refuses_what_another_job_uses_or_is_protected() {
        let dir = crate::test_dir("transfer-replace-busy");
        let src = dir.join("src");
        sources(&src);
        let out = dir.join("out");
        put(&out, "g/zero.bin", b"old zero");
        put(&out, "f.bin", b"old");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();

        // A queued transfer reads the file job 1 would replace: job 1 is refused.
        let (jobs, release, done) = held();
        let mut req = request(&[src.join("g")], &out);
        req.replace = true;
        assert_eq!(jobs.start(req), 1);
        let reader = jobs.start(request(&[out.join("g/zero.bin")], &elsewhere));
        release.send(()).unwrap();
        let (job, result) = next(&done);
        assert_eq!(job, 1);
        has(result.unwrap_err(), "in use by an unfinished job");
        assert_eq!(std::fs::read(out.join("g/zero.bin")).unwrap(), b"old zero");
        assert_eq!(next(&done).0, reader);

        // The same for an extract (a heavy job) behind which another one reads the folder.
        let (jobs, release, done) = held();
        let mut req = convert(&src.join("g"), &out);
        req.extract = vec!["zero.bin".into()];
        req.replace = true;
        std::fs::rename(out.join("g/zero.bin"), out.join("zero.bin")).unwrap();
        assert_eq!(jobs.start(req), 1);
        let mut reader = convert(&out, &elsewhere);
        reader.extract = vec!["f.bin".into()];
        let reader = jobs.start(reader);
        release.send(()).unwrap();
        let (job, result) = next(&done);
        assert_eq!(job, 1);
        has(result.unwrap_err(), "in use by an unfinished job");
        assert_eq!(std::fs::read(out.join("zero.bin")).unwrap(), b"old zero");
        assert_eq!(next(&done).0, reader);

        // A protected file is not replaced either.
        let (jobs, release, done) = held();
        let kept = out.join("f.bin");
        jobs.set_protected(Arc::new(move || vec![kept.clone()]));
        let mut req = request(&[src.join("f.bin")], &out);
        req.replace = true;
        jobs.start(req);
        release.send(()).unwrap();
        has(next(&done).1.unwrap_err(), "which is not replaced");
        assert_eq!(std::fs::read(out.join("f.bin")).unwrap(), b"old");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
