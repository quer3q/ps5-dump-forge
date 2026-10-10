// Pieces of a Target card that Convert and LZ4 share: format copy, the per-format settings,
// Full verification, and the output name (generated or typed) with its folder.

import { useRef, useState } from "react";

import { api, errorText, pick, type Format, type InnerFormat, type KrakenLevel } from "./api";
import { basename, dirname, FormatPicker, IMAGE_EXTENSIONS, joinPath, SEPARATORS } from "./common";
import { Icon, type IconName } from "./icons";
import type { Job } from "./jobs";

/** One line under a target picker: which one to pick, and when. */
export const FORMAT_INFO: Record<Format, string> = {
  folder: "Unpack an image or package back into a plain game folder.",
  exfat:
    "For games that misbehave as .ffpkg and only run like external-drive content. Also opens on a Mac or PC.",
  ffpkg: "Recommended. The PS5's own file system (UFS2). Writable mounts possible.",
  ffpfs: "Experimental. The PS5's PFS file system, uncompressed. File names must be plain ASCII.",
  ffpfsc: "Experimental. Smallest: a compressed container around one image, always mounted read-only.",
  pkg: "Installs like a store game and runs at native speed. Needs kstuff, fpkg-enable and ppr-patch.",
  lz4: "Experimental. A game folder with its assets packed into LZ4 volumes, which the bundled AMPR emulator (ampr_emu) unpacks while the game runs. Only for games that use AMPR.",
};

/** A note with its leading "Recommended" in green or "Experimental" in amber (text colour
 * only; the word itself says it). */
export function Lead({ text }: { text: string }) {
  const word = /^(Recommended|Experimental)\b/.exec(text)?.[1];
  if (!word) return <>{text}</>;
  return (
    <>
      <span className={word === "Recommended" ? "lead-good" : "lead-warn"}>{word}</span>
      {text.slice(word.length)}
    </>
  );
}

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
export const DEFAULT_FFPFSC_LEVEL = 6;

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

/** The `.fpkg` firmware limit, under the format's one-liner. */
export function PkgWarning() {
  return (
    <p className="alert-bar warn">
      <Icon name="warn" />
      <span>Works only on firmware 11.60 or lower.</span>
    </p>
  );
}

/** `.ffpfsc`: the image inside and the zlib level. `prefix` keeps ids and radio names apart
 * per screen. */
export function FfpfscField(props: {
  prefix: string;
  inner: InnerFormat;
  onInner: (f: InnerFormat) => void;
  level: number;
  onLevel: (n: number) => void;
}) {
  return (
    <div className="field inner">
      {/* The radio group's own label says the same to a screen reader. */}
      <span className="label" aria-hidden="true">
        Image inside
      </span>
      <FormatPicker
        name={`${props.prefix}-inner-format`}
        label="Image inside the .ffpfsc"
        className="inner"
        formats={INNER_FORMATS}
        value={props.inner}
        onChange={props.onInner}
      />
      <p className="muted hint">
        <Lead text={INNER_INFO[props.inner]} />
      </p>
      <label className="label range-label" htmlFor={`${props.prefix}-ffpfsc-level`}>
        <span>Compression level</span>
        <span className="range-value">{props.level}</span>
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
        <Lead text={ffpfscLevelNote(props.level)} />
      </p>
    </div>
  );
}

/** `.fpkg`: Fast / Balanced / Smallest. */
export function KrakenField(props: {
  prefix: string;
  value: KrakenLevel;
  onChange: (l: KrakenLevel) => void;
}) {
  return (
    <div className="field inner">
      {/* The radio group's own label says the same to a screen reader. */}
      <span className="label" aria-hidden="true">
        Compression
      </span>
      <div className="seg formats inner" role="radiogroup" aria-label="Compression level">
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
              <Icon name={KRAKEN_INFO[l].icon} />
              {KRAKEN_INFO[l].label}
            </span>
          </label>
        ))}
      </div>
      <p className="muted hint">
        <Lead text={KRAKEN_INFO[props.value].note} />
      </p>
    </div>
  );
}

