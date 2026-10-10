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
  SnapshotJob,
} from "./api";

const LOG_LINES = 500;

export interface Job {
  id: JobId;
  /** Unknown for a moment when an event beats `start_job`'s reply. */
  request?: ConvertRequest;
  /** No progress yet: the job waits in core's queue. */
  stage?: string;
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
}

export type Action =
  | { type: "started"; id: JobId; request: ConvertRequest }
  | { type: "progress"; e: ProgressEvent; at: number }
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

function blank(id: JobId): Job {
  return { id, done: 0, total: 0, shown: 0, stageAt: 0, stageDone: 0, log: [] };
}

/** A job as the server's list has it: request, last progress, log tail and result. */
function fromSnapshot(j: Job, s: SnapshotJob, at: number): Job {
  let k: Job = { ...j, request: s.request ?? j.request, log: s.log.slice(-LOG_LINES) };
  const p = s.progress;
  if (p) {
    const shown = bar(j.shown, p.done, p.total);
    k = { ...k, stage: p.stage, done: p.done, total: p.total, shown, stageAt: at, stageDone: p.done };
    k = { ...k, etaMs: undefined, speed: undefined };
  }
  if (s.done) k = finish(k, s.done.result);
  return k;
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

export function jobsReducer(jobs: Job[], a: Action): Job[] {
  switch (a.type) {
    case "started":
      return update(jobs, a.id, (j) => ({ ...j, request: a.request }));
    case "progress":
      return update(jobs, a.e.job, (j) => {
        const { stage, done, total } = a.e;
        const shown = j.result ? j.shown : bar(j.shown, done, total);
        if (stage !== j.stage) {
          return {
            ...j,
            stage,
            done,
            total,
            shown,
            stageAt: a.at,
            stageDone: done,
            etaMs: undefined,
            speed: undefined,
          };
        }
        const elapsed = a.at - j.stageAt;
        const rate = elapsed >= 1000 ? (done - j.stageDone) / elapsed : 0;
        const etaMs = rate > 0 && total > done ? (total - done) / rate : undefined;
        const speed = rate > 0 ? rate * 1000 : j.speed;
        return { ...j, done, total, shown, etaMs, speed };
      });
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
