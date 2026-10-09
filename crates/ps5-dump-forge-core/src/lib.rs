//! Conversion core: scanning, preflight, jobs, verification, finalization.
//!
//! The public surface below is frozen: ps5-dump-forge and the Tauri app (`app/src-tauri`) build
//! against it. Internals can move freely.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod backport;
mod convert;
// The PS5 (FreeBSD) destination probe; also built for macOS tests.
#[cfg(any(target_os = "freebsd", all(test, target_os = "macos")))]
mod dest;
mod dlc;
// Wired into the PS5 (FreeBSD) build only; tested everywhere.
#[cfg_attr(not(target_os = "freebsd"), allow(dead_code))]
mod durable;
mod extract;
mod finalize;
mod inspect;
mod jobs;
mod package;
mod portable;
mod prefetch;
mod preflight;
mod scan;
mod sdk;
mod verify;
#[cfg(windows)]
mod win;

pub use dlc::Dlc;
pub use extract::extraction_findings;
pub use finalize::rename_no_replace;
pub use scan::ScannedFolder;

/// What a conversion produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// A plain game folder (extraction).
    Folder,
    /// `.exfat` image.
    Exfat,
    /// `.ffpkg` (UFS2) image.
    Ffpkg,
    /// `.ffpfs`: an uncompressed PFS image.
    Ffpfs,
    /// `.ffpfsc`: a compressed PFS container holding one `.exfat`, `.ffpkg` or `.ffpfs` image
    /// ([`ConvertRequest::inner`]).
    Ffpfsc,
    /// `.pkg` debug FPKG.
    Pkg,
}

/// How hard a `.pkg` build compresses its image: the encoder's `kraken::Level`, named the same,
/// but `Fast` by default. The console reads every level the same way; a slower one makes a
/// slightly smaller package with every core busy for longer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KrakenLevel {
    /// A lazy parse. On a third-party package's blocks: 331 MB/s on 14 cores, 36.8% of the bytes.
    #[default]
    Fast,
    /// An optimal parse priced twice: ~6x slower than `Fast`, ~2.6% smaller (57 MB/s, 35.8%).
    Balanced,
    /// An optimal parse with deeper searches priced three times: ~9x slower than `Fast`
    /// (38 MB/s, 35.7%).
    Smallest,
}

/// One conversion: any readable source to one target format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertRequest {
    /// A game folder, `.exfat`, `.ffpkg`, `.ffpfs`, `.ffpfsc` or `.pkg`.
    pub source: PathBuf,
    pub format: Format,
    /// The final output path (file, or directory for `Folder`). Must not exist yet.
    pub output: PathBuf,
    /// Compression thread cap for `.pkg` and `.ffpfsc` builds (all cores when unset).
    #[serde(default)]
    pub compression_threads: Option<usize>,
    /// For `.ffpfsc`: the image inside, `Exfat` (when unset), `Ffpkg` or `Ffpfs`; any other
    /// format fails preflight. Ignored for every other target.
    #[serde(default)]
    pub inner: Option<Format>,
    /// Leave the backport out: the `fakelib`/`fakelib2` files that are not a known emulator
    /// ([`Inspection::backport`]). Preflight refuses when [`Inspection::backport_blocked`] would.
    #[serde(default)]
    pub remove_backport: bool,
    /// Re-read every byte of the output; unset, verification is fast: every structural check
    /// and a seeded sample of the content ([`VerifySummary`]); a `.pkg`'s builder self-check
    /// samples its blocks with the same seed.
    #[serde(default)]
    pub full_verify: bool,
    /// For `.pkg`: the compression level. Ignored for every other target.
    #[serde(default)]
    pub kraken_level: KrakenLevel,
    /// For `.ffpfsc`: the zlib level, 0 (store) through 9 (smallest), 6 when unset. Ignored for
    /// every other target.
    #[serde(default = "default_ffpfsc_level")]
    pub ffpfsc_level: u32,
}

/// The `.ffpfsc` zlib level a request without one gets.
pub const DEFAULT_FFPFSC_LEVEL: u32 = ps5_dump_forge_pfs::DEFAULT_LEVEL;

