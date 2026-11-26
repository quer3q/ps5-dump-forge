//! Conversion core: scanning, preflight, jobs, verification, finalization. The CLI, the server
//! and the Tauri app build against this public surface.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod archive;
mod backport;
mod convert;
mod delete;
#[cfg(any(target_os = "freebsd", all(test, target_os = "macos")))]
mod dest;
mod dlc;
mod edit;
mod edit_folder;
mod edit_job;
mod estimate;
pub use estimate::{EstimateKind, PackEstimate, pack_estimate};
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
mod fself;
mod library;
mod library_roots;
mod listing;
mod lz4;
mod metadata;
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
pub use library::{PackageContent, PackageEntry, PackageScan};
pub use listing::{ListEntry, Listing, list_dir, list_explorer};
pub use ps5_dump_forge_archive::{
    ARCHIVE_PASSWORD_REQUIRED, ARCHIVE_PASSWORD_WRONG, ArchiveFormat,
};
pub use scan::ScannedFolder;

/// What a conversion produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Folder,
    Exfat,
    /// UFS2.
    Ffpkg,
    /// Uncompressed PFS.
    Ffpfs,
    /// Compressed PFS container around one image ([`ConvertRequest::inner`]).
    Ffpfsc,
    /// Debug FPKG.
    Pkg,
    /// An LZ4 packed folder: the same as `Folder` with [`Lz4Mode::Pack`] (kept for older requests).
    Lz4,
}

/// What a job does with LZ4 asset packs besides its target ([`ConvertRequest::lz4`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lz4Mode {
    Unpack,
    /// Into the request's `format` (not `.pkg`); an image in two passes, no staging folder.
    Pack,
}

/// The packs a Pack job makes, for its generated name's `{pack}` ([`generated_output`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackKind {
    Zstd,
    Lz4,
}

/// The naming template [`generated_output`] uses when none is given: `[Title]-[TITLE_ID]`.
pub const DEFAULT_NAMING: &str = "[{title}]-[{title_id}]";

/// Why `template` can't name an output (English), `None` when it can.
pub fn naming_problem(template: &str) -> Option<String> {
    inspect::parse_naming(template).err()
}

/// The runtime an LZ4 Pack installs ([`ConvertRequest::lz4_runtime`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lz4Runtime {
    #[default]
    Ng,
    /// No longer bundled: a preflight finding.
    AmprEmu,
}

/// How hard a `.pkg` build compresses its image: the encoder's `kraken::Level`, named the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KrakenLevel {
    /// A lazy parse. On a third-party package's blocks: 331 MB/s on 14 cores, 36.8% of the bytes.
    #[default]
    Fast,
    /// An optimal parse priced twice: ~6x slower than `Fast`, ~2.6% smaller (57 MB/s, 35.8%).
    Balanced,
    /// ~9x slower than `Fast` (38 MB/s, 35.7%).
    Smallest,
    /// Every block raw: the fallback for a console that won't start a compressed package.
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

/// A metadata recovery's options ([`ConvertRequest::recover_metadata`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetadataRecovery {
    /// The game's version XML, its manifest or the `_sc.pkg` piece itself; `None` (or empty):
    /// `param.json`'s `versionFileUri`.
    #[serde(default)]
    pub url: Option<String>,
    /// Take Sony's `sdkVersion`, `requiredSystemSoftwareVersion` and `applicationDrmType` too.
    #[serde(default)]
    pub original_runtime: bool,
}

