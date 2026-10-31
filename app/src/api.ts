// Typed IPC for app/src-tauri/src/main.rs, or the same commands over HTTP
// (crates/ps5-dump-forge-server) in the http build. Types mirror
// crates/ps5-dump-forge-core/src/lib.rs.

import { transport } from "forge-transport";
import { msg, num } from "./i18n";
import { readable, type NasProtocol } from "./peers";
import type { DropEvent, DropItem, PickOptions, Unlisten } from "./transport";

export type { DropEvent, DropItem, HostTree } from "./transport";

type UnlistenFn = Unlisten;
const invoke = transport.call;
const listen = transport.listen;

/** What a source can be (its containing file system or package); each is also a target. */
export type Kind = "folder" | "exfat" | "ffpkg" | "ffpfs" | "ffpfsc" | "pkg";
/** An archive holding one game folder: a source only (Edit, Convert, LZ4 Pack/Unpack), never a
 * target. Jobs read it in place, with nothing unpacked to disk. */
export type ArchiveKind = "zip" | "7z" | "rar";
/** Core's two password errors (`ps5_dump_forge_archive`), matched as they are; the UI shows its
 * own words for them. */
export const ARCHIVE_PASSWORD_REQUIRED = "This archive needs a password.";
export const ARCHIVE_PASSWORD_WRONG = "The password is not correct.";
/** A target: every source kind, plus the LZ4 packed folder (a folder, read as one). */
export type Format = Kind | "lz4";
/** What a job does with LZ4 asset packs, besides the target: write packs back as plain files
 * (`unpack`), or pack into `format` (`pack`: folder, exfat, ffpkg, ffpfs, ffpfsc; the `lz4`
 * format is the same as `folder` with `pack`). */
export type Lz4Mode = "unpack" | "pack";
/** The image inside a `.ffpfsc` container. */
export type InnerFormat = "exfat" | "ffpkg" | "ffpfs";
/** How hard a `.pkg` build compresses (Kraken); `stored`: every inner block raw, same layout. */
export type KrakenLevel = "fast" | "balanced" | "smallest" | "stored";
export type JobId = number;

/** One change of an edit job (`ConvertRequest.edits`), applied in order (core's `Edit`). Game
 * paths are relative and `/`-separated, with no empty, `.` or `..` component and no `\`. */
export type Edit =
  /** Copy `from` (a file or folder on the machine that runs the job; http build: a staged upload)
   * to game path `to`, replacing a file there; a folder onto a folder merges. */
  | { op: "add"; from: string; to: string }
  /** Delete a file, or a folder with everything in it. */
  | { op: "delete"; path: string }
  /** Move or rename a file or folder; `to` must not exist. */
  | { op: "rename"; from: string; to: string }
  /** A new empty folder (parents created too). */
  | { op: "mkdir"; path: string };

/** Whether an edit can be applied in place and/or rebuilt (core's `EditCheck`; reads only). */
export interface EditCheck {
  /** Why it can't be applied in place (empty: it can). */
  in_place: string[];
  /** Why it can't be rebuilt (empty: it can; a folder is never rebuilt). */
  rebuild: string[];
  /** Bytes the changes need inside the image, and what it has free (images only). */
  needed_inside: number | null;
  free_inside: number | null;
  /** Free bytes a rebuild needs beside the source, and what that folder has. */
  rebuild_needs: number | null;
  free_beside: number | null;
}

/** The runtime an LZ4 Pack installs: ampr-emu-ng (reads zstd and LZ4 packs) or upstream ampr_emu. */
export type Lz4Runtime = "ng" | "ampr_emu";