/** Full verification: re-read every byte instead of sampling. */
export function VerifySwitch(props: { checked: boolean; onChange: (on: boolean) => void }) {
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
          Full verification
          <span className="muted hint">Re-reads every byte to check the output. Takes longer.</span>
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
  if (SEPARATORS.test(name))
    return { name, error: "A name can't contain a folder separator; use Change… for the folder." };
  const tooLong = (stem: string) => {
    const bytes = new TextEncoder().encode(stem).length;
    const max = STEM_LIMIT[format];
    const what = FOLDER_FORMATS.includes(format) ? "" : " before the extension";
    return bytes > max ? `Too long: ${bytes} bytes${what}, at most ${max} (UTF-8).` : undefined;
  };
  if (FOLDER_FORMATS.includes(format)) return { name, error: tooLong(name) };
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

/** Where unfinished jobs will publish: taken for a generated name. */
export function running(jobs: Job[]): string[] {
  return jobs.flatMap((j) => (j.request && !j.result ? [j.request.output] : []));
}

/** `preferred` (never "folder"), unless an image is already in that format: then it
 * extracts instead. */
export function startFormat(preferred: Format, path: string, isImage: boolean): Format {
  if (!isImage) return preferred;
  return path.toLowerCase().endsWith(`.${preferred}`) ? "folder" : preferred;
}

/**
 * The output of a screen's job: a generated name (`[GAME_NAME]-[TITLE_ID]`, the next free one)
 * or a typed one, in a folder that starts as the source's. `format` is the target this render
 * shows; calls made right after a source or format change pass the new ones themselves.
 */
export function useOutput(format: Format, jobs: Job[], onError: (e: string) => void) {
  const [output, setOutput] = useState("");
  const [generate, setGenerate] = useState(true);
  // Only the latest output request may land.
  const outputSeq = useRef(0);
  // Bumped when the source, format or naming changes: a file dialog opened before that is
  // answered for a choice that no longer stands, so its result is dropped.
  const choiceSeq = useRef(0);
  // The output's folder, kept while a new name is pending (output is empty then).
  const outDir = useRef("");

  // Recompute the name, keeping the folder.
  const refresh = async (src: string, fmt: Format, gen = generate, taken = running(jobs)) => {
    const seq = ++outputSeq.current;
    try {
      const out = gen
        ? await api.generatedOutput(src, fmt, outDir.current, taken)
        : await api.defaultOutput(src, fmt, outDir.current);
      if (seq !== outputSeq.current) return;
      if (out) outDir.current = dirname(out);
      setOutput(out);
    } catch (e) {
      if (seq === outputSeq.current) onError(errorText(e));
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
      outDir.current = dirname(src);
      setOutput("");
      if (fmt) void refresh(src, fmt);
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
      const title = FOLDER_FORMATS.includes(format) ? "Extract into this folder" : "Save into this folder";
      const dir = await pick({ directory: true, title });
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
  const { out, source } = props;
  const id = `${props.prefix}-output`;
  return (
    <div className="field" role="group" aria-labelledby={`${id}-label`}>
      <div className="label-row">
        <span className="label" id={`${id}-label`}>
          Output
        </span>
        <button className="small" onClick={() => void out.choose(source)} disabled={!source}>
          Change…
        </button>
      </div>
      {out.generate ? (
        // A generated name: shown whole (it wraps), only its folder is chosen.
        <p className="out-box" id={id}>
          {out.output ? (
            out.name
          ) : (
            <span className="muted">{source ? "Naming…" : "Choose a source first"}</span>
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
          placeholder={source ? "File name" : "Choose a source first"}
          disabled={!source}
          spellCheck={false}
        />
      )}
      {out.named.error && (
        <p className="bad field-error" id={`${id}-error`}>
          {out.named.error}
        </p>
      )}
      {source && (
        <p className="out-dir path" id={`${id}-dir`}>
          {out.named.name !== out.name && !out.named.error && <>saved as {out.named.name} </>}in{" "}
          {out.dir}
        </p>
      )}
    </div>
  );
}

/** "Generate name based on content", in the build row. */
export function GenerateSwitch(props: { out: Output; source: string | null }) {
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
        Generate name based on content
        {/* When on, the Output box shows the generated name itself. */}
        {!out.generate && <span className="muted mono hint">[game name]-[title ID]</span>}
      </span>
    </label>
  );
}
