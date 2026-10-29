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
pub mod ftp;
mod host_tree;
mod inspect;
mod install;
mod jobs;
mod zstd_pack;
pub use awake::{Awake, awake};
mod awake;
mod library;
mod library_roots;
mod listing;
mod lz4;
pub mod net;
mod nfs;
mod package;
mod portable;
mod prefetch;
mod preflight;
pub mod remote;
mod scan;
mod sdk;
pub mod smb;
mod temp_file;
mod transfer;
mod verify;
#[cfg(windows)]
mod win;

pub use delete::DeleteError;
pub use dlc::Dlc;
pub use extract::extraction_findings;
pub use finalize::rename_no_replace;
pub use host_tree::{HostFile, HostKind, HostTree, host_tree};
pub use library::{LibraryEntry, LibraryScan, PackageContent, PackageEntry, PackageScan};
pub use listing::{ListEntry, Listing, list_dir, list_explorer};
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
    /// Decode the packs back to plain files.
    Unpack,
    /// Pack the assets into LZ4 packs, into the request's `format`: a folder (as the `Lz4`
    /// target), `.exfat`, `.ffpkg`, `.ffpfs` or `.ffpfsc` (not `.pkg`). An image gets its packs
    /// straight from the source in two passes (measure, then write), with no staging folder.
    Pack,
}

/// The runtime an LZ4 Pack installs ([`ConvertRequest::lz4_runtime`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lz4Runtime {
    /// ampr-emu-ng (`runtime::ZSTD`): reads LZ4 and zstd packs.
    #[default]
    Ng,
    /// The bundled upstream ampr_emu release build (`runtime::RELEASE`).
    AmprEmu,
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
    /// The final output path (file, or directory for `Folder`). Must not exist yet.
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
    /// Pack or unpack LZ4 packs; unset, packs travel as plain files (Forge's old trace
    /// runtime in an unpacked dump is swapped for the release one).
    #[serde(default)]
    pub lz4: Option<Lz4Mode>,
    /// When packing LZ4: the C liblz4 level, 0 (fast, `LZ4_compress_default`) or an HC level
    /// 3 through 12 (9 medium, 12 max); 0 when unset. Any other value fails preflight.
    #[serde(default)]
    pub lz4_level: u8,
    /// When packing LZ4: the runtime the packs get, ampr-emu-ng when unset (kept where the
    /// game already has it), or the bundled ampr_emu release build. Ignored otherwise.
    #[serde(default)]
    pub lz4_runtime: Lz4Runtime,
    /// Pack the game into zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, zstd, 256 KiB chunks;
    /// `docs/PAK.md`) and install the zstd runtime. Into a `Folder` or an `Exfat`, `Ffpkg`,
    /// `Ffpfs` or `Ffpfsc` image (straight in, after a measure pass; not `Pkg`), for a title
    /// that imports libSceAmpr, and with no LZ4 option; anything else fails preflight.
    #[serde(default)]
    pub zstd_pack: bool,
    /// With [`Self::zstd_pack`]: the zstd level, 1 through 22, [`DEFAULT_ZSTD_LEVEL`] when
    /// unset. Ignored otherwise.
    #[serde(default = "default_zstd_level")]
    pub zstd_level: u8,
    /// Decode a zstd-packed source's packs into its logical files and write them to `format`
    /// (any target): the bundled release runtime replaces the zstd one and a fresh
    /// `ampr_emu.index` is written. A source without zstd packs, [`Self::zstd_pack`] or any
    /// LZ4 option fails preflight.
    #[serde(default)]
    pub zstd_unpack: bool,
    /// File changes applied in place, in order (an edit job when not empty).
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// With [`Self::edits`] on an image: rebuild it instead of editing it in place. A new image
    /// of the source's own format is written from the source plus the edits beside it (its
    /// `.part`), verified, then renamed over the source once the source is seen unchanged; on
    /// any failure the source stays as it was. `format` and
    /// `output` are ignored. Ignored for a folder, which is always changed directly.
    #[serde(default)]
    pub edit_rebuild: bool,
    /// Game paths (`/`-separated, relative) of files or folders to copy out of the source into
    /// `output`, an existing folder, each under its own last component; nothing there is
    /// replaced unless [`Self::replace`] (an extract job when not empty; `format` must be
    /// `Folder`).
    #[serde(default)]
    pub extract: Vec<String>,
    /// Files or folders on this device, a peer (`flink://`) or a NAS (`smb://`) to copy into
    /// `output`, an existing folder on this device or a peer, each under its own name; nothing
    /// there is replaced unless [`Self::replace`] (a transfer job when not empty; `source` and
    /// `format` are ignored). A transfer runs beside the one heavy job, one transfer at a time.
    #[serde(default)]
    pub transfer: Vec<PathBuf>,
    /// For an extract or a transfer (the user confirmed it, [`Jobs::existing_targets`]): what
    /// already exists at a target is replaced, a file onto a file, a folder onto a folder
    /// merged (same-named files replaced, everything else there kept). A kind mismatch at any
    /// level, and a replaced file [`Jobs::delete_path`] would refuse, fail preflight (on a peer
    /// or a NAS, what this device's jobs use; a peer refuses the rest itself). Ignored for
    /// every other job.
    #[serde(default)]
    pub replace: bool,
    /// A `.pkg` on this device or another one (`flink://`, `smb://`, `ftp://`, `nfs://`) to hand
    /// to the PS5's installer, which fetches it from a 127.0.0.1 server the job runs (an install
    /// job when set; `source`, `output` and `format` are ignored, any other job option fails
    /// preflight, and off the PS5 it fails preflight). It runs in the transfer slot.
    #[serde(default)]
    pub install: Option<PathBuf>,
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

