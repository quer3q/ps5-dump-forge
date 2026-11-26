// The http build's client for `ps5-dump-forge serve` (ps5/README.md): JSON over HTTP and one poller over `GET /api/jobs`. Type imports only, so plain `node` can drive it (scripts/check-poller.mjs).

import type { DoneEvent, JobsRestore, LogEvent, ProgressEvent, SnapshotJob, StagesEvent } from "./api";
import type { EventName, Unlisten } from "./transport";
import type { UpdateTimeout } from "./update";

export interface Status {
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

/** What the client itself says, by code; the page installs the words (`setClientText`), until then the code is the text. */
export type ClientText = "unreachable" | "noPlatform" | "quitUnreachable" | "stillAnswering" | "gone" | "uploadIncomplete";

let text: (code: ClientText) => string = (code) => code;

export function setClientText(f: (code: ClientText) => string): void {
  text = f;
}

/** A job the page saw running that the server's 32-job history dropped between two polls. */
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

/** A network failure rejects instead (fetch's TypeError, or an abort). */
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

/** api.rs `SHARED_READS`: asked again after a network error (`READ_TRIES`) or a 202, within `READ_RETRY_MS`. */
const SHARED_READS = new Set([
  "inspect",
  "default_output",
  "generated_output",
  "package_scan",
  "host_tree",
  "pack_estimate",
  "edit_check",
]);
const READ_TRIES = 10;
const READ_RETRY_MS = 30 * 60 * 1000;

/** Rejects with the server's `error` string, as the Tauri commands do. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (cmd === "reveal") return undefined as T;
  if (cmd === "quit_app") return quit() as Promise<T>;
  if (cmd === "bundled_ng_version") return serverNgVersion() as Promise<T>;
  let res;
  const end = Date.now() + READ_RETRY_MS;
  for (let asked = 0, failed = 0; ; asked++) {
    try {
      res = await request("POST", `/api/${cmd}${asked > 0 ? "?retry=1" : ""}`, args ?? {});
      if (res.status === 202 && SHARED_READS.has(cmd) && Date.now() < end) continue;
      break;
    } catch {
      // A long read can outlast the browser's request: ask again while the server answers; the retry joins that read or takes its reply (issue #74).
      failed++;
      const again = SHARED_READS.has(cmd) && failed < READ_TRIES && Date.now() < end && (await sessionInstance(5000)) !== null;
      if (again) continue;
      setOffline(true);
      kick();
      throw text("unreachable");
    }
  }
  setOffline(false);
  if (res.status !== 200) throw errorOf(res.data, res.status);
  if (cmd === "start_job" || cmd === "cancel_job") kick();
  return res.data as T;
}

/** `GET /api/connect/state`, else `POST /api/connect/<cmd>`; `nas_save`'s `{id}` as the bare id, as Tauri's. */
export async function connectCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  let res;
  try {
    res = cmd === "state" ? await request("GET", "/api/connect/state", undefined, 5000) : await request("POST", `/api/connect/${cmd}`, args ?? {});
  } catch {
    throw text("unreachable");
  }
  if (res.status !== 200) throw errorOf(res.data, res.status);
  const id = (res.data as { id?: unknown } | null)?.id;
  const bare = cmd === "nas_save" && typeof id === "string";
  return (bare ? id : res.data) as T;
}

/** `encodeURIComponent`, not `URLSearchParams`: the server decodes `%XX` only and keeps `+` as it is. */
export function uploadUrl(source: string, gamePath: string): string {
  return `/api/edit_upload?source=${encodeURIComponent(source)}&path=${encodeURIComponent(gamePath)}`;
}

/** XHR, for upload progress; rejects as `call` does, or with "aborted" on `signal`. */
export function upload(
  source: string,
  gamePath: string,
  file: File,
  onProgress?: (sent: number, total: number) => void,
  signal?: AbortSignal,
): Promise<string> {
  return postFile(uploadUrl(source, gamePath), file, onProgress, signal).then(pathOf);
}

function pathOf(data: Record<string, unknown>): string {
  if (typeof data.path === "string") return data.path;
  throw errorOf(data, 200);
}

