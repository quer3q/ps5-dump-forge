// Typed IPC for src-tauri/src/main.rs, or the same commands over HTTP (ps5-dump-forge-server); types mirror core's lib.rs.

import { transport } from "forge-transport";
import { msg, num } from "./i18n";
import { readable, type NasProtocol } from "./peers";
import { shownRoots } from "./prefs";
import type { DropEvent, DropItem, PickOptions, Unlisten } from "./transport";

export type { DropEvent, DropItem, HostTree } from "./transport";

type UnlistenFn = Unlisten;
const invoke = transport.call;
const listen = transport.listen;

export type Kind = "folder" | "exfat" | "ffpkg" | "ffpfs" | "ffpfsc" | "pkg";
/** An archive holding one game folder: a source only, read in place. */
export type ArchiveKind = "zip" | "7z" | "rar";
/** Core's two password errors, matched as they are. */
export const ARCHIVE_PASSWORD_REQUIRED = "This archive needs a password.";
export const ARCHIVE_PASSWORD_WRONG = "The password is not correct.";
export type Format = Kind | "lz4";
export type PackKind = "zstd" | "lz4";
/** LZ4 packs: `unpack` writes them back as plain files, `pack` packs into `format`. */
export type Lz4Mode = "unpack" | "pack";
export type InnerFormat = "exfat" | "ffpkg" | "ffpfs";
/** `stored`: every inner block raw, same layout. */
export type KrakenLevel = "fast" | "balanced" | "smallest" | "stored";
export type JobId = number;

export type EstimateKind = "zstd" | "lz4" | "kraken" | "zlib";
/** `level`: zstd 1..22, LZ4 0 or 3..12, Kraken 0 stored..3 smallest, zlib 0..9; `unpack` for Kraken or zlib of a packed game. */
export interface PackEstimateRequest {
  source: string;
  kind: EstimateKind;
  level: number;
  password?: string | null;
  unpack?: boolean;
}
/** Core's `PackEstimate`, in bytes of file data (no image or pack metadata). */
export interface PackEstimate {
  logical_bytes: number;
  /** What is compressed: the packed set, or every file for Kraken and zlib. */
  packed_set_bytes: number;
  loose_bytes: number;
  /** The loose bytes plus the packed set as estimated once compressed. */
  estimated_bytes: number;
  sampled_bytes: number;
}

/** One change of an edit job (core's `Edit`), applied in order; game paths relative, `/`-separated. */
export type Edit =
  /** `from`: a host file or folder (http build: a staged upload); a folder onto a folder merges. */
  | { op: "add"; from: string; to: string }
  | { op: "delete"; path: string }
  | { op: "rename"; from: string; to: string }
  | { op: "mkdir"; path: string };

/** core's `EditCheck`; reads only. */
export interface EditCheck {
  /** Why not in place / not rebuilt; empty: that mode works. */
  in_place: string[];
  rebuild: string[];
  needed_inside: number | null;
  free_inside: number | null;
  rebuild_needs: number | null;
  free_beside: number | null;
}

