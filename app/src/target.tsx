// Target pieces shared by the screens: format copy, per-format settings and the output name.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";

import { api, errorText, pick, type Format, type PackKind } from "./api";
import {
  basename,
  dirname,
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
import { getNaming, render, subscribeNaming, template } from "./naming";
import type { Job } from "./jobs";

/** A one-line note; `tone` colours its first word. */
export interface Note {
  tone?: "good" | "warn";
  text: string;
}

export const DEFAULT_FFPFSC_LEVEL = 6;

/** A typed name's extensions, plus "fpkg", the debug package's label. */
const TYPED_EXTENSIONS = [...IMAGE_EXTENSIONS, "fpkg"];

const FOLDER_FORMATS: Format[] = ["folder", "lz4"];

/** The longest output stem in UTF-8 bytes (core's `preflight::stem_limit`: SMP's mount name). */
const STEM_LIMIT: Record<Format, number> = {
  folder: 63,
  exfat: 63,
  ffpkg: 63,
  ffpfs: 63,
  ffpfsc: 58,
  pkg: 63,
  lz4: 63,
};

/** A typed name with the target's extension appended; another image extension or a stem over `STEM_LIMIT` is an error. */
function checkName(name: string, format: Format): { name: string; error?: string } {
  const t = msg().target;
  if (SEPARATORS.test(name)) return { name, error: t.separator };
  const stemError = (stem: string) => {
    // Blank, hidden (leading dot), a trailing dot or space (dropped on some filesystems), or a control character.
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

/** Where unfinished jobs will publish (a transfer: each source's name in its folder, core's `transfer::targets`). */
export function running(jobs: Job[]): string[] {
  return jobs.flatMap((j) => {
    const r = j.request;
    if (!r || j.result) return [];
    if (r.extract?.length) return r.extract.map((p) => joinPath(r.output, p.split("/").pop() || ""));
    if (!r.transfer?.length) return [r.output];
    return r.transfer.map((s) => (basename(s) ? joinPath(r.output, basename(s)) : r.output));
  });
}

export function useNaming(): string {
  return useSyncExternalStore(subscribeNaming, getNaming);
}

/** A job's output: a generated or typed name in a folder that starts as the source's; calls right after a change pass the new values. */
export function useOutput(
  format: Format,
  jobs: Job[],
  onError: (e: string | null) => void,
  pack: PackKind | null = null,
  active = true,
) {
  useMessages().target; // the typed name's error is in the current language
  const [output, setOutput] = useState("");
  const [generate, setGenerate] = useState(true);
  const [failed, setFailed] = useState(false);
  // The error `onError` shows is a naming one: a later name that works takes it back.
  const namingError = useRef(false);
  const outputSeq = useRef(0);
  // Bumped on a source or format change: a dialog opened before then is dropped.
  const choiceSeq = useRef(0);
  // The output's folder, kept while a new name is pending (output is empty then).
  const outDir = useRef("");
  // An archive source's password (names read its param.json); in memory only.
  const password = useRef<string | null>(null);
  // This render's naming, for `opened`, which lands after an inspection began on an older render.
  const live = useRef({ generate, typed: false });
  live.current = { generate, typed: !generate && output !== "" };
  const packRef = useRef(pack);
  packRef.current = pack;

  // The last source and target named: named again when the naming changes.
  const lastNamed = useRef<{ src: string; fmt: Format } | null>(null);

  // The latest jobs, for a late callback: a job added since must count as taken.
  const jobsNow = useRef(jobs);
  jobsNow.current = jobs;
  const refresh = async (src: string, fmt: Format, gen = generate, taken = running(jobsNow.current)) => {
    lastNamed.current = { src, fmt };
    const seq = ++outputSeq.current;
    setFailed(false);
    try {
      const out = gen
        ? await api.generatedOutput(src, fmt, outDir.current, taken, password.current, packRef.current, template(getNaming()))
        : await api.defaultOutput(src, fmt, outDir.current, password.current);
      if (seq !== outputSeq.current) return;
      if (out) outDir.current = dirname(out);
      setOutput(out);
      if (namingError.current) {
        namingError.current = false;
        onError(null);
      }
    } catch (e) {
      if (seq !== outputSeq.current) return;
      setFailed(true);
      if (!isPasswordRequired(errorText(e))) {
        namingError.current = true;
        onError(errorText(e));
      }
    }
  };

  // The naming changed: a generated name is made again, only while shown (each name reads the source; PR #39).
  const naming = useNaming();
  const namingSeen = useRef(naming);
  useEffect(() => {
    if (naming === namingSeen.current || !active) return;
    namingSeen.current = naming;
    const last = lastNamed.current;
    if (!live.current.generate || !last) return;
    setOutput("");
    void refresh(last.src, last.fmt, true);
  }, [naming, active]);

  // A typed name keeps whatever was typed, so the folder is never taken from typing.
  const prefix = joinPath(outDir.current, "");
  const name = output.startsWith(prefix) ? output.slice(prefix.length) : basename(output);
  const named = !generate && output ? checkName(name, format) : { name };
  const target = !generate && output && !named.error ? joinPath(outDir.current, named.name) : output;

  return {
    generate,
    output,
    failed,
    dir: outDir.current,
    pack,
    name,
    named,
    /** Empty while a name is pending. */
    target,
    choiceSeq,
    reset(src: string, fmt: Format | null) {
      choiceSeq.current++;
      outputSeq.current++;
      password.current = null;
      lastNamed.current = null;
      // A source on another device: the user picks the output folder (core says so until then).
      // An installed game (PS5 /user/app) is read only: its output goes to /data (#78).
      outDir.current = isNetwork(src) ? "" : /^\/(mnt\/[^/]+\/)?user\/app(\/|$)/.test(src) ? "/data" : dirname(src);
      setOutput("");
      setFailed(false);
      if (fmt) void refresh(src, fmt);
    },
    /** The source opened with `pw` (an archive's password): a fresh name unless one was typed meanwhile. */
    opened(src: string, fmt: Format | null, pw: string | null, pk?: PackKind | null) {
      password.current = pw;
      if (pk !== undefined) packRef.current = pk;
      if (!fmt) lastNamed.current = null;
      if (!fmt || live.current.typed) return;
      outputSeq.current++;
      setOutput("");
      void refresh(src, fmt, live.current.generate);
    },
    /** A new target: a typed name keeps its stem, a generated one is made again. */
    retarget(src: string | null, fmt: Format, pk?: PackKind | null) {
      choiceSeq.current++;
      if (pk !== undefined) packRef.current = pk;
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
    async choose(src: string | null) {
      if (!src) return;
      const choice = choiceSeq.current;
      const t = msg().target;
      const title = FOLDER_FORMATS.includes(format) ? t.extractInto : t.saveInto;
      // Opens at the output's folder; a peer takes any output, an FTP server all but an fpkg.
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
    /** A job started on `used`: a generated name moves to the next free one (unless the choices changed since `choice`). */
    next(src: string, used: string, choice: number) {
      if (!generate || choice !== choiceSeq.current) return;
      setOutput("");
      void refresh(src, format, true, [...running(jobs), used]);
    },
  };
}

export type Output = ReturnType<typeof useOutput>;

export function OutputField(props: { out: Output; source: string | null; prefix: string }) {
  const m = useMessages();
  const t = m.target;
  const { out, source } = props;
  const id = `${props.prefix}-output`;
  // The output's device: a peer by name, else this one (named only while there are devices to tell apart).
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
        <p className="out-box" id={id}>
          {out.output ? (
            <span className="out-name">{out.name}</span>
          ) : (
            <span className="muted">{!source ? t.chooseSourceFirst : out.failed ? "—" : t.naming}</span>
          )}
        </p>
      ) : (
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

export function GenerateSwitch(props: { out: Output; source: string | null }) {
  const m = useMessages();
  const t = m.target;
  const { out } = props;
  const naming = useNaming();
  const pattern = render(naming, (p) => (p === "pack" && !out.pack ? "" : m.naming.parts[p]));
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
        {/* LTR: the template's order, part names included, in every language. */}
        {!out.generate && <span className="muted mono hint" dir="ltr">{pattern}</span>}
      </span>
    </label>
  );
}
