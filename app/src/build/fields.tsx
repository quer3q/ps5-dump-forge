// Build's own setting fields: target.tsx's without their notes (explanations in tooltips only).

import type { InnerFormat, KrakenLevel } from "../api";
import { basename, FormatPicker, PathLine } from "../common";
import { pick } from "../api";
import { useMessages } from "../i18n";
import { Icon, type IconName } from "../icons";
import { LEVEL_OF, type Engine, type PackLevel } from "./formats";

const PACK_LEVELS: PackLevel[] = ["fast", "medium", "max"];
const PACK_ICON: Record<PackLevel, IconName> = { fast: "bolt", medium: "scale", max: "compress" };
const KRAKEN_LEVELS: KrakenLevel[] = ["fast", "balanced", "smallest", "stored"];
const KRAKEN_ICON: Record<KrakenLevel, IconName> = { fast: "bolt", balanced: "scale", smallest: "compress", stored: "file" };
const INNER_FORMATS: InnerFormat[] = ["exfat", "ffpkg", "ffpfs"];

/** A compression level's tone: fast green, the middle one yellow, the smallest red, stored gray. */
const TONE: Record<PackLevel | KrakenLevel, string> = {
  fast: "b-green",
  medium: "b-yellow",
  balanced: "b-yellow",
  max: "b-red",
  smallest: "b-red",
  stored: "b-gray",
};

/** A labelled row of level chips (one pressed). */
function LevelRow<T extends PackLevel | KrakenLevel>(props: {
  label: string;
  groupLabel: string;
  options: T[];
  value: T;
  onChange: (v: T) => void;
  icon: (v: T) => IconName;
  text: (v: T) => string;
  tip?: (v: T) => string;
}) {
  return (
    <div className="b-field">
      <span className="label" aria-hidden="true">
        {props.label}
      </span>
      <div className="b-chips" role="group" aria-label={props.groupLabel}>
        {props.options.map((v) => (
          <button
            key={v}
            type="button"
            className={`b-chip b-lv ${TONE[v]}`}
            aria-pressed={props.value === v}
            title={props.tip?.(v)}
            onClick={() => props.onChange(v)}
          >
            <Icon name={props.icon(v)} />
            {props.text(v)}
          </button>
        ))}
      </div>
    </div>
  );
}

/** zstd / LZ4 Pack's Fast / Medium / Max. */
export function PackLevelField(props: { engine: Engine; value: PackLevel; onChange: (l: PackLevel) => void }) {
  const p = useMessages().pack;
  const t = useMessages().target;
  const bm = useMessages().build;
  return (
    <LevelRow
      label={t.compression}
      groupLabel={t.compressionLevel}
      options={PACK_LEVELS}
      value={props.value}
      onChange={props.onChange}
      icon={(l) => PACK_ICON[l]}
      text={(l) => p.levels[l]}
      tip={(l) => bm.levelTip(props.engine === "zstd" ? "ZSTD" : "LZ4", LEVEL_OF[props.engine][l], p.levelNotes[props.engine][l].text)}
    />
  );
}

/** fpkg: Fast / Balanced / Smallest / Stored. */
export function KrakenLevelField(props: { value: KrakenLevel; onChange: (l: KrakenLevel) => void }) {
  const t = useMessages().target;
  return (
    <LevelRow
      label={t.compression}
      groupLabel={t.compressionLevel}
      options={KRAKEN_LEVELS}
      value={props.value}
      onChange={props.onChange}
      icon={(l) => KRAKEN_ICON[l]}
      text={(l) => t.kraken[l].label}
      tip={(l) => t.kraken[l].note.text}
    />
  );
}

/** ffpfsc: the zlib level slider. */
export function ZlibLevelField(props: { value: number; onChange: (n: number) => void }) {
  const t = useMessages().target;
  return (
    <div className="field inner">
      <label className="label range-label" htmlFor="b-ffpfsc-level">
        <span>{t.compressionLevel}</span>
        <span className="range-value" aria-hidden="true">
          {props.value}
        </span>
      </label>
      <input
        id="b-ffpfsc-level"
        type="range"
        className="range"
        min={0}
        max={9}
        step={1}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
    </div>
  );
}

/** ffpfsc: the image inside. */
export function InnerField(props: { value: InnerFormat; onChange: (f: InnerFormat) => void }) {
  const t = useMessages().target;
  return (
    <div className="field inner">
      <span className="label" aria-hidden="true">
        {t.imageInside}
      </span>
      <FormatPicker
        name="b-inner-format"
        label={t.imageInsideLabel}
        className="inner"
        formats={INNER_FORMATS}
        value={props.value}
        onChange={props.onChange}
      />
    </div>
  );
}

export function Toggle(props: {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
  disabled?: boolean;
  tip?: string;
  className?: string;
}) {
  return (
    <div className={`field verify ${props.className ?? ""}`} title={props.tip}>
      <label className="switch">
        <input
          type="checkbox"
          className="visually-hidden"
          checked={props.checked}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.target.checked)}
        />
        <span className="switch-track" aria-hidden="true" />
        <span>{props.label}</span>
      </label>
    </div>
  );
}

/** Pack's custom rules: a TOML file on any device, Pick and Clear. */
export function RulesPick(props: { value: string | null; onChange: (p: string | null) => void }) {
  const t = useMessages().pack.rules;
  const c = useMessages().common;
  const choose = async () => {
    const p = await pick({ directory: false, title: t.pickTitle, extensions: ["toml"], nasExtensions: ["toml"], nas: true, devices: true });
    if (p !== null) props.onChange(p);
  };
  return (
    <div className="field inner" title={`${t.info} ${t.warn}`}>
      <span className="label">{props.value === null ? t.title : t.titleWith(basename(props.value) || props.value)}</span>
      {props.value !== null && <PathLine path={props.value} />}
      <div className="row">
        <button type="button" className="small" onClick={() => void choose()}>
          <Icon name="file" />
          {c.pickButton}
        </button>
        {props.value !== null && (
          <button type="button" className="small" onClick={() => props.onChange(null)}>
            {t.clear}
          </button>
        )}
      </div>
    </div>
  );
}
