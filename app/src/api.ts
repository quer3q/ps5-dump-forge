// Typed IPC for app/src-tauri/src/main.rs, or the same commands over HTTP
// (crates/ps5-dump-forge-server) in the http build. Types mirror
// crates/ps5-dump-forge-core/src/lib.rs.

import { transport } from "forge-transport";
import type { PickOptions, Unlisten } from "./transport";

type UnlistenFn = Unlisten;
const invoke = transport.call;
const listen = transport.listen;

export type Format = "folder" | "exfat" | "ffpkg" | "ffpfs" | "ffpfsc" | "pkg";
/** What a source can be: every target format is also read. */
export type Kind = Format;
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
  /** icon0.png as a data: URL. */
  cover: string | null;
  param_json: unknown;
  files: InspectFile[];
  empty_dirs: string[];
  total_bytes: number;
  details: string[];
  findings: string[];
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

export const api = {
  inspect: (path: string) => invoke<Inspection>("inspect", { path }),
  defaultOutput: (source: string, format: Format, dir: string) =>
    invoke<string>("default_output", { source, format, dir }),
  /**
   * `[GAME_NAME]-[TITLE_ID]-[FIRMWARE].<ext>` (brackets included) in `dir`, from the
   * source's param.json; never
   * an existing path nor one in `taken` (outputs of running jobs): `-2`, `-3`, ... instead.
   */
  generatedOutput: (source: string, format: Format, dir: string, taken: string[]) =>
    invoke<string>("generated_output", { source, format, dir, taken }),
  startJob: (request: ConvertRequest) => invoke<JobId>("start_job", { request }),
  cancelJob: (id: JobId) => invoke<void>("cancel_job", { id }),
  /** Leftover `.part` files in these folders (deduplicated, missing ones skipped). */
  staleParts: (dirs: string[]) => invoke<string[]>("stale_parts", { dirs }),
  /** Show a finished job's output in Finder, Explorer or its folder (Rust looks the path up by
   * job id). The http build does nothing here: the job row shows the path instead. */
  reveal: (id: JobId) => invoke<void>("reveal", { id }),
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
