// The Queue's job list: one row per job, newest first.

import { memo, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";

import {
  api,
  connect,
  errorText,
  web,
  prettyBytes,
  prettyDuration,
  type ConvertRequest,
  type JobReport,
  type VerifyReport,
} from "./api";
import { basename, CardHead, FormatPill, isNetwork, kindLabel, PathLine, usePeerPath } from "./common";
import { msg, num, useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import { canRetry as retryable, DEFAULT_ZSTD_LEVEL, isCancelled, jobType, packJob, stageFraction, stageWeight, type Action, type Job } from "./jobs";
import { LEVEL_OF, type PackLevel } from "./build/formats";
import { readable } from "./peers";
import { askReplace } from "./replace";
import { refreshConnect, useSavedNames } from "./useConnectState";

type T = Messages["jobs"];

/** `id` names its heading. */
export function JobsCard(props: { jobs: Job[]; dispatch: (a: Action) => void; id: string }) {
  const t = useMessages().jobs;
  const pending = props.jobs.filter((j) => j.request && !j.result).length;
  return (
    <section className="card jobs" aria-labelledby={props.id}>
      <CardHead icon="jobs" title={t.jobs} id={props.id}>
        <span className="muted head-note">
          {pending > 0 ? t.pending(pending) : t.allFinished}
        </span>
      </CardHead>
      <div className="job-list">
        {[...props.jobs].reverse().map((j) => (
          <JobCard
            // A peer's job ids are its own: keyed apart from this device's.
            key={j.remote ? `${j.remote.keyId}:${j.remote.instance ?? ""}:${j.id}` : j.id}
            job={j}
            fresh={!!j.fresh}
            dispatch={props.dispatch}
          />
        ))}
      </div>
    </section>
  );
}

/** Core's `source deleted: <path>`, `source kept: <reason>` or `source partly deleted: ...` line, by its English prefix. */
export function sourceLine(checks: string[]): { deleted: boolean; partly: boolean; text: string } | null {
  for (const c of checks) {
    if (c.startsWith(SOURCE_DELETED)) return { deleted: true, partly: false, text: c.slice(SOURCE_DELETED.length) };
    if (c.startsWith(SOURCE_KEPT)) return { deleted: false, partly: false, text: c.slice(SOURCE_KEPT.length) };
    if (c.startsWith(SOURCE_PARTLY)) return { deleted: false, partly: true, text: c.slice(SOURCE_PARTLY.length) };
  }
  return null;
}
const SOURCE_DELETED = "source deleted: ";
const SOURCE_KEPT = "source kept: ";
const SOURCE_PARTLY = "source partly deleted: ";

/** Verify reads the output back, compress and measure count the bytes they took in, the rest write. */
function rateLabel(t: T, stage: string): string {
  if (stage === "verify") return t.rate.read;
  if (stage === "compress" || stage === "measure") return t.rate.compress;
  if (stage === "pack") return t.rate.pack;
  return t.rate.write;
}

/** Only Finder gets its face; the http build shows the path, selected, to copy. */
const IS_MAC = !web && navigator.userAgent.includes("Mac");
const REVEAL_ICON: IconName = IS_MAC ? "finder" : web ? "file" : "folder";
function revealLabel(t: T): string {
  if (web) return t.showPath;
  if (IS_MAC) return t.showFinder;
  return navigator.userAgent.includes("Windows") ? t.showExplorer : t.showFolder;
}

function editTag(t: T, r: ConvertRequest | undefined): string | null {
  if (r?.extract?.length) return t.extract;
  if (r?.recover_metadata && !r.edits?.length) return msg().metadata.tag;
  if (!r?.edits?.length) return null;
  return r.edit_rebuild && r.format !== "folder" ? t.editRebuild : t.edit;
}

/** Pack with its level (Fast, Medium, Max, else "level N") or unpack. */
function packTag(t: T, p: { kind: "lz4" | "zstd"; id: "pack" | "unpack" }, r: ConvertRequest): string {
  if (p.id === "unpack") return p.kind === "lz4" ? t.packTag.lz4Unpack : t.packTag.zstdUnpack;
  const n = p.kind === "lz4" ? (r.lz4_level ?? 0) : (r.zstd_level ?? DEFAULT_ZSTD_LEVEL);
  const preset = (Object.keys(LEVEL_OF[p.kind]) as PackLevel[]).find((l) => LEVEL_OF[p.kind][l] === n);
  const level = preset ? msg().pack.levels[preset] : t.packTag.level(n);
  const tag = p.kind === "lz4" ? t.packTag.lz4Pack(level) : t.packTag.zstdPack(level);
  return r.pack_profile ? t.packTag.withRules(tag) : tag;
}

function targetKind(r: ConvertRequest): string {
  return r.lz4 === "pack" && r.format === "folder" ? "lz4" : r.format;
}

/** Its output's name; a peer's share root by the device's name; "Job N" before its request arrives. */
export function jobName(job: Job, shown: (path: string) => string, fallback: (id: number) => string): string {
  if (job.request?.install) return basename(job.request.install);
  const out = job.request?.output;
  if (out === undefined) return fallback(job.id);
  return /^flink:\/\/[^/\\]+[/\\]*$/i.test(out) ? shown(out).replace(/: \/$/, "") : basename(out);
}

/** Memoized: a progress event re-renders only its own row (`jobsReducer` keeps the others' objects). */
const JobCard = memo(function JobCard(props: { job: Job; fresh: boolean; dispatch: (a: Action) => void }) {
  const { job, fresh, dispatch } = props;
  const t = useMessages().jobs;
  const installTag = useMessages().install.tag;
  const [cancelling, setCancelling] = useState(false);
  const [retrying, setRetrying] = useState(false);
  /** Still listed (a Dismiss unmounts it). */
  const mounted = useRef(true);
  useEffect(() => {
    // Set here too: Strict Mode's development check unmounts and mounts once.
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  // A running job's Cancel asks once ("Stop job?") for a few seconds; a second click stops it.
  const [confirming, setConfirming] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  /** http build: shown and selected (no clipboard API over plain HTTP). */
  const [pathShown, setPathShown] = useState(false);
  const pathBox = useRef<HTMLParagraphElement>(null);
  const shown = usePeerPath();
  useEffect(() => {
    const el = pathBox.current;
    if (!pathShown || !el) return;
    el.focus();
    window.getSelection()?.selectAllChildren(el);
  }, [pathShown]);
  const name = jobName(job, shown, t.job);
  const result = job.result;
  const remote = job.remote;
  const stopping = cancelling || !!remote?.cancelling;
  const cancel = async () => {
    if (stopping) return;
    if (job.stage !== undefined && !confirming) {
      setConfirming(true);
      return;
    }
    setConfirming(false);
    setCancelling(true);
    setActionError(null);
    try {
      if (remote) {
        // False: the peer's list is gone; the row goes on the next poll.
        if (!(await connect.cancelJob(remote.keyId, job.id))) setCancelling(false);
        await refreshConnect();
      } else await api.cancelJob(job.id);
    } catch (e) {
      setCancelling(false);
      setActionError(errorText(e));
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
    setActionError(null);
    try {
      await api.reveal(job.id);
    } catch (e) {
      setActionError(errorText(e));
    }
  };

  // An edit's "cancelled; …" says what stays changed.
  const cancelled = isCancelled(job);
  const canRetry = retryable(job);
  const retry = async () => {
    if (retrying) return;
    setRetrying(true);
    setActionError(null);
    try {
      await retryJob(job, dispatch, () => mounted.current);
    } catch (e) {
      setActionError(errorText(e));
    } finally {
      setRetrying(false);
    }
  };

  let status: string;
  let tone: string;
  if (result) {
    status = "Ok" in result ? t.done : cancelled ? t.cancelled : t.failed;
    tone = "Ok" in result ? "green" : cancelled ? "" : "red";
  } else if (stopping) [status, tone] = [t.cancelling, ""];
  else if (job.stage === undefined) [status, tone] = [t.queued, ""];
  else [status, tone] = [(t.stages as Record<string, string | undefined>)[job.stage] ?? job.stage, "blue"];
  const running = !result && job.stage !== undefined;
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [running]);
  const alive = running && !stopping;
  const step = running ? clock(Math.max(0, now - job.stageAt)) : null;
  const frac = running ? stageFraction(job, now) : 0;
  const pack = packJob(job.request);
  const edit = editTag(t, job.request);
  const transfer = job.request?.transfer?.length ? job.request.transfer : null;
  const install = job.request?.install ?? null;
  const sourceNote = result && "Ok" in result ? sourceLine(result.Ok.checks) : null;
  const failedHere = !!result && "Err" in result && !cancelled;
  const crumbs = job.stages?.length && (running || failedHere) ? <Crumbs job={job} /> : null;
  const report = result && "Ok" in result ? result.Ok : null;
  if (running && job.stage === "write" && pack?.kind === "lz4" && pack.id === "pack" && targetKind(job.request!) !== "lz4")
    status = t.packingWriting;

  // A job just started here scrolls the page to itself; restored ones (a reload) don't.
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (fresh) ref.current?.scrollIntoView({ block: "nearest" });
  }, [fresh]);
  // Dismiss removes its own row: a focused Dismiss hands focus to a neighbour's first button.
  const dismiss = (e: { currentTarget: HTMLButtonElement }) => {
    if (document.activeElement === e.currentTarget) {
      const row = ref.current;
      const other = row?.nextElementSibling ?? row?.previousElementSibling;
      other?.querySelector<HTMLElement>("button")?.focus();
    }
    dispatch({ type: "dismiss", id: job.id });
  };

  return (
    <article
      className={`job ${jobType(job.request)}${remote ? " remote" : ""}`}
      aria-label={remote ? t.remoteLabel(name, remote.name) : name}
      ref={ref}
    >
      <header className="job-head">
        {job.request && !transfer && !install && <FormatPill kind={targetKind(job.request)} />}
        {pack && job.request && <span className="tag lz4">{packTag(t, pack, job.request)}</span>}
        {edit && <span className="tag edit">{edit}</span>}
        {transfer && <span className="tag transfer">{t.transfer}</span>}
        {install && <span className="tag install">{installTag}</span>}
        {remote && (
          <span className="tag green" title={t.remoteTitle(remote.name)}>
            {remote.name}
          </span>
        )}
        <div className="job-name">
          <strong title={job.request && shown(job.request.output)}>{name}</strong>
          {job.request && <PathLine path={install ?? (transfer ? transfer[0] : job.request.source)} prefix={t.from} />}
        </div>
        {/* An output on a peer (`flink://`) or a NAS has no local folder for the file manager to show. */}
        {report && <CopyDetails job={job} report={report} />}
        {!remote && result && "Ok" in result && (web || !isNetwork(job.request?.output)) && (
          <button className="small" onClick={reveal}>
            <Icon name={REVEAL_ICON} />
            {revealLabel(t)}
          </button>
        )}
        {canRetry && (
          <button
            className="small"
            onClick={retry}
            // Not `disabled`: a focused button that turns disabled drops focus to the page.
            aria-disabled={retrying || undefined}
            aria-busy={retrying || undefined}
          >
            <Icon name="refresh" />
            {retrying ? t.retrying : t.retry}
          </button>
        )}
        {result ? (
          !remote && (
            <button className="small" onClick={dismiss}>
              {t.dismiss}
            </button>
          )
        ) : (
          <button
            className={confirming ? "small danger" : "small"}
            onClick={cancel}
            // Not `disabled`: a focused button that turns disabled drops focus to the page.
            aria-disabled={stopping}
          >
            {confirming ? t.stopJob : t.cancel}
          </button>
        )}
        {/* Says what the second click does when the button turns into "Stop job?". */}
        <span className="visually-hidden" role="status">
          {confirming ? t.stopHint : ""}
        </span>
      </header>
      {running && <SegmentBar job={job} frac={frac} alive={alive} label={`${name}: ${status}`} />}
      {crumbs}
      <p className="job-status">
        <span className={`tag status ${tone}`}>
          {status}
        </span>
        {running && (
          <span className="meta">
            <strong>{Math.floor(100 * frac)}%</strong>
            {job.speed !== undefined && (
              <>
                {` · ${rateLabel(t, job.stage ?? "")} `}
                <bdi>{prettyBytes(job.speed)}/s</bdi>
              </>
            )}
            {job.etaMs !== undefined && ` · ${t.left(prettyDuration(job.etaMs))}`}
            {step && ` · ${t.inStep(step)}`}
          </span>
        )}
        {report && <ResultLine job={job} report={report} />}
      </p>
      {report && <ResultMore job={job} report={report} />}
      {pathShown && result && "Ok" in result && (
        <p className="out-box path-shown" ref={pathBox} tabIndex={-1} role="region" aria-label={`${name}: ${t.outputPath}`}>
          {result.Ok.output}
        </p>
      )}
      {actionError && (
        <p className="bad" role="alert">
          {actionError}
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
            {result.Ok.checks.filter((c) => !sourceLine([c])).map((c, i) => (
              <li key={i}>
                <Icon name="check" />
                <span className="backend">{readable(c)}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
      {/* Delete the source after a successful conversion: what became of it (the job stays Ok). */}
      {sourceNote && (
        <p className={sourceNote.deleted ? "note-line muted" : "note-line warn"}>
          <Icon name={sourceNote.deleted ? "trash" : "warn"} />
          <span className="backend">
            {sourceNote.deleted
              ? t.sourceDeleted(readable(sourceNote.text))
              : sourceNote.partly
                ? t.sourcePartlyDeleted(readable(sourceNote.text))
                : t.sourceKept(readable(sourceNote.text))}
          </span>
        </p>
      )}
      {/* A cancelled edit's "cancelled; …" says what stays changed: a note, not a failure. */}
      {result && "Err" in result && result.Err.startsWith("cancelled; ") && (
        <p className="note-line muted">
          <Icon name="info" />
          <span className="backend">{readable(result.Err)}</span>
        </p>
      )}
      {result && "Err" in result && result.Err !== "cancelled" && !result.Err.startsWith("cancelled; ") && (
        <FailureText
          err={result.Err}
          format={job.request && targetKind(job.request)}
          edit={edit !== null}
          extract={!!job.request?.extract?.length}
          transfer={!!transfer}
          install={!!install}
          metadata={!!job.request?.recover_metadata && !job.request.edits?.length}
        />
      )}
      {job.log.length > 0 && <Log lines={job.log} name={name} />}
    </article>
  );
});

/** A failed copy leaves files behind: an extract or a transfer asks about names there first; false when nothing started. */
/** Jobs already retried (rows stay, D4), by object. ponytail: a list rebuilt after the poller lost the server is new objects, so a retried row is eligible again; key by instance + id if that bites. */
export const retriedJobs = new WeakSet<Job>();

export async function retryJob(job: Job, dispatch: (a: Action) => void, still: () => boolean = () => true): Promise<boolean> {
  const request = job.request;
  if (!request || !retryable(job)) return false;
  const next = request.transfer?.length || request.extract?.length ? await askReplace(request, still) : request;
  if (!next || !still()) return false;
  const id = await api.startJob(next);
  retriedJobs.add(job);
  dispatch({ type: "started", id, request: next });
  return true;
}

type Fact = "file" | "type" | "size" | "files" | "time";

function kindOfPath(path: string): string {
  const m = /\.(exfat|ffpkg|ffpfs|ffpfsc|pkg)$/i.exec(path);
  return m ? m[1].toLowerCase() : "folder";
}

function outputType(r: ConvertRequest | undefined, output: string): string {
  const R = msg().result;
  if (!r) return R.kinds[kindOfPath(output)] ?? kindOfPath(output);
  if (r.install) return R.installed;
  if (r.transfer?.length) return R.transferred;
  if (r.extract?.length) return R.extracted;
  if (r.edits?.length) {
    const k = kindOfPath(output);
    const what = R.kinds[k] ?? k;
    return r.edit_rebuild && k !== "folder" ? R.rebuilt(what) : R.edited(what);
  }
  const p = packJob(r);
  const kind = targetKind(r);
  if (p?.id === "pack") {
    if (kind === "folder" || kind === "lz4") return p.kind === "zstd" ? R.zstdFolder : R.lz4Folder;
    return R.withPacks(R.kinds[kind] ?? kind, p.kind === "zstd" ? "ZSTD" : "LZ4");
  }
  return R.kinds[kind] ?? kindLabel(kind);
}

/** As label + text (the card and the copied summary alike). */
function resultFacts(job: Job, report: JobReport, shown: (p: string) => string): [Fact, string][] {
  const R = msg().result;
  const facts: [Fact, string][] = [
    ["file", shown(report.output)],
    ["type", outputType(job.request, report.output)],
  ];
  const after = report.bytes;
  const before = report.source_bytes || report.verify?.total_bytes;
  // A ratio only when they differ (packs, compression, image metadata), never for a copy.
  const copy = !!job.request?.transfer?.length || !!job.request?.extract?.length;
  if (before && after && !copy && Math.abs(after - before) / before >= 0.01)
    facts.push(["size", `${prettyBytes(before)} -> ${prettyBytes(after)} (${R.ofSource((100 * after) / before)})`]);
  else facts.push(["size", prettyBytes(after)]);
  if (report.files > 0) facts.push(["files", num(report.files)]);
  // Core's times (a reloaded page shows them too), else this page's for an older server.
  const took =
    report.started_at_ms && report.ended_at_ms
      ? report.ended_at_ms - report.started_at_ms
      : job.startedAt !== undefined && job.endedAt !== undefined
        ? job.endedAt - job.startedAt
        : 0;
  if (took > 0) facts.push(["time", prettyDuration(took)]);
  return facts;
}

function verifiedWord(report: JobReport): string {
  const R = msg().result;
  if (report.verify) return report.verify.mode === "full" ? R.verifiedFull : R.verifiedFast;
  return report.checks.length > 0 ? R.verified : R.notVerified;
}

/** The async API on a secure page, else a selected textarea and `execCommand` (old WebKit, plain HTTP). */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  const focused = document.activeElement as HTMLElement | null;
  area.select();
  area.setSelectionRange(0, text.length);
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  }
  area.remove();
  focused?.focus();
  return ok;
}

function ResultLine({ report }: { job: Job; report: JobReport }) {
  const R = useMessages().result;
  const shown = usePeerPath();
  const size = prettyBytes(report.bytes) + (report.files > 0 ? `, ${R.filesN(report.files)}` : "");
  return (
    <span className="result-line">
      <bdi>{size}</bdi>
      <span className="arrow" aria-hidden="true">
        →
      </span>
      <span className="visually-hidden">{R.to}</span>
      <bdi className="path">{shown(report.output)}</bdi>
    </span>
  );
}

function ResultMore({ job, report }: { job: Job; report: JobReport }) {
  const R = useMessages().result;
  const shown = usePeerPath();
  const names = useSavedNames();
  const parts = useMemo(
    () => resultFacts(job, report, shown).filter(([k, v]) => k !== "file" && k !== "files" && (k !== "size" || v.includes("->"))),
    [job, report, shown, names, R],
  );
  return (
    <p className="result-more muted">
      {parts.map(([k, v], i) => (
        <span key={k}>
          {i > 0 && " · "}
          <bdi>{k === "time" ? R.took(v) : v}</bdi>
        </span>
      ))}
    </p>
  );
}

function CopyDetails({ job, report }: { job: Job; report: JobReport }) {
  const t = useMessages().jobs;
  const R = useMessages().result;
  const shown = usePeerPath();
  const [copied, setCopied] = useState<"" | "ok" | "fail">("");
  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(""), 2500);
    return () => clearTimeout(id);
  }, [copied]);
  const copy = async () => {
    const verified = verifiedWord(report);
    const lines = [jobName(job, shown, t.job), ...resultFacts(job, report, shown).map(([k, v]) => `${R[k]}: ${v}`)];
    lines.push(report.verify?.mode === "fast" ? `${verified}: ${coverage(t, report.verify)}` : verified);
    if (report.checks.length) lines.push(R.checks, ...report.checks.map((c) => `- ${readable(c)}`));
    setCopied((await copyText(lines.join("\n"))) ? "ok" : "fail");
  };
  const label = copied === "ok" ? R.copied : copied === "fail" ? R.copyFailed : R.copy;
  return (
    <>
      <button type="button" className="small" onClick={copy}>
        <Icon name={copied === "ok" ? "check" : "file"} />
        {label}
      </button>
      <span className="visually-hidden" role="status">
        {copied ? label : ""}
      </span>
    </>
  );
}

/** One segment per stage, as wide as its weight; one progressbar for assistive tech at the job's whole fraction. */
function SegmentBar({ job, frac, alive, label }: { job: Job; frac: number; alive: boolean; label: string }) {
  const stages = job.stages?.length ? job.stages : [job.stage ?? ""];
  const at = job.stages?.length ? (job.step ?? -1) : 0;
  const pct = Math.floor(100 * job.shown);
  return (
    <div className="segbar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct} aria-label={label}>
      {stages.map((id, i) => {
        const state = i < at ? "done" : i === at ? "now" : "later";
        return (
          <span
            key={i}
            className={`segbar-seg ${state}${state === "now" && alive ? " alive" : ""}`}
            style={{ flexGrow: stageWeight(id), ...(state === "now" ? { "--p": frac } : {}) } as CSSProperties}
          />
        );
      })}
    </div>
  );
}

/** Neighbours with one label merged; a failed job's step crossed in red; an unnamed stage shows its id. */
function Crumbs({ job }: { job: Job }) {
  const t = useMessages().jobs;
  const labels = t.steps as Record<string, string | undefined>;
  const steps: { label: string; last: number }[] = [];
  (job.stages ?? []).forEach((id, i) => {
    const label = labels[id] ?? id;
    const prev = steps[steps.length - 1];
    if (prev?.label === label) prev.last = i;
    else steps.push({ label, last: i });
  });
  const failed = !!job.result && "Err" in job.result;
  const step = job.step;
  const at = step === undefined ? -1 : steps.findIndex((s) => step <= s.last);
  return (
    // `role="list"`: WebKit drops the list role of an `ol` styled without markers.
    <ol className="stage-crumbs" role="list" aria-label={t.stepsLabel}>
      {steps.map((s, i) => {
        const state = at < 0 || i > at ? "later" : i < at ? "done" : failed ? "fail" : "now";
        return (
          <li key={i} className={state} aria-current={state === "now" ? "step" : undefined}>
            {state === "done" && <Icon name="check" />}
            {state === "fail" && <Icon name="x" />}
            <span aria-hidden={state === "now" ? undefined : "true"}>{s.label}</span>
            {state !== "now" && (
              <span className="visually-hidden">
                {state === "done" ? t.stepDone(s.label) : state === "fail" ? t.stepFailed(s.label) : t.stepLater(s.label)}
              </span>
            )}
            {i < steps.length - 1 && (
              <span className="sep" aria-hidden="true">
                ›
              </span>
            )}
          </li>
        );
      })}
    </ol>
  );
}

function verifiedTitle(t: T, r: JobReport): string {
  const n = r.checks.filter((c) => !sourceLine([c])).length;
  if (!r.verify) return t.verifiedPlain(n);
  return r.verify.mode === "full" ? t.verifiedFull(n) : t.verifiedFast(n);
}

function coverage(t: T, v: VerifyReport): string {
  return t.coverage(prettyBytes(v.checked_bytes), prettyBytes(v.total_bytes), v.samples);
}

const CONFIRM_MS = 4000;

function clock(ms: number): string {
  const s = Math.floor(ms / 1000);
  const ss = String(s % 60).padStart(2, "0");
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}:${ss}`;
  return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${ss}`;
}

/** A plain title, then core's text verbatim (without its "preflight failed:"). */
function FailureText(props: { err: string; format?: string; edit?: boolean; extract?: boolean; transfer?: boolean; install?: boolean; metadata?: boolean }) {
  const { format, edit, extract, transfer, install, metadata } = props;
  const t = useMessages().jobs;
  const cantMetadata = useMessages().metadata.cant;
  const cantInstall = useMessages().install.cantInstall;
  const names = useSavedNames();
  const err = useMemo(() => readable(props.err), [props.err, names]);
  const pre = /^preflight failed:\n?/.exec(err);
  return (
    <div className="error-box">
      <strong className="error-title">{install ? cantInstall : metadata ? cantMetadata : transfer ? t.cantTransfer : extract ? t.cantExtract : edit ? t.cantEdit : format ? t.cantBuildAs(kindLabel(format)) : t.cantBuild}</strong>
      <span className="backend">{pre ? err.slice(pre[0].length) : err}</span>
    </div>
  );
}

function lineClass(line: string): string | undefined {
  if (/\b(error|errors|fail|failed|failure|refused|panic)\b|can't/i.test(line)) return "bad";
  if (/\bwarn(ing)?\b/i.test(line)) return "warn";
  if (/\b(verified|ok|done|passed)\b/i.test(line)) return "good";
  return undefined;
}

function logFileName(name: string): string {
  const base = name.replace(/[\\/:*?"<>|\u0000-\u001f]+/g, "_").trim() || "job";
  return `${base}.log`;
}

/** Collapsed until asked for; Clear hides lines up to the last one shown (this view only). */
function Log({ lines, name }: { lines: string[]; name: string }) {
  const t = useMessages().jobs;
  const L = useMessages().log;
  const names = useSavedNames();
  const all = useMemo(() => lines.map((l) => readable(l)), [lines, names]);
  // ponytail: a later line with the same text as Clear's last one hides what came before it.
  const [clearedAt, setClearedAt] = useState<string | null>(null);
  const shown = useMemo(() => {
    if (clearedAt === null) return all;
    const k = all.lastIndexOf(clearedAt);
    return k < 0 ? all : all.slice(k + 1);
  }, [all, clearedAt]);
  const [filter, setFilter] = useState("");
  const q = filter.trim().toLowerCase();
  const visible = useMemo(() => (q ? shown.filter((l) => l.toLowerCase().includes(q)) : shown), [shown, q]);
  const [follow, setFollow] = useState(true);
  const [el, setEl] = useState<HTMLPreElement | null>(null);
  useEffect(() => {
    if (el && follow) el.scrollTop = el.scrollHeight;
  }, [el, visible, follow]);
  const [said, setSaid] = useState("");
  useEffect(() => {
    if (!said) return;
    const id = setTimeout(() => setSaid(""), 2500);
    return () => clearTimeout(id);
  }, [said]);
  const copy = async () => setSaid((await copyText(visible.join("\n"))) ? L.copied : L.copyFailed);
  const save = () => {
    // The Tauri webview doesn't download a link: copy there instead.
    if (!web) return void copy();
    const url = URL.createObjectURL(new Blob([visible.join("\n") + "\n"], { type: "text/plain" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = logFileName(name);
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  const clear = () => {
    setClearedAt(all.length ? all[all.length - 1] : null);
    setSaid(L.cleared);
  };
  return (
    <details className="fold">
      <summary>{t.log(lines.length)}</summary>
      <div className="log-bar">
        <span className="log-count">{q ? L.lines(visible.length, shown.length) : L.lines(shown.length)}</span>
        <input
          type="search"
          className="log-filter"
          value={filter}
          placeholder={L.filter}
          aria-label={`${name}: ${L.filter}`}
          onChange={(e) => setFilter(e.target.value)}
        />
        <label className="log-follow">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          {L.follow}
        </label>
        <button type="button" className="small" onClick={copy} disabled={visible.length === 0}>
          <Icon name="file" />
          {L.copy}
        </button>
        <button type="button" className="small" onClick={save} disabled={visible.length === 0} title={web ? undefined : L.copy}>
          <Icon name="download" />
          {L.save}
        </button>
        <button type="button" className="small" onClick={clear} disabled={shown.length === 0}>
          <Icon name="x" />
          {L.clear}
        </button>
        <span className="visually-hidden" role="status">
          {said}
        </span>
        {said && (
          <span className="log-said" aria-hidden="true">
            {said}
          </span>
        )}
      </div>
      <pre
        className="log log-panel"
        ref={setEl}
        tabIndex={0}
        // A `pre` can't carry a name; a region can.
        role="region"
        aria-label={`${name}: ${t.logLabel}`}
        onScroll={(e) => {
          const p = e.currentTarget;
          const atEnd = p.scrollHeight - p.scrollTop - p.clientHeight < 24;
          if (!atEnd && follow) setFollow(false);
        }}
      >
        {visible.map((l, k) => (
          <span key={k} className={lineClass(l)}>
            {l}
            {"\n"}
          </span>
        ))}
      </pre>
    </details>
  );
}
