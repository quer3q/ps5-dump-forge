// Convert: source → target format → output → Build, then one row per job below the cards.

import { useEffect, useRef, useState } from "react";

import {
  api,
  errorText,
  pick,
  web,
  prettyBytes,
  prettyDuration,
  type Format,
  type InnerFormat,
  type Inspection,
  type KrakenLevel,
  type JobId,
  type JobReport,
  type VerifyReport,
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
  emulatorNames,
} from "./common";
import { Icon, type IconName } from "./icons";
import type { Action, Job } from "./jobs";

/** The target a new source starts with. */
const DEFAULT_FORMAT: Format = "ffpkg"; // what ShadowMountPlus recommends

/** One line under the target picker: which one to pick, and when. */
const FORMAT_INFO: Record<Format, string> = {
  folder: "Unpack an image or package back into a plain game folder.",
  exfat:
    "For games that misbehave as .ffpkg and only run like external-drive content. Also opens on a Mac or PC.",
  ffpkg: "Recommended. The PS5's own file system (UFS2). Writable mounts possible.",
  ffpfs: "Experimental. The PS5's PFS file system, uncompressed. File names must be plain ASCII.",
  ffpfsc: "Experimental. Smallest: a compressed container around one image, always mounted read-only.",
  pkg: "Installs like a store game and runs at native speed. Needs kstuff, fpkg-enable and ppr-patch.",
};

/** The image a `.ffpfsc` holds, in picker order. */
const INNER_FORMATS: InnerFormat[] = ["exfat", "ffpkg", "ffpfs"];

/** One line under the inner-image picker: the format's own line, less what a `.ffpfsc` (always
 * mounted read-only) doesn't give. */
const INNER_INFO: Record<InnerFormat, string> = {
  exfat: FORMAT_INFO.exfat,
  ffpkg: "Recommended. The PS5's own file system (UFS2).",
  ffpfs: FORMAT_INFO.ffpfs,
};

/** The `.ffpfsc` zlib level a new window starts on. */
const DEFAULT_FFPFSC_LEVEL = 6;

/** One line under the `.ffpfsc` level slider: what the chosen level costs, from a 13 GB test
 * game on 14 cores (0: 3.0 s, 100% stored; 1: 5.7 s, 78.6%; 4: 13.0 s, 76.4%; 6: 16.3 s,
 * 76.25%; 7: 17.3 s; 9: 20.4 s, 76.21%). */
function ffpfscLevelNote(level: number): string {
  if (level === 0) return "No compression: every block stored as it is. The file is as large as the image.";
  if (level === DEFAULT_FFPFSC_LEVEL)
    return "Recommended (zlib's default). 1 builds about 3× faster with a file about 3% larger; 9 is barely smaller.";
  if (level < DEFAULT_FFPFSC_LEVEL)
    return "Builds faster than 6 (level 1 about 3×); the file comes out larger (level 1 about 3%).";
  return "Barely smaller than 6 (level 9 by 0.04%), but compressing takes longer (level 9 about 25%), with every core busy throughout.";
}

/** The `.fpkg` compression levels, in picker order: icon, label, and one line under the picker. */
const KRAKEN_LEVELS: KrakenLevel[] = ["fast", "balanced", "smallest"];
const KRAKEN_INFO: Record<KrakenLevel, { icon: IconName; label: string; note: string }> = {
  fast: {
    icon: "bolt",
    label: "Fast",
    note: "Recommended. The quickest build.",
  },
  balanced: {
    icon: "scale",
    label: "Balanced",
    note: "About 2.6% smaller than Fast, but compressing takes about 6× as long, with every core busy the whole time.",
  },
  smallest: {
    icon: "compress",
    label: "Smallest",
    note: "About 2.7% smaller than Fast, but compressing takes about 9× as long, with every core busy the whole time.",
  },
};

/** A note with its leading "Recommended" in green or "Experimental" in amber (text colour
 * only; the word itself says it). */
function Lead({ text }: { text: string }) {
  const word = /^(Recommended|Experimental)\b/.exec(text)?.[1];
  if (!word) return <>{text}</>;
  return (
    <>
      <span className={word === "Recommended" ? "lead-good" : "lead-warn"}>{word}</span>
      {text.slice(word.length)}
    </>
  );
}

/** Extensions a typed name can end in: the real ones, plus "fpkg", the debug package's label. */
const TYPED_EXTENSIONS = [...IMAGE_EXTENSIONS, "fpkg"];

