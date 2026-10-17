// The job list under a screen's cards: one row per job, newest first, the same list on every
// screen that starts jobs (Convert, LZ4).

import { useEffect, useRef, useState, type CSSProperties, type MutableRefObject } from "react";

import {
  api,
  errorText,
  web,
  prettyBytes,
  prettyDuration,
  type ConvertRequest,
  type Format,
  type JobId,
  type JobReport,
  type VerifyReport,
} from "./api";
import { basename, CardHead, FormatPill, kindLabel, PathLine } from "./common";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import type { Action, Job } from "./jobs";
import { running } from "./target";

type T = Messages["jobs"];

/** Below all the cards, full width: a new job shows next to the choices that made it, and the
 * queue fills the rest of the window, growing with its rows (the page scrolls). `started`: jobs
 * this screen started, which scroll into view (not the ones a reload restores). */
export function JobsCard(props: {
  jobs: Job[];
  dispatch: (a: Action) => void;
  started: MutableRefObject<Set<JobId>>;
  prefix: string;
}) {
  const t = useMessages().jobs;
  const pending = running(props.jobs).length;
  return (
    <section className="card jobs" aria-labelledby={`${props.prefix}-jobs`}>
      <CardHead icon="jobs" title={t.jobs} id={`${props.prefix}-jobs`}>
        <span className="muted head-note">
          {pending > 0 ? t.pending(pending) : t.allFinished}
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

/** What a stage's byte rate measures: verify reads the output back, compress and measure
 * (packing without writing, to size the packs) count the uncompressed bytes they took in, the
 * rest write. */
function rateLabel(t: T, stage: string): string {
  if (stage === "verify") return t.rate.read;
  if (stage === "compress" || stage === "measure") return t.rate.compress;
  if (stage === "pack") return t.rate.pack;
  return t.rate.write;
}

/** A finished job's button names the platform's file manager; only Finder gets its face. The
 * http build has none on the console: it shows the path, selected, to copy. */
const IS_MAC = !web && navigator.userAgent.includes("Mac");
const REVEAL_ICON: IconName = IS_MAC ? "finder" : web ? "file" : "folder";
function revealLabel(t: T): string {
  if (web) return t.showPath;
  if (IS_MAC) return t.showFinder;
  return navigator.userAgent.includes("Windows") ? t.showExplorer : t.showFolder;
}

/** What a job does with LZ4, for its row: its tag (a key of the `lz4` messages), and whether it
 * is an LZ4 tab job (the yellow accent). A legacy `lz4` target is a pack into a folder. An
 * `unpatch` without `lz4_in_place` (a server API request; Convert no longer sends one) is no LZ4
 * tab job and never "packing". */
function lz4Job(r: ConvertRequest | undefined): { id: keyof T["lz4"]; tab: boolean } | null {
  if (!r) return null;
  const mode = r.lz4 ?? (r.format === "lz4" ? "pack" : null);
  if (mode === "pack" || mode === "unpack" || mode === "trace") return { id: mode, tab: true };
  if (mode === "unpatch") return r.lz4_in_place ? { id: "unpatch", tab: true } : { id: "runtime", tab: false };
  return null;
}

/** An Edit tab job: an extract, or its changes applied to the source (in place, or rebuilt beside it). */
function editTag(t: T, r: ConvertRequest | undefined): string | null {
  if (r?.extract?.length) return t.extract;
  if (!r?.edits?.length) return null;
  return r.edit_rebuild && r.format !== "folder" ? t.editRebuild : t.edit;
}

/** The target as its pill shows it: a pack into a folder writes an LZ4 packed folder. */
function targetKind(r: ConvertRequest): string {
  return r.lz4 === "pack" && r.format === "folder" ? "lz4" : r.format;
}

function JobCard(props: { job: Job; fresh: boolean; dispatch: (a: Action) => void }) {
  const { job, fresh, dispatch } = props;
  const t = useMessages().jobs;
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
  const name = job.request ? basename(job.request.output) : t.job(job.id);
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
    // An edit's "cancelled; …" says what stays changed.
    const cancelled = "Err" in result && (result.Err === "cancelled" || result.Err.startsWith("cancelled; "));
    status = "Ok" in result ? t.done : cancelled ? t.cancelled : t.failed;
    tone = "Ok" in result ? "green" : cancelled ? "" : "red";
  } else if (cancelling) [status, tone] = [t.cancelling, ""];
  else if (job.stage === undefined) [status, tone] = [t.queued, ""];
  else [status, tone] = [(t.stages as Record<string, string | undefined>)[job.stage] ?? job.stage, "blue"];
  const running = !result && job.stage !== undefined;
  // One clock per running row, each second: how long the stage has run.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [running]);
  // A sheen crosses the bar while the job runs (the whole track while the fill is too thin to
  // see it): many stages move it little or in steps (checking, planning, a large file), and a
  // sheen that came and went with each step flickered.
  const alive = running && !cancelling;
  const stepMs = Math.max(0, now - job.stageAt);
  const step = running && stepMs >= STEP_MS ? clock(stepMs) : null;
  const lz4 = lz4Job(job.request);
  const edit = editTag(t, job.request);
  // Every step the job will run, once core said; gone when the job ends (its result says it).
  const steps = !result && job.stages?.length ? <Steps job={job} /> : null;
  // Packing straight into an image: its write pass packs too.
  if (running && job.stage === "write" && lz4?.id === "pack" && targetKind(job.request!) !== "lz4")
    status = t.packingWriting;

  // A job just started here scrolls the page to itself; restored ones (a reload) leave the
  // page where it is.
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (fresh) ref.current?.scrollIntoView({ block: "nearest" });
  }, [fresh]);

  return (
    <article className={lz4?.tab ? "job lz4" : "job"} aria-label={name} ref={ref}>
      <header className="job-head">
        {job.request && <FormatPill kind={targetKind(job.request)} />}
        {lz4 && <span className={lz4.tab ? "tag lz4" : "tag"}>{t.lz4[lz4.id]}</span>}
        {edit && <span className="tag">{edit}</span>}
        <div className="job-name">
          <strong title={job.request?.output}>{name}</strong>
          {job.request && <PathLine path={job.request.source} prefix={t.from} />}
        </div>
        {result && "Ok" in result && (
          <button className="small" onClick={reveal}>
            <Icon name={REVEAL_ICON} />
            {revealLabel(t)}
          </button>
        )}
        {result ? (
          <button className="small" onClick={() => dispatch({ type: "dismiss", id: job.id })}>
            {t.dismiss}
          </button>
        ) : (
          <button
            className={confirming ? "small danger" : "small"}
            onClick={cancel}
            disabled={cancelling}
          >
            {confirming ? t.stopJob : t.cancel}
          </button>
        )}
        {/* Says what the second click does when the button turns into "Stop job?". */}
        <span className="visually-hidden" role="status">
          {confirming ? t.stopHint : ""}
        </span>
      </header>
      {running && (
        <div className="bar" style={{ "--p": job.shown } as CSSProperties}>
          <progress value={job.shown} max={1} aria-label={`${name}: ${status}`} />
          {alive && <span className={job.shown < 0.02 ? "sheen track" : "sheen"} aria-hidden="true" />}
        </div>
      )}
      {running && steps}
      <p className="job-status">
        <span className={`tag status ${tone}`}>
          {status}
        </span>
        {running && (job.total > 0 || step) && (
          <span className="meta">
            {job.total > 0 && (
              <>
                <strong>{Math.floor(100 * job.shown)}%</strong>
                {job.speed !== undefined && ` · ${rateLabel(t, job.stage ?? "")} ${prettyBytes(job.speed)}/s`}
                {job.etaMs !== undefined && ` · ${t.left(prettyDuration(job.etaMs))}`}
              </>
            )}
            {step && `${job.total > 0 ? " · " : ""}${t.inStep(step)}`}
          </span>
        )}
        {result && "Ok" in result && (
          <span>
            {t.result(prettyBytes(result.Ok.bytes), result.Ok.files)}{" "}
            <span className="path">{result.Ok.output}</span>
          </span>
        )}
      </p>
      {!running && steps}
      {pathShown && result && "Ok" in result && (
        <p className="out-box path-shown" ref={pathBox} tabIndex={-1} aria-label={t.outputPath}>
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
            {verifiedTitle(t, result.Ok)}
          </summary>
          <ul className="checks" aria-label={t.checks}>
            {result.Ok.verify?.mode === "fast" && (
              <li>
                <Icon name="check" />
                <span>{coverage(t, result.Ok.verify)}</span>
              </li>
            )}
            {result.Ok.checks.map((c, i) => (
              <li key={i}>
                <Icon name="check" />
                <span className="backend">{c}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
      {result && "Err" in result && result.Err !== "cancelled" && (
        <FailureText
          err={result.Err}
          format={job.request?.format}
          edit={edit !== null}
          extract={!!job.request?.extract?.length}
        />
      )}
      {job.log.length > 0 && <Log lines={job.log} />}
    </article>
  );
}

/** The job's steps in order, neighbours with one label merged (preflight and check are one
 * "Checking"): done before the current one, the rest not started. A stage the messages don't
 * name shows its id, like the status tag. */
function Steps({ job }: { job: Job }) {
  const t = useMessages().jobs;
  const labels = t.steps as Record<string, string | undefined>;
  const steps: { label: string; last: number }[] = [];
  (job.stages ?? []).forEach((id, i) => {
    const label = labels[id] ?? id;
    const prev = steps[steps.length - 1];
    if (prev?.label === label) prev.last = i;
    else steps.push({ label, last: i });
  });
  const step = job.step;
  const at = step === undefined ? -1 : steps.findIndex((s) => step <= s.last);
  return (
    // `role="list"`: WebKit drops the list role of an `ol` styled without markers.
    <ol className="steps" role="list" aria-label={t.stepsLabel}>
      {steps.map((s, i) => {
        const state = at < 0 || i > at ? "later" : i < at ? "done" : "now";
        return (
          <li key={i} className={state} aria-current={state === "now" ? "step" : undefined}>
            {i > 0 && (
              <span className="sep" aria-hidden="true">
                ›
              </span>
            )}
            {state === "done" && <Icon name="check" />}
            {state === "now" ? (
              s.label
            ) : (
              <>
                <span aria-hidden="true">{s.label}</span>
                <span className="visually-hidden">
                  {state === "done" ? t.stepDone(s.label) : t.stepLater(s.label)}
                </span>
              </>
            )}
          </li>
        );
      })}
    </ol>
  );
}

/** The verified fold's title: the mode (when the report has one) and the checks passed. */
function verifiedTitle(t: T, r: JobReport): string {
  const n = r.checks.length;
  if (!r.verify) return t.verifiedPlain(n);
  return r.verify.mode === "full" ? t.verifiedFull(n) : t.verifiedFast(n);
}

/** Fast verification's coverage: "checked 2.1 GiB of 80.9 GiB in 41 samples". */
function coverage(t: T, v: VerifyReport): string {
  return t.coverage(prettyBytes(v.checked_bytes), prettyBytes(v.total_bytes), v.samples);
}

/** How long "Stop job?" waits for the second click. */
const CONFIRM_MS = 4000;
/** A stage running this long shows its time on the meta line. */
const STEP_MS = 10_000;

/** A stage's running time as a clock: "0:42", "12:05", "1:02:09". */
function clock(ms: number): string {
  const s = Math.floor(ms / 1000);
  const ss = String(s % 60).padStart(2, "0");
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}:${ss}`;
  return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${ss}`;
}

/** A failed job's error: core's text verbatim, under a plain title for a preflight failure. */
function FailureText(props: { err: string; format?: Format; edit?: boolean; extract?: boolean }) {
  const { err, format, edit, extract } = props;
  const t = useMessages().jobs;
  const pre = /^preflight failed:\n?/.exec(err);
  if (!pre) return <p className="error-box backend">{err}</p>;
  return (
    <div className="error-box">
      <strong className="error-title">{extract ? t.cantExtract : edit ? t.cantEdit : format ? t.cantBuildAs(kindLabel(format)) : t.cantBuild}</strong>
      <span className="backend">{err.slice(pre[0].length)}</span>
    </div>
  );
}

/** Collapsed until asked for: the job's row stays short in the list. */
function Log({ lines }: { lines: string[] }) {
  const t = useMessages().jobs;
  const [el, setEl] = useState<HTMLPreElement | null>(null);
  // Follow the tail as lines arrive.
  useEffect(() => {
    if (el) el.scrollTop = el.scrollHeight;
  }, [el, lines]);
  return (
    <details className="fold">
      <summary>{t.log(lines.length)}</summary>
      <pre className="log" ref={setEl} tabIndex={0} aria-label={t.logLabel}>
        {lines.join("\n")}
      </pre>
    </details>
  );
}
