//! Conversion core: scanning, preflight, jobs, verification, finalization.
//!
//! The public surface below is frozen: ps5-dump-forge and the Tauri app (`app/src-tauri`) build
//! against it. Internals can move freely.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod archive;
mod backport;
mod convert;
mod delete;
// The PS5 (FreeBSD) destination probe; also built for macOS tests.
#[cfg(any(target_os = "freebsd", all(test, target_os = "macos")))]
mod dest;
mod dlc;
mod edit;
mod edit_folder;
mod edit_job;
// Wired into the PS5 (FreeBSD) build only; tested everywhere.
#[cfg_attr(not(target_os = "freebsd"), allow(dead_code))]
mod durable;
mod extract;
mod extract_paths;
mod finalize;
mod host_tree;
mod inspect;
mod jobs;
mod library;
mod library_roots;
mod lz4;
mod lz4_patch;
mod lz4_traces;
mod package;
mod portable;
mod prefetch;
mod preflight;
mod scan;
mod sdk;
mod verify;
#[cfg(windows)]
mod win;
mod zip;

pub use delete::DeleteError;
pub use dlc::Dlc;
pub use extract::extraction_findings;
pub use finalize::rename_no_replace;
pub use host_tree::{HostFile, HostKind, HostTree, host_tree};
pub use library::{LibraryEntry, LibraryScan};
pub use ps5_dump_forge_archive::{
    ARCHIVE_PASSWORD_REQUIRED, ARCHIVE_PASSWORD_WRONG, ArchiveFormat,
};
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
    /// An LZ4 packed folder: a plain folder (no extension) whose assets sit in AMPR LZ4 packs
    /// (`ampr_assets.index`, `ampr_assets-000.pak`, ...) that the injected
    /// `fakelib/libSceAmpr.sprx` reads. Only for a title that imports `libSceAmpr`. The same
    /// as `Folder` with [`Lz4Mode::Pack`] (kept for older requests).
    Lz4,
}

/// What a job does with LZ4 asset packs besides its target ([`ConvertRequest::lz4`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lz4Mode {
    /// Install the tracing runtime, so the game records what it reads (the next `Lz4` job
    /// packs those files). A folder, `.exfat` or `.ffpkg` only: `/app0` must be writable.
    Trace,
    /// Decode the packs back to plain files.
    Unpack,
    /// Undo a trace patch: install Forge's release runtime (whatever runtime is there), drop
    /// the journal and logs and write a fresh `ampr_emu.index`. Refused for a packed source
    /// (unpack it first) and with the `Lz4` target (which always installs the release one).
    Unpatch,
    /// Pack the assets into LZ4 packs, into the request's `format`: a folder (as the `Lz4`
    /// target), `.exfat`, `.ffpkg`, `.ffpfs` or `.ffpfsc` (not `.pkg`). An image gets its packs
    /// straight from the source in two passes (measure, then write), with no staging folder.
    Pack,
}

/// How hard a `.pkg` build compresses its image: the encoder's `kraken::Level`, named the same,
/// but `Fast` by default. The console reads every compressed level the same way; a slower one
/// makes a slightly smaller package with every core busy for longer.
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
    /// No compression: every block stored raw in the same layout. The fallback for a console
    /// that refuses to start a compressed package (compression is proven on FW 5.10 only).
    Stored,
}

/// An archive's password. `Debug` prints `<redacted>` and [`ConvertRequest`] never serializes
/// it, so it reaches no log, event, job list or report.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ArchivePassword(String);

