// Job list state, fed by `job://*` events (http build: the server's job list); lives in App.

import type {
  ConvertRequest,
  DoneEvent,
  JobId,
  JobResult,
  JobsRestore,
  LogEvent,
  ProgressEvent,
  RemoteJob,
  SnapshotJob,
  StagesEvent,
} from "./api";

const LOG_LINES = 500;

export interface Job {
  id: JobId;
  /** Unknown for a moment when an event beats `start_job`'s reply. */
  request?: ConvertRequest;
  /** No progress yet: the job waits in core's queue. */
  stage?: string;
  /** Every stage the job will run, in order, once core said (`job://stages`). */
  stages?: string[];
  /** Where `stage` is in `stages`, searched forward (a stage id may come twice). */
  step?: number;
  done: number;
  total: number;
  /** Shown fraction, 0..1. Never drops: core's job total is an estimate that can grow. */
  shown: number;
  stageAt: number;
  stageDone: number;
  stageTotal: number;
  /** Core's `stage_done`/`stage_total`; absent from an older server. */
  stageBytes?: { done: number; total: number };
  /** Milliseconds left in the job at the current stage's rate. */
  etaMs?: number;
  speed?: number;
  log: string[];
  result?: JobResult;
  /** On this page (not for a restored job): the result card's time. */
  startedAt?: number;
  endedAt?: number;
  /** Started from this page (not restored by a reload): its row scrolls into view. */
  fresh?: boolean;
  /** A paired peer's job using this device's files: read only but Cancel; `id` is the peer's. */
  remote?: { keyId: string; name: string; cancelling: boolean; instance?: string };
}

/** An LZ4 or zstd pack or unpack, for its row's tag (a legacy `lz4` target: an LZ4 pack). */
export function packJob(r: ConvertRequest | undefined): { kind: "lz4" | "zstd"; id: "pack" | "unpack" } | null {
  if (!r) return null;
  if (r.zstd_pack) return { kind: "zstd", id: "pack" };
  if (r.zstd_unpack) return { kind: "zstd", id: "unpack" };
  const mode = r.lz4 ?? (r.format === "lz4" ? "pack" : null);
  if (mode === "pack" || mode === "unpack") return { kind: "lz4", id: mode };
  return null;
}

/** The zstd level a zstd pack request without one gets (core's `DEFAULT_ZSTD_LEVEL`). */
export const DEFAULT_ZSTD_LEVEL = 5;

/** A job's type, for its row's tag. */
export type JobType = "convert" | "edit" | "pack" | "transfer" | "install";
export function jobType(r: ConvertRequest | undefined): JobType {
  if (r?.install) return "install";
  if (r?.transfer?.length) return "transfer";
  if (r?.edits?.length || r?.extract?.length || r?.recover_metadata) return "edit";
  if (packJob(r)) return "pack";
  return "convert";
}

export type Action =
  | { type: "started"; id: JobId; request: ConvertRequest }
  | { type: "progress"; e: ProgressEvent; at: number }
  | { type: "stages"; e: StagesEvent }
  | { type: "log"; e: LogEvent }
  | { type: "done"; e: DoneEvent }
  | { type: "restore"; e: JobsRestore; at: number }
  | { type: "dismiss"; id: JobId }
  | { type: "clearFinished" };

/** A cancel, also an edit's "cancelled; ..." (what stays changed). */
export function isCancelled(j: Job): boolean {
  const r = j.result;
  return !!r && "Err" in r && (r.Err === "cancelled" || r.Err.startsWith("cancelled; "));
}

/** A failure here (not a cancel, not a peer's job) can start again, but not an edit adding a
 * browser's uploads: their staging folder goes when the job ends. */
export function canRetry(j: Job): boolean {
  const r = j.result;
  const staged = j.request?.edits?.some((e) => e.op === "add" && /\.forge-upload-\d+\.part[\\/]/.test(e.from));
  return !j.remote && !!j.request && !staged && !!r && "Err" in r && !isCancelled(j);
}

/** Never back, and short of full until the job succeeds (core's estimate can be met early). */
function bar(shown: number, done: number, total: number): number {
  return Math.max(shown, total > 0 ? Math.min(UNFINISHED, done / total) : 0);
}
const UNFINISHED = 0.99;

