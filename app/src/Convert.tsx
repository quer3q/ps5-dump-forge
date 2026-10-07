// Convert: source → target format → output → Build, then one row per job under the target.

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  errorText,
  prettyBytes,
  prettyDuration,
  type Format,
  type InnerFormat,
  type Inspection,
} from "./api";
import {
  basename,
  CardHead,
  classify,
  dirname,
  FormatPicker,
  FormatPill,
  IMAGE_EXTENSIONS,
  joinPath,
  kindLabel,
  PathLine,
  SEPARATORS,
  SourceCard,
} from "./common";
import { Icon } from "./icons";
import type { Action, Job } from "./jobs";

/** The target a new source starts with. */
const DEFAULT_FORMAT: Format = "ffpkg"; // what ShadowMountPlus recommends

/** One line under the target picker: which one to pick, and when. */
const FORMAT_INFO: Record<Format, string> = {
  folder: "Unpack an image or package back into a plain game folder.",
  exfat:
    "For games that misbehave as .ffpkg and only run like external-drive content. Also opens on a Mac or PC.",
  ffpkg:
    "Recommended. The PS5's own file system (UFS2), mounted by ShadowMountPlus. Writable mounts possible.",
  ffpfs:
    "Experimental. The PS5's PFS file system, uncompressed, mounted by ShadowMountPlus. File names must be plain ASCII.",
  ffpfsc:
    "Experimental. Smallest: a compressed container around one image, always mounted read-only. Reads at 150–250 MB/s on the console; busy games may stutter.",
  pkg: "Installs like a store game. Needs kstuff, fpkg-enable and ppr-patch, firmware 11.60 or lower. Slow to build.",
};

/** The image a `.ffpfsc` holds, in picker order. */
const INNER_FORMATS: InnerFormat[] = ["exfat", "ffpkg", "ffpfs"];

/** One line under the inner-image picker. */
const INNER_INFO: Record<InnerFormat, string> = {
  exfat: "Recommended by ShadowMountPlus; MkPFS calls it the most stable layout.",
  ffpkg: "A UFS2 image (.ffpkg) inside.",
  ffpfs:
    "An uncompressed PFS image inside, also recommended by ShadowMountPlus. File names must be plain ASCII.",
};

// ponytail: approximate: the compress pass runs ~85–105 MB/s (measured on Apple Silicon), then
// write and verify run at disk speed, so a whole .pkg job averages ~80 MB/s of source
// (about 20 min for an 89 GB game). Ceiling: other machines and drives differ.
const PKG_BYTES_PER_SEC = 80e6;

/** "about 20 min" for a .fpkg build of `bytes`, to the nearest 5 minutes. */
function pkgBuildTime(bytes: number): string {
  const min = Math.max(5, Math.round(bytes / PKG_BYTES_PER_SEC / 60 / 5) * 5);
  return min < 120 ? `about ${min} min` : `about ${Math.round(min / 60)} h`;
}

/** Extensions a typed name can end in: the real ones, plus "fpkg", the debug package's label. */
const TYPED_EXTENSIONS = [...IMAGE_EXTENSIONS, "fpkg"];

/** A typed output name, with the target's extension appended when it has none. A different
 * image extension is an error: the extension picks the mount driver. */
function checkName(name: string, format: Format): { name: string; error?: string } {
  if (SEPARATORS.test(name))
    return { name, error: "A name can't contain a folder separator; use Change… for the folder." };
  if (format === "folder") return { name };
  const dot = name.lastIndexOf(".");
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  if (ext === format) return { name };
  // The UI calls the debug package ".fpkg"; a typed "Game.fpkg" is saved as "Game.pkg".
  if (format === "pkg" && ext === "fpkg") return { name: `${name.slice(0, dot)}.pkg` };
  if (TYPED_EXTENSIONS.includes(ext))
    return {
      name,
      error:
        format === "pkg"
          ? `Ends in .${ext}, but .fpkg is saved as .pkg: the extension picks the driver.`
          : `Ends in .${ext}, but the target is .${format}: the extension picks the driver.`,
    };
  return { name: `${name}.${format}` };
}

