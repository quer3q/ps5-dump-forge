// Job list state, fed by `job://*` events (and, in the http build, rebuilt from the server's
// job list). Lives in App so it survives tab switches.

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
  /** Where `stage` is in `stages`: searched forward from the last one, so a stage id that comes
   * twice is found in order. */
  step?: number;
  done: number;
  total: number;
  /** Shown fraction, 0..1. Never drops: core's job total is an estimate that can grow. */
  shown: number;
  /** When the current stage began, and how far it was then (for the rate). */
  stageAt: number;
  stageDone: number;
  /** Milliseconds left in the job at the current stage's rate, when a rate is known. */
  etaMs?: number;
  /** Bytes per second in the current stage, when known. */
  speed?: number;
  log: string[];
  result?: JobResult;
  /** Started from this page (not restored by a reload): its row scrolls into view. */
  fresh?: boolean;
  /** A job on a paired peer that uses this device's files (Connect's `remote_jobs`): shown read
   * only, only cancellable. Its `id` is the peer's. */
  remote?: { keyId: string; name: string; cancelling: boolean; instance?: string };
}

/** What a Pack tab job does, for its row's tag: an LZ4 or zstd pack or unpack. A legacy `lz4`
 * target is an LZ4 pack into a folder. */
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

/** The tab a job belongs to (its counter and colour): a package install (Storage; a peer's, using
 * this device's package, Connect), a transfer (Connect), an edit or extract (Edit), a Pack tab
 * job (LZ4 or zstd, pack or unpack), else a conversion. */
export type JobType = "convert" | "edit" | "pack" | "transfer" | "install";
export function jobType(r: ConvertRequest | undefined): JobType {
  if (r?.install) return "install";
  if (r?.transfer?.length) return "transfer";
  if (r?.edits?.length || r?.extract?.length) return "edit";
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
  | { type: "dismiss"; id: JobId };

/** The bar for `done` of `total`: never back, and short of full until the job succeeds (core's
 * estimate can be met before its last pass ends). */
function bar(shown: number, done: number, total: number): number {
  return Math.max(shown, total > 0 ? Math.min(UNFINISHED, done / total) : 0);
}
const UNFINISHED = 0.99;

/** `stage`'s index in `stages`, from `from` on, else anywhere; else `from` (a stage the list
 * doesn't name leaves the step where it was). */
function stepOf(stages: string[] | undefined, stage: string | undefined, from: number | undefined): number | undefined {
  if (!stages || stage === undefined) return from;
  const i = stages.indexOf(stage, from ?? 0);
  if (i >= 0) return i;
  const j = stages.indexOf(stage);
  return j >= 0 ? j : from;
}

function blank(id: JobId): Job {
  return { id, done: 0, total: 0, shown: 0, stageAt: 0, stageDone: 0, log: [] };
}

/** A job as the server's list has it: request, last progress, log tail and result. */
function fromSnapshot(j: Job, s: Omit<SnapshotJob, "log_total">, at: number): Job {
  let k: Job = { ...j, request: s.request ?? j.request, log: s.log.slice(-LOG_LINES) };
  if (s.stages?.length) k = { ...k, stages: s.stages };
  const p = s.progress;
  if (p) {
    const shown = bar(j.shown, p.done, p.total);
    k = { ...k, stage: p.stage, done: p.done, total: p.total, shown, stageAt: at, stageDone: p.done };
    k = { ...k, etaMs: undefined, speed: undefined };
  }
  k = { ...k, step: stepOf(k.stages, k.stage, k.step) };
  if (s.done) k = finish(k, s.done.result);
  return k;
}

/** Connect's `remote_jobs` as rows, each built on its row in `prev` (same peer and id): the bar
 * never drops and a stage keeps its start while it lasts. */
export function remoteJobs(list: RemoteJob[], prev: Job[], at: number): Job[] {
  return list.map((r) => {
    // Same peer, same run (ids start again at 1 when it restarts), same id.
    const old = prev.find((j) => j.remote?.keyId === r.key_id && j.remote.instance === r.instance && j.id === r.job.id);
    let k = fromSnapshot(old ?? blank(r.job.id), r.job, at);
    if (old && old.stage !== undefined && old.stage === k.stage) k = { ...k, stageAt: old.stageAt, stageDone: old.stageDone };
    return { ...k, remote: { keyId: r.key_id, name: r.name, cancelling: r.cancelling, instance: r.instance } };
  });
}

/** The job's result: full only on success. */
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

/** Stages whose time grows with the bytes they stream: speed and time left are shown only for
 * these. */
const BYTE_STAGES = new Set(["compress", "write", "measure", "pack", "verify", "install"]);

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
            stage,
            step: stepOf(j.stages, stage, j.step),
            done,
            total,
            shown,
            stageAt: a.at,
            stageDone: done,
            etaMs: undefined,
            speed: undefined,
          };
        }
        // Only a pass that streams the game's bytes has a rate worth projecting; the others
        // (reading the source, checking, planning, finishing) show their time in the step.
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
      return update(jobs, a.e.job, (j) => finish(j, a.e.result));
    case "restore": {
      // `replace`: the server's list is the whole truth (a reload, or the payload restarted,
      // which reuses ids), so each job is only what the snapshot says.
      if (a.e.replace) return a.e.jobs.map((s) => fromSnapshot(blank(s.id), s, a.at));
      let next = jobs;
      for (const s of a.e.jobs) next = update(next, s.id, (j) => fromSnapshot(j, s, a.at));
      return next;
    }
    case "dismiss":
      return jobs.filter((j) => j.id !== a.id);
  }
}
