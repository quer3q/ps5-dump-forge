// Pack: game packs for a title that uses AMPR, by one of two kinds: zstd (the default) or LZ4;
// ampr-emu-ng reads both, LZ4 may run on upstream ampr_emu instead. A source
// already packed by the kind chosen gets Unpack instead. Jobs join the list below.

import { useEffect, useRef, useState } from "react";

import { msg, useMessages } from "./i18n";

import { api, errorText, type ConvertRequest, type Format, type Inspection, type JobId, type Lz4Runtime } from "./api";
import {
  ARCHIVE_KINDS,
  CardHead,
  classify,
  Dots,
  FormatPicker,
  FormatPill,
  isNas,
  isWithin,
  KINDS,
  kindLabel,
  onDeleted,
  pickSource as openPicker,
  SourceCard,
} from "./common";
import { Icon, type IconName } from "./icons";
import { inspectSource as readSource } from "./password";
import { jobType, type Action, type Job } from "./jobs";
import { openPackHelp } from "./PackHelp";
import { DeleteSourceSwitch, formatInfo, GenerateSwitch, Lead, OutputField, useOutput, VerifySwitch } from "./target";

/** The kind of packs: zstd or LZ4 (ampr_emu). */
export type Engine = "zstd" | "lz4";
const ENGINES: Engine[] = ["zstd", "lz4"];

/** What the button does: pack a plain game, or unpack one packed by the kind chosen. */
export type PackAction = "pack" | "unpack";

/** The levels, in picker order, with each kind's number (zstd 1..22; LZ4 0 or HC 3..12) and
 * icons, as Convert's fpkg compression picker. */
export type PackLevel = "fast" | "medium" | "max";
const LEVELS: PackLevel[] = ["fast", "medium", "max"];
export const LEVEL_OF: Record<Engine, Record<PackLevel, number>> = {
  zstd: { fast: 5, medium: 9, max: 19 },
  lz4: { fast: 0, medium: 9, max: 12 },
};
const LEVEL_ICON: Record<PackLevel, IconName> = { fast: "bolt", medium: "scale", max: "compress" };

/** The runtimes an LZ4 Pack can install, in picker order. */
const RUNTIMES: Lz4Runtime[] = ["ng", "ampr_emu"];

/** Targets, in picker order: a folder (LZ4's packed folder is `lz4`, shown as Folder) or
 * straight into a plain image. No .ffpfsc here (core and the CLI still take it). */
const ZSTD_PACK_FORMATS: Format[] = ["folder", "exfat", "ffpkg", "ffpfs"];
const LZ4_PACK_FORMATS: Format[] = ["lz4", "exfat", "ffpkg", "ffpfs"];
const LZ4_UNPACK_FORMATS: Format[] = ["folder", "exfat", "ffpkg", "ffpfs"];
const ZSTD_UNPACK_FORMATS: Format[] = ["folder", "exfat", "ffpkg", "ffpfs", "pkg"];

/** Packed by zstd (`ampr_zstd.index`) or by LZ4 (a sound manifest). */
const packedBy = (e: Engine, ins: Inspection | null): boolean =>
  e === "zstd" ? !!ins?.zstd_packed : !!ins?.lz4?.packed;

/** Why packs of kind `e` can't be used on this game (shown in place of the settings); null when
 * they can: Unpack for a game packed by `e`, else Pack for a plain game that uses AMPR. */
function whyNot(e: Engine, ins: Inspection | null): string | null {
  const t = msg().pack;
  if (!ins) return null;
  if (ins.patch) return t.patch;
  if (packedBy(e, ins)) return null;
  if (ins.additional_content) return t.additionalContent;
  if (!ins.lz4?.imports_ampr) return t.noAmprGame(e);
  return null;
}

