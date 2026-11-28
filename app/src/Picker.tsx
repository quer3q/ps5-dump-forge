// Every picker (no system dialog on any platform): a modal over `list_dir`, the app behind it inert.
// A folder, a file or (`folderToo`) either; `pickFiles` several of one folder; `devices` adds a tab per online device.

import { Fragment, useEffect, useRef, useState, type KeyboardEvent } from "react";

import { errorText, prettyBytes, runtimePlatform } from "./api";
import { NAS_PORT, type NasProtocol } from "./peers";
import { Dots, FOCUSABLE, openOverlay, pairPromptUp, panelId, tabId, TabList } from "./common";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import { Bookmarks, BookmarkToggle } from "./bookmarks";
import { joinPath, SEPARATORS } from "./paths";
import { shownRoots } from "./prefs";
import type { PickOptions } from "./transport";

interface Entry {
  name: string;
  path: string;
  dir: boolean;
  size: number | null;
  nas?: boolean;
}

export interface Listing {
  path: string | null;
  parent: string | null;
  entries: Entry[];
  truncated?: boolean;
}

/** Where the last browse ended, for the next one (kept until the page closes). */
let lastDir: string | null = null;

export interface SavedDevices {
  devices: { key_id: string; name: string; online: boolean }[];
  nas: { name: string; host: string; port: number; protocol?: NasProtocol; online: boolean }[];
}

/** `list_dir` (`null`: its roots) and Connect's saved devices, offered as roots or, in tabs, the online ones. */
export interface PickerBackend {
  list(path: string | null): Promise<Listing>;
  saved(): Promise<SavedDevices>;
}

interface Tab {
  label: string;
  root: string | null;
  nas?: boolean;
}

const isNet = (p: string) => /^(smb|ftp|nfs|flink):\/\//i.test(p);
const isNas = (p: string) => /^(smb|ftp|nfs):\/\//i.test(p);
/** Whether the picker offers saved NAS devices: a source picker, and any output picker but Edit's Add (`peersOnly`). */
const offered = (o: PickOptions) => !!o.nas || (!!o.devices && !o.peersOnly);
const within = (p: string, root: string) => p === root || p.startsWith(`${root}/`);

async function tabsOf(o: PickOptions, b: PickerBackend): Promise<Tab[]> {
  if (!o.devices) return [];
  const s = await b.saved().catch(() => null);
  if (!s) return [];
  const others: Tab[] = [
    ...s.devices.filter((d) => d.online).map((d) => ({ label: d.name, root: `flink://${d.key_id}` })),
    ...s.nas.filter((n) => n.online && offered(o)).map((n) => ({ label: n.name, root: nasRoot(n.host, n.port, n.protocol), nas: true })),
  ];
  return others.length === 0 ? [] : [{ label: "", root: null }, ...others];
}

/** A saved NAS's root as core spells it (port only when not the protocol's; IPv6 in brackets; host lowercased). */
export function nasRoot(host: string, port: number, protocol: NasProtocol = "smb"): string {
  const lower = host.replace(/[A-Z]/g, (c) => c.toLowerCase());
  const h = lower.includes(":") ? `[${lower}]` : lower;
  return `${protocol}://${h}${port === NAS_PORT[protocol] ? "" : `:${port}`}`;
}

export function picker(b: PickerBackend) {
  return {
    pick: (o: PickOptions): Promise<string | null> => open(o, false, b).then((p) => p?.[0] ?? null),
    pickFiles: (o: PickOptions): Promise<string[] | null> => open(o, true, b),
  };
}

/** A picker is looking up the devices: the page isn't inert yet, so a second click opens nothing. */
let opening = false;

async function open(o: PickOptions, multiple: boolean, b: PickerBackend): Promise<string[] | null> {
  if (opening) return null;
  opening = true;
  const tabs = await tabsOf(o, b).finally(() => (opening = false));
  // common's overlay: the app stays inert until the last one closes; `closeOverlays` gives null.
  return new Promise((resolve) => {
    let picked: string[] | null = null;
    openOverlay(
      (close) => (
        <Picker
          o={o}
          multiple={multiple}
          b={b}
          tabs={tabs}
          done={(path) => {
            picked = path;
            close();
          }}
        />
      ),
      () => resolve(picked),
    );
  });
}

function shows(o: PickOptions, e: Entry): boolean {
  if (e.dir) return true;
  if (o.directory) return false;
  const dot = e.name.lastIndexOf(".");
  const ext = dot > 0 ? e.name.slice(dot + 1).toLowerCase() : "";
  return o.extensions ? o.extensions.includes(ext) : true;
}

