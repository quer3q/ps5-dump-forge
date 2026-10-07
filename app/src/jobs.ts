// Job list state, fed by `job://*` events. Lives in App so it survives tab switches.

import type { ConvertRequest, DoneEvent, JobId, JobResult, LogEvent, ProgressEvent } from "./api";

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
  | { type: "dismiss"; id: JobId };

function blank(id: JobId): Job {
  return { id, done: 0, total: 0, shown: 0, stageAt: 0, stageDone: 0, log: [] };
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
        const shown = Math.max(j.shown, total > 0 ? Math.min(1, done / total) : 0);
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
      return update(jobs, a.e.job, (j) => ({
        ...j,
        result: a.e.result,
        etaMs: undefined,
        speed: undefined,
      }));
    case "dismiss":
      return jobs.filter((j) => j.id !== a.id);
  }
}
