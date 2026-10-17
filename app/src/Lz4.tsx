// LZ4: ampr_emu asset packs for a title that uses AMPR. Two scenarios: Pack/Unpack (an LZ4
// packed folder or plain image from traces, a profile or a guess; Unpack for a packed source;
// the default) and Trace (Patch or Unpatch the source itself: a folder in place by one request,
// an .exfat/.ffpkg by a job that rebuilds it and replaces it once verified). Jobs join the shared list below.

import { useEffect, useRef, useState } from "react";

import { msg, num, useMessages } from "./i18n";

import {
  api,
  errorText,
  pick,
  prettyBytes,
  web,
  type ConvertRequest,
  type Format,
  type Inspection,
  type JobId,
  type Lz4Facts,
  type Lz4Patch,
} from "./api";
import {
  CardHead,
  ChooseMenu,
  classify,
  FormatPicker,
  isArchive,
  isWithin,
  kindLabel,
  onDeleted,
  pickSource as openPicker,
  SourceCard,
} from "./common";
import { openLz4Help } from "./Lz4Help";
import { Icon } from "./icons";
import { JobsCard } from "./JobList";
import { inspectSource as readSource } from "./password";
import type { Action, Job } from "./jobs";
import {
  formatInfo,
  GenerateSwitch,
  Lead,
  OutputField,
  running,
  useOutput,
  VerifySwitch,
} from "./target";

export type Scenario = "trace" | "pack";
const SCENARIOS: Scenario[] = ["pack", "trace"];

/** What Trace does to the source. */
export type TraceAct = "patch" | "unpatch";
const TRACE_ACTS: TraceAct[] = ["patch", "unpatch"];

/** Sources Trace changes: a folder in place, these images rebuilt and replaced. The others are
 * read-only on the console, so a game there can't write its trace. */
const WRITABLE_IMAGES: Format[] = ["exfat", "ffpkg"];

/** The free space a traced image gets for its trace by default, in MiB (core's
 * `DEFAULT_LZ4_TRACE_SPACE_MIB`). */
const DEFAULT_TRACE_MIB = 256;

const traceLabel = (mib: number) => (mib >= 1024 ? `${num(mib / 1024)} GiB` : `${num(mib)} MiB`);

export type Lz4Action = TraceAct | "pack" | "unpack";

/** Why a scenario doesn't apply to this source; null when it does. `archive`: the source is
 * an archive, which jobs unpack first: Pack/Unpack only, nothing to trace in place. */
function scenarioWhy(s: Scenario, lz4: Lz4Facts | null, archive = false): string | null {
  const t = msg().lz4;
  // An AMPR game without its own ampr_emu gets every action too: Pack and Patch/Unpatch put
  // the bundled runtime in (by user decision).
  if (s === "pack") return lz4?.imports_ampr || lz4?.packed ? null : t.noAmpr;
  if (archive) return t.archive;
  if (!lz4?.imports_ampr) return t.noAmpr;
  if (lz4.packed) return t.packedFirst;
  if (lz4.manifest_error !== null) return t.damaged;
  return null;
}

/** The scenario a new source starts on: Pack/Unpack whenever it is usable, else Trace. */
function defaultScenario(lz4: Lz4Facts | null, archive = false): Scenario | null {
  return SCENARIOS.find((s) => scenarioWhy(s, lz4, archive) === null) ?? null;
}

/** Pack's targets, in picker order: the LZ4 packed folder, then the plain images it packs
 * straight into. Unpack's: a folder or the same images. The tab offers no .ffpfsc or .fpkg
 * (core and the CLI still do): Convert turns the folder into any format. */
const PACK_FORMATS: Format[] = ["lz4", "ffpkg", "exfat", "ffpfs"];
const UNPACK_FORMATS: Format[] = ["folder", "ffpkg", "exfat", "ffpfs"];

/** Pack's or Unpack's target: Unpack starts on ffpkg, Pack on its chosen target. */
function packTarget(lz4: Lz4Facts | null, packFormat: Format): Format {
  return lz4?.packed ? "ffpkg" : packFormat;
}

/** The source's own traces load by themselves (journal + index, recorded by Forge's trace
 * runtime): the Traces control starts locked. */
