// The http build's client for `ps5-dump-forge serve` (contract: ps5/README.md, "Web server"): plain
// JSON over HTTP, and one poller over `GET /api/jobs` that turns snapshots into the app's job
// events. Type imports only, so a plain `node` can drive it (scripts/check-poller.mjs,
// scripts/smoke.mjs).

import type { DoneEvent, JobsRestore, LogEvent, ProgressEvent, SnapshotJob } from "./api";
import type { EventName, Unlisten } from "./transport";

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
const MAX_BACKOFF_MS = 30000;
const POLL_TIMEOUT_MS = 15000;

/** The result of a job the page saw running, once the server's list no longer has it (its
 * 32-job history dropped it between two polls, so its real result was never seen). */
export const GONE = "no longer reported by PS5 Dump Forge (dropped from its job history before its result was seen)";

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
    throw "Can't reach PS5 Dump Forge. Check that the payload is still running and this device is on the same network.";
  }
  setOffline(false);
  if (res.status !== 200) throw errorOf(res.data, res.status);
  // A job just started or was cancelled: show it without waiting for the next tick.
  if (cmd === "start_job" || cmd === "cancel_job") kick();
  return res.data as T;
}

/** `GET /api/session`'s path separator (`"/"` or `"\\"`), or null when it doesn't answer. */
export async function serverSeparator(): Promise<string | null> {
  try {
    const res = await request("GET", "/api/session", undefined, 3000);
    const sep = (res.data as { separator?: unknown } | null)?.separator;
    return res.status === 200 && (sep === "/" || sep === "\\") ? sep : null;
  } catch {
    return null;
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
    logTotal: j.log_total,
    done: j.done !== null,
    request: j.request !== null,
  };
}

/** Feed one snapshot into the app's events: progress when it changed, only the log lines it
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
    if (!was.done) emit("job://done", { kind: "done", job: id, result: { Err: GONE } } satisfies DoneEvent);
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
export const STILL_ANSWERING = "PS5 Dump Forge is still answering after 2 minutes; check the PS5.";

/** `POST /api/quit`, then wait until the server is gone. Polling pauses meanwhile: an
 * unreachable server is the expected end, not an offline notice. Rejects with
 * `STILL_ANSWERING` (and polls again) if it is still up at the deadline. */
async function quit(): Promise<void> {
  const res = await request("POST", "/api/quit", {}).catch(() => null);
  if (!res) throw "Can't reach PS5 Dump Forge to stop it.";
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
  throw STILL_ANSWERING;
}
