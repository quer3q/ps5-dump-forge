//! Durable writes for drives that drop data (the PS5's USB stack): fsync with ps5upload's retry
//! policy, and [`SyncEvery`], an output wrapper that bounds unsynced data. A sync that only
//! succeeded after a retry fails the job: pages the drive dropped in between cannot be told
//! apart, so the output is not trusted.
//!
//! A cancel during a long zero fill surfaces as an `io::Error` whose inner error
//! is [`Cancelled`]; callers tell it apart with `e.get_ref().is_some_and(|e| e.is::<Cancelled>())`.
//!
//! Also [`explain_removal`]: a failed job's error, told apart from a drive that went away.

use std::fmt;
use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The inner error of an `io::Error` a cancel produced.
#[derive(Debug)]
pub(crate) struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A copy of `e` for each later call that a latched failure fails: the errno, the cancel
/// marker, or the kind and message.
fn copy(e: &io::Error) -> io::Error {
    if let Some(code) = e.raw_os_error() {
        return io::Error::from_raw_os_error(code);
    }
    if e.get_ref().is_some_and(|e| e.is::<Cancelled>()) {
        return io::Error::other(Cancelled);
    }
    io::Error::new(e.kind(), e.to_string())
}

fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        // Not `Interrupted`: `write_all` retries those.
        return Err(io::Error::other(Cancelled));
    }
    Ok(())
}

/// The errno of `e`, with Sony's 0x8002xxxx form folded to the plain errno.
/// On Windows the raw code is a Win32 error, so nothing here matches it; no Windows caller.
pub(crate) fn errno(e: &io::Error) -> Option<i32> {
    // The PS5 kernel hands some errors back as 0x8002xxxx; the low 16 bits are the errno.
    e.raw_os_error().map(|c| {
        if c as u32 >> 16 == 0x8002 {
            c & 0xffff
        } else {
            c
        }
    })
}

/// EINTR, EAGAIN, EBUSY, ETIMEDOUT, ENOENT, ENXIO, ENODEV (after folding). Never EIO, ENOSPC, EROFS:
/// EIO is the kernel saying the data did not reach the drive, and asking again proves nothing.
pub(crate) fn is_transient(e: &io::Error) -> bool {
    use libc::{EAGAIN, EBUSY, EINTR, ENODEV, ENOENT, ENXIO, ETIMEDOUT};
    matches!(
        errno(e),
        Some(EINTR | EAGAIN | EBUSY | ETIMEDOUT | ENOENT | ENXIO | ENODEV)
    )
}

/// One path U7 re-checks when a job fails: a mount point (or, where `statfs` failed when it
/// was recorded, the folder itself) and its `st_dev` then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Watched {
    pub path: PathBuf,
    pub dev: u64,
    /// `path` is the mount point `statfs` named, not a folder on it.
    pub mount: bool,
}

/// U7: if a watched mount point's drive is gone, `e` is prefixed with it; otherwise it is `e`
/// unchanged. Asked on any failure, not by errno: some readers turn their I/O errors into
/// text. Gone: `now` (a path's `(st_dev, f_blocks)` today) shows another device or no blocks,
/// or fails on a mount point with an errno a lost drive gives (EIO, ENXIO, ENODEV, ENOENT).
/// A folder that is missing or unreadable (EACCES, ...) on a healthy drive is not a removal.
pub(crate) fn explain_removal(
    e: anyhow::Error,
    watched: &[Watched],
    now: impl Fn(&Path) -> io::Result<(u64, u64)>,
) -> anyhow::Error {
    use libc::{EIO, ENODEV, ENOENT, ENXIO};
    let gone = watched.iter().find(|w| match now(&w.path) {
        Ok((dev, blocks)) => dev != w.dev || blocks == 0,
        Err(err) => w.mount && matches!(errno(&err), Some(EIO | ENXIO | ENODEV | ENOENT)),
    });
    match gone {
        Some(w) => e.context(format!(
            "the drive at {} was removed or stopped responding",
            w.path.display()
        )),
        None => e,
    }
}