/** `stage`'s index in `stages`, from `from` on, else anywhere, else `from`. */
function stepOf(stages: string[] | undefined, stage: string | undefined, from: number | undefined): number | undefined {
  if (!stages || stage === undefined) return from;
  const i = stages.indexOf(stage, from ?? 0);
  if (i >= 0) return i;
  const j = stages.indexOf(stage);
  return j >= 0 ? j : from;
}

function blank(id: JobId): Job {
  return { id, done: 0, total: 0, shown: 0, stageAt: 0, stageDone: 0, stageTotal: 0, log: [] };
}

function fromSnapshot(j: Job, s: Omit<SnapshotJob, "log_total">, at: number): Job {
  let k: Job = { ...j, request: s.request ?? j.request, log: s.log.slice(-LOG_LINES) };
  if (s.stages?.length) k = { ...k, stages: s.stages };
  const p = s.progress;
  if (p) {
    const shown = bar(j.shown, p.done, p.total);
    k = { ...k, stage: p.stage, done: p.done, total: p.total, shown, stageAt: stageStart(p, at), stageDone: p.done, stageTotal: p.total };
    k = { ...k, stageBytes: sameStageBytes(j, p), ...rateOf(p) };
  }
  k = { ...k, step: stepOf(k.stages, k.stage, k.step) };
  if (s.done) k = finish(k, s.done.result);
  return k;
}

/** Connect's `remote_jobs` as rows, each built on its row in `prev`, so the bar never drops. */
export function remoteJobs(list: RemoteJob[], prev: Job[], at: number): Job[] {
  return list.map((r) => {
    // Same peer, same run (ids start again at 1 when it restarts), same id.
    const old = prev.find((j) => j.remote?.keyId === r.key_id && j.remote.instance === r.instance && j.id === r.job.id);
    let k = fromSnapshot(old ?? blank(r.job.id), r.job, at);
    // An older peer sends no stage start: the stage keeps the one this page saw.
    if (old && old.stage !== undefined && old.stage === k.stage && r.job.progress?.stage_started_ms === undefined)
      k = { ...k, stageAt: old.stageAt, stageDone: old.stageDone, stageTotal: old.stageTotal };
    return { ...k, remote: { keyId: r.key_id, name: r.name, cancelling: r.cancelling, instance: r.instance } };
  });
}

function stageBytes(p: ProgressEvent): Job["stageBytes"] {
  return p.stage_done !== undefined && p.stage_total !== undefined ? { done: p.stage_done, total: p.stage_total } : undefined;
}

/** Never behind the count shown: a stage of several passes learns each pass's size only when it starts. */
function sameStageBytes(j: Job, p: ProgressEvent): Job["stageBytes"] {
  const next = stageBytes(p);
  const old = j.stageBytes;
  const frac = (b: { done: number; total: number }) => (b.total > 0 ? b.done / b.total : 0);
  return next && old && j.stage === p.stage && frac(next) < frac(old) ? old : next;
}

/** Core's clock when sent (a reloaded page shows the real time), never later than `at`.
 * ponytail: a server clock behind the page's reads as a longer step. */
function stageStart(p: ProgressEvent, at: number): number {
  return p.stage_started_ms ? Math.min(p.stage_started_ms, at) : at;
}

/** Core's speed for a byte stage and the job's time left at it. */
function rateOf(p: ProgressEvent): Pick<Job, "speed" | "etaMs"> {
  const bps = BYTE_STAGES.has(p.stage) ? p.bytes_per_s : null;
  if (!bps || bps <= 0) return { speed: undefined, etaMs: undefined };
  return { speed: bps, etaMs: p.total > p.done ? ((p.total - p.done) / bps) * 1000 : undefined };
}

function finish(j: Job, result: JobResult): Job {
  return { ...j, result, shown: "Ok" in result ? 1 : j.shown, etaMs: undefined, speed: undefined };
}

function update(jobs: Job[], id: JobId, f: (j: Job) => Job): Job[] {
  const i = jobs.findIndex((j) => j.id === id);
  if (i < 0) return [...jobs, f(blank(id))];
  const next = jobs.slice();
  next[i] = f(jobs[i]);
  return next;
}

/** Stages whose time grows with their bytes: only these show speed and time left. */
const BYTE_STAGES = new Set(["compress", "write", "measure", "pack", "verify", "install"]);