export interface ConvertRequest {
  source: string;
  format: Format;
  output: string;
  /** `null`: every core. */
  compression_threads: number | null;
  /** The image inside a `.ffpfsc`; `null`: `.ffpkg`. Ignored for any other target. */
  inner: InnerFormat | null;
  /** Leave the backport libraries (not the emulators) out of fakelib/; refused when the
   * executables' SDK was lowered. */
  remove_backport: boolean;
  /** Re-read every byte of the output; `false`: fast (sampled) verification. */
  full_verify: boolean;
  /** For `.pkg`: the compression level; missing or any other target: `"fast"`. */
  kraken_level?: KrakenLevel;
  /** For `.ffpfsc`: the zlib level, 0 (store) to 9 (smallest); missing: 6. */
  ffpfsc_level?: number;
  /** `"pack"`: pack into LZ4 asset packs; `"unpack"`: write packed assets back as plain files;
   * missing or null: neither. */
  lz4?: Lz4Mode | null;
  /** For an LZ4 pack: 0 (fast, the default) or 3..=12 (LZ4 HC; 9 medium, 12 max). */
  lz4_level?: number;
  /** For an LZ4 pack: the runtime it installs; missing: `"ng"` (ampr-emu-ng). */
  lz4_runtime?: Lz4Runtime;
  /** The Pack tab's zstd: pack into zstd packs (`ampr_zstd.index` + `ampr_zstd-NNN.pak`, zstd in
   * 256 KiB blocks) and install ampr-emu-ng. `format`: folder, exfat, ffpkg or ffpfs; no `lz4` option. */
  zstd_pack?: boolean;
  /** With `zstd_pack`: the zstd level, 1 (fastest) to 22 (smallest); missing: 5. */
  zstd_level?: number;
  /** The Pack tab's zstd: write a zstd packed source's files back as plain files into `format`. */
  zstd_unpack?: boolean;
  /** For an archive source: the password inspect accepted. Never echoed back (the job list
   * from the server has none). */
  archive_password?: string | null;
  /** Non-empty: an edit job; `output` and `format` are ignored (the source itself is changed). */
  edits?: Edit[];
  /** An edit job on an image: write a new image beside it and swap it in, instead of in place. */
  edit_rebuild?: boolean;
  /** Non-empty: an extract job; game-relative `/` paths of files or folders, each copied into
   * `output` (an existing folder) under its own last component. `format` is `"folder"`. */
  extract?: string[];
  /** Non-empty: a transfer job (Connect's copy job, its own queue beside the others); files or
   * folders on this device, a peer (`flink://`) or a NAS (`smb://`, `ftp://`, `nfs://`), each
   * copied into `output` (an existing folder here, on a peer or on a NAS) under its own name. `source` and `format` unused. */
  transfer?: string[];
  /** An extract or a transfer the user confirmed (`existingTargets` found names there): a file
   * replaces a file, a folder merges into a folder; any other clash still fails preflight. */
  replace?: boolean;
  /** Set: an install job (the PS5 only): this `.pkg` (here, on a peer or a NAS) is handed to the
   * PS5's installer, which fetches it from the job. `source`, `output` and `format` are ignored. */
  install?: string;
  /** A conversion or a pack/unpack only: once the job is Ok (output verified and in place),
   * delete the source as Delete would; its `checks` then say `source deleted: <path>` or
   * `source kept: <reason>` (the job stays Ok). */
  delete_source?: boolean;
}

/** How the output was verified. `samples` and `seed` are 0 for full. */
export interface VerifyReport {
  mode: "fast" | "full";
  checked_bytes: number;
  total_bytes: number;
  samples: number;
  seed: number;
}

export interface JobReport {
  output: string;
  bytes: number;
  files: number;
  checks: string[];
  /** Missing from older servers and restored reports. */
  verify?: VerifyReport;
}

/** serde's externally tagged `Result<JobReport, String>`. */
export type JobResult = { Ok: JobReport } | { Err: string };

export interface ProgressEvent {
  kind: "progress";
  job: JobId;
  stage: string;
  done: number;
  total: number;
}
export interface LogEvent {
  kind: "log";
  job: JobId;
  line: string;
}
/** Every stage the job will run, in order (the same ids as `ProgressEvent.stage`); sent once,
 * early (possibly after the first `scan` progress). */
export interface StagesEvent {
  kind: "stages";
  job: JobId;
  stages: string[];
}
export interface DoneEvent {
  kind: "done";
  job: JobId;
  result: JobResult;
}

export interface Dlc {
  content_id: string;
  /** The last 16 characters of the content id. */
  label: string;
  name: string | null;
  /** Its own folder; null when merged into the game's. */
  folder: string | null;
  bytes: number;
  /** Listed in the DLC emulator's dlc_emu.ini: its download_status there. */
  emulated: string | null;
}

