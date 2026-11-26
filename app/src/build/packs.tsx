// Build's Target step: the packs (None, zstd, LZ4), the format, the output; Build.tsx's result panel beside them.

import { createContext, Fragment, useContext, useState, type ReactNode } from "react";

import type { Format, InnerFormat, KrakenLevel } from "../api";
import { AC_TARGETS, LEVEL_OF, LZ4_PACK_FORMATS, TARGETS, ZSTD_PACK_FORMATS, type Engine, type PackLevel } from "./formats";
import { basename, classify, kindLabel, usePeerPath } from "../common";
import { msg, useMessages } from "../i18n";
import { Icon, type IconName } from "../icons";
import { jobType } from "../jobs";
import { Advanced, getPrefs, usePrefs } from "../prefs";
import { openPackHelp } from "../PackHelp";
import { DEFAULT_FFPFSC_LEVEL, GenerateSwitch, OutputField, type Output } from "../target";
import { InnerField, KrakenLevelField, PackLevelField, RulesPick, ZlibLevelField } from "./fields";
import { defineStep, type BuildCtx, type DeriveCtx, type PackMode, type PacksChoice, type SummaryLine } from "./steps";

const engineName = (e: Engine) => (e === "zstd" ? "ZSTD" : "LZ4");

export interface PacksState {
  choice: PacksChoice;
  levels: Record<Engine, PackLevel>;
  /** A TOML pack-rules file, sent with a pack only. */
  rules: string | null;
  format: Format;
  /** The image inside a `.ffpfsc`; kept while other targets are picked. */
  inner: InnerFormat;
  krakenLevel: KrakenLevel;
  ffpfscLevel: number;
}

/** The formats shown before More (`lz4`: LZ4's packed folder). */
const BASIC: Format[] = ["folder", "lz4", "pkg"];

/** The request's format: LZ4's packed folder is a folder with `lz4: "pack"`. */
const requestFormat = (f: Format): Format => (f === "lz4" ? "folder" : f);

function OutputLine({ out }: { out: Output }) {
  const shown = usePeerPath();
  const name = out.generate ? out.name : out.named.name;
  const bm = useMessages().build;
  if (!out.output || !name) return <>{bm.naming}</>;
  return bm.output(name, shown(out.dir));
}

const sameJobs = (c: BuildCtx) =>
  c.jobs.filter(
    (j) =>
      !j.result &&
      j.request?.source === c.source &&
      j.request.format === requestFormat(c.format) &&
      (jobType(j.request) === "convert" || jobType(j.request) === "pack"),
  );
const collides = (c: BuildCtx) =>
  !c.out.generate && c.jobs.some((j) => !j.result && j.request?.output === c.out.target);

const zstdPacked = (c: DeriveCtx) => !!c.ins?.zstd_packed;
const lz4Packed = (c: DeriveCtx) => !!c.ins?.lz4?.packed;

/** A patch takes only None (Build refuses it); additional content is never packed (core refuses it). */
function offered(c: DeriveCtx): PacksChoice[] {
  const ins = c.ins;
  if (!ins || ins.patch || ins.additional_content) return ["none"];
  return ["none", "zstd", "lz4"];
}

const effective = (c: DeriveCtx, s: PacksState): PacksChoice => (offered(c).includes(s.choice) ? s.choice : "none");

/** The game's own packs (zstd first, as core reads them). */
const ownPacks = (c: DeriveCtx): Engine | null => (zstdPacked(c) ? "zstd" : lz4Packed(c) ? "lz4" : null);

/** The packs rule: packs are kept only when the chosen kind is the game's own. */
function modeOf(c: DeriveCtx, choice: PacksChoice): PackMode {
  // Additional content takes no LZ4 or zstd option (core refuses it): its packs go as they are.
  if (c.ins?.additional_content) return "keep";
  const own = ownPacks(c);
  if (choice === "none") return own ? "unpack" : "keep";
  return choice === own ? "keep" : choice;
}

const packing = (mode: PackMode): mode is Engine => mode === "zstd" || mode === "lz4";

