// Pieces shared by the screens.

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createRoot } from "react-dom/client";

import {
  APP_NAME,
  ARCHIVE_PASSWORD_REQUIRED,
  pick,
  prettyBytes,
  serverAddress,
  type ArchiveKind,
  type Format,
  type Inspection,
  type Kind,
  type Lz4Facts,
} from "./api";
import { gameTitle, msg, num, useLang, useMessages } from "./i18n";
import { basename, SEPARATORS, trimSep } from "./paths";
import { useCoverGlow } from "./glow";
import { Icon, type IconName } from "./icons";

// Format names stay as they are in every language; only "Folder" is translated (kindLabel).
const KIND_LABEL: Record<Format | ArchiveKind, string> = {
  folder: "Folder",
  exfat: "exfat",
  ffpkg: "ffpkg",
  ffpfs: "ffpfs",
  ffpfsc: "ffpfsc",
  pkg: "fpkg", // the debug FPKG's display name; its file still ends in .pkg
  lz4: "LZ4", // an LZ4 packed folder
  zip: "zip", // archives: a source only
  "7z": "7z",
  rar: "rar",
};

/** What a source can be, in picker order; also the target picker's formats. The LZ4 packed
 * folder is not among them: the LZ4 tab's Pack picks it. */
export const KINDS: Kind[] = ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

/** Archives holding one game: sources only, never a target. */
export const ARCHIVE_KINDS: ArchiveKind[] = ["zip", "7z", "rar"];

/** An inspected source's kind is an archive's. */
export function isArchive(kind: string | undefined): boolean {
  return ARCHIVE_KINDS.includes(kind as ArchiveKind);
}

/** A format's display name, read at call time (a component calling it re-renders on a switch
 * through its own `useMessages`). */
export function kindLabel(kind: string): string {
  if (kind === "folder") return msg().common.folder;
  return KIND_LABEL[kind as Format | ArchiveKind] ?? kind;
}

/** A format as a small tinted tag (hues: `.fmt-*` in styles.css). */
export function FormatPill({ kind }: { kind: string }) {
  useLang(); // re-render on a language switch: kindLabel reads the current one
  const known = kind in KIND_LABEL;
  // The three archives share one hue: what they hold is the same.
  const hue = isArchive(kind) ? "archive" : kind;
  return <span className={known ? `tag fmt fmt-${hue}` : "tag"}>{kindLabel(kind)}</span>;
}

/** The image formats' names (no dot) as words in running text: not an extension (`.exfat`) nor
 * part of a longer name (`fpkg-enable`). The character before is matched, not looked behind:
 * the PS5's WebKit has no lookbehind. */
const FORMAT_NAME = /(^|[^.\w-])(ffpfsc|ffpfs|ffpkg|exfat|fpkg)(?![\w-])/g;

/** Running text with every format name in bold, so it reads as a name. Only for copy: a path
 * or file name (`game.ffpkg`) keeps its dot and stays plain. */
export function Prose({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const m of text.matchAll(FORMAT_NAME)) {
    const start = (m.index ?? 0) + m[1].length;
    parts.push(text.slice(at, start), <b key={start}>{m[2]}</b>);
    at = start + m[2].length;
  }
  parts.push(text.slice(at));
  return <>{parts}</>;
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
  /** A format shown as Folder (label and colour): the LZ4 tab's packed folder. */
  asFolder?: F;
  className?: string;
}) {
  useLang(); // re-render on a language switch: kindLabel reads the current one
  const formats = props.formats ?? (KINDS as F[]);
  return (
    <div
      className={`seg formats ${props.className ?? ""}`}
      role="radiogroup"
      aria-label={props.label}
    >
      {formats.map((f) => {
        const disabled = props.disabled?.(f) ?? false;
        const shown = f === props.asFolder ? "folder" : f;
        const why = disabled ? props.reason?.(f) : undefined;
        return (
          // `on` from state, not `input:checked + …`: WebKit misses that restyle when React
          // sets `checked` by code (a new source switching the format).
          <label
            key={f}
            className={`seg-choice fmt-${shown}${props.value === f ? " on" : ""}`}
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
              {kindLabel(shown)}
              {why &&<span className="visually-hidden">, {why}</span>}
            </span>
          </label>
        );
      })}
    </div>
  );
}

export const IMAGE_EXTENSIONS = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

