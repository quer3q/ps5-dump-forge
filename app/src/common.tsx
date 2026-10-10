// Pieces shared by the screens.

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createRoot } from "react-dom/client";

import { pick, prettyBytes, serverAddress, type Format, type Inspection, type Kind, type Lz4Facts } from "./api";
import { basename, SEPARATORS, trimSep } from "./paths";
import { useCoverGlow } from "./glow";
import { Icon, type IconName } from "./icons";

const KIND_LABEL: Record<Format, string> = {
  folder: "Folder",
  exfat: "exfat",
  ffpkg: "ffpkg",
  ffpfs: "ffpfs",
  ffpfsc: "ffpfsc",
  pkg: "fpkg", // the debug FPKG's display name; its file still ends in .pkg
  lz4: "LZ4", // an LZ4 packed folder
};

/** What a source can be, in picker order; also the target picker's formats. The LZ4 packed
 * folder is not among them: the LZ4 tab's Pack picks it. */
export const KINDS: Kind[] = ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

export function kindLabel(kind: string): string {
  return KIND_LABEL[kind as Format] ?? kind;
}

/** A format as a small tinted tag (hues: `.fmt-*` in styles.css). */
export function FormatPill({ kind }: { kind: string }) {
  const known = kind in KIND_LABEL;
  return <span className={known ? `tag fmt fmt-${kind}` : "tag"}>{kindLabel(kind)}</span>;
}

/** The image formats' names (no dot) as words in running text. */
const FORMAT_NAME = /\b(ffpfsc|ffpfs|ffpkg|exfat|fpkg)\b/;

/** Running text with every format name in bold, so it reads as a name. Only for copy: a path
 * or file name (`game.ffpkg`) keeps its dot and stays plain. */
export function Prose({ text }: { text: string }) {
  return <>{text.split(FORMAT_NAME).map((part, i) => (i % 2 ? <b key={i}>{part}</b> : part))}</>;
}

/** A card's title row: icon + title, actions on the right. */
export function CardHead(props: {
  /** Left out where the title stands alone (How to connect). */
  icon?: IconName;
  title: string;
  id?: string;
  /** The heading can take focus from code (tabIndex -1), e.g. when the control that had it
   * goes away. */
  focusable?: boolean;
  children?: ReactNode;
}) {
  return (
    <header className="card-head">
      {props.icon && (
        <span className="head-icon">
          <Icon name={props.icon} />
        </span>
      )}
      <h2 id={props.id} tabIndex={props.focusable ? -1 : undefined}>
        {props.title}
      </h2>
      {props.children && <div className="head-actions">{props.children}</div>}
    </header>
  );
}

/** Element ids of a tab and its panel. */
export function tabId(prefix: string, id: string): string {
  return `${prefix}-tab-${id}`;
}
export function panelId(prefix: string, id: string): string {
  return `${prefix}-panel-${id}`;
}

/**
 * A segmented row of tabs (WAI-ARIA tabs: one tab stop, arrow keys / Home / End move and
 * select). The caller renders each panel with `id={panelId(..)}`, `role="tabpanel"`,
 * `aria-labelledby={tabId(..)}` and `hidden` when not selected.
 */
