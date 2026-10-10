// Typed IPC for app/src-tauri/src/main.rs, or the same commands over HTTP
// (crates/ps5-dump-forge-server) in the http build. Types mirror
// crates/ps5-dump-forge-core/src/lib.rs.

import { transport } from "forge-transport";
import type { PickOptions, Unlisten } from "./transport";

type UnlistenFn = Unlisten;
const invoke = transport.call;
const listen = transport.listen;

/** What a source can be (its containing file system or package); each is also a target. */
export type Kind = "folder" | "exfat" | "ffpkg" | "ffpfs" | "ffpfsc" | "pkg";
/** A target: every source kind, plus the LZ4 packed folder (a folder, read as one). */
export type Format = Kind | "lz4";
/** What a job does with LZ4 asset packs, besides the target: install the trace runtime, write
 * packs back as plain files, or put Forge's release runtime back. */
/** `pack`: LZ4 packs into `format` (folder, exfat, ffpkg, ffpfs, ffpfsc); the `lz4` format is
 * the same as `folder` with `pack`. */
export type Lz4Mode = "trace" | "unpack" | "unpatch" | "pack";
/** The image inside a `.ffpfsc` container. */
export type InnerFormat = "exfat" | "ffpkg" | "ffpfs";
/** How hard a `.pkg` build compresses (Kraken). */
export type KrakenLevel = "fast" | "balanced" | "smallest";
export type JobId = number;