/** What the Image/Archive picker offers besides images: an archive, or the first part of a
 * multi-part one (`.part1.rar` ends in .rar, `.7z.001`/`.zip.001`, a split zip's `.zip`; `.z01`
 * too, so core can name the part to choose instead). */
export const ARCHIVE_EXTENSIONS = ["zip", "7z", "rar", "001", "z01"];

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
 * WebKit has no `inert`). Give the first control `autoFocus`. Without `close` (Stopping, has
 * stopped) it takes no keys or clicks at all: Escape stays the browser's (the PS5's Circle
 * leaves the page). */
export function Modal(props: {
  close?: () => void;
  className: string;
  role?: "dialog" | "alertdialog";
  labelledBy: string;
  describedBy?: string;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const onKey = (e: KeyboardEvent) => {
    if (!props.close) return;
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
        if (e.target !== e.currentTarget || !props.close) return;
        e.preventDefault(); // focus stays where it was until the overlay gives it back
        props.close();
      }}
    >
      <div
        className={props.className}
        role={props.role ?? "dialog"}
        aria-modal="true"
        aria-labelledby={props.labelledBy}
        aria-describedby={props.describedBy}
        ref={box}
        // A click on the dialog's text focuses the box, so Escape and the trap still reach it.
        tabIndex={-1}
        onKeyDown={onKey}
      >
        {props.children}
      </div>
    </div>
  );
}

// ---- Deleted sources ----

const deletedSubs = new Set<(path: string) => void>();

/** Calls `f` with every path Edit or the Library deletes; returns the unsubscribe. */
export function onDeleted(f: (path: string) => void): () => void {
  deletedSubs.add(f);
  return () => void deletedSubs.delete(f);
}

/** A game Library handed to Edit or Convert; a new `id` is a new request, even for the same
 * path (the screen reads it again). */