export function TabList<T extends string>(props: {
  label: string;
  prefix: string;
  /** `className`: extra classes on that tab's button (the LZ4 tab's straps). `sep`: a divider
   * before it (drawn only; the keys still move through every tab). */
  tabs: { id: T; label: ReactNode; className?: string; sep?: boolean }[];
  value: T;
  onChange: (id: T) => void;
  className?: string;
}) {
  const ids = props.tabs.map((t) => t.id);
  const onKey = (e: KeyboardEvent) => {
    const i = ids.indexOf(props.value);
    const next =
      e.key === "ArrowRight" ? ids[(i + 1) % ids.length]
      : e.key === "ArrowLeft" ? ids[(i - 1 + ids.length) % ids.length]
      : e.key === "Home" ? ids[0]
      : e.key === "End" ? ids[ids.length - 1]
      : undefined;
    if (next === undefined) return;
    e.preventDefault();
    props.onChange(next);
    document.getElementById(tabId(props.prefix, next))?.focus();
  };
  return (
    <div
      className={`seg ${props.className ?? ""}`}
      role="tablist"
      aria-label={props.label}
      onKeyDown={onKey}
    >
      {props.tabs.map((t) => {
        const on = t.id === props.value;
        const tab = (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={tabId(props.prefix, t.id)}
            aria-selected={on}
            aria-controls={panelId(props.prefix, t.id)}
            tabIndex={on ? 0 : -1}
            className={`seg-opt${on ? " on" : ""}${t.className ? ` ${t.className}` : ""}`}
            onClick={() => props.onChange(t.id)}
          >
            {t.label}
          </button>
        );
        return t.sep ? [<span key={`${t.id}-sep`} className="tab-sep" aria-hidden="true" />, tab] : tab;
      })}
    </div>
  );
}

/**
 * One-of-N format choice as a segmented group, each format in its own hue. Native radios
 * underneath (visually hidden), so Tab / arrow keys and radio semantics come from the
 * browser.
 */
export function FormatPicker<F extends Format>(props: {
  name: string;
  label: string;
  value: F;
  onChange: (f: F) => void;
  disabled?: (f: F) => boolean;
  /** Why a disabled format is refused: read out with its label, and its tooltip. */
  reason?: (f: F) => string | undefined;
  /** Default: every source kind (the LZ4 packed folder is chosen elsewhere). */
  formats?: F[];
  className?: string;
}) {
  const formats = props.formats ?? (KINDS as F[]);
  return (
    <div
      className={`seg formats ${props.className ?? ""}`}
      role="radiogroup"
      aria-label={props.label}
    >
      {formats.map((f) => {
        const disabled = props.disabled?.(f) ?? false;
        const why = disabled ? props.reason?.(f) : undefined;
        return (
          // `on` from state, not `input:checked + …`: WebKit misses that restyle when React
          // sets `checked` by code (a new source switching the format).
          <label
            key={f}
            className={`seg-choice fmt-${f}${props.value === f ? " on" : ""}`}
            title={why}
          >
            <input
              type="radio"
              className="visually-hidden"
              name={props.name}
              value={f}
              checked={props.value === f}
              disabled={disabled}
              onChange={() => props.onChange(f)}
            />
            <span className="seg-opt">
              {KIND_LABEL[f]}
              {why && <span className="visually-hidden">, {why}</span>}
            </span>
          </label>
        );
      })}
    </div>
  );
}

export const IMAGE_EXTENSIONS = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

export { basename, dirname, joinPath, SEPARATORS } from "./paths";

/**
 * A path on one line that never hides its last part: the folder part ellipsizes, the name
 * stays. The full path is the tooltip.
 */
export function PathLine({ path, prefix }: { path: string; prefix?: string }) {
  const name = basename(path);
  const dir = path.slice(0, trimSep(path).length - name.length);
  return (
    <p className="pathline" title={path}>
      <span className="dir">
        {prefix}
        {dir}
      </span>
      <span className="base">{name}</span>
    </p>
  );
}

// ---- Dialogs over the whole app (the LZ4 help, the delete question) ----

const overlays = new Set<() => void>();

/** Closes every open dialog (as if cancelled): the quit prompt never stacks behind one. */
export function closeOverlays(): void {
  for (const close of [...overlays]) close();
}

/**
 * A dialog in its own React root, like the http Picker: the app behind it (header, every
 * screen) is inert. `onClosed` runs once it closes; focus goes back to what had it.
 */