/// The `.ffpfsc` zlib level a request without one gets.
pub const DEFAULT_FFPFSC_LEVEL: u32 = ps5_dump_forge_pfs::DEFAULT_LEVEL;

fn default_ffpfsc_level() -> u32 {
    DEFAULT_FFPFSC_LEVEL
}

/// The zstd level a request without one gets.
pub const DEFAULT_ZSTD_LEVEL: u8 = 5;

fn default_zstd_level() -> u8 {
    DEFAULT_ZSTD_LEVEL
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
        /// or `"cancelled; "` + what stays changed or what else failed, a restore say).
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

/// Runs jobs on worker threads: one heavy job and one transfer at a time, the rest queue.
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
    /// folder holding one, an interrupted edit's undo journal while its image is there, and
    /// anything at, in or around what an unfinished job reads or writes, or what a peer is
    /// writing through [`Jobs::set_external_busy`]; on the PS5 also
    /// anything at or in ShadowMount+'s mount folder, an image it has mounted and a folder
    /// holding one (fails closed). No job is admitted while it runs. The third explicit exception to "never touch
    /// the source", with the LZ4 patches. A paired peer's `flink://` path, unless one of this
    /// device's unfinished jobs uses it, is deleted by that peer under its own rules
    /// ([`remote::Remote::delete_path`]); a NAS share's is refused.
    pub fn delete_path(&self, path: &Path, protected: &[PathBuf]) -> Result<(), DeleteError> {
        self.inner.delete_path(path, protected)
    }

    /// Moves or renames `from` to `to` on the same volume: `to` is a new name in an existing
    /// folder, never replaced unless `replace`, and not inside `from`. `from` is refused as
    /// [`Jobs::delete_path`] refuses it (a link, a special file, a root, any of `protected` or
    /// a folder holding one, anything an unfinished job uses), and so is a `to` at, in or
    /// above what an unfinished job uses or a peer is writing, or an image with an interrupted edit's undo journal
    /// beside it. With `replace` (the user confirmed it) an existing `to` is taken over: a file
    /// by a file in one rename (the old one refused as `delete_path` refuses it), a folder by a
    /// folder merged (each entry moved, same-named files replaced, folders both hold merged,
    /// everything else there kept, the emptied source folders removed); a kind mismatch at any
    /// level refuses. Every check runs before the first rename; a failure part way says what
    /// moved. Both folders are synced after; no job is admitted while it runs. Two `flink://`
    /// paths on one peer are moved by that peer, as [`Jobs::delete_path`] deletes one there.
    pub fn move_path(
        &self,
        from: &Path,
        to: &Path,
        replace: bool,
        protected: &[PathBuf],
    ) -> Result<(), DeleteError> {
        self.inner.move_path(from, to, replace, protected)
    }

    /// Whether an unfinished job reads or writes `path`, or anything inside or around it
    /// (its source, output, `.part`s; an edit's undo journal and added
    /// files), compared by identity as [`Jobs::delete_path`] compares them. A snapshot: a
    /// job may start right after.
    pub fn uses(&self, path: &Path) -> bool {
        self.inner.uses(path)
    }

    /// [`Jobs::uses`] as a hook of its own (Connect refuses a peer's writes into what this
    /// device's unfinished jobs use).
    pub fn uses_hook(&self) -> Arc<dyn Fn(&Path) -> bool + Send + Sync> {
        let inner = self.inner.clone();
        Arc::new(move |path| inner.uses(path))
    }

    /// Why a file must not be put in place of `path` (a peer's replacing rename here): as a
    /// job's replace is refused, so as [`Jobs::delete_path`] refuses it (a link, a folder, a
    /// root, any of [`Jobs::set_protected`]'s paths, an interrupted edit's undo journal or its
    /// image, anything at, in or around an unfinished job's paths or what a peer is writing
    /// ([`Jobs::set_external_busy`]); on the PS5 ShadowMount+'s mount folder and the images it
    /// has mounted). Nothing at `path` passes. A snapshot: a job may start right after.
    pub fn replace_check(&self, path: &Path) -> Result<(), DeleteError> {
        self.inner.replace_check(path)
    }

    /// Sets what says another device is writing a path on this device (Connect's open write
    /// handles): a job using such a path is refused when it starts, and Delete and Move
    /// refuse it too. Like job admission, the external check is a snapshot.
    pub fn set_external_busy(&self, busy: Arc<dyn Fn(&Path) -> bool + Send + Sync>) {
        self.inner.set_external_busy(busy);
    }

    /// Sets what replacing never touches (an extract's or transfer's
    /// [`ConvertRequest::replace`]), as `protected` is for [`Jobs::delete_path`]: asked once
    /// per job, at its preflight. Unset, nothing beyond delete's other refusals.
    pub fn set_protected(&self, protected: Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>) {
        self.inner.set_protected(protected);
    }

    /// The targets of an extract or transfer `request` that exist already (each selection's,
    /// at the top level; on another device too), for the UI to ask before it sends
    /// [`ConvertRequest::replace`]. Reads only; an error for any other request.
    pub fn existing_targets(&self, request: &ConvertRequest) -> anyhow::Result<Vec<PathBuf>> {
        extract_paths::existing_targets(request)
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
    /// zstd packed: a root `ampr_zstd.index` that starts with `AMPRZPK1`.
    pub zstd_packed: bool,
    /// An archive source: the archive's own facts (`files` and `total_bytes` are the game's,
    /// unpacked). `None` for anything else.
    pub archive: Option<ArchiveFacts>,
    /// An image whose last in-place edit was interrupted: its undo journal
    /// (`<image>.forge-undo`) is beside it, and the next edit restores the image first.
    pub interrupted_edit: bool,
    /// A patch package (`.pkg` starting `\x7FLIH`): `files` are the ones it carries whole;
    /// browsed and extracted from only, never converted or edited.
    pub patch: bool,
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
    /// The runtime at `fakelib/libSceAmpr.sprx`: `forge_release`, `forge_trace`, `zstd`
    /// (ampr-emu-ng, which reads LZ4 and zstd packs), `other` or `none`.
    pub runtime: String,
    /// The ampr_emu version of the release runtime Forge ships.
    pub shipped_runtime_version: &'static str,
    /// The ampr_emu version the game's own runtime names itself with, when it does.
    pub runtime_version: Option<String>,
    /// What a plain conversion does to the runtime (by user decision): `install` (none there),
    /// `replace` (Forge's trace runtime, an older ampr_emu or one naming no version: the
    /// bundled release runtime goes in) or `keep` (Forge's release runtime, or ampr_emu at
    /// [`Lz4Facts::shipped_runtime_version`] or newer). `None` for a non-AMPR title.
    pub runtime_action: Option<&'static str>,
    /// The same for the LZ4 tab's Unpack (a packed source) or Pack with
    /// [`Lz4Runtime::AmprEmu`] (an unpacked one), which also replaces a not-older runtime that
    /// can't read packs, and ampr-emu-ng. Pack with [`Lz4Runtime::Ng`] keeps ampr-emu-ng where
    /// [`Lz4Facts::runtime`] is `zstd` and installs it otherwise.
    pub pack_runtime_action: Option<&'static str>,
    /// `ampr_emu.index` against the files a plain conversion writes (the same comparison the
    /// job makes, the runtime as it installs, replaces or keeps it): `ok`, `stale`, `damaged`
    /// or `missing` (the last three written anew by any conversion). `None` when no conversion
    /// touches it: a packed source (kept byte for byte), a non-AMPR title or an archive's
    /// unread index.
    pub index: Option<&'static str>,
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

/// Whether `path` is at, in or above one of `guarded`, as [`Jobs::uses`] judges a job's paths
/// (Connect's hook for [`Jobs::set_external_busy`]: the files peers are writing here).
pub fn path_in_use(path: &Path, guarded: &[PathBuf]) -> bool {
    delete::in_use(path, guarded)
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

/// PS5 only, as [`scan_library`]: every `.pkg` under `roots` (any case), walked by the same
/// rules and limits, game folders entered too. Each is read only for its header and container
/// (a few KiB; a patch's blocks are not placed); one that can't be read is listed as
/// [`PackageContent::Unknown`] with no title. Sorted by title, untitled last.
pub fn scan_packages(roots: &[PathBuf]) -> anyhow::Result<PackageScan> {
    library::scan_packages(roots)
}

/// Like [`scan_library`] for one folder the user chose: any filesystem (a network share, mounts
/// nested inside it), since the choice is explicit. Errors (nothing listed) when the folder is
/// the filesystem root (Windows: the system drive root), `~/Library` or `~/AppData`, or one of the OS's own folders or
/// inside one, is under ShadowMountPlus's mounts, or cannot be read.
/// Failures below the folder are warnings.
pub fn scan_library_folder(root: &Path) -> anyhow::Result<LibraryScan> {
    library::scan_library_folder(root)
}

/// Why the desktop explorer refuses `path` (a folder to list, a delete, a move's source or
/// target): the folders [`scan_library_folder`] refuses (the filesystem root,
/// `~/Library`/`~/AppData`, the OS's own folders and everything inside), with the Library's
/// message. `None` when allowed, and always on the PS5.
pub fn folder_refusal(path: &Path) -> Option<String> {
    library::folder_refusal(path)
}

/// The cover of a Library entry (a supported image or a plain game folder, on any filesystem:
/// no locality check) as a PNG data URL. `None` when the icon is absent, empty, over 512 KiB (checked before and
/// after reading) or not a PNG; an error when the source is a link, not eligible, or cannot
/// be opened.
pub fn library_cover(path: &Path) -> anyhow::Result<Option<String>> {
    library::library_cover(path)
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

/// What a paired device writing onto this one (the server's `connect_fs`) writes with: the
/// same rules as a job's outputs here (no link followed, 0777 on the PS5, its sync retries).
pub mod out {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    /// Why a peer may not write, rename or remove at `path` (resolved): on the PS5 ShadowMount+'s
    /// mount folder and everything inside it, as a job's outputs here. ponytail: no destination
    /// probe (U1) per peer write; the sync rules below still apply.
    pub fn refusal(path: &Path) -> Option<String> {
        #[cfg(target_os = "freebsd")]
        return ps5_refusal(path);
        #[cfg(not(target_os = "freebsd"))]
        {
            let _ = path;
            None
        }
    }

    /// The PS5 destination rule, also available to a server selected by `Platform::Ps5`:
    /// refuse ShadowMount+'s mount folder and everything inside it, including aliases.
    pub fn ps5_refusal(path: &Path) -> Option<String> {
        crate::edit_job::smp_inside(path)
    }

    /// A file that must not exist yet (a link at `path` not followed).
    pub fn create_new(path: &Path) -> io::Result<File> {
        crate::finalize::create_new(path)
    }

    /// One folder; its parent must exist.
    pub fn make_dir(path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)?;
        crate::finalize::make_dir_open(path)
    }

    /// A written file's data on the drive. On FreeBSD a sync that only succeeded after a
    /// retry fails (pages may have been lost in between, U4).
    pub fn sync_file(file: &File) -> io::Result<()> {
        #[cfg(target_os = "freebsd")]
        {
            let never = std::sync::atomic::AtomicBool::new(false);
            match crate::durable::sync_retry(file, false, &never)? {
                crate::durable::Synced::Retried => Err(crate::durable::retried_sync()),
                _ => Ok(()),
            }
        }
        #[cfg(not(target_os = "freebsd"))]
        file.sync_all()
    }

    /// A folder's entries on the drive (a filesystem without folder fsync is fine).
    pub fn sync_dir(dir: &Path) -> io::Result<()> {
        crate::finalize::fsync_dir(dir)
    }

    /// The bytes an unprivileged user may still write in `dir`'s filesystem.
    pub fn free_space(dir: &Path) -> io::Result<u64> {
        #[cfg(target_os = "freebsd")]
        return crate::dest::free_at(dir);
        #[cfg(not(target_os = "freebsd"))]
        crate::preflight::fs_stat(dir).map(|(free, _)| free)
    }
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