/** 4 chunks in flight in a browser (about 6 requests per host; the page's polls need room). */
const CHUNK = 8 << 20;
const PARALLEL = 4;

/** Chunks put together by offset on the server; a failure or abort cancels the rest and drops its part (`cancel=1`). */
export async function connectUpload(
  dir: string,
  name: string,
  file: File,
  onProgress?: (sent: number, total: number) => void,
  signal?: AbortSignal,
  replace = false,
): Promise<string> {
  if (signal?.aborted) throw "aborted";
  const id = Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, "0")).join("");
  const base = `/api/connect/upload?dir=${encodeURIComponent(dir)}&name=${encodeURIComponent(name)}&id=${id}${replace ? "&replace=1" : ""}`;
  const offsets: number[] = [];
  for (let at = 0; at < file.size || offsets.length === 0; at += CHUNK) offsets.push(at);
  const sent = new Map<number, number>();
  const report = () => onProgress?.([...sent.values()].reduce((a, b) => a + b, 0), file.size);
  const stop = new AbortController();
  const abort = () => stop.abort();
  signal?.addEventListener("abort", abort, { once: true });
  let path: string | undefined;
  let next = 0;
  const lane = async () => {
    while (next < offsets.length) {
      const at = offsets[next++];
      const piece = file.slice(at, Math.min(at + CHUNK, file.size));
      const url = `${base}&offset=${at}&size=${file.size}`;
      const data = await postFile(url, piece, (loaded) => (sent.set(at, loaded), report()), stop.signal);
      sent.set(at, piece.size);
      report();
      if (data.done === true) path = pathOf(data);
    }
  };
  try {
    await Promise.all(Array.from({ length: Math.min(PARALLEL, offsets.length) }, lane));
  } catch (e) {
    stop.abort();
    void postFile(`${base}&cancel=1`, new Blob([])).catch(() => {});
    throw e;
  } finally {
    signal?.removeEventListener("abort", abort);
  }
  if (path === undefined) throw text("uploadIncomplete");
  return path;
}

export function connectDownloadUrl(path: string): string {
  return `/api/connect/download?path=${encodeURIComponent(path)}`;
}

