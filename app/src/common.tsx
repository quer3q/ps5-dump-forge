
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
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
import { nasName, nasPath } from "./peers";
import { useCoverGlow } from "./glow";
import { Icon, type IconName } from "./icons";
import { useSavedNames } from "./useConnectState";
import { getPrefs } from "./prefs";
import "./source-card.css";

// Format names stay as they are in every language; only "Folder" is translated (kindLabel).
const KIND_LABEL: Record<Format | ArchiveKind, string> = {
  folder: "Folder",
  exfat: "exfat",
  ffpkg: "ffpkg",
  ffpfs: "ffpfs",
  ffpfsc: "ffpfsc",
  pkg: "FPKG", // the debug FPKG's display name; its file still ends in .pkg
  lz4: "LZ4", // an LZ4 packed folder
  zip: "zip", // archives: a source only
  "7z": "7z",
  rar: "rar",
};

/** Source kinds in picker order (the LZ4 packed folder is Build's). */
export const KINDS: Kind[] = ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

export const ARCHIVE_KINDS: ArchiveKind[] = ["zip", "7z", "rar"];

export function isArchive(kind: string | undefined): boolean {
  return ARCHIVE_KINDS.includes(kind as ArchiveKind);
}

/** Read at call time (a component re-renders on a switch through its own `useMessages`). */
export function kindLabel(kind: string): string {
  if (kind === "folder") return msg().common.folder;
  if (kind === "zstd") return "ZSTD";
  return KIND_LABEL[kind as Format | ArchiveKind] ?? kind;
}

export function FormatPill({ kind }: { kind: string }) {
  useLang(); // re-render on a language switch: kindLabel reads the current one
  const known = kind in KIND_LABEL || kind === "zstd";
  // The three archives share one hue: what they hold is the same.
  const hue = isArchive(kind) ? "archive" : kind;
  return <span className={known ? `tag fmt fmt-${hue}` : "tag"}>{kindLabel(kind)}</span>;
}

/** Format names as words, not `.exfat` nor `fpkg-enable`; the char before is matched since the PS5's WebKit has no lookbehind. */
const FORMAT_NAME = /(^|[^.\w-])(ffpfsc|ffpfs|ffpkg|exfat|fpkg)(?![\w-])/g;

/** Copy only: a path or file name keeps its dot and stays plain. */
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

export function Head({ id, icon, title }: { id: string; icon: IconName; title: string }) {
  return (
    <div className="modal-head">
      <span className="head-icon">
        <Icon name={icon} />
      </span>
      <h2 id={id} dir="auto">
        {title}
      </h2>
    </div>
  );
}

