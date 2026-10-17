// The http build's client for `ps5-dump-forge serve` (contract: ps5/README.md, "Web server"): plain
// JSON over HTTP, and one poller over `GET /api/jobs` that turns snapshots into the app's job
// events. Type imports only, so a plain `node` can drive it (scripts/check-poller.mjs,
// scripts/smoke.mjs).

import type { DoneEvent, JobsRestore, LogEvent, ProgressEvent, SnapshotJob, StagesEvent } from "./api";
import type { EventName, Unlisten } from "./transport";
import type { UpdateTimeout } from "./update";

export interface Status {
  /** The last request failed at the network level; polling goes on with backoff. */
  offline: boolean;
}

export interface JobsSnapshot {
  instance: string;
  jobs: SnapshotJob[];
}

const FAST_MS = 1000; // while a job is unfinished
const SLOW_MS = 5000;
const MAX_BACKOFF_MS = 10000; // back within seconds once the payload answers again
const POLL_TIMEOUT_MS = 15000;

// ---------- the client's own messages ----------

/** What the client itself says, by code. The page installs the words (`setClientText`, from
 * main-http.tsx, in the UI's language); until then the code is the text, which is what a plain
 * `node` check sees. */
export type ClientText = "unreachable" | "noPlatform" | "quitUnreachable" | "stillAnswering" | "gone";

let text: (code: ClientText) => string = (code) => code;

export function setClientText(f: (code: ClientText) => string): void {
  text = f;
}

/** The result of a job the page saw running, once the server's list no longer has it (its
 * 32-job history dropped it between two polls, so its real result was never seen). */
export const GONE: ClientText = "gone";

// ---------- status, for the UI (useSyncExternalStore) ----------

let status: Status = { offline: false };
const watchers = new Set<() => void>();

function setOffline(offline: boolean) {
  if (offline === status.offline) return;
  status = { offline };
  watchers.forEach((w) => w());
}

export function getStatus(): Status {
  return status;
}

export function watchStatus(w: () => void): Unlisten {
  watchers.add(w);
  return () => watchers.delete(w);
}

// ---------- requests ----------

/** A request that reached the server: its status and parsed JSON body. A network failure
 * rejects instead (fetch's TypeError, or an abort). */
async function request(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
  timeoutMs?: number,
): Promise<{ status: number; data: unknown }> {
  const abort = timeoutMs ? new AbortController() : null;
  const timer = abort ? setTimeout(() => abort.abort(), timeoutMs) : undefined;
  try {
    const res = await fetch(path, {
      method,
      headers: method === "POST" ? { "Content-Type": "application/json" } : {},
      body: method === "POST" ? JSON.stringify(body ?? {}) : undefined,
      cache: "no-store",
      signal: abort?.signal,
    });
    let data: unknown = null;
    try {
      data = await res.json();
    } catch {
      data = { error: `${res.status} ${res.statusText}`.trim() };
    }
    return { status: res.status, data };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

function errorOf(data: unknown, status: number): string {
  const e = (data as { error?: unknown } | null)?.error;
  return typeof e === "string" ? e : `HTTP ${status}`;
}

/** An api.ts command. Rejects with the server's `error` string (what errorText shows), as the
 * Tauri commands do. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // The page shows the output path itself; nothing on the console opens a file manager.
  if (cmd === "reveal") return undefined as T;
  if (cmd === "quit_app") return quit() as Promise<T>;
  let res;
  try {
    res = await request("POST", `/api/${cmd}`, args ?? {});
  } catch {
    setOffline(true);
    kick();
    throw text("unreachable");
  }
  setOffline(false);
  if (res.status !== 200) throw errorOf(res.data, res.status);
  // A job just started or was cancelled: show it without waiting for the next tick.
  if (cmd === "start_job" || cmd === "cancel_job") kick();
  return res.data as T;
}

/** The upload route for `source` and `gamePath`. Each value is percent-encoded with
 * `encodeURIComponent` (a space is `%20`, `+` is `%2B`): the server decodes `%XX` only and keeps
 * `+` as it is, so `URLSearchParams`'s `+` for a space would name another path. */
export function uploadUrl(source: string, gamePath: string): string {
  return `/api/edit_upload?source=${encodeURIComponent(source)}&path=${encodeURIComponent(gamePath)}`;
}

/** `POST /api/edit_upload?source=&path=` with `file` as the raw body (XHR, for upload progress).
 * Resolves with the staged file's path; rejects as `call` does, or with "aborted" on `signal`. */
export function upload(
  source: string,
  gamePath: string,
  file: File,
  onProgress?: (sent: number, total: number) => void,
  signal?: AbortSignal,
): Promise<string> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject("aborted");
    const x = new XMLHttpRequest();
    x.open("POST", uploadUrl(source, gamePath));
    x.setRequestHeader("Content-Type", "application/octet-stream");
    if (onProgress) x.upload.onprogress = (e) => onProgress(e.loaded, e.lengthComputable ? e.total : file.size);
    const onAbort = () => x.abort();
    signal?.addEventListener("abort", onAbort, { once: true });
    const end = () => signal?.removeEventListener("abort", onAbort);
    x.onload = () => {
      end();
      setOffline(false);
      let data: unknown = null;
      try {
        data = JSON.parse(x.responseText);
      } catch {
        data = { error: `${x.status} ${x.statusText}`.trim() };
      }
      const path = (data as { path?: unknown } | null)?.path;
      if (x.status === 200 && typeof path === "string") resolve(path);
      else reject(errorOf(data, x.status));
    };
    x.onerror = () => {
      end();
      setOffline(true);
      kick();
      reject(text("unreachable"));
    };
    x.onabort = () => {
      end();
      reject("aborted");
    };
    x.send(file);
  });
}