function postFile(
  url: string,
  file: Blob,
  onProgress?: (sent: number, total: number) => void,
  signal?: AbortSignal,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject("aborted");
    const x = new XMLHttpRequest();
    x.open("POST", url);
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
      if (x.status === 200 && data && typeof data === "object") resolve(data as Record<string, unknown>);
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
let notifications: boolean | null = null;
let version: string | null = null;
let ngVersion: string | null = null;
let cliAsset: string | null = null;
let debugBuild = false;

/** The server's own address from its session's `url`, never the viewer's `location`. */
export function serverAddress(): string | null {
  return address;
}

export function serverDebug(): boolean {
  return debugBuild;
}

/** Its path separator and address; null where it doesn't answer or says nothing usable. */
export async function serverSession(): Promise<{ separator: string | null; address: string | null }> {
  try {
    const res = await request("GET", "/api/session", undefined, 3000);
    const s = res.status === 200 ? (res.data as { separator?: unknown; url?: unknown } | null) : null;
    const sep = s?.separator;
    const host = typeof s?.url === "string" ? /^https?:\/\/([\w.:[\]-]+)\/?$/.exec(s.url)?.[1] : undefined;
    address = host ?? null;
    // Only the server's own word counts (never the browser's OS or URL).
    const plat = (s as { platform?: unknown } | null)?.platform;
    if (plat === "host" || plat === "ps5") platform = plat;
    const on = (s as { notifications?: unknown } | null)?.notifications;
    if (typeof on === "boolean") notifications = on;
    const ver = (s as { version?: unknown } | null)?.version;
    if (typeof ver === "string") version = ver;
    const ng = (s as { bundled_ng_version?: unknown } | null)?.bundled_ng_version;
    if (typeof ng === "string") ngVersion = ng;
    const cli = (s as { cli_asset?: unknown } | null)?.cli_asset;
    if (typeof cli === "string") cliAsset = cli;
    debugBuild = (s as { debug?: unknown } | null)?.debug === true;
    return { separator: sep === "/" || sep === "\\" ? sep : null, address };
  } catch {
    return { separator: null, address: null };
  }
}

async function serverNgVersion(): Promise<string | null> {
  if (ngVersion === null) await serverSession();
  return ngVersion;
}

/** Cached once valid; an unknown one retries the session once, then rejects (no PS5-only choice by accident). */
export async function serverPlatform(): Promise<"host" | "ps5"> {
  if (platform === null) await serverSession();
  if (platform === null) throw text("noPlatform");
  return platform;
}

/** Read afresh (another viewer may have changed it). */
export async function serverNotifications(): Promise<boolean | null> {
  notifications = null;
  await serverSession();
  return platform === "ps5" ? notifications : null;
}

export async function setServerNotifications(on: boolean): Promise<boolean> {
  const answer = await call<{ on: boolean }>("notifications", { on });
  if (typeof answer?.on !== "boolean") throw new Error("Invalid notification setting reply");
  return answer.on;
}

/** The PS5's payload, or on a computer the CLI's own file (session `cli_asset`); null when none. */
export async function serverUpdateTarget(): Promise<{ version: string; asset: string } | null> {
  const p = await serverPlatform();
  if (!version) return null;
  if (p !== "ps5") return cliAsset ? { version, asset: cliAsset } : null;
  // Installable only through elfldr: no toast while it doesn't answer; never marks the page offline.
  try {
    const res = await request("GET", "/api/elfldr", undefined, 3000);
    return res.status === 200 && (res.data as { up?: unknown } | null)?.up === true
      ? { version, asset: "ps5-dump-forge-{v}-ps5.elf" }
      : null;
  } catch {
    return null;
  }
}

export const UPDATE_WAIT_MS = 60000;
const UPDATE_TIMEOUT: UpdateTimeout = "update-timeout";

async function sessionInstance(ms: number): Promise<string | null> {
  try {
    const res = await request("GET", "/api/session", undefined, ms);
    const i = (res.data as { instance?: unknown } | null)?.instance;
    return res.status === 200 && typeof i === "string" ? i : null;
  } catch {
    return null;
  }
}

/** Resolves once a new `instance` answers the session; a network error meanwhile is expected (the old server exits). */
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
  if (event === "close-requested") return Promise.resolve(() => {});
  let set = listeners.get(event);
  if (!set) listeners.set(event, (set = new Set()));
  const g = f as (payload: unknown) => void;
  set.add(g);
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
  stages: boolean;
  logTotal: number;
  done: boolean;
}

/** Null: the next snapshot rebuilds the whole list. */
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
  };
}

/** One snapshot into the app's events: stages once, progress when changed, unseen log lines, done once; a job gone unfinished ends `GONE`. */
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
    if (!was) {
      fresh.push(j);
      continue;
    }
    if (j.stages?.length > 0 && !was.stages)
      emit("job://stages", { kind: "stages", job: j.id, stages: j.stages } satisfies StagesEvent);
    if (j.progress && JSON.stringify(j.progress) !== was.progress) {
      const { stage, done, total, stage_done, stage_total, bytes_per_s, stage_started_ms } = j.progress;
      const event = { kind: "progress" as const, job: j.id, stage, done, total, stage_done, stage_total, bytes_per_s, stage_started_ms };
      emit("job://progress", event satisfies ProgressEvent);
    }
    const n = Math.min(j.log_total - was.logTotal, j.log.length);
    for (const line of n > 0 ? j.log.slice(j.log.length - n) : [])
      emit("job://log", { kind: "log", job: j.id, line } satisfies LogEvent);
    if (j.done && !was.done)
      emit("job://done", { kind: "done", job: j.id, result: j.done.result } satisfies DoneEvent);
  }
  if (fresh.length > 0) emit("jobs://restore", { replace: false, jobs: fresh } satisfies JobsRestore);
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

export const QUIT_WAIT_MS = 120000;
export const STILL_ANSWERING: ClientText = "stillAnswering";

/** Polling pauses meanwhile (an unreachable server is the expected end); rejects with `STILL_ANSWERING` if still up at the deadline. */
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