impl ArchivePassword {
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ArchivePassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// One conversion: any readable source to one target format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertRequest {
    /// A game folder, `.exfat`, `.ffpkg`, `.ffpfs`, `.ffpfsc` or `.pkg`, or a `.zip`, `.7z` or
    /// `.rar` holding one game folder (its first part, for a multi-part set), read straight
    /// from the archive with nothing unpacked to disk.
    pub source: PathBuf,
    pub format: Format,
    /// The final output path (file, or directory for `Folder`). Must not exist yet. Ignored
    /// with [`ConvertRequest::lz4_in_place`].
    pub output: PathBuf,
    /// Compression thread cap for `.pkg` and `.ffpfsc` builds (all cores when unset).
    #[serde(default)]
    pub compression_threads: Option<usize>,
    /// For `.ffpfsc`: the image inside, `Ffpkg` (when unset), `Exfat` or `Ffpfs`; any other
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
    /// Trace or unpack LZ4 packs; unset, packs travel as plain files (Forge's own trace
    /// runtime in an unpacked dump is swapped for the release one).
    #[serde(default)]
    pub lz4: Option<Lz4Mode>,
    /// When packing (`Lz4`, or [`Lz4Mode::Pack`]): a TOML packing profile (else this dump's traces, else a built-in guess).
    /// Any other target fails preflight.
    #[serde(default)]
    pub lz4_profile: Option<PathBuf>,
    /// When packing: traces copied from the console: the `*-amprtrace.zip` [`lz4_traces`] writes
    /// (STORED entries, each checked against its CRC-32), a folder holding `ampr_commands.bin`
    /// and `ampr_emu.index`, or that journal with its index beside it. Picks the files to pack
    /// ahead of the dump's own traces. Any other target, or a profile too, fails preflight.
    #[serde(default)]
    pub lz4_traces: Option<PathBuf>,
    /// For [`Lz4Mode::Trace`] into an `.exfat`/`.ffpkg`: free space for the trace, 64..=1024 MiB in
    /// 64 MiB steps, 256 when unset. Ignored otherwise.
    #[serde(default = "default_lz4_trace_space_mib")]
    pub lz4_trace_space_mib: u32,
    /// The output replaces the source: a patched copy is written beside the source image
    /// (its `.part`), verified as usual, then renamed over the source once the source is seen
    /// unchanged; on any failure the source stays as it was. Only for an `.exfat` or `.ffpkg`
    /// source, with `format` the source's own and [`Lz4Mode::Trace`] or [`Lz4Mode::Unpatch`];
    /// anything else is a preflight finding, as is a packed source. Needs about the image's
    /// size of free space beside it until the rename. `output` is ignored. With
    /// [`lz4_patch`], the explicit exceptions to "never touch the source".
    #[serde(default)]
    pub lz4_in_place: bool,
    /// File changes applied in place, in order (an edit job when not empty).
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// With [`Self::edits`] on an image: rebuild it instead of editing it in place. A new image
    /// of the source's own format is written from the source plus the edits beside it (its
    /// `.part`), verified, then renamed over the source once the source is seen unchanged, as
    /// [`Self::lz4_in_place`] does; on any failure the source stays as it was. `format` and
    /// `output` are ignored. Ignored for a folder, which is always changed directly.
    #[serde(default)]
    pub edit_rebuild: bool,
    /// Game paths (`/`-separated, relative) of files or folders to copy out of the source into
    /// `output`, an existing folder, each under its own last component; nothing there is
    /// replaced (an extract job when not empty; `format` must be `Folder`).
    #[serde(default)]
    pub extract: Vec<String>,
    /// For an archive source that needs one. Never serialized.
    #[serde(default, skip_serializing)]
    pub archive_password: Option<ArchivePassword>,
}

/// One change of an in-place edit ([`ConvertRequest::edits`]), applied in order. Game paths are
/// relative, `/`-separated, with no empty, `.` or `..` component and no `\`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Edit {
    /// Copy `from` (a file on this computer, or a folder and everything in it) to game path `to`,
    /// replacing a file there; a folder onto an existing folder merges (same-named files replaced).
    Add { from: PathBuf, to: String },
    /// Delete a file, or a folder with everything in it.
    Delete { path: String },
    /// Move or rename a file or folder; `to` must not exist.
    Rename { from: String, to: String },
    /// A new empty folder (parents created too).
    Mkdir { path: String },
}

