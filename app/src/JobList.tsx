// The job list under a screen's cards: one row per job, newest first, the same list on every
// screen that starts jobs (Convert, LZ4).

import { useEffect, useRef, useState, type MutableRefObject } from "react";

import {
  api,
  errorText,
  web,
  prettyBytes,
  prettyDuration,
  type Format,
  type JobId,
  type JobReport,
  type VerifyReport,
} from "./api";
import { basename, CardHead, FormatPill, kindLabel, PathLine } from "./common";
import { Icon, type IconName } from "./icons";
import type { Action, Job } from "./jobs";
import { running } from "./target";

/** Below all the cards, full width: a new job shows next to the choices that made it, and the
 * queue grows to fill the rest of the window (scrolling inside itself). `started`: jobs this
 * screen started, which scroll into view (not the ones a reload restores). */
export function JobsCard(props: {
  jobs: Job[];
  dispatch: (a: Action) => void;
  started: MutableRefObject<Set<JobId>>;
  prefix: string;
}) {
  const pending = running(props.jobs).length;
  return (
    <section className="card jobs" aria-labelledby={`${props.prefix}-jobs`}>
      <CardHead icon="jobs" title="Jobs" id={`${props.prefix}-jobs`}>
        <span className="muted head-note">
          {pending > 0 ? `${pending} running or queued` : "all finished"}
        </span>
      </CardHead>
      <div className="job-list">
        {[...props.jobs].reverse().map((j) => (
          <JobCard
            key={j.id}
            job={j}
            fresh={props.started.current.has(j.id)}
            dispatch={props.dispatch}
          />
        ))}
      </div>
    </section>
  );
}

/** Core's stage ids; one bar spans them all, the label says which pass is running. */
const STAGES: Record<string, string> = {
  scan: "Reading the source",
  preflight: "Checking",
  check: "Checking",
  plan: "Planning",
  compress: "Compressing",
  write: "Writing",
  pack: "Packing",
  verify: "Verifying",
  finalize: "Finishing",
};

/** What a stage's byte rate measures: verify reads the output back, compress counts the
 * uncompressed bytes it took in, the rest write. */
function rateLabel(stage: string): string {
  if (stage === "verify") return "read";
  if (stage === "compress") return "compress";
  if (stage === "pack") return "pack";
  return "write";
}

/** A finished job's button names the platform's file manager; only Finder gets its face. The
 * http build has none on the console: it shows the path, selected, to copy. */
const IS_MAC = !web && navigator.userAgent.includes("Mac");
const REVEAL_ICON: IconName = IS_MAC ? "finder" : web ? "file" : "folder";
function revealLabel(): string {
  if (web) return "Show path";
  if (IS_MAC) return "Show in Finder";
  return navigator.userAgent.includes("Windows") ? "Show in Explorer" : "Show in folder";
}

