//! Worker threads: one per job, one heavy job and one transfer running at a time, each kind's
//! others waiting in FIFO order. Each job has its own cancel flag. A job that would use what an
//! unfinished job of the other kind uses (or what another device is writing here) is refused
//! when it starts.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::bail;

use crate::{ConvertRequest, Event, JobId};

type Emit = Box<dyn Fn(Event) + Send + Sync>;
type Busy = Arc<dyn Fn(&Path) -> bool + Send + Sync>;
type Protected = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;
/// [`Ctx::replace_refusal`]'s check against the job table: (path, a folder merged into).
type ReplaceGuard<'a> = &'a dyn Fn(&Path, bool) -> Result<(), crate::DeleteError>;

pub(crate) struct Inner {
    emit: Emit,
    next: AtomicU64,
    state: Mutex<State>,
    /// Signalled when the running job finishes or a waiting one is cancelled.
    turn: Condvar,
    /// Whether another device is writing a path here ([`crate::Jobs::set_external_busy`]).
    busy: Mutex<Option<Busy>>,
    /// What replacing never touches ([`crate::Jobs::set_protected`]).
    protected: Mutex<Option<Protected>>,
}

#[derive(Default)]
struct State {
    /// Waiting and running heavy jobs; the front one runs.
    queue: VecDeque<JobId>,
    /// Waiting and running transfers; the front one runs.
    transfers: VecDeque<JobId>,
    cancels: HashMap<JobId, Arc<AtomicBool>>,
    /// What each unfinished job reads or writes, which [`Inner::delete_path`] leaves alone.
    paths: HashMap<JobId, Vec<PathBuf>>,
    threads: Vec<JoinHandle<()>>,
}

impl Inner {
    pub(crate) fn new(emit: Emit) -> Arc<Self> {
        Arc::new(Self {
            emit,
            next: AtomicU64::new(1),
            state: Mutex::new(State::default()),
            turn: Condvar::new(),
            busy: Mutex::new(None),
            protected: Mutex::new(None),
        })
    }

    pub(crate) fn set_external_busy(&self, busy: Busy) {
        *self.busy.lock().unwrap_or_else(|e| e.into_inner()) = Some(busy);
    }

    pub(crate) fn set_protected(&self, protected: Protected) {
        *self.protected.lock().unwrap_or_else(|e| e.into_inner()) = Some(protected);
    }

    /// Why `job` must not replace `path` (`dir`: a folder it merges into), as
    /// [`Inner::delete_path`] would refuse it: against what `job` itself reads (`reads`), what
    /// another device is writing and every other unfinished job's paths (its other own paths
    /// are its targets).
    fn replace_refusal(
        &self,
        job: JobId,
        reads: &[PathBuf],
        path: &Path,
        dir: bool,
    ) -> Result<(), crate::DeleteError> {
        if reads.iter().any(|r| at_or_in(path, r)) {
            return Err(crate::DeleteError::Refused(format!(
                "{} is what this job copies from: it is not replaced",
                path.display()
            )));
        }
        self.refuse_external_write(path)?;
        let protected = self
            .protected
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let protected = protected.map(|p| p()).unwrap_or_default();
        let state = self.state();
        let others = (state.paths.iter())
            .filter(|(j, _)| **j != job)
            .flat_map(|(_, p)| p);
        crate::delete::replace_refusal(path, dir, &protected, others)
    }

    /// [`crate::Jobs::replace_check`]: `path` as a file put in its place, against what another
    /// device is writing, the protected paths and every unfinished job; nothing there passes.
    pub(crate) fn replace_check(&self, path: &Path) -> Result<(), crate::DeleteError> {
        self.refuse_external_write(path)?;
        let protected = self
            .protected
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let protected = protected.map(|p| p()).unwrap_or_default();
        let state = self.state();
        match crate::delete::replace_refusal(
            path,
            false,
            &protected,
            state.paths.values().flatten(),
        ) {
            Err(crate::DeleteError::Missing(_)) => Ok(()),
            done => done,
        }
    }

    /// Asked without the job table locked: Connect's hook takes locks of its own.
    fn externally_busy(&self, path: &Path) -> bool {
        let busy = self.busy.lock().unwrap_or_else(|e| e.into_inner()).clone();
        busy.is_some_and(|busy| busy(path))
    }