function ownTraces(lz4: Lz4Facts | null): boolean {
  return !!lz4 && lz4.runtime === "forge_trace" && lz4.journal_bytes !== null && lz4.traces_zip !== null;
}

export function Lz4(props: { jobs: Job[]; dispatch: (a: Action) => void }) {
  const t = useMessages().lz4;
  const c = useMessages().convert;
  const [source, setSource] = useState<string | null>(null);
  const [sourceIsImage, setSourceIsImage] = useState(false);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [insError, setInsError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  /** The password that opened an archive source, sent with its jobs; in memory only. */
  const [password, setPassword] = useState<string | null>(null);
  /** The scenario and Trace's action picked for this source; null: the default for it. */
  const [chosen, setChosen] = useState<Scenario | null>(null);
  const [act, setAct] = useState<TraceAct | null>(null);
  /** Unpack's target. */
  const [format, setFormat] = useState<Format>("ffpkg");
  /** Pack's target: an image (default .ffpkg) or the LZ4 packed folder. */
  const [packFormat, setPackFormat] = useState<Format>("ffpkg");
  const [fullVerify, setFullVerify] = useState(false);
  /** The free space a traced image gets, 64 MiB to 1 GiB; kept across sources. */
  const [traceMib, setTraceMib] = useState(DEFAULT_TRACE_MIB);
  /** Pack's rule sources, one or the other: a TOML profile, or traces copied from the console
   * (a zip, or a folder with both files). Per game, so a new source drops them. */
  const [profile, setProfile] = useState<string | null>(null);
  const [traces, setTraces] = useState<string | null>(null);
  /** The source's own traces are overridden: the Traces control is unlocked. */
  const [unlocked, setUnlocked] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  /** The last folder patch or unpatch on this screen. */
  const [patched, setPatched] = useState<{ source: string; act: TraceAct; result: Lz4Patch } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false); // state lags a fast double click
  const started = useRef(new Set<JobId>());
  /** Jobs seen unfinished; one that finishes is checked against the source once. */
  const live = useRef(new Set<JobId>());
  const sourceSeq = useRef(0);

  const lz4 = ins?.lz4 ?? null;
  const kind = ins?.kind ?? (sourceIsImage ? "" : "folder");
  const archive = isArchive(kind);
  const why = (s: Scenario) =>
    !source ? t.chooseFirst : !ins ? t.notRead : ins.additional_content ? t.noEboot : scenarioWhy(s, lz4, archive);
  const usable = SCENARIOS.filter((s) => why(s) === null);
  const scenario: Scenario | null =
    chosen && usable.includes(chosen) ? chosen : ins && !ins.additional_content ? defaultScenario(lz4, archive) : null;
  const packed = !!lz4?.packed;
  const traceAct: TraceAct = act ?? (lz4?.runtime === "forge_trace" ? "unpatch" : "patch");
  const writableImage = sourceIsImage && WRITABLE_IMAGES.includes(kind as Format);
  const readOnly = scenario === "trace" && sourceIsImage && !!ins && !writableImage;
  const lockable = ownTraces(lz4);
  const locked = lockable && !unlocked;
  // The target in effect: Trace rebuilds an image as itself, Pack writes the LZ4 packed
  // folder, Unpack any format.
  const fmt: Format =
    scenario === "trace"
      ? writableImage ? (kind as Format) : "folder"
      : packed ? (format === "lz4" ? "folder" : format) : packFormat;
  const out = useOutput(fmt, props.jobs, setError);

  // Read the source (again after a patch: its LZ4 facts changed); `name`: name the output
  // for the scenario the source starts on. `pw`: the password that opened it before (an
  // archive asks for one when it needs it).
  const inspectSource = async (path: string, name: boolean, pw: string | null) => {
    setInspecting(true);
    const seq = ++sourceSeq.current;
    try {
      const current = () => seq === sourceSeq.current;
      const { ins: found, password: opened } = await readSource(path, pw, current);
      if (seq !== sourceSeq.current) return;
      setIns(found);
      setInsError(null); // an earlier read may have failed (a busy console)
      setPassword(opened);
      if (!name) return;
      const facts = found.lz4 ?? null;
      const target = packTarget(facts, packFormat);
      if (facts?.packed) setFormat(target);
      out.opened(path, defaultScenario(facts, isArchive(found.kind)) === "pack" ? target : null, opened);
    } catch (e) {
      if (seq === sourceSeq.current) setInsError(errorText(e));
    } finally {
      if (seq === sourceSeq.current) setInspecting(false);
    }
  };

  // A job that read or wrote this source (a Patch or Unpatch replacing it, or any job writing
  // to its path) has finished: read it again. Every job seen unfinished counts, also one
  // restored from the server after a reload, not only those started here.
  useEffect(() => {
    let again = false;
    for (const j of props.jobs) {
      if (!j.result) {
        live.current.add(j.id);
        continue;
      }
      if (!live.current.delete(j.id)) continue;
      if (source !== null && (j.request?.source === source || j.request?.output === source)) again = true;
    }
    if (again && source !== null) void inspectSource(source, false, password);
  }, [props.jobs, source]);

  // Edit or the Library deleted this source (or the folder holding it): forget it, and any late answer.
  useEffect(
    () =>
      onDeleted((gone) => {
        if (source === null || !isWithin(source, gone)) return;
        sourceSeq.current++;
        setSource(null);
        setIns(null);
        setPassword(null);
        setInsError(null);
        setInspecting(false);
        setPatched(null);
        setError(null);
      }),
    [source],
  );

  const pickSource = async (path: string, isImage: boolean) => {
    setSource(path);
    setSourceIsImage(isImage);
    setChosen(null);
    setAct(null);
    setProfile(null);
    setTraces(null);
    setUnlocked(false);
    setProfileOpen(false);
    setPatched(null);
    setIns(null);
    setPassword(null);
    setInsError(null);
    setError(null);
    out.reset(path, null);
    await inspectSource(path, true, null);
  };

  const chooseScenario = (s: Scenario) => {
    setChosen(s);
    setError(null);
    if (s !== "pack") return;
    const target = packTarget(lz4, packFormat);
    if (packed) setFormat(target);
    out.retarget(source, target);
  };

  const pickFormat = (f: Format) => {
    setFormat(f);
    out.retarget(source, f);
  };

  const pickPackFormat = (f: Format) => {
    setPackFormat(f);
    out.retarget(source, f);
  };

  // A file or folder on the machine that runs the job (http build: the server's browser).
  // "either": the http picker's zip-or-folder mode.
  const chooseRules = async (what: "profile" | "zip" | "folder" | "either") => {
    const choice = out.choiceSeq.current;
    const p = await pick(
      what === "profile"
        ? { directory: false, title: t.pickProfile, filterName: t.profileFilter, extensions: ["toml"] }
        : what === "folder"
          ? { directory: true, title: t.pickTracesFolder }
          : {
              directory: false,
              folderToo: what === "either",
              title:
                what === "either" ? t.pickTracesEither : t.pickTracesZip,
              filterName: t.pickTracesZip,
              extensions: ["zip"],
            },
    );
    if (p === null || choice !== out.choiceSeq.current) return;
    setProfile(what === "profile" ? p : null);
    setTraces(what === "profile" ? null : p);
  };
  // Back to the source's own traces: a profile would still win over them, so it goes too.
  const relock = () => {
    setUnlocked(false);
    setTraces(null);
    setProfile(null);
  };

  const pending = running(props.jobs).length;
  const hard = ins ? ins.findings.filter((l) => classify(l).kind === "block") : [];
  const folderTrace = scenario === "trace" && !sourceIsImage;
  const imageTrace = scenario === "trace" && writableImage;
  const job = scenario === "pack" || imageTrace;
  const action: Lz4Action = scenario === "trace" ? traceAct : packed ? "unpack" : "pack";
  const replacing = props.jobs.some(
    (j) => !j.result && j.request?.lz4_in_place && j.request.source === source,
  );
  const blockReason = insError
    ? t.unreadable
    : folderTrace && pending > 0
      ? t.pending(traceAct)
      : imageTrace && replacing
        ? t.replacing
        : (job || folderTrace) && hard.length > 0
          ? t.blocked(action, hard[0], hard.length - 1)
          : null;

  // Trace on a folder: one request, not a job.
  const runFolder = async () => {
    if (!source || !folderTrace || blockReason || submittingRef.current) return;
    setError(null);
    setPatched(null);
    submittingRef.current = true;
    setSubmitting(true);
    const src = source;
    const a = traceAct;
    const seq = sourceSeq.current; // a new source meanwhile drops the answer
    try {
      const result = a === "patch" ? await api.lz4Patch(src) : await api.lz4Unpatch(src);
      if (seq !== sourceSeq.current) return;
      setPatched({ source: src, act: a, result });
      void inspectSource(src, false, null);
    } catch (e) {
      if (seq !== sourceSeq.current) return;
      setError(errorText(e));
      // A patch that failed part way may still have changed the folder: show what it holds now.
      void inspectSource(src, false, null);
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  const { target, named } = out;
  // Packing: `lz4: "pack"` into the folder or the image picked.
  const packing = scenario === "pack" && !packed;
  const makeRequest = (source: string): ConvertRequest => ({
    source,
    format: packing && fmt === "lz4" ? "folder" : fmt,
    // Rebuilt in place: core writes a .part beside the source and derives the rest.
    output: imageTrace ? source : target,
    compression_threads: null,
    inner: null,
    remove_backport: false,
    full_verify: fullVerify,
    lz4: imageTrace
      ? traceAct === "patch"
        ? "trace"
        : "unpatch"
      : packed
        ? "unpack"
        : packing
          ? "pack"
          : null,
    // Only what applies: core refuses rules off the LZ4 target, and its default trace space
    // (1 GiB) stands when nothing is traced into an image.
    ...(imageTrace && { lz4_in_place: true }),
    ...(imageTrace && traceAct === "patch" && { lz4_trace_space_mib: traceMib }),
    ...(packing && profile !== null && { lz4_profile: profile }),
    ...(packing && !locked && traces !== null && { lz4_traces: traces }),
    ...(password !== null && { archive_password: password }),
  });
  const start = async () => {
    if (!source || !job || blockReason || submittingRef.current) return;
    if (!imageTrace && (!target || named.error || collides)) return;
    setError(null);
    const request = makeRequest(source);
    submittingRef.current = true;
    setSubmitting(true);
    const choice = out.choiceSeq.current;
    try {
      const id = await api.startJob(request);
      started.current.add(id);
      props.dispatch({ type: "started", id, request });
      if (!imageTrace) out.next(source, request.output, choice);
      // A double click's second press lands once the job started: the button rests a moment.
      await new Promise((r) => setTimeout(r, 1000));
    } catch (e) {
      setError(errorText(e));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  const patchDone = folderTrace && patched?.source === source ? patched : null;
  const sameJobs = props.jobs.filter(
    (j) =>
      !j.result && !j.request?.lz4_in_place && !j.request?.edits?.length && j.request?.source === source && j.request.format === fmt,
  );
  const outputJob = scenario === "pack";
  // Any unfinished job writing this path, from any source.
  const collides = !out.generate && props.jobs.some((j) => !j.result && j.request?.output === target);
  const dupe = outputJob && sameJobs.length > 0;
  const formatWarn =
    ins && job ? ins.findings.filter((l) => classify(l).formats?.includes(fmt)).length : 0;
  const unusable = SCENARIOS.filter((s) => why(s) !== null);
  const hasJobs = props.jobs.length > 0;
  const showButton = folderTrace || job;
  const journal = lz4?.journal_bytes ?? null;
  // What the action does to the game's runtime and index (hidden while blocked). Pack/Unpack:
  // core's pack_runtime_action (install, replace or keep; null for a game that doesn't use
  // AMPR). Patch/Unpatch put their own runtime in, so only a game without one hears about it.
  // A stale, damaged or missing ampr_emu.index is written afresh (a packed game keeps its own).
  const shipped = lz4?.shipped_runtime_version;
  const packAct = lz4?.pack_runtime_action ?? null;
  const runtimeNote =
    blockReason || !showButton || !lz4 || !shipped
      ? null
      : scenario === "pack"
        ? packAct === "install"
          ? packing
            ? t.installsRuntime("pack", shipped)
            : c.amprRuntime("install", null, shipped)
          : packAct
            ? c.amprRuntime(
                packAct === "replace" && lz4.runtime === "forge_trace" ? "trace" : packAct,
                lz4.runtime === "other" ? (lz4.runtime_version ?? null) : null,
                shipped,
              )
            : null
        : lz4.runtime === "none" && lz4.imports_ampr
          ? t.installsRuntime(traceAct, shipped)
          : null;
  const indexNote =
    !blockReason && showButton && !packed && lz4?.index && lz4.index !== "ok"
      ? t.freshIndex(lz4.index, scenario === "pack" ? "pack" : traceAct)
      : null;

  return (
    <div className={hasJobs ? "screen with-jobs" : "screen"}>
      <div className="cols">
        <SourceCard
          path={source}
          onPick={pickSource}
          busy={inspecting}
          error={insError}
          ins={ins}
          prefix="l"
          empty={<Lz4Empty onPick={pickSource} />}
          onUnlock={() => source && void pickSource(source, sourceIsImage)}
        />

        <section className="card" aria-labelledby="l-lz4">
          <CardHead icon="compress" title={t.title} id="l-lz4">
            <button type="button" className="small help-btn" onClick={openLz4Help}>
              <Icon name="info" />
              {t.help}
            </button>
          </CardHead>
          <SegPicker
            name="l-scenario"
            label={t.scenario}
            options={SCENARIOS}
            labels={t.scenarios}
            value={scenario}
            onChange={chooseScenario}
            why={why}
          />
          {scenario && (
            <p className="muted desc">
              <Lead note={{ text: t.scenarioInfo(scenario, packed) }} />
            </p>
          )}
          {!source ? (
            <p className="muted hint lz4-why">{t.chooseGame}</p>
          ) : ins && usable.length === 0 ? (
            <p className="note-line warn lz4-why">
              <Icon name="warn" />
              <span>{ins.additional_content ? t.additionalContent : t.noAmprGame}</span>
            </p>
          ) : (
            ins &&
            unusable.length > 0 && (
              <p className="muted hint lz4-why">{t.unavailable(unusable.map((s) => `${t.scenarios[s]} (${why(s)})`))}</p>
            )
          )}
          {readOnly && (
            <p className="note-line warn lz4-why">
              <Icon name="warn" />
              <span>{t.readOnly(kindLabel(kind))}</span>
            </p>
          )}
          {scenario === "trace" && !readOnly && ins && source && (
            <>
              {journal !== null && <RecordedTraces source={source} ins={ins} />}
              <span className="label target-label" aria-hidden="true">{t.action}</span>
              <SegPicker
                name="l-trace-act"
                label={t.traceAction}
                className="inner"
                options={TRACE_ACTS}
                labels={t.actions}
                value={traceAct}
                onChange={(a) => {
                  setAct(a);
                  setError(null);
                }}
                why={() => null}
              />
              <p className="muted desc">{t.traceInfo(traceAct, sourceIsImage)}</p>
              <div className="field lz4">
                <p className="muted hint">{t.changes(traceAct, sourceIsImage)}</p>
                {traceAct === "patch" && <p className="muted hint">{t.replay(web)}</p>}
                {traceAct === "patch" && <FakelibWarning />}
                {traceAct === "patch" && sourceIsImage && (
                  <>
                    <label className="label range-label" htmlFor="l-trace-mib">
                      <span>{t.room}</span>
                      <span className="range-value">{traceLabel(traceMib)}</span>
                    </label>
                    <input
                      id="l-trace-mib"
                      type="range"
                      className="range"
                      min={64}
                      max={1024}
                      step={64}
                      value={traceMib}
                      onChange={(e) => setTraceMib(Number(e.target.value))}
                    />
                    <p className="muted hint">{t.roomHint}</p>
                  </>
                )}
                {traceAct === "unpatch" && lz4?.runtime === "forge_release" && journal === null && (
                  <p className="muted hint">{t.alreadyRelease}</p>
                )}
                {sourceIsImage && (
                  <p className="note-line muted">
                    <Icon name="info" />
                    <span>{t.spaceHint}</span>
                  </p>
                )}
                {journal !== null && (
                  <p className="note-line warn">
                    <Icon name="warn" />
                    <span>{t.deletesTraces(traceAct, prettyBytes(journal), web)}</span>
                  </p>
                )}
              </div>
            </>
          )}
          {scenario === "pack" && ins && !packed && (
            <div className="field lz4">
              <TracesControl
                locked={locked}
                lockable={lockable}
                path={traces}
                journal={journal}
                onUnlock={() => setUnlocked(true)}
                onRelock={relock}
                onChoose={(what) => void chooseRules(what)}
                onClear={() => setTraces(null)}
              />
              <details
                className="fold rules"
                open={profileOpen}
                onToggle={(e) => setProfileOpen(e.currentTarget.open)}
              >
                <summary>{t.useProfile}</summary>
                <PathChoice
                  id="l-lz4-profile"
                  label={t.profile}
                  path={profile}
                  choices={[{ label: t.choose, onChoose: () => void chooseRules("profile") }]}
                  onClear={() => setProfile(null)}
                  hint={t.profileHint}
                />
              </details>
              <p className="muted hint">{ruleSource(profile, locked ? null : traces, journal)}</p>
            </div>
          )}
          {scenario === "pack" && ins && !packed && (
            <>
              <span className="label target-label" aria-hidden="true">{t.target}</span>
              <FormatPicker
                name="l-pack-format"
                label={t.packInto}
                value={fmt}
                onChange={pickPackFormat}
                formats={PACK_FORMATS}
                asFolder="lz4"
              />
              <p className="muted desc">
                <Lead note={formatInfo(fmt)} />
              </p>
              {fmt !== "lz4" && (
                <p className="muted hint">{t.direct}</p>
              )}
            </>
          )}
          {scenario === "pack" && lz4?.packed && (
            <>
              <span className="label target-label" aria-hidden="true">{t.target}</span>
              <FormatPicker
                name="l-target-format"
                label={t.targetFormat}
                value={fmt}
                onChange={pickFormat}
                formats={UNPACK_FORMATS}
              />
              <p className="muted desc">
                <Lead note={fmt !== "folder" ? formatInfo(fmt) : { text: t.unpackFolder }} />
              </p>
              <div className="field lz4">
                <p className="muted hint">{t.unpackHint(lz4.packed.packed_files)}</p>
              </div>
            </>
          )}
          {job && <VerifySwitch checked={fullVerify} onChange={setFullVerify} />}
          {outputJob && <OutputField out={out} source={source} prefix="l" />}
          {showButton && (
            <div className="build-row">
              {outputJob && <GenerateSwitch out={out} source={source} />}
              <button
                className="primary"
                onClick={folderTrace ? runFolder : start}
                disabled={
                  !!blockReason || submitting || (outputJob && (!target || !!named.error || collides))
                }
              >
                <Icon name="bolt" />
                {t.actions[action]}
              </button>
            </div>
          )}
          {(blockReason || formatWarn > 0 || dupe || error || patchDone || runtimeNote || indexNote) && (
            <div className="build-notes">
              {error && <p className="bad">{error}</p>}
              {patchDone && <PatchResult act={patchDone.act} result={patchDone.result} />}
              {blockReason && (
                <p className="note-line block">
                  <Icon name="alert" />
                  <span>{blockReason}</span>
                </p>
              )}
              {!blockReason && formatWarn > 0 && (
                <p className="note-line warn">
                  <Icon name="warn" />
                  <span>{t.likelyFail(kindLabel(fmt))}</span>
                </p>
              )}
              {runtimeNote && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>{runtimeNote}</span>
                </p>
              )}
              {indexNote && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>{indexNote}</span>
                </p>
              )}
              {dupe && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>
                    {collides
                      ? t.collides(action, fmt === "folder" || fmt === "lz4")
                      : t.already(action, kindLabel(fmt))}
                  </span>
                </p>
              )}
            </div>
          )}
        </section>
      </div>

      {hasJobs && <JobsCard jobs={props.jobs} dispatch={props.dispatch} started={started} prefix="l" />}
    </div>
  );
}

/** A one-of-N choice as a segmented group in the LZ4 tint (native radios underneath); an
 * option that doesn't apply is disabled, its reason read out and in its tooltip. */
function SegPicker<T extends string>(props: {
  name: string;
  label: string;
  options: T[];
  labels: Record<T, string>;
  value: T | null;
  onChange: (v: T) => void;
  why: (v: T) => string | null;
  className?: string;
}) {
  return (
    <div className={`seg formats ${props.className ?? ""}`} role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => {
        const why = props.why(o);
        return (
          <label
            key={o}
            className={`seg-choice lz4-y${props.value === o ? " on" : ""}`}
            title={why ?? undefined}
          >
            <input
              type="radio"
              className="visually-hidden"
              name={props.name}
              value={o}
              checked={props.value === o}
              disabled={why !== null}
              onChange={() => props.onChange(o)}
            />
            <span className="seg-opt">
              {props.labels[o]}
              {why && <span className="visually-hidden">, {why}</span>}
            </span>
          </label>
        );
      })}
    </div>
  );
}

