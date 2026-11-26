//! Identical source reads share one run (issue #74: a long PS5 read outlived the browser's
//! request, and asking again met the cap of 2). A finished reply is kept for [`KEEP`] for one
//! `?retry=1`; a fresh request never takes it, so a Refresh is never answered from before it.
//! Keys are BLAKE3 hashes of route + body, so no body (and no password in one) is kept.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::http::Response;

type Reply = Result<Response, Response>;

const KEEP: Duration = Duration::from_secs(120);
/// Then 202 (the page asks again): shorter than a browser keeps a request open.
const WAIT_MAX: Duration = Duration::from_secs(60);
/// Past it a read runs unshared.
// ponytail: a retry whose reply was pushed out or expired reads again.
const MAX_RUNS: usize = 8;

#[derive(Default)]
pub(crate) struct Reads {
    runs: Mutex<HashMap<[u8; 32], Entry>>,
}

struct Entry {
    run: Arc<Run>,
    /// `None` while in flight.
    finished: Option<Instant>,
}

#[derive(Default)]
struct Run {
    state: Mutex<State>,
    ready: Condvar,
}

#[derive(Default)]
enum State {
    #[default]
    Running,
    Done(Reply),
    /// The run panicked, or its reply can't be copied.
    Gone,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn copy(reply: &Reply) -> Option<Reply> {
    match reply {
        Ok(r) => r.duplicate().map(Ok),
        Err(r) => r.duplicate().map(Err),
    }
}

/// Ends a run for every waiter (`Gone` when the call unwinds).
struct Leader<'a> {
    reads: &'a Reads,
    key: [u8; 32],
    run: Arc<Run>,
}

impl Leader<'_> {
    fn end(&self, reply: Option<&Reply>) {
        let shared = reply.and_then(copy);
        // Busy or stopping says nothing about the source: never kept.
        let keep = shared
            .as_ref()
            .is_some_and(|r| !matches!(r, Err(e) if e.status == 429 || e.status == 409));
        {
            let mut runs = lock(&self.reads.runs);
            let ours = runs
                .get(&self.key)
                .is_some_and(|e| Arc::ptr_eq(&e.run, &self.run));
            if ours {
                if keep {
                    if let Some(e) = runs.get_mut(&self.key) {
                        e.finished = Some(Instant::now());
                    }
                } else {
                    runs.remove(&self.key);
                }
            }
        }
        *lock(&self.run.state) = match shared {
            Some(r) => State::Done(r),
            None => State::Gone,
        };
        self.run.ready.notify_all();
    }
}

impl Drop for Leader<'_> {
    fn drop(&mut self) {
        if matches!(*lock(&self.run.state), State::Running) {
            self.end(None);
        }
    }
}

impl Reads {
    pub fn run(
        &self,
        route: &str,
        body: &[u8],
        retry: bool,
        stopping: &AtomicBool,
        call: impl FnOnce() -> Reply,
    ) -> Reply {
        let mut hasher = blake3::Hasher::new();
        hasher.update(route.as_bytes()).update(&[0]).update(body);
        let key = *hasher.finalize().as_bytes();
        let run = {
            let mut runs = lock(&self.runs);
            runs.retain(|_, e| e.finished.is_none_or(|t| t.elapsed() < KEEP));
            match runs.get(&key) {
                Some(e) if e.finished.is_none() => Some(Err(e.run.clone())),
                // A kept reply, taken by this retry: waited for, since `Leader::end` marks the
                // entry finished just before it stores the reply.
                Some(_) if retry => runs.remove(&key).map(|e| Err(e.run)),
                found => {
                    if found.is_some() {
                        runs.remove(&key);
                    }
                    if runs.len() >= MAX_RUNS {
                        let oldest = runs
                            .iter()
                            .filter_map(|(k, e)| e.finished.map(|t| (t, *k)))
                            .min()
                            .map(|(_, k)| k);
                        if let Some(k) = oldest {
                            runs.remove(&k);
                        }
                    }
                    if runs.len() >= MAX_RUNS {
                        None
                    } else {
                        let run = Arc::new(Run::default());
                        let entry = Entry {
                            run: run.clone(),
                            finished: None,
                        };
                        runs.insert(key, entry);
                        Some(Ok(run))
                    }
                }
            }
        };
        let run = match run {
            None => return call(),
            Some(Err(run)) => return wait(&run, stopping),
            Some(Ok(run)) => run,
        };
        let leader = Leader {
            reads: self,
            key,
            run,
        };
        let reply = call();
        leader.end(Some(&reply));
        reply
    }
}

fn wait(run: &Run, stopping: &AtomicBool) -> Reply {
    let start = Instant::now();
    let mut state = lock(&run.state);
    loop {
        match &*state {
            State::Done(r) => return copy(r).unwrap_or_else(|| Err(gone())),
            State::Gone => return Err(gone()),
            State::Running => {}
        }
        if stopping.load(Ordering::SeqCst) {
            return Err(Response::error(409, "Dump Forge is stopping"));
        }
        if start.elapsed() >= WAIT_MAX {
            return Err(Response::error(202, "still reading; ask again"));
        }
        state = run
            .ready
            .wait_timeout(state, Duration::from_millis(100))
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

fn gone() -> Response {
    Response::error(500, "the same read failed; try again")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn text(r: Reply) -> String {
        match r {
            Ok(r) => r.text(),
            Err(r) => panic!("{} {}", r.status, r.text()),
        }
    }

    #[test]
    fn identical_reads_share_one_run() {
        let reads = Reads::default();
        let stopping = AtomicBool::new(false);
        let runs = AtomicUsize::new(0);
        let call = || {
            let n = runs.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(300));
            Ok(Response::json(&n))
        };
        let body = br#"{"path":"/data/game"}"#;
        let (a, b) = std::thread::scope(|s| {
            let a = s.spawn(|| reads.run("inspect", body, false, &stopping, call));
            std::thread::sleep(Duration::from_millis(50));
            let b = s.spawn(|| reads.run("inspect", body, false, &stopping, call));
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(text(a), "0");
        assert_eq!(text(b), "0");
        let kept = text(reads.run("inspect", body, true, &stopping, call));
        assert_eq!((kept, runs.load(Ordering::SeqCst)), ("0".into(), 1));
        // Taken once: the next retry, and any other route, read again.
        let again = text(reads.run("inspect", body, true, &stopping, call));
        assert_eq!((again, runs.load(Ordering::SeqCst)), ("1".into(), 2));
        text(reads.run("host_tree", body, true, &stopping, call));
        assert_eq!(runs.load(Ordering::SeqCst), 3);
        // A fresh request never takes a kept reply.
        let fresh = text(reads.run("inspect", body, false, &stopping, call));
        assert_eq!((fresh, runs.load(Ordering::SeqCst)), ("3".into(), 4));
    }
}