/// Which way an edit request ([`ConvertRequest::edits`]) can be applied, as [`Jobs::edit_check`]
/// finds it before Apply: the scan and preflight of both modes, nothing written. A check that
/// passes means that mode's job passes its preflight on the same files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditCheck {
    /// Why it can't be applied in place (empty = it can): no room, format, unsupported op.
    pub in_place: Vec<String>,
    /// Why it can't be rebuilt (empty = it can; a folder is never rebuilt).
    pub rebuild: Vec<String>,
    /// Bytes the changes need inside the image, as its editor plans them (file data, new and
    /// grown folders, indirect blocks), and what it has free: `.exfat` and `.ffpkg` once the
    /// edits apply to the game.
    pub needed_inside: Option<u64>,
    pub free_inside: Option<u64>,
    /// Free bytes a rebuild needs beside the source, and what the folder has.
    pub rebuild_needs: Option<u64>,
    pub free_beside: Option<u64>,
}

/// The trace space a request without one gets, in MiB.
/// ponytail: measured once (Stellar Blade): ~2 MB of journal per 5 minutes of heavy loading, about
/// 25 MB/hour, so 256 MiB is ~10 hours; the cap is 1024 MiB. Recalibrate on more titles.
pub const DEFAULT_LZ4_TRACE_SPACE_MIB: u32 = 256;

fn default_lz4_trace_space_mib() -> u32 {
    DEFAULT_LZ4_TRACE_SPACE_MIB
}

/// The `.ffpfsc` zlib level a request without one gets.
pub const DEFAULT_FFPFSC_LEVEL: u32 = ps5_dump_forge_pfs::DEFAULT_LEVEL;

fn default_ffpfsc_level() -> u32 {
    DEFAULT_FFPFSC_LEVEL
}

pub type JobId = u64;

/// What a running job tells its owner. Serialized as `{"kind": "...", ...}`; the app emits
/// these as `job://stages`, `job://progress`, `job://log` and `job://done`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// Once, first: every `Progress` stage the job will report, in the order it reports them.
    /// A job that fails stops part way through the list.
    Stages {
        job: JobId,
        stages: Vec<String>,
    },
    Progress {
        job: JobId,
        /// `extract` (an archive source, first), `scan`, `preflight`, `measure` (LZ4 into an
        /// image), `write`, `pack` (LZ4 folder),
        /// `verify`, `finalize`, or an FPKG build stage.
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
        /// `Ok` carries the report; `Err` a message for the user (`"cancelled"` when cancelled,
        /// or an edit's `"cancelled; "` + what stays changed).
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

    /// Deletes `path`, a file or a folder (links inside are removed, not followed), for good.
    /// Refused for a link, a special file, a drive or volume root, any of `protected` or a
    /// folder holding one, and anything at, in or around what an unfinished job reads or
    /// writes; no job is admitted while it runs. The third explicit exception to "never touch
    /// the source", with the LZ4 patches.
    pub fn delete_path(&self, path: &Path, protected: &[PathBuf]) -> Result<(), DeleteError> {
        self.inner.delete_path(path, protected)
    }

    /// Whether an unfinished job reads or writes `path`, or anything inside or around it
    /// (its source, output, `.part`s, profile, traces; an edit's undo journal and added
    /// files), compared by identity as [`Jobs::delete_path`] compares them. A snapshot: a
    /// job may start right after.
    pub fn uses(&self, path: &Path) -> bool {
        self.inner.uses(path)
    }

    /// Which way `request`'s edits can be applied, in place or by a rebuild ([`EditCheck`]),
    /// reading the image and the added files only. A journal an interrupted edit left beside
    /// the image is settled first, as every edit does, unless a job is using the image; then
    /// both modes say so.
    pub fn edit_check(&self, request: &ConvertRequest) -> EditCheck {
        self.inner.edit_check(request)
    }

    /// Cancel everything and wait until every job has cleaned up (app close).
    pub fn cancel_all_and_wait(&self) {
        self.inner.cancel_all_and_wait();
    }
}