    fn refuse_external_write(&self, path: &Path) -> Result<(), crate::DeleteError> {
        if self.externally_busy(path) {
            return Err(crate::DeleteError::Busy(format!(
                "{} is being written by another device",
                path.display()
            )));
        }
        Ok(())
    }

    /// A panicking job must not wedge every later one, so a poisoned lock is still used.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn cancel(&self, job: JobId) {
        if let Some(flag) = self.state().cancels.get(&job) {
            flag.store(true, Ordering::Relaxed);
        }
        self.turn.notify_all();
    }

    /// Deletes `path` with the table locked: no job is admitted between the check and the
    /// removal, and none that reads or writes anything at, in or around `path` is unfinished.
    // ponytail: the lock is held through the removal, so a large folder holds off starting,
    // cancelling and finishing jobs until it is gone.
    pub(crate) fn delete_path(
        &self,
        path: &Path,
        protected: &[PathBuf],
    ) -> Result<(), crate::DeleteError> {
        self.refuse_external_write(path)?;
        let state = self.state();
        crate::delete::delete(path, protected, state.paths.values().flatten())
    }

    /// Moves `from` to `to` with the table locked, as [`Inner::delete_path`] deletes.
    pub(crate) fn move_path(
        &self,
        from: &Path,
        to: &Path,
        replace: bool,
        protected: &[PathBuf],
    ) -> Result<(), crate::DeleteError> {
        self.refuse_external_write(from)?;
        self.refuse_external_write(to)?;
        let state = self.state();
        crate::delete::move_(from, to, replace, protected, state.paths.values().flatten())
    }

    /// Whether an unfinished job reads or writes `path`, or something inside or around it.
    pub(crate) fn uses(&self, path: &Path) -> bool {
        crate::delete::in_use(path, self.state().paths.values().flatten())
    }

    /// [`crate::Jobs::edit_check`]: with the table locked, no job is admitted between the
    /// check that none uses the image and the recovery of its leftover journal; the rest reads
    /// only, unlocked.
    pub(crate) fn edit_check(&self, req: &crate::ConvertRequest) -> crate::EditCheck {
        // A game folder is changed directly: never rebuilt, so only `in_place` speaks.
        let folder = matches!(
            crate::convert::Kind::of(&req.source),
            Ok(crate::convert::Kind::Folder)
        );
        let settled = {
            let state = self.state();
            if crate::delete::in_use(&req.source, state.paths.values().flatten()) {
                let busy = format!(
                    "a job is using {}; check again once it ends",
                    req.source.display()
                );
                return crate::EditCheck {
                    in_place: vec![busy.clone()],
                    rebuild: if folder { Vec::new() } else { vec![busy] },
                    ..crate::EditCheck::default()
                };
            }
            if folder {
                None
            } else {
                crate::edit_job::settle(req)
            }
        };
        if folder {
            return crate::edit_folder::check(req);
        }
        crate::edit_job::check(req, settled)
    }

    pub(crate) fn cancel_all_and_wait(&self) {
        let threads = {
            let mut state = self.state();
            for flag in state.cancels.values() {
                flag.store(true, Ordering::Relaxed);
            }
            std::mem::take(&mut state.threads)
        };
        self.turn.notify_all();
        for thread in threads {
            let _ = thread.join();
        }
    }
}

pub(crate) fn start(inner: &Arc<Inner>, request: ConvertRequest) -> JobId {
    let job = inner.next.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    let transfer = transfer_slot(&request);
    let paths = used_paths(job, &request);
    // Asked unlocked: the hook may take locks of its own (Connect's).
    // ponytail: check, then admit; a remote write that starts between the two isn't caught
    // (no shared reservation with Connect).
    let written = paths.iter().find(|p| inner.externally_busy(p)).cloned();
    let mut state = inner.state();
    state.threads.retain(|t| !t.is_finished());
    let refusal = match written {
        Some(p) => Some(format!(
            "{} is being written by another device",
            p.display()
        )),
        None => conflict(&state, transfer, &paths),
    };
    if refusal.is_none() {
        let queue = if transfer {
            &mut state.transfers
        } else {
            &mut state.queue
        };
        queue.push_back(job);
        state.paths.insert(job, paths);
    }
    state.cancels.insert(job, cancel.clone());
    let worker = inner.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("forge-job-{job}"))
        .spawn(move || worker.run(job, &request, cancel, refusal));
    match spawned {
        Ok(handle) => state.threads.push(handle),
        Err(e) => {
            state.queue.retain(|j| *j != job);
            state.transfers.retain(|j| *j != job);
            state.cancels.remove(&job);
            state.paths.remove(&job);
            drop(state);
            (inner.emit)(Event::Done {
                job,
                result: Err(format!("could not start a worker thread: {e}")),
            });
        }
    }
    job
}

