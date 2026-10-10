// LZ4: ampr_emu asset packs for a title that uses AMPR. Two scenarios: Trace (Patch or Unpatch
// the source itself: a folder in place by one request, an .exfat/.ffpkg by a job that rebuilds
// it and replaces it once verified) and Pack/Unpack (an LZ4 packed folder or plain image from
// traces, a profile or a guess; Unpack for a packed source). Jobs join the shared list below.

import { useEffect, useRef, useState } from "react";

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
  type Lz4PlanProfile,
} from "./api";
import {
  CardHead,
  ChooseMenu,
  classify,
  FormatPicker,
  isWithin,
  kindLabel,
  onDeleted,
  pickSource as openPicker,
  Prose,
  SourceCard,
} from "./common";
import { openLz4Help } from "./Lz4Help";
import { Icon } from "./icons";
import { JobsCard } from "./JobList";
import { dirname } from "./paths";
import type { Action, Job } from "./jobs";
import {
  FORMAT_INFO,
  GenerateSwitch,
  Lead,
  OutputField,
  running,
  useOutput,
  VerifySwitch,
} from "./target";

type Scenario = "trace" | "pack";
const SCENARIOS: Scenario[] = ["trace", "pack"];
const SCENARIO_LABEL: Record<Scenario, string> = { trace: "Trace", pack: "Pack/Unpack" };

/** What Trace does to the source. */
type TraceAct = "patch" | "unpatch";
const TRACE_ACTS: TraceAct[] = ["patch", "unpatch"];
const TRACE_LABEL: Record<TraceAct, string> = { patch: "Patch", unpatch: "Unpatch" };

/** Sources Trace changes: a folder in place, these images rebuilt and replaced. The others are
 * read-only on the console, so a game there can't write its trace. */
const WRITABLE_IMAGES: Format[] = ["exfat", "ffpkg"];

/** The free space a traced image gets for its trace by default, in MiB (core's
 * `DEFAULT_LZ4_TRACE_SPACE_MIB`). */
const DEFAULT_TRACE_MIB = 256;

const traceLabel = (mib: number) => (mib >= 1024 ? `${mib / 1024} GiB` : `${mib} MiB`);

/** Forge's release build of the AMPR runtime (what Unpatch installs). */
const RELEASE = "Forge's release runtime (0.4.2.1)";

/** One line under the scenario picker. */
function scenarioInfo(s: Scenario, packed: boolean): string {
  if (s === "trace")
    return "Makes this game record which assets it reads while you play (ampr_commands.bin), for Pack to use. Unpatch puts the release runtime back.";
  return packed
    ? "Unpack: writes the game's LZ4 packs back as plain files, into a folder or a plain image."
    : "Pack: packs the game's assets into LZ4 volumes that the bundled AMPR emulator (ampr_emu) reads while the game runs, into a folder or a plain image. A packed source is unpacked instead.";
}

/** One line under the Patch / Unpatch picker. */
function traceInfo(a: TraceAct, image: boolean): string {
  if (a === "patch")
    return image
      ? "Rebuilds this image with the trace runtime and room for the trace, then replaces it."
      : "Installs the trace runtime in this folder where it is: no copy is written.";
  return image
    ? `Rebuilds this image with ${RELEASE}, without the recorded traces, then replaces it.`
    : `Puts ${RELEASE} back in this folder where it is: no copy is written.`;
}

/** Why a scenario doesn't apply to this source; null when it does. */
function scenarioWhy(s: Scenario, lz4: Lz4Facts | null): string | null {
  if (s === "pack") return lz4?.imports_ampr || lz4?.packed ? null : "the game doesn't use AMPR";
  if (!lz4?.imports_ampr) return "the game doesn't use AMPR";
  if (lz4.packed) return "the source is LZ4 packed: unpack it first";
  if (lz4.manifest_error !== null) return "its LZ4 packs are damaged";
  return null;
}