/// What `inspect` found in a source.
#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    /// `folder`, `exfat`, `ffpkg`, `ffpfs`, `ffpfsc` or `pkg`; `zip`, `7z` or `rar` for an archive.
    pub kind: String,
    /// The source reader's one-line description.
    pub describe: String,
    pub title_id: Option<String>,
    pub content_id: Option<String>,
    /// No `eboot.bin` at the root but a `param.json` with a title id: a DLC, an update or
    /// another package. Not a finding; it converts only to fpkg or a folder.
    pub additional_content: bool,
    /// From `param.json`: the title in its default language.
    pub title_name: Option<String>,
    /// From `param.json`: every `localizedParameters` title by language tag (`en-US`, ...).
    pub title_names: std::collections::BTreeMap<String, String>,
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
    /// AMPR / LZ4 pack facts; `None` when the title does not import `libSceAmpr` and holds
    /// no LZ4 artifact (packs, journal, path index, Forge's runtime).
    pub lz4: Option<Lz4Facts>,
    /// An archive source: the archive's own facts (`files` and `total_bytes` are the game's,
    /// unpacked). `None` for anything else.
    pub archive: Option<ArchiveFacts>,
    /// An image whose last in-place edit was interrupted: its undo journal
    /// (`<image>.forge-undo`) is beside it, and the next edit restores the image first.
    pub interrupted_edit: bool,
}

/// What an archive source is.
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveFacts {
    pub format: ArchiveFormat,
    /// Every part on disk.
    pub archive_bytes: u64,
    /// The parts' file names, the first one first.
    pub volumes: Vec<String>,
    pub solid: bool,
    pub encrypted: bool,
    /// The game folder inside the archive (`Game/`); empty when the game is at its root.
    pub root: String,
}

/// What `inspect` found about LZ4 asset packs in a source. The containing-format facts of
/// [`Inspection`] are unchanged; these describe the logical game inside it.
#[derive(Debug, Clone, Serialize)]
pub struct Lz4Facts {
    /// `eboot.bin` imports `libSceAmpr`.
    pub imports_ampr: bool,
    /// From the manifest, when the source holds packs and the manifest is sound.
    pub packed: Option<Lz4Packed>,
    /// A root `ampr_assets.index` starts with the pack magic but does not parse: the reason
    /// (also a finding). `packed` is `None` then.
    pub manifest_error: Option<String>,
    /// The runtime at `fakelib/libSceAmpr.sprx`: `forge_release`, `forge_trace`, `other`
    /// or `none`.
    pub runtime: String,
    /// The ampr_emu version of the runtimes Forge ships (and installs on unpatch).
    pub shipped_runtime_version: &'static str,
    /// The ampr_emu version the game's own runtime names itself with, when it does.
    pub runtime_version: Option<String>,
    /// What a plain conversion does to the runtime (by user decision): `install` (none there),
    /// `replace` (Forge's trace runtime, an older ampr_emu or one naming no version: the
    /// bundled release runtime goes in) or `keep` (Forge's release runtime, or ampr_emu at
    /// [`Lz4Facts::shipped_runtime_version`] or newer). `None` for a non-AMPR title.
    pub runtime_action: Option<&'static str>,
    /// The same for the LZ4 tab's Pack (an unpacked source) or Unpack (a packed one): Pack
    /// also replaces a not-older runtime that can't read packs.
    pub pack_runtime_action: Option<&'static str>,
    /// `ampr_emu.index` against the files a plain conversion writes (the same comparison the
    /// job makes, the runtime as it installs, replaces or keeps it): `ok`, `stale`, `damaged`
    /// or `missing` (the last three written anew by any conversion). `None` when no conversion
    /// touches it: a packed source (kept byte for byte), a non-AMPR title or an archive's
    /// unread index.
    pub index: Option<&'static str>,
    /// Size of the trace journal (`ampr_commands.bin`), when present.
    pub journal_bytes: Option<u64>,
    /// With the journal and its `ampr_emu.index` both present: the name [`lz4_traces`] gives
    /// their zip (`[GAME_TITLE]-[TITLE_ID]-amprtrace.zip`).
    pub traces_zip: Option<String>,
}