export function openOverlay(render: (close: () => void) => ReactNode, onClosed?: () => void): void {
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const app = document.getElementById("root");
  app?.setAttribute("inert", "");
  app?.setAttribute("aria-hidden", "true");
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    overlays.delete(close);
    if (overlays.size === 0) {
      app?.removeAttribute("inert");
      app?.removeAttribute("aria-hidden");
    }
    setTimeout(() => {
      root.unmount();
      host.remove();
      opener?.focus();
    }, 0);
    onClosed?.();
  };
  overlays.add(close);
  root.render(render(close));
}

const FOCUSABLE = 'button:not(:disabled), a[href], input:not(:disabled), [tabindex]:not([tabindex="-1"])';

/** An overlay's backdrop and box: Escape and a click outside close it, Tab stays inside (old
 * WebKit has no `inert`). Give the first control `autoFocus`. */
export function Modal(props: {
  close: () => void;
  className: string;
  role?: "dialog" | "alertdialog";
  labelledBy: string;
  describedBy?: string;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      props.close();
    } else if (e.key === "Tab" && box.current) {
      const all = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (all.length === 0) return;
      const i = all.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) {
        e.preventDefault();
        all[all.length - 1].focus();
      } else if (!e.shiftKey && i === all.length - 1) {
        e.preventDefault();
        all[0].focus();
      }
    }
  };
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) props.close();
      }}
    >
      <div
        className={props.className}
        role={props.role ?? "dialog"}
        aria-modal="true"
        aria-labelledby={props.labelledBy}
        aria-describedby={props.describedBy}
        ref={box}
        onKeyDown={onKey}
      >
        {props.children}
      </div>
    </div>
  );
}

// ---- Deleted sources ----

const deletedSubs = new Set<(path: string) => void>();

/** Calls `f` with every path Inspect deletes; returns the unsubscribe. */
export function onDeleted(f: (path: string) => void): () => void {
  deletedSubs.add(f);
  return () => void deletedSubs.delete(f);
}

export function notifyDeleted(path: string): void {
  for (const f of deletedSubs) f(path);
}

/** `path` is `dir` or inside it, by spelling (the server compared the real paths). */
export function isWithin(path: string, dir: string): boolean {
  const p = trimSep(path);
  const d = trimSep(dir);
  return p === d || (p.startsWith(d) && SEPARATORS.test(p[d.length] ?? ""));
}

/** The picker for a game folder or an image file; `onPick` unless cancelled. */
export async function pickSource(
  what: "folder" | "image",
  onPick: (path: string, isImage: boolean) => void,
): Promise<void> {
  const p =
    what === "folder"
      ? await pick({ directory: true, title: "Game folder" })
      : await pick({
          directory: false,
          title: "Game image",
          filterName: "PS5 image or package",
          extensions: IMAGE_EXTENSIONS,
        });
  if (p !== null) onPick(p, what === "image");
}

/** "Choose folder…" / "Choose image…": a game folder or an image file. */
function PickButtons(props: { onPick: (path: string, isImage: boolean) => void; big?: boolean }) {
  const pickFolder = () => pickSource("folder", props.onPick);
  const pickImage = () => pickSource("image", props.onPick);
  return (
    <>
      <button className={props.big ? "primary" : "small"} onClick={pickFolder}>
        <Icon name="folder" />
        Choose folder…
      </button>
      <button className={props.big ? undefined : "small"} onClick={pickImage}>
        <Icon name="disc" />
        Choose image…
      </button>
    </>
  );
}

/**
 * The Source card: an empty state with the pickers until a source is chosen, then the game
 * it holds (or why it can't be read). `full` adds what only Inspect shows.
 */