let address: string | null = null;
let platform: "host" | "ps5" | null = null;
let version: string | null = null;

/** The server's `host:port` from its session's `url` (the console's own address, never the
 * viewer's `location`), once `serverSession` read it; null before, or when it has none. */
export function serverAddress(): string | null {
  return address;
}

/** `GET /api/session`: its path separator (`"/"` or `"\\"`) and address (`serverAddress`),
 * null where it doesn't answer or says nothing usable. */
export async function serverSession(): Promise<{ separator: string | null; address: string | null }> {
  try {
    const res = await request("GET", "/api/session", undefined, 3000);
    const s = res.status === 200 ? (res.data as { separator?: unknown; url?: unknown } | null) : null;
    const sep = s?.separator;
    // `http://<ip>:<port>`; a server that couldn't tell its IP says so in words: none then.
    const host = typeof s?.url === "string" ? /^https?:\/\/([\w.:[\]-]+)\/?$/.exec(s.url)?.[1] : undefined;
    address = host ?? null;
    // Only the server's own word counts (never the browser's OS or URL); a valid one is kept.
    const plat = (s as { platform?: unknown } | null)?.platform;
    if (plat === "host" || plat === "ps5") platform = plat;
    const ver = (s as { version?: unknown } | null)?.version;
    if (typeof ver === "string") version = ver;
    return { separator: sep === "/" || sep === "\\" ? sep : null, address };
  } catch {
    return { separator: null, address: null };
  }
}

/** The server's platform from its session, cached once valid; an unknown one retries the
 * session once, then rejects (so no PS5-only choice is ever shown by accident). */
export async function serverPlatform(): Promise<"host" | "ps5"> {
  if (platform === null) await serverSession();
  if (platform === null) throw text("noPlatform");
  return platform;
}

/** The update the page may offer: the PS5's payload (`ps5.elf`) at the session's version; null
 * for `serve` on a computer or a session without a version. Rejects as `serverPlatform`. */
export async function serverUpdateTarget(): Promise<{ version: string; asset: string } | null> {
  const p = await serverPlatform();
  if (p !== "ps5" || !version) return null;
  // Installable only through elfldr: no toast while it doesn't answer (or the server predates
  // the route). Never marks the page offline.
  try {
    const res = await request("GET", "/api/elfldr", undefined, 3000);
    return res.status === 200 && (res.data as { up?: unknown } | null)?.up === true
      ? { version, asset: "ps5.elf" }
      : null;
  } catch {
    return null;
  }
}

