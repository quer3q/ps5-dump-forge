// Typed IPC for app/src-tauri/src/main.rs, or the same commands over HTTP
// (crates/ps5-dump-forge-server) in the http build. Types mirror
// crates/ps5-dump-forge-core/src/lib.rs.

import { transport } from "forge-transport";
import { msg, num } from "./i18n";
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
/** What a job does with LZ4 asset packs, besides the target: install the trace runtime, write
 * packs back as plain files, or put Forge's release runtime back. */
/** `pack`: LZ4 packs into `format` (folder, exfat, ffpkg, ffpfs, ffpfsc); the `lz4` format is
 * the same as `folder` with `pack`. */
export type Lz4Mode = "trace" | "unpack" | "unpatch" | "pack";
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
  /** The ampr_emu version the game's own runtime names itself with, when it does. */
  runtime_version?: string | null;
  /** What a plain conversion does to the runtime: "install" (none there), "replace" (Forge's
   * trace runtime, an older ampr_emu or one naming no version: the bundled one goes in) or
   * "keep" (Forge's release runtime, or ampr_emu at the bundled version or newer:
   * runtime_version names it). Null for a non-AMPR title; missing from an older backend. */
  runtime_action?: "install" | "replace" | "keep" | null;
  /** The same for the LZ4 tab's Pack (an unpacked source) or Unpack (a packed one): Pack also
   * replaces a not-older runtime that can't read packs. */
  pack_runtime_action?: "install" | "replace" | "keep" | null;
  /** ampr_emu.index against the files a plain conversion writes: "stale", "damaged" and
   * "missing" are written anew by any conversion. Null when no conversion touches it (packed:
   * kept as it is; a non-AMPR title; an archive's index too large to read); missing from an
   * older backend. */
  index?: "ok" | "stale" | "damaged" | "missing" | null;
  /** Size of the trace journal (ampr_commands.bin), when present. */
  journal_bytes: number | null;
  /** With the journal and its ampr_emu.index both present: the name Get traces' zip takes. */
  traces_zip: string | null;
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

/** Where the backend runs (the server's own answer in the http build); rejects when unknown. */
export const runtimePlatform: () => Promise<RuntimePlatform> = transport.platform;

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
  /** Patch a game folder in place for LZ4 tracing: the trace runtime in fakelib/, a fresh
   * index, stale traces removed. Not a job; core refuses it while a job runs. */
  lz4Patch: (source: string) => transport.lz4Patch<Lz4Patch>(source),
  /** Put Forge's release runtime back in a game folder in place: journal and logs removed, a
   * fresh index. Not a job; core refuses it while a job runs. */
  lz4Unpatch: (source: string) => transport.lz4Unpatch<Lz4Patch>(source),
  /** Delete a file or folder for good (the page asks first). Refused for a link, a drive or
   * browse root or a folder holding one, and anything an unfinished job reads or writes. */
  deletePath: (path: string) => invoke<null>("delete_path", { path }).then(() => undefined),
  /** Discover game dumps under the chosen folder, or the PS5's roots (read-only, bounded). */
  libraryScan: (root?: string) => invoke<LibraryScan>("library_scan", root ? { root } : {}),
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
  /** A close/quit was held back because jobs are running. Never in the http build. */
  closeRequested: (f: () => void): Promise<UnlistenFn> => listen("close-requested", () => f()),
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
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
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