/** The rows shown at most (the explorers' limit; old WebKit mounts every button). */
const MAX_ROWS = 2000;
const UP = "\u0000up";

const IMAGES = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];
/** common.tsx's ARCHIVE_EXTENSIONS. */
const ARCHIVES = ["7z", "rar", "001", "z01"];
const ext = (name: string) => name.slice(name.lastIndexOf(".") + 1).toLowerCase();

/** A file row's icon: a disc for an image, the zipped file for an archive (or a .zip where only those are listed). */
function fileIcon(o: PickOptions, name: string): IconName {
  const e = ext(name);
  if (IMAGES.includes(e)) return "disc";
  const sources = (o.extensions ?? []).some((x) => IMAGES.includes(x));
  return ARCHIVES.includes(e) || (sources && e === "zip") ? "archive" : "file";
}

function noFiles(t: Messages["picker"], o: PickOptions): string {
  const ext = o.extensions;
  if (!ext) return t.nothing;
  return ext.some((e) => IMAGES.includes(e)) ? t.noImages : t.noFiles(ext);
}

function crumbs(drives: string, path: string | null, roots: string[]): { label: string; path: string | null }[] {
  const out: { label: string; path: string | null }[] = [{ label: drives, path: null }];
  if (path === null) return out;
  // Another device's paths take `/`; this device's its own separators (Windows: `\` too).
  const net = isNet(path);
  const sep = net ? /\// : SEPARATORS;
  const root = roots
    .filter((r) => path.startsWith(r) && (path.length === r.length || sep.test(r.slice(-1)) || sep.test(path[r.length])))
    .sort((a, b) => b.length - a.length)[0];
  if (!root) return [...out, { label: path, path }];
  out.push({ label: root, path: root });
  let at = root;
  for (const part of path.slice(root.length).split(sep).filter(Boolean)) {
    at = net ? (at.endsWith("/") ? at + part : `${at}/${part}`) : joinPath(at, part);
    out.push({ label: part, path: at });
  }
  return out;
}

/** The backend's roots, then each saved NAS offered (not with tabs); a NAS list that fails adds none. */
async function roots(o: PickOptions, b: PickerBackend, tabbed: boolean): Promise<Listing> {
  const [base, nas] = await Promise.all([
    b.list(null).then(async (l) => shownRoots(l, await runtimePlatform().catch(() => "ps5"))),
    offered(o) && !tabbed ? b.saved().then((s) => s.nas, () => []) : Promise.resolve([]),
  ]);
  const shares = nas.map((n) => ({ name: n.name, path: nasRoot(n.host, n.port, n.protocol), dir: true, size: null, nas: true }));
  return { ...base, entries: [...base.entries, ...shares] };
}