function JobCard(props: { job: Job; fresh: boolean; dispatch: (a: Action) => void }) {
  const { job, fresh, dispatch } = props;
  const [cancelling, setCancelling] = useState(false);
  // A running job's Cancel asks once ("Stop job?") for a few seconds; a second click stops it.
  const [confirming, setConfirming] = useState(false);
  const [revealError, setRevealError] = useState<string | null>(null);
  /** http build: the output path, shown and selected (no clipboard API over plain HTTP). */
  const [pathShown, setPathShown] = useState(false);
  const pathBox = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    const el = pathBox.current;
    if (!pathShown || !el) return;
    el.focus();
    window.getSelection()?.selectAllChildren(el);
  }, [pathShown]);
  const name = job.request ? basename(job.request.output) : `Job ${job.id}`;
  const result = job.result;
  const cancel = async () => {
    if (job.stage !== undefined && !confirming) {
      setConfirming(true);
      return;
    }
    setConfirming(false);
    setCancelling(true);
    try {
      await api.cancelJob(job.id);
    } catch {
      setCancelling(false);
    }
  };
  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), CONFIRM_MS);
    return () => clearTimeout(t);
  }, [confirming]);
  const reveal = async () => {
    if (web) {
      const el = pathBox.current;
      if (!el) setPathShown(true);
      else {
        el.focus();
        window.getSelection()?.selectAllChildren(el);
      }
      return;
    }
    setRevealError(null);
    try {
      await api.reveal(job.id);
    } catch (e) {
      setRevealError(errorText(e));
    }
  };

  let status: string;
  let tone: string;
  if (result) {
    status = "Ok" in result ? "Done" : result.Err === "cancelled" ? "Cancelled" : "Failed";
    tone = "Ok" in result ? "green" : result.Err === "cancelled" ? "" : "red";
  } else if (cancelling) [status, tone] = ["Cancelling…", ""];
  else if (job.stage === undefined) [status, tone] = ["Queued", ""];
  else [status, tone] = [STAGES[job.stage] ?? job.stage, "blue"];
  const running = !result && job.stage !== undefined;

  // A job just started here scrolls into view inside the job list, which may already be
  // scrolled; restored ones (a reload) leave the list at its top, the newest job.
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (fresh) ref.current?.scrollIntoView({ block: "nearest" });
  }, [fresh]);

  return (
    <article className="job" aria-label={name} ref={ref}>
      <header className="job-head">
        {job.request && <FormatPill kind={job.request.format} />}
        <div className="job-name">
          <strong title={job.request?.output}>{name}</strong>
          {job.request && <PathLine path={job.request.source} prefix="from " />}
        </div>
        {result && "Ok" in result && (
          <button className="small" onClick={reveal}>
            <Icon name={REVEAL_ICON} />
            {revealLabel()}
          </button>
        )}
        {result ? (
          <button className="small" onClick={() => dispatch({ type: "dismiss", id: job.id })}>
            Dismiss
          </button>
        ) : (
          <button
            className={confirming ? "small danger" : "small"}
            onClick={cancel}
            disabled={cancelling}
          >
            {confirming ? "Stop job?" : "Cancel"}
          </button>
        )}
        {/* Says what the second click does when the button turns into "Stop job?". */}
        <span className="visually-hidden" role="status">
          {confirming ? "Click Stop job? again within 4 seconds to stop it." : ""}
        </span>
      </header>
      {running && <progress value={job.shown} max={1} aria-label={`${name}: ${status}`} />}
      <p className="job-status">
        <span className={`tag status ${tone}`}>{status}</span>
        {running && job.total > 0 && (
          <span className="meta">
            <strong>{Math.floor(100 * job.shown)}%</strong>
            {job.speed !== undefined && ` · ${rateLabel(job.stage ?? "")} ${prettyBytes(job.speed)}/s`}
            {job.etaMs !== undefined && ` · ${prettyDuration(job.etaMs)} left`}
          </span>
        )}
        {result && "Ok" in result && (
          <span>
            {prettyBytes(result.Ok.bytes)}, {result.Ok.files.toLocaleString()} files →{" "}
            <span className="path">{result.Ok.output}</span>
          </span>
        )}
      </p>
      {pathShown && result && "Ok" in result && (
        <p className="out-box path-shown" ref={pathBox} tabIndex={-1} aria-label="Output path">
          {result.Ok.output}
        </p>
      )}
      {revealError && (
        <p className="bad" role="alert">
          {revealError}
        </p>
      )}
      {result && "Ok" in result && (
        <details className="fold verified">
          <summary>
            <Icon name="checkCircle" />
            {verifiedTitle(result.Ok)}
          </summary>
          <ul className="checks" aria-label="Checks">
            {result.Ok.verify?.mode === "fast" && (
              <li>
                <Icon name="check" />
                <span>{coverage(result.Ok.verify)}</span>
              </li>
            )}
            {result.Ok.checks.map((c, i) => (
              <li key={i}>
                <Icon name="check" />
                <span>{c}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
      {result && "Err" in result && result.Err !== "cancelled" && (
        <FailureText err={result.Err} format={job.request?.format} />
      )}
      {job.log.length > 0 && <Log lines={job.log} />}
    </article>
  );
}

/** The verified fold's title: the mode (when the report has one) and the checks passed. */
function verifiedTitle(r: JobReport): string {
  const n = `${r.checks.length} check${r.checks.length === 1 ? "" : "s"}`;
  if (!r.verify) return `Verified: ${n} passed`;
  return `${r.verify.mode === "full" ? "Full" : "Fast"} verification passed: ${n}`;
}

/** Fast verification's coverage: "checked 2.1 GiB of 80.9 GiB in 41 samples". */
function coverage(v: VerifyReport): string {
  return `Checked ${prettyBytes(v.checked_bytes)} of ${prettyBytes(v.total_bytes)} in ${v.samples.toLocaleString()} sample${v.samples === 1 ? "" : "s"}`;
}

/** How long "Stop job?" waits for the second click. */
const CONFIRM_MS = 4000;

/** A failed job's error: core's text verbatim, under a plain title for a preflight failure. */
function FailureText({ err, format }: { err: string; format?: Format }) {
  const pre = /^preflight failed:\n?/.exec(err);
  if (!pre) return <p className="error-box">{err}</p>;
  return (
    <div className="error-box">
      <strong className="error-title">
        Can't build this game{format ? ` as ${kindLabel(format)}` : ""}:
      </strong>
      {err.slice(pre[0].length)}
    </div>
  );
}

/** Collapsed until asked for: the job's row stays short in the list. */
function Log({ lines }: { lines: string[] }) {
  const [el, setEl] = useState<HTMLPreElement | null>(null);
  // Follow the tail as lines arrive.
  useEffect(() => {
    if (el) el.scrollTop = el.scrollHeight;
  }, [el, lines]);
  return (
    <details className="fold">
      <summary>Log ({lines.length})</summary>
      <pre className="log" ref={setEl} tabIndex={0} aria-label="Log">
        {lines.join("\n")}
      </pre>
    </details>
  );
}
