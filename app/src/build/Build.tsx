// Build: the source, the steps in STEPS (steps.ts), the summary and one Build button; this file only folds the steps.

import { useEffect, useMemo, useRef, useState } from "react";

import { api, errorText, type ConvertRequest, type EstimateKind, type InspectFile, type Inspection, type JobId, type KrakenLevel, type PackEstimate } from "../api";
import { LEVEL_OF, TARGETS } from "./formats";
import { classify, Dots, isWithin, onDeleted, Prose, type SourceSelection, SourceCard, usePeerPath } from "../common";
import { num, useMessages } from "../i18n";
import { Icon } from "../icons";
import { jobType, type Action, type Job } from "../jobs";
import { getPrefs } from "../prefs";
import { inspectSource } from "../password";
import { useOutput } from "../target";
import { backportStep } from "./backport";
import { optionsStep } from "./options";
import { packsStep, ResultSlot, type PacksState } from "./packs";
import "./build.css";
import type { AnyStep, BuildCtx, DeriveCtx, SummaryLine } from "./steps";

/** In screen order: a step reads the facts of those before it. */
const STEPS: AnyStep[] = [packsStep, backportStep, optionsStep];

const KRAKEN_LEVEL: Record<KrakenLevel, number> = { stored: 0, fast: 1, balanced: 2, smallest: 3 };

/** What core's `pack_estimate` compresses for this build: the packs made, else the whole tree for an fpkg or ffpfsc. */
function estimateOf(
  d: { packKind: string | null; unpackKind: string | null; format: string },
  p: PacksState,
): { kind: EstimateKind; level: number; unpack: boolean; sizeOnly?: boolean } | null {
  const unpack = d.unpackKind !== null;
  // ponytail: no estimate with custom rules (they pick other files); pass the profile to `pack_estimate` if that matters.
  if ((d.packKind === "zstd" || d.packKind === "lz4") && p.rules !== null) return null;
  if (d.packKind === "zstd" || d.packKind === "lz4") return { kind: d.packKind, level: LEVEL_OF[d.packKind][p.levels[d.packKind]], unpack: false };
  if (d.format === "pkg") return { kind: "kraken", level: KRAKEN_LEVEL[p.krakenLevel], unpack };
  if (d.format === "ffpfsc") return { kind: "zlib", level: p.ffpfscLevel, unpack };
  // An unpack into a target that doesn't compress needs only the decoded size. ponytail: zlib 0 still samples 64 MiB.
  if (unpack) return { kind: "zlib", level: 0, unpack, sizeOnly: true };
  return null;
}

/** Decimal GB (one decimal under 10), MB under 1 GB. */
function size(bytes: number): string {
  if (bytes < 1e9) return `${num(Math.max(1, Math.round(bytes / 1e6)))} MB`;
  const tenths = Math.round(bytes / 1e8) / 10;
  return `${tenths < 10 ? num(tenths, 1) : num(Math.round(bytes / 1e9))} GB`;
}

const MIB = 2 ** 20;
const BLOCK = 64 * 1024;

/** What rounding each file up to 64 KiB (image clusters and blocks, LZ4 pages) adds, plus a block per folder. */
function slackOf(files: InspectFile[], emptyDirs: string[]): number {
  const dirs = new Set(emptyDirs);
  for (const f of files) for (let i = f.path.lastIndexOf("/"); i > 0; i = f.path.lastIndexOf("/", i - 1)) dirs.add(f.path.slice(0, i));
  return files.reduce((s, f) => s + ((BLOCK - (f.size % BLOCK)) % BLOCK), (dirs.size + 1) * BLOCK);
}

/** About an image's size as its writer plans it: `alloc` the files in 64 KiB units, `entries` files
 * and folders. fpkg and ffpfsc as if nothing compressed (core's preflight sizes them so).
 * ponytail: each writer's metadata approximated a little over; ask core for its plan if it matters. */