fn default_ffpfsc_level() -> u32 {
    DEFAULT_FFPFSC_LEVEL
}

pub type JobId = u64;

/// What a running job tells its owner. Serialized as `{"kind": "...", ...}`; the app emits
/// these as `job://progress`, `job://log` and `job://done`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    Progress {
        job: JobId,
        /// `scan`, `preflight`, `write`, `verify`, `finalize`, or an FPKG build stage.
        stage: String,
        /// Bytes over the whole job, every pass (write, verify, ...) included, so one bar
        /// fills once. `total` is an estimate that can grow; 0 before it is known.
        done: u64,
        total: u64,
    },
    Log {
        job: JobId,
        line: String,
    },
    Done {
        job: JobId,
        /// `Ok` carries the report; `Err` a message for the user (`"cancelled"` when cancelled).
        result: Result<JobReport, String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct JobReport {
    pub output: PathBuf,
    pub bytes: u64,
    pub files: u64,
    /// Verification lines, one per check.
    pub checks: Vec<String>,
    pub verify: VerifySummary,
}

/// How a job's output content was verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyMode {
    /// Small files whole, a seeded sample of the slices of larger ones.
    Fast,
    /// Every byte.
    Full,
}

/// What the content comparison read back. Structural checks (geometry, manifest, sizes, empty
/// dirs, the `.ffpfsc` container) run in both modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerifySummary {
    pub mode: VerifyMode,
    /// Bytes of file content read back and compared.
    pub checked_bytes: u64,
    /// Bytes of file content the output holds.
    pub total_bytes: u64,
    /// Ranges compared (whole files and 8 MiB slices); 0 in full mode.
    pub samples: u64,
    /// The sample plan's seed, below 2^53 so JSON readers keep it exact; 0 in full mode.
    pub seed: u64,
}

/// Runs jobs on worker threads: one heavy job at a time, the rest queue.
pub struct Jobs {
    inner: Arc<jobs::Inner>,
}

impl Jobs {
    /// `emit` receives every event of every job, from worker threads.
    pub fn new(emit: impl Fn(Event) + Send + Sync + 'static) -> Self {
        Self {
            inner: jobs::Inner::new(Box::new(emit)),
        }
    }

    pub fn start(&self, request: ConvertRequest) -> JobId {
        jobs::start(&self.inner, request)
    }

    pub fn cancel(&self, job: JobId) {
        self.inner.cancel(job);
    }

    /// Cancel everything and wait until every job has cleaned up (app close).
    pub fn cancel_all_and_wait(&self) {
        self.inner.cancel_all_and_wait();
    }
}

/// What `inspect` found in a source.
#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    /// `folder`, `exfat`, `ffpkg`, `ffpfs`, `ffpfsc` or `pkg`.
    pub kind: String,
    /// The source reader's one-line description.
    pub describe: String,
    pub title_id: Option<String>,
    pub content_id: Option<String>,
    /// From `param.json`: the title in its default language.
    pub title_name: Option<String>,
    /// From `param.json`: `contentVersion`, e.g. `01.000.000`.
    pub version: Option<String>,
    /// The firmware `param.json` declares (`requiredSystemSoftwareVersion`), e.g. `7.00`.
    pub firmware: Option<String>,
    /// The SDK it was built with (`sdkVersion`), e.g. `7.00`.
    pub sdk: Option<String>,
    /// Backport libraries: the `fakelib`/`fakelib2` files that are not a known emulator
    /// (system libraries from newer firmware).
    pub backport: Vec<String>,
    /// The emulators in `fakelib`/`fakelib2` (AMPR, DLC, PlayGo); removing the backport
    /// keeps them.
    pub emulators: Vec<Emulator>,
    /// With backport libraries: why removing them is refused (the executables' SDK was
    /// lowered, or can't be compared). `None` when removable or there is no backport.
    pub backport_blocked: Option<String>,
    /// With a `fakelib`/`fakelib2`: the lowest firmware the executables allow (their
    /// highest PS5 SDK, `fakelib` left out), e.g. `4.50`. `None` without one or a readable
    /// param.
    pub backport_firmware: Option<String>,
    /// DLC embedded in the dump (in content-id folders, or merged with its metadata left).
    pub dlcs: Vec<Dlc>,
    /// The PS5 Dump Forge version that wrote the image (`0.0.1-pre4`), from the maker's mark
    /// of an `.exfat`, `.ffpkg` or the image inside a `.ffpfsc`. `None` for anything else.
    pub forge_version: Option<String>,
    /// `sce_sys/icon0.png` as a `data:image/png;base64,` URL, when present and small.
    pub cover: Option<String>,
    /// `sce_sys/param.json`, parsed, when present.
    pub param_json: Option<serde_json::Value>,
    pub files: Vec<InspectFile>,
    pub empty_dirs: Vec<String>,
    pub total_bytes: u64,
    /// Format-specific facts (geometry, block size, ...), one per line.
    pub details: Vec<String>,
    /// Preflight findings for this source (errors and warnings), one per line.
    pub findings: Vec<String>,
}