/** A stage's share of the job's bytes and its segment's width in the bar. */
export const STAGE_WEIGHT: Record<string, number> = { compress: 4, write: 4, measure: 3, pack: 3, install: 4, verify: 2 };
export function stageWeight(stage: string): number {
  return STAGE_WEIGHT[stage] ?? 1;
}

/** How far the current stage is, 0..1 (never 1 while it runs): core's `stage_done` of
 * `stage_total`. A stage that moves no bytes (0 of 0) eases with time (half at 8 s) and waits
 * under 95% until the next stage starts.
 * An older server sends only the job's `done`/`total`: a byte stage's span is then guessed as its
 * weight's share of the bytes left when it began, among the byte stages still to run. */
export function stageFraction(j: Job, now: number): number {
  if (j.stage === undefined) return 0;
  const own = j.stageBytes;
  if (own && own.total > 0) return Math.max(0, Math.min(UNFINISHED, own.done / own.total));
  if (own) return easing(now - j.stageAt);
  // The job total is an estimate that grows as passes learn their size: the larger of then and now.
  const left = Math.max(j.total, j.stageTotal) - j.stageDone;
  if (BYTE_STAGES.has(j.stage) && left > 0) {
    const rest = (j.stages ?? []).slice(j.step ?? 0).filter((s) => BYTE_STAGES.has(s));
    const sum = rest.reduce((n, s) => n + stageWeight(s), 0) || stageWeight(j.stage);
    const span = (left * stageWeight(j.stage)) / sum;
    return Math.max(0, Math.min(UNFINISHED, (j.done - j.stageDone) / span));
  }
  return easing(now - j.stageAt);
}

function easing(ms: number): number {
  return 0.95 * (1 - Math.pow(0.5, Math.max(0, ms) / 1000 / 8));
}

export function jobsReducer(jobs: Job[], a: Action): Job[] {
  switch (a.type) {
    case "started":
      return update(jobs, a.id, (j) => ({ ...j, request: a.request, fresh: true }));
    case "progress":
      return update(jobs, a.e.job, (j) => {
        const { stage, done, total } = a.e;
        const shown = j.result ? j.shown : bar(j.shown, done, total);
        if (stage !== j.stage) {
          return {
            ...j,
            startedAt: j.startedAt ?? a.at,
            stage,
            step: stepOf(j.stages, stage, j.step),
            done,
            total,
            shown,
            stageAt: stageStart(a.e, a.at),
            stageDone: done,
            stageTotal: total,
            stageBytes: stageBytes(a.e),
            ...rateOf(a.e),
          };
        }
        // Core's speed when it sends one (`bytes_per_s`, null until known).
        if (a.e.bytes_per_s !== undefined) return { ...j, done, total, shown, stageBytes: sameStageBytes(j, a.e), ...rateOf(a.e) };
        // An older server: the bytes done since the stage began over the time since then.
        const elapsed = a.at - j.stageAt;
        const rate = BYTE_STAGES.has(stage) && elapsed >= 1000 ? (done - j.stageDone) / elapsed : 0;
        const etaMs = rate > 0 && total > done ? (total - done) / rate : undefined;
        const speed = rate > 0 ? rate * 1000 : j.speed;
        return { ...j, done, total, shown, etaMs, speed };
      });
    case "stages":
      if (a.e.stages.length === 0) return jobs;
      return update(jobs, a.e.job, (j) => ({ ...j, stages: a.e.stages, step: stepOf(a.e.stages, j.stage, undefined) }));
    case "log":
      return update(jobs, a.e.job, (j) => ({ ...j, log: [...j.log, a.e.line].slice(-LOG_LINES) }));
    case "done":
      return update(jobs, a.e.job, (j) => ({ ...finish(j, a.e.result), endedAt: j.endedAt ?? Date.now() }));
    case "restore": {
      // `replace`: the server's list is the whole truth (a restarted payload reuses ids).
      if (a.e.replace) return a.e.jobs.map((s) => fromSnapshot(blank(s.id), s, a.at));
      let next = jobs;
      for (const s of a.e.jobs) next = update(next, s.id, (j) => fromSnapshot(j, s, a.at));
      return next;
    }
    case "dismiss":
      return jobs.filter((j) => j.id !== a.id);
    case "clearFinished":
      return jobs.filter((j) => !j.result);
  }
}