export interface ConvertRequest {
  source: string;
  format: Format;
  output: string;
  compression_threads: number | null;
  /** `null`: `.ffpkg`. */
  inner: InnerFormat | null;
  /** Refused when the executables' SDK was lowered. */
  remove_backport: boolean;
  full_verify: boolean;
  kraken_level?: KrakenLevel;
  ffpfsc_level?: number;
  lz4?: Lz4Mode | null;
  /** 0 (fast) or 3..=12 (LZ4 HC). */
  lz4_level?: number;
  lz4_runtime?: "ng";
  zstd_pack?: boolean;
  zstd_level?: number;
  zstd_unpack?: boolean;
  /** A TOML pack profile (here, a peer or a NAS); null: the built-in strategy. */
  pack_profile?: string | null;
  /** Never echoed back (the server's job list has none). */
  archive_password?: string | null;
  /** Non-empty: an edit job; `output` and `format` ignored. */
  edits?: Edit[];
  edit_rebuild?: boolean;
  /** Non-empty: an extract job; each game path copied into `output` under its last component. */
  extract?: string[];
  /** Non-empty: a transfer job; each path (here, `flink://`, a NAS) copied into `output` under its own name. */
  transfer?: string[];
  /** The user confirmed: a file replaces a file, a folder merges into a folder. */
  replace?: boolean;
  /** Set: an install job (PS5 only) of this `.pkg`. */
  install?: string;
  /** Set: a metadata recovery edit of `source` (`url` null: param.json's versionFileUri). */
  recover_metadata?: { url: string | null; original_runtime: boolean };
  /** Sony's metadata into the output; a failure is a log warning. */
  restore_metadata?: boolean;
  /** Once Ok, delete the source; `checks` then say `source deleted:` or `source kept:`. */
  delete_source?: boolean;
  /** Missing: true. */
  replace_ampr_emu?: boolean;
}

/** `samples` and `seed` are 0 for full. */
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
  verify?: VerifyReport;
  source_bytes?: number;
  /** ms since the Unix epoch (0 unknown). */
  started_at_ms?: number;
  ended_at_ms?: number;
}

/** serde's externally tagged `Result<JobReport, String>`. */
export type JobResult = { Ok: JobReport } | { Err: string };

export interface ProgressEvent {
  kind: "progress";
  job: JobId;
  stage: string;
  done: number;
  total: number;
  /** This stage alone (verify: all its passes); absent from an older peer's mirrored job. */
  stage_done?: number;
  stage_total?: number;
  /** Measured in core over the last few seconds; null until known. */
  bytes_per_s?: number | null;
  /** Epoch ms. */
  stage_started_ms?: number;
}
export interface LogEvent {
  kind: "log";
  job: JobId;
  line: string;
}
/** Every stage the job will run, in order; sent once, early. */
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
  label: string;
  name: string | null;
  folder: string | null;
  bytes: number;
  /** Its download_status in dlc_emu.ini. */
  emulated: string | null;
}

export interface Emulator {
  path: string;
  /** Unrecognised homebrew ("Other") included; always kept. */
  name: string;
}

export interface Lz4Packed {
  files: number;
  /** Null for a manifest over 64 MiB (a finding says so). */
  packed_files: number | null;
  volumes: number;
  stored_percent: number | null;
}

/** Present when eboot.bin imports libSceAmpr or a pack or runtime is there. */
export interface Lz4Facts {
  imports_ampr: boolean;
  packed: Lz4Packed | null;
  manifest_error: string | null;
  /** `forge_trace`: the old trace runtime; `zstd`: ampr-emu-ng. */
  runtime: "forge_release" | "forge_trace" | "zstd" | "other" | "none";
  shipped_runtime_version?: string;
  runtime_version?: string | null;
  /** A plain conversion's action (replace_ampr_emu on); null for a non-AMPR title. */
  runtime_action?: "install" | "replace" | "keep" | null;
  keep_runtime_action?: "install" | "replace" | "keep" | null;
  unpack_runtime_action?: "install" | "replace" | "keep" | null;
  pack_runtime_action?: "install" | "replace" | "keep" | null;
  pkg_runtime_action?: "omit" | null;
  /** ampr_emu.index against a plain conversion's files; null when no conversion touches it. */
  index?: "ok" | "stale" | "damaged" | "missing" | null;
  keep_index?: "ok" | "stale" | "damaged" | "missing" | null;
}

export interface InspectFile {
  path: string;
  size: number;
}

export interface ArchiveFacts {
  format: ArchiveKind;
  archive_bytes: number;
  volumes: string[];
  solid: boolean;
  encrypted: boolean;
  /** "" when the game is at the archive's root. */
  root: string;
}