/// A filesystem without directory fsync. Comparisons, not a match: ENOTSUP == EOPNOTSUPP on
/// FreeBSD and Linux.
fn unsupported(e: &io::Error) -> bool {
    errno(e).is_some_and(|c| c == libc::EINVAL || c == libc::ENOTSUP || c == libc::EOPNOTSUPP)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Synced {
    Clean,
    /// Succeeded only after a retry: pages may have been lost in between.
    Retried,
    /// A directory on a filesystem that cannot fsync one.
    Unsupported,
}

/// ps5upload's `ava1_fsync_retry`: the first try and four retries.
const BACKOFF_MS: [u64; 4] = [20, 60, 200, 600];
/// Backoff is slept in slices this long, checking the cancel flag between them.
const SLICE_MS: u64 = 20;

/// fsync with retry. `dir`: a directory fsync that fails with EINVAL/ENOTSUP/EOPNOTSUPP is
/// Ok(Unsupported) (a filesystem without directory fsync); for a file those are errors.
/// A cancel during the backoff returns the last error.
pub(crate) fn sync_retry(f: &File, dir: bool, cancel: &AtomicBool) -> io::Result<Synced> {
    retry(|| fsync_once(f), dir, cancel, std::thread::sleep)
}

/// One try. std's `sync_all` retries EINTR itself on unix, which would hide it from the
/// backoff, the cancel check and `Synced::Retried`.
#[cfg(unix)]
fn fsync_once(f: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // macOS: F_FULLFSYNC, as `sync_all` does there; plain fsync stops at the drive's cache.
    // SAFETY: a valid descriptor and plain integer arguments.
    #[cfg(target_vendor = "apple")]
    let rc = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) };
    // SAFETY: a valid descriptor.
    #[cfg(not(target_vendor = "apple"))]
    let rc = unsafe { libc::fsync(f.as_raw_fd()) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn fsync_once(f: &File) -> io::Result<()> {
    f.sync_all()
}

/// What a sync that only succeeded after a retry fails with.
pub(crate) fn retried_sync() -> io::Error {
    io::Error::other(
        "the drive reported a temporary error while syncing, so written data may be lost; \
         the output is not trusted (try again)",
    )
}

fn retry(
    mut op: impl FnMut() -> io::Result<()>,
    dir: bool,
    cancel: &AtomicBool,
    mut sleep: impl FnMut(Duration),
) -> io::Result<Synced> {
    let mut attempt = 0;
    loop {
        let e = match op() {
            Ok(()) if attempt == 0 => return Ok(Synced::Clean),
            Ok(()) => return Ok(Synced::Retried),
            Err(e) => e,
        };
        if dir && unsupported(&e) {
            return Ok(Synced::Unsupported);
        }
        if !is_transient(&e) || attempt == BACKOFF_MS.len() {
            return Err(e);
        }
        let mut ms = BACKOFF_MS[attempt];
        while ms > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(e);
            }
            let step = ms.min(SLICE_MS);
            sleep(Duration::from_millis(step));
            ms -= step;
        }
        attempt += 1;
    }
}

/// What [`SyncEvery`] writes into: a seekable output it can fsync.
pub(crate) trait Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    /// Ok(true) when the sync only succeeded after a retry (pages may have been lost).
    fn sync(&mut self, cancel: &AtomicBool) -> io::Result<bool>;
}

impl Output for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(self, buf)
    }
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }
    fn sync(&mut self, cancel: &AtomicBool) -> io::Result<bool> {
        Ok(sync_retry(self, false, cancel)? == Synced::Retried)
    }
}

impl<O: Output + ?Sized> Output for &mut O {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (**self).write(buf)
    }
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        (**self).seek(pos)
    }
    fn sync(&mut self, cancel: &AtomicBool) -> io::Result<bool> {
        (**self).sync(cancel)
    }
}

// ponytail: calibration knobs, to be tuned on the /data hardware runs: N starts at 32 MiB and
// adapts to about one second of measured throughput, within 16..256 MiB.
const START_N: u64 = 32 << 20;
const MIN_N: u64 = 16 << 20;
const MAX_N: u64 = 256 << 20;
const TARGET: Duration = Duration::from_secs(1);