/// An emulator in `fakelib`/`fakelib2`, named by the file it reads from the game root.
#[derive(Debug, Clone, Serialize)]
pub struct Emulator {
    pub path: String,
    /// `AMPR`, `DLC` or `PlayGo`.
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectFile {
    pub path: String,
    pub size: u64,
}

pub fn inspect(path: &Path) -> anyhow::Result<Inspection> {
    inspect::inspect(path)
}

/// The default output path for `source` in `format`, next to `dir` and named from the title id.
pub fn default_output(source: &Path, format: Format, dir: &Path) -> anyhow::Result<PathBuf> {
    inspect::default_output(source, format, dir)
}

/// The output path for `source` in `format` in `dir`, named from the game:
/// `[GAME_NAME]-[TITLE_ID]-[FIRMWARE]`, brackets included (the firmware `param.json`
/// declares), e.g. `[Astro Bot]-[PPSA01234]-[7.00].exfat`. Parts the source lacks are left out. Never a path
/// that exists or is in `taken` (outputs of jobs still running): `-2`, `-3`, ... instead.
/// `.ffpfs`/`.ffpfsc` names stay within the 63 bytes SMP mounts: the game name is cut first.
pub fn generated_output(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
) -> anyhow::Result<PathBuf> {
    inspect::generated_output(source, format, dir, taken)
}

/// Leftover `*.part` files/dirs from earlier jobs in `dir`. Never deleted automatically.
pub fn stale_parts(dir: &Path) -> Vec<PathBuf> {
    finalize::stale_parts(dir)
}

/// Where the app keeps its WebView data: always next to the app (user decision 2026-10-06: no
/// marker file, no OS per-user fallback).
#[derive(Debug, Clone, Serialize)]
pub struct DataDirs {
    /// The app dir: the folder holding the exe, or the folder holding `PS5 Dump Forge.app` on macOS.
    pub root: PathBuf,
}

impl DataDirs {
    /// `exe` is the running executable (`std::env::current_exe()`).
    pub fn resolve(exe: &Path) -> Self {
        Self {
            root: portable::app_dir(exe),
        }
    }

    /// WebView data on Windows and Linux (macOS keeps it in memory).
    pub fn webview(&self) -> PathBuf {
        self.root.join("data").join("webview")
    }
}

/// A fresh, empty folder for one unit test.
#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ps5-dump-forge-core-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether a test's hard link or symlink `made` it: false only where the volume cannot make
/// one at all (EOPNOTSUPP/ENOTSUP, FreeBSD's msdosfs); any other error fails the test.
#[cfg(test)]
pub(crate) fn supported(made: std::io::Result<()>, what: &str) -> bool {
    match made {
        Ok(()) => true,
        #[cfg(unix)]
        Err(e)
            if crate::durable::errno(&e)
                .is_some_and(|c| c == libc::EOPNOTSUPP || c == libc::ENOTSUP) =>
        {
            false
        }
        Err(e) => panic!("{what}: {e}"),
    }
}