export interface Inspection {
  kind: string;
  describe: string;
  title_id: string | null;
  content_id: string | null;
  /** In param.json's default language; `gameTitle` picks from `title_names`. */
  title_name: string | null;
  title_names?: Record<string, string>;
  version: string | null;
  firmware: string | null;
  sdk: string | null;
  /** fakelib/ files that are not a known emulator. */
  backport: string[];
  emulators: Emulator[];
  /** Why removing the backport is refused; null when it can be or there is none. */
  backport_blocked: string | null;
  backport_firmware: string | null;
  /** The maker's mark; null for folders, .pkg, .ffpfs and other tools' images. */
  forge_version: string | null;
  dlcs: Dlc[];
  /** No eboot.bin but a param.json with a title ID. */
  additional_content?: boolean;
  lz4?: Lz4Facts | null;
  zstd_packed?: boolean;
  cover: string | null;
  param_json: unknown;
  files: InspectFile[];
  empty_dirs: string[];
  total_bytes: number;
  /** The source's own size on disk (an archive's every part); null for a folder. */
  file_bytes?: number | null;
  details: string[];
  findings: string[];
  /** `files` and `total_bytes` are unpacked. */
  archive?: ArchiveFacts | null;
  /** Its undo journal is beside it; the next edit or check restores the image first. */
  interrupted_edit?: boolean;
  /** `files`: the ones it carries whole; browse and extract only. */
  patch?: boolean;
  /** Plain-ELF executables a conversion fake-signs. */
  unsigned?: string[];
}

/** One job in `GET /api/jobs` (http build). */
export interface SnapshotJob {
  id: JobId;
  request: ConvertRequest;
  progress: ProgressEvent | null;
  stages: string[];
  done: DoneEvent | null;
  /** The last lines (the server keeps 200). */
  log: string[];
  /** Lines ever logged; `log` ends at this count. */
  log_total: number;
}

/** `replace`: the whole list (first snapshot, or after the payload restarted). */
export interface JobsRestore {
  replace: boolean;
  jobs: SnapshotJob[];
}

export const web = transport.web;

/** The PS5 web UI drops "PS5"; the desktop app keeps it. */
export const APP_NAME = web ? "Dump Forge" : "PS5 Dump Forge";

export const pick = (o: PickOptions): Promise<string | null> => transport.pick(o);
export const pickFiles = (o: PickOptions): Promise<string[] | null> => transport.pickFiles(o);

export const useOffline = transport.useOffline;

/** The server's `host:port` (http build); null in the app or when unknown. */
export const serverAddress = transport.address;
export const debugBuild = transport.debug;

export type RuntimePlatform = "host" | "ps5";

export type PackageContent = "game" | "patch" | "additional_content" | "unknown";

/** The title fields are null when its container can't be read. */
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

/** Bytes per second each way over `bytes`. */
export interface SpeedTest {
  up_bytes_per_s: number;
  down_bytes_per_s: number;
  bytes: number;
}

export interface PackageScan {
  entries: PackageEntry[];
  truncated: boolean;
}

/** A link's `path` is where it points, resolved. */
export interface DirEntry {
  name: string;
  path: string;
  dir: boolean;
  size: number | null;
  link?: boolean;
  mounted?: boolean;
  /** Files.tsx only: a link row's resolved target (its `path` is then the link's own). */
  target?: string;
}

export interface DirListing {
  /** Canonical; null: the server's browse roots. */
  path: string | null;
  /** Null at a filesystem or browse root. */
  parent: string | null;
  entries: DirEntry[];
  /** More than 10,000 entries: the rest left out. */
  truncated?: boolean;
}

// ---- Connect (server `connect.rs`) ----

export interface ConnectThisDevice {
  key_id: string;
  name: string;
  platform: string;
  port: number;
  /** Null while the FLINK listener runs. */
  listen_error: string | null;
  ip: string | null;
}

/** `address` `ip:port`; `last_seen` unix seconds, 0 = never. */
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