export interface Emulator {
  path: string;
  /** "AMPR", "DLC", "PlayGo", or "Other": unrecognised homebrew that reads files from /app0/,
   * or a fakelib file that couldn't be read. Always kept, never removed. */
  name: string;
}

/** Counts from a sound LZ4 manifest. */
export interface Lz4Packed {
  /** Files the manifest lists, packed and loose. */
  files: number;
  /** Null with `stored_percent` for a manifest too large to inspect fully (over 64 MiB; a
   * finding says so). */
  packed_files: number | null;
  volumes: number;
  /** Whole percent: stored bytes of the packed files over their unpacked bytes. */
  stored_percent: number | null;
}

/** LZ4 asset packs (AMPR): present when eboot.bin imports libSceAmpr or a pack or runtime
 * artifact is there. */
export interface Lz4Facts {
  imports_ampr: boolean;
  packed: Lz4Packed | null;
  /** The manifest starts like one but doesn't parse (also a finding); `packed` is null. */
  manifest_error: string | null;
  /** `forge_trace`: the old LZ4 trace runtime, which a conversion replaces; `zstd`:
   * ampr-emu-ng, which reads zstd and LZ4 packs. */
  runtime: "forge_release" | "forge_trace" | "zstd" | "other" | "none";
  /** The ampr_emu version of the runtimes Forge ships (and installs on unpatch), e.g.
   * "0.4.2.1"; missing from an older backend. */
  shipped_runtime_version?: string;
  /** The ampr_emu version the game's own runtime names itself with, when it does. */
  runtime_version?: string | null;
  /** What a plain conversion does to the runtime: "install" (none there), "replace" (Forge's
   * trace runtime, an older ampr_emu or one naming no version: the bundled one goes in) or
   * "keep" (Forge's release runtime, or ampr_emu at the bundled version or newer:
   * runtime_version names it). Null for a non-AMPR title; missing from an older backend. */
  runtime_action?: "install" | "replace" | "keep" | null;
  /** The same for the Pack tab's LZ4 Pack (an unpacked source) or Unpack (a packed one): Pack also
   * replaces a not-older runtime that can't read packs. */
  pack_runtime_action?: "install" | "replace" | "keep" | null;
  /** ampr_emu.index against the files a plain conversion writes: "stale", "damaged" and
   * "missing" are written anew by any conversion. Null when no conversion touches it (packed:
   * kept as it is; a non-AMPR title; an archive's index too large to read); missing from an
   * older backend. */
  index?: "ok" | "stale" | "damaged" | "missing" | null;
}

export interface InspectFile {
  path: string;
  size: number;
}

/** An archive source: what the archive itself is. */
export interface ArchiveFacts {
  format: ArchiveKind;
  /** Every part on disk. */
  archive_bytes: number;
  /** The parts' file names, the first one first. */
  volumes: string[];
  solid: boolean;
  encrypted: boolean;
  /** The game folder inside the archive ("Game/"); "" when the game is at its root. */
  root: string;
}

export interface Inspection {
  /** A `Kind`, or an `ArchiveKind` for an archive. */
  kind: string;
  describe: string;
  title_id: string | null;
  content_id: string | null;
  /** In param.json's default language; `gameTitle` picks from `title_names`. */
  title_name: string | null;
  /** Every `localizedParameters` title by language tag ("en-US", "ru-RU"). Missing from an older backend. */
  title_names?: Record<string, string>;
  version: string | null;
  /** Required firmware, e.g. "7.00". */
  firmware: string | null;
  sdk: string | null;
  /** Backport libraries: fakelib/ (or fakelib2/) files that are not a known emulator;
   * empty when none. */
  backport: string[];
  /** Emulators in fakelib/ (AMPR, DLC, PlayGo, Other): homebrew the game runs with, not a
   * backport. */
  emulators: Emulator[];
  /** Why removing the backport is refused (eboot.bin's SDK is below param.json's sdkVersion,
   * or it can't be told); null when it can be removed or there is none. */
  backport_blocked: string | null;
  /** With a fakelib/ (backport or emulators): the lowest firmware its executables allow,
   * e.g. "4.50". */
  backport_firmware: string | null;
  /** The PS5 Dump Forge version that wrote the image (its maker's mark), e.g. "0.0.1-pre4";
   * null for folders, .pkg, .ffpfs and images made by other tools. */
  forge_version: string | null;
  /** DLC embedded in the dump. */
  dlcs: Dlc[];
  /** No eboot.bin but a param.json with a title ID: a DLC, an update or another package. */
  additional_content?: boolean;
  /** LZ4 asset packs; null (or missing from an older server) when the title has nothing to do
   * with AMPR. */
  lz4?: Lz4Facts | null;
  /** zstd packed (a root `ampr_zstd.index` starting `AMPRZPK1`). Missing from an older backend. */
  zstd_packed?: boolean;
  /** icon0.png as a data: URL. */
  cover: string | null;
  param_json: unknown;
  files: InspectFile[];
  empty_dirs: string[];
  total_bytes: number;
  details: string[];
  findings: string[];
  /** An archive source: the archive's own facts (`files` and `total_bytes` are unpacked). */
  archive?: ArchiveFacts | null;
  /** An image whose last in-place edit was interrupted: its undo journal (`<image>.forge-undo`)
   * is beside it, and the next edit (or check) puts the image back first. Missing from an older
   * backend. */
  interrupted_edit?: boolean;
  /** A patch package (`.pkg` starting `\x7FLIH`): `files` are the ones it carries whole;
   * browsed and extracted from only. Missing from an older backend. */
  patch?: boolean;
}