export interface SourceSelection {
  id: number;
  path: string;
  kind: Kind;
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

/** The picker for a game folder, or an image or archive file; `onPick` unless cancelled
 * (`isImage`: a file, so not a plain folder). */
export async function pickSource(
  what: "folder" | "image",
  onPick: (path: string, isImage: boolean) => void,
): Promise<void> {
  const t = msg().common;
  const p =
    what === "folder"
      ? await pick({ directory: true, title: t.pickFolder })
      : await pick({
          directory: false,
          title: t.pickImage,
          filterName: t.pickImageFilter,
          extensions: [...IMAGE_EXTENSIONS, ...ARCHIVE_EXTENSIONS],
        });
  if (p !== null) onPick(p, what === "image");
}

/** "Folder" / "Image/Archive": a game folder, or an image or archive file. */
function PickButtons(props: { onPick: (path: string, isImage: boolean) => void; big?: boolean; disabled?: boolean }) {
  const t = useMessages().common;
  const pickFolder = () => pickSource("folder", props.onPick);
  const pickImage = () => pickSource("image", props.onPick);
  return (
    <>
      <button className={props.big ? "primary" : "small"} onClick={pickFolder} disabled={props.disabled}>
        <Icon name="folder" />
        {t.pickFolderButton}
      </button>
      <button className={props.big ? undefined : "small"} onClick={pickImage} disabled={props.disabled}>
        <Icon name="archive" />
        {t.pickImageButton}
      </button>
    </>
  );
}

/** Core's "needs a password" error, as the UI says it. */
export function isPasswordRequired(error: string | null): boolean {
  return error !== null && error.includes(ARCHIVE_PASSWORD_REQUIRED);
}

/**
 * The Source card: an empty state with the pickers until a source is chosen, then the game
 * it holds (or why it can't be read). `full` adds what only Edit shows; `compact` (Edit) keeps the
 * card to the cover, title, kind, title ID, version and findings, with `children` (Edit's folds)
 * at its foot.
 */
export function SourceCard(props: {
  path: string | null;
  onPick: (path: string, isImage: boolean) => void;
  busy: boolean;
  error: string | null;
  ins: Inspection | null;
  full?: boolean;
  /** Edit's small card: the facts beyond title ID and version go into `children`'s folds. */
  compact?: boolean;
  /** At the card's foot once a source is chosen (Edit's folds). */
  children?: ReactNode;
  /** Keeps element ids apart per screen; default "i" with `full`, else "c". */
  prefix?: string;
  /** Stands in for the default empty state (the LZ4 tab's). */
  empty?: ReactNode;
  /** More buttons in the title row once a source is chosen (Edit's Delete). */
  actions?: ReactNode;
  /** An archive that needs a password was left locked (Cancel): ask again. */
  onUnlock?: () => void;
  /** The source can't change now (Edit, while changes are applied): the pick buttons are off. */
  locked?: boolean;
}) {
  const t = useMessages().common;
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
      className={`card hero${props.full ? " full" : ""}${props.compact ? " compact-src" : ""}${glow.ready ? " lit" : ""}`}
      style={glow.style}
      aria-labelledby={headId}
    >
      <CardHead icon="disc" title={t.source} id={headId} focusable>
        {path && <PickButtons onPick={props.onPick} disabled={props.locked} />}
        {/* Last, like the Library's: a destructive button away from the pickers. */}
        {path && props.actions}
      </CardHead>
      {!path && props.empty ? (
        props.empty
      ) : !path ? (
        <div className="empty">
          <span className="empty-icon">
            <Icon name="disc" />
          </span>
          <p className="empty-title">{t.emptyTitle}</p>
          <p className="muted">{t.emptyText}</p>
          <div className="kinds" aria-label={t.accepted}>
            {[...KINDS, ...ARCHIVE_KINDS].map((k) => (
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
          {isPasswordRequired(props.error) ? (
            <div className="row locked">
              <p className="bad">{t.needsPassword}</p>
              {props.onUnlock && (
                <button className="small" onClick={props.onUnlock}>
                  <Icon name="lock" />
                  {t.enterPassword}
                </button>
              )}
            </div>
          ) : (
            props.error && <p className="bad backend">{props.error}</p>
          )}
          {ins && (props.compact ? <CompactSummary ins={ins} /> : <InspectSummary ins={ins} full={props.full} />)}
          {props.children}
        </>
      )}
    </section>
  );
}

/** "." "..", "..." in turn after a waiting text (static "..." with reduced motion). */
export function Dots() {
  return (
    <span className="dots" aria-hidden="true">
      <span>.</span>
      <span>.</span>
      <span>.</span>
    </span>
  );
}

/** Cover, title, kind tags and the source's path. */
function GameHead(props: { path: string; ins: Inspection | null; busy: boolean; full?: boolean }) {
  const t = useMessages().common;
  const { ins } = props;
  const title = ins
    ? (gameTitle(ins.title_names, ins.title_name) ?? ins.title_id ?? t.unknownTitle)
    : props.busy
      ? <>{t.reading}<Dots /></>
      : t.unreadable;
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
          {ins && ins.backport.length > 0 && <span className="tag orange">{t.backport}</span>}
          {ins && ins.emulators.length > 0 && <span className="tag">{t.emulators}</span>}
          {ins && ins.dlcs.length > 0 && <span className="tag violet">DLC</span>}
          {ins?.additional_content && <span className="tag violet">{t.additionalContent}</span>}
        </div>
        <PathLine path={props.path} />
      </div>
    </div>
  );
}

/** The game's facts as inset tiles. */
function Facts({ ins, full, rest }: { ins: Inspection; full: boolean; rest?: boolean }) {
  const t = useMessages().common;
  const bp = ins.backport.length > 0;
  const emus = emulatorNames(ins);
  // With a fakelib/ (backport or emulators) the game runs as low as its executables allow.
  const fromExe = ins.backport_firmware !== null;
  const minFw = ins.backport_firmware ?? ins.firmware;
  const fwNote: string[] = [];
  if (fromExe && ins.firmware) fwNote.push(t.declares(ins.firmware));
  if ((bp || emus.length > 0) && !fromExe) fwNote.push(t.fwUnknown);
  if (bp) fwNote.push(backportLib(ins));
  if (emus.length > 0)
    fwNote.push(t.emulatorList(emus.map((n) => (n === "Other" ? t.otherEmulator : n)).join(", ")));
  if (full && ins.sdk) fwNote.push(t.sdk(ins.sdk));
  return (
    <dl className="tiles">
      {!rest && <IdTiles ins={ins} />}
      <div className="tile">
        <dt>{t.firmware}</dt>
        <dd>
          <span className="value">{minFw ? `${minFw}+` : "—"}</span>
          {minFw && (
            <span className={!fromExe ? "tag blue" : bp ? "tag orange" : "tag"}>
              {!fromExe ? t.fwDeclared : bp ? t.fwBackport : t.fwExecutables}
            </span>
          )}
          {fwNote.length > 0 && <span className="note">{fwNote.join(" · ")}</span>}
        </dd>
      </div>
      <div className="tile">
        <dt>{t.size}</dt>
        <dd>
          <span className="value">{prettyBytes(ins.total_bytes)}</span>
          <span className="tag">{t.files(ins.files.length)}</span>
          {ins.archive && (
            <span className="tag">{t.inArchive(`.${ins.archive.format}`, prettyBytes(ins.archive.archive_bytes))}</span>
          )}
        </dd>
      </div>
      {full && ins.archive && <ArchiveTile facts={ins.archive} />}
      {ins.lz4 && lz4Shown(ins.lz4) && <Lz4Tile facts={ins.lz4} />}
      {full && (
        <div className="tile wide">
          <dt>{t.contentId}</dt>
          <dd className={ins.content_id ? "value mono" : "value"}>{ins.content_id ?? "—"}</dd>
        </div>
      )}
      {full && ins.forge_version !== null && (
        <div className="tile wide">
          <dt>{APP_NAME}</dt>
          <dd className="value mono">v{ins.forge_version}</dd>
        </div>
      )}
    </dl>
  );
}

/** An archive source, in Edit: its parts and how it is packed, and where the game sits. */
function ArchiveTile({ facts }: { facts: NonNullable<Inspection["archive"]> }) {
  const t = useMessages().common;
  const notes = [facts.root ? t.archiveRoot(facts.root) : t.archiveAtRoot];
  if (facts.solid) notes.push(t.solid);
  if (facts.encrypted) notes.push(t.encrypted);
  return (
    <div className="tile wide">
      <dt>{t.archive}</dt>
      <dd>
        <span className="value">{t.parts(facts.volumes.length)}</span>
        <span className="note">{notes.join(" · ")}</span>
        {facts.volumes.length > 1 && <span className="note path">{facts.volumes.join(", ")}</span>}
      </dd>
    </div>
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
  const t = useMessages().common;
  const { state, detail } = lz4State(facts);
  const notes = [detail];
  if (!facts.imports_ampr) notes.push(t.noAmpr);
  const runtime = facts.imports_ampr ? runtimeAction(facts) : null;
  if (runtime) notes.push(runtime);
  if (facts.index && facts.index !== "ok") notes.push(t.index[facts.index]);
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

/** What a conversion does to an AMPR game's runtime, as the CLI's `inspect` says it: installs
 * or replaces with the bundled ampr_emu, or keeps the game's own (named only when it is another
 * build: Forge's release runtime is the bundled one). Null when there is nothing to say. */
function runtimeAction(f: Lz4Facts): string | null {
  const t = msg().common;
  const shipped = f.shipped_runtime_version;
  if (!shipped || !f.runtime_action) return null;
  if (f.runtime_action !== "keep") return t.runtimeAction(f.runtime_action, shipped);
  return f.runtime === "other" && f.runtime_version ? t.runtimeAction("keep", f.runtime_version) : null;
}

/** The LZ4 (AMPR) tile shows for a game with its own ampr_emu, any LZ4 artifact (packs, a
 * damaged manifest, traces), an ampr_emu.index a conversion rewrites, or an AMPR game a
 * conversion gives the bundled ampr_emu. */
function lz4Shown(f: Lz4Facts): boolean {
  return (
    f.runtime !== "none" ||
    !!f.packed ||
    f.manifest_error !== null ||
    f.journal_bytes !== null ||
    (!!f.index && f.index !== "ok") ||
    (f.imports_ampr && f.runtime_action === "install")
  );
}

/** "Packed" / "Damaged packs" / "Traced" / "Plain", and what backs it (read at call time). */
export function lz4State(f: Lz4Facts): { state: string; detail: string } {
  const t = msg().common;
  if (f.packed) {
    const p = f.packed;
    return { state: t.packed, detail: t.packedDetail(p.files, p.volumes, p.stored_percent) };
  }
  if (f.manifest_error !== null) return { state: t.damaged, detail: f.manifest_error };
  if (f.journal_bytes !== null) return { state: t.traced, detail: t.journal(prettyBytes(f.journal_bytes)) };
  if (f.runtime === "forge_trace") return { state: t.traced, detail: t.noJournal };
  return { state: t.plain, detail: t.runtimeIs(t.runtime[f.runtime]) };
}

/** Which fakelib folder a backport ships, and how many backport libraries it has. */
function backportLib(ins: Inspection): string {
  const exclusive = ins.backport.some((p) => /^fakelib2\//i.test(p));
  return msg().common.backportLibs(exclusive, ins.backport.length);
}

/** The emulators in fakelib/, each named once ("AMPR", "DLC", "PlayGo", "Other"). */
export function emulatorNames(ins: Inspection): string[] {
  return [...new Set(ins.emulators.map((e) => e.name))];
}

/** Preflight findings, each with a status icon; nothing at all when there are none. */
export function Findings({ lines }: { lines: string[] }) {
  const t = useMessages().common;
  if (lines.length === 0) return null;
  return (
    <ul className="findings" aria-label={t.findings}>
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
  // PFS holds ASCII names only; a .ffpfsc defaults to ffpkg inside, which takes them.
  if (/^name not ASCII/.test(line)) return { kind: "warn", formats: ["ffpfs"] };
  if (/^extraction:/.test(line)) return { kind: "warn", formats: ["folder", "lz4"] };
  if (/^(already 512-byte sectors|geometry differs from the SMP fast path)/.test(line))
    return { kind: "info" };
  return { kind: "warn" };
}

/** The DLC embedded in the dump, collapsed until asked for. */
function DlcList({ dlcs }: { dlcs: Inspection["dlcs"] }) {
  const t = useMessages().common;
  if (dlcs.length === 0) return null;
  const notes = dlcs.map(dlcNote);
  // One shared explanation goes on top once instead of under every row.
  const shared = notes.every((n) => n === notes[0]) ? notes[0] : null;
  return (
    <details className="fold">
      <summary>{t.dlcTitle(dlcs.length)}</summary>
      {shared && <p className="muted fold-note">{t.dlcAll(shared)}</p>}
      <ul className="rows scroll" tabIndex={0} aria-label={t.dlcLabel}>
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
  const t = msg().common;
  if (d.folder) return t.dlcIn(d.folder, prettyBytes(d.bytes));
  if (d.emulated === "NO_EXTRA_DATA") return t.dlcNoData;
  if (d.emulated !== null) return t.dlcStatus(d.emulated);
  return t.dlcMerged;
}

/** The title ID and version tiles. */
function IdTiles({ ins }: { ins: Inspection }) {
  const t = useMessages().common;
  return (
    <>
      <div className="tile">
        <dt>{t.titleId}</dt>
        <dd className={ins.title_id ? "value mono" : "value"}>{ins.title_id ?? "—"}</dd>
      </div>
      <div className="tile">
        <dt>{t.version}</dt>
        <dd className="value">{ins.version ?? "—"}</dd>
      </div>
    </>
  );
}

/** Edit's compact card: title ID and version, then the findings. */
function CompactSummary({ ins }: { ins: Inspection }) {
  return (
    <div className="summary">
      <dl className="tiles">
        <IdTiles ins={ins} />
      </dl>
      <Findings lines={ins.findings} />
    </div>
  );
}

/** The game's facts, its embedded DLC (and backport and emulator files, `full`), then the
 * findings. `rest`: Edit's facts fold, without what its compact card already shows (title ID,
 * version, findings). */
export function InspectSummary({ ins, full = false, rest = false }: { ins: Inspection; full?: boolean; rest?: boolean }) {
  const t = useMessages().common;
  return (
    <div className="summary">
      <Facts ins={ins} full={full} rest={rest} />
      {(ins.dlcs.length > 0 ||
        (full && (ins.backport.length > 0 || ins.emulators.length > 0))) && (
        <div className="folds">
          <DlcList dlcs={ins.dlcs} />
          {full && ins.backport.length > 0 && (
            <details className="fold">
              <summary>
                {t.backportFiles} ({num(ins.backport.length)})
              </summary>
              <ul className="rows scroll" tabIndex={0} aria-label={t.backportFiles}>
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
              <summary>
                {t.emulatorsIn} ({num(ins.emulators.length)})
              </summary>
              <ul className="rows scroll" tabIndex={0} aria-label={t.emulatorsIn}>
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
      {!rest && <Findings lines={ins.findings} />}
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
  const t = useMessages().common;
  const [failed, setFailed] = useState(false);
  const address = serverAddress();
  if (!address || failed) return null;
  return (
    <img
      className="qr"
      src="/api/qr"
      width={props.size}
      height={props.size}
      alt={t.qr(`http://${address}`)}
      onError={() => setFailed(true)}
    />
  );
}