/** One per host + port; the password never comes back (`has_password`). */
export interface NasDevice {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol: NasProtocol;
  /** Empty: guest (anonymous for FTP; always empty for NFS). */
  user: string;
  has_password: boolean;
  online: boolean;
  /** Unix seconds of the last probe that reached it; 0 = none. */
  last_online: number;
}

/** Another device's pairing request; `check`: the six digits both screens show. */
export interface PairRequest {
  id: string;
  name: string;
  platform: string;
  ip: string;
  check: string;
}

/** This device's own request: `asking`, `waiting` (`check` set), then an outcome. */
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
  /** Peers' jobs that use this device's files; missing from an older backend. */
  remote_jobs?: RemoteJob[];
}

/** A peer's job using this device's files: read only, cancellable; the peer's paths are `flink://<key_id>/...`. */
export interface RemoteJob {
  key_id: string;
  name: string;
  cancelling: boolean;
  /** The peer's run: its job ids restart at 1. */
  instance?: string;
  job: Omit<SnapshotJob, "log_total"> & { log_total?: number };
}

const cc = <T,>(cmd: string, args?: Record<string, unknown>) => transport.connect<T>(cmd, args);

/** Errors are strings: a refusal's code (`too_many_attempts`, ...) or the service's text. */
export const connect = {
  state: () => cc<ConnectState>("state"),
  pair: (address: string) => cc<null>("pair", { address }),
  pairCancel: () => cc<null>("pair_cancel"),
  /** With no port a Forge first, then SMB, NFS, FTP (about 3 s at most). */
  probe: (address: string) => cc<ConnectProbe>("probe", { address }),
  /** False when it no longer waits. */
  approve: (id: string, allow: boolean) => cc<boolean>("approve", { id, allow }),
  forget: (keyId: string) => cc<boolean>("forget", { key_id: keyId }),
  discoverStart: () => cc<null>("discover_start"),
  discoverStop: () => cc<null>("discover_stop"),
  /** Its id; `id`: the entry an edit changes; `user`/`password` undefined keep the saved ones, "" clears them. */
  nasSave: (nas: { id?: string; name: string; host: string; port?: number; protocol?: NasProtocol; user?: string; password?: string }) =>
    cc<string>("nas_save", nas),
  nasForget: (id: string) => cc<boolean>("nas_forget", { id }),
  /** False when it is no longer listed. */
  cancelJob: (keyId: string, job: JobId) => cc<boolean>("cancel_job", { key_id: keyId, job }),
  /** Desktop app: elfldr at `address` downloads release `version`'s PS5 ELF from GitHub and starts it. */
  payloadInstall: (address: string, version: string) => cc<null>("payload_install", { address, version }),
  /** Desktop app: the bytes sent. */
  payloadSend: (address: string, path: string) => cc<number>("payload_send", { address, path }),
  /** The PS5 reads the file over FLINK; its job id. */
  install: (keyId: string, path: string) => transport.connectInstall(keyId, path),
  /** 128 MiB each way, nothing on disk. */
  speedTest: (keyId: string, run: string) => transport.connectSpeedTest(keyId, run),
  /** A cancel before the run starts is remembered. */
  speedTestCancel: (keyId: string, run: string) => transport.connectSpeedTestCancel(keyId, run),
  upload: transport.connectUpload,
  /** http build only; the app: null. */
  downloadUrl: transport.connectDownloadUrl,
};

/** Rejects when unknown. */
export const runtimePlatform: () => Promise<RuntimePlatform> = transport.platform;

/** PS5 only. */
export const jobNotifications = {
  get: transport.notifications,
  set: transport.setNotifications,
};