/** One job in `GET /api/jobs` (http build): what a reloaded page rebuilds its list from. */
export interface SnapshotJob {
  id: JobId;
  /** Always there: the server lists a job only once its request is known. */
  request: ConvertRequest;
  progress: ProgressEvent | null;
  /** The job's stages in order (`StagesEvent`); empty until known. */
  stages: string[];
  done: DoneEvent | null;
  /** The last lines (the server keeps 200). */
  log: string[];
  /** Lines ever logged; `log` ends at this count. */
  log_total: number;
}

/** Jobs the page doesn't know yet; `replace`: the whole list (first snapshot after loading, or
 * after the payload restarted). */
export interface JobsRestore {
  replace: boolean;
  jobs: SnapshotJob[];
}

/** The http build: a browser on the LAN, paths are the server's, nothing is revealed locally. */
export const web = transport.web;

/** The product's name in the UI: the PS5 payload's web UI drops "PS5" (its tile says "Dump
 * Forge"); the desktop app keeps it. */
export const APP_NAME = web ? "Dump Forge" : "PS5 Dump Forge";

/** A folder or file chosen in the native dialog, or (http build) the server's folder browser. */
export const pick = (o: PickOptions): Promise<string | null> => transport.pick(o);
/** Several files at once (file mode), or null when cancelled. */
export const pickFiles = (o: PickOptions): Promise<string[] | null> => transport.pickFiles(o);

/** The server can't be reached (http build); polling goes on. */
export const useOffline = transport.useOffline;

/** The server's `host:port` from its session (http build); null in the app or when unknown. */
export const serverAddress = transport.address;
/** A debug build of the PS5 payload (http build). */
export const debugBuild = transport.debug;

export type RuntimePlatform = "host" | "ps5";

export interface LibraryEntry {
  path: string;
  kind: Kind;
  title_id: string;
  title_name: string | null;
  /** As `Inspection.title_names`. */
  title_names?: Record<string, string>;
  version: string | null;
  has_cover: boolean;
  /** LZ4 packed (ampr_emu asset packs: `ampr_assets.index` at the root). Missing from an older backend. */
  lz4_packed?: boolean;
  /** zstd packed (`ampr_zstd.index` at the root). Missing from an older backend. */
  zstd_packed?: boolean;
  /** No eboot.bin but a param.json with a title ID: a DLC, an update or another package. */
  additional_content?: boolean;
}

export interface LibraryScan {
  roots: string[];
  entries: LibraryEntry[];
  truncated: boolean;
  warnings: string[];
  /** The PS5 only: the internal storage (`/data`) in bytes, as of this scan. */
  storage?: { free: number; total: number };
}

/** What a package installs, by its container (core's `PackageContent`). */
export type PackageContent = "game" | "patch" | "additional_content" | "unknown";

/** One `.pkg` the PS5's package scan found; the title fields are null when its container can't
 * be read (the console judges it). */
