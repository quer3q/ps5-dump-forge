//! U8: holds off the console's auto rest mode while anything long runs here (a job, a paired
//! device's reads or writes, a browser's upload or download). One process-wide count: while
//! it is above 0 one ticker thread calls `sceSystemServicePowerTick` at once (on 0 -> 1) and
//! then every 30 s; back at 0 it waits again, and auto rest mode is allowed. Rest mode entered
//! by hand is not held off. Elsewhere only the count is kept (no thread, no tick).

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Held while auto rest mode must wait; see [`awake`]. Dropping it lets the count go down.
#[must_use = "rest mode is held off only while this is held"]
pub struct Awake(&'static Shared);

/// One more holder of the count: the ticker ticks at once if it was 0.
pub fn awake() -> Awake {
    #[cfg(target_env = "ps5")]
    static SHARED: Shared = Shared::new(Some(power_tick), Duration::from_secs(30));
    #[cfg(not(target_env = "ps5"))]
    static SHARED: Shared = Shared::new(None, Duration::from_secs(30));
    SHARED.hold()
}

#[cfg(target_env = "ps5")]
fn power_tick() {
    unsafe extern "C" {
        /// libSceSystemService: resets the idle timer auto rest mode counts.
        fn sceSystemServicePowerTick() -> i32;
    }
    // SAFETY: no arguments, no memory handed over. ponytail: a failing tick goes unreported.
    let _ = unsafe { sceSystemServicePowerTick() };
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    /// `None`: count only (not the PS5).
    tick: Option<fn()>,
    every: Duration,
}

struct State {
    count: usize,
    /// 0 -> 1 transitions so far: the ticker ticks at once for each one it hasn't seen.
    starts: u64,
    ticker: bool,
}

impl Shared {
    const fn new(tick: Option<fn()>, every: Duration) -> Self {
        Self {
            state: Mutex::new(State {
                count: 0,
                starts: 0,
                ticker: false,
            }),
            wake: Condvar::new(),
            tick,
            every,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn hold(&'static self) -> Awake {
        let mut state = self.lock();
        state.count += 1;
        if state.count == 1 {
            state.starts += 1;
            if let Some(tick) = self.tick.filter(|_| !state.ticker) {
                // ponytail: a thread that fails to start goes unreported (the next 0 -> 1
                // tries again); the job still runs, and auto rest mode may cut it short.
                state.ticker = std::thread::Builder::new()
                    .name("forge-power-tick".into())
                    .spawn(move || self.ticker(tick))
                    .is_ok();
            }
            self.wake.notify_all();
        }
        Awake(self)
    }

    /// The ticker thread: lives as long as the process, asleep while the count is 0.
    fn ticker(&self, tick: fn()) {
        let mut seen = 0;
        let mut last = Instant::now();
        let mut state = self.lock();
        loop {
            while state.count == 0 {
                state = self.wake.wait(state).unwrap_or_else(|e| e.into_inner());
            }
            if state.starts != seen || last.elapsed() >= self.every {
                seen = state.starts;
                drop(state);
                tick();
                last = Instant::now();
                state = self.lock();
                continue;
            }
            let left = self.every.saturating_sub(last.elapsed());
            state = (self.wake.wait_timeout(state, left))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.count = state.count.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;

    static TICKS: AtomicUsize = AtomicUsize::new(0);
    // A period long against a busy CI runner's stalls: the checks below run well inside one.
    static SHARED: Shared = Shared::new(Some(count), Duration::from_millis(500));

    fn count() {
        TICKS.fetch_add(1, Ordering::SeqCst);
    }

    fn ticks() -> usize {
        TICKS.load(Ordering::SeqCst)
    }

    /// Waits up to 3 s for `want` ticks.
    fn wait_for(want: usize) -> usize {
        for _ in 0..300 {
            if ticks() >= want {
                break;
            }
            sleep(Duration::from_millis(10));
        }
        ticks()
    }

    #[test]
    fn ticks_at_once_then_every_period_and_stops_at_zero() {
        // At once on 0 -> 1, not again for a second holder.
        let first = SHARED.hold();
        assert_eq!(wait_for(1), 1);
        let second = SHARED.hold();
        sleep(Duration::from_millis(30));
        assert_eq!(ticks(), 1);
        // Every period while held, by either holder.
        drop(first);
        assert!(wait_for(3) >= 3);
        // Back at 0: no more ticks.
        drop(second);
        sleep(Duration::from_millis(20));
        let stopped = ticks();
        sleep(Duration::from_millis(700));
        assert_eq!(ticks(), stopped);
        assert_eq!(SHARED.lock().count, 0);
        // Held again within a period: at once again.
        let again = SHARED.hold();
        assert_eq!(wait_for(stopped + 1), stopped + 1);
        drop(again);
    }

    #[test]
    fn off_the_ps5_it_only_counts() {
        static COUNTED: Shared = Shared::new(None, Duration::from_millis(1));
        let a = COUNTED.hold();
        let b = COUNTED.hold();
        assert_eq!(COUNTED.lock().count, 2);
        assert!(!COUNTED.lock().ticker);
        drop((a, b));
        assert_eq!(COUNTED.lock().count, 0);
    }
}