/// Whether `req` runs in the transfer slot: a transfer, or an install (ponytail: an install
/// waits behind a transfer and a transfer behind an install).
fn transfer_slot(req: &ConvertRequest) -> bool {
    !req.transfer.is_empty() || req.install.is_some()
}

/// Why a job (a transfer or not) using `paths` can't start: one of them is at, in or above
/// what an unfinished job of the other kind uses (jobs of one kind run one after another).
fn conflict(state: &State, transfer: bool, paths: &[PathBuf]) -> Option<String> {
    let others: Vec<&PathBuf> = (state.paths.iter())
        .filter(|(job, _)| state.transfers.contains(job) != transfer)
        .flat_map(|(_, p)| p)
        .collect();
    if others.is_empty() {
        return None;
    }
    let used = paths
        .iter()
        .find(|p| crate::delete::in_use(p, others.iter().copied()))?;
    Some(format!(
        "{} is in use by another job: start this one once it ends",
        used.display()
    ))
}

/// The paths a job reads or writes: source, output (the source, in place), its `.part`s
/// (`finalize::part_path`, and `Part::replace`'s `.orig.part`), an archive's later parts,
/// for an edit, the undo journal and
/// every file or folder it adds.
/// An archive's later parts, found by name beside it (`source` is the first part).
fn later_parts(source: &Path) -> Vec<PathBuf> {
    if !crate::archive::is_archive(source) {
        return Vec::new();
    }
    let folder = source.parent().filter(|p| !p.as_os_str().is_empty());
    let Ok(items) = std::fs::read_dir(folder.unwrap_or(Path::new("."))) else {
        return Vec::new();
    };
    (items.flatten().map(|i| i.path()))
        .filter(|p| ps5_dump_forge_archive::later_part_of(p, source))
        .collect()
}

/// What an Extract or a transfer reads (a transfer's sources, else the source and an
/// archive's later parts): never what it replaces.
fn read_paths(req: &ConvertRequest) -> Vec<PathBuf> {
    if let Some(package) = &req.install {
        return install_paths(package);
    }
    if !req.transfer.is_empty() {
        return req.transfer.clone();
    }
    let mut paths = vec![req.source.clone()];
    paths.extend(later_parts(&req.source));
    paths
}

/// An install's package and its `.crc` sidecar: here only when it exists. These run under the
/// job table's lock (start, delete, quit), so another device is never asked: its sidecar is
/// guarded whether or not it is there.
// ponytail: a remote package's missing sidecar name is guarded too (refused to other jobs).
fn install_paths(package: &Path) -> Vec<PathBuf> {
    let mut paths = vec![package.to_path_buf()];
    let sidecar = package.with_extension("crc");
    if crate::remote::is_remote(package) || sidecar.symlink_metadata().is_ok() {
        paths.push(sidecar);
    }
    paths
}

/// Whether `path` is `read` or inside it (resolved on this device; on another one as
/// [`crate::remote::FlinkPath::within`] compares them). A folder above `read` is not: what it
/// holds is checked name by name.
fn at_or_in(path: &Path, read: &Path) -> bool {
    use crate::remote::{FlinkPath, is_remote};
    if is_remote(path) || is_remote(read) {
        return matches!(
            (FlinkPath::parse(path), FlinkPath::parse(read)),
            (Ok(p), Ok(r)) if p.within(&r)
        );
    }
    let real = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    real(path).starts_with(real(read))
}