const levelLabel = (e: Engine, l: PackLevel) => msg().build.levelLabel(msg().pack.levels[l], engineName(e), LEVEL_OF[e][l]);

/** What the job does to the game's AMPR runtime (AGENTS.md's runtime rule). */
function runtimeLine(c: DeriveCtx, mode: PackMode, replaceAmprEmu: boolean): string | null {
  const bm = msg().build;
  const lz4 = c.ins?.lz4 ?? null;
  const rt = lz4?.runtime ?? "none";
  const ngOnly = packing(mode) || (mode === "keep" && zstdPacked(c));
  if (requestFormat(c.format) === "pkg") return lz4?.imports_ampr || rt !== "none" || ngOnly ? bm.pkgNoRuntime : null;
  if (!ngOnly && !lz4?.imports_ampr && rt !== "forge_trace") return null;
  const v = lz4?.runtime_version ? ` ${lz4.runtime_version}` : "";
  if (rt === "none") return bm.ngInstalled;
  if (rt === "zstd") {
    const action =
      mode === "keep" ? lz4?.runtime_action : mode === "unpack" ? lz4?.unpack_runtime_action : lz4?.pack_runtime_action;
    return action === "replace" ? bm.ngUpdated(v) : bm.ngSame(v);
  }
  if (rt === "forge_trace") return bm.ngReplacedTrace;
  if (ngOnly || replaceAmprEmu) return bm.ngReplaced(`ampr_emu${v}`);
  // An LZ4-packed game kept as it is: core still replaces an ampr_emu that reads no packs.
  const kept = mode !== "keep" || (lz4?.keep_runtime_action ?? "keep") === "keep";
  return kept ? bm.ngKept(`ampr_emu${v}`) : bm.ngReplaced(`ampr_emu${v}`);
}

function indexNote(c: DeriveCtx): string | null {
  const t = msg().convert;
  const lz4 = c.ins?.lz4 ?? null;
  if (!lz4?.imports_ampr || lz4.packed || zstdPacked(c)) return null;
  if (requestFormat(c.format) === "pkg") return null; // the builder indexes what it packages
  const index = getPrefs().replaceAmprEmu ? lz4.index : (lz4.keep_index ?? lz4.index);
  // Remove backport leaves fakelib files out before core compares the index, so an ok one goes stale.
  const state = index === "ok" && c.removeBackport ? "stale" : index;
  return state && state !== "ok" ? t.amprIndex(state) : null;
}

const keepsOwn = (c: DeriveCtx, choice: PacksChoice): boolean =>
  !c.ins?.additional_content && ownPacks(c) !== null && choice === ownPacks(c);

/** A `.pkg` carries no packs (issue #73): keeping the game's own moves an fpkg output elsewhere. */
function pickChoice(c: DeriveCtx, k: PacksChoice): Partial<PacksState> {
  if (!keepsOwn(c, k) || requestFormat(c.format) !== "pkg") return { choice: k };
  const folderSource = c.source !== null && !c.sourceIsImage;
  return { choice: k, format: folderSource ? "exfat" : "folder" };
}

/** Into an fpkg a game keeping its own packs is unpacked (issue #73): the choice becomes None. */
function pickFormat(c: DeriveCtx, s: PacksState, f: Format): Partial<PacksState> {
  return requestFormat(f) === "pkg" && keepsOwn(c, effective(c, s)) ? { format: f, choice: "none" } : { format: f };
}

/** The packs facts for the chip that applies (before the format is picked). */
function packsFacts(c: DeriveCtx, s: PacksState): Partial<DeriveCtx> {
  const mode = modeOf(c, effective(c, s));
  if (!packing(mode)) {
    const ac = !!c.ins?.additional_content;
    const folderSource = c.source !== null && !c.sourceIsImage;
    return {
      packs: mode,
      unpackKind: mode === "unpack" ? ownPacks(c) : null,
      packKind: null,
      formats: ac ? AC_TARGETS : TARGETS,
      defaultFormat: "pkg",
      // A folder kept as it is is already a folder; unpacked into one it is a new game folder.
      formatDisabled: (f) => (f === "folder" && folderSource && mode === "keep" ? msg().convert.alreadyFolder : undefined),
    };
  }
  return {
    packs: mode,
    unpackKind: null,
    packKind: mode,
    formats: mode === "zstd" ? ZSTD_PACK_FORMATS : LZ4_PACK_FORMATS,
    defaultFormat: mode === "zstd" ? "folder" : "lz4",
    formatDisabled: () => undefined,
  };
}