export interface PackageEntry {
  path: string;
  size: number;
  content_id: string | null;
  title_id: string | null;
  title_name: string | null;
  title_names: Record<string, string>;
  version: string | null;
  content: PackageContent;
  has_cover: boolean;
}

/** A Connect speed test's result (`connect.speedTest`): each direction's bytes per second over
 * `bytes` (sent, then fetched). */
export interface SpeedTest {
  up_bytes_per_s: number;
  down_bytes_per_s: number;
  bytes: number;
}

export interface PackageScan {
  entries: PackageEntry[];
  truncated: boolean;
}

/** One entry of a folder (`list_dir`). A link's `path` is where it points, resolved. */
export interface DirEntry {
  name: string;
  path: string;
  dir: boolean;
  /** A file's size; null for a folder. */
  size: number | null;
  /** The entry itself is a link (never deleted, renamed or moved from the explorer). */
  link?: boolean;
  /** An image ShadowMount+ has mounted (the explorer on the PS5): never deleted, renamed or moved. */
  mounted?: boolean;
  /** Files.tsx only: a link row's resolved target (its `path` is then the link's own). */
  target?: string;
}

/** A folder's entries, folders first (`list_dir`). */
export interface DirListing {
  /** The folder, canonical; null: the server's browse roots. */
  path: string | null;
  /** The folder above; null at a filesystem or browse root. */
  parent: string | null;
  entries: DirEntry[];
  /** More than 10,000 entries: the rest are left out. */
  truncated?: boolean;
}

// ---- Connect (the Connect tab; server `connect.rs`) ----------------------------------------

/** This device on the Connect card. */
export interface ConnectThisDevice {
  key_id: string;
  name: string;
  platform: string;
  port: number;
  /** Why the FLINK listener isn't running; null while it is. */
  listen_error: string | null;
  /** The address the default route leaves from; null when unknown. */
  ip: string | null;
}

/** A saved (paired) device. `address` `ip:port`; `last_seen` unix seconds, 0 = never. */
export interface ConnectDevice {
  key_id: string;
  name: string;
  platform: string;
  address: string | null;
  last_seen: number;
  online: boolean;
}

export interface ForgeFound {
  key_id: string;
  name: string;
  platform: string;
  address: string;
  paired: boolean;
}

export interface NasFound {
  name: string;
  service: "smb" | "ftp" | "nfs" | string;
  address: string;
  port: number;
}

export interface Discovered {
  searching: boolean;
  error: string | null;
  forge: ForgeFound[];
  nas: NasFound[];
}

/** A saved NAS: one per host + port. The password never comes back: `has_password`. */
export interface NasDevice {
  id: string;
  name: string;
  host: string;
  port: number;
  /** A game source and an output, whichever it is (its games aren't edited in place). */
  protocol: NasProtocol;
  /** Empty: guest (anonymous for FTP; always empty for NFS, which has no login). */
  user: string;
  has_password: boolean;
  online: boolean;
  /** Unix seconds of the last probe that reached it since the service started; 0 = none. */
  last_online: number;
}

/** Another device's pairing request, waiting for this device's user. `check`: the six digits
 * both screens show. */
export interface PairRequest {
  id: string;
  name: string;
  platform: string;
  ip: string;
  check: string;
}

/** This device's own pairing request: `asking` (connecting), `waiting` (`check` set: the other
 * device's user decides), then an outcome. */
export interface PairOutgoing {
  id: number;
  address: string;
  check: string | null;
  state:
    | "asking"
    | "waiting"
    | "paired"
    | "denied"
    | "timed_out"
    | "busy"
    | "already_paired"
    | "too_many_attempts"
    | "failed"
    | "cancelled";
  error: string | null;
  device: ConnectDevice | null;
}

/** What answers at an address (`connect.probe`): a Forge (its FLINK port and name) or a NAS. */
export interface ConnectProbe {
  kind: "forge" | NasProtocol;
  host: string;
  port: number;
  name: string | null;
}

export interface ConnectState {
  device: ConnectThisDevice;
  devices: ConnectDevice[];
  nas: NasDevice[];
  discovery: Discovered;
  pending: PairRequest[];
  outgoing: PairOutgoing | null;
  /** Jobs on paired peers that use this device's files (`connect_jobs.rs`); missing from an
   * older backend. */
  remote_jobs?: RemoteJob[];
}