fn used_paths(job: JobId, req: &ConvertRequest) -> Vec<PathBuf> {
    // An install: the package and its existing sidecar, nothing written.
    if let Some(package) = &req.install {
        return install_paths(package);
    }
    // A transfer: its sources and where each lands (its temporary files are inside those).
    if !req.transfer.is_empty() {
        let mut paths = req.transfer.clone();
        paths.extend(crate::transfer::targets(req));
        return paths;
    }
    let out = if !req.edits.is_empty() {
        &req.source
    } else {
        &req.output
    };
    let mut paths = vec![req.source.clone()];
    if req.extract.is_empty() {
        let part = crate::finalize::part_path(out, job);
        let orig = part.with_extension("orig.part");
        paths.extend([out.clone(), part, orig]);
    } else {
        // An Extract makes only `<output>/<each selection's last part>` (its temporary files
        // inside or beside them), not the whole output folder.
        let name = |p: &String| p.rsplit('/').next().unwrap_or(p).to_string();
        paths.extend(req.extract.iter().map(|p| req.output.join(name(p))));
    }
    paths.extend(later_parts(&req.source));
    // An edit job's undo journal and added files.
    if !req.edits.is_empty() {
        paths.push(ps5_dump_forge_patch::journal_path(&req.source));
        paths.extend(req.edits.iter().filter_map(|e| match e {
            crate::Edit::Add { from, .. } => Some(from.clone()),
            _ => None,
        }));
    }
    paths
}

impl Inner {
    fn run(
        &self,
        job: JobId,
        request: &ConvertRequest,
        shared: Arc<AtomicBool>,
        refusal: Option<String>,
    ) {
        let cancel: &AtomicBool = &shared;
        // Dropped last: Done goes out before the next job is woken, so no event of the next
        // job can arrive before this job's Done. A panicking `emit` still dequeues.
        let _leave = Leave(self, job);
        let transfer = transfer_slot(request);
        let admitted = match refusal {
            Some(why) => Err(anyhow::anyhow!("preflight failed:\n  {why}")),
            None => self.wait_turn(job, transfer, cancel),
        };
        let result = admitted.and_then(|()| {
            // U8: auto rest mode waits while the job runs.
            let _awake = crate::awake();
            let reads = read_paths(request);
            let guard = |path: &Path, dir: bool| self.replace_refusal(job, &reads, path, dir);
            let mut ctx = Ctx::new(job, &self.emit, cancel);
            ctx.shared_cancel = Some(shared.clone());
            ctx.replace_guard = Some(&guard);
            match catch_unwind(AssertUnwindSafe(|| crate::convert::run(request, &ctx))) {
                Ok(result) => result.map(|report| (report, ctx.read.take())),
                Err(panic) => {
                    let msg = panic
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown panic".to_string());
                    bail!("internal error: {msg}")
                }
            }
        });
        let result = result.map_err(|e| {
            let msg = format!("{e:#}");
            // An edit's "cancelled; …" says what stays changed (a folder edit has no rollback).
            if !cancel.load(Ordering::Relaxed) || msg.starts_with("cancelled; ") {
                msg
            } else if e
                .root_cause()
                .to_string()
                .to_lowercase()
                .trim_end_matches('.')
                .ends_with("cancelled")
            {
                // The cancel itself ("cancelled", "exFAT image cancelled", "Cancelled.").
                "cancelled".to_string()
            } else {
                // Cancelled, but something else failed too (a restore, a replace): say what.
                format!("cancelled; {msg}")
            }
        });
        // Its files are settled (published or cleaned up): they may be deleted from now on.
        let result = match result {
            Ok((mut report, read)) if request.delete_source => {
                use crate::delete::SourceKept;
                let line = match self.delete_source(job, read.as_ref(), &report.output) {
                    Ok(()) => format!("source deleted: {}", request.source.display()),
                    Err(SourceKept::Kept(e)) => format!("source kept: {e}"),
                    Err(SourceKept::Partly(gone, e)) => {
                        format!("source partly deleted: {gone}; {e}")
                    }
                };
                (self.emit)(Event::Log {
                    job,
                    line: line.clone(),
                });
                report.checks.push(line);
                Ok(report)
            }
            result => {
                self.state().paths.remove(&job);
                result.map(|(report, _)| report)
            }
        };
        (self.emit)(Event::Done { job, result });
    }