export function SourceCard(props: {
  path: string | null;
  onPick: (path: string, isImage: boolean) => void;
  busy: boolean;
  error: string | null;
  ins: Inspection | null;
  full?: boolean;
  /** Keeps element ids apart per screen; default "i" with `full`, else "c". */
  prefix?: string;
  /** Stands in for the default empty state (the LZ4 tab's). */
  empty?: ReactNode;
  /** More buttons in the title row once a source is chosen (Inspect's Delete). */
  actions?: ReactNode;
}) {
  const { path, ins } = props;
  const headId = `${props.prefix ?? (props.full ? "i" : "c")}-source`;
  const glow = useCoverGlow(!path ? null : ins ? ins.cover : props.busy ? undefined : null);

  // The first pick unmounts the empty state's buttons, which had focus: hand it to the
  // card's heading instead of letting it fall to the page.
  const hadPath = useRef(path !== null);
  useEffect(() => {
    const a = document.activeElement;
    if (path && !hadPath.current && (!a || a === document.body)) {
      document.getElementById(headId)?.focus();
    }
    hadPath.current = path !== null;
  }, [path, headId]);

  return (
    <section
      className={`card hero${props.full ? " full" : ""}${glow.ready ? " lit" : ""}`}
      style={glow.style}
      aria-labelledby={headId}
    >
      <CardHead icon="disc" title="Source" id={headId} focusable>
        {path && props.actions}
        {path && <PickButtons onPick={props.onPick} />}
      </CardHead>
      {!path && props.empty ? (
        props.empty
      ) : !path ? (
        <div className="empty">
          <span className="empty-icon">
            <Icon name="disc" />
          </span>
          <p className="empty-title">Choose a game folder or image</p>
          <p className="muted">
            A dumped PS5 game folder (sce_sys/param.json and eboot.bin at its root), or an image
            or package of one.
          </p>
          <div className="kinds" aria-label="Accepted sources">
            {KINDS.map((k) => (
              <FormatPill key={k} kind={k} />
            ))}
          </div>
          <div className="row center">
            <PickButtons onPick={props.onPick} big />
          </div>
        </div>
      ) : (
        <>
          <GameHead path={path} ins={ins} busy={props.busy} full={props.full} />
          {props.error && <p className="bad">{props.error}</p>}
          {ins && <InspectSummary ins={ins} full={props.full} />}
        </>
      )}
    </section>
  );
}

/** Cover, title, kind tags and the source's path. */
function GameHead(props: { path: string; ins: Inspection | null; busy: boolean; full?: boolean }) {
  const { ins } = props;
  const title = ins
    ? (ins.title_name ?? ins.title_id ?? "Unknown title")
    : props.busy
      ? "Reading the source…"
      : "Unreadable source";
  return (
    <div className={props.full ? "game full" : "game"}>
      {ins?.cover ? (
        <img className="cover" src={ins.cover} alt="" />
      ) : (
        <div className="cover none" aria-hidden="true">
          <Icon name={ins ? "disc" : "folder"} />
        </div>
      )}
      <div className="game-text">
        <div className="title-row">
          <h3 className={ins ? "title" : "title pending"}>{title}</h3>
          {ins && <FormatPill kind={ins.kind} />}
          {ins && ins.backport.length > 0 && <span className="tag orange">Backport</span>}
          {ins && ins.emulators.length > 0 && <span className="tag">Emulators</span>}
          {ins && ins.dlcs.length > 0 && <span className="tag violet">DLC</span>}
        </div>
        <PathLine path={props.path} />
      </div>
    </div>
  );
}