/** A job running on a paired peer that uses this device's files: shown here read only, only
 * cancellable. Paths on this device are local; the peer's are `flink://<its key_id>/...`. */
export interface RemoteJob {
  key_id: string;
  /** The peer's saved name. */
  name: string;
  /** This device's user asked the peer to cancel it. */
  cancelling: boolean;
  /** The peer's run: its job ids start again at 1 when it restarts. */
  instance?: string;
  job: Omit<SnapshotJob, "log_total"> & { log_total?: number };
}

const cc = <T,>(cmd: string, args?: Record<string, unknown>) => transport.connect<T>(cmd, args);

/** The Connect service. Errors are strings: a refusal's code (`too_many_attempts`,
 * `not_paired`, ...) or the service's text. */
export const connect = {
  state: () => cc<ConnectState>("state"),
  /** Asks the device at `address` (ip, ip:port or host[:port]) to pair; `state.outgoing`
   * follows it. */
  pair: (address: string) => cc<null>("pair", { address }),
  pairCancel: () => cc<null>("pair_cancel"),
  /** What answers at `address` (host or host:port, IPv6 in brackets; about 3 s at most):
   * with no port a Forge first, then SMB, NFS, FTP; a Forge off the local network and
   * "nothing to connect to at <address>" are errors. */
  probe: (address: string) => cc<ConnectProbe>("probe", { address }),
  /** This device's user answers another device's request; false when it no longer waits. */
  approve: (id: string, allow: boolean) => cc<boolean>("approve", { id, allow }),
  forget: (keyId: string) => cc<boolean>("forget", { key_id: keyId }),
  discoverStart: () => cc<null>("discover_start"),
  discoverStop: () => cc<null>("discover_stop"),
  /** Saves a NAS (`host` host[:port], port default the protocol's own: 445, 21 or 2049; updated
   * in place when saved before; NFS drops `user` and `password`);
   * its id. `id`: the saved entry an edit changes. `user`/`password` undefined keep the saved
   * ones, "" clears them. */
  nasSave: (nas: { id?: string; name: string; host: string; port?: number; protocol?: NasProtocol; user?: string; password?: string }) =>
    cc<string>("nas_save", nas),
  nasForget: (id: string) => cc<boolean>("nas_forget", { id }),
  /** Asks the paired peer `keyId` to cancel its job `job` shown here (`state.remote_jobs`);
   * false when it is no longer listed. */
  cancelJob: (keyId: string, job: JobId) => cc<boolean>("cancel_job", { key_id: keyId, job }),
  /** Desktop app: elfldr at `address` (host[:port], port 9021 by default) downloads release
   * `version`'s PS5 ELF from GitHub and starts it (within a minute). */
  payloadInstall: (address: string, version: string) => cc<null>("payload_install", { address, version }),
  /** Desktop app: the ELF file at `path` streamed to elfldr at `address`; the bytes sent. */
  payloadSend: (address: string, path: string) => cc<number>("payload_send", { address, path }),
  /** Asks the paired PS5 `keyId` to install the `.pkg` at `path` on this device (it reads the
   * file over FLINK; the job shows here mirrored); the PS5's job id. */
  install: (keyId: string, path: string) => transport.connectInstall(keyId, path),
  /** Times the link to the paired device `keyId`: 256 MiB sent, then fetched, nothing on disk
   * (several seconds; rejects with the backend's text, e.g. an older peer's). */
  speedTest: (keyId: string, run: string) => transport.connectSpeedTest(keyId, run),
  /** Stops the speed test `run` with `keyId` (its `speedTest` rejects with "cancelled", at once
   * when it starts after this). */
  speedTestCancel: (keyId: string, run: string) => transport.connectSpeedTestCancel(keyId, run),
  /** http build: a browser file into the server's folder `dir`; its path. */
  upload: transport.connectUpload,
  /** http build: the URL that downloads the server's file; the app: null. */
  downloadUrl: transport.connectDownloadUrl,
};

/** Where the backend runs (the server's own answer in the http build); rejects when unknown. */
export const runtimePlatform: () => Promise<RuntimePlatform> = transport.platform;

/** PS5 only: the About tab's saved job-end notification choice. */
export const jobNotifications = {
  get: transport.notifications,
  set: transport.setNotifications,
};