/** How long `selfUpdate` waits in all, from the request: download, start and takeover. */
export const UPDATE_WAIT_MS = 60000;
const UPDATE_TIMEOUT: UpdateTimeout = "update-timeout";

/** The session's `instance`, or null when it doesn't answer with one. */
async function sessionInstance(ms: number): Promise<string | null> {
  try {
    const res = await request("GET", "/api/session", undefined, ms);
    const i = (res.data as { instance?: unknown } | null)?.instance;
    return res.status === 200 && typeof i === "string" ? i : null;
  } catch {
    return null;
  }
}

/** `POST /api/self_update`: elfldr downloads and starts `v`, which takes over from this server.
 * Resolves once a new `instance` answers the session; rejects with the server's error, or
 * `UPDATE_TIMEOUT` once `UPDATE_WAIT_MS` has passed since the request. A network error during
 * the call or the wait is expected (the old server exits): it waits on. */
export async function selfUpdate(v: string): Promise<void> {
  const end = Date.now() + UPDATE_WAIT_MS;
  const left = () => end - Date.now();
  const before = await sessionInstance(5000);
  if (before === null) throw text("unreachable");
  // Never ask once the minute is over: the update could start after the toast gave up.
  if (left() <= 0) throw UPDATE_TIMEOUT;
  // The server holds the reply while elfldr downloads (55 s at most there).
  const res = await request("POST", "/api/self_update", { version: v }, Math.max(left(), 1)).catch(() => null);
  if (res && res.status !== 200) throw errorOf(res.data, res.status);
  for (;;) {
    if (left() <= 0) throw UPDATE_TIMEOUT;
    const now = await sessionInstance(Math.min(3000, left()));
    if (now !== null && now !== before && left() > 0) return;
    if (left() <= 0) throw UPDATE_TIMEOUT;
    await new Promise((r) => setTimeout(r, Math.min(1000, left())));
  }
}

let started = false;

/** Start polling, once, at page load. */
export function start(): void {
  if (started) return;
  started = true;
  kick();
}

// ---------- events ----------

const listeners = new Map<EventName, Set<(payload: unknown) => void>>();

function emit(event: EventName, payload: unknown) {
  listeners.get(event)?.forEach((f) => f(payload));
}

export function listen<T>(event: EventName, f: (payload: T) => void): Promise<Unlisten> {
  // The browser window closes on its own; jobs keep running on the server.
  if (event === "close-requested") return Promise.resolve(() => {});
  let set = listeners.get(event);
  if (!set) listeners.set(event, (set = new Set()));
  const g = f as (payload: unknown) => void;
  set.add(g);
  // A new job list subscribed (the app mounted): rebuild it whole.
  if (event === "jobs://restore") {
    seen = null;
    kick();
  }
  return Promise.resolve(() => {
    set.delete(g);
  });
}

// ---------- the poller ----------

interface Seen {
  progress: string;
  /** The job's stage list was known (sent once, when it first is). */
  stages: boolean;
  logTotal: number;
  done: boolean;
  request: boolean;
}

/** What each job last looked like; null: the next snapshot rebuilds the whole list. */
let seen: Map<number, Seen> | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
let inFlight = false;
let again = false;
let backoff = 0;
let stopping = false;

function remember(j: SnapshotJob): Seen {
  return {
    progress: JSON.stringify(j.progress),
    stages: j.stages?.length > 0,
    logTotal: j.log_total,
    done: j.done !== null,
    request: j.request !== null,
  };
}

/** Feed one snapshot into the app's events: the stage list once it is known, progress when it
 * changed, only the log lines it
 * hasn't seen (by `log_total`), done once. A job it never saw is restored whole; so is the
 * whole list after `seen` was reset. A job missing from the list is dropped from `seen`; one
 * it never saw finish gets a done event with `GONE`. */
