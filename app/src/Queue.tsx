// The Queue screen: every job of this device and its paired peers, newest first, under a head
// with the counts, Retry failed, Clear finished and a filter.

import { useRef, useState } from "react";

import { errorText } from "./api";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { JobsCard, retriedJobs, retryJob } from "./JobList";
import { canRetry, isCancelled, type Action, type Job } from "./jobs";
import type { Messages } from "./locales/en";
import "./queue.css";

type Filter = keyof Messages["queue"]["filters"];
type State = "running" | "queued" | "failed" | "done" | "cancelled";

function stateOf(j: Job): State {
  if (!j.result) return j.stage === undefined ? "queued" : "running";
  if ("Ok" in j.result) return "done";
  return isCancelled(j) ? "cancelled" : "failed";
}

function shownBy(f: Filter, s: State): boolean {
  if (f === "all") return true;
  if (f === "running") return s === "running" || s === "queued";
  if (f === "failed") return s === "failed";
  return s === "done";
}

export function Queue(props: { jobs: Job[]; dispatch: (a: Action) => void }) {
  const { jobs, dispatch } = props;
  const S = useMessages().queue;
  const [filter, setFilter] = useState<Filter>("all");
  const [retrying, setRetrying] = useState<{ i: number; n: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);

  const count: Record<State, number> = { running: 0, queued: 0, failed: 0, done: 0, cancelled: 0 };
  for (const j of jobs) count[stateOf(j)]++;
  const failed = jobs.filter((j) => canRetry(j) && !retriedJobs.has(j));
  // A peer's mirrored job isn't this device's to drop (it goes when the peer stops listing it).
  const finished = jobs.some((j) => j.result && !j.remote);
  const shown = jobs.filter((j) => shownBy(filter, stateOf(j)));

  // One after another: an extract or transfer may ask about names already there, one question at a time.
  const retryAll = async () => {
    if (busy.current || failed.length === 0) return;
    busy.current = true;
    setError(null);
    try {
      for (let i = 0; i < failed.length; i++) {
        setRetrying({ i: i + 1, n: failed.length });
        await retryJob(failed[i], dispatch);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      busy.current = false;
      setRetrying(null);
    }
  };

  if (jobs.length === 0)
    return (
      <div className="screen jobs-screen">
        <div className="card queue-empty">
          <p className="muted">{S.empty}</p>
        </div>
      </div>
    );

  return (
    <div className="screen jobs-screen">
      <div className="queue-head">
        <p className="muted queue-counts" role="status">
          {S.counts(count.running, count.queued, count.failed, count.done)}
        </p>
        <div className="queue-actions">
          <button
            type="button"
            className="small"
            onClick={retryAll}
            // Not `disabled`: a focused button that turns disabled drops focus to the page.
            aria-disabled={retrying !== null || failed.length === 0 || undefined}
            aria-busy={retrying !== null || undefined}
          >
            <Icon name="refresh" />
            {retrying ? S.retrying(retrying.i, retrying.n) : S.retryFailed(failed.length)}
          </button>
          <button
            type="button"
            className="small"
            onClick={() => finished && dispatch({ type: "clearFinished" })}
            aria-disabled={!finished || undefined}
          >
            <Icon name="trash" />
            {S.clearFinished}
          </button>
        </div>
        <div className="kind-chips queue-filter" role="group" aria-label={S.filterLabel}>
          {(Object.keys(S.filters) as Filter[]).map((f) => {
            const n = jobs.filter((j) => shownBy(f, stateOf(j))).length;
            return (
              <button key={f} type="button" className="kind-chip" aria-pressed={filter === f} onClick={() => setFilter(f)}>
                {S.filters[f]} <span className="chip-count">{n}</span>
              </button>
            );
          })}
        </div>
        {error && (
          <p className="bad" role="alert">
            {error}
          </p>
        )}
      </div>
      {shown.length > 0 ? (
        <JobsCard jobs={shown} dispatch={dispatch} id="jobs-queue" />
      ) : (
        <div className="card queue-empty">
          <p className="muted">{S.noneShown}</p>
        </div>
      )}
    </div>
  );
}
