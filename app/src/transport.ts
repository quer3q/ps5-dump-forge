// What api.ts runs on. Two implementations, picked at build time by the `forge-transport`
// alias (vite.config.ts): transport-tauri.tsx (the app, `npm run build`) and
// transport-http.tsx (the PS5 web UI, `npm run build:http`). Types only here.

export type Unlisten = () => void;

/** The events api.ts listens to. `jobs://restore` comes from the http poller only. */
export type EventName =
  | "job://progress"
  | "job://stages"
  | "job://log"
  | "job://done"
  | "close-requested"
  | "jobs://restore";

export interface PickOptions {
  /** A folder, else a file. */
  directory: boolean;
  title: string;
  /** File mode: the extensions offered (without the dot); missing: every file. */
  extensions?: string[];
  /** File mode: what the extensions are, for the dialog's filter menu. */
  filterName?: string;
  /** File mode, http build only: the current folder can be chosen too ("Choose this folder").
   * The native dialog can't pick both kinds at once (rfd: files or folders), so the app asks
   * which kind first. */
  folderToo?: boolean;
  /** The folder to open at (else where the last browse ended). */
  start?: string;
}

/** One thing dropped on the window: a file or a folder. The app gives its host path; the http
 * build gives the browser's handles (nothing is uploaded until `stage`). */
export interface DropItem {
  name: string;
  /** The app: the dropped file's or folder's path on this computer. */
  path?: string;
  /** http build: the dropped file (absent for a folder). */
  file?: File;
  /** http build: the dropped entry, to walk a folder (`webkitGetAsEntry`; absent when the browser
   * has none). */
  entry?: FileSystemEntry;
}

/** A drop, at CSS pixels in the viewport (for `document.elementFromPoint`). */
export interface DropEvent {
  x: number;
  y: number;
  items: DropItem[];
}

/** A file or folder on the machine that runs the job, as core would add it (`host_tree`): a
 * folder's files and empty folders, paths relative to it and `/`-separated, junk left out;
 * for a file, its size and empty lists. `truncated`: the listing stopped at its limit. */
export interface HostTree {
  kind: "file" | "dir";
  size: number;
  files: { path: string; size: number }[];
  empty_dirs: string[];
  truncated: boolean;
}

export interface Transport {
  /** The http build (a browser talking to `ps5-dump-forge serve`). */
  web: boolean;
  /** A Tauri command, or `POST /api/<cmd>`; rejects with the command's error string. */
  call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: EventName, f: (payload: T) => void): Promise<Unlisten>;
  /** A folder or file path, or null when cancelled. */
  pick(o: PickOptions): Promise<string | null>;
  /** Several files at once (file mode), or null when cancelled. */
  pickFiles(o: PickOptions): Promise<string[] | null>;
  /** What a path holds (Tauri `host_tree {path}`, `POST /api/host_tree {path}`). */
  hostTree(path: string): Promise<HostTree>;
  /** The server can't be reached right now (http only). */
  useOffline(): boolean;
  /** The server's `host:port` as its session reports it (http only; the app: null). */
  address(): string | null;
  /** Where the backend runs: the app is always "host"; the http build asks the server. */
  platform(): Promise<"host" | "ps5">;
  /** Patch a game folder in place for LZ4 tracing (api.ts `Lz4Patch`). Its own function: the
   * Tauri command takes `path`, the route `source`. */
  lz4Patch<T>(source: string): Promise<T>;
  /** Put Forge's release runtime back in a game folder (same argument names as lz4Patch). */
  lz4Unpatch<T>(source: string): Promise<T>;
  /** The running version and its release file's suffix (after `ps5-dump-forge-<v>-`), or null
   * when this build gets no updates (the http build on a computer). */
  updateTarget(): Promise<{ version: string; asset: string } | null>;
  /** Install `version`: the PS5 has elfldr load it, then the page reloads once the new server
   * answers (rejects with the server's error or update.ts's `UPDATE_TIMEOUT`); the app opens
   * the release archive's download in the system browser. */
  startUpdate(version: string): Promise<void>;
  /** Files or folders dropped on the window. `over`: the pointer while files are dragged over it
   * (CSS pixels), null when they leave or are dropped. */
  onDrop(f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten;
  /** http build: upload `file` for game path `gamePath` of `source` to the server's staging folder
   * beside it; resolves with the staged file's path (an `Edit` `add`'s `from`). `onProgress`: bytes
   * sent of the total. Rejects with the server's error, or when `signal` aborts. The app rejects
   * (its drops are host paths already). */
  stage(
    source: string,
    gamePath: string,
    file: File,
    onProgress?: (sent: number, total: number) => void,
    signal?: AbortSignal,
  ): Promise<string>;
  /** http build: remove `source`'s staging folder (nothing if there is none). The app: nothing. */
  discardStaged(source: string): Promise<void>;
}