export function Convert(props: {
  jobs: Job[];
  dispatch: (a: Action) => void;
  /** Show the "About formats" tab. */
  onCompare: () => void;
}) {
  const [source, setSource] = useState<string | null>(null);
  const [sourceIsImage, setSourceIsImage] = useState(false);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [insError, setInsError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [format, setFormat] = useState<Format>(DEFAULT_FORMAT);
  /** The image inside a `.ffpfsc`; kept while other targets are picked. */
  const [inner, setInner] = useState<InnerFormat>("exfat");
  const [output, setOutput] = useState("");
  /** Name the output `[GAME_NAME]-[TITLE_ID]-[FIRMWARE]`, brackets included; only its
   * folder is chosen. */
  const [generate, setGenerate] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false); // state lags a fast double click

  // Only the latest pick / output request may land (an older inspect can finish last).
  const sourceSeq = useRef(0);
  const outputSeq = useRef(0);
  // Bumped when the source, format or naming changes: a file dialog opened before that is
  // answered for a choice that no longer stands, so its result is dropped.
  const choiceSeq = useRef(0);
  // The output's folder, kept while a new name is pending (output is empty then).
  const outDir = useRef("");

  const setOut = (path: string) => {
    if (path) outDir.current = dirname(path);
    setOutput(path);
  };

  // Recompute the name whenever the source, format or naming changes, keeping the folder.
  const refreshOutput = async (
    src: string,
    fmt: Format,
    dir: string,
    gen = generate,
    taken = running(props.jobs),
  ) => {
    const seq = ++outputSeq.current;
    try {
      const out = gen
        ? await api.generatedOutput(src, fmt, dir, taken)
        : await api.defaultOutput(src, fmt, dir);
      if (seq === outputSeq.current) setOut(out);
    } catch (e) {
      if (seq === outputSeq.current) setError(errorText(e));
    }
  };

  const pickSource = async (path: string, isImage: boolean) => {
    choiceSeq.current++;
    const fmt = startFormat(DEFAULT_FORMAT, path, isImage);
    const dir = dirname(path);
    setSource(path);
    setSourceIsImage(isImage);
    outDir.current = dir;
    setFormat(fmt);
    setIns(null);
    setInsError(null);
    setError(null);
    setOutput("");
    void refreshOutput(path, fmt, dir);
    setInspecting(true);
    const seq = ++sourceSeq.current;
    try {
      const found = await api.inspect(path);
      if (seq === sourceSeq.current) setIns(found);
    } catch (e) {
      if (seq === sourceSeq.current) setInsError(errorText(e));
    } finally {
      if (seq === sourceSeq.current) setInspecting(false);
    }
  };

  const pickFormat = (fmt: Format) => {
    choiceSeq.current++;
    setFormat(fmt);
    if (!source) return;
    if (!generate && name) {
      // A typed name keeps its stem; only the extension follows the format.
      const dot = name.lastIndexOf(".");
      const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
      const stem = TYPED_EXTENSIONS.includes(ext) ? name.slice(0, dot) : name;
      outputSeq.current++;
      setOutput(joinPath(outDir.current, fmt === "folder" ? stem : `${stem}.${fmt}`));
      return;
    }
    // Clear first: the old name has the old extension, and Build waits for a path.
    setOutput("");
    void refreshOutput(source, fmt, outDir.current);
  };

  const toggleGenerate = (gen: boolean) => {
    choiceSeq.current++;
    setGenerate(gen);
    if (!source) return;
    setOutput("");
    void refreshOutput(source, format, outDir.current, gen);
  };

  // The name part of the output: a typed name keeps whatever the user typed (separators
  // included, which `checkName` refuses), so the folder is never taken from typing.
  const prefix = joinPath(outDir.current, "");
  const name = output.startsWith(prefix) ? output.slice(prefix.length) : basename(output);
  const named = !generate && output ? checkName(name, format) : { name };
  const target = !generate && output && !named.error ? joinPath(outDir.current, named.name) : output;

  // Build would certainly fail: say why next to it instead.
  const hard = ins ? ins.findings.filter((l) => classify(l).kind === "block") : [];
  const blockReason = insError
    ? "Can't build: this source couldn't be read (see Source)."
    : hard.length > 0
      ? `Can't build: ${hard[0]}${hard.length > 1 ? ` (and ${hard.length - 1} more, see Source)` : ""}.`
      : null;

  const typeName = (typed: string) => {
    outputSeq.current++; // a pending default must not overwrite what the user types
    setOutput(typed ? joinPath(outDir.current, typed) : "");
  };

  // Only the folder is chosen here; the name is generated or typed.
  const chooseOutput = async () => {
    if (!source) return;
    const choice = choiceSeq.current;
    const title = format === "folder" ? "Extract into this folder" : "Save into this folder";
    const dir = await open({ directory: true, title });
    if (typeof dir !== "string" || choice !== choiceSeq.current) return;
    outDir.current = dir;
    if (!generate && name) {
      outputSeq.current++;
      setOutput(joinPath(dir, name));
    } else {
      setOutput("");
      void refreshOutput(source, format, dir);
    }
  };

  const build = async () => {
    if (!source || !target || named.error || blockReason || submittingRef.current) return;
    setError(null);
    const request = {
      source,
      format,
      output: target,
      compression_threads: null,
      inner: format === "ffpfsc" ? inner : null,
    };
    submittingRef.current = true;
    setSubmitting(true);
    const choice = choiceSeq.current;
    try {
      const id = await api.startJob(request);
      props.dispatch({ type: "started", id, request });
      // A generated name moves on to the next free one, so Build again starts another job
      // (unless the choices changed meanwhile: their own refresh is the newer one).
      if (generate && choice === choiceSeq.current) {
        setOutput("");
        void refreshOutput(source, format, outDir.current, true, [
          ...running(props.jobs),
          request.output,
        ]);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  const pending = running(props.jobs).length;
  const sameJobs = props.jobs.filter(
    (j) => !j.result && j.request?.source === source && j.request.format === format,
  );
  // A typed name that an unfinished job is writing can't be used twice; a generated one
  // moves on to the next free name, so another Build is a second copy.
  const collides = !generate && sameJobs.some((j) => j.request?.output === target);
  const dupe = source !== null && sameJobs.length > 0;
  const formatWarn = ins
    ? ins.findings.filter((l) => classify(l).formats?.includes(format)).length
    : 0;

  return (
    <div className="screen">
      <div className="cols">
        <SourceCard
          path={source}
          onPick={pickSource}
          busy={inspecting}
          error={insError}
          ins={ins}
        />

        {/* Target, then the jobs under it: a new job shows next to the choices that made it. */}
        <div className="stack">
          <section className="card" aria-labelledby="c-target">
            <CardHead icon="target" title="Target" id="c-target" />
            <FormatPicker
              name="target-format"
              label="Target format"
              value={format}
              onChange={pickFormat}
              disabled={(f) => f === "folder" && source !== null && !sourceIsImage}
            />
            <p className="muted desc">{FORMAT_INFO[format]}</p>
            {format === "ffpfsc" && (
              <div className="field inner">
                {/* The radio group's own label says the same to a screen reader. */}
                <span className="label" aria-hidden="true">
                  Image inside
                </span>
                <FormatPicker
                  name="inner-format"
                  label="Image inside the .ffpfsc"
                  className="inner"
                  formats={INNER_FORMATS}
                  value={inner}
                  onChange={setInner}
                />
                <p className="muted hint">{INNER_INFO[inner]}</p>
              </div>
            )}
            <button className="link" onClick={props.onCompare}>
              <Icon name="table" />
              Compare formats
            </button>
            {format === "pkg" && ins && (
              <p className="alert-bar warn">
                <Icon name="warn" />
                <span>Estimated build time for this game: {pkgBuildTime(ins.total_bytes)}.</span>
              </p>
            )}
            <div className="field" role="group" aria-labelledby="c-output-label">
              <div className="label-row">
                <span className="label" id="c-output-label">
                  Output
                </span>
                <button className="small" onClick={chooseOutput} disabled={!source}>
                  Change…
                </button>
              </div>
              {generate ? (
                // A generated name: shown whole (it wraps), only its folder is chosen.
                <p className="out-box" id="c-output">
                  {output ? (
                    name
                  ) : (
                    <span className="muted">{source ? "Naming…" : "Choose a source first"}</span>
                  )}
                </p>
              ) : (
                // A typed name: the file name only; the folder stays the one below.
                <input
                  id="c-output"
                  className="mono out-input"
                  aria-labelledby="c-output-label"
                  aria-describedby={named.error ? "c-output-error c-output-dir" : "c-output-dir"}
                  aria-invalid={named.error ? true : undefined}
                  value={name}
                  onChange={(e) => typeName(e.target.value)}
                  placeholder={source ? "File name" : "Choose a source first"}
                  disabled={!source}
                  spellCheck={false}
                />
              )}
              {named.error && (
                <p className="bad field-error" id="c-output-error">
                  {named.error}
                </p>
              )}
              {source && (
                <p className="out-dir path" id="c-output-dir">
                  {named.name !== name && !named.error && <>saved as {named.name} </>}in{" "}
                  {outDir.current}
                </p>
              )}
            </div>
            <div className="build-row">
              <label className="switch">
                <input
                  type="checkbox"
                  className="visually-hidden"
                  checked={generate}
                  onChange={(e) => toggleGenerate(e.target.checked)}
                />
                <span className="switch-track" aria-hidden="true" />
                <span>
                  Generate name based on content
                  {/* When on, the Output box shows the generated name itself. */}
                  {!generate && (
                    <span className="muted mono hint">[game name]-[title ID]-[firmware]</span>
                  )}
                </span>
              </label>
              <button
                className="primary"
                onClick={build}
                disabled={!source || !target || !!named.error || !!blockReason || submitting}
              >
                <Icon name="bolt" />
                Build
              </button>
            </div>
            {(blockReason || formatWarn > 0 || dupe || error) && (
              <div className="build-notes">
                {error && <p className="bad">{error}</p>}
                {blockReason && (
                  <p className="note-line block">
                    <Icon name="alert" />
                    <span>{blockReason}</span>
                  </p>
                )}
                {!blockReason && formatWarn > 0 && (
                  <p className="note-line warn">
                    <Icon name="warn" />
                    <span>
                      Likely to fail as {kindLabel(format)}: it refuses file names listed in Source.
                    </span>
                  </p>
                )}
                {dupe && (
                  <p className="note-line muted">
                    <Icon name="info" />
                    <span>
                      {collides
                        ? "A job is already writing this file: Build again would fail. Pick another name."
                        : `Already building this game as ${kindLabel(format)}; Build again makes a second copy.`}
                    </span>
                  </p>
                )}
              </div>
            )}
          </section>

          {props.jobs.length > 0 && (
            <section className="card jobs" aria-labelledby="c-jobs">
              <CardHead icon="jobs" title="Jobs" id="c-jobs">
                <span className="muted head-note">
                  {pending > 0 ? `${pending} running or queued` : "all finished"}
                </span>
              </CardHead>
              {[...props.jobs].reverse().map((j) => (
                <JobCard key={j.id} job={j} dispatch={props.dispatch} />
              ))}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}

/** Where unfinished jobs will publish: taken for a generated name. */
function running(jobs: Job[]): string[] {
  return jobs.flatMap((j) => (j.request && !j.result ? [j.request.output] : []));
}

/** `preferred` (never "folder"), unless an image is already in that format: then it
 * extracts instead. */
function startFormat(preferred: Format, path: string, isImage: boolean): Format {
  if (!isImage) return preferred;
  return path.toLowerCase().endsWith(`.${preferred}`) ? "folder" : preferred;
}

/** Core's stage ids; one bar spans them all, the label says which pass is running. */
const STAGES: Record<string, string> = {
  scan: "Reading the source",
  preflight: "Checking",
  check: "Checking",
  plan: "Planning",
  compress: "Compressing",
  write: "Writing",
  verify: "Verifying",
  finalize: "Finishing",
};

/** What a stage's byte rate measures: verify reads the output back, compress counts the
 * uncompressed bytes it took in, the rest write. */
function rateLabel(stage: string): string {
  if (stage === "verify") return "read";
  if (stage === "compress") return "compress";
  return "write";
}

function JobCard({ job, dispatch }: { job: Job; dispatch: (a: Action) => void }) {
  const [cancelling, setCancelling] = useState(false);
  // A running job's Cancel asks once ("Stop job?") for a few seconds; a second click stops it.
  const [confirming, setConfirming] = useState(false);
  const [revealError, setRevealError] = useState<string | null>(null);
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

  // A new job scrolls into view (it lands under Target, which may end near the fold).
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    ref.current?.scrollIntoView({ block: "nearest" });
  }, []);

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
            <Icon name="finder" />
            Show in Finder
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
      {revealError && (
        <p className="bad" role="alert">
          {revealError}
        </p>
      )}
      {result && "Ok" in result && (
        <details className="fold verified">
          <summary>
            <Icon name="checkCircle" />
            Verified: {result.Ok.checks.length} check{result.Ok.checks.length === 1 ? "" : "s"}{" "}
            passed
          </summary>
          <ul className="checks" aria-label="Checks">
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

/** Collapsed until asked for: the job's card stays short enough to sit beside the form. */
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