function diff(snap: JobsSnapshot): void {
  if (seen === null) {
    seen = new Map(snap.jobs.map((j) => [j.id, remember(j)]));
    emit("jobs://restore", { replace: true, jobs: snap.jobs } satisfies JobsRestore);
    return;
  }
  const fresh: SnapshotJob[] = [];
  for (const j of snap.jobs) {
    const was = seen.get(j.id);
    seen.set(j.id, remember(j));
    // New to the page (another browser's Build, or ours before start_job answered), or its
    // request only just merged in: restored whole.
    if (!was || (!was.request && j.request)) {
      fresh.push(j);
      continue;
    }
    if (j.stages?.length > 0 && !was.stages)
      emit("job://stages", { kind: "stages", job: j.id, stages: j.stages } satisfies StagesEvent);
    if (j.progress && JSON.stringify(j.progress) !== was.progress) {
      const { stage, done, total } = j.progress;
      emit("job://progress", { kind: "progress", job: j.id, stage, done, total } satisfies ProgressEvent);
    }
    const n = Math.min(j.log_total - was.logTotal, j.log.length);
    for (const line of n > 0 ? j.log.slice(j.log.length - n) : [])
      emit("job://log", { kind: "log", job: j.id, line } satisfies LogEvent);
    if (j.done && !was.done)
      emit("job://done", { kind: "done", job: j.id, result: j.done.result } satisfies DoneEvent);
  }
  if (fresh.length > 0) emit("jobs://restore", { replace: false, jobs: fresh } satisfies JobsRestore);
  // Gone from the server's list: forget it, and end it if the page never saw it finish.
  const listed = new Set(snap.jobs.map((j) => j.id));
  for (const [id, was] of [...seen]) {
    if (listed.has(id)) continue;
    seen.delete(id);
    if (!was.done) emit("job://done", { kind: "done", job: id, result: { Err: text(GONE) } } satisfies DoneEvent);
  }
}

let instance: string | null = null;

/** One `GET /api/jobs` reply. A new `instance` (the payload restarted) rebuilds the list. */
export function applySnapshot(snap: JobsSnapshot): void {
  if (snap.instance !== instance) seen = null;
  instance = snap.instance;
  diff(snap);
}

function schedule(ms: number) {
  if (timer !== undefined) clearTimeout(timer);
  timer = setTimeout(poll, ms);
}

/** Poll now (or right after the poll in flight). */
function kick() {
  if (inFlight) again = true;
  else schedule(0);
}

async function poll(): Promise<void> {
  timer = undefined;
  if (!started || stopping) return;
  inFlight = true;
  let next = SLOW_MS;
  try {
    const res = await request("GET", "/api/jobs", undefined, POLL_TIMEOUT_MS);
    setOffline(false);
    backoff = 0;
    if (res.status === 200) {
      const snap = res.data as JobsSnapshot;
      applySnapshot(snap);
      next = snap.jobs.some((j) => j.done === null) ? FAST_MS : SLOW_MS;
    }
  } catch {
    setOffline(true);
    backoff = Math.min(backoff ? backoff * 2 : 2000, MAX_BACKOFF_MS);
    next = backoff;
  } finally {
    inFlight = false;
  }
  if (stopping) return;
  schedule(again ? 0 : next);
  again = false;
}

// ---------- quit ----------

/** How long quit waits for the server to go away (it cancels jobs and cleans up first). */
export const QUIT_WAIT_MS = 120000;
export const STILL_ANSWERING: ClientText = "stillAnswering";

/** `POST /api/quit`, then wait until the server is gone. Polling pauses meanwhile: an
 * unreachable server is the expected end, not an offline notice. Rejects with
 * `STILL_ANSWERING` (and polls again) if it is still up at the deadline. */
async function quit(): Promise<void> {
  const res = await request("POST", "/api/quit", {}).catch(() => null);
  if (!res) throw text("quitUnreachable");
  if (res.status !== 200) throw errorOf(res.data, res.status);
  stopping = true;
  if (timer !== undefined) clearTimeout(timer);
  const end = Date.now() + QUIT_WAIT_MS;
  for (;;) {
    await new Promise((r) => setTimeout(r, 1000));
    const left = end - Date.now();
    if (left <= 0) break;
    try {
      await request("GET", "/api/session", undefined, Math.min(5000, left));
    } catch {
      return;
    }
  }
  stopping = false;
  kick();
  throw text(STILL_ANSWERING);
}