/** The longest output name without its extension (UTF-8 bytes): what ShadowMountPlus mounts an
 * image from, the same for a folder and a .pkg; core's `preflight::stem_limit`, which refuses
 * longer ones when the job starts. */
const STEM_LIMIT: Record<Format, number> = {
  folder: 63,
  exfat: 63,
  ffpkg: 63,
  ffpfs: 63,
  ffpfsc: 58,
  pkg: 63,
};

/** A typed output name, with the target's extension appended when it has none. A different
 * image extension is an error: the extension picks the mount driver. So is a name over
 * `STEM_LIMIT`. */
function checkName(name: string, format: Format): { name: string; error?: string } {
  if (SEPARATORS.test(name))
    return { name, error: "A name can't contain a folder separator; use Change… for the folder." };
  const tooLong = (stem: string) => {
    const bytes = new TextEncoder().encode(stem).length;
    const max = STEM_LIMIT[format];
    const what = format === "folder" ? "" : " before the extension";
    return bytes > max ? `Too long: ${bytes} bytes${what}, at most ${max} (UTF-8).` : undefined;
  };
  if (format === "folder") return { name, error: tooLong(name) };
  const dot = name.lastIndexOf(".");
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  let saved: string;
  if (ext === format) saved = name;
  // The UI calls the debug package ".fpkg"; a typed "Game.fpkg" is saved as "Game.pkg".
  else if (format === "pkg" && ext === "fpkg") saved = `${name.slice(0, dot)}.pkg`;
  else if (TYPED_EXTENSIONS.includes(ext))
    return {
      name,
      error:
        format === "pkg"
          ? `Ends in .${ext}, but .fpkg is saved as .pkg: the extension picks the driver.`
          : `Ends in .${ext}, but the target is .${format}: the extension picks the driver.`,
    };
  else saved = `${name}.${format}`;
  const error = tooLong(saved.slice(0, saved.lastIndexOf(".")));
  return error ? { name, error } : { name: saved };
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
  /** Name the output `[GAME_NAME]-[TITLE_ID]`, brackets included; only its folder is
   * chosen. */
  const [generate, setGenerate] = useState(true);
  /** Leave the backport libraries out; offered only for a source that has some. */
  const [removeBackport, setRemoveBackport] = useState(false);
  /** Re-read every byte instead of sampling; kept across sources and targets. */
  const [fullVerify, setFullVerify] = useState(false);
  /** The `.fpkg` compression level; kept across sources and targets. */
  const [krakenLevel, setKrakenLevel] = useState<KrakenLevel>("fast");
  /** The `.ffpfsc` zlib level, 0–9; kept across sources and targets. */
  const [ffpfscLevel, setFfpfscLevel] = useState(DEFAULT_FFPFSC_LEVEL);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false); // state lags a fast double click
  /** Jobs this page started: only those scroll into view (not the ones a reload restores). */
  const started = useRef(new Set<JobId>());

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
    setRemoveBackport(false);
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

  // Removing the backport is refused when its executables' SDK was lowered (core says why).
  const removable = ins !== null && ins.backport.length > 0 && !ins.backport_blocked;
  const libs = ins ? ins.backport.length : 0;
  const emus = ins ? emulatorNames(ins) : [];

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
    const dir = await pick({ directory: true, title });
    if (dir === null || choice !== choiceSeq.current) return;
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
      remove_backport: removable && removeBackport,
      full_verify: fullVerify,
      kraken_level: krakenLevel,
      ffpfsc_level: ffpfscLevel,
    };
    submittingRef.current = true;
    setSubmitting(true);
    const choice = choiceSeq.current;
    try {
      const id = await api.startJob(request);
      started.current.add(id);
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
  const hasJobs = props.jobs.length > 0;

  return (
    <div className={hasJobs ? "screen with-jobs" : "screen"}>
      <div className="cols">
        <SourceCard
          path={source}
          onPick={pickSource}
          busy={inspecting}
          error={insError}
          ins={ins}
        />

        <section className="card" aria-labelledby="c-target">
          <CardHead icon="target" title="Target" id="c-target" />
          <FormatPicker
            name="target-format"
            label="Target format"
            value={format}
            onChange={pickFormat}
            disabled={(f) => f === "folder" && source !== null && !sourceIsImage}
          />
          <p className="muted desc">
            <Lead text={FORMAT_INFO[format]} />
          </p>
          {format === "pkg" && (
            <p className="alert-bar warn">
              <Icon name="warn" />
              <span>Works only on firmware 11.60 or lower.</span>
            </p>
          )}
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
              <p className="muted hint">
                <Lead text={INNER_INFO[inner]} />
              </p>
              <label className="label range-label" htmlFor="c-ffpfsc-level">
                <span>Compression level</span>
                <span className="range-value">{ffpfscLevel}</span>
              </label>
              <input
                id="c-ffpfsc-level"
                type="range"
                className="range"
                min={0}
                max={9}
                step={1}
                value={ffpfscLevel}
                onChange={(e) => setFfpfscLevel(Number(e.target.value))}
              />
              <p className="muted hint">
                <Lead text={ffpfscLevelNote(ffpfscLevel)} />
              </p>
            </div>
          )}
          {ins && ins.backport.length > 0 && (
            <div className="field backport">
              <label className="switch">
                <input
                  type="checkbox"
                  className="visually-hidden"
                  checked={removable && removeBackport}
                  disabled={!removable}
                  onChange={(e) => setRemoveBackport(e.target.checked)}
                />
                <span className="switch-track" aria-hidden="true" />
                <span>
                  Remove backport
                  {removable && (
                    <span className="muted hint">
                      Leaves out {libs} backport librar{libs === 1 ? "y" : "ies"} from fakelib/
                      {keptNote(emus)}
                    </span>
                  )}
                </span>
              </label>
              {ins.backport_blocked && (
                <p className="note-line warn">
                  <Icon name="warn" />
                  <span>Can't remove the backport: {ins.backport_blocked.replace(/^remove backport: /, "")}</span>
                </p>
              )}
            </div>
          )}
          {format === "pkg" && (
            <div className="field inner">
              {/* The radio group's own label says the same to a screen reader. */}
              <span className="label" aria-hidden="true">
                Compression
              </span>
              <div className="seg formats inner" role="radiogroup" aria-label="Compression level">
                {KRAKEN_LEVELS.map((l) => (
                  <label key={l} className={`seg-choice fmt-pkg${krakenLevel === l ? " on" : ""}`}>
                    <input
                      type="radio"
                      className="visually-hidden"
                      name="kraken-level"
                      value={l}
                      checked={krakenLevel === l}
                      onChange={() => setKrakenLevel(l)}
                    />
                    <span className="seg-opt">
                      <Icon name={KRAKEN_INFO[l].icon} />
                      {KRAKEN_INFO[l].label}
                    </span>
                  </label>
                ))}
              </div>
              <p className="muted hint">
                <Lead text={KRAKEN_INFO[krakenLevel].note} />
              </p>
            </div>
          )}
          <div className="field verify">
            <label className="switch">
              <input
                type="checkbox"
                className="visually-hidden"
                checked={fullVerify}
                onChange={(e) => setFullVerify(e.target.checked)}
              />
              <span className="switch-track" aria-hidden="true" />
              <span>
                Full verification
                <span className="muted hint">Re-reads every byte to check the output. Takes longer.</span>
              </span>
            </label>
          </div>
          <button className="link" onClick={props.onCompare}>
            <Icon name="table" />
            Compare formats
          </button>
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
                  <span className="muted mono hint">[game name]-[title ID]</span>
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
      </div>

      {/* Below all the cards, full width: a new job shows next to the choices that made it,
          and the queue grows to fill the rest of the window (scrolling inside itself). */}
      {hasJobs && (
        <section className="card jobs" aria-labelledby="c-jobs">
          <CardHead icon="jobs" title="Jobs" id="c-jobs">
            <span className="muted head-note">
              {pending > 0 ? `${pending} running or queued` : "all finished"}
            </span>
          </CardHead>
          <div className="job-list">
            {[...props.jobs].reverse().map((j) => (
              <JobCard
                key={j.id}
                job={j}
                fresh={started.current.has(j.id)}
                dispatch={props.dispatch}
              />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/** What "Remove backport" keeps: the emulators by name, and any other homebrew libraries. */
function keptNote(emus: string[]): string {
  const named = emus.filter((n) => n !== "Other");
  const other = emus.includes("Other");
  if (named.length === 0) return other ? "; other homebrew libraries stay." : ".";
  return `; the emulators (${named.join(", ")})${other ? " and other homebrew libraries" : ""} stay.`;
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