/** The game's facts as inset tiles. */
function Facts({ ins, full }: { ins: Inspection; full: boolean }) {
  const bp = ins.backport.length > 0;
  const emus = emulatorNames(ins);
  // With a fakelib/ (backport or emulators) the game runs as low as its executables allow.
  const fromExe = ins.backport_firmware !== null;
  const minFw = ins.backport_firmware ?? ins.firmware;
  const fwNote: string[] = [];
  if (fromExe && ins.firmware) fwNote.push(`declares ${ins.firmware}`);
  if ((bp || emus.length > 0) && !fromExe)
    fwNote.push("firmware unknown from its executables (no readable SDK)");
  if (bp) fwNote.push(backportLib(ins));
  if (emus.length > 0) fwNote.push(`emulators: ${emus.join(", ")}`);
  if (full && ins.sdk) fwNote.push(`SDK ${ins.sdk} (declared)`);
  return (
    <dl className="tiles">
      <div className="tile">
        <dt>Title ID</dt>
        <dd className={ins.title_id ? "value mono" : "value"}>{ins.title_id ?? "—"}</dd>
      </div>
      <div className="tile">
        <dt>Version</dt>
        <dd className="value">{ins.version ?? "—"}</dd>
      </div>
      <div className="tile">
        <dt>Firmware</dt>
        <dd>
          <span className="value">{minFw ? `${minFw}+` : "—"}</span>
          {minFw && (
            <span className={!fromExe ? "tag blue" : bp ? "tag orange" : "tag"}>
              {!fromExe ? "declared" : bp ? "backport" : "executables"}
            </span>
          )}
          {fwNote.length > 0 && <span className="note">{fwNote.join(" · ")}</span>}
        </dd>
      </div>
      <div className="tile">
        <dt>Size</dt>
        <dd>
          <span className="value">{prettyBytes(ins.total_bytes)}</span>
          <span className="tag">
            {ins.files.length.toLocaleString()} file{ins.files.length === 1 ? "" : "s"}
          </span>
        </dd>
      </div>
      {ins.lz4 && <Lz4Tile facts={ins.lz4} />}
      {full && (
        <div className="tile wide">
          <dt>Content ID</dt>
          <dd className={ins.content_id ? "value mono" : "value"}>{ins.content_id ?? "—"}</dd>
        </div>
      )}
      {full && ins.forge_version !== null && (
        <div className="tile wide">
          <dt>PS5 Dump Forge</dt>
          <dd className="value mono">v{ins.forge_version}</dd>
        </div>
      )}
    </dl>
  );
}

/** The source records (or recorded) LZ4 traces: Forge's trace runtime, or a journal. */
export function lz4Traced(f: Lz4Facts | null | undefined): boolean {
  return !!f && (f.runtime === "forge_trace" || f.journal_bytes !== null);
}

/** LZ4 asset packs, as the CLI's `inspect` line says it ("LZ4 (AMPR): packed, …"): the state
 * as the value, the details under it. Packed gets the green neon straps, a traced source the
 * lime, magenta and cyan ones; damaged packs keep the warning colour and no straps. */
function Lz4Tile({ facts }: { facts: Lz4Facts }) {
  const { state, detail } = lz4State(facts);
  const notes = [detail];
  if (!facts.imports_ampr) notes.push("eboot.bin does not import libSceAmpr");
  const look = facts.packed
    ? " neon packed"
    : facts.manifest_error !== null
      ? " damaged"
      : lz4Traced(facts)
        ? " neon"
        : "";
  return (
    <div className={`tile wide${look}`}>
      <dt>LZ4 (AMPR)</dt>
      <dd>
        <span className="value">{state}</span>
        <span className="note">{notes.join(" · ")}</span>
      </dd>
    </div>
  );
}

const RUNTIME_LABEL: Record<Lz4Facts["runtime"], string> = {
  forge_release: "Forge release",
  forge_trace: "Forge trace",
  other: "other",
  none: "none",
};

/** "Packed" / "Damaged packs" / "Traced" / "Plain", and what backs it. */
export function lz4State(f: Lz4Facts): { state: string; detail: string } {
  if (f.packed) {
    const p = f.packed;
    return {
      state: "Packed",
      detail:
        `${p.files.toLocaleString()} files in ${p.volumes.toLocaleString()} volume${p.volumes === 1 ? "" : "s"}` +
        (p.stored_percent === null ? "" : `, ${p.stored_percent}% of size`),
    };
  }
  if (f.manifest_error !== null) return { state: "Damaged packs", detail: f.manifest_error };
  if (f.journal_bytes !== null)
    return { state: "Traced", detail: `journal ${prettyBytes(f.journal_bytes)}` };
  if (f.runtime === "forge_trace") return { state: "Traced", detail: "no journal yet" };
  return { state: "Plain", detail: `runtime ${RUNTIME_LABEL[f.runtime]}` };
}