export function CardHead(props: {
  icon?: IconName;
  title: string;
  id?: string;
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

export function tabId(prefix: string, id: string): string {
  return `${prefix}-tab-${id}`;
}
export function panelId(prefix: string, id: string): string {
  return `${prefix}-panel-${id}`;
}

/** WAI-ARIA tabs; the caller renders each panel with `panelId`/`tabId` and `hidden`. */
export function TabList<T extends string>(props: {
  label: string;
  prefix: string;
  tabs: { id: T; label: ReactNode }[];
  value: T;
  onChange: (id: T) => void;
  className?: string;
}) {
  const ids = props.tabs.map((t) => t.id);
  // A bar that scrolls keeps the chosen tab in view; by hand, since older WebKit's scrollIntoView would scroll the page.
  useEffect(() => {
    const tab = document.getElementById(tabId(props.prefix, props.value));
    const bar = tab?.parentElement;
    if (!tab || !bar || bar.scrollWidth <= bar.clientWidth) return;
    const t = tab.getBoundingClientRect();
    const b = bar.getBoundingClientRect();
    if (t.left < b.left) bar.scrollLeft -= b.left - t.left;
    else if (t.right > b.right) bar.scrollLeft += t.right - b.right;
  }, [props.prefix, props.value]);
  const onKey = (e: KeyboardEvent) => {
    const i = ids.indexOf(props.value);
    const rtl = document.documentElement.dir === "rtl";
    const next =
      e.key === (rtl ? "ArrowLeft" : "ArrowRight") ? ids[(i + 1) % ids.length]
      : e.key === (rtl ? "ArrowRight" : "ArrowLeft") ? ids[(i - 1 + ids.length) % ids.length]
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
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={tabId(props.prefix, t.id)}
            aria-selected={on}
            aria-controls={panelId(props.prefix, t.id)}
            tabIndex={on ? 0 : -1}
            className={`seg-opt${on ? " on" : ""}`}
            onClick={() => props.onChange(t.id)}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

/** Native radios underneath (visually hidden), so keys and semantics come from the browser. */
export function FormatPicker<F extends Format>(props: {
  name: string;
  label: string;
  value: F;
  onChange: (f: F) => void;
  formats: F[];
  className?: string;
}) {
  useLang(); // re-render on a language switch: kindLabel reads the current one
  return (
    <div className={`seg formats ${props.className ?? ""}`} role="radiogroup" aria-label={props.label}>
      {props.formats.map((f) => {
        return (
          // `on` from state, not `input:checked + …`: WebKit misses that restyle when React
          // sets `checked` by code (a new source switching the format).
          <label
            key={f}
            className={`seg-choice fmt-${f}${props.value === f ? " on" : ""}`}
          >
            <input
              type="radio"
              className="visually-hidden"
              name={props.name}
              value={f}
              checked={props.value === f}
              onChange={() => props.onChange(f)}
            />
            <span className="seg-opt">
              {kindLabel(f)}
            </span>
          </label>
        );
      })}
    </div>
  );
}

export const IMAGE_EXTENSIONS = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

/** Also a multi-part archive's first part (`.7z.001`, `.zip.001`; `.z01` so core can name the part to choose). */
export const ARCHIVE_EXTENSIONS = ["zip", "7z", "rar", "001", "z01"];

/** A Mac (or iPad) keyboard: its key marked delete sends Backspace, so the explorers' tips name fn+Delete. */
export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent);

export { basename, dirname, joinPath, SEPARATORS } from "./paths";

/** The folder part ellipsizes, the name stays; the full path is the tooltip. */
export function PathLine({ path: raw, prefix }: { path: string; prefix?: string }) {
  const path = usePeerPath()(raw);
  const name = basename(path) || path;
  const dir = name === path ? "" : path.slice(0, trimSep(path).length - name.length);
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

/** `flink://` or a saved NAS's path as "<device name>: /<rel>" while that device is saved. */
export function usePeerPath(): (path: string) => string {
  const { devices, nas } = useSavedNames();
  return useCallback(
    (path: string) => {
      const p = peerOf(path, devices);
      return p ? `${p.name}: ${p.rest}` : (nasPath(path, nas) ?? path);
    },
    [devices, nas],
  );
}

/** By name; `null` for this device's paths. */
export function useDeviceName(): (path: string) => string | null {
  const { devices, nas } = useSavedNames();
  return useCallback(
    (path: string) => {
      const peer = peerOf(path, devices);
      if (peer) return peer.name;
      if (/^flink:\/\//i.test(path)) return path.split(/[/\\]/)[2];
      return nasName(path, nas);
    },
    [devices, nas],
  );
}

function peerOf(path: string, devices: { key_id: string; name: string }[] | undefined) {
  // The key ends at either separator: a Windows join gives `flink://<key>\name`.
  const m = /^flink:\/\/([^/\\]+)(.*)$/i.exec(path);
  const d = m && devices?.find((x) => x.key_id === m[1]);
  return d && m ? { name: d.name, rest: m[2] || "/" } : null;
}

// ---- Dialogs over the whole app ----

const overlays = new Set<() => void>();
const returns = new Map<HTMLElement, { to: HTMLElement | null; closed: boolean }>();

/** Another device's pairing request is up: a dialog under it leaves focus in the prompt. */
export function pairPromptUp(): boolean {
  return document.querySelector(".pair-layer") !== null;
}

/** Closes every open dialog (as if cancelled): the quit prompt never stacks behind one. */
export function closeOverlays(): void {
  for (const close of [...overlays]) close();
}

/** A dialog in its own React root; the app behind it is inert; focus goes back to what had it. */
export function openOverlay(render: (close: () => void) => ReactNode, onClosed?: () => void): void {
  let opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  // Opened from a dialog closing now: give focus back to that one's opener (its button is gone by then).
  for (const [h, r] of returns)
    if (r.closed && opener && h.contains(opener)) {
      opener = r.to;
      r.to = null;
    }
  const app = document.getElementById("root");
  app?.setAttribute("inert", "");
  app?.setAttribute("aria-hidden", "true");
  const under = [...returns].filter(([h, r]) => !r.closed && !h.hasAttribute("inert")).map(([h]) => h);
  for (const h of under) {
    h.setAttribute("inert", "");
    h.setAttribute("aria-hidden", "true");
  }
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  const back = { to: opener, closed: false };
  returns.set(host, back);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    back.closed = true;
    overlays.delete(close);
    for (const h of under) {
      h.removeAttribute("inert");
      h.removeAttribute("aria-hidden");
    }
    if (overlays.size === 0) {
      app?.removeAttribute("inert");
      app?.removeAttribute("aria-hidden");
    }
    setTimeout(() => {
      root.unmount();
      host.remove();
      returns.delete(host);
      if (!pairPromptUp()) back.to?.focus();
    }, 0);
    onClosed?.();
  };
  overlays.add(close);
  root.render(render(close));
}

/** Not a button taken out of the Tab order (a TabList's or ViewSwitch's unselected options). */
export const FOCUSABLE =
  'button:not(:disabled):not([tabindex="-1"]), a[href], input:not(:disabled), [tabindex]:not([tabindex="-1"])';

/** Escape and a click outside close it; Tab stays inside (old WebKit has no `inert`). Without `close` it takes no keys at all (the PS5's Circle leaves the page). */
export function Modal(props: {
  close?: () => void;
  className: string;
  role?: "dialog" | "alertdialog";
  labelledBy: string;
  describedBy?: string;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  // Opened under the pairing prompt: its autoFocus took focus from the prompt; give it back.
  useEffect(() => {
    if (pairPromptUp() && !box.current?.closest(".pair-layer"))
      document.querySelector(".pair-layer")?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
  }, []);
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


const deletedSubs = new Set<(path: string) => void>();

export function onDeleted(f: (path: string) => void): () => void {
  deletedSubs.add(f);
  return () => void deletedSubs.delete(f);
}

/** A new `id` is a new request, even for the same path. */
export interface SourceSelection {
  id: number;
  path: string;
  kind: Kind | ArchiveKind;
}

export function notifyDeleted(path: string): void {
  for (const f of deletedSubs) f(path);
}

/** By spelling (the server compared the real paths). */
export function isWithin(path: string, dir: string): boolean {
  const p = trimSep(path);
  const d = trimSep(dir);
  return p === d || (p.startsWith(d) && SEPARATORS.test(p[d.length] ?? ""));
}

/** A file click picks an image or archive (`isImage`), "Choose this folder" the folder shown. */
export async function pickSource(onPick: (path: string, isImage: boolean) => void): Promise<void> {
  const t = msg().common;
  // A saved NAS or peer is a tab: no .pkg or archive there, core reads those from a local path only.
  const p = await pick({
    directory: false,
    folderToo: true,
    title: t.pickAny,
    extensions: [...IMAGE_EXTENSIONS, ...ARCHIVE_EXTENSIONS],
    nas: true,
    nasExtensions: NAS_IMAGES,
    devices: true,
  });
  // ponytail: a folder named like an image (`game.exfat/`) reads as an image here; core's
  // inspect then says what it is.
  if (p !== null) onPick(p, isImagePath(p));
}

const isImagePath = (p: string): boolean => {
  const ext = /\.([^./\\]+)$/.exec(p)?.[1]?.toLowerCase();
  return !!ext && (IMAGE_EXTENSIONS.includes(ext) || ARCHIVE_EXTENSIONS.includes(ext) || /^(part\d+\.rar|r\d\d|\d{3})$/.test(ext));
};

const NAS_IMAGES = ["exfat", "ffpkg", "ffpfs", "ffpfsc"];

/** Read only, like an archive. */
export const isNas = (path: string | null | undefined): boolean => !!path && /^(smb|ftp|nfs):\/\//i.test(path);
export const isPeer = (path: string | null | undefined): boolean => !!path && /^flink:\/\//i.test(path);
export const isNetwork = (path: string | null | undefined): boolean => isNas(path) || isPeer(path);

function PickButtons(props: { onPick: (path: string, isImage: boolean) => void; big?: boolean; disabled?: boolean }) {
  const t = useMessages().common;
  return (
    <button className={props.big ? "primary" : "small primary"} onClick={() => void pickSource(props.onPick)} disabled={props.disabled}>
      <Icon name="disc" />
      {t.pickButton}
    </button>
  );
}

export function isPasswordRequired(error: string | null): boolean {
  return error !== null && error.includes(ARCHIVE_PASSWORD_REQUIRED);
}

/** The game card (Build's, Edit's stacked): cover, title, chips, the ID/firmware/size tiles, findings; `children` at its foot. */
export function SourceCard(props: {
  path: string | null;
  onPick: (path: string, isImage: boolean) => void;
  busy: boolean;
  error: string | null;
  ins: Inspection | null;
  children?: ReactNode;
  /** Keeps element ids apart per screen; default "c". */
  prefix?: string;
  actions?: ReactNode;
  /** An archive left locked (Cancel): ask again. */
  onUnlock?: () => void;
  /** Edit, while changes are applied: the pick buttons are off. */
  locked?: boolean;
  /** false: no findings list (Build lists them in its Summary). */
  findings?: boolean;
  /** Build's card, the cover above the text (Edit's narrow column). */
  stacked?: boolean;
}) {
  const t = useMessages().common;
  const { path, ins } = props;
  const headId = `${props.prefix ?? "c"}-source`;
  const glow = useCoverGlow(!path ? null : ins ? ins.cover : props.busy ? undefined : null);
  const wide = !!ins;
  // Couldn't be read (not a password to ask for): an error state with Pick, as the empty one.
  const failed = !!path && !ins && !props.busy && !!props.error && !isPasswordRequired(props.error);

  // The first pick unmounts the focused empty-state buttons: hand focus to the title.
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
      className={`card hero${wide ? " src-2b" : ""}${wide && props.stacked ? " stack" : ""}${glow.ready ? " lit" : ""}`}
      style={glow.style}
      aria-label={t.source}
      id={headId}
      tabIndex={-1}
    >
      {/* No title (the section is labelled): Pick, and Edit's Delete, at the head's end. */}
      {path && !failed && (
        <header className="card-head src-head">
          <div className="head-actions">
            <PickButtons onPick={props.onPick} disabled={props.locked} />
            {/* Last, like the File explorer's: a destructive button away from the pickers. */}
            {props.actions}
          </div>
        </header>
      )}
      {failed ? (
        <div className="empty" role="alert">
          <span className="empty-icon bad">
            <Icon name="alert" />
          </span>
          <p className="empty-title">{t.unreadable}</p>
          {path && <PathLine path={path} />}
          <p className="bad backend">{props.error}</p>
          <div className="row center">
            <PickButtons onPick={props.onPick} big disabled={props.locked} />
          </div>
        </div>
      ) : !path ? (
        <div className="empty">
          <span className="empty-icon">
            <Icon name="disc" />
          </span>
          <p className="empty-title">{t.emptyTitle}</p>
          <div className="kinds" role="group" aria-label={t.accepted}>
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
          <GameHead path={path} ins={ins} busy={props.busy} titleId={`${headId}-title`} />
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
          {ins && <InspectSummary ins={ins} ampr={false} findings={props.findings} />}
          {/* Build says it in its summary instead. */}
          {ins && props.findings !== false && !ins.patch && !!ins.unsigned?.length && (
            <p className="note-line warn interrupted-note">
              <Icon name="warn" />
              <span>{t.unsigned(ins.unsigned.length)}</span>
            </p>
          )}
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

function GameHead(props: {
  path: string;
  ins: Inspection | null;
  busy: boolean;
  titleId: string;
}) {
  const t = useMessages().common;
  const { ins } = props;
  const device = useDeviceName()(props.path);
  const title = ins
    ? (gameTitle(ins.title_names, ins.title_name) ?? ins.title_id ?? t.unknownTitle)
    : props.busy
      ? <>{t.reading}<Dots /></>
      : t.unreadable;
  return (
    <div className="game">
      {ins?.cover ? (
        <img className="cover" src={ins.cover} alt="" />
      ) : (
        <div className="cover none" aria-hidden="true">
          <Icon name={ins ? "disc" : "folder"} />
        </div>
      )}
      <div className="game-text">
        <div className="title-row">
          <h3 className={ins ? "title" : "title pending"} id={props.titleId} tabIndex={-1}>
            {title}
          </h3>
          {ins && <FormatPill kind={ins.kind} />}
          {/* Packed: its pack kind right after the kind (a packed game is still a folder or image). */}
          {(ins?.zstd_packed || ins?.lz4?.packed) && <span className="tag green">{ins.zstd_packed ? "ZSTD" : "LZ4"}</span>}
          {device && (
            <span className="tag green" title={device}>
              {device}
            </span>
          )}
          {ins && ins.backport.length > 0 && <span className="tag orange">{t.backport}</span>}
          {ins && ins.emulators.length > 0 && <span className="tag">{t.emulators}</span>}
          {ins && ins.dlcs.length > 0 && <span className="tag violet">DLC</span>}
          {ins?.additional_content && <span className="tag violet">{t.additionalContent}</span>}
          {ins?.patch && <span className="tag orange">{t.patch}</span>}
        </div>
        {ins && <SourceChips ins={ins} />}
        <PathLine path={props.path} />
      </div>
    </div>
  );
}

function SourceChips({ ins }: { ins: Inspection }) {
  const t = useMessages().common;
  const c = useMessages().sourceChips;
  const fw = ins.backport_firmware ?? ins.firmware;
  const eboot = ins.files.some((f) => f.path === "eboot.bin");
  return (
    <ul className="src-chips" aria-label={t.source}>
      {ins.sdk && <li className="tag">{c.sdk(ins.sdk)}</li>}
      {fw && <li className="tag">{c.firmware(fw)}</li>}
      {eboot && <li className="tag mono">eboot.bin</li>}
    </ul>
  );
}

function Facts({ ins, full, rest, ampr }: { ins: Inspection; full: boolean; rest?: boolean; ampr: boolean }) {
  const t = useMessages().common;
  const bp = ins.backport.length > 0;
  const emus = emulatorNames(ins);
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
      {ampr && (ins.zstd_packed || (ins.lz4 && lz4Shown(ins.lz4))) && <AmprTile ins={ins} />}
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

/** An AMPR game's runtime (with its version as the title) and packs, as the CLI's `inspect` says them. */
function AmprTile({ ins }: { ins: Inspection }) {
  const t = useMessages().common;
  const facts = ins.lz4;
  const zstd = ins.zstd_packed;
  const damaged = !zstd && !!facts && facts.manifest_error !== null;
  // No runtime: the title says it all, no notes.
  const noRuntime = !facts || facts.runtime === "none";
  const notes: string[] = zstd && !noRuntime ? [t.zstdKept] : [];
  let detail: ReactNode = null;
  if (!zstd && facts) {
    if (facts.packed) {
      const p = facts.packed;
      detail = t.packedDetail(p.files, p.volumes, p.stored_percent);
    } else if (damaged) {
      // Core's English, isolated for RTL.
      detail = <bdi>{facts.manifest_error}</bdi>;
    }
    if (facts.runtime !== "none") {
      if (!facts.imports_ampr) notes.push(t.noAmpr);
      const runtime = facts.imports_ampr ? runtimeAction(facts) : null;
      if (runtime) notes.push(runtime);
      const index = getPrefs().replaceAmprEmu ? facts.index : (facts.keep_index ?? facts.index);
      if (index && index !== "ok") notes.push(t.index[index]);
    }
  }
  const packed = zstd ? "ZSTD" : facts?.packed ? "LZ4" : null;
  const state = packed ? t.packed(packed) : damaged ? t.damaged : t.unpacked;
  const look = packed ? " neon packed" : damaged ? " damaged" : " unpacked";
  return (
    <div className={`tile wide${look}`}>
      <dt>{facts ? <bdi>{runtimeName(facts)}</bdi> : t.noRuntime}</dt>
      <dd>
        <span className="value">{state}</span>
        {(detail !== null || notes.length > 0) && (
          <span className="note">
            {detail}
            {notes.map((n, i) => (i > 0 || detail !== null ? ` · ${n}` : n)).join("")}
          </span>
        )}
      </dd>
    </div>
  );
}

function runtimeName(f: Lz4Facts): string {
  const t = msg().common;
  const v = f.runtime_version ? ` ${f.runtime_version}` : "";
  if (f.runtime === "zstd") return `ampr-emu-ng${v}`;
  if (f.runtime === "forge_release" || f.runtime === "other") return `ampr_emu${v}`;
  if (f.runtime === "forge_trace") return t.traceRuntime;
  return t.noRuntime;
}

/** Said only for an ampr_emu build the switch keeps; null when there is nothing to say. */
function runtimeAction(f: Lz4Facts): string | null {
  const t = msg().common;
  const replace = getPrefs().replaceAmprEmu;
  const action = replace ? f.runtime_action : (f.keep_runtime_action ?? f.runtime_action);
  if (!action) return null;
  if (action === "keep") return !replace && f.runtime_action === "replace" ? t.runtimeAction("keep") : null;
  return t.runtimeAction(action);
}

function lz4Shown(f: Lz4Facts): boolean {
  const index = f.index;
  return (
    f.runtime !== "none" ||
    !!f.packed ||
    f.manifest_error !== null ||
    (!!index && index !== "ok") ||
    (f.imports_ampr && f.runtime_action === "install")
  );
}

function backportLib(ins: Inspection): string {
  const exclusive = ins.backport.some((p) => /^fakelib2\//i.test(p));
  return msg().common.backportLibs(exclusive, ins.backport.length);
}

export function emulatorNames(ins: Inspection): string[] {
  return [...new Set(ins.emulators.map((e) => e.name))];
}

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
            <span className="backend">{l}</span>
          </li>
        );
      })}
    </ul>
  );
}

/** `block` fails every target, `warn` may fail (`formats`: the targets it fails), `info` is only news. */
// ponytail: core reports findings as plain text, so this matches its wording (preflight.rs
// `input`, inspect.rs). Ceiling: core should tag each finding with a severity and formats.
export function classify(line: string): { kind: "block" | "warn" | "info"; formats?: Format[] } {
  if (/^(eboot\.bin is missing|sce_sys\/param\.json)/.test(line)) return { kind: "block" };
  // Core says "(refused by .exfat/.ffpkg)", but a .pkg build refuses them too (package.rs).
  // .ffpfs takes ASCII names only, and a .ffpfsc's inner image refuses them too.
  if (/^name not in NFC/.test(line))
    return { kind: "warn", formats: ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"] };
  // PFS holds ASCII names only; a .ffpfsc's default inner image (exfat in the UI, ffpkg in core) takes them.
  if (/^name not ASCII/.test(line)) return { kind: "warn", formats: ["ffpfs"] };
  if (/^extraction:/.test(line)) return { kind: "warn", formats: ["folder", "lz4"] };
  if (/^(already 512-byte sectors|geometry differs from the SMP fast path)/.test(line))
    return { kind: "info" };
  return { kind: "warn" };
}

function DlcList({ dlcs }: { dlcs: Inspection["dlcs"] }) {
  const t = useMessages().common;
  if (dlcs.length === 0) return null;
  const notes = dlcs.map(dlcNote);
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

function dlcNote(d: Inspection["dlcs"][number]): string {
  const t = msg().common;
  if (d.folder) return t.dlcIn(d.folder, prettyBytes(d.bytes));
  if (d.emulated === "NO_EXTRA_DATA") return t.dlcNoData;
  if (d.emulated !== null) return t.dlcStatus(d.emulated);
  return t.dlcMerged;
}

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

/** `rest`: Edit's facts fold, without the ID tiles its card already shows. */
export function InspectSummary({
  ins,
  full = false,
  rest = false,
  ampr = true,
  findings = true,
}: {
  ins: Inspection;
  full?: boolean;
  rest?: boolean;
  ampr?: boolean;
  findings?: boolean;
}) {
  const t = useMessages().common;
  return (
    <div className="summary">
      <Facts ins={ins} full={full} rest={rest} ampr={ampr} />
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
      {!rest && findings && <Findings lines={ins.findings} />}
    </div>
  );
}

/** Focus goes back to the button before the choice runs; the menu opens where its scrolling container won't clip it. */
export function ChooseMenu<K extends string>(props: {
  id?: string;
  label: ReactNode;
  menuLabel: string;
  /** `disabled`: shown and focusable, but not chosen (`aria-disabled`); `busy`: its work runs. */
  items: { id: K; icon: IconName; label: ReactNode; disabled?: boolean; busy?: boolean }[];
  onChoose: (what: K) => void;
  className?: string;
  buttonLabel?: string;
  tabIndex?: number;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [flip, setFlip] = useState("");
  const box = useRef<HTMLSpanElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const m = menu.current;
    if (!open || !m) return setFlip("");
    let bound = { top: 0, bottom: window.innerHeight, left: 0, right: window.innerWidth };
    for (let el = m.parentElement; el && el !== document.body; el = el.parentElement) {
      const st = getComputedStyle(el);
      if (st.overflowY !== "visible" || st.overflowX !== "visible") {
        const r = el.getBoundingClientRect();
        bound = { top: Math.max(r.top, 0), bottom: Math.min(r.bottom, bound.bottom), left: Math.max(r.left, 0), right: Math.min(r.right, bound.right) };
        break;
      }
    }
    const r = m.getBoundingClientRect();
    const b = btn.current?.getBoundingClientRect();
    const up = r.bottom > bound.bottom && b !== undefined && b.top - bound.top > bound.bottom - b.bottom;
    const side = r.left < bound.left || r.right > bound.right;
    setFlip(`${up ? " up" : ""}${side ? " start" : ""}`);
  }, [open]);
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
    } else if ((e.key === "Home" || e.key === "End") && items.length > 0) {
      e.preventDefault();
      items[e.key === "Home" ? 0 : items.length - 1].focus();
    }
  };
  return (
    <span className="menu-wrap" ref={box} onKeyDown={onKey}>
      <button
        type="button"
        id={props.id}
        ref={btn}
        className={props.className ?? "small"}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={props.buttonLabel}
        title={props.buttonLabel}
        tabIndex={props.tabIndex}
        disabled={props.disabled}
        onClick={() => setOpen((o) => !o)}
      >
        {props.label}
        {props.buttonLabel === undefined && <Icon name="chevron" />}
      </button>
      {open && !props.disabled && (
        <span ref={menu} className={`menu${flip}`} role="menu" aria-label={props.menuLabel}>
          {props.items.map((it) => (
            <button
              key={it.id}
              type="button"
              role="menuitem"
              aria-disabled={it.disabled || undefined}
              aria-busy={it.busy || undefined}
              onClick={() => !it.disabled && choose(it.id)}
            >
              <Icon name={it.icon} />
              {it.label}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

/** http build: the page's url as a QR code drawn by the server (`GET /api/qr`); nothing in the app or when it doesn't load. */
export function AddressQr(props: { size: number; connect?: boolean; className?: string; caption?: ReactNode }) {
  const t = useMessages().common;
  const [failed, setFailed] = useState(false);
  const address = serverAddress();
  if (!address || failed) return null;
  return (
    <div className={props.className}>
      <img
        className="qr"
        src={props.connect ? "/api/qr?connect=1" : "/api/qr"}
        width={props.size}
        height={props.size}
        alt={t.qr(`http://${address}/${props.connect ? "#connect" : ""}`)}
        onError={() => setFailed(true)}
      />
      {props.caption && <p className="note">{props.caption}</p>}
    </div>
  );
}

/** A small pill radiogroup (Storage's view switch, Connect's filter): one tab stop, arrows, Home and End; each option `.view-<id>`. */
export function ViewSwitch<T extends string>(props: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: { id: T; icon?: IconName; label: ReactNode }[];
  className?: string;
  /** Off (aria-disabled, so it keeps focus): no click or key changes the choice. */
  disabled?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const ids = props.options.map((o) => o.id);
  const onKey = (e: KeyboardEvent) => {
    if (props.disabled) return;
    const i = ids.indexOf(props.value);
    const rtl = document.documentElement.dir === "rtl";
    const fwd = e.key === "ArrowDown" || e.key === (rtl ? "ArrowLeft" : "ArrowRight");
    const back = e.key === "ArrowUp" || e.key === (rtl ? "ArrowRight" : "ArrowLeft");
    const n = fwd ? (i + 1) % ids.length : back ? (i - 1 + ids.length) % ids.length : e.key === "Home" ? 0 : e.key === "End" ? ids.length - 1 : -1;
    if (n < 0) return;
    e.preventDefault();
    props.onChange(ids[n]);
    box.current?.querySelectorAll<HTMLElement>("[role=radio]")[n]?.focus();
  };
  return (
    <div className={`seg view-switch${props.className ? ` ${props.className}` : ""}`} role="radiogroup" aria-label={props.label} aria-disabled={props.disabled || undefined} ref={box} onKeyDown={onKey}>
      {props.options.map((o, i) => {
        const on = o.id === props.value;
        return (
          <button
            key={o.id}
            type="button"
            role="radio"
            aria-checked={on}
            aria-disabled={props.disabled || undefined}
            // With no option chosen (the chosen device went offline) the first is the Tab stop.
            tabIndex={on || (i === 0 && !ids.includes(props.value)) ? 0 : -1}
            className={`seg-opt view-${o.id}${on ? " on" : ""}`}
            onClick={() => !props.disabled && props.onChange(o.id)}
          >
            {o.icon && <Icon name={o.icon} />}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/** Decimal GB as the console's Settings shows it: whole numbers, one decimal under 10 GB. */
const gb = (bytes: number) => {
  // The precision follows the rounded value: 9.96 GB is "10 GB", not "10.0 GB".
  const tenths = Math.round(bytes / 1e8) / 10;
  return `${tenths < 10 ? num(tenths, 1) : num(Math.round(bytes / 1e9))} GB`;
};

export function FreeSpace({ storage }: { storage: { free: number; total: number } | null | undefined }) {
  const t = useMessages().library;
  if (!storage) return null;
  return (
    <span className="library-storage" title={t.storageTip}>
      <Icon name="drive" />
      <span>{t.free(gb(storage.free), gb(storage.total))}</span>
      <span className="visually-hidden">{t.storageTip}</span>
    </span>
  );
}
