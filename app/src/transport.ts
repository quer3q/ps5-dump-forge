// What api.ts runs on. Two implementations, picked at build time by the `forge-transport`
// alias (vite.config.ts): transport-tauri.tsx (the app, `npm run build`) and
// transport-http.tsx (the PS5 web UI, `npm run build:http`). Types only here.

export type Unlisten = () => void;

/** The events api.ts listens to. `jobs://restore` comes from the http poller only. */
export type EventName = "job://progress" | "job://log" | "job://done" | "close-requested" | "jobs://restore";

export interface PickOptions {
  /** A folder, else a file. */
  directory: boolean;
  title: string;
  /** File mode: the extensions offered (without the dot). */
  extensions?: string[];
  /** File mode: what the extensions are, for the dialog's filter menu. */
  filterName?: string;
}

export interface Transport {
  /** The http build (a browser talking to `ps5-dump-forge serve`). */
  web: boolean;
  /** A Tauri command, or `POST /api/<cmd>`; rejects with the command's error string. */
  call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: EventName, f: (payload: T) => void): Promise<Unlisten>;
  /** A folder or file path, or null when cancelled. */
  pick(o: PickOptions): Promise<string | null>;
  /** The server can't be reached right now (http only). */
  useOffline(): boolean;
}