function imageBytes(format: string, inner: string, alloc: number, entries: number): number {
  if (format === "exfat") {
    const t = alloc * 1.0003 + entries * 256 + 33 * MIB; // FAT, bitmap, entries, fixed metadata
    return t + Math.min(512 * MIB, Math.max(64 * MIB, t / 200));
  }
  if (format === "ffpkg") return alloc * 1.005 + entries * 256 + 72 * MIB; // inode tables, groups, 1024 free blocks
  if (format === "ffpfsc") return imageBytes(inner, inner, alloc, entries) * 1.0002 + 8 * BLOCK; // + block table
  if (format === "pkg") return (imageBytes("ffpfs", inner, alloc, entries) + alloc / 512 + entries * 600) * 1.01 + 3 * MIB;
  return alloc + entries * 1024 + MIB; // ffpfs
}

/** About the free space core's preflight asks for: a folder (packs at worst stored raw, an LZ4 chunk
 * a whole page, + the indexes) its data + 2% + 64 MiB, an image its planned size + 64 MiB.
 * ponytail: an unpack counts the packed listing's files and a custom profile's smaller chunks aren't
 * seen, so a pack of many tiny files can be under-asked; core's preflight still refuses before writing. */
function neededBytes(format: string, inner: string, data: number, slack: number, entries: number, pack: "zstd" | "lz4" | null): number {
  if (format === "folder" || format === "lz4") return ((pack === "lz4" ? data + slack : data) + (pack ? entries * 256 : 0)) * 1.02 + 64 * MIB;
  return imageBytes(format, inner, data + slack, entries) + 64 * MIB;
}

/** The game as it is stored now (`onDisk`: a .pkg's or archive's file, an image's; `source` is its files) beside
 * about what the build weighs, and a gauge between them: its full height the larger of the two. */
function ResultPanel(props: { source: number; onDisk: number; same: boolean; est: PackEstimate | null; estimating: boolean; sizeOnly: boolean; rules: boolean; layout?: (data: number) => number }) {
  const bm = useMessages().build;
  const { source, onDisk, est } = props;
  // Into its own format: about the size it has now (an estimate would only add sampling noise).
  const data = props.same ? onDisk : props.rules || !est ? (props.estimating ? null : source) : props.sizeOnly ? est.logical_bytes : est.estimated_bytes;
  // An exfat, ffpkg or ffpfs image: the data as its writer lays it out (blocks, metadata, spare).
  const out = data !== null && !props.same && props.layout ? props.layout(data) : data;
  const top = Math.max(onDisk, out ?? 0);
  const fill = out === null || top === 0 ? 0 : (out / top) * 100;
  const over = out !== null && out > onDisk;
  return (
    <div className="b-result" aria-live="polite">
      <div className="b-track" aria-hidden="true">
        <div className={over ? "b-fill over" : "b-fill"} style={{ height: `${fill}%` }} />
        {over && top > 0 && <div className="b-tick" style={{ bottom: `${(onDisk / top) * 100}%` }} />}
      </div>
      <div className="b-col">
        <span className="b-k">{bm.resultNow}</span>
        <span className="b-now">{size(onDisk)}</span>
        <div className="grow" />
        <span className="b-k">{bm.resultTitle}</span>
        {out === null ? (
          <>
            <span className="b-big muted">
              <Dots />
            </span>
            <span className="b-k">{bm.estimate}</span>
          </>
        ) : (
          <span className="b-big">{size(out)}</span>
        )}
        {props.rules && <span className="tag">{bm.resultCustomRules}</span>}
      </div>
    </div>
  );
}

/** Jobs that take the one heavy slot (convert, pack, edit, extract): a new build waits behind them. */
const heavy = (j: Job) => !j.result && jobType(j.request) !== "transfer" && jobType(j.request) !== "install";

type States = Record<string, unknown>;
const initialStates = (): States => Object.fromEntries(STEPS.map((s) => [s.id, s.initial]));