/// How many dirty bytes [`SyncEvery`] lets build up before a sync. One per job, shared by
/// every output the job writes, so what one file measured carries over to the next.
pub(crate) struct Cadence {
    n: u64,
    min: u64,
    max: u64,
    change: Option<(u64, u64)>,
}

impl Cadence {
    pub(crate) fn new() -> Self {
        Self {
            n: START_N,
            min: MIN_N,
            max: MAX_N,
            change: None,
        }
    }

    /// A fixed N, so tests stay small.
    #[cfg(test)]
    fn fixed(n: u64) -> Self {
        Self {
            n,
            min: n,
            max: n,
            change: None,
        }
    }

    pub(crate) fn n(&self) -> u64 {
        self.n
    }

    /// One full interval: `bytes` took `took` from the interval's start through its sync.
    pub(crate) fn record(&mut self, bytes: u64, took: Duration) {
        let rate = u128::from(bytes) * TARGET.as_nanos() / took.as_nanos().max(1);
        let n = u64::try_from(rate)
            .unwrap_or(u64::MAX)
            .clamp(self.min, self.max);
        if n != self.n {
            self.change = Some((self.n, n));
            self.n = n;
        }
    }

    /// The last change of N as `(old, new)`, once, for the caller to log.
    pub(crate) fn take_change(&mut self) -> Option<(u64, u64)> {
        self.change.take()
    }
}

/// Zeros for gap materialization.
static ZEROS: [u8; 1 << 20] = [0; 1 << 20];
/// The largest offset an `off_t` holds.
const MAX_OFF: u64 = i64::MAX as u64;

fn out_of_range() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "offset past the largest file offset",
    )
}

/// Bounds unsynced data inside every call: a sync once N bytes were written since the last one
/// (writes are split there), and gaps past the end are written as real zeros (no sparse holes,
/// no driver zero-filling a gap in one syscall). Seeks never sync. The output must start empty.
///
/// A failed sync point, a retried one ([`retried_sync`]) or a failed inner seek is latched: bytes
/// a write already handed to the output are still reported as written, and every later write,
/// seek and `finish` fails.
pub(crate) struct SyncEvery<'a, O: Output> {
    out: O,
    cadence: &'a mut Cadence,
    cancel: &'a AtomicBool,
    /// Cursor, for us and the inner output alike.
    pos: u64,
    /// Largest end offset written: the output's length.
    high: u64,
    /// Bytes written since the last sync, and when that sync ended.
    dirty: u64,
    since: Instant,
    /// Never cleared: the output's state past a failed sync point is unknown.
    failed: Option<io::Error>,
}

impl<'a, O: Output> SyncEvery<'a, O> {
    /// `out` is empty, at offset 0.
    pub(crate) fn new(out: O, cadence: &'a mut Cadence, cancel: &'a AtomicBool) -> Self {
        Self {
            out,
            cadence,
            cancel,
            pos: 0,
            high: 0,
            dirty: 0,
            since: Instant::now(),
            failed: None,
        }
    }

    /// Final sync and the inner output back.
    pub(crate) fn finish(mut self) -> io::Result<O> {
        self.failed()?;
        self.sync_point(false)?;
        Ok(self.out)
    }

    /// [`finish`](Self::finish), also handing back the cadence for the next output.
    pub(crate) fn finish_parts(mut self) -> io::Result<(O, &'a mut Cadence)> {
        self.failed()?;
        self.sync_point(false)?;
        Ok((self.out, self.cadence))
    }

    fn failed(&self) -> io::Result<()> {
        self.failed.as_ref().map_or(Ok(()), |e| Err(copy(e)))
    }

    fn latch(&mut self, e: io::Error) -> io::Error {
        let copy = copy(&e);
        self.failed = Some(e);
        copy
    }

    /// One inner write at `pos`, at most what N still allows; a sync point once N is reached.
    /// `buf` is not empty.
    fn put(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.failed()?;
        if self.dirty >= self.cadence.n() {
            self.sync_point(true)?;
        }
        let room = (self.cadence.n() - self.dirty).min(MAX_OFF - self.pos);
        if room == 0 {
            return Err(out_of_range());
        }
        let len = buf.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        let done = self.out.write(&buf[..len])?;
        self.pos += done as u64;
        self.high = self.high.max(self.pos);
        self.dirty += done as u64;
        if self.dirty >= self.cadence.n() {
            // The bytes are written: a failure is latched for the next call, not this one.
            let _ = self.sync_point(true);
        }
        Ok(done)
    }

