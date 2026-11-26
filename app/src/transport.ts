// What api.ts runs on, picked at build time by the `forge-transport` alias (vite.config.ts): transport-tauri.tsx or transport-http.tsx. Types only.

import type { JobId, PackageScan, SpeedTest } from "./api";

export type Unlisten = () => void;

/** `jobs://restore` comes from the http poller only. */
export type EventName =
  | "job://progress"
  | "job://stages"
  | "job://log"
  | "job://done"
  | "close-requested"
  | "jobs://restore";

export interface PickOptions {
  directory: boolean;
  title: string;
  /** File mode: the extensions offered (without the dot); missing: every file. */
  extensions?: string[];
  folderToo?: boolean;
  /** Else where the last browse ended. */
  start?: string;
  /** http build: the saved NAS devices are roots too. */
  nas?: boolean;
  /** While a paired Forge or saved NAS is online, a tab per online device (This device first). */
  devices?: boolean;
  /** With `devices`: the paired Forges only (Edit's Add: core adds no file from a NAS). */
  peersOnly?: boolean;
  /** With `devices`, no `start`: open on this device. */
  local?: boolean;
  nasExtensions?: string[];
}

/** The app gives its host path; the http build the browser's handles (nothing uploaded until `stage`). */
export interface DropItem {
  name: string;
  path?: string;
  file?: File;
  /** http build: to walk a folder (`webkitGetAsEntry`; absent when the browser has none). */
  entry?: FileSystemEntry;
}

export interface DropEvent {
  x: number;
  y: number;
  items: DropItem[];
}

/** As core would add it (`host_tree`): `/`-separated paths relative to it, junk left out; `truncated`: stopped at its limit. */
export interface HostTree {
  kind: "file" | "dir";
  size: number;
  files: { path: string; size: number }[];
  empty_dirs: string[];
  truncated: boolean;
}

export interface Transport {
  web: boolean;
  /** Rejects with the command's error string. */
  call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: EventName, f: (payload: T) => void): Promise<Unlisten>;
  pick(o: PickOptions): Promise<string | null>;
  pickFiles(o: PickOptions): Promise<string[] | null>;
  hostTree(path: string): Promise<HostTree>;
  useOffline(): boolean;
  address(): string | null;
  debug(): boolean;
  /** The app is always "host"; the http build asks the server. */
  platform(): Promise<"host" | "ps5">;
  /** Null in the app or on a host server. */
  notifications(): Promise<boolean | null>;
  setNotifications(on: boolean): Promise<boolean | null>;
  /** Null when this build gets no updates. */
  updateTarget(): Promise<{ version: string; asset: string } | null>;
  /** PS5: elfldr loads it and the page reloads once the new server answers; the app opens the download in the browser. */
  startUpdate(version: string): Promise<void>;
  /** `over`: the pointer (CSS pixels) while dragged over, null when they leave or drop. */
  onDrop(f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten;
  /** http build: resolves with the staged path (an `add`'s `from`); rejects with the server's error or on `signal`. The app rejects. */
  stage(
    source: string,
    gamePath: string,
    file: File,
    onProgress?: (sent: number, total: number) => void,
    signal?: AbortSignal,
  ): Promise<string>;
  discardStaged(source: string): Promise<void>;
  /** Tauri `connect_<cmd>` or `/api/connect/<cmd>` (`state` a GET), snake_case keys both ways; `nas_save` resolves with its bare id. */
  connect<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  /** http build: Connect's phone upload; `replace`: a file at that name is replaced. The app rejects. */
  connectUpload(
    dir: string,
    name: string,
    file: File,
    onProgress?: (sent: number, total: number) => void,
    signal?: AbortSignal,
    replace?: boolean,
  ): Promise<string>;
  connectDownloadUrl(path: string): string | null;
  packageScan(): Promise<PackageScan>;
  /** Tauri `connect_install {keyId, path}` (camelCase), `POST /api/connect/install {key_id, path}`. */
  connectInstall(keyId: string, path: string): Promise<JobId>;
  /** 128 MiB each way over FLINK's lanes, nothing on disk; Tauri takes `{keyId, run}`, the server `{key_id, run}`. */
  connectSpeedTest(keyId: string, run: string): Promise<SpeedTest>;
  /** A run not started yet ends `cancelled` as it starts. */
  connectSpeedTestCancel(keyId: string, run: string): Promise<void>;
}