export function Pack(props: { jobs: Job[]; dispatch: (a: Action) => void }) {
  const t = useMessages().pack;
  const c = useMessages().convert;
  const [source, setSource] = useState<string | null>(null);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [insError, setInsError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  /** The password that opened an archive source, sent with its jobs; in memory only. */
  const [password, setPassword] = useState<string | null>(null);
  /** The kind of packs and each target; kept across sources. */
  const [engine, setEngine] = useState<Engine>("zstd");
  const [levels, setLevels] = useState<Record<Engine, PackLevel>>({ zstd: "fast", lz4: "fast" });
  const [lz4Runtime, setLz4Runtime] = useState<Lz4Runtime>("ng");
  const [zstdPackFormat, setZstdPackFormat] = useState<Format>("folder");
  const [lz4PackFormat, setLz4PackFormat] = useState<Format>("lz4");
  const [lz4UnpackFormat, setLz4UnpackFormat] = useState<Format>("folder");
  const [zstdUnpackFormat, setZstdUnpackFormat] = useState<Format>("folder");
  const [fullVerify, setFullVerify] = useState(false);
  /** Delete the source once the job is Ok; off by default, kept across sources. */
  const [deleteSource, setDeleteSource] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false); // state lags a fast double click
  /** Jobs seen unfinished; one that finishes is checked against the source once. */
  const live = useRef(new Set<JobId>());
  const sourceSeq = useRef(0);

  /** The target for kind `e` on this game, as picked for Pack or Unpack. */
  const targetOf = (e: Engine, i: Inspection | null): Format =>
    e === "zstd"
      ? packedBy("zstd", i) ? zstdUnpackFormat : zstdPackFormat
      : packedBy("lz4", i) ? lz4UnpackFormat : lz4PackFormat;

  const lz4 = ins?.lz4 ?? null;
  const packed = packedBy(engine, ins);
  const action: PackAction = packed ? "unpack" : "pack";
  const notForGame = whyNot(engine, ins);
  const usable = !!ins && notForGame === null;
  const fmt = targetOf(engine, ins);
  const out = useOutput(fmt, props.jobs, setError);
  // This render's kind and targets, for an inspection that began on an older render.
  const kind = useRef({ engine, targetOf });
  kind.current = { engine, targetOf };

  // Read the source; `name`: name the output for the kind chosen once the game can take it.
  // `pw`: the password that opened it before (an archive asks for one when it needs it).
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
      const { engine: e, targetOf: target } = kind.current;
      out.opened(path, whyNot(e, found) === null ? target(e, found) : null, opened);
    } catch (e) {
      if (seq === sourceSeq.current) setInsError(errorText(e));
    } finally {
      if (seq === sourceSeq.current) setInspecting(false);
    }
  };

  // A job that wrote this source's path has finished: read it again. Every job seen unfinished
  // counts, also one restored from the server after a reload, not only those started here.
  useEffect(() => {
    let again = false;
    for (const j of props.jobs) {
      if (!j.result) {
        live.current.add(j.id);
        continue;
      }
      if (!live.current.delete(j.id)) continue;
      if (source !== null && j.request?.output === source) again = true;
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
        setError(null);
      }),
    [source],
  );

  const pickSource = async (path: string) => {
    setSource(path);
    setIns(null);
    setPassword(null);
    setInsError(null);
    setError(null);
    out.reset(path, null);
    await inspectSource(path, true, null);
  };

  const chooseEngine = (e: Engine) => {
    setEngine(e);
    setError(null);
    if (whyNot(e, ins) === null) out.retarget(source, targetOf(e, ins));
  };

  const pickFormat = (f: Format) => {
    if (engine === "zstd") (packed ? setZstdUnpackFormat : setZstdPackFormat)(f);
    else (packed ? setLz4UnpackFormat : setLz4PackFormat)(f);
    out.retarget(source, f);
  };

  const { target, named } = out;
  // The request's format: LZ4's packed folder is a folder with `lz4: "pack"`.
  const format: Format = fmt === "lz4" ? "folder" : fmt;
  const hard = ins ? ins.findings.filter((f) => classify(f).kind === "block") : [];
  const blockReason = insError
    ? t.unreadable
    : usable && hard.length > 0
      ? t.blocked(action, hard[0], hard.length - 1)
      : usable && format === "pkg" && !ins.content_id
        ? c.noContentId
        : null;
  // Any unfinished job writing this path, from any source.
  const collides = !out.generate && props.jobs.some((j) => !j.result && j.request?.output === target);
  const dupe = props.jobs.some(
    (j) => !j.result && jobType(j.request) === "pack" && j.request?.source === source && j.request.format === format,
  );
  const formatWarn = usable ? ins.findings.filter((f) => classify(f).formats?.includes(fmt)).length : 0;

  const makeRequest = (src: string): ConvertRequest => {
    const base: ConvertRequest = {
      source: src,
      format,
      output: target,
      compression_threads: null,
      inner: null,
      remove_backport: false,
      full_verify: fullVerify,
      // A NAS's game can't be deleted from here.
      ...(deleteSource && !isNas(src) && { delete_source: true }),
      lz4: null,
      ...(password !== null && { archive_password: password }),
    };
    const level = LEVEL_OF[engine][levels[engine]];
    if (engine === "zstd") return packed ? { ...base, zstd_unpack: true } : { ...base, zstd_pack: true, zstd_level: level };
    return packed ? { ...base, lz4: "unpack" } : { ...base, lz4: "pack", lz4_level: level, lz4_runtime: lz4Runtime };
  };
  const start = async () => {
    if (!source || !usable || blockReason || inspecting || submittingRef.current) return;
    if (!target || named.error || collides) return;
    setError(null);
    const request = makeRequest(source);
    submittingRef.current = true;
    setSubmitting(true);
    const choice = out.choiceSeq.current;
    const seq = sourceSeq.current;
    try {
      const id = await api.startJob(request);
      props.dispatch({ type: "started", id, request });
      out.next(source, request.output, choice);
      // A double click's second press lands once the job started: the button rests a moment.
      await new Promise((r) => setTimeout(r, 1000));
    } catch (e) {
      // Not under a source picked since.
      if (seq === sourceSeq.current) setError(errorText(e));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  // What the job does to the game's runtime and index (hidden while blocked). A Pack that
  // installs ampr-emu-ng says so under its settings (ngNote). LZ4 Unpack, or Pack with ampr_emu:
  // core's pack_runtime_action (install, replace or keep), and a stale, damaged or missing
  // ampr_emu.index written afresh by Pack (a packed game keeps its own).
  const shipped = lz4?.shipped_runtime_version;
  const lz4Act = lz4?.pack_runtime_action ?? null;
  const installsNg = !packed && (engine === "zstd" || lz4Runtime === "ng");
  const runtimeNote =
    blockReason || !usable || engine === "zstd" || installsNg
      ? null
      : !lz4 || !shipped || !lz4Act
          ? null
          : lz4Act === "install"
            ? packed
              ? c.amprRuntime("install", null, shipped)
              : t.installsRuntime(shipped)
            : lz4.runtime === "zstd"
              ? // A kept ampr-emu-ng is named on the source card's tile.
                lz4Act === "replace"
                ? c.amprNgReplaced(shipped)
                : null
              : c.amprRuntime(
                lz4Act === "replace" && lz4.runtime === "forge_trace" ? "trace" : lz4Act,
                lz4.runtime === "other" ? (lz4.runtime_version ?? null) : null,
                shipped,
              );
  const indexNote =
    !blockReason && usable && engine === "lz4" && !packed && lz4?.index && lz4.index !== "ok"
      ? t.freshIndex(lz4.index)
      : null;
  const formats =
    engine === "zstd"
      ? packed ? ZSTD_UNPACK_FORMATS : ZSTD_PACK_FORMATS
      : packed ? LZ4_UNPACK_FORMATS : LZ4_PACK_FORMATS;
  const folderOut = fmt === "folder" || fmt === "lz4";

  return (
    <div className="screen">
      <div className="cols">
        <SourceCard
          path={source}
          onPick={(p) => void pickSource(p)}
          busy={inspecting}
          error={insError}
          ins={ins}
          prefix="p"
          onUnlock={() => source && void pickSource(source)}
          empty={<PackEmpty onPick={(p) => void pickSource(p)} />}
        />

        <section className="card" aria-labelledby="p-pack">
          <CardHead icon="compress" title={t.title} id="p-pack">
            <button type="button" className="small" onClick={openPackHelp}>
              <Icon name="info" />
              {t.help}
            </button>
          </CardHead>
          <SegPicker name="p-engine" label={t.engine} options={ENGINES} labels={t.engines} value={engine} onChange={chooseEngine} />
          <p className="muted desc">
            <Lead note={engine === "lz4" && !packed ? { tone: "warn", text: t.info(engine, packed) } : { text: t.info(engine, packed) }} />
          </p>
          {source && notForGame && (
            <p className="note-line warn lz4-why">
              <Icon name="warn" />
              <span>{notForGame}</span>
            </p>
          )}
          {usable && (
            <>
              <span className="label target-label" id="p-format-label">{t.target}</span>
              <FormatPicker
                name="p-format"
                labelledBy="p-format-label"
                value={fmt}
                onChange={pickFormat}
                formats={formats}
                asFolder={engine === "lz4" && !packed ? "lz4" : undefined}
              />
              <p className="muted desc">
                <Lead
                  note={
                    folderOut ? (packed ? { text: t.unpackFolder } : { tone: "good", text: t.packFolder(engine) }) : formatInfo(fmt)
                  }
                />
              </p>
              {!packed && !folderOut && <p className="muted hint">{t.direct}</p>}
              {packed && (
                <div className="field lz4">
                  <p className="muted hint">{t.unpackHint(engine === "lz4" ? (lz4?.packed?.packed_files ?? null) : null)}</p>
                </div>
              )}
              {!packed && (
                <LevelField
                  engine={engine}
                  value={levels[engine]}
                  onChange={(l) => setLevels((v) => ({ ...v, [engine]: l }))}
                />
              )}
              {engine === "lz4" && !packed && (
                <RuntimeField value={lz4Runtime} onChange={setLz4Runtime} version={shipped ?? null} />
              )}
              {installsNg && (
                <p className="muted hint">
                  <Lead note={{ tone: "warn", text: t.ngNote }} />
                </p>
              )}
              <VerifySwitch checked={fullVerify} onChange={setFullVerify} />
              <DeleteSourceSwitch checked={deleteSource} onChange={setDeleteSource} nas={isNas(source)} />
              <OutputField out={out} source={source} prefix="p" />
              <div className="build-row">
                <GenerateSwitch out={out} source={source} />
                <button
                  className="primary"
                  onClick={start}
                  disabled={!!blockReason || inspecting || !target || !!named.error || collides}
                  // Busy with its own request: aria-disabled, so it keeps focus (the handler ignores clicks then).
                  aria-disabled={submitting || undefined}
                  aria-busy={submitting || undefined}
                >
                  <Icon name="bolt" />
                  {t.actions[action]}
                  {submitting && <Dots />}
                </button>
              </div>
            </>
          )}
          {(blockReason || error || formatWarn > 0 || runtimeNote || indexNote || (usable && (dupe || collides))) && (
            <div className="build-notes">
              {error && (
                <p className="bad backend" role="alert">
                  {error}
                </p>
              )}
              {blockReason && (
                <p className="note-line block">
                  <Icon name="alert" />
                  <span>{blockReason}</span>
                </p>
              )}
              {!blockReason && formatWarn > 0 && (
                <p className="note-line warn">
                  <Icon name="warn" />
                  <span>{t.likelyFail(kindLabel(format))}</span>
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
              {usable && (dupe || collides) && (
                <p className="note-line muted">
                  <Icon name="info" />
                  <span>{collides ? t.collides(action, format === "folder") : t.already(action, kindLabel(format))}</span>
                </p>
              )}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

/** A one-of-N choice as a segmented group in the Pack tint (native radios underneath). */
function SegPicker<T extends string>(props: {
  name: string;
  label: string;
  options: T[];
  labels: Record<T, string>;
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="seg formats" role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => (
        <label key={o} className={`seg-choice lz4-y${props.value === o ? " on" : ""}`}>
          <input
            type="radio"
            className="visually-hidden"
            name={props.name}
            value={o}
            checked={props.value === o}
            onChange={() => props.onChange(o)}
          />
          <span className="seg-opt">{props.labels[o]}</span>
        </label>
      ))}
    </div>
  );
}

/** Pack's level: Fast / Medium / Max, built as Convert's fpkg compression picker. */
function LevelField(props: { engine: Engine; value: PackLevel; onChange: (l: PackLevel) => void }) {
  const t = useMessages().pack;
  const tt = useMessages().target;
  return (
    <div className="field inner">
      {/* The radio group's own label says the same to a screen reader. */}
      <span className="label" aria-hidden="true">
        {tt.compression}
      </span>
      <div className="seg formats inner" role="radiogroup" aria-label={tt.compressionLevel}>
        {LEVELS.map((l) => (
          <label key={l} className={`seg-choice fmt-pkg${props.value === l ? " on" : ""}`}>
            <input
              type="radio"
              className="visually-hidden"
              name="p-level"
              value={l}
              checked={props.value === l}
              onChange={() => props.onChange(l)}
            />
            <span className="seg-opt">
              <Icon name={LEVEL_ICON[l]} />
              {t.levels[l]}
            </span>
          </label>
        ))}
      </div>
      <p className="muted hint">
        <Lead note={t.levelNotes[props.engine][props.value]} />
      </p>
    </div>
  );
}

/** LZ4 Pack's runtime: ampr-emu-ng (default) or the bundled upstream ampr_emu. */
function RuntimeField(props: { value: Lz4Runtime; onChange: (r: Lz4Runtime) => void; version: string | null }) {
  const t = useMessages().pack;
  const label = (r: Lz4Runtime) => (r === "ng" ? "ampr-emu-ng" : props.version ? `ampr_emu ${props.version}` : "ampr_emu");
  return (
    <div className="field inner">
      <span className="label" aria-hidden="true">
        {t.runtime}
      </span>
      <div className="seg formats inner" role="radiogroup" aria-label={t.runtime}>
        {RUNTIMES.map((r) => (
          <label key={r} className={`seg-choice fmt-pkg${props.value === r ? " on" : ""}`}>
            <input
              type="radio"
              className="visually-hidden"
              name="p-lz4-runtime"
              value={r}
              checked={props.value === r}
              onChange={() => props.onChange(r)}
            />
            <span className="seg-opt">{label(r)}</span>
          </label>
        ))}
      </div>
      <p className="muted hint">
        <Lead note={t.runtimeNotes[props.value]} />
      </p>
    </div>
  );
}

/** Pack's empty source card: the shared prompt and accepted formats in Pack's yellow. */
function PackEmpty({ onPick }: { onPick: (path: string) => void }) {
  const c = useMessages().common;
  return (
    <div className="empty lz4">
      <span className="empty-icon">
        <Icon name="compress" />
      </span>
      <p className="empty-title">{c.emptyTitle}</p>
      <div className="kinds" role="group" aria-label={c.accepted}>
        {[...KINDS, ...ARCHIVE_KINDS].map((k) => (
          <FormatPill key={k} kind={k} />
        ))}
      </div>
      <div className="row center">
        <button type="button" id="p-choose-source" className="lz4-btn" onClick={() => void openPicker((p) => onPick(p))}>
          <Icon name="disc" />
          {c.pickButton}
        </button>
      </div>
    </div>
  );
}