    /// Zeros over `[pos, to)`; `pos` is the end of the data.
    fn zeros_to(&mut self, to: u64) -> io::Result<()> {
        while self.pos < to {
            check(self.cancel)?;
            let len = (to - self.pos).min(ZEROS.len() as u64) as usize;
            if self.put(&ZEROS[..len])? == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
        }
        Ok(())
    }

    /// To `target`; past the end, through zeros from the end.
    fn go(&mut self, target: u64) -> io::Result<()> {
        let at = target.min(self.high);
        // Not taken in the common UFS2 case (a partial last block, then a small gap).
        if at != self.pos {
            if let Err(e) = self.out.seek(SeekFrom::Start(at)) {
                return Err(self.latch(e));
            }
            self.pos = at;
        }
        self.zeros_to(target)
    }

    /// Sync; a retried sync is a failure. `full`: N was reached, so the interval measures the
    /// drive; finish's partial interval is mostly fsync latency and would read as a slow drive.
    fn sync_point(&mut self, full: bool) -> io::Result<()> {
        let synced = match self.out.sync(self.cancel) {
            Ok(false) => Ok(()),
            Ok(true) => Err(retried_sync()),
            Err(e) => Err(e),
        };
        if let Err(e) = synced {
            return Err(self.latch(e));
        }
        if full {
            self.cadence.record(self.dirty, self.since.elapsed());
        }
        self.dirty = 0;
        self.since = Instant::now();
        Ok(())
    }
}