export const api = {
  /** `password`: for an archive that needs one (else core answers `ARCHIVE_PASSWORD_REQUIRED`). */
  inspect: (path: string, password?: string) =>
    invoke<Inspection>("inspect", password === undefined ? { path } : { path, password }),
  /** `password`: an archive's, as `inspect` takes it (else a locked one can't be named). */
  defaultOutput: (source: string, format: Format, dir: string, password: string | null = null) =>
    invoke<string>("default_output", { source, format, dir, password }),
  /**
   * `[GAME_NAME]-[TITLE_ID].<ext>` (brackets included) in `dir`, from the
   * source's param.json; never
   * an existing path nor one in `taken` (outputs of running jobs): `-2`, `-3`, ... instead.
   */
  generatedOutput: (source: string, format: Format, dir: string, taken: string[], password: string | null = null) =>
    invoke<string>("generated_output", { source, format, dir, taken, password }),
  startJob: (request: ConvertRequest) => invoke<JobId>("start_job", { request }),
  /** A file or folder on the machine that runs the job, as an edit would add it (sizes; a
   * folder's files and empty folders, junk left out; bounded: `truncated`). */
  hostTree: (path: string) => transport.hostTree(path),
  /** Whether `request`'s edits can go in place and/or be rebuilt, before Apply (reads only). */
  editCheck: (request: ConvertRequest) => invoke<EditCheck>("edit_check", { request }),
  /** An extract's or transfer's targets that exist already (top level; on a peer or NAS too),
   * so the page asks before it sends `replace` (reads only). */
  existingTargets: (request: ConvertRequest) => invoke<string[]>("existing_targets", { request }),
  /** http build: upload a dropped file for `gamePath` of `source`; resolves with the staged path
   * an `add` takes as `from`. The app rejects (its drops are host paths). */
  stage: (
    source: string,
    gamePath: string,
    file: File,
    onProgress?: (sent: number, total: number) => void,
    signal?: AbortSignal,
  ) => transport.stage(source, gamePath, file, onProgress, signal),
  /** http build: drop `source`'s staged uploads (after Discard all, or once the job is done). */
  discardStaged: (source: string) => transport.discardStaged(source),
  cancelJob: (id: JobId) => invoke<void>("cancel_job", { id }),
  /** Delete a file or folder for good (the page asks first). Refused for a link, a drive or
   * browse root or a folder holding one, and anything an unfinished job reads or writes. */
  deletePath: (path: string) => invoke<null>("delete_path", { path }).then(() => undefined),
  /** Move or rename a file or folder on its drive: `to` (the new path) is replaced only with
   * `replace` (the user confirmed it: a file over a file, a folder merged into a folder);
   * refused as `deletePath` refuses `from`, and between drives. */
  movePath: (from: string, to: string, replace = false) =>
    invoke<null>("move_path", { from, to, replace }).then(() => undefined),
  /** One folder's entries; null (http build only): the server's browse roots. */
  listDir: (path: string | null) => invoke<DirListing>("list_dir", { path, explorer: true }),
  /** http build on the PS5: its internal storage (`/data`) in bytes; null when unknown. */
  storage: () => invoke<{ free: number; total: number } | null>("storage", {}),
  /** The app: this user's home folder (the File explorer's start), or null when there is none.
   * The http build has none (it starts at the server's roots). */
  homeFolder: () => (web ? Promise.resolve(null) : invoke<string | null>("home_folder")),
  /** Discover game dumps under the chosen folder, or the PS5's roots (read-only, bounded). `root`
   * may be left out only in the http build (the PS5 scans its roots; a computer answers 400);
   * the app's `library_scan` requires it. */
  libraryScan: (root?: string) => invoke<LibraryScan>("library_scan", root ? { root } : {}),
  /** http build on the PS5: every `.pkg` under its roots (read-only, bounded like the Library). */
  packageScan: () => transport.packageScan(),
  /** A dump's cover as a data URL, or null. */
  libraryCover: (path: string) => invoke<string | null>("library_cover", { path }),
  /** Leftover `.part` files in these folders (deduplicated,
   * missing ones skipped). */
  staleParts: (dirs: string[]) => invoke<string[]>("stale_parts", { dirs }),
  /** Show a finished job's output in Finder, Explorer or its folder (Rust looks the path up by
   * job id). The http build does nothing here: the job row shows the path instead. */
  reveal: (id: JobId) => invoke<void>("reveal", { id }),
  /** The app's GitHub button: the project page in the default browser (the http build links it). */
  openRepo: () => invoke<void>("open_repo"),
  /** Cancel every job, wait for cleanup, exit. http build: resolves once the server is gone. */
  quitApp: () => invoke<void>("quit_app"),
  /** This build's version and release file suffix; null: no updates here (`serve` on a computer). */
  updateTarget: () => transport.updateTarget(),
  /** PS5: elfldr loads `version`, the page reloads into it. The app: its download opens in the
   * browser. */
  startUpdate: (version: string) => transport.startUpdate(version),
};