export const api = {
  /** `password`: else core answers `ARCHIVE_PASSWORD_REQUIRED`. */
  inspect: (path: string, password?: string) =>
    invoke<Inspection>("inspect", password === undefined ? { path } : { path, password }),
  defaultOutput: (source: string, format: Format, dir: string, password: string | null = null) =>
    invoke<string>("default_output", { source, format, dir, password }),
  /** A free name in `dir` by the `naming` template (`null`: the default), never an existing path nor one in `taken`. */
  generatedOutput: (
    source: string,
    format: Format,
    dir: string,
    taken: string[],
    password: string | null = null,
    pack: PackKind | null = null,
    naming: string | null = null,
  ) => invoke<string>("generated_output", { source, format, dir, taken, password, pack, naming }),
  startJob: (request: ConvertRequest) => invoke<JobId>("start_job", { request }),
  /** As an edit would add it; bounded (`truncated`). */
  hostTree: (path: string) => transport.hostTree(path),
  /** A sample compressed with the real encoder (reads only; about 10 s at most). */
  packEstimate: (req: PackEstimateRequest) =>
    invoke<PackEstimate>("pack_estimate", { ...req, password: req.password ?? null, unpack: req.unpack ?? false }),
  editCheck: (request: ConvertRequest) => invoke<EditCheck>("edit_check", { request }),
  /** Top-level targets that exist already, so the page asks before it sends `replace`. */
  existingTargets: (request: ConvertRequest) => invoke<string[]>("existing_targets", { request }),
  /** http build: resolves with the staged path an `add` takes as `from`; the app rejects. */
  stage: (
    source: string,
    gamePath: string,
    file: File,
    onProgress?: (sent: number, total: number) => void,
    signal?: AbortSignal,
  ) => transport.stage(source, gamePath, file, onProgress, signal),
  discardStaged: (source: string) => transport.discardStaged(source),
  cancelJob: (id: JobId) => invoke<void>("cancel_job", { id }),
  /** The page asks first; core refuses links, roots and anything a job uses. */
  deletePath: (path: string) => invoke<null>("delete_path", { path }).then(() => undefined),
  /** `to` replaced only with `replace`; refused as `deletePath` refuses `from`, and between drives. */
  movePath: (from: string, to: string, replace = false) =>
    invoke<null>("move_path", { from, to, replace }).then(() => undefined),
  /** Null (http build only): the server's browse roots. */
  listDir: (path: string | null) =>
    invoke<DirListing>("list_dir", { path, explorer: true }).then(async (l) => (path === null ? shownRoots(l, await runtimePlatform().catch(() => "ps5")) : l)),
  /** http build on the PS5: `/data` in bytes; null when unknown. */
  storage: () => invoke<{ free: number; total: number } | null>("storage", {}),
  freeSpace: (path: string) => invoke<number | null>("free_space", { path }),
  /** The app: the home folder; null in the http build. */
  homeFolder: () => (web ? Promise.resolve(null) : invoke<string | null>("home_folder")),
  bundledNgVersion: () => invoke<string | null>("bundled_ng_version"),
  packageScan: () => transport.packageScan(),
  staleParts: (dirs: string[]) => invoke<string[]>("stale_parts", { dirs }),
  /** The http build does nothing here (the row shows the path). */
  reveal: (id: JobId) => invoke<void>("reveal", { id }),
  openRepo: () => invoke<void>("open_repo"),
  /** http build: resolves once the server is gone. */
  quitApp: () => invoke<void>("quit_app"),
  /** Null: no updates here. */
  updateTarget: () => transport.updateTarget(),
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
  /** Never in the http build. */
  closeRequested: (f: (peer: boolean) => void): Promise<UnlistenFn> =>
    listen<{ peer?: boolean } | null>("close-requested", (e) => f(e?.peer === true)),
  /** http build only. */
  restore: (f: (e: JobsRestore) => void): Promise<UnlistenFn> =>
    listen<JobsRestore>("jobs://restore", f),
};

/** `over`: the pointer while dragging (CSS pixels), null once gone. */
export const onDrop = (f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten =>
  transport.onDrop(f, over);

/** Game-relative paths under `item.name`: every file and every empty folder. */
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

export function errorText(e: unknown): string {
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
