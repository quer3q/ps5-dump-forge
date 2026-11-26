//! U8: holds off the PS5's auto rest mode while anything long runs: one process-wide count;
//! above 0 a ticker calls `sceSystemServicePowerTick` at once and every 30 s; elsewhere a count.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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
        fn sceSystemServicePowerTick() -> i32;
    }
    // SAFETY: no arguments, no memory handed over. ponytail: a failing tick goes unreported.
    let _ = unsafe { sceSystemServicePowerTick() };
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    tick: Option<fn()>,
    every: Duration,
}

struct State {
    count: usize,
    /// 0 -> 1 transitions: the ticker ticks at once for each one it hasn't seen.
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
                // ponytail: a failed spawn goes unreported; the next 0 -> 1 tries again.
                state.ticker = std::thread::Builder::new()
                    .name("forge-power-tick".into())
                    .spawn(move || self.ticker(tick))
                    .is_ok();
            }
            self.wake.notify_all();
        }
        Awake(self)
    }

    /// Lives as long as the process, asleep while the count is 0.
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