/// Counts from an LZ4 manifest.
#[derive(Debug, Clone, Serialize)]
pub struct Lz4Packed {
    /// Files the manifest lists (packed and loose).
    pub files: u64,
    /// Of those, the files stored in volumes. `None` when the manifest was too large to
    /// parse (only its header counts are known; a finding says so).
    pub packed_files: Option<u64>,
    pub volumes: u64,
    /// Whole percent: the stored bytes of the packed files over their logical (unpacked)
    /// bytes. 0 with nothing packed; `None` like `packed_files`.
    pub stored_percent: Option<u64>,
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
    inspect::inspect(path, None)
}

/// [`inspect`] with an archive's password (ignored for any other source). An archive is read
/// from its listing and the few files inspecting needs, in memory; without the password it
/// needs the error is exactly [`ARCHIVE_PASSWORD_REQUIRED`], with a wrong one
/// [`ARCHIVE_PASSWORD_WRONG`].
pub fn inspect_with(path: &Path, password: Option<&str>) -> anyhow::Result<Inspection> {
    inspect::inspect(path, password)
}

/// What [`lz4_patch`] or [`lz4_unpatch`] did to a game folder.
#[derive(Debug, Clone, Serialize)]
pub struct Lz4Patch {
    /// Files the new `ampr_emu.index` lists (the runtime included).
    pub indexed: usize,
    /// Stale trace files deleted from the folder's root (the journal, the logs).
    pub removed: Vec<String>,
    /// The runtime's known-issue line, for the report.
    pub warning: String,
}

/// Patches a game folder in place for recording LZ4 traces: the embedded trace runtime
/// replaces whatever `fakelib/libSceAmpr.sprx` is there, the last session's journal and logs
/// are deleted and `ampr_emu.index` is rebuilt for the folder's files. An exception to "never
/// touch the source", on request (with [`ConvertRequest::lz4_in_place`] for images and
/// [`lz4_unpatch`]). Refuses an image or package (an `.exfat`/`.ffpkg` is patched by a
/// conversion in place), a packed folder and a title whose `eboot.bin` does not import
/// `libSceAmpr`. Each file is written under a temporary name, synced and renamed over its
/// target; the folders are synced after. On the PS5 every file it writes is 0777.
pub fn lz4_patch(folder: &Path) -> anyhow::Result<Lz4Patch> {
    lz4_patch::patch(folder, lz4_patch::Runtime::Trace)
}

/// Undoes [`lz4_patch`] in place, with the same checks and writes: Forge's release runtime
/// (0.4.2.1) replaces whatever `fakelib/libSceAmpr.sprx` is there (no backup of an earlier one
/// exists), the journal and logs are deleted and `ampr_emu.index` is rebuilt.
pub fn lz4_unpatch(folder: &Path) -> anyhow::Result<Lz4Patch> {
    lz4_patch::patch(folder, lz4_patch::Runtime::Release)
}

/// PS5 only (FreeBSD; any other OS returns an error for non-empty `roots`). Lists the game dumps
/// under `roots` (folders and `.exfat`, `.ffpkg`, `.ffpfs`, `.ffpfsc`,
/// `.pkg` images, by extension, any case) on local disk filesystems: breadth first, at most 6
/// folders deep, links and network mounts never entered, a game folder never entered, bounded
/// by fixed limits (`truncated` when one cut the walk short) and about 15 seconds (checked
/// between steps, so a slow read can overrun it). Reads only `param.json` and the listings;
/// an image is opened with its reader, which is slower than a folder. Sorted by title. One
/// scan runs at a time; a second call waits. Empty `roots` is an empty result.
pub fn scan_library(roots: &[PathBuf]) -> anyhow::Result<LibraryScan> {
    library::scan_library(roots)
}