/** Where Pack's rules come from, in core's order: a profile, traces chosen here, this
 * source's own traces, else a built-in guess. */
function ruleSource(profile: string | null, traces: string | null, journalBytes: number | null): string {
  const t = msg().lz4;
  if (profile !== null) return t.profileRules;
  if (traces !== null) return t.chosenRules;
  if (journalBytes !== null) return t.ownRules;
  return t.guessRules;
}

const INDEX = "ampr_emu.index";

/** Trace, a source with a journal: what was recorded. The web build offers the traces as one
 * zip from the server (the route streams it out of a folder or any image), to pack the
 * original dump on a computer; the app has them at hand for Pack/Unpack. */
function RecordedTraces(props: { source: string; ins: Inspection }) {
  const t = useMessages().lz4;
  const zip = props.ins.lz4?.traces_zip ?? null;
  const index = props.ins.files.find((f) => f.path === INDEX)?.size ?? 0;
  const total = (props.ins.lz4?.journal_bytes ?? 0) + index;
  return (
    <div className="field lz4 recorded">
      <span className="label">{t.recorded}</span>
      {!web ? (
        <p className="muted hint">{t.recordedHint(prettyBytes(props.ins.lz4?.journal_bytes ?? 0))}</p>
      ) : (
        <p className="muted hint">{t.downloadHint}</p>
      )}
      {!web ? null : zip !== null ? (
        <div className="row downloads">
          <a
            className="button"
            href={`/api/lz4_traces?source=${encodeURIComponent(props.source)}`}
            download={zip}
          >
            <Icon name="download" />
            <span>{t.download(prettyBytes(total), zip)}</span>
          </a>
        </div>
      ) : (
        <p className="note-line warn">
          <Icon name="warn" />
          <span>{t.missingIndex}</span>
        </p>
      )}
      <p className="muted hint">{t.truncatedJournal}</p>
      <FakelibWarning recorded />
    </div>
  );
}