    /// [`ConvertRequest::delete_source`] for `job`, done: the source it read (`read`: an
    /// archive's every part it stamped) checked unchanged first (a NAS or server source refused
    /// at once), then its paths dropped and the source deleted with the table locked, as
    /// [`Inner::delete_path`] deletes, so no job is admitted between them. A peer's source is
    /// deleted with the table unlocked (the peer's delete takes its own table, and a peer
    /// deleting here at once would wait on this one), the job's paths kept in it meanwhile as
    /// the reservation, then dropped. Its output is never at or inside what goes.
    fn delete_source(
        &self,
        job: JobId,
        read: Option<&crate::delete::ReadSource>,
        output: &Path,
    ) -> Result<(), crate::delete::SourceKept> {
        use crate::delete::SourceKept::Kept;
        let Some(read) = read else {
            self.state().paths.remove(&job);
            return Err(Kept(crate::DeleteError::Refused(
                "the job kept no record of what it read".to_string(),
            )));
        };
        if let Some(e) = read
            .paths()
            .iter()
            .find_map(|p| self.refuse_external_write(p).err())
        {
            self.state().paths.remove(&job);
            return Err(Kept(e));
        }
        let protected = self
            .protected
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let protected = protected.map(|p| p()).unwrap_or_default();
        let output = output.to_path_buf();
        // The rescan may be a long listing over the network: done before the table is locked
        // (the job's paths still guard the source meanwhile).
        if let Err(e) = crate::delete::source_unchanged(read) {
            self.state().paths.remove(&job);
            return Err(e);
        }
        if let Some(source) = read
            .paths()
            .iter()
            .find(|p| crate::delete::in_use(p, [&output]))
        {
            self.state().paths.remove(&job);
            return Err(Kept(crate::DeleteError::Refused(format!(
                "the output {} is inside {}",
                output.display(),
                source.display()
            ))));
        }
        let mut state = self.state();
        if read.paths().iter().any(|p| crate::remote::is_remote(p)) {
            let others: Vec<PathBuf> = (state.paths.iter())
                .filter(|(j, _)| **j != job)
                .flat_map(|(_, paths)| paths.iter().cloned())
                .collect();
            drop(state);
            let guarded: Vec<&PathBuf> = others.iter().collect();
            let result = crate::delete::remove_source(read, &protected, &guarded);
            self.state().paths.remove(&job);
            return result;
        }
        state.paths.remove(&job);
        let guarded: Vec<&PathBuf> = state.paths.values().flatten().collect();
        crate::delete::remove_source(read, &protected, &guarded)
    }

    /// Blocks until `job` is at the front of its queue (the transfers' or the heavy jobs'), or
    /// fails if it is cancelled first.
    fn wait_turn(&self, job: JobId, transfer: bool, cancel: &AtomicBool) -> anyhow::Result<()> {
        let mut state = self.state();
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let queue = if transfer {
                &state.transfers
            } else {
                &state.queue
            };
            if queue.front() == Some(&job) {
                return Ok(());
            }
            state = self.turn.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Takes a finished job off the queue and wakes the next one.
struct Leave<'a>(&'a Inner, JobId);

impl Drop for Leave<'_> {
    fn drop(&mut self) {
        {
            let mut state = self.0.state();
            state.queue.retain(|j| *j != self.1);
            state.transfers.retain(|j| *j != self.1);
            state.cancels.remove(&self.1);
            state.paths.remove(&self.1);
        }
        self.0.turn.notify_all();
    }
}