/// Like [`scan_library`] for one folder the user chose: any filesystem (a network share, mounts
/// nested inside it), since the choice is explicit. Errors (nothing listed) when the folder is
/// the filesystem root (Windows: the system drive root), `~/Library` or `~/AppData`, or one of the OS's own folders or
/// inside one, is under ShadowMountPlus's mounts, or cannot be read.
/// Failures below the folder are warnings.
pub fn scan_library_folder(root: &Path) -> anyhow::Result<LibraryScan> {
    library::scan_library_folder(root)
}

/// The cover of a Library entry (a supported image or a plain game folder, on any filesystem:
/// no locality check) as a PNG data URL. `None` when the icon is absent, empty, over 512 KiB (checked before and
/// after reading) or not a PNG; an error when the source is a link, not eligible, or cannot
/// be opened.
pub fn library_cover(path: &Path) -> anyhow::Result<Option<String>> {
    library::library_cover(path)
}

pub use lz4_traces::Lz4Traces;

/// The LZ4 trace files a traced game writes at its root: the journal, and the path index
/// whose file ids it names. Pack takes both from one folder.
pub const LZ4_TRACE_FILES: [&str; 2] = [ps5_dump_forge_lz4::JOURNAL, ps5_dump_forge_lz4::INDEX];

/// Both [`LZ4_TRACE_FILES`] at the root of `source` (a folder or any image this app reads), to
/// copy off the console, with the name their download takes:
/// `[GAME_TITLE]-[TITLE_ID]-amprtrace.zip` from `param.json`, the stem exactly as
/// [`generated_output`] builds it (the title made file-safe and cut first to keep the stem within
/// 63 bytes; a part it lacks left out; `amprtrace.zip` without either). The inner `Err`
/// says which file is missing; the outer one is a source that can't be read. A folder's files
/// are opened directly; an image is read through its own reader. Only metadata is read here.
pub fn lz4_traces(source: &Path) -> anyhow::Result<Result<Lz4Traces, String>> {
    lz4_traces::open(source)
}

/// The default output path for `source` in `format`: in `dir`, named from the title id. An
/// empty `dir` is the folder holding the source (absolute, never the working directory).
pub fn default_output(source: &Path, format: Format, dir: &Path) -> anyhow::Result<PathBuf> {
    inspect::default_output(source, format, dir, None)
}

/// [`default_output`] with an archive's password (ignored for any other source): without the
/// one an archive with encrypted headers needs, the name is the archive's own.
pub fn default_output_with(
    source: &Path,
    format: Format,
    dir: &Path,
    password: Option<&str>,
) -> anyhow::Result<PathBuf> {
    inspect::default_output(source, format, dir, password)
}

/// The output path for `source` in `format` in `dir` (empty: the folder holding the source),
/// named from the game: `[GAME_NAME]-[TITLE_ID]`, brackets included, e.g.
/// `[Astro Bot]-[PPSA01234].ffpkg`. Parts the source lacks are left out. Never a path that
/// exists or is in `taken` (outputs of jobs still running): `-2`, `-3`, ... instead. The name
/// without its extension stays within what ShadowMountPlus mounts (63 bytes, 58 for a
/// `.ffpfsc`, and 63 for a folder or `.pkg` too; see `preflight::stem_limit`): the game name is
/// cut first.
pub fn generated_output(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
) -> anyhow::Result<PathBuf> {
    inspect::generated_output(source, format, dir, taken, None)
}

/// [`generated_output`] with an archive's password (ignored for any other source). An archive
/// is named from its game folder's `param.json`, the only file read; without the password it
/// needs the error is exactly [`ARCHIVE_PASSWORD_REQUIRED`], with a wrong one
/// [`ARCHIVE_PASSWORD_WRONG`].
pub fn generated_output_with(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
    password: Option<&str>,
) -> anyhow::Result<PathBuf> {
    inspect::generated_output(source, format, dir, taken, password)
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

    /// WebView data on Windows and Linux (macOS: WebKit's own store in ~/Library/WebKit).
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
/// Only the macOS/FreeBSD rename tests in `finalize.rs` use it.
#[cfg(all(test, any(target_os = "macos", target_os = "freebsd")))]
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