/** The scenario a new source starts on. A packed one starts on Pack (Unpack). The desktop app
 * has a traced source's files at hand, so it starts on Pack; the web build on Trace, which
 * offers them as a zip. A read-only image starts on Pack: Trace can't change it. */
function defaultScenario(lz4: Lz4Facts | null, kind: string): Scenario | null {
  const writable = kind === "folder" || WRITABLE_IMAGES.includes(kind as Format);
  const order: Scenario[] =
    lz4?.packed || !writable || (!web && lz4?.journal_bytes != null) ? ["pack", "trace"] : ["trace", "pack"];
  return order.find((s) => scenarioWhy(s, lz4) === null) ?? null;
}

/** Pack's targets, in picker order: the LZ4 packed folder, then the plain images it packs
 * straight into. Unpack's: a folder or the same images. The tab offers no .ffpfsc or .fpkg
 * (core and the CLI still do): Convert turns the folder into any format. */
const PACK_FORMATS: Format[] = ["lz4", "ffpkg", "exfat", "ffpfs"];
const UNPACK_FORMATS: Format[] = ["folder", "ffpkg", "exfat", "ffpfs"];
const ANY_FORMAT = "Need ffpfsc or fpkg? Write a folder here, then convert it to any format on the Convert tab.";

/** Pack's or Unpack's target: Unpack starts on a folder, Pack on its chosen target. */
function packTarget(lz4: Lz4Facts | null, packFormat: Format): Format {
  return lz4?.packed ? "folder" : packFormat;
}

/** The source's own traces load by themselves (journal + index, recorded by Forge's trace
 * runtime): the Traces control starts locked. */
function ownTraces(lz4: Lz4Facts | null): boolean {
  return !!lz4 && lz4.runtime === "forge_trace" && lz4.journal_bytes !== null && lz4.traces_zip !== null;
}