/// How often progress for the same stage is passed on. Writers report per chunk and per
/// file; a quarter of a million events would only flood the UI.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What a running job uses to report and to notice cancellation.
pub(crate) struct Ctx<'a> {
    pub job: JobId,
    emit: &'a dyn Fn(Event),
    pub cancel: &'a AtomicBool,
    /// The same flag, shared, for a reader thread that outlives a borrow (an archive's).
    pub(crate) shared_cancel: Option<Arc<AtomicBool>>,
    last: Cell<Option<Instant>>,
    stage: RefCell<String>,
    /// What every byte pass of the job adds up to, set by [`Ctx::expect`]; 0 until known.
    expected: Cell<u64>,
    /// Bytes of the passes already finished.
    base: Cell<u64>,
    /// (done, total) of the running pass.
    pass: Cell<(u64, u64)>,
    /// Bytes of passes after those [`Ctx::expect_rest`] is told of, set by [`Ctx::reserve`].
    later: Cell<u64>,
    /// U7: the source's and destination's mount points, with their `st_dev` at job start.
    #[cfg(target_os = "freebsd")]
    pub mounts: RefCell<Vec<crate::durable::Watched>>,
    /// The job table's [`Ctx::replace_refusal`]; unset outside [`crate::Jobs`] (a test's).
    pub(crate) replace_guard: Option<ReplaceGuard<'a>>,
    /// What the job read of its source, set by a conversion that deletes it afterwards.
    pub(crate) read: RefCell<Option<crate::delete::ReadSource>>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(job: JobId, emit: &'a Emit, cancel: &'a AtomicBool) -> Self {
        Self {
            job,
            emit: emit.as_ref(),
            cancel,
            shared_cancel: None,
            last: Cell::new(None),
            stage: RefCell::new(String::new()),
            expected: Cell::new(0),
            base: Cell::new(0),
            pass: Cell::new((0, 0)),
            later: Cell::new(0),
            #[cfg(target_os = "freebsd")]
            mounts: RefCell::new(Vec::new()),
            replace_guard: None,
            read: RefCell::new(None),
        }
    }

    /// Why this job must not replace `path` (`dir`: a folder it merges into), as Delete would
    /// refuse it ([`crate::delete::replace_refusal`]): against every other unfinished job and
    /// what another device is writing; outside [`crate::Jobs`], against no job.
    pub(crate) fn replace_refusal(&self, path: &Path, dir: bool) -> Result<(), crate::DeleteError> {
        match self.replace_guard {
            Some(guard) => guard(path, dir),
            None => crate::delete::replace_refusal(path, dir, &[], std::iter::empty()),
        }
    }

    /// Records the mount point `path` is on (`statfs`), once, with its `st_dev`, for the
    /// drive-removal check (U7); `path` itself where `statfs` fails. A path that cannot be
    /// stat'ed is skipped: the job fails on it anyway.
    #[cfg(target_os = "freebsd")]
    pub(crate) fn watch(&self, path: &std::path::Path) {
        let (path, mount) = match crate::dest::mount_of(path) {
            Ok(mount) => (mount, true),
            Err(_) => (path.to_path_buf(), false),
        };
        let mut watched = self.mounts.borrow_mut();
        if watched.iter().all(|w| w.path != path)
            && let Ok((dev, _)) = crate::dest::mount_state(&path)
        {
            watched.push(crate::durable::Watched { path, dev, mount });
        }
    }

    pub(crate) fn check(&self) -> anyhow::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        Ok(())
    }

    pub(crate) fn log(&self, line: impl Into<String>) {
        (self.emit)(Event::Log {
            job: self.job,
            line: line.into(),
        });
    }

    /// Announces the job's stages ([`Event::Stages`]).
    pub(crate) fn stages(&self, stages: &[&str]) {
        (self.emit)(Event::Stages {
            job: self.job,
            stages: stages.iter().map(|s| s.to_string()).collect(),
        });
    }

    /// Starts a pass: it and every pass after it move `bytes` in total. Renewed as each
    /// pass learns its size, so progress is one bar for the whole job that ends at 100%.
    /// Plus what [`Ctx::reserve`] holds back, so no pass fills the bar while one is left.
    pub(crate) fn expect_rest(&self, bytes: u64) {
        self.close_pass();
        let rest = bytes.saturating_add(self.later.get());
        self.expected.set(self.base.get().saturating_add(rest));
    }

    /// Holds back `bytes` for passes that come after the ones the next [`Ctx::expect_rest`]
    /// counts (a packed image's check through its packs while it is checked as written);
    /// 0 once they are the ones it counts.
    pub(crate) fn reserve(&self, bytes: u64) {
        self.later.set(bytes);
    }

    fn close_pass(&self) {
        let (done, _) = self.pass.replace((0, 0));
        self.base.set(self.base.get().saturating_add(done));
    }

    /// `done` of `total` bytes in the running pass of `stage`. A `total` of 0 or 1 only
    /// marks a stage and ends the running pass. Emitted as job-wide `done`/`total`: a pass
    /// starts where the last one ended. Passed on when the stage changes, a pass
    /// completes, or enough time has passed.
    pub(crate) fn progress(&self, stage: &str, done: u64, total: u64) {
        let new_stage = *self.stage.borrow() != stage;
        if new_stage || total <= 1 {
            self.close_pass();
        }
        if total > 1 {
            let (was_done, was_total) = self.pass.get();
            if done < was_done || total != was_total {
                self.close_pass();
            }
            self.pass.set((done, total));
        }
        let (pass_done, pass_total) = self.pass.get();
        let job_done = self.base.get().saturating_add(pass_done);
        let job_total = self
            .expected
            .get()
            .max(self.base.get().saturating_add(pass_total));

        let now = Instant::now();
        let due = self.last.get().is_none_or(|t| now - t >= PROGRESS_EVERY);
        if !(new_stage || due || done >= total) {
            return;
        }
        if new_stage {
            *self.stage.borrow_mut() = stage.to_string();
        }
        self.last.set(Some(now));
        (self.emit)(Event::Progress {
            job: self.job,
            stage: stage.to_string(),
            done: job_done,
            total: job_total,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_is_the_delete_guard() {
        let dir = crate::test_dir("jobs-uses");
        let host = dir.join("host");
        std::fs::create_dir_all(host.join("pack")).unwrap();
        std::fs::write(dir.join("game.exfat"), b"img").unwrap();
        std::fs::write(dir.join("other.exfat"), b"img").unwrap();
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": dir.join("game.exfat"), "format": "exfat", "output": dir.join("out")
        }))
        .unwrap();
        req.edits = vec![crate::Edit::Add {
            from: host.join("pack"),
            to: "pack".into(),
        }];
        let inner = Inner::new(Box::new(|_| {}));
        inner.state().paths.insert(1, used_paths(1, &req));
        // The image, its journal (not there yet), an added folder, inside it (new paths too),
        // above it.
        for used in [
            dir.join("game.exfat"),
            dir.join("game.exfat.forge-undo"),
            host.join("pack"),
            host.join("pack/../pack"),
            host.join("pack/new/file"),
            host.clone(),
            dir.clone(),
        ] {
            assert!(inner.uses(&used), "{}", used.display());
        }
        for free in [
            dir.join("other.exfat"),
            dir.join("missing"),
            host.join("new"),
            dir.join("out"),
        ] {
            assert!(!inner.uses(&free), "{}", free.display());
        }
        inner.state().paths.clear();
        assert!(!inner.uses(&dir.join("game.exfat")));

        // An Extract: what it makes in the output folder, not the folder's other entries.
        req.edits.clear();
        req.extract = vec!["data/movies".into(), "eboot.bin".into()];
        inner.state().paths.insert(2, used_paths(2, &req));
        for used in [
            dir.join("out/movies/a.mp4"),
            dir.join("out/eboot.bin"),
            dir.join("out"),
        ] {
            assert!(inner.uses(&used), "{}", used.display());
        }
        assert!(!inner.uses(&dir.join("out/other")));
    }

    #[test]
    fn install_guards_existing_sidecar_from_delete_and_move() {
        let dir = crate::test_dir("jobs-install-sidecar");
        let package = dir.join("game.pkg");
        let sidecar = package.with_extension("crc");
        std::fs::write(&package, b"package").unwrap();
        std::fs::write(&sidecar, b"crc").unwrap();
        let req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": "", "format": "folder", "output": "", "install": package
        }))
        .unwrap();
        assert_eq!(read_paths(&req), vec![package.clone(), sidecar.clone()]);
        let inner = Inner::new(Box::new(|_| {}));
        inner.state().paths.insert(1, used_paths(1, &req));
        assert!(inner.uses(&sidecar));
        assert!(!inner.uses(&dir.join("other.crc")));
        // Another device's sidecar is guarded without asking it.
        let remote = Path::new("flink://0123456789abcdef/games/game.pkg");
        let paths = install_paths(remote);
        assert_eq!(
            paths[1],
            Path::new("flink://0123456789abcdef/games/game.crc")
        );
    }
}