export interface ConvertRequest {
  source: string;
  format: Format;
  output: string;
  /** `null`: every core. */
  compression_threads: number | null;
  /** The image inside a `.ffpfsc`; `null` (or any other target): `.exfat`. */
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
  /** `"trace"`: install the trace runtime (folder, `.exfat`, `.ffpkg`); `"unpack"`: write
   * packed assets back as plain files; `"unpatch"`: install Forge's release runtime, drop the
   * journal and logs; missing or null: none of these. */
  lz4?: Lz4Mode | null;
  /** With `"trace"` or `"unpatch"` on an `.exfat`/`.ffpkg` source and `format` the source's
   * own: the verified output replaces the source (`output` is derived: a `.part` beside it). */
  lz4_in_place?: boolean;
  /** For the LZ4 target: a TOML rules profile (a path on the machine that runs the job). */
  lz4_profile?: string | null;
  /** For the LZ4 target: a trace journal (`ampr_commands.bin`) copied from elsewhere, with its
   * `ampr_emu.index` next to it; never with `lz4_profile`. */
  lz4_traces?: string | null;
  /** With `"trace"` into an image: the free space added for the trace, 64–1024 MiB in 64 MiB steps; missing: 256. */
  lz4_trace_space_mib?: number;
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

/** LZ4 asset packs (AMPR): present when eboot.bin imports libSceAmpr or a pack, trace or
 * runtime artifact is there. */
export interface Lz4Facts {
  imports_ampr: boolean;
  packed: Lz4Packed | null;
  /** The manifest starts like one but doesn't parse (also a finding); `packed` is null. */
  manifest_error: string | null;
  runtime: "forge_release" | "forge_trace" | "other" | "none";
  /** The ampr_emu version of the runtimes Forge ships (and installs on unpatch), e.g.
   * "0.4.2.1"; missing from an older backend. */
  shipped_runtime_version?: string;
  /** Size of the trace journal (ampr_commands.bin), when present. */
  journal_bytes: number | null;
  /** With the journal and its ampr_emu.index both present: the name Get traces' zip takes. */
  traces_zip: string | null;
}

export interface InspectFile {
  path: string;
  size: number;
}

export interface Inspection {
  kind: string;
  describe: string;
  title_id: string | null;
  content_id: string | null;
  title_name: string | null;
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
  /** LZ4 asset packs; null (or missing from an older server) when the title has nothing to do
   * with AMPR. */
  lz4?: Lz4Facts | null;
  /** icon0.png as a data: URL. */
  cover: string | null;
  param_json: unknown;
  files: InspectFile[];
  empty_dirs: string[];
  total_bytes: number;
  details: string[];
  findings: string[];
}

/** Save as profile: the pack plan Forge resolved for a Pack request, as an editable TOML. */
export interface Lz4PlanProfile {
  /** `[GAME_NAME]-[TITLE_ID]-lz4profile.toml` (the app: the name saved under). */
  file_name: string;
  toml: string;
  packed: number;
  loose: number;
  /** How the plan was resolved (rule source, keep-loose, auto-loose), as a job logs it. */
  log: string[];
}

/** What patching (or unpatching) a folder in place for LZ4 tracing did. */
export interface Lz4Patch {
  /** Files listed in the fresh `ampr_emu.index`. */
  indexed: number;
  /** Stale trace files deleted from the folder's root (relative paths). */
  removed: string[];
  /** The installed runtime's known issue (`ps5_dump_forge_lz4::runtime::WARNING`). */
  warning?: string;
}

/** One job in `GET /api/jobs` (http build): what a reloaded page rebuilds its list from. */
export interface SnapshotJob {
  id: JobId;
  request: ConvertRequest | null;
  progress: ProgressEvent | null;
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

/** A folder or file chosen in the native dialog, or (http build) the server's folder browser. */
export const pick = (o: PickOptions): Promise<string | null> => transport.pick(o);

/** The server can't be reached (http build); polling goes on. */
export const useOffline = transport.useOffline;

/** The server's `host:port` from its session (http build); null in the app or when unknown. */
export const serverAddress = transport.address;

export const api = {
  inspect: (path: string) => invoke<Inspection>("inspect", { path }),
  defaultOutput: (source: string, format: Format, dir: string) =>
    invoke<string>("default_output", { source, format, dir }),
  /**
   * `[GAME_NAME]-[TITLE_ID].<ext>` (brackets included) in `dir`, from the
   * source's param.json; never
   * an existing path nor one in `taken` (outputs of running jobs): `-2`, `-3`, ... instead.
   */
  generatedOutput: (source: string, format: Format, dir: string, taken: string[]) =>
    invoke<string>("generated_output", { source, format, dir, taken }),
  startJob: (request: ConvertRequest) => invoke<JobId>("start_job", { request }),
  cancelJob: (id: JobId) => invoke<void>("cancel_job", { id }),
  /** Patch a game folder in place for LZ4 tracing: the trace runtime in fakelib/, a fresh
   * index, stale traces removed. Not a job; core refuses it while a job runs. */
  lz4Patch: (source: string) => transport.lz4Patch<Lz4Patch>(source),
  /** Put Forge's release runtime back in a game folder in place: journal and logs removed, a
   * fresh index. Not a job; core refuses it while a job runs. */
  lz4Unpatch: (source: string) => transport.lz4Unpatch<Lz4Patch>(source),
  /** Save as profile for a Pack request: the app's save dialog (offering `name` in `dir`) then
   * writes it; the http build downloads it. Reads the source, not a job. null: cancelled. */
  savePlanProfile: (request: ConvertRequest, name: string, dir: string) =>
    transport.savePlanProfile<Lz4PlanProfile>(request, name, dir),
  /** Delete a file or folder for good (the page asks first). Refused for a link, a drive or
   * browse root or a folder holding one, and anything an unfinished job reads or writes. */
  deletePath: (path: string) => invoke<null>("delete_path", { path }).then(() => undefined),
  /** Leftover `.part` files in these folders (deduplicated, missing ones skipped). */
  staleParts: (dirs: string[]) => invoke<string[]>("stale_parts", { dirs }),
  /** Show a finished job's output in Finder, Explorer or its folder (Rust looks the path up by
   * job id). The http build does nothing here: the job row shows the path instead. */
  reveal: (id: JobId) => invoke<void>("reveal", { id }),
  /** The app's GitHub button: the project page in the default browser (the http build links it). */
  openRepo: () => invoke<void>("open_repo"),
  /** Cancel every job, wait for cleanup, exit. http build: resolves once the server is gone. */
  quitApp: () => invoke<void>("quit_app"),
};

export const events = {
  progress: (f: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("job://progress", f),
  log: (f: (e: LogEvent) => void): Promise<UnlistenFn> =>
    listen<LogEvent>("job://log", f),
  done: (f: (e: DoneEvent) => void): Promise<UnlistenFn> =>
    listen<DoneEvent>("job://done", f),
  /** A close/quit was held back because jobs are running. Never in the http build. */
  closeRequested: (f: () => void): Promise<UnlistenFn> => listen("close-requested", () => f()),
  /** http build: jobs from the server's list (after loading, a payload restart, or another
   * browser's Build). Never in the app. */
  restore: (f: (e: JobsRestore) => void): Promise<UnlistenFn> =>
    listen<JobsRestore>("jobs://restore", f),
};

/** Invoke errors arrive as the command's `String` error. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function prettyBytes(n: number): string {
  if (n >= 2 ** 40) return `${(n / 2 ** 40).toFixed(2)} TiB`;
  if (n >= 2 ** 30) return `${(n / 2 ** 30).toFixed(2)} GiB`;
  if (n >= 2 ** 20) return `${(n / 2 ** 20).toFixed(1)} MiB`;
  if (n >= 2 ** 10) return `${(n / 2 ** 10).toFixed(0)} KiB`;
  return `${n} B`;
}

export function prettyDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}
