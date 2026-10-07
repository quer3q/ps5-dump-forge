// Typed IPC for app/src-tauri/src/main.rs. Types mirror crates/ps5-dump-forge-core/src/lib.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Format = "folder" | "exfat" | "ffpkg" | "ffpfs" | "ffpfsc" | "pkg";
/** What a source can be: every target format is also read. */
export type Kind = Format;
/** The image inside a `.ffpfsc` container. */
export type InnerFormat = "exfat" | "ffpkg" | "ffpfs";
export type JobId = number;

export interface ConvertRequest {
  source: string;
  format: Format;
  output: string;
  /** `null`: every core. */
  compression_threads: number | null;
  /** The image inside a `.ffpfsc`; `null` (or any other target): `.exfat`. */
  inner: InnerFormat | null;
}

export interface JobReport {
  output: string;
  bytes: number;
  files: number;
  checks: string[];
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
  /** Backport files (fakelib/*, plus ampr_emu.index next to them); empty when none. */
  backport: string[];
  /** For a backport: the lowest firmware its executables allow, e.g. "4.50". */
  backport_firmware: string | null;
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
   * job id). */
  reveal: (id: JobId) => invoke<void>("reveal", { id }),
  /** Cancel every job, wait for cleanup, exit. */
  quitApp: () => invoke<void>("quit_app"),
};

export const events = {
  progress: (f: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("job://progress", (e) => f(e.payload)),
  log: (f: (e: LogEvent) => void): Promise<UnlistenFn> =>
    listen<LogEvent>("job://log", (e) => f(e.payload)),
  done: (f: (e: DoneEvent) => void): Promise<UnlistenFn> =>
    listen<DoneEvent>("job://done", (e) => f(e.payload)),
  /** A close/quit was held back because jobs are running. */
  closeRequested: (f: () => void): Promise<UnlistenFn> => listen("close-requested", () => f()),
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
