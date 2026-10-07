//! Conversion core: scanning, preflight, jobs, verification, finalization.
//!
//! The public surface below is frozen: ps5-dump-forge and the Tauri app (`app/src-tauri`) build
//! against it. Internals can move freely.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod convert;
mod dlc;
mod extract;
mod finalize;
mod inspect;
mod jobs;
mod package;
mod portable;
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
    /// Backport files (`fakelib/*`, plus `ampr_emu.index` next to them): set for a game
    /// patched for older firmware.
    pub backport: Vec<String>,
    /// For a backport: the lowest firmware its executables allow (their highest PS5 SDK,
    /// `fakelib` left out), e.g. `4.50`. `None` without a backport or a readable param.
    pub backport_firmware: Option<String>,
    /// DLC embedded in the dump (in content-id folders, or merged with its metadata left).
    pub dlcs: Vec<Dlc>,
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