/** Before and after tracing: a fakelib updater puts its own libSceAmpr.sprx back over the
 * trace runtime, so nothing is recorded. */
function FakelibWarning({ recorded = false }: { recorded?: boolean }) {
  const t = useMessages().lz4;
  return (
    <p className="note-line warn">
      <Icon name="warn" />
      <span>{t.fakelibWarning(recorded)}</span>
    </p>
  );
}

/** The LZ4 tab's empty Source: one yellow Choose source… (a game folder, or an image or archive, asked
 * first: the native dialogs pick one kind) and what to expect. */
function Lz4Empty({ onPick }: { onPick: (path: string, isImage: boolean) => void }) {
  const t = useMessages().lz4;
  const c = useMessages().common;
  return (
    <div className="empty lz4">
      <span className="empty-icon">
        <Icon name="compress" />
      </span>
      <p className="empty-title">{t.emptyTitle}</p>
      <p className="muted">
        <Lead note={{ tone: "warn", text: t.experimental }} />
      </p>
      <div className="row center">
        <ChooseMenu
          id="l-choose-source"
          className="lz4-btn"
          label={
            <>
              <Icon name="folder" />
              {t.chooseSource}
            </>
          }
          menuLabel={t.source}
          items={[
            { id: "folder", icon: "folder", label: c.pickFolderButton },
            { id: "image", icon: "archive", label: c.pickImageButton },
          ]}
          onChoose={(what) => void openPicker(what, onPick)}
        />
      </div>
    </div>
  );
}