export const events = {
  progress: (f: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("job://progress", f),
  stages: (f: (e: StagesEvent) => void): Promise<UnlistenFn> =>
    listen<StagesEvent>("job://stages", f),
  log: (f: (e: LogEvent) => void): Promise<UnlistenFn> =>
    listen<LogEvent>("job://log", f),
  done: (f: (e: DoneEvent) => void): Promise<UnlistenFn> =>
    listen<DoneEvent>("job://done", f),
  /** A close/quit was held back because jobs are running or (`peer`) a paired device is reading
   * or writing files here. Never in the http build. */
  closeRequested: (f: (peer: boolean) => void): Promise<UnlistenFn> =>
    listen<{ peer?: boolean } | null>("close-requested", (e) => f(e?.peer === true)),
  /** http build: jobs from the server's list (after loading, a payload restart, or another
   * browser's Build). Never in the app. */
  restore: (f: (e: JobsRestore) => void): Promise<UnlistenFn> =>
    listen<JobsRestore>("jobs://restore", f),
};

/** Files and folders dropped on the window (`over`: the pointer while dragging, null once gone).
 * Positions are CSS pixels, for `document.elementFromPoint`. */
export const onDrop = (f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten =>
  transport.onDrop(f, over);

/** What an http drop holds, as game-relative paths under `item.name`: every file and every empty
 * folder (a folder entry walked to the end). An item without an entry is its one file. */
export async function dropContents(item: DropItem): Promise<{ files: { path: string; file: File }[]; emptyDirs: string[] }> {
  const files: { path: string; file: File }[] = [];
  const emptyDirs: string[] = [];
  const walk = async (e: FileSystemEntry, path: string): Promise<void> => {
    if (e.isFile) {
      const file = await new Promise<File>((ok, fail) => (e as FileSystemFileEntry).file(ok, fail));
      files.push({ path, file });
      return;
    }
    if (!e.isDirectory) return;
    const reader = (e as FileSystemDirectoryEntry).createReader();
    let any = false;
    // readEntries answers in batches (Chrome: 100) until an empty one.
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((ok, fail) => reader.readEntries(ok, fail));
      if (batch.length === 0) break;
      any = true;
      for (const c of batch) await walk(c, `${path}/${c.name}`);
    }
    if (!any) emptyDirs.push(path);
  };
  if (item.entry) await walk(item.entry, item.name);
  else if (item.file) files.push({ path: item.name, file: item.file });
  return { files, emptyDirs };
}

/** Invoke errors arrive as the command's `String` error. */
export function errorText(e: unknown): string {
  // A paired device's `flink://` address by its name.
  return readable(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
}

// IEC units stay as they are in every language; only the decimal mark follows it.
export function prettyBytes(n: number): string {
  if (n >= 2 ** 40) return `${num(n / 2 ** 40, 2)} TiB`;
  if (n >= 2 ** 30) return `${num(n / 2 ** 30, 2)} GiB`;
  if (n >= 2 ** 20) return `${num(n / 2 ** 20, 1)} MiB`;
  if (n >= 2 ** 10) return `${num(n / 2 ** 10)} KiB`;
  return `${n} B`;
}

export function prettyDuration(ms: number): string {
  const t = msg().duration;
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return t.s(s);
  const m = Math.floor(s / 60);
  if (m < 60) return t.ms(m, s % 60);
  return t.hm(Math.floor(m / 60), m % 60);
}
