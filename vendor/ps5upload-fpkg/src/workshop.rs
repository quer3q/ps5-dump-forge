//! The Workshop's share of this computer: pausing a running job (#631) and "use less of this
//! computer" (#632).
//!
//! **Pause.** A job is known to its workers by its cancel flag, which every heavy loop already
//! checks. [`register`] gives that flag a pause gate, and [`checkpoint`] — called where the
//! loops checked the flag — blocks while the gate is closed, so a paused job's producer stops
//! reading and its pool, starved of work by the bounded queues between them, stops computing.
//! A cancel while paused stops at once: the wait wakes on [`wake`] and re-checks the flag every
//! [`WAIT_SLICE`] whatever happens. With no job paused a checkpoint is one atomic load more than
//! the plain flag check it replaces.
//!
//! **Use less of this computer.** One switch, [`set_gentle`]. Jobs started while it is on use
//! about half the cores ([`workshop_threads`]), and every Workshop thread runs at low priority
//! (macOS: the utility QoS class; Linux: nice 10 and the lowest best-effort I/O level; Windows:
//! background mode), which on all three also lowers its disk priority: the machine is
//! handed back whenever anything else wants it, which a "use N% of the CPU" cap cannot promise
//! the same way on Windows, macOS and Linux. A thread follows the switch at its next checkpoint,
//! so flipping it applies to a running job too. Only threads that belong to a Workshop job
//! ([`on_job_thread`] and the workers it spawns through a [`Lane`]) are ever lowered: the engine
//! also serves uploads, Remote Play and the UI, which keep their normal priority.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

/// The longest a paused checkpoint sleeps before looking at the cancel flag again on its own.
pub const WAIT_SLICE: Duration = Duration::from_millis(250);

static GENTLE: AtomicBool = AtomicBool::new(false);

/// Turn "use less of this computer" on or off for every Workshop job, running ones included.
pub fn set_gentle(on: bool) {
    GENTLE.store(on, Ordering::Relaxed);
}

/// Whether "use less of this computer" is on.
pub fn gentle() -> bool {
    GENTLE.load(Ordering::Relaxed)
}

static CAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The most worker threads any job of this process starts, whatever the core count (0, the
/// default: no cap). The converter on the PS5 sets it to its share of the console's CPUs.
pub fn set_thread_cap(threads: usize) {
    CAP.store(threads, Ordering::Relaxed);
}

/// How many worker threads a Workshop job starts: every core, or about half of them while
/// [`gentle`] is on; never more than [`set_thread_cap`] said.
pub fn workshop_threads() -> usize {
    let n = threads_for(
        std::thread::available_parallelism().map_or(4, |n| n.get()),
        gentle(),
    );
    capped(n, CAP.load(Ordering::Relaxed))
}

/// `n` threads under a cap of `cap` (0: none).
fn capped(n: usize, cap: usize) -> usize {
    match cap {
        0 => n,
        cap => n.min(cap),
    }
}

/// [`workshop_threads`] for a machine with `cores` cores.
pub fn threads_for(cores: usize, gentle: bool) -> usize {
    if gentle {
        (cores / 2).max(1)
    } else {
        cores.max(1)
    }
}

// ── pause gates ─────────────────────────────────────────────────────────────────────────────

struct Gate {
    /// The job's cancel flag. Held weakly, which also keeps its address from being reused by
    /// another flag while this entry exists, so the address is a sound key.
    cancel: Weak<AtomicBool>,
    state: Mutex<GateState>,
    wake: Condvar,
}

#[derive(Default)]
struct GateState {
    paused_since: Option<Instant>,
    /// Paused time before the current pause.
    paused_before: Duration,
}

impl GateState {
    fn paused_for(&self) -> Duration {
        self.paused_before + self.paused_since.map_or(Duration::ZERO, |t| t.elapsed())
    }
}

fn gates() -> &'static Mutex<HashMap<usize, Arc<Gate>>> {
    static GATES: OnceLock<Mutex<HashMap<usize, Arc<Gate>>>> = OnceLock::new();
    GATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Gates that are closed (their job paused). Zero is the checkpoints' fast path.
static CLOSED: AtomicUsize = AtomicUsize::new(0);

fn key(cancel: &AtomicBool) -> usize {
    cancel as *const AtomicBool as usize
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Drop the gates of jobs that have ended, recounting the closed ones.
fn prune(map: &mut HashMap<usize, Arc<Gate>>) {
    map.retain(|_, g| g.cancel.strong_count() > 0);
    let closed = map
        .values()
        .filter(|g| lock(&g.state).paused_since.is_some())
        .count();
    CLOSED.store(closed, Ordering::Release);
}

/// Make the job that owns `cancel` pausable. Its gate lives as long as the flag does.
pub fn register(cancel: &Arc<AtomicBool>) {
    let mut map = lock(gates());
    prune(&mut map);
    map.insert(
        key(cancel),
        Arc::new(Gate {
            cancel: Arc::downgrade(cancel),
            state: Mutex::new(GateState::default()),
            wake: Condvar::new(),
        }),
    );
}

fn gate_of(map: &HashMap<usize, Arc<Gate>>, cancel: &AtomicBool) -> Option<Arc<Gate>> {
    map.get(&key(cancel))
        .filter(|g| g.cancel.strong_count() > 0)
        .cloned()
}

/// Pause or resume the job that owns `cancel`. False when it is not a pausable job (never
/// registered, or already over).
pub fn set_paused(cancel: &AtomicBool, paused: bool) -> bool {
    let mut map = lock(gates());
    let Some(gate) = gate_of(&map, cancel) else {
        prune(&mut map);
        return false;
    };
    {
        let mut st = lock(&gate.state);
        match (st.paused_since, paused) {
            (None, true) => st.paused_since = Some(Instant::now()),
            (Some(since), false) => {
                st.paused_before += since.elapsed();
                st.paused_since = None;
            }
            _ => {}
        }
    }
    prune(&mut map);
    drop(map);
    gate.wake.notify_all();
    true
}

/// A pausable job's pause state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseStatus {
    pub paused: bool,
    /// Time spent paused so far, the current pause included.
    pub paused_ms: u64,
}

/// The pause state of the job that owns `cancel`, or None when it is not a pausable job.
pub fn status(cancel: &AtomicBool) -> Option<PauseStatus> {
    let gate = gate_of(&lock(gates()), cancel)?;
    let st = lock(&gate.state);
    Some(PauseStatus {
        paused: st.paused_since.is_some(),
        paused_ms: st.paused_for().as_millis() as u64,
    })
}

/// Wake the checkpoints of the job that owns `cancel`; call after setting the flag so a paused
/// job stops now rather than within [`WAIT_SLICE`].
pub fn wake(cancel: &AtomicBool) {
    if let Some(gate) = gate_of(&lock(gates()), cancel) {
        // Taken so a waiter between its check and its wait cannot miss the notification.
        drop(lock(&gate.state));
        gate.wake.notify_all();
    }
}

/// Where a Workshop loop checks for a cancel: follows "use less of this computer", waits here
/// while the job is paused, and says whether the job was cancelled.
pub fn checkpoint(cancel: &AtomicBool) -> bool {
    sync_thread_priority();
    if cancel.load(Ordering::Relaxed) {
        return true;
    }
    if CLOSED.load(Ordering::Acquire) == 0 {
        return false;
    }
    let Some(gate) = gate_of(&lock(gates()), cancel) else {
        return false;
    };
    let mut st = lock(&gate.state);
    while st.paused_since.is_some() && !cancel.load(Ordering::Relaxed) {
        st = gate
            .wake
            .wait_timeout(st, WAIT_SLICE)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    drop(st);
    sync_thread_priority();
    cancel.load(Ordering::Relaxed)
}

/// [`checkpoint`] for a loop whose cancel flag is optional: no flag, never cancelled.
pub fn checkpoint_opt(cancel: Option<&AtomicBool>) -> bool {
    match cancel {
        Some(c) => checkpoint(c),
        None => {
            sync_thread_priority();
            false
        }
    }
}

// ── thread priority ─────────────────────────────────────────────────────────────────────────

thread_local! {
    /// This thread belongs to a Workshop job, so "use less of this computer" applies to it.
    static WORKSHOP: Cell<bool> = const { Cell::new(false) };
    /// This thread runs at background priority now.
    static LOWERED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the current thread belongs to a Workshop job; carried into the workers a job
/// spawns, so they follow the switch too.
#[derive(Debug, Clone, Copy)]
pub struct Lane(bool);

impl Lane {
    /// The current thread's lane, to hand to the threads it spawns.
    pub fn current() -> Self {
        Lane(WORKSHOP.with(Cell::get))
    }

    /// Called first thing on a spawned worker: it joins its parent's lane.
    pub fn enter(self) {
        WORKSHOP.with(|w| w.set(self.0));
        sync_thread_priority();
    }
}

/// Run a Workshop job's work on a thread of its own, joined before this returns. The caller's
/// thread (a shared pool thread, usually) is never lowered: a lowered thread could not always
/// be raised again (Linux needs a privilege to undo a nice), and the pool runs other work next.
pub fn on_job_thread<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("workshop-job".into())
            .spawn_scoped(s, || {
                Lane(true).enter();
                work()
            })
            .expect("start the Workshop job's thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Bring the current thread's priority in line with [`gentle`]: background while it is on (for
/// a Workshop thread), normal otherwise. Cheap when nothing changes.
pub fn sync_thread_priority() {
    let want = WORKSHOP.with(Cell::get) && gentle();
    if LOWERED.with(Cell::get) != want {
        os::set_background(want);
        LOWERED.with(|l| l.set(want));
    }
}

/// Whether the current thread runs at background priority (for tests and logs).
pub fn thread_lowered() -> bool {
    LOWERED.with(Cell::get)
}

#[cfg(target_os = "macos")]
mod os {
    use std::cell::Cell;
    use std::os::raw::c_int;

    // Declared here with plain integers: libc's `qos_class_t` is an enum, and reading a class
    // it does not list into one would be undefined.
    extern "C" {
        fn pthread_set_qos_class_self_np(class: u32, relative: c_int) -> c_int;
        fn pthread_get_qos_class_np(
            thread: libc::pthread_t,
            class: *mut u32,
            rel: *mut c_int,
        ) -> c_int;
    }
    /// `QOS_CLASS_UTILITY`: Apple's class for long work with a progress bar. Other apps' threads
    /// (default and user-initiated) get the cores first, and its disk I/O is throttled behind
    /// theirs. Not `QOS_CLASS_BACKGROUND`: on a busy Mac (load average 60) a 1 GiB `.ffpfsc` wrap
    /// took 0.9 s at normal priority and 1.5 s at utility with half the cores, but had written
    /// 175 MiB after two minutes at background, which is a stopped conversion, not a slower one.
    const UTILITY: u32 = 0x11;
    /// `QOS_CLASS_DEFAULT`, when the thread's own class could not be read.
    const DEFAULT: u32 = 0x15;

    thread_local! {
        static BEFORE: Cell<Option<u32>> = const { Cell::new(None) };
    }

    pub(super) fn set_background(on: bool) {
        // SAFETY: plain calls on the current thread with valid out-pointers.
        unsafe {
            if on {
                let (mut class, mut rel) = (0u32, 0 as c_int);
                let read =
                    pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut rel) == 0;
                BEFORE.with(|b| b.set(Some(if read && class != 0 { class } else { DEFAULT })));
                pthread_set_qos_class_self_np(UTILITY, 0);
            } else {
                let class = BEFORE.with(|b| b.take()).unwrap_or(DEFAULT);
                pthread_set_qos_class_self_np(class, 0);
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod os {
    /// Nice 10: other programs' threads at the usual 0 get the cores first.
    const NICE: libc::c_int = 10;
    const IOPRIO_WHO_PROCESS: libc::c_long = 1;
    const IOPRIO_CLASS_SHIFT: libc::c_long = 13;
    const IOPRIO_CLASS_BE: libc::c_long = 2;
    /// Best-effort's lowest level: the disk serves everyone else first, but the job is never
    /// starved outright as the idle class can be.
    const IOPRIO_LOWEST: libc::c_long = 7;

    pub(super) fn set_background(on: bool) {
        // SAFETY: plain syscalls on the calling thread (who = its tid, or 0 for ioprio).
        unsafe {
            let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
            // Raising a nice back needs CAP_SYS_NICE (or RLIMIT_NICE); without it the thread
            // keeps nice 10 until the job ends, which only ever errs on the gentle side.
            libc::setpriority(libc::PRIO_PROCESS as _, tid, if on { NICE } else { 0 });
            let prio = if on {
                (IOPRIO_CLASS_BE << IOPRIO_CLASS_SHIFT) | IOPRIO_LOWEST
            } else {
                0 // IOPRIO_CLASS_NONE: back to following the CPU priority
            };
            libc::syscall(
                libc::SYS_ioprio_set,
                IOPRIO_WHO_PROCESS,
                0 as libc::c_long,
                prio,
            );
        }
    }
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }
    /// Background mode lowers the thread's CPU priority and its disk and memory priority.
    const THREAD_MODE_BACKGROUND_BEGIN: i32 = 0x0001_0000;
    const THREAD_MODE_BACKGROUND_END: i32 = 0x0002_0000;

    pub(super) fn set_background(on: bool) {
        // SAFETY: the pseudo-handle of the calling thread, which background mode requires.
        unsafe {
            SetThreadPriority(
                GetCurrentThread(),
                if on {
                    THREAD_MODE_BACKGROUND_BEGIN
                } else {
                    THREAD_MODE_BACKGROUND_END
                },
            );
        }
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "android",
    windows
)))]
mod os {
    pub(super) fn set_background(_on: bool) {}
}