/** Pack's Traces: one control for a zip or a folder. Locked (greyed, a lock icon) while the
 * source's own traces load by themselves; the lock unlocks it, and "Use the source's traces"
 * locks it again and drops the choice. */
function TracesControl(props: {
  locked: boolean;
  lockable: boolean;
  path: string | null;
  journal: number | null;
  onUnlock: () => void;
  onRelock: () => void;
  onChoose: (what: "zip" | "folder" | "either") => void;
  onClear: () => void;
}) {
  const t = useMessages().lz4;
  const id = "l-lz4-traces";
  // The lock or the link that had focus goes away: hand it to what replaces it.
  const was = useRef(props.locked);
  useEffect(() => {
    if (was.current !== props.locked && document.activeElement === document.body)
      document.getElementById(props.locked ? `${id}-lock` : `${id}-choose`)?.focus();
    was.current = props.locked;
  }, [props.locked]);

  if (props.locked)
    return (
      <div className="field" role="group" aria-labelledby={`${id}-label`}>
        <div className="label-row">
          <span className="label" id={`${id}-label`}>{t.traces}</span>
        </div>
        <div className="lock-box">
          <button
            type="button"
            id={`${id}-lock`}
            className="lock-btn"
            onClick={props.onUnlock}
            title={t.unlock}
            aria-label={t.unlock}
          >
            <Icon name="lock" />
          </button>
          <span>{t.loaded}</span>
        </div>
        <p className="muted hint">{t.lockedHint(prettyBytes(props.journal ?? 0))}</p>
      </div>
    );
  return (
    <div className="field" role="group" aria-labelledby={`${id}-label`}>
      <div className="label-row">
        <span className="label" id={`${id}-label`}>
          {props.lockable ? t.traces : t.optionalTraces}
        </span>
        <span className="row nowrap">
          {props.path !== null && (
            <button className="small" onClick={props.onClear}>
              {t.clear}
            </button>
          )}
          {web ? (
            <button className="small" id={`${id}-choose`} onClick={() => props.onChoose("either")}>
              {t.choose}
            </button>
          ) : (
            <ChooseMenu
              id={`${id}-choose`}
              label={t.choose}
              menuLabel={t.traces}
              items={[
                { id: "zip", icon: "file", label: t.zip },
                { id: "folder", icon: "folder", label: t.folder },
              ]}
              onChoose={props.onChoose}
            />
          )}
        </span>
      </div>
      {props.path !== null && <p className="out-box path">{props.path}</p>}
      <p className="muted hint">{t.tracesHint}</p>
      {props.lockable && (
        <button type="button" className="link" onClick={props.onRelock}>
          <Icon name="lock" />
          {t.useOwn}
        </button>
      )}
    </div>
  );
}