/// One conversion: any readable source to one target format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertRequest {
    /// A game folder, an image, a `.pkg`, or an archive's (first part's) path.
    pub source: PathBuf,
    pub format: Format,
    /// The final output path (file, or directory for `Folder`). Must not exist yet.
    pub output: PathBuf,
    /// Compression thread cap for `.pkg` and `.ffpfsc` builds (all cores when unset).
    #[serde(default)]
    pub compression_threads: Option<usize>,
    /// For `.ffpfsc`: the image inside, `Ffpkg` (when unset), `Exfat` or `Ffpfs`.
    #[serde(default)]
    pub inner: Option<Format>,
    /// Leave the backport out ([`Inspection::backport`]).
    #[serde(default)]
    pub remove_backport: bool,
    /// Re-read every byte of the output; unset, a seeded sample ([`VerifySummary`]).
    #[serde(default)]
    pub full_verify: bool,
    /// For `.pkg`.
    #[serde(default)]
    pub kraken_level: KrakenLevel,
    /// For `.ffpfsc`: the zlib level, 0 (store) through 9.
    #[serde(default = "default_ffpfsc_level")]
    pub ffpfsc_level: u32,
    /// Pack or unpack LZ4 packs; unset, packs travel as plain files.
    #[serde(default)]
    pub lz4: Option<Lz4Mode>,
    /// When packing LZ4: 0 (fast) or an HC level 3 through 12.
    #[serde(default)]
    pub lz4_level: u8,
    #[serde(default)]
    pub lz4_runtime: Lz4Runtime,
    /// Replace a game's ampr_emu build with the bundled ampr-emu-ng (else keep it).
    #[serde(default = "default_true")]
    pub replace_ampr_emu: bool,
    /// Pack the game into zstd packs (`docs/zstd.md`), into any `format` but `Pkg`.
    #[serde(default)]
    pub zstd_pack: bool,
    /// With [`Self::zstd_pack`]: 1 through 22.
    #[serde(default = "default_zstd_level")]
    pub zstd_level: u8,
    /// Decode a zstd-packed source's packs into its logical files, into any `format`.
    #[serde(default)]
    pub zstd_unpack: bool,
    /// With an LZ4 or zstd pack: a TOML pack profile (`docs/packing.md`) on any device.
    #[serde(default)]
    pub pack_profile: Option<PathBuf>,
    /// File changes applied in place, in order (an edit job when not empty).
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// With [`Self::edits`] on an image: rebuild it beside and rename it over the source.
    #[serde(default)]
    pub edit_rebuild: bool,
    /// Game paths to copy out into the folder `output`, each under its last component (an
    /// extract job when not empty).
    #[serde(default)]
    pub extract: Vec<String>,
    /// Paths on any device to copy into the folder `output` (a transfer job when not empty).
    #[serde(default)]
    pub transfer: Vec<PathBuf>,
    /// For an extract or a transfer: replace a file onto a file, merge a folder onto a folder.
    #[serde(default)]
    pub replace: bool,
    /// A `.pkg` on any device to hand to the PS5's installer (an install job when set).
    #[serde(default)]
    pub install: Option<PathBuf>,
    /// Edit's switch: restore the source's `sce_sys` metadata from Sony's update server, with
    /// [`Self::edits`] as one edit job. Needs the internet.
    #[serde(default)]
    pub recover_metadata: Option<MetadataRecovery>,
    /// Build's switch: the output gets Sony's metadata for this version; a failure is a warning.
    #[serde(default)]
    pub restore_metadata: bool,
    /// For a conversion or a pack/unpack: delete the source once the job is done, as
    /// [`Jobs::delete_path`] deletes; the outcome is one line in [`JobReport::checks`].
    #[serde(default)]
    pub delete_source: bool,
    /// For an archive source that needs one. Never serialized.
    #[serde(default, skip_serializing)]
    pub archive_password: Option<ArchivePassword>,
}

/// One change of an edit ([`ConvertRequest::edits`]); game paths are relative and `/`-separated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Edit {
    /// Copy a file or folder to `to`, replacing a file there; a folder onto a folder merges.
    Add {
        from: PathBuf,
        to: String,
    },
    Delete {
        path: String,
    },
    /// Move or rename a file or folder; `to` must not exist.
    Rename {
        from: String,
        to: String,
    },
    /// A new empty folder (parents created too).
    Mkdir {
        path: String,
    },
}

/// Which way an edit request can be applied ([`Jobs::edit_check`]); nothing written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditCheck {
    /// Why it can't be applied in place (empty = it can).
    pub in_place: Vec<String>,
    /// Why it can't be rebuilt (empty = it can).
    pub rebuild: Vec<String>,
    /// `.exfat`/`.ffpkg`: bytes the changes need inside the image and what it has free.
    pub needed_inside: Option<u64>,
    pub free_inside: Option<u64>,
    /// Free bytes a rebuild needs beside the source, and what the folder has.
    pub rebuild_needs: Option<u64>,
    pub free_beside: Option<u64>,
}

pub const DEFAULT_FFPFSC_LEVEL: u32 = ps5_dump_forge_pfs::DEFAULT_LEVEL;

fn default_ffpfsc_level() -> u32 {
    DEFAULT_FFPFSC_LEVEL
}

pub const DEFAULT_ZSTD_LEVEL: u8 = 5;

fn default_zstd_level() -> u8 {
    DEFAULT_ZSTD_LEVEL
}

fn default_true() -> bool {
    true
}

pub type JobId = u64;