function baseCtx(source: string | null, sourceIsImage: boolean, ins: Inspection | null): DeriveCtx {
  return {
    source,
    sourceIsImage,
    ins,
    packs: "keep",
    unpackKind: null,
    packKind: null,
    formats: TARGETS,
    defaultFormat: "pkg",
    formatDisabled: () => undefined,
    format: "pkg",
    removeBackport: false,
  };
}

function derive(base: DeriveCtx, states: States): { ctx: DeriveCtx; active: AnyStep[] } {
  let ctx = base;
  const active: AnyStep[] = [];
  for (const s of STEPS) {
    if (!s.available(ctx)) continue;
    active.push(s);
    if (s.context) ctx = { ...ctx, ...s.context(ctx, states[s.id]) };
  }
  return { ctx, active };
}

function summaryOf(active: AnyStep[], ctx: BuildCtx, states: States): SummaryLine[] {
  return active.flatMap((s) => s.summary(ctx, states[s.id]) as SummaryLine[]);
}

export function Build(props: {
  jobs: Job[];
  dispatch: (a: Action) => void;
  /** This tab is the one shown: a naming change renames its output only then. */
  active: boolean;
  /** A game the File explorer opened here; each new `id` is read once. */
  selection?: SourceSelection | null;
  /** Open this source in the Edit tab (a patch package, which only Edit reads). */
  onEdit?: (path: string) => void;
  onCompare: () => void;
  onQueue?: () => void;
}) {
  const t = useMessages().convert;
  const bm = useMessages().build;
  const shown = usePeerPath();
  const [source, setSource] = useState<string | null>(null);
  const [sourceIsImage, setSourceIsImage] = useState(false);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [insError, setInsError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  /** The password that opened an archive source; in memory only. */
  const [password, setPassword] = useState<string | null>(null);
  const [states, setStates] = useState<States>(initialStates);
  const statesNow = useRef(states);
  statesNow.current = states;
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false); // state lags a fast double click
  // Only the latest pick may land (an older inspect can finish last).
  const sourceSeq = useRef(0);

  const { ctx: derived, active } = useMemo(
    () => derive(baseCtx(source, sourceIsImage, ins), states),
    [source, sourceIsImage, ins, states],
  );
  const out = useOutput(derived.format, props.jobs, setError, derived.packKind, props.active);
  const ctx: BuildCtx = { ...derived, jobs: props.jobs, out };

  // The (format, pack kind) the output was last named for: a change names it again.
  const named = useRef<string | null>(null);
  const keyOf = (c: DeriveCtx) => `${c.format}|${c.packKind ?? ""}`;

  const setStep = (id: string, patch: object) => {
    setError(null);
    setStates((s) => ({ ...s, [id]: { ...(s[id] as object), ...patch } }));
  };

  // Read the source; `name`: name the output once it is read (a first read, or an unlock).
  const open = async (path: string, isImage: boolean, name: boolean, pw: string | null) => {
    setInsError(null);
    setInspecting(true);
    const seq = ++sourceSeq.current;
    try {
      const opened = await inspectSource(path, pw, () => seq === sourceSeq.current);
      if (seq !== sourceSeq.current) return;
      setIns(opened.ins);
      setPassword(opened.password);
      if (name || opened.password !== null) {
        const c = derive(baseCtx(path, isImage, opened.ins), statesNow.current).ctx;
        named.current = keyOf(c);
        out.opened(path, c.format, opened.password, c.packKind);
      }
    } catch (e) {
      if (seq === sourceSeq.current) setInsError(errorText(e));
    } finally {
      if (seq === sourceSeq.current) setInspecting(false);
    }
  };

  const pickSource = async (path: string, isImage: boolean) => {
    const base = baseCtx(path, isImage, null);
    setSource(path);
    setSourceIsImage(isImage);
    setIns(null);
    setPassword(null);
    setError(null);
    setStates((s) => {
      const next = { ...s };
      for (const st of STEPS) if (st.onSource) next[st.id] = st.onSource(base, s[st.id]);
      statesNow.current = next;
      return next;
    });
    named.current = null;
    out.reset(path, null);
    await open(path, isImage, true, null);
  };

  // A format or pack kind changed (a step's choice, or a re-read source): name the output again.
  useEffect(() => {
    if (!source || !ins) return;
    const key = keyOf(derived);
    if (named.current === key) return;
    named.current = key;
    out.retarget(source, derived.format, derived.packKind);
  }, [source, ins, derived.format, derived.packKind]);

  // A new Storage selection: the same path as a picker choice (resets the form for it).
  const pickRef = useRef(pickSource);
  pickRef.current = pickSource;
  const consumed = useRef(0);
  useEffect(() => {
    const sel = props.selection;
    if (!sel || sel.id === consumed.current) return;
    consumed.current = sel.id;
    void pickRef.current(sel.path, sel.kind !== "folder");
  }, [props.selection]);

  // No source: forget it and any late answer; `steps`: every step back to its start for a new source.
  const clear = (old: string, steps: boolean) => {
    sourceSeq.current++;
    setSource(null);
    setSourceIsImage(false);
    setIns(null);
    setPassword(null);
    setInsError(null);
    setInspecting(false);
    setError(null);
    named.current = null;
    out.reset(old, null);
    if (steps)
      setStates((s) => {
        const base = baseCtx(null, false, null);
        const next = { ...s };
        for (const st of STEPS) next[st.id] = st.onSource ? st.onSource(base, s[st.id]) : s[st.id];
        statesNow.current = next;
        return next;
      });
  };

  // Edit or the File explorer deleted this source (or the folder holding it).
  useEffect(
    () =>
      onDeleted((gone) => {
        if (source === null || !isWithin(source, gone)) return;
        clear(source, false);
      }),
    [source],
  );

  // A job that changed this source (an edit) or wrote its path has finished: read it again.
  // Every job seen unfinished counts, also one started on another tab.
  const live = useRef(new Set<JobId>());
  useEffect(() => {
    let again = false;
    for (const j of props.jobs) {
      if (!j.result) {
        live.current.add(j.id);
        continue;
      }
      if (!live.current.delete(j.id)) continue;
      if (source === null) continue;
      if ((j.request?.source === source && j.request.edits?.length) || j.request?.output === source) again = true;
    }
    if (again && source !== null) void open(source, sourceIsImage, false, password);
  }, [props.jobs, source]);

  // Why Build can't start: each of the source's own, then each step's (the card lists none: all are here).
  const hard = ins ? ins.findings.filter((l) => classify(l).kind === "block") : [];
  const blocks: string[] = ins?.patch
    ? [t.patchBlocked]
    : [...hard.map((h) => t.blocked(h, 0)), ...active.flatMap((s) => s.findings?.(ctx, states[s.id]) ?? [])];
  // The source's other findings lead the summary: warnings in warn, news muted.
  const sourceLines: SummaryLine[] = (ins?.patch ? [] : (ins?.findings ?? []))
    .map((l) => ({ l, kind: classify(l).kind }))
    .filter((f) => f.kind !== "block")
    .map((f) => ({ text: f.l, tone: f.kind === "warn" ? "warn" : "muted" }));
  const summary: SummaryLine[] = ins ? [...sourceLines, ...summaryOf(active, ctx, states)] : [];

  const { target } = out;

  // Free space in the output folder, asked 300 ms after it (or a finished job) last changed; null: unknown.
  const finished = props.jobs.filter((j) => j.result).length;
  const [free, setFree] = useState<{ dir: string; bytes: number } | null>(null);
  const outDir = out.dir;
  useEffect(() => {
    if (!outDir) return;
    let gone = false;
    const timer = setTimeout(() => {
      api
        .freeSpace(outDir)
        .then((bytes) => !gone && setFree(bytes === null ? null : { dir: outDir, bytes }))
        .catch(() => !gone && setFree(null));
    }, 300);
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [outDir, finished]);
  // Core's estimate, asked 500 ms after the choice last changed; only the latest answer lands.
  // A source into its own format (pkg -> pkg, an image rebuilt) keeps about its size: nothing to estimate.
  // Stored (Kraken stored, zlib 0) is no longer "about the same": every block is raw, so it is estimated.
  const p0 = states["packs"] as PacksState;
  const stored = (derived.format === "pkg" && p0.krakenLevel === "stored") || (derived.format === "ffpfsc" && p0.ffpfscLevel === 0);
  const same = !!ins && ins.kind === derived.format && derived.unpackKind === null && derived.packKind === null && ins.file_bytes != null && !stored;
  const want = ins && source && !same ? estimateOf(derived, states["packs"] as PacksState) : null;
  const wantKey = want && source ? JSON.stringify([source, want.kind, want.level, want.unpack, password]) : null;
  const [estimate, setEstimate] = useState<{ key: string; value: PackEstimate | null } | null>(null);
  const estimateSeq = useRef(0);
  useEffect(() => {
    const seq = ++estimateSeq.current;
    if (!want || !source || !wantKey) return;
    const timer = setTimeout(() => {
      api
        .packEstimate({ source, kind: want.kind, level: want.level, unpack: want.unpack, password })
        .then((value) => seq === estimateSeq.current && setEstimate({ key: wantKey, value }))
        .catch(() => seq === estimateSeq.current && setEstimate({ key: wantKey, value: null }));
    }, 500);
    return () => clearTimeout(timer);
  }, [wantKey]);
  const settled = !want || estimate?.key === wantKey;
  const est = estimate && estimate.key === wantKey ? estimate.value : null;
  const fmt = derived.format;
  const slack = useMemo(() => (ins ? slackOf(ins.files, ins.empty_dirs) : 0), [ins]);
  const entries = ins ? ins.files.length + ins.empty_dirs.length : 0;
  const inner = p0.inner;
  // A plain image (no packs made or decoded: the listing is what is written), or a stored container.
  const plain = derived.packKind === null && derived.unpackKind === null;
  const laidOut = fmt === "exfat" || fmt === "ffpkg" || fmt === "ffpfs" || stored;
  const layout = plain && laidOut ? (d: number) => imageBytes(fmt, inner, d + slack, entries) : undefined;
  const panel = ins ? (
    <ResultPanel
      layout={layout}
      source={ins.total_bytes}
      onDisk={ins.file_bytes ?? ins.total_bytes}
      same={same}
      est={est}
      estimating={!settled}
      sizeOnly={!!want?.sizeOnly}
      rules={!!derived.packKind && (states["packs"] as PacksState).rules !== null}
    />
  ) : null;
  // An unpack's decoded size is known only from the estimate: until then (or if it fails) no figure.
  // Packs are sized as stored raw (core measures them first; a sample can't promise they fit).
  const data = est ? est.logical_bytes : want?.unpack ? undefined : ins?.total_bytes;
  const needed = ins && data !== undefined ? neededBytes(fmt, inner, data, slack, entries, derived.packKind) : null;
  // The panel shows a compressed size but core checks room for the uncompressed one.
  const raw = !!est && est.estimated_bytes < est.logical_bytes * 0.95 && (!!derived.packKind || fmt === "pkg" || fmt === "ffpfsc");
  const freeHere = free && free.dir === outDir ? free.bytes : null;

  const queue = props.jobs.some(heavy);
  const [queued, setQueued] = useState(false);
  useEffect(() => {
    if (!queued) return;
    const timer = setTimeout(() => setQueued(false), 6000);
    return () => clearTimeout(timer);
  }, [queued]);
  const ready = !!source && !!ins && !!target && !out.named.error && blocks.length === 0 && !inspecting;
  const build = async () => {
    if (!ready || !source || submittingRef.current) return;
    setError(null);
    const base: ConvertRequest = {
      source,
      format: "folder",
      output: target,
      compression_threads: null,
      inner: null,
      remove_backport: false,
      full_verify: false,
      lz4: null,
      replace_ampr_emu: getPrefs().replaceAmprEmu,
      ...(password !== null && { archive_password: password }),
    };
    const request = active.reduce<ConvertRequest>((r, s) => s.request(ctx, states[s.id], r), base);
    submittingRef.current = true;
    setSubmitting(true);
    const choice = out.choiceSeq.current;
    const seq = sourceSeq.current;
    try {
      const waits = queue;
      const id = await api.startJob(request);
      props.dispatch({ type: "started", id, request });
      setQueued(waits);
      // Started: the page empties for the next game.
      if (seq === sourceSeq.current) clear(source, true);
      else out.next(source, request.output, choice);
    } catch (e) {
      if (seq === sourceSeq.current) setError(errorText(e));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };
  // An unreadable source fails its naming too: Source already says why.
  const shownError = insError ? null : error;

  return (
    <div className="screen build">
      <div className="build-steps">
        <SourceCard
          path={source}
          onPick={(path, isImage) => void pickSource(path, isImage)}
          busy={inspecting}
          error={insError}
          ins={ins}
          prefix="b"
          findings={false}
          onUnlock={() => source && void open(source, sourceIsImage, true, null)}
        />
        {queued && (
          <p className="muted build-queued" role="status">
            <Icon name="check" />
            <span>{bm.queued}</span>
            {props.onQueue && (
              <button type="button" className="link" onClick={props.onQueue}>
                {bm.openQueue}
              </button>
            )}
          </p>
        )}
        {ins &&
          active.map((s) => (
            <section key={s.id} className="card build-step" aria-labelledby={`b-step-${s.id}`}>
              <div className="build-step-head">
                <h3 id={`b-step-${s.id}`}>{s.title()}</h3>
                {s.id === "packs" && (
                  <button type="button" className="small b-head-btn" onClick={props.onCompare}>
                    <Icon name="table" />
                    {t.compare}
                  </button>
                )}
              </div>
              <ResultSlot.Provider value={s.id === "packs" ? panel : null}>
                <s.Body ctx={ctx} state={states[s.id]} set={(patch: object) => setStep(s.id, patch)} />
              </ResultSlot.Provider>
            </section>
          ))}
        {ins && (
          <section className="card build-step" aria-labelledby="b-step-summary">
            <div className="build-step-head">
              <h3 id="b-step-summary">{bm.summary}</h3>
            </div>
            {summary.length > 0 && (
              <ul className="build-summary">
                {summary.map((l, i) => (
                  <li key={i} className={l.tone}>
                    {typeof l.text === "string" ? <Prose text={l.text} /> : l.text}
                  </li>
                ))}
              </ul>
            )}
            {(blocks.length > 0 || shownError) && (
              <div className="build-notes">
                {shownError && (
                  <p className="bad backend" role="alert">
                    {shownError}
                  </p>
                )}
                {blocks.map((b, i) => (
                  <p key={i} className="note-line block">
                    <Icon name="alert" />
                    <span>
                      <Prose text={b} />
                    </span>
                  </p>
                ))}
                {ins?.patch && source && props.onEdit && (
                  <button type="button" className="small" onClick={() => props.onEdit?.(source)}>
                    <Icon name="pencil" />
                    {t.openInEdit}
                  </button>
                )}
              </div>
            )}
            {needed !== null && outDir && (
              <p className={`build-space ${freeHere !== null && freeHere < needed ? "warn" : "muted"}`}>
                <Icon name={freeHere !== null && freeHere < needed ? "warn" : "info"} />
                <span>
                  {freeHere !== null ? bm.space(size(freeHere), shown(outDir), size(needed)) : bm.needed(size(needed))}
                  {raw && <> {bm.spaceRaw}</>}
                </span>
              </p>
            )}
            <div className="build-row">
              <button
                className="primary"
                onClick={() => void build()}
                disabled={!ready}
                // aria-disabled while busy, so it keeps focus.
                aria-disabled={submitting || undefined}
                aria-busy={submitting || undefined}
              >
                <Icon name={queue ? "plus" : "bolt"} />
                {queue ? bm.addToQueue : t.build}
                {submitting && <Dots />}
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
