// Pieces of a Target card that Convert and Pack share: format copy, the per-format settings,
// Full verification, and the output name (generated or typed) with its folder.

import { useRef, useState } from "react";

import { api, errorText, pick, type Format, type InnerFormat, type KrakenLevel } from "./api";
import {
  basename,
  dirname,
  FormatPicker,
  IMAGE_EXTENSIONS,
  isPasswordRequired,
  isNetwork,
  joinPath,
  kindLabel,
  Prose,
  SEPARATORS,
  useDeviceName,
  usePeerPath,
} from "./common";
import { useConnectState } from "./useConnectState";
import { msg, useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import type { Job } from "./jobs";

/** A one-line note; `tone`: its first word ("Recommended", "Experimental") is coloured. */
export interface Note {
  tone?: "good" | "warn";
  text: string;
}

/** One line under a target picker, in the current language (read at render). */
export function formatInfo(f: Format): Note {
  return msg().target.format[f];
}

/** A note with its leading word in green (`good`: "Recommended") or amber (`warn`:
 * "Experimental"); text colour only, the word itself says it. */
export function Lead({ note }: { note: Note }) {
  const word = note.tone ? /^[\p{L}\p{M}]+/u.exec(note.text)?.[0] : undefined;
  if (!word) return <Prose text={note.text} />;
  return (
    <>
      <span className={note.tone === "good" ? "lead-good" : "lead-warn"}>{word}</span>
      <Prose text={note.text.slice(word.length)} />
    </>
  );
}

/** The image a `.ffpfsc` holds, in picker order. */
const INNER_FORMATS: InnerFormat[] = ["exfat", "ffpkg", "ffpfs"];

/** One line under the inner-image picker: the format's own line, less what a `.ffpfsc` (always
 * mounted read-only) doesn't give. */
function innerInfo(f: InnerFormat, t: Messages["target"]): Note {
  return f === "ffpkg" ? t.innerFfpkg : t.format[f];
}

/** The `.ffpfsc` zlib level a new window starts on. */
export const DEFAULT_FFPFSC_LEVEL = 6;

/** One line under the `.ffpfsc` level slider: what the chosen level costs, from a 13 GB test
 * game on 14 cores (0: 3.0 s, 100% stored; 1: 5.7 s, 78.6%; 4: 13.0 s, 76.4%; 6: 16.3 s,
 * 76.25%; 7: 17.3 s; 9: 20.4 s, 76.21%). */
function ffpfscLevelNote(level: number, t: Messages["target"]): Note {
  if (level === 0) return t.level0;
  if (level === DEFAULT_FFPFSC_LEVEL) return t.levelDefault;
  return level < DEFAULT_FFPFSC_LEVEL ? t.levelLow : t.levelHigh;
}

/** The `.fpkg` compression levels, in picker order, with their icons (labels and notes: `M`). */
const KRAKEN_LEVELS: KrakenLevel[] = ["fast", "balanced", "smallest", "stored"];
const KRAKEN_ICON: Record<KrakenLevel, IconName> = {
  fast: "bolt",
  balanced: "scale",
  smallest: "compress",
  stored: "file",
};

/** `.ffpfsc`: the image inside and the zlib level. `prefix` keeps ids and radio names apart
 * per screen. */
export function FfpfscField(props: {
  prefix: string;
  inner: InnerFormat;
  onInner: (f: InnerFormat) => void;
  level: number;
  onLevel: (n: number) => void;
}) {
  const t = useMessages().target;
  return (
    <div className="field inner">
      {/* The radio group's own label says the same to a screen reader. */}
      <span className="label" aria-hidden="true">
        {t.imageInside}
      </span>
      <FormatPicker
        name={`${props.prefix}-inner-format`}
        label={t.imageInsideLabel}
        className="inner"
        formats={INNER_FORMATS}
        value={props.inner}
        onChange={props.onInner}
      />
      <p className="muted hint">
        <Lead note={innerInfo(props.inner, t)} />
      </p>
      <label className="label range-label" htmlFor={`${props.prefix}-ffpfsc-level`}>
        <span>{t.compressionLevel}</span>
        {/* The slider says its value itself: kept out of its name. */}
        <span className="range-value" aria-hidden="true">
          {props.level}
        </span>
      </label>
      <input
        id={`${props.prefix}-ffpfsc-level`}
        type="range"
        className="range"
        min={0}
        max={9}
        step={1}
        value={props.level}
        onChange={(e) => props.onLevel(Number(e.target.value))}
      />
      <p className="muted hint">
        <Lead note={ffpfscLevelNote(props.level, t)} />
      </p>
    </div>
  );
}

/** `.fpkg`: Fast / Balanced / Smallest / Stored. */
export function KrakenField(props: {
  prefix: string;
  value: KrakenLevel;
  onChange: (l: KrakenLevel) => void;
}) {
  const t = useMessages().target;
  return (
    <div className="field inner">
      {/* The radio group's own label says the same to a screen reader. */}
      <span className="label" aria-hidden="true">
        {t.compression}
      </span>
      <div className="seg formats inner" role="radiogroup" aria-label={t.compressionLevel}>
        {KRAKEN_LEVELS.map((l) => (
          <label key={l} className={`seg-choice fmt-pkg${props.value === l ? " on" : ""}`}>
            <input
              type="radio"
              className="visually-hidden"
              name={`${props.prefix}-kraken-level`}
              value={l}
              checked={props.value === l}
              onChange={() => props.onChange(l)}
            />
            <span className="seg-opt">
              <Icon name={KRAKEN_ICON[l]} />
              {t.kraken[l].label}
            </span>
          </label>
        ))}
      </div>
      <p className="muted hint">
        <Lead note={t.kraken[props.value].note} />
      </p>
    </div>
  );
}

/** Full verification: re-read every byte instead of sampling. */
export function VerifySwitch(props: {
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  const t = useMessages().target;
  return (
    <div className="field verify">
      <label className="switch">
        <input
          type="checkbox"
          className="visually-hidden"
          checked={props.checked}
          onChange={(e) => props.onChange(e.target.checked)}
        />
        <span className="switch-track" aria-hidden="true" />
        <span>
          {t.fullVerify}
          <span className="muted hint">{t.fullVerifyHint}</span>
        </span>
      </label>
    </div>
  );
}

/** Extensions a typed name can end in: the real ones, plus "fpkg", the debug package's label. */
const TYPED_EXTENSIONS = [...IMAGE_EXTENSIONS, "fpkg"];

/** Targets without an extension: the output is a folder. */
const FOLDER_FORMATS: Format[] = ["folder", "lz4"];

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
  lz4: 63,
};

/** A typed output name, with the target's extension appended when it has none. A different
 * image extension is an error: the extension picks the mount driver. So is a name over
 * `STEM_LIMIT`. */
function checkName(name: string, format: Format): { name: string; error?: string } {
  const t = msg().target;
  if (SEPARATORS.test(name)) return { name, error: t.separator };
  const stemError = (stem: string) => {
    // Blank, hidden (a leading dot: junk filters skip it), a trailing dot or space (dropped on
    // some filesystems), or a control character: never a name worth writing.
    if (!stem.trim() || stem.startsWith(".") || /[. ]$/.test(stem) || /[\x00-\x1f\x7f]/.test(stem))
      return t.badName;
    const bytes = new TextEncoder().encode(stem).length;
    const max = STEM_LIMIT[format];
    return bytes > max ? t.tooLong(bytes, max, FOLDER_FORMATS.includes(format)) : undefined;
  };
  if (FOLDER_FORMATS.includes(format)) return { name, error: stemError(name) };
  const dot = name.lastIndexOf(".");
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  let saved: string;
  if (ext === format) saved = name;
  // The UI calls the debug package ".fpkg"; a typed "Game.fpkg" is saved as "Game.pkg".
  else if (format === "pkg" && ext === "fpkg") saved = `${name.slice(0, dot)}.pkg`;
  else if (TYPED_EXTENSIONS.includes(ext))
    return {
      name,
      error: format === "pkg" ? t.pkgExt(ext) : t.otherExt(ext, kindLabel(format)),
    };
  else saved = `${name}.${format}`;
  const error = stemError(saved.slice(0, saved.lastIndexOf(".")));
  return error ? { name, error } : { name: saved };
}

/** Where unfinished jobs will publish: taken for a generated name. A transfer publishes each
 * source under its name in its folder (core's `transfer::targets`). */
export function running(jobs: Job[]): string[] {
  return jobs.flatMap((j) => {
    const r = j.request;
    if (!r || j.result) return [];
    if (r.extract?.length) return r.extract.map((p) => joinPath(r.output, p.split("/").pop() || ""));
    if (!r.transfer?.length) return [r.output];
    return r.transfer.map((s) => (basename(s) ? joinPath(r.output, basename(s)) : r.output));
  });
}

/**
 * The output of a screen's job: a generated name (`[GAME_NAME]-[TITLE_ID]`, the next free one)
 * or a typed one, in a folder that starts as the source's. `format` is the target this render
 * shows; calls made right after a source or format change pass the new ones themselves.
 */
export function useOutput(format: Format, jobs: Job[], onError: (e: string | null) => void) {
  useMessages().target; // the typed name's error is in the current language
  const [output, setOutput] = useState("");
  const [generate, setGenerate] = useState(true);
  // The latest naming failed (the error went to `onError`): the box stops saying "Naming".
  const [failed, setFailed] = useState(false);
  // The error `onError` shows is a naming one: a later name that works takes it back.
  const namingError = useRef(false);
  // Only the latest output request may land.
  const outputSeq = useRef(0);
  // Bumped when the source, format or naming changes: a file dialog opened before that is
  // answered for a choice that no longer stands, so its result is dropped.
  const choiceSeq = useRef(0);
  // The output's folder, kept while a new name is pending (output is empty then).
  const outDir = useRef("");
  // The password that opened an archive source (names read its param.json); in memory only.
  const password = useRef<string | null>(null);
  // This render's naming, for `opened`, which lands after an inspection began on an older render.
  const live = useRef({ generate, typed: false });
  live.current = { generate, typed: !generate && output !== "" };

  // Recompute the name, keeping the folder.
  const refresh = async (src: string, fmt: Format, gen = generate, taken = running(jobs)) => {
    const seq = ++outputSeq.current;
    setFailed(false);
    try {
      const out = gen
        ? await api.generatedOutput(src, fmt, outDir.current, taken, password.current)
        : await api.defaultOutput(src, fmt, outDir.current, password.current);
      if (seq !== outputSeq.current) return;
      if (out) outDir.current = dirname(out);
      setOutput(out);
      if (namingError.current) {
        namingError.current = false;
        onError(null);
      }
    } catch (e) {
      // A locked archive: the screen asks for its password, then `opened` names it.
      if (seq !== outputSeq.current) return;
      setFailed(true);
      if (!isPasswordRequired(errorText(e))) {
        namingError.current = true;
        onError(errorText(e));
      }
    }
  };

  // The name part: a typed name keeps whatever the user typed (separators included, which
  // `checkName` refuses), so the folder is never taken from typing.
  const prefix = joinPath(outDir.current, "");
  const name = output.startsWith(prefix) ? output.slice(prefix.length) : basename(output);
  const named = !generate && output ? checkName(name, format) : { name };
  const target = !generate && output && !named.error ? joinPath(outDir.current, named.name) : output;

  return {
    generate,
    output,
    failed,
    dir: outDir.current,
    name,
    named,
    /** The path a job writes: empty while a name is pending. */
    target,
    choiceSeq,
    /** A new source: its folder, and a fresh name for `fmt` (none yet when null). */
    reset(src: string, fmt: Format | null) {
      choiceSeq.current++;
      outputSeq.current++;
      password.current = null;
      // A source on another device: the user picks the output folder (core says so until then).
      outDir.current = isNetwork(src) ? "" : dirname(src);
      setOutput("");
      setFailed(false);
      if (fmt) void refresh(src, fmt);
    },
    /** The source opened, with `pw` (an archive's password, or null): names use it from now
     * on; a fresh name for `fmt` now (none when null, or when a name was typed meanwhile). */
    opened(src: string, fmt: Format | null, pw: string | null) {
      password.current = pw;
      if (!fmt || live.current.typed) return;
      outputSeq.current++;
      setOutput("");
      void refresh(src, fmt, live.current.generate);
    },
    /** A new target: a typed name keeps its stem, a generated one is made again. */
    retarget(src: string | null, fmt: Format) {
      choiceSeq.current++;
      if (!src) return;
      if (!generate && name) {
        const dot = name.lastIndexOf(".");
        const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
        const stem = TYPED_EXTENSIONS.includes(ext) ? name.slice(0, dot) : name;
        outputSeq.current++;
        setOutput(joinPath(outDir.current, FOLDER_FORMATS.includes(fmt) ? stem : `${stem}.${fmt}`));
        return;
      }
      // Clear first: the old name has the old extension, and the job waits for a path.
      setOutput("");
      void refresh(src, fmt);
    },
    toggle(src: string | null, gen: boolean) {
      choiceSeq.current++;
      setGenerate(gen);
      if (!src) return;
      setOutput("");
      void refresh(src, format, gen);
    },
    type(typed: string) {
      outputSeq.current++; // a pending default must not overwrite what the user types
      setOutput(typed ? joinPath(outDir.current, typed) : "");
    },
    /** Change…: only the folder is chosen; the name is generated or typed. */
    async choose(src: string | null) {
      if (!src) return;
      const choice = choiceSeq.current;
      const t = msg().target;
      const title = FOLDER_FORMATS.includes(format) ? t.extractInto : t.saveInto;
      // Opens at the output's folder, not wherever the last browse ended (the game's own folder).
      // A paired Forge takes any output, an fpkg included (the builder writes through the peer);
      // an FTP server takes all but an fpkg (it goes back over what it wrote).
      const dir = await pick({ directory: true, title, start: outDir.current || undefined, devices: true });
      if (dir === null || choice !== choiceSeq.current) return;
      outDir.current = dir;
      if (!generate && name) {
        outputSeq.current++;
        setOutput(joinPath(dir, name));
      } else {
        setOutput("");
        void refresh(src, format);
      }
    },
    /** A job started on `used`: a generated name moves on to the next free one, so the button
     * again starts another job (unless the choices changed since `choice`). */
    next(src: string, used: string, choice: number) {
      if (!generate || choice !== choiceSeq.current) return;
      setOutput("");
      void refresh(src, format, true, [...running(jobs), used]);
    },
  };
}

export type Output = ReturnType<typeof useOutput>;

/** The Output box: the generated name whole, or the typed one; its folder on the line below. */
export function OutputField(props: { out: Output; source: string | null; prefix: string }) {
  const m = useMessages();
  const t = m.target;
  const { out, source } = props;
  const id = `${props.prefix}-output`;
  // Which device the output goes to: a paired one by name, else this one (named only while
  // there are devices to tell apart). Its folder below reads "<device>: /<path>" too.
  const shown = usePeerPath();
  const peer = useDeviceName()(out.dir);
  const saved = useConnectState().state;
  const paired = (saved?.devices.length ?? 0) + (saved?.nas.length ?? 0) > 0;
  return (
    <div className="field" role="group" aria-labelledby={`${id}-label`}>
      <div className="label-row">
        <span className="label-group">
          <span className="label" id={`${id}-label`}>
            {t.output}
          </span>
          {source && out.dir && (peer || paired) && (
            <span className={peer ? "tag green" : "tag"} title={peer ?? undefined}>
              {peer ?? m.picker.thisDevice}
            </span>
          )}
        </span>
        <button className="small" onClick={() => void out.choose(source)} disabled={!source}>
          {t.change}
        </button>
      </div>
      {out.generate ? (
        // A generated name: shown whole (it wraps), only its folder is chosen.
        <p className="out-box" id={id}>
          {out.output ? (
            <span className="out-name">{out.name}</span>
          ) : (
            <span className="muted">{!source ? t.chooseSourceFirst : out.failed ? "—" : t.naming}</span>
          )}
        </p>
      ) : (
        // A typed name: the file name only; the folder stays the one below.
        <input
          id={id}
          className="mono out-input"
          aria-labelledby={`${id}-label`}
          aria-describedby={out.named.error ? `${id}-error ${id}-dir` : `${id}-dir`}
          aria-invalid={out.named.error ? true : undefined}
          value={out.name}
          onChange={(e) => out.type(e.target.value)}
          placeholder={source ? t.fileName : t.chooseSourceFirst}
          disabled={!source}
          spellCheck={false}
          autoCapitalize="none"
          autoCorrect="off"
        />
      )}
      {out.named.error && (
        <p className="bad field-error" id={`${id}-error`}>
          <Prose text={out.named.error} />
        </p>
      )}
      {source && out.dir && (
        <p className="out-dir path" id={`${id}-dir`}>
          {out.named.name !== out.name && !out.named.error
            ? t.savedAs(out.named.name, shown(out.dir))
            : t.inDir(shown(out.dir))}
        </p>
      )}
    </div>
  );
}

/** "Generate name based on content", in the build row. */
export function GenerateSwitch(props: { out: Output; source: string | null }) {
  const t = useMessages().target;
  const { out } = props;
  return (
    <label className="switch">
      <input
        type="checkbox"
        className="visually-hidden"
        checked={out.generate}
        onChange={(e) => out.toggle(props.source, e.target.checked)}
      />
      <span className="switch-track" aria-hidden="true" />
      <span>
        {t.generate}
        {/* When on, the Output box shows the generated name itself. */}
        {!out.generate && <span className="muted mono hint">{t.generatePattern}</span>}
      </span>
    </label>
  );
}