/** An optional file for Pack (the profile): its choose button and Clear, the whole path
 * shown. */
function PathChoice(props: {
  id: string;
  label: string;
  path: string | null;
  choices: { label: string; onChoose: () => void }[];
  onClear: () => void;
  hint?: string;
}) {
  const t = useMessages().lz4;
  return (
    <div className="field" role="group" aria-labelledby={`${props.id}-label`}>
      <div className="label-row">
        <span className="label" id={`${props.id}-label`}>
          {props.label}
        </span>
        <span className="row nowrap">
          {props.path !== null && (
            <button className="small" onClick={props.onClear}>
              {t.clear}
            </button>
          )}
          {props.choices.map((c) => (
            <button key={c.label} className="small" onClick={c.onChoose}>
              {c.label}
            </button>
          ))}
        </span>
      </div>
      {props.path !== null && <p className="out-box path">{props.path}</p>}
      {props.hint && <p className="muted hint">{props.hint}</p>}
    </div>
  );
}

/** What a folder Patch or Unpatch did, and the runtime's known issue. */
function PatchResult({ act, result }: { act: TraceAct; result: Lz4Patch }) {
  const t = useMessages().lz4;
  return (
    <>
      <p className="note-line good">
        <Icon name="checkCircle" />
        <span>{t.patchResult(act, result.indexed)}</span>
      </p>
      {result.removed.length > 0 && (
        <p className="note-line muted">
          <Icon name="info" />
          <span>{t.removed(result.removed)}</span>
        </p>
      )}
      {result.warning && (
        <p className="note-line warn">
          <Icon name="warn" />
          <span>{result.warning}.</span>
        </p>
      )}
    </>
  );
}
