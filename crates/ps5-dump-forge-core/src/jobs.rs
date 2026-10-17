//! Worker threads: one per job, one heavy job running at a time, the rest waiting in FIFO
//! order. Each job has its own cancel flag.

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

pub(crate) struct Inner {
    emit: Emit,
    next: AtomicU64,
    state: Mutex<State>,
    /// Signalled when the running job finishes or a waiting one is cancelled.
    turn: Condvar,
}

#[derive(Default)]
struct State {
    /// Waiting and running jobs; the front one runs.
    queue: VecDeque<JobId>,
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
        })
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
        let state = self.state();
        crate::delete::delete(path, protected, state.paths.values().flatten())
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
    let mut state = inner.state();
    state.threads.retain(|t| !t.is_finished());
    state.queue.push_back(job);
    state.cancels.insert(job, cancel.clone());
    state.paths.insert(job, used_paths(job, &request));
    let worker = inner.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("forge-job-{job}"))
        .spawn(move || worker.run(job, &request, cancel));
    match spawned {
        Ok(handle) => state.threads.push(handle),
        Err(e) => {
            state.queue.retain(|j| *j != job);
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

/// The paths a job reads or writes: source, output (the source, in place), its `.part`s
/// (`finalize::part_path`, and `Part::replace`'s `.orig.part`), an archive's later parts,
/// profile and traces (with a lone journal's index); in place, the undo journal; for an edit,
/// every file or folder it adds.
fn used_paths(job: JobId, req: &ConvertRequest) -> Vec<PathBuf> {
    let out = if req.lz4_in_place || !req.edits.is_empty() {
        &req.source
    } else {
        &req.output
    };
    let part = crate::finalize::part_path(out, job);
    let orig = part.with_extension("orig.part");
    let mut paths = vec![req.source.clone(), out.clone(), part, orig];
    // An archive's later parts, found by name beside it (the source is the first part).
    if crate::archive::is_archive(&req.source) {
        let folder = req.source.parent().filter(|p| !p.as_os_str().is_empty());
        if let Ok(items) = std::fs::read_dir(folder.unwrap_or(Path::new("."))) {
            paths.extend(
                (items.flatten().map(|i| i.path()))
                    .filter(|p| ps5_dump_forge_archive::later_part_of(p, &req.source)),
            );
        }
    }
    paths.extend(req.lz4_profile.iter().chain(&req.lz4_traces).cloned());
    // An in-place job's undo journal (an edit's own; one an LZ4 in-place job refuses).
    if req.lz4_in_place || !req.edits.is_empty() {
        paths.push(ps5_dump_forge_patch::journal_path(&req.source));
    }
    if !req.edits.is_empty() {
        paths.extend(req.edits.iter().filter_map(|e| match e {
            crate::Edit::Add { from, .. } => Some(from.clone()),
            _ => None,
        }));
    }
    // A journal named on its own is read with the index beside it.
    if let Some(traces) = &req.lz4_traces
        && !traces
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        paths.push(traces.with_file_name(crate::LZ4_TRACE_FILES[1]));
    }
    paths
}

impl Inner {
    fn run(&self, job: JobId, request: &ConvertRequest, shared: Arc<AtomicBool>) {
        let cancel: &AtomicBool = &shared;
        // Dropped last: Done goes out before the next job is woken, so no event of the next
        // job can arrive before this job's Done. A panicking `emit` still dequeues.
        let _leave = Leave(self, job);
        let result = self.wait_turn(job, cancel).and_then(|()| {
            #[cfg(target_env = "ps5")]
            let _awake = PowerTick::start();
            let mut ctx = Ctx::new(job, &self.emit, cancel);
            ctx.shared_cancel = Some(shared.clone());
            match catch_unwind(AssertUnwindSafe(|| crate::convert::run(request, &ctx))) {
                Ok(result) => result,
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
        let result = match result.map_err(|e| format!("{e:#}")) {
            Ok(report) => Ok(report),
            // An edit's "cancelled; …" says what stays changed (a folder edit has no rollback).
            Err(e) if cancel.load(Ordering::Relaxed) && !e.starts_with("cancelled; ") => {
                Err("cancelled".to_string())
            }
            Err(e) => Err(e),
        };
        // Its files are settled (published or cleaned up): they may be deleted from now on.
        self.state().paths.remove(&job);
        (self.emit)(Event::Done { job, result });
    }

    /// Blocks until `job` is at the front of the queue, or fails if it is cancelled first.
    fn wait_turn(&self, job: JobId, cancel: &AtomicBool) -> anyhow::Result<()> {
        let mut state = self.state();
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            if state.queue.front() == Some(&job) {
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
            state.cancels.remove(&self.1);
            state.paths.remove(&self.1);
        }
        self.0.turn.notify_all();
    }
}

/// U8: holds off the console's auto rest mode while a job runs, by calling
/// `sceSystemServicePowerTick` now and every [`PowerTick::EVERY`] on a thread of its own.
/// Dropping it (the job ended, or unwound) stops and joins the thread at once. Rest mode
/// entered by hand is not held off.
#[cfg(target_env = "ps5")]
struct PowerTick(Option<(std::sync::mpsc::Sender<()>, JoinHandle<()>)>);

#[cfg(target_env = "ps5")]
impl PowerTick {
    const EVERY: Duration = Duration::from_secs(30);

    fn start() -> Self {
        unsafe extern "C" {
            /// libSceSystemService: resets the idle timer auto rest mode counts.
            fn sceSystemServicePowerTick() -> i32;
        }
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        // ponytail: a thread that fails to start, or a failing tick, goes unreported: the job
        // still runs, and auto rest mode may cut it short.
        let thread = std::thread::Builder::new()
            .name("forge-power-tick".into())
            .spawn(move || {
                use std::sync::mpsc::RecvTimeoutError;
                loop {
                    // SAFETY: no arguments, no memory handed over.
                    let _ = unsafe { sceSystemServicePowerTick() };
                    if stopped.recv_timeout(Self::EVERY) != Err(RecvTimeoutError::Timeout) {
                        return;
                    }
                }
            });
        Self(thread.ok().map(|t| (stop, t)))
    }
}

#[cfg(target_env = "ps5")]
impl Drop for PowerTick {
    fn drop(&mut self) {
        if let Some((stop, thread)) = self.0.take() {
            drop(stop); // wakes the ticker's wait as disconnected
            let _ = thread.join();
        }
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
    fn a_folder_edit_check_waits_for_a_job_using_the_folder() {
        let dir = crate::test_dir("jobs-folder-check");
        std::fs::create_dir_all(dir.join("game")).unwrap();
        let mut req: ConvertRequest = serde_json::from_value(serde_json::json!({
            "source": dir.join("game"), "format": "exfat", "output": dir.join("out")
        }))
        .unwrap();
        req.edits = vec![crate::Edit::Mkdir { path: "x".into() }];
        let inner = Inner::new(Box::new(|_| {}));
        inner.state().paths.insert(1, vec![dir.join("game")]);
        let check = inner.edit_check(&req);
        assert_eq!(check.in_place.len(), 1, "{check:?}");
        assert!(check.in_place[0].starts_with("a job is using"), "{check:?}");
        // A folder is never rebuilt: no rebuild findings, no sizes.
        assert!(check.rebuild.is_empty() && check.rebuild_needs.is_none());
    }

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
        // The image, its journal (not there yet), an added folder, inside it and above it.
        for used in [
            dir.join("game.exfat"),
            dir.join("game.exfat.forge-undo"),
            host.join("pack"),
            host.join("pack/../pack"),
            host.clone(),
            dir.clone(),
        ] {
            assert!(inner.uses(&used), "{}", used.display());
        }
        for free in [
            dir.join("other.exfat"),
            dir.join("missing"),
            dir.join("out"),
        ] {
            assert!(!inner.uses(&free), "{}", free.display());
        }
        inner.state().paths.clear();
        assert!(!inner.uses(&dir.join("game.exfat")));
    }
}