/** The result panel (Build.tsx fills it). */
export const ResultSlot = createContext<ReactNode>(null);

/** One big chip; `best`: the recommended mark (a green dot, said to screen readers). */
function BChip(props: {
  pressed: boolean;
  title?: string;
  disabled?: string;
  best?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const bm = useMessages().build;
  return (
    <button type="button" className="b-chip" aria-pressed={props.pressed} disabled={!!props.disabled} title={props.title} onClick={props.onClick}>
      {props.children}
      {props.best && (
        <>
          <span className="b-best" title={bm.recommended} aria-hidden="true" />
          <span className="visually-hidden">, {bm.recommended.toLocaleLowerCase()}</span>
        </>
      )}
      {props.disabled && <span className="visually-hidden">, {props.disabled}</span>}
    </button>
  );
}

function MoreChip(props: { open: boolean; onClick: () => void }) {
  const bm = useMessages().build;
  return (
    <button type="button" className={`b-chip b-more${props.open ? " open" : ""}`} aria-expanded={props.open} title={bm.moreTip} onClick={props.onClick}>
      {bm.more}
      <Icon name="chevron" />
    </button>
  );
}

const formatIcon = (f: Format): IconName => (f === "folder" || f === "lz4" ? "folder" : f === "pkg" ? "package" : "drive");

/** The basic chips, then More and the rest (closed, it still shows the chosen one). */
function FormatRow(props: { ctx: DeriveCtx; value: Format; onChange: (f: Format) => void; all: boolean }) {
  const bm = useMessages().build; // kindLabel reads the current language too
  const { ctx, value } = props;
  const [open, setOpen] = useState(false);
  // A pack has only Folder among the basic ones (no FPKG): its few targets are all shown, no More.
  const all = props.all || ctx.packKind !== null;
  const basic = ctx.formats.filter((f) => BASIC.includes(f));
  const more = ctx.formats.filter((f) => !BASIC.includes(f));
  const chip = (f: Format) => {
    const why = ctx.formatDisabled(f);
    return (
      <BChip
        key={f}
        pressed={value === f}
        disabled={why}
        best={f === "pkg"}
        title={why ?? (f === "folder" && ctx.packKind === "zstd" ? bm.zstdFolderCard : bm.cards[f])}
        onClick={() => props.onChange(f)}
      >
        <Icon name={formatIcon(f)} />
        {/* A packed folder is still a folder (the packs row says how). */}
        {kindLabel(f === "lz4" ? "folder" : f)}
      </BChip>
    );
  };
  return (
    <div className="b-chips" role="group" aria-labelledby="b-format-label">
      {basic.map(chip)}
      {more.length > 0 && !all && <MoreChip open={open} onClick={() => setOpen((o) => !o)} />}
      {more.filter((f) => all || open || f === value).map(chip)}
    </div>
  );
}

export const packsStep = defineStep<PacksState>({
  id: "packs",
  title: () => msg().build.packsTitle,
  initial: {
    choice: "none",
    levels: { zstd: "fast", lz4: "fast" },
    rules: null,
    format: "pkg",
    inner: "exfat",
    krakenLevel: "fast",
    ffpfscLevel: DEFAULT_FFPFSC_LEVEL,
  },
  available: (c) => c.ins !== null,
  // Every source starts at None + fpkg; levels, rules and format settings stay.
  onSource: (_c, s) => ({ ...s, choice: "none", format: "pkg" }),
  context(c, s) {
    const facts = packsFacts(c, s);
    const f = { ...c, ...facts };
    const format = f.formats.includes(s.format) && !f.formatDisabled(s.format) ? s.format : f.defaultFormat;
    // A `.pkg` carries no packs (issue #73): a game keeping its own is unpacked. Additional content keeps them.
    const own = ownPacks(c);
    if (facts.packs === "keep" && own && !c.ins?.additional_content && requestFormat(format) === "pkg") {
      return { ...facts, packs: "unpack", unpackKind: own, format };
    }
    return { ...facts, format };
  },
  Body({ ctx, state, set }) {
    const t = useMessages().pack;
    const cv = useMessages().convert;
    const bm = useMessages().build;
    const list = offered(ctx);
    // An unpack shows None pressed: the game's own kind into an fpkg is unpacked (issue #73).
    const choice = ctx.packs === "unpack" ? "none" : effective(ctx, state);
    const mode = ctx.packs;
    const zp = zstdPacked(ctx);
    const lp = lz4Packed(ctx);
    const pc = bm.packCards;
    const tip: Record<PacksChoice, string> = {
      none: zp || lp ? pc.unpack : pc.nonePlain,
      zstd: zp ? pc.keepZstd : lp ? pc.repackZstd : pc.zstd,
      lz4: lp ? pc.keepLz4 : zp ? pc.repackLz4 : pc.lz4,
    };
    const blocked = ctx.ins?.patch ? t.patch : ctx.ins?.additional_content ? t.additionalContent : undefined;
    const engine: Engine | null = packing(mode) ? mode : null;
    const fmt = ctx.format;
    const ffpfsc = !engine && fmt === "ffpfsc";
    const kraken = !engine && fmt === "pkg";
    const fold = ffpfsc || !!engine;
    const changed = (ffpfsc && state.inner !== "exfat") || (!!engine && state.rules !== null);
    // Settings' "Always show advanced options": every chip, no More, no How pack works.
    const all = usePrefs().advancedOpen;
    const [packsOpen, setPacksOpen] = useState(false);
    const result = useContext(ResultSlot);
    return (
      <>
        <div className="b-split">
          <div className="b-choices">
            <div className="b-field">
              <span className="label" id="b-packs-label">
                {bm.packs}
              </span>
              <div className="b-chips" role="group" aria-labelledby="b-packs-label" title={blocked}>
                {list
                  .filter((k) => k === "none" || all || packsOpen || k === choice)
                  .map((k) => (
                    <Fragment key={k}>
                      <BChip pressed={choice === k} title={tip[k]} best={k === "none"} onClick={() => set(pickChoice(ctx, k))}>
                        {bm.choices[k]}
                      </BChip>
                      {k === "none" && list.length > 1 && !all && <MoreChip open={packsOpen} onClick={() => setPacksOpen((o) => !o)} />}
                    </Fragment>
                  ))}
                {packsOpen && !all && (
                  <button type="button" className="small" onClick={openPackHelp}>
                    <Icon name="info" />
                    {t.help}
                  </button>
                )}
              </div>
            </div>
            <div className="b-field">
              <span className="label" id="b-format-label">
                {cv.targetFormat}
              </span>
              <FormatRow ctx={ctx} value={fmt} onChange={(f) => set(pickFormat(ctx, state, f))} all={all} />
            </div>
            {engine && (
              <PackLevelField engine={engine} value={state.levels[engine]} onChange={(l) => set({ levels: { ...state.levels, [engine]: l } })} />
            )}
            {ffpfsc && <ZlibLevelField value={state.ffpfscLevel} onChange={(n) => set({ ffpfscLevel: n })} />}
            {kraken && <KrakenLevelField value={state.krakenLevel} onChange={(l) => set({ krakenLevel: l })} />}
          </div>
          {result}
        </div>
        <hr className="b-sep" />
        {fold && (
          <Advanced title={bm.advanced} changed={changed}>
            {ffpfsc && <InnerField value={state.inner} onChange={(f) => set({ inner: f })} />}
            {engine && <RulesPick value={state.rules} onChange={(p) => set({ rules: p })} />}
          </Advanced>
        )}
        <OutputField out={ctx.out} source={ctx.source} prefix="b" />
        <div className="field build-generate">
          <GenerateSwitch out={ctx.out} source={ctx.source} />
        </div>
      </>
    );
  },
  request(c, s, r) {
    const format = requestFormat(c.format);
    // A pack's targets have no format settings.
    const req =
      packing(c.packs)
        ? { ...r, format, inner: null }
        : {
            ...r,
            format,
            inner: format === "ffpfsc" ? s.inner : null,
            kraken_level: s.krakenLevel,
            ffpfsc_level: s.ffpfscLevel,
          };
    const choice = c.packs;
    // `lz4: null`, as Convert sends: no packs, or the game's own packs kept byte for byte.
    if (choice === "keep") return req;
    if (choice === "unpack") return c.unpackKind === "zstd" ? { ...req, zstd_unpack: true } : { ...req, lz4: "unpack" };
    const level = LEVEL_OF[choice][s.levels[choice]];
    const withRules = { ...req, ...(s.rules !== null && { pack_profile: s.rules }) };
    if (choice === "zstd") return { ...withRules, zstd_pack: true, zstd_level: level };
    return { ...withRules, lz4: "pack", lz4_level: level, lz4_runtime: "ng" };
  },
  summary(c, s) {
    const cv = msg().convert;
    const bm = msg().build;
    const lines = packsSummary(c, s);
    // Unsigned executables don't block the build: a plain line, before Output.
    if (c.ins && !c.ins.patch && c.ins.unsigned?.length) lines.push({ text: msg().common.unsigned(c.ins.unsigned.length) });
    lines.push({ text: <OutputLine out={c.out} /> });
    const fmt = requestFormat(c.format);
    if (!packing(c.packs) && fmt === "ffpfsc") lines.push({ text: bm.sumInside(kindLabel(s.inner), s.ffpfscLevel), tone: "muted" });
    const warn = c.ins ? c.ins.findings.filter((l) => classify(l).formats?.includes(c.format)).length : 0;
    if (warn > 0) lines.push({ text: cv.likelyFail(kindLabel(fmt)), tone: "warn" });
    if (!collides(c) && c.source !== null && sameJobs(c).length > 0) lines.push({ text: cv.already(kindLabel(fmt)), tone: "muted" });
    return lines;
  },
  findings(c) {
    const t = msg().convert;
    const list: string[] = [];
    if (requestFormat(c.format) === "pkg" && c.ins && !c.ins.content_id) list.push(t.noContentId);
    if (collides(c)) list.push(t.collides);
    return list;
  },
});

function packsSummary(c: BuildCtx, s: PacksState): SummaryLine[] {
  const t = msg().pack;
  const bm = msg().build;
  const lines: SummaryLine[] = [];
  const zp = zstdPacked(c);
  const lp = lz4Packed(c);
  const lz4 = c.ins?.lz4 ?? null;
  const choice = c.packs;
  const runtime = runtimeLine(c, choice, getPrefs().replaceAmprEmu);
  if (choice === "keep") {
    lines.push({ text: zp ? bm.sumKeepZstd : lp ? bm.sumKeepLz4 : bm.sumKeepPlain });
    if (runtime) lines.push({ text: runtime, tone: "muted" });
    const index = indexNote(c);
    if (index) lines.push({ text: index, tone: "muted" });
    return lines;
  }
  if (choice === "unpack") {
    lines.push({ text: bm.sumUnpack(engineName(c.unpackKind ?? "lz4")) });
    if (runtime) lines.push({ text: runtime, tone: "muted" });
    return lines;
  }
  const level = levelLabel(choice, s.levels[choice]);
  const from: Engine | null = choice === "zstd" ? (lp ? "lz4" : null) : zp ? "zstd" : null;
  lines.push({ text: from ? bm.sumRepack(engineName(from), engineName(choice), level) : bm.sumPack(engineName(choice), level) });
  if (s.rules !== null) lines.push({ text: bm.sumRules(basename(s.rules) || s.rules) });
  if (runtime) lines.push({ text: runtime });
  if (choice === "lz4" && lz4?.index && lz4.index !== "ok") lines.push({ text: t.freshIndex(lz4.index), tone: "muted" });
  return lines;
}