impl<O: Output> Write for SyncEvery<'_, O> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.failed()?;
        if buf.is_empty() {
            return Ok(0);
        }
        self.put(buf)
    }

    /// Nothing to do: data is synced at sync points only.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<O: Output> Seek for SyncEvery<'_, O> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.failed()?;
        let target = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => self.high.checked_add_signed(d),
        }
        .filter(|&t| t <= MAX_OFF)
        .ok_or_else(out_of_range)?;
        self.go(target)?;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    /// An output over a Vec that counts calls and fails on request.
    #[derive(Default)]
    struct Fake {
        data: Vec<u8>,
        cur: usize,
        writes: usize,
        max_write: usize,
        seeks: usize,
        syncs: usize,
        /// `(k, Err(errno) | Ok(len))`: the k-th write (from 1) fails or accepts only `len`.
        fail_write: Option<(usize, Result<usize, i32>)>,
        /// `(k, errno)`: the k-th sync (from 1) fails for good.
        fail_sync: Option<(usize, i32)>,
        /// The k-th sync (from 1) only succeeds after a retry.
        retry_sync: Option<usize>,
    }

    impl Output for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            let mut len = buf.len();
            match self.fail_write {
                Some((k, Err(code))) if k == self.writes => return Err(os(code)),
                Some((k, Ok(short))) if k == self.writes => len = len.min(short),
                _ => {}
            }
            self.max_write = self.max_write.max(len);
            let end = self.cur + len;
            if self.data.len() < end {
                self.data.resize(end, 0);
            }
            self.data[self.cur..end].copy_from_slice(&buf[..len]);
            self.cur = end;
            Ok(len)
        }
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            let SeekFrom::Start(n) = pos else {
                panic!("SyncEvery seeks from the start")
            };
            self.seeks += 1;
            self.cur = n as usize;
            Ok(n)
        }
        fn sync(&mut self, _: &AtomicBool) -> io::Result<bool> {
            self.syncs += 1;
            if let Some((k, code)) = self.fail_sync
                && k == self.syncs
            {
                return Err(os(code));
            }
            Ok(self.retry_sync == Some(self.syncs))
        }
    }

    static NO: AtomicBool = AtomicBool::new(false);

    fn sony(c: i32) -> i32 {
        (0x8002_0000_u32 | c as u32) as i32
    }

    fn bytes(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    #[test]
    fn a_retried_sync_fails_the_job() {
        let retried = |e: io::Error| e.to_string().contains("temporary error while syncing");
        let (mut c, cancel) = (Cadence::fixed(64), AtomicBool::new(false));
        // At an N-driven sync point.
        let fake = Fake {
            retry_sync: Some(1),
            ..Fake::default()
        };
        let mut w = SyncEvery::new(fake, &mut c, &cancel);
        assert_eq!(w.write(&[1; 64]).unwrap(), 64);
        assert!(retried(w.write(&[2]).unwrap_err()));
        // At the final sync.
        let fake = Fake {
            retry_sync: Some(1),
            ..Fake::default()
        };
        let mut w = SyncEvery::new(fake, &mut c, &cancel);
        w.write_all(&[1; 10]).unwrap();
        assert!(retried(w.finish().err().unwrap()));
    }

    #[test]
    fn a_failed_sync_point_keeps_failing() {
        for retried in [false, true] {
            let fake = Fake {
                fail_sync: (!retried).then_some((1, libc::EIO)),
                retry_sync: retried.then_some(1),
                ..Fake::default()
            };
            let is = |e: io::Error| match retried {
                true => e.to_string().contains("temporary error while syncing"),
                false => e.raw_os_error() == Some(libc::EIO),
            };
            let (mut c, cancel) = (Cadence::fixed(64), AtomicBool::new(false));
            let mut w = SyncEvery::new(fake, &mut c, &cancel);
            // The bytes were accepted, so the write that hit the sync point reports them.
            assert_eq!(w.write(&[1; 100]).unwrap(), 64);
            for _ in 0..2 {
                assert!(is(w.write(&[1; 36]).unwrap_err()));
                assert!(is(w.seek(SeekFrom::Start(0)).unwrap_err()));
                assert!(is(w.seek(SeekFrom::Start(1000)).unwrap_err()));
            }
            assert_eq!((w.out.writes, w.out.seeks, w.out.data.len()), (1, 0, 64));
            assert!(is(w.finish().err().unwrap()));
        }
    }

    /// Runs `retry` over `errs` (then success) and returns the result, tries and slept ms.
    fn run(errs: &[i32], dir: bool, cancel: &AtomicBool) -> (io::Result<Synced>, usize, Vec<u64>) {
        let (mut tries, mut slept) = (0, Vec::new());
        let r = retry(
            || {
                tries += 1;
                errs.get(tries - 1).map_or(Ok(()), |&c| Err(os(c)))
            },
            dir,
            cancel,
            |d| slept.push(d.as_millis() as u64),
        );
        (r, tries, slept)
    }

    /// U4: which errnos a sync retries, fails at once, or (a directory only) lets pass.
    #[test]
    fn sync_errors_are_classified() {
        let err = |r: io::Result<Synced>| r.unwrap_err().raw_os_error();
        for code in [libc::EINVAL, libc::ENOTSUP, libc::EOPNOTSUPP] {
            assert_eq!(run(&[code], true, &NO).0.unwrap(), Synced::Unsupported);
            assert_eq!(err(run(&[code], false, &NO).0), Some(code));
        }
        for code in [libc::EIO, libc::ENOSPC, libc::EROFS] {
            let (r, tries, slept) = run(&[code], false, &NO);
            assert_eq!((err(r), tries, slept.len()), (Some(code), 1, 0));
        }
        let (r, tries, slept) = run(&[libc::EAGAIN, libc::EBUSY], false, &NO);
        assert_eq!((r.unwrap(), tries), (Synced::Retried, 3));
        assert!(slept.iter().all(|&ms| ms <= SLICE_MS));
        assert_eq!(run(&[], false, &NO).0.unwrap(), Synced::Clean);
        let (r, tries, _) = run(&[libc::ETIMEDOUT; 9], false, &NO);
        assert_eq!(
            (err(r), tries),
            (Some(libc::ETIMEDOUT), BACKOFF_MS.len() + 1)
        );
        // Sony's 0x8002xxxx errnos fold to the plain ones.
        assert_eq!(errno(&os(sony(libc::EAGAIN))), Some(libc::EAGAIN));
        assert!(is_transient(&os(sony(libc::ENXIO))));
        assert!(!is_transient(&os(sony(libc::EIO))));
        let (r, tries, _) = run(&[sony(libc::EINTR)], false, &NO);
        assert_eq!((r.unwrap(), tries), (Synced::Retried, 2));
    }

    #[test]
    fn a_lost_drive_is_named_only_when_its_mount_changed() {
        /// What stat/statfs say of `/mnt/usb0` today.
        type Usb0 = fn() -> io::Result<(u64, u64)>;
        let watched = |path: &str, dev, mount| Watched {
            path: PathBuf::from(path),
            dev,
            mount,
        };
        let all = vec![
            watched("/data", 1, true),
            watched("/mnt/usb0", 7, true),
            watched("/mnt/usb1/games", 9, false),
        ];
        let explain = |e: anyhow::Error, now: &dyn Fn(&Path) -> io::Result<(u64, u64)>| {
            format!("{:#}", explain_removal(e, &all, now))
        };
        // `/mnt/usb0` as `usb0` says, everything else healthy.
        let with = |usb0: Usb0| {
            move |m: &Path| match m.to_str() {
                Some("/mnt/usb0") => usb0(),
                Some("/mnt/usb1/games") => Ok((9, 50)),
                _ => Ok((1, 100)),
            }
        };
        let eio = || anyhow::Error::new(os(libc::EIO)).context("writing the image");
        let removed = "the drive at /mnt/usb0 was removed or stopped responding: writing the \
                       image: ";
        // Device changed, no blocks, stat failing as a lost drive does: removed, the original
        // error kept after it.
        let gone: [Usb0; 4] = [
            || Ok((8, 100)),
            || Ok((7, 0)),
            || Err(os(libc::ENOENT)),
            || Err(os(sony(libc::ENXIO))),
        ];
        for usb0 in gone {
            let got = explain(eio(), &with(usb0));
            assert!(got.starts_with(removed), "{got}");
            assert!(got.ends_with(&os(libc::EIO).to_string()), "{got}");
        }
        // Everything as it was, or a mount point we may not stat: unchanged, whatever the error.
        let fine: [Usb0; 2] = [|| Ok((7, 100)), || Err(os(libc::EACCES))];
        for usb0 in fine {
            for e in [eio(), anyhow::anyhow!("a reader's own text")] {
                let want = format!("{e:#}");
                assert_eq!(explain(e, &with(usb0)), want);
            }
        }
        // A folder (recorded where statfs failed) that is missing or unreadable now, on a
        // drive that is fine: not a removal. On another device: one.
        for err in [libc::ENOENT, libc::EACCES, libc::EIO] {
            let folder = |m: &Path| match m.to_str() {
                Some("/mnt/usb1/games") => Err(os(err)),
                _ => Ok(if m == Path::new("/mnt/usb0") {
                    (7, 100)
                } else {
                    (1, 100)
                }),
            };
            assert_eq!(explain(eio(), &folder), format!("{:#}", eio()));
        }
        let moved = |m: &Path| match m.to_str() {
            Some("/mnt/usb1/games") => Ok((10, 50)),
            Some("/mnt/usb0") => Ok((7, 100)),
            _ => Ok((1, 100)),
        };
        assert!(explain(eio(), &moved).starts_with("the drive at /mnt/usb1/games was removed"));
        // An error turned into text still gets the message when the drive is gone.
        let text = anyhow::anyhow!("image.exfat: Input/output error");
        assert_eq!(
            explain(text, &with(|| Ok((8, 1)))),
            "the drive at /mnt/usb0 was removed or stopped responding: image.exfat: \
             Input/output error"
        );
    }

    #[test]
    fn real_file_round_trip() {
        let path =
            std::env::temp_dir().join(format!("ps5-dump-forge-durable-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        assert_eq!(sync_retry(&file, false, &NO).unwrap(), Synced::Clean);
        let (mut c, cancel) = (Cadence::fixed(64), AtomicBool::new(false));
        let mut w = SyncEvery::new(file, &mut c, &cancel);
        w.write_all(&bytes(100, 1)).unwrap();
        w.seek(SeekFrom::Start(150)).unwrap();
        w.write_all(b"end").unwrap();
        drop(w.finish().unwrap());
        let got = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(got, [bytes(100, 1), vec![0; 50], b"end".to_vec()].concat());
    }
}