/// What a running job tells its owner, serialized as `{"kind": "...", ...}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// Once, first: every `Progress` stage the job will report, in order.
    Stages {
        job: JobId,
        stages: Vec<String>,
    },
    Progress {
        job: JobId,
        stage: String,
        /// Bytes over the whole job, every pass included; `total` can grow, 0 until known.
        done: u64,
        total: u64,
        /// Bytes of this stage alone, every pass in it added up; 0 of 0 when it moves none.
        stage_done: u64,
        stage_total: u64,
        /// This stage's speed over its last few seconds, `None` until known.
        bytes_per_s: Option<u64>,
        /// Epoch ms.
        stage_started_ms: u64,
    },
    Log {
        job: JobId,
        line: String,
    },
    Done {
        job: JobId,
        /// `Err` is `"cancelled"`, or `"cancelled; "` + what stays changed, when cancelled.
        result: Result<JobReport, String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct JobReport {
    pub output: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub checks: Vec<String>,
    pub verify: VerifySummary,
    /// Logical bytes of the source the job read (its files' sizes, an edit's added files).
    pub source_bytes: u64,
    /// Epoch ms, set by the job runner.
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
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

/// What the content comparison read back; structural checks run in both modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerifySummary {
    pub mode: VerifyMode,
    pub checked_bytes: u64,
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
        metadata::clean_stale();
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

    /// Deletes `path`, a file or a folder (links inside removed, not followed), for good; the
    /// refusals are `delete.rs`'s (AGENTS.md, exception 1). No job is admitted while it runs.
    pub fn delete_path(&self, path: &Path, protected: &[PathBuf]) -> Result<(), DeleteError> {
        self.inner.delete_path(path, protected)
    }

    /// Moves or renames `from` to `to` (a new name in an existing folder) on the same volume;
    /// `from` refused as [`Jobs::delete_path`] refuses it. With `replace` a file goes over a
    /// file and a folder merges into a folder. Every check runs before the first rename.
    pub fn move_path(
        &self,
        from: &Path,
        to: &Path,
        replace: bool,
        protected: &[PathBuf],
    ) -> Result<(), DeleteError> {
        self.inner.move_path(from, to, replace, protected)
    }

    /// Whether an unfinished job reads or writes `path`, or anything inside or around it.
    /// A snapshot: a job may start right after.
    pub fn uses(&self, path: &Path) -> bool {
        self.inner.uses(path)
    }

    /// [`Jobs::uses`] as a hook of its own (Connect's).
    pub fn uses_hook(&self) -> Arc<dyn Fn(&Path) -> bool + Send + Sync> {
        let inner = self.inner.clone();
        Arc::new(move |path| inner.uses(path))
    }

    /// Why a file must not be put in place of `path` (a peer's replacing rename here), as a
    /// job's replace is refused. Nothing at `path` passes. A snapshot.
    pub fn replace_check(&self, path: &Path) -> Result<(), DeleteError> {
        self.inner.replace_check(path)
    }

    /// Sets what says another device is writing a path here: jobs, Delete and Move refuse it.
    pub fn set_external_busy(&self, busy: Arc<dyn Fn(&Path) -> bool + Send + Sync>) {
        self.inner.set_external_busy(busy);
    }

    /// Sets what replacing never touches, as `protected` is for [`Jobs::delete_path`].
    pub fn set_protected(&self, protected: Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>) {
        self.inner.set_protected(protected);
    }

    /// The top-level targets of an extract or transfer that exist already. Reads only.
    pub fn existing_targets(&self, request: &ConvertRequest) -> anyhow::Result<Vec<PathBuf>> {
        extract_paths::existing_targets(request)
    }

    /// Which way `request`'s edits can be applied. A leftover undo journal is settled first,
    /// unless a job is using the image.
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
    /// `folder`, `exfat`, ..., `pkg`; `zip`, `7z` or `rar` for an archive.
    pub kind: String,
    pub describe: String,
    pub title_id: Option<String>,
    pub content_id: Option<String>,
    /// No `eboot.bin` at the root but a `param.json` with a title id.
    pub additional_content: bool,
    /// The rest from `param.json`: the default language's title, the localized ones by tag,
    /// `contentVersion`, `requiredSystemSoftwareVersion` and `sdkVersion` (`7.00`).
    pub title_name: Option<String>,
    pub title_names: std::collections::BTreeMap<String, String>,
    pub version: Option<String>,
    pub firmware: Option<String>,
    pub sdk: Option<String>,
    /// The `fakelib`/`fakelib2` files that are not a known emulator.
    pub backport: Vec<String>,
    /// The emulators there (AMPR, DLC, PlayGo); removing the backport keeps them.
    pub emulators: Vec<Emulator>,
    /// Why removing the backport is refused, `None` when removable or there is none.
    pub backport_blocked: Option<String>,
    /// With a `fakelib`: the lowest firmware the executables allow (`4.50`).
    pub backport_firmware: Option<String>,
    pub dlcs: Vec<Dlc>,
    /// From the maker's mark of an `.exfat`, `.ffpkg` or a `.ffpfsc`'s inner image.
    pub forge_version: Option<String>,
    /// `sce_sys/icon0.png` as a `data:image/png;base64,` URL, when present and small.
    pub cover: Option<String>,
    pub param_json: Option<serde_json::Value>,
    pub files: Vec<InspectFile>,
    pub empty_dirs: Vec<String>,
    pub total_bytes: u64,
    /// The source's own size on disk (an archive's every part); `None` for a folder (`total_bytes`).
    pub file_bytes: Option<u64>,
    pub details: Vec<String>,
    pub findings: Vec<String>,
    /// `None` for a non-AMPR title holding no LZ4 artifact.
    pub lz4: Option<Lz4Facts>,
    /// A root `ampr_zstd.index` starting `AMPRZPK1`.
    pub zstd_packed: bool,
    /// An archive's own facts (`files` and `total_bytes` are the game's).
    pub archive: Option<ArchiveFacts>,
    /// An undo journal (`<image>.forge-undo`) is beside the image.
    pub interrupted_edit: bool,
    /// A patch package (`\x7FLIH`): `files` are the ones it carries whole.
    pub patch: bool,
    /// Plain ELF executables, not fake-signed SELF; a conversion signs them.
    pub unsigned: Vec<String>,
}

/// What an archive source is.
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveFacts {
    pub format: ArchiveFormat,
    /// Every part's bytes, and their names, the first one first.
    pub archive_bytes: u64,
    pub volumes: Vec<String>,
    pub solid: bool,
    pub encrypted: bool,
    /// The game folder inside the archive (`Game/`); empty when the game is at its root.
    pub root: String,
}

/// What `inspect` found about AMPR and LZ4 packs in the logical game.
#[derive(Debug, Clone, Serialize)]
pub struct Lz4Facts {
    pub imports_ampr: bool,
    pub packed: Option<Lz4Packed>,
    /// Why a root `ampr_assets.index` with the pack magic does not parse (also a finding).
    pub manifest_error: Option<String>,
    /// `forge_release`, `forge_trace`, `zstd` (ampr-emu-ng), `other` or `none`.
    pub runtime: String,
    /// The ampr_emu version Forge last bundled.
    pub shipped_runtime_version: &'static str,
    pub runtime_version: Option<String>,
    /// `install`, `replace` or `keep` (AGENTS.md's runtime rule): a plain conversion's, with
    /// `replace_ampr_emu` on, then off; an unpack's, a Pack's, and into a `.pkg` `omit`.
    /// `None` for a non-AMPR title.
    pub runtime_action: Option<&'static str>,
    pub keep_runtime_action: Option<&'static str>,
    pub unpack_runtime_action: Option<&'static str>,
    pub pack_runtime_action: Option<&'static str>,
    pub pkg_runtime_action: Option<&'static str>,
    /// `ampr_emu.index` against what a plain conversion writes: `ok`, `stale`, `damaged` or
    /// `missing`; `None` when no conversion touches it. `keep_index` with `replace_ampr_emu` off.
    pub index: Option<&'static str>,
    pub keep_index: Option<&'static str>,
}

/// Counts from an LZ4 manifest.
#[derive(Debug, Clone, Serialize)]
pub struct Lz4Packed {
    /// Packed and loose.
    pub files: u64,
    /// `None` when the manifest was too large to parse.
    pub packed_files: Option<u64>,
    pub volumes: u64,
    /// Stored over logical bytes of the packed files, whole percent.
    pub stored_percent: Option<u64>,
}

/// An emulator in `fakelib`/`fakelib2`, named by the file it reads from the game root.
#[derive(Debug, Clone, Serialize)]
pub struct Emulator {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectFile {
    pub path: String,
    pub size: u64,
}

/// Whether `path` is at, in or above one of `guarded`, as [`Jobs::uses`] judges.
pub fn path_in_use(path: &Path, guarded: &[PathBuf]) -> bool {
    delete::in_use(path, guarded)
}

pub fn inspect(path: &Path) -> anyhow::Result<Inspection> {
    inspect::inspect(path, None)
}

/// [`inspect`] with an archive's password; errors are exactly [`ARCHIVE_PASSWORD_REQUIRED`]
/// or [`ARCHIVE_PASSWORD_WRONG`] for one missing or wrong.
pub fn inspect_with(path: &Path, password: Option<&str>) -> anyhow::Result<Inspection> {
    inspect::inspect(path, password)
}

/// The bytes free on the volume holding the folder `dir`, on any device; `None` when unknown.
pub fn free_space(dir: &Path) -> Option<u64> {
    if remote::is_remote(dir) {
        let at = remote::FlinkPath::parse(dir).ok()?;
        if at.scheme == remote::Scheme::Ftp {
            return None;
        }
        return remote::get_for(&at).ok()?.free_space(&at).ok();
    }
    out::free_space(dir).ok()
}

/// PS5 only: every `.pkg` under `roots` on local disk filesystems, within the walk's limits
/// (`library.rs`), each read only for its header and container. Sorted by title, untitled last.
pub fn scan_packages(roots: &[PathBuf]) -> anyhow::Result<PackageScan> {
    library::scan_packages(roots)
}

/// Why the desktop explorer refuses `path` (`library_roots::refusal`); always `None` on the PS5.
pub fn folder_refusal(path: &Path) -> Option<String> {
    library::folder_refusal(path)
}

/// The default output path for `source` in `format`: in `dir`, named from the title id. An
/// empty `dir` is the folder holding the source (absolute, never the working directory).
pub fn default_output(source: &Path, format: Format, dir: &Path) -> anyhow::Result<PathBuf> {
    inspect::default_output(source, format, dir, None)
}

/// [`default_output`] with an archive's password.
pub fn default_output_with(
    source: &Path,
    format: Format,
    dir: &Path,
    password: Option<&str>,
) -> anyhow::Result<PathBuf> {
    inspect::default_output(source, format, dir, password)
}

/// The output path for `source` in `format` in `dir` (empty: the source's folder), named by
/// the `naming` template (`None`: [`DEFAULT_NAMING`]; `inspect::Template`). Never a path that
/// exists or is in `taken`: `-2`, `-3`, ... instead; the stem stays within
/// `preflight::stem_limit`.
pub fn generated_output(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
    pack: Option<PackKind>,
    naming: Option<&str>,
) -> anyhow::Result<PathBuf> {
    inspect::generated_output(source, format, dir, taken, None, pack, naming)
}

/// [`generated_output`] with an archive's password (errors as [`inspect_with`]'s).
pub fn generated_output_with(
    source: &Path,
    format: Format,
    dir: &Path,
    taken: &[PathBuf],
    password: Option<&str>,
    pack: Option<PackKind>,
    naming: Option<&str>,
) -> anyhow::Result<PathBuf> {
    inspect::generated_output(source, format, dir, taken, password, pack, naming)
}

/// Leftover `*.part` files/dirs from earlier jobs in `dir`. Never deleted automatically.
pub fn stale_parts(dir: &Path) -> Vec<PathBuf> {
    finalize::stale_parts(dir)
}

/// What a paired device writing onto this one writes with: a job's output rules.
pub mod out {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    /// Why a peer may not write, rename or remove at `path`: on the PS5 ShadowMount+'s mount
    /// folder. ponytail: no destination probe (U1) per peer write.
    pub fn refusal(path: &Path) -> Option<String> {
        #[cfg(target_os = "freebsd")]
        return ps5_refusal(path);
        #[cfg(not(target_os = "freebsd"))]
        {
            let _ = path;
            None
        }
    }

    /// The PS5 rule on any OS (a server selected by `Platform::Ps5`).
    pub fn ps5_refusal(path: &Path) -> Option<String> {
        crate::edit_job::smp_inside(path)
    }

    /// A file that must not exist yet (a link at `path` not followed).
    pub fn create_new(path: &Path) -> io::Result<File> {
        crate::finalize::create_new(path)
    }

    /// A file made by [`create_new`] and closed, open again; refused unless it matches its
    /// [`closed_stamp`].
    pub fn reopen(path: &Path, (dev, len): (u64, u64)) -> io::Result<File> {
        crate::finalize::reopen(path, dev, len)
    }

    /// What [`reopen`] checks: the file's device and length.
    pub fn closed_stamp(file: &File) -> io::Result<(u64, u64)> {
        Ok((crate::finalize::handle_id(file)?.0, file.metadata()?.len()))
    }

    /// One folder; its parent must exist.
    pub fn make_dir(path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)?;
        crate::finalize::make_dir_open(path)
    }

    /// On FreeBSD a sync that only succeeded after a retry fails (pages may be lost, U4).
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

/// Where the app keeps its WebView data: always next to the app.
#[derive(Debug, Clone, Serialize)]
pub struct DataDirs {
    /// The folder holding the exe, or `PS5 Dump Forge.app` on macOS.
    pub root: PathBuf,
}

impl DataDirs {
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

/// Whether a test's link `made` it: false only for EOPNOTSUPP/ENOTSUP (msdosfs).
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