function Picker({
  o,
  multiple,
  b,
  tabs,
  done: give,
}: {
  o: PickOptions;
  multiple: boolean;
  b: PickerBackend;
  tabs: Tab[];
  done: (paths: string[] | null) => void;
}) {
  const t = useMessages().picker;
  const [tab, setTab] = useState(0);
  const tabAt = useRef(0);
  const dev = tabs[tab] ?? { label: "", root: null };
  const done = (path: string | null) => give(path === null ? null : [path]);
  const [chosen, setChosen] = useState<string[]>([]);
  const toggle = (path: string) => setChosen((c) => (c.includes(path) ? c.filter((p) => p !== path) : [...c, path]));
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rootPaths, setRoots] = useState<string[]>([]);
  // A listing is on its way: what shows is still the previous folder's, so it can't be chosen.
  const [pending, setPending] = useState(true);
  const seq = useRef(0);
  const [cursor, setCursor] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);

  // Focus moves in at once (old WebKit has no `inert`), so keys reach the dialog while the first listing loads.
  useEffect(() => {
    if (!pairPromptUp()) cancel.current?.focus();
  }, []);

  const go = async (path: string | null, fallBack = false) => {
    const mine = ++seq.current;
    const at = path ?? tabs[tabAt.current]?.root ?? null;
    setError(null);
    setPending(true);
    setChosen([]);
    try {
      const l = at === null ? await roots(o, b, tabs.length > 0) : await b.list(at);
      if (mine !== seq.current) return;
      setPending(false);
      setListing(l);
      if (l.path === null) setRoots(l.entries.map((e) => e.path));
      lastDir = l.path;
    } catch (e) {
      if (mine !== seq.current) return;
      // The last folder may be gone (a drive unplugged): start from the drives instead.
      if (fallBack) void go(null);
      else {
        setPending(false);
        setError(errorText(e));
      }
    }
  };

  useEffect(() => {
    // Another device's folder is a start only on its tab, or, without tabs, a NAS folder for a picker listing NAS.
    let start = o.start ?? lastDir;
    if (o.local && start !== null && isNet(start)) start = null;
    const at = start === null ? -1 : tabs.findIndex((d) => d.root !== null && within(start!, d.root));
    if (at > 0) {
      tabAt.current = at;
      setTab(at);
      void go(start, true);
      return;
    }
    const keep =
      (o.start !== undefined && (o.devices || !/^flink:\/\//i.test(o.start))) ||
      (isNas(start ?? "") && offered(o));
    if (start !== null && isNet(start) && (tabs.length > 0 || !keep)) start = null;
    if (start !== null) {
      roots(o, b, tabs.length > 0).then(
        (l) => setRoots(l.entries.map((e) => e.path)),
        () => {},
      );
      void go(start, true);
    } else void go(null);
  }, []);


  useEffect(() => {
    if (!listing) return;
    if (list.current) list.current.scrollTop = 0;
    // Arrow keys on the device tabs keep focus there while the tab's listing loads.
    if (box.current?.querySelector('[role="tablist"]')?.contains(document.activeElement)) return;
    if (pairPromptUp()) return;
    const rows = list.current?.querySelectorAll<HTMLElement>(".pick-row");
    const first = list.current?.querySelector<HTMLElement>(".pick-row.entry") ?? rows?.[0];
    (first ?? box.current?.querySelector<HTMLElement>(FOCUSABLE))?.focus();
  }, [listing]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      done(null);
    } else if (e.key === "Tab" && box.current) {
      const all = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (all.length === 0) return;
      const i = all.indexOf(document.activeElement as HTMLElement);
      // A row out of the Tab order (not the list's stop) lets Shift+Tab go back as usual.
      if (e.shiftKey && (i === 0 || document.activeElement === box.current)) {
        e.preventDefault();
        all[all.length - 1].focus();
      } else if (!e.shiftKey && i === all.length - 1) {
        e.preventDefault();
        all[0].focus();
      }
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && list.current) {
      const rows = [...list.current.querySelectorAll<HTMLElement>(".pick-row")];
      const i = rows.indexOf(document.activeElement as HTMLElement);
      if (i < 0) return;
      e.preventDefault();
      rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
    }
  };

  const here = listing?.path ?? null;
  // On another device core reads only `nasExtensions`: the rest is listed greyed out, not pickable (issue #20).
  const files = here !== null && isNet(here) && o.nasExtensions ? { ...o, extensions: o.nasExtensions } : o;
  const off = (e: Entry) => files !== o && !shows(files, e);
  // Greyed rows after the rest, so they never push a pickable one past MAX_ROWS.
  const listed = listing ? listing.entries.filter((e) => shows(o, e)) : [];
  const entries = files === o ? listed : [...listed.filter((e) => !off(e)), ...listed.filter(off)];
  const atTop = here === null || here === dev.root;
  const rows = entries.slice(0, MAX_ROWS);
  const anyOff = rows.some(off);
  const stop = cursor !== null && (cursor === UP ? !atTop : rows.some((e) => e.path === cursor)) ? cursor : atTop ? rows[0]?.path : UP;
  const top = t.drives;
  const trail =
    dev.root === null
      ? crumbs(top, here, rootPaths)
      : [{ label: dev.label, path: dev.root }, ...crumbs(top, here, [dev.root]).slice(2)];
  const choose = (i: number) => {
    if (i === tabAt.current) return;
    tabAt.current = i;
    setTab(i);
    setListing(null);
    void go(null);
  };

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target !== e.currentTarget) return;
        e.preventDefault(); // focus stays where it was until the overlay gives it back
        done(null);
      }}
    >
      <div
        className="modal card picker"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pick-title"
        ref={box}
        // A click on the dialog's text focuses the box, so Escape and the trap still reach it.
        tabIndex={-1}
        onKeyDown={onKey}
      >
        <div className="modal-head">
          <span className="head-icon">
            <Icon name={o.directory || o.folderToo ? "folder" : o.extensions ? "archive" : "file"} />
          </span>
          <h2 id="pick-title">{o.title}</h2>
        </div>
        {tabs.length > 0 && (
          <TabList
            label={t.devices}
            prefix="pick"
            className="pick-tabs"
            tabs={tabs.map((d, i) => ({ id: String(i), label: d.root === null ? t.thisDevice : <span title={d.label}>{d.label}</span> }))}
            value={String(tab)}
            onChange={(id) => choose(Number(id))}
          />
        )}
        {/* One panel shows every tab's listing; the others' are empty, so each tab names one. */}
        {tabs.map(
          (_, i) =>
            i !== tab && (
              <div key={i} role="tabpanel" id={panelId("pick", String(i))} aria-labelledby={tabId("pick", String(i))} hidden />
            ),
        )}
        <div
          className="pick-panel"
          {...(tabs.length > 0 && {
            role: "tabpanel",
            id: panelId("pick", String(tab)),
            "aria-labelledby": tabId("pick", String(tab)),
          })}
        >
          <div className="row nowrap">
            <nav className="crumbs grow" aria-label={t.location}>
              {trail.map((c, i) => (
                <Fragment key={i}>
                  {i > 0 && (
                    <span className="muted" aria-hidden="true">
                      /
                    </span>
                  )}
                  {/* After a failed listing the last crumb stays a button: it tries again. */}
                  {i === trail.length - 1 && !error ? (
                    <span className="here" aria-current="location">
                      {c.label}
                    </span>
                  ) : (
                    <button type="button" className="link" onClick={() => go(c.path)}>
                      {c.label}
                    </button>
                  )}
                </Fragment>
              ))}
            </nav>
            {/* Pinned here too: with no other device the panes that also pin aren't shown (#76). */}
            {dev.root === null && !(here !== null && isNet(here)) && <BookmarkToggle path={here} root={null} />}
          </div>
          {/* This device's bookmarks (the Transfer screen's and File explorer's), to jump to a folder. */}
          {dev.root === null && <Bookmarks root={null} here={here} onOpen={(path) => path !== here && void go(path)} />}
          {anyOff && (
            <p className="note-line muted">
              <Icon name="info" />
              <span>{t.netOnlyHere}</span>
            </p>
          )}
          <ul className="rows pick-list" ref={list} aria-label={here ?? top} aria-busy={pending}>
            {!atTop && (
              <li>
                <button
                  type="button"
                  className="pick-row"
                  tabIndex={stop === UP ? 0 : -1}
                  onFocus={() => setCursor(UP)}
                  onClick={() => go(listing?.parent ?? null)}
                >
                  <Icon name="up" />
                  <span className="pick-name">
                    {listing?.parent || dev.root !== null ? t.parent : t.allDrives}
                  </span>
                </button>
              </li>
            )}
            {rows.map((e) => {
              const no = off(e);
              return (
                <li key={e.path}>
                  <button
                    type="button"
                    className={no ? "pick-row entry off" : chosen.includes(e.path) ? "pick-row entry chosen" : "pick-row entry"}
                    tabIndex={stop === e.path ? 0 : -1}
                    onFocus={() => setCursor(e.path)}
                    aria-pressed={multiple && !e.dir && !no ? chosen.includes(e.path) : undefined}
                    aria-disabled={no || undefined}
                    onClick={() => {
                      if (no) return;
                      if (e.dir) void go(e.path);
                      else if (multiple) toggle(e.path);
                      else done(e.path);
                    }}
                  >
                    <Icon name={e.nas ? "drive" : e.dir ? "folder" : chosen.includes(e.path) ? "check" : fileIcon(o, e.name)} />
                    <span className={here === null ? "pick-name mono" : "pick-name"}>
                      {here === null ? e.path : e.name}
                    </span>
                    {e.size !== null && <span className="muted">{prettyBytes(e.size)}</span>}
                    {e.nas && <span className="muted">{e.name}</span>}
                  </button>
                </li>
              );
            })}
            {(listing?.truncated || rows.length < entries.length) && <li className="muted">{t.truncated(rows.length < entries.length ? MAX_ROWS : 10000)}</li>}
            {!listing && !error && (
              <li className="muted">
                {t.loading}
                <Dots />
              </li>
            )}
            {listing && entries.length === 0 && (
              <li className="muted">
                {here === null ? t.noDrives : o.directory ? t.noFolders : noFiles(t, files)}
              </li>
            )}
          </ul>
          {error && (
            <p className="bad pick-error" role="alert">
              {t.cantList} <span className="backend">{error}</span>
            </p>
          )}
        </div>
        <div className="row end pick-foot">
          <button type="button" ref={cancel} onClick={() => done(null)}>
            {t.cancel}
          </button>
          {multiple && (
            <button type="button" className="primary" disabled={chosen.length === 0 || pending} onClick={() => give(chosen)}>
              {t.chooseFiles(chosen.length)}
            </button>
          )}
          {/* A NAS tab's root lists shares or exports (not a folder), but an FTP server's is its root folder. */}
          {(o.directory || o.folderToo) && (
            <button
              type="button"
              className="primary"
              disabled={here === null || (dev.nas && here === dev.root && !here.startsWith("ftp://")) || pending || error !== null}
              onClick={() => here !== null && done(here)}
            >
              {t.chooseFolder}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