export function Lz4(props: { jobs: Job[]; dispatch: (a: Action) => void }) {
  const [source, setSource] = useState<string | null>(null);
  const [sourceIsImage, setSourceIsImage] = useState(false);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [insError, setInsError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  /** The scenario and Trace's action picked for this source; null: the default for it. */
  const [chosen, setChosen] = useState<Scenario | null>(null);
  const [act, setAct] = useState<TraceAct | null>(null);
  /** Unpack's target. */
  const [format, setFormat] = useState<Format>("folder");
  /** Pack's target: the LZ4 packed folder or an image. */
  const [packFormat, setPackFormat] = useState<Format>("lz4");
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
  /** Save as profile: running, and what it saved for which source. */
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<{ source: string; result: Lz4PlanProfile } | null>(null);
  const submittingRef = useRef(false); // state lags a fast double click
  const started = useRef(new Set<JobId>());
  /** Jobs seen unfinished; one that finishes is checked against the source once. */
  const live = useRef(new Set<JobId>());
  const sourceSeq = useRef(0);

  const lz4 = ins?.lz4 ?? null;
  const kind = ins?.kind ?? (sourceIsImage ? "" : "folder");
  const why = (s: Scenario) =>
    !source ? "choose a source first" : !ins ? "the source isn't read yet" : scenarioWhy(s, lz4);
  const usable = SCENARIOS.filter((s) => why(s) === null);
  const scenario: Scenario | null =
    chosen && usable.includes(chosen) ? chosen : ins ? defaultScenario(lz4, kind) : null;
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
  // for the scenario the source starts on.
  const inspectSource = async (path: string, name: boolean) => {
    setInspecting(true);
    const seq = ++sourceSeq.current;
    try {
      const found = await api.inspect(path);
      if (seq !== sourceSeq.current) return;
      setIns(found);
      if (!name) return;
      const facts = found.lz4 ?? null;
      const target = packTarget(facts, packFormat);
      if (facts?.packed) setFormat(target);
      if (defaultScenario(facts, found.kind) === "pack") out.reset(path, target);
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
    if (again && source !== null) void inspectSource(source, false);
  }, [props.jobs, source]);

  // Inspect deleted this source (or the folder holding it): forget it, and any late answer.
  useEffect(
    () =>
      onDeleted((gone) => {
        if (source === null || !isWithin(source, gone)) return;
        sourceSeq.current++;
        setSource(null);
        setIns(null);
        setInsError(null);
        setInspecting(false);
        setPatched(null);
        setSaved(null);
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
    setSaved(null);
    setIns(null);
    setInsError(null);
    setError(null);
    out.reset(path, null);
    await inspectSource(path, true);
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
        ? { directory: false, title: "LZ4 profile", filterName: "LZ4 profile (TOML)", extensions: ["toml"] }
        : what === "folder"
          ? { directory: true, title: "LZ4 traces (a folder with ampr_commands.bin and ampr_emu.index)" }
          : {
              directory: false,
              folderToo: what === "either",
              title:
                what === "either" ? "LZ4 traces: the zip, or a folder with both files" : "LZ4 traces (zip)",
              filterName: "LZ4 traces (zip)",
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
  const verb = scenario === "trace" ? TRACE_LABEL[traceAct] : packed ? "Unpack" : "Pack";
  const replacing = props.jobs.some(
    (j) => !j.result && j.request?.lz4_in_place && j.request.source === source,
  );
  const v = verb.toLowerCase();
  const blockReason = insError
    ? "Can't start: this source couldn't be read (see Source)."
    : folderTrace && pending > 0
      ? `Can't ${v} while a job is running or queued: ${v} once they have finished.`
      : imageTrace && replacing
        ? "A job is already rebuilding this image: wait for it to finish."
        : (job || folderTrace) && hard.length > 0
          ? `Can't ${v}: ${hard[0]}${hard.length > 1 ? ` (and ${hard.length - 1} more, see Source)` : ""}.`
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
      void inspectSource(src, false);
    } catch (e) {
      if (seq === sourceSeq.current) setError(errorText(e));
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
  });
  const start = async () => {
    if (!source || !job || blockReason || submittingRef.current) return;
    if (!imageTrace && (!target || named.error)) return;
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
    } catch (e) {
      setError(errorText(e));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  // Save as profile: the plan Pack would use, with the current settings, as a TOML.
  const savePlan = async () => {
    if (!source || !packing || blockReason || pending > 0 || saving) return;
    setError(null);
    setSaved(null);
    setSaving(true);
    const src = source;
    const seq = sourceSeq.current;
    try {
      const result = await api.savePlanProfile(makeRequest(src), profileName(ins), dirname(src));
      if (result && seq === sourceSeq.current) setSaved({ source: src, result });
    } catch (e) {
      if (seq === sourceSeq.current) setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const patchDone = folderTrace && patched?.source === source ? patched : null;
  const savedHere = packing && saved?.source === source ? saved.result : null;
  const sameJobs = props.jobs.filter(
    (j) => !j.result && !j.request?.lz4_in_place && j.request?.source === source && j.request.format === fmt,
  );
  const outputJob = scenario === "pack";
  const collides = !out.generate && sameJobs.some((j) => j.request?.output === target);
  const dupe = outputJob && sameJobs.length > 0;
  const formatWarn =
    ins && job ? ins.findings.filter((l) => classify(l).formats?.includes(fmt)).length : 0;
  const unusable = SCENARIOS.filter((s) => why(s) !== null);
  const hasJobs = props.jobs.length > 0;
  const showButton = folderTrace || job;
  const journal = lz4?.journal_bytes ?? null;

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
        />

        <section className="card" aria-labelledby="l-lz4">
          <CardHead icon="compress" title="LZ4 asset packs" id="l-lz4">
            <button type="button" className="small help-btn" onClick={openLz4Help}>
              <Icon name="info" />
              How LZ4 works
            </button>
          </CardHead>
          <SegPicker
            name="l-scenario"
            label="LZ4 scenario"
            options={SCENARIOS}
            labels={SCENARIO_LABEL}
            value={scenario}
            onChange={chooseScenario}
            why={why}
          />
          {scenario && (
            <p className="muted desc">
              <Lead text={scenarioInfo(scenario, packed)} />
            </p>
          )}
          {!source ? (
            <p className="muted hint lz4-why">Choose a game folder or image first.</p>
          ) : ins && usable.length === 0 ? (
            <p className="note-line warn lz4-why">
              <Icon name="warn" />
              <span>
                Nothing to do here: this game doesn't use AMPR (its eboot.bin doesn't import
                libSceAmpr), and it isn't LZ4 packed.
              </span>
            </p>
          ) : (
            ins &&
            unusable.length > 0 && (
              <p className="muted hint lz4-why">
                Not for this source:{" "}
                {unusable.map((s) => `${SCENARIO_LABEL[s]} (${why(s)})`).join("; ")}.
              </p>
            )
          )}
          {readOnly && (
            <p className="note-line warn lz4-why">
              <Icon name="warn" />
              <span>
                Can't patch or unpatch <b>{kindLabel(kind)}</b>: it is read-only on the console, so
                the game can't write its trace there. Convert it to <b>ffpkg</b> (or <b>exfat</b>) on
                the Convert tab, then patch that.
              </span>
            </p>
          )}
          {scenario === "trace" && !readOnly && ins && source && (
            <>
              {journal !== null && <RecordedTraces source={source} ins={ins} />}
              <span className="label target-label" aria-hidden="true">
                Action
              </span>
              <SegPicker
                name="l-trace-act"
                label="Trace action"
                className="inner"
                options={TRACE_ACTS}
                labels={TRACE_LABEL}
                value={traceAct}
                onChange={(a) => {
                  setAct(a);
                  setError(null);
                }}
                why={() => null}
              />
              <p className="muted desc">{traceInfo(traceAct, sourceIsImage)}</p>
              <div className="field lz4">
                {traceAct === "patch" ? (
                  <p className="muted hint">
                    Changes this {sourceIsImage ? "image" : "folder"}: the trace runtime
                    replaces fakelib/libSceAmpr.sprx (no backup kept) and a fresh ampr_emu.index
                    lists its files.
                  </p>
                ) : (
                  <p className="muted hint">
                    Changes this {sourceIsImage ? "image" : "folder"}: {RELEASE} replaces
                    fakelib/libSceAmpr.sprx (no backup kept), ampr_commands.bin and the trace logs
                    are deleted, and a fresh ampr_emu.index lists its files.
                  </p>
                )}
                {traceAct === "patch" && (
                  <p className="muted hint">
                    Play, then pick this source again: Pack/Unpack uses the traces it recorded
                    {web && ", and Trace offers them as a zip for a computer"}. Each launch
                    overwrites the previous session's trace.
                  </p>
                )}
                {traceAct === "patch" && <FakelibWarning />}
                {traceAct === "patch" && sourceIsImage && (
                  <>
                    <label className="label range-label" htmlFor="l-trace-mib">
                      <span>Room for the trace</span>
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
                    <p className="muted hint">
                      Free space added to the image for the game to write its trace into. Mount
                      the image writable with ShadowMountPlus image_rw= or nothing is recorded:
                      reserved space alone does not make it writable.
                    </p>
                  </>
                )}
                {traceAct === "unpatch" && lz4?.runtime === "forge_release" && journal === null && (
                  <p className="muted hint">
                    This source already has the release runtime: only the index is written again.
                  </p>
                )}
                {sourceIsImage && (
                  <p className="note-line muted">
                    <Icon name="info" />
                    <span>
                      Needs free space about the image's size next to it; the image is replaced
                      only after the copy verifies.
                    </span>
                  </p>
                )}
                {journal !== null && (
                  <p className="note-line warn">
                    <Icon name="warn" />
                    <span>
                      {TRACE_LABEL[traceAct]} deletes the traces this source recorded
                      (ampr_commands.bin, {prettyBytes(journal)}):{" "}
                      {web
                        ? "download them above or pack with them first."
                        : "pack with them or copy them first."}
                    </span>
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
                <summary>Use a rules profile instead</summary>
                <PathChoice
                  id="l-lz4-profile"
                  label="Profile"
                  path={profile}
                  choices={[{ label: "Choose…", onChoose: () => void chooseRules("profile") }]}
                  onClear={() => setProfile(null)}
                  hint="A TOML file of pack rules. It replaces the traces: choosing one clears the other."
                />
              </details>
              <p className="muted hint">{ruleSource(profile, locked ? null : traces, journal)}</p>
            </div>
          )}
          {scenario === "pack" && ins && !packed && (
            <>
              <span className="label target-label" aria-hidden="true">
                Target
              </span>
              <FormatPicker
                name="l-pack-format"
                label="Pack into"
                value={fmt}
                onChange={pickPackFormat}
                formats={PACK_FORMATS}
              />
              <p className="muted desc">
                <Lead text={FORMAT_INFO[fmt]} />
              </p>
              {fmt !== "lz4" && (
                <p className="muted hint">
                  Packs into the image directly: no temporary folder; the packed files are read twice.
                </p>
              )}
              <p className="muted hint">
                <Prose text={ANY_FORMAT} />
              </p>
            </>
          )}
          {scenario === "pack" && lz4?.packed && (
            <>
              <span className="label target-label" aria-hidden="true">
                Target
              </span>
              <FormatPicker
                name="l-target-format"
                label="Target format"
                value={fmt}
                onChange={pickFormat}
                formats={UNPACK_FORMATS}
              />
              <p className="muted desc">
                <Lead
                  text={
                    fmt !== "folder"
                      ? FORMAT_INFO[fmt]
                      : "A new game folder with its LZ4 packs written back as plain files."
                  }
                />
              </p>
              <p className="muted hint">
                <Prose text={ANY_FORMAT} />
              </p>
              <div className="field lz4">
                <p className="muted hint">
                  {lz4.packed.packed_files === null
                    ? "Writes the packed files back as plain files"
                    : `Writes the ${lz4.packed.packed_files.toLocaleString()} packed file${lz4.packed.packed_files === 1 ? "" : "s"} back as plain files`}
                  ; keeps the AMPR runtime and its index.
                </p>
              </div>
            </>
          )}
          {job && <VerifySwitch checked={fullVerify} onChange={setFullVerify} />}
          {outputJob && <OutputField out={out} source={source} prefix="l" />}
          {showButton && (
            <div className="build-row">
              {outputJob && <GenerateSwitch out={out} source={source} />}
              {/* Save as profile sits left of Pack, also in the tab order. */}
              <span className="build-actions">
                {packing && (
                  <button
                    className="neon-btn"
                    onClick={savePlan}
                    disabled={!!blockReason || saving || pending > 0}
                    title={
                      pending > 0
                        ? "Wait for the running jobs to finish"
                        : "Save the files Pack would pack as an editable rules profile"
                    }
                  >
                    <Icon name="download" />
                    {saving ? "Saving…" : "Save as profile"}
                  </button>
                )}
                <button
                  className="primary"
                  onClick={folderTrace ? runFolder : start}
                  disabled={
                    !!blockReason || submitting || (outputJob && (!target || !!named.error))
                  }
                >
                  <Icon name="bolt" />
                  {verb}
                </button>
              </span>
            </div>
          )}
          {(blockReason || formatWarn > 0 || dupe || error || patchDone || savedHere) && (
            <div className="build-notes">
              {error && <p className="bad">{error}</p>}
              {savedHere && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>
                    Saved {savedHere.file_name}: {savedHere.packed.toLocaleString()} file
                    {savedHere.packed === 1 ? "" : "s"} packed, {savedHere.loose.toLocaleString()} loose. Edit
                    it and load it with Use a rules profile.
                  </span>
                </p>
              )}
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
                  <span>Likely to fail as <b>{kindLabel(fmt)}</b>: it refuses file names listed in Source.</span>
                </p>
              )}
              {dupe && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>
                    {collides
                      ? `A job is already writing this ${fmt === "folder" || fmt === "lz4" ? "folder" : "file"}: ${verb} again would fail. Pick another name.`
                      : <>Already writing this game as <b>{kindLabel(fmt)}</b>; {verb} again makes a second copy.</>}
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

/** Save as profile's offered name, as core names it: `[GAME_NAME]-[TITLE_ID]-lz4profile.toml`
 * (the title's file-safe characters; a part it lacks left out). */
function profileName(ins: Inspection | null): string {
  const title = (ins?.title_name ?? "")
    .normalize("NFC")
    .replace(/[^\p{L}\p{N} \-_.,'()&+!]/gu, "")
    .replace(/\s+/g, " ")
    .replace(/^[. ]+|[. ]+$/g, "");
  const parts = [title && `[${title}]`, ins?.title_id && `[${ins.title_id}]`].filter(Boolean);
  return [...parts, "lz4profile.toml"].join("-");
}

/** Where Pack's rules come from, in core's order: a profile, traces chosen here, this
 * source's own traces, else a built-in guess. */
function ruleSource(profile: string | null, traces: string | null, journalBytes: number | null): string {
  if (profile !== null)
    return "Packs what the profile's rules say, except the files the engine opens directly (container indexes, configs, media) — those stay loose.";
  if (traces !== null) return "Packs the files the game read in the chosen traces, except those the engine opens directly (container indexes, configs, media) — those stay loose.";
  if (journalBytes !== null)
    return "Packs the files the game read in its last traced session (this source's traces), except those the engine opens directly (container indexes, configs, media) — those stay loose.";
  return "No traces in this source: packs by a built-in guess, which is less reliable. Record traces first (Trace → Patch), or choose traces copied from the console, or a profile.";
}

const INDEX = "ampr_emu.index";

/** Trace, a source with a journal: what was recorded. The web build offers the traces as one
 * zip from the server (the route streams it out of a folder or any image), to pack the
 * original dump on a computer; the app has them at hand for Pack/Unpack. */
function RecordedTraces(props: { source: string; ins: Inspection }) {
  const zip = props.ins.lz4?.traces_zip ?? null;
  const index = props.ins.files.find((f) => f.path === INDEX)?.size ?? 0;
  const total = (props.ins.lz4?.journal_bytes ?? 0) + index;
  return (
    <div className="field lz4 recorded">
      <span className="label">Recorded traces</span>
      {!web ? (
        <p className="muted hint">
          ampr_commands.bin ({prettyBytes(props.ins.lz4?.journal_bytes ?? 0)}): Pack/Unpack loads
          these traces by itself.
        </p>
      ) : (
        <p className="muted hint">
          Close the game and wait about a minute first: while the image is mounted, its file on disk
          can lag behind what the game wrote. Use a computer's browser opened at this console's Forge
          address, then pick the zip in LZ4 → Pack/Unpack → Traces on your computer. A download fails
          if the game or a sync changes the source meanwhile: close the game and download again.
        </p>
      )}
      {!web ? null : zip !== null ? (
        <div className="row downloads">
          <a
            className="button"
            href={`/api/lz4_traces?source=${encodeURIComponent(props.source)}`}
            download={zip}
          >
            <Icon name="download" />
            <span>
              Download traces ({prettyBytes(total)}) as {zip}
            </span>
          </a>
        </div>
      ) : (
        <p className="note-line warn">
          <Icon name="warn" />
          <span>No {INDEX} in this source: Pack needs it beside ampr_commands.bin, whose file ids it names.</span>
        </p>
      )}
      <p className="muted hint">A journal cut off at the end still packs: Pack stops at its last whole record.</p>
      <FakelibWarning recorded />
    </div>
  );
}

/** Before and after tracing: a fakelib updater puts its own libSceAmpr.sprx back over the
 * trace runtime, so nothing is recorded. */
function FakelibWarning({ recorded = false }: { recorded?: boolean }) {
  return (
    <p className="note-line warn">
      <Icon name="warn" />
      <span>
        {recorded ? "Few or no traces? " : ""}Turn off any fakelib updater while tracing: it
        overwrites fakelib/libSceAmpr.sprx, and then nothing is recorded
        {recorded ? ". Turn it off, Patch again and replay." : "."}
      </span>
    </p>
  );
}

/** The LZ4 tab's empty Source: one yellow Choose source… (a game folder or an image, asked
 * first: the native dialogs pick one kind) and what to expect. */
function Lz4Empty({ onPick }: { onPick: (path: string, isImage: boolean) => void }) {
  return (
    <div className="empty lz4">
      <span className="empty-icon">
        <Icon name="compress" />
      </span>
      <p className="empty-title">Choose a game that uses AMPR</p>
      <p className="muted">
        <Lead text="Experimental: traces and LZ4 packs for ampr_emu. No guarantee the game runs afterwards; keep your original dump." />
      </p>
      <div className="row center">
        <ChooseMenu
          id="l-choose-source"
          className="lz4-btn"
          label={
            <>
              <Icon name="folder" />
              Choose source…
            </>
          }
          menuLabel="Source"
          items={[
            { id: "folder", icon: "folder", label: "Game folder…" },
            { id: "image", icon: "disc", label: "Image…" },
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
          <span className="label" id={`${id}-label`}>
            Traces
          </span>
        </div>
        <div className="lock-box">
          <button
            type="button"
            id={`${id}-lock`}
            className="lock-btn"
            onClick={props.onUnlock}
            title="Unlock to choose other traces"
            aria-label="Unlock to choose other traces"
          >
            <Icon name="lock" />
          </button>
          <span>Traces are already loaded from this source</span>
        </div>
        <p className="muted hint">
          Its own ampr_commands.bin ({prettyBytes(props.journal ?? 0)}) and {INDEX}. Click the
          lock to choose other traces instead.
        </p>
      </div>
    );
  return (
    <div className="field" role="group" aria-labelledby={`${id}-label`}>
      <div className="label-row">
        <span className="label" id={`${id}-label`}>
          {props.lockable ? "Traces" : "Traces (optional)"}
        </span>
        <span className="row nowrap">
          {props.path !== null && (
            <button className="small" onClick={props.onClear}>
              Clear
            </button>
          )}
          {web ? (
            <button className="small" id={`${id}-choose`} onClick={() => props.onChoose("either")}>
              Choose…
            </button>
          ) : (
            <ChooseMenu
              id={`${id}-choose`}
              label="Choose…"
              menuLabel="Traces"
              items={[
                { id: "zip", icon: "file", label: "Zip…" },
                { id: "folder", icon: "folder", label: "Folder…" },
              ]}
              onChoose={props.onChoose}
            />
          )}
        </span>
      </div>
      {props.path !== null && <p className="out-box path">{props.path}</p>}
      <p className="muted hint">
        Pick the [game name]-[title ID]-amprtrace.zip from Trace → Download traces, or a folder with ampr_commands.bin
        and {INDEX}.
      </p>
      {props.lockable && (
        <button type="button" className="link" onClick={props.onRelock}>
          <Icon name="lock" />
          Use the source's traces
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
  return (
    <div className="field" role="group" aria-labelledby={`${props.id}-label`}>
      <div className="label-row">
        <span className="label" id={`${props.id}-label`}>
          {props.label}
        </span>
        <span className="row nowrap">
          {props.path !== null && (
            <button className="small" onClick={props.onClear}>
              Clear
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
  return (
    <>
      <p className="note-line good">
        <Icon name="checkCircle" />
        <span>
          {act === "patch"
            ? "Patched: the trace runtime is in fakelib/libSceAmpr.sprx"
            : `Unpatched: ${RELEASE} is in fakelib/libSceAmpr.sprx`}{" "}
          and ampr_emu.index lists {result.indexed.toLocaleString()} file{result.indexed === 1 ? "" : "s"}.
        </span>
      </p>
      {result.removed.length > 0 && (
        <p className="note-line muted">
          <Icon name="info" />
          <span>Removed the old trace files: {result.removed.join(", ")}.</span>
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