/** Which fakelib folder a backport ships, and how many backport libraries it has. */
function backportLib(ins: Inspection): string {
  const exclusive = ins.backport.some((p) => /^fakelib2\//i.test(p));
  const libs = ins.backport.length;
  return `${exclusive ? "fakelib2/, mounted alone" : "fakelib/"}, ${libs} backport librar${libs === 1 ? "y" : "ies"}`;
}

/** The emulators in fakelib/, each named once ("AMPR", "DLC", "PlayGo", "Other"). */
export function emulatorNames(ins: Inspection): string[] {
  return [...new Set(ins.emulators.map((e) => e.name))];
}

/** Preflight findings, each with a status icon; one green line when there are none. */
export function Findings({ lines }: { lines: string[] }) {
  if (lines.length === 0)
    return (
      <p className="status-line good">
        <Icon name="checkCircle" />
        No preflight findings
      </p>
    );
  return (
    <ul className="findings" aria-label="Preflight findings">
      {lines.map((l, i) => {
        const { kind } = classify(l);
        return (
          <li key={i} className={kind}>
            <Icon name={kind === "info" ? "info" : kind === "block" ? "alert" : "warn"} />
            <span>{l}</span>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * What a finding from core means for a build: `block` fails every target, `warn` may fail
 * (`formats`: the targets it fails), `info` is only news.
 */
// ponytail: core reports findings as plain text, so this matches its wording (preflight.rs
// `input`, inspect.rs). Ceiling: core should tag each finding with a severity and formats.
export function classify(line: string): { kind: "block" | "warn" | "info"; formats?: Format[] } {
  if (/^(eboot\.bin is missing|sce_sys\/param\.json)/.test(line)) return { kind: "block" };
  // Core says "(refused by .exfat/.ffpkg)", but a .pkg build refuses them too (package.rs).
  // .ffpfs takes ASCII names only, and a .ffpfsc's inner image refuses them too.
  if (/^name not in NFC/.test(line))
    return { kind: "warn", formats: ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"] };
  // PFS holds ASCII names only; a .ffpfsc defaults to exFAT inside, which takes them.
  if (/^name not ASCII/.test(line)) return { kind: "warn", formats: ["ffpfs"] };
  if (/^extraction:/.test(line)) return { kind: "warn", formats: ["folder", "lz4"] };
  if (/^(already 512-byte sectors|geometry differs from the SMP fast path)/.test(line))
    return { kind: "info" };
  return { kind: "warn" };
}

/** The DLC embedded in the dump, collapsed until asked for. */
function DlcList({ dlcs }: { dlcs: Inspection["dlcs"] }) {
  if (dlcs.length === 0) return null;
  const notes = dlcs.map(dlcNote);
  // One shared explanation goes on top once instead of under every row.
  const shared = notes.every((n) => n === notes[0]) ? notes[0] : null;
  return (
    <details className="fold">
      <summary>Embedded DLC ({dlcs.length})</summary>
      {shared && <p className="muted fold-note">All {shared}.</p>}
      <ul className="rows scroll" tabIndex={0} aria-label="Embedded DLC">
        {dlcs.map((d, i) => (
          <li key={d.content_id}>
            <span className="row-title">
              {d.name && <strong>{d.name} </strong>}
              <span className="path">
                {d.content_id.slice(0, d.content_id.length - d.label.length)}
                <strong className="cid-label">{d.label}</strong>
              </span>
            </span>
            {!shared && <span className="muted">{notes[i]}</span>}
          </li>
        ))}
      </ul>
    </details>
  );
}

/** Where a DLC's content comes from, as one phrase. */
function dlcNote(d: Inspection["dlcs"][number]): string {
  if (d.folder) return `in ${d.folder}/, ${prettyBytes(d.bytes)}`;
  if (d.emulated === "NO_EXTRA_DATA")
    return "unlocked by the DLC emulator (dlc_emu.ini): entitlement only, no data of its own";
  if (d.emulated !== null)
    return `unlocked by the DLC emulator (dlc_emu.ini), status ${d.emulated || "not set"}`;
  return "merged into the game's folders";
}

/** The game's facts, its embedded DLC (and backport and emulator files, `full`), then the
 * findings. */
export function InspectSummary({ ins, full = false }: { ins: Inspection; full?: boolean }) {
  return (
    <div className="summary">
      <Facts ins={ins} full={full} />
      {(ins.dlcs.length > 0 ||
        (full && (ins.backport.length > 0 || ins.emulators.length > 0))) && (
        <div className="folds">
          <DlcList dlcs={ins.dlcs} />
          {full && ins.backport.length > 0 && (
            <details className="fold">
              <summary>Backport files ({ins.backport.length})</summary>
              <ul className="rows scroll" tabIndex={0} aria-label="Backport files">
                {ins.backport.map((p) => (
                  <li key={p} className="path">
                    {p}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {full && ins.emulators.length > 0 && (
            <details className="fold">
              <summary>Emulators in fakelib ({ins.emulators.length})</summary>
              <ul className="rows scroll" tabIndex={0} aria-label="Emulators in fakelib">
                {ins.emulators.map((e) => (
                  <li key={e.path}>
                    <span className="path">{e.path}</span>
                    <span className="muted">{e.name}</span>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
      <Findings lines={ins.findings} />
    </div>
  );
}

/** A button that opens a short menu of choices: the native dialog picks files or folders,
 * never both (rfd), so the kind is asked first. Esc or a click outside closes it; focus goes
 * back to the button before the choice runs, so a cancelled dialog leaves it there. */
export function ChooseMenu<K extends string>(props: {
  id: string;
  label: ReactNode;
  /** The menu's accessible name. */
  menuLabel: string;
  items: { id: K; icon: IconName; label: string }[];
  onChoose: (what: K) => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    // mousedown, not blur: WebKit doesn't focus a clicked button, so a blur would close the
    // menu before the item's click lands.
    const outside = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", outside);
    return () => document.removeEventListener("mousedown", outside);
  }, [open]);
  const choose = (what: K) => {
    setOpen(false);
    btn.current?.focus();
    props.onChoose(what);
  };
  const onKey = (e: KeyboardEvent) => {
    if (!open) return;
    const items = [...(box.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      btn.current?.focus();
    } else if (e.key === "Tab") {
      setOpen(false);
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && items.length > 0) {
      e.preventDefault();
      items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length].focus();
    }
  };
  return (
    <span className="menu-wrap" ref={box} onKeyDown={onKey}>
      <button
        id={props.id}
        ref={btn}
        className={props.className ?? "small"}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {props.label}
        <Icon name="chevron" />
      </button>
      {open && (
        <span className="menu" role="menu" aria-label={props.menuLabel}>
          {props.items.map((it) => (
            <button key={it.id} type="button" role="menuitem" onClick={() => choose(it.id)}>
              <Icon name={it.icon} />
              {it.label}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

/** http build: the page's url as a QR code for a phone, drawn by the server (`GET /api/qr`).
 * Nothing in the app (no address) or when it doesn't load. */
export function AddressQr(props: { size: number }) {
  const [failed, setFailed] = useState(false);
  const address = serverAddress();
  if (!address || failed) return null;
  return (
    <img
      className="qr"
      src="/api/qr"
      width={props.size}
      height={props.size}
      alt={`QR code: http://${address}`}
      onError={() => setFailed(true)}
    />
  );
}
