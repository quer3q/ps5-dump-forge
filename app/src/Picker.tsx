// http build: the folder and file browser over `POST /api/list_dir` that stands in for the
// native dialog. A modal in its own React root; the app behind it is inert. Three modes: a
// folder, a file, or (`folderToo`) either: a file click picks it, "Choose this folder" the
// folder shown. `pickFiles`: file clicks select (in one folder), "Choose N files" takes them.

import { Fragment, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { createRoot } from "react-dom/client";

import { errorText, prettyBytes } from "./api";
import { call } from "./http-client";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import type { PickOptions } from "./transport";

interface Entry {
  name: string;
  path: string;
  dir: boolean;
  /** null for a folder. */
  size: number | null;
}

interface Listing {
  /** null: the list of roots (drives). */
  path: string | null;
  /** null at a root. */
  parent: string | null;
  entries: Entry[];
  /** The server lists at most 10,000 entries. */
  truncated?: boolean;
}

/** Where the last browse ended, for the next one (kept until the page closes). */
let lastDir: string | null = null;

export function pick(o: PickOptions): Promise<string | null> {
  return open(o, false).then((p) => p?.[0] ?? null);
}

/** Several files of one folder (the folder shown when choosing). */
export function pickFiles(o: PickOptions): Promise<string[] | null> {
  return open(o, true);
}

function open(o: PickOptions, multiple: boolean): Promise<string[] | null> {
  return new Promise((resolve) => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const app = document.getElementById("root");
    app?.setAttribute("inert", "");
    app?.setAttribute("aria-hidden", "true");
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    let settled = false;
    const done = (path: string[] | null) => {
      if (settled) return;
      settled = true;
      app?.removeAttribute("inert");
      app?.removeAttribute("aria-hidden");
      // Unmount after the click that closed it has finished rendering.
      setTimeout(() => {
        root.unmount();
        host.remove();
        opener?.focus();
      }, 0);
      resolve(path);
    };
    root.render(<Picker o={o} multiple={multiple} done={done} />);
  });
}

function shows(o: PickOptions, e: Entry): boolean {
  if (e.dir) return true;
  if (o.directory) return false;
  const dot = e.name.lastIndexOf(".");
  const ext = dot > 0 ? e.name.slice(dot + 1).toLowerCase() : "";
  return o.extensions ? o.extensions.includes(ext) : true;
}

const IMAGES = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];
/** Game archives and the first parts of split ones (common.tsx's ARCHIVE_EXTENSIONS). */
const ARCHIVES = ["7z", "rar", "001", "z01"];
const ext = (name: string) => name.slice(name.lastIndexOf(".") + 1).toLowerCase();

/** A file row's icon: a disc for an image, the zipped file for an archive (and a plain .zip
 * where the picker lists only those: LZ4 traces). */
function fileIcon(o: PickOptions, name: string): IconName {
  const e = ext(name);
  if (IMAGES.includes(e)) return "disc";
  const sources = (o.extensions ?? []).some((x) => IMAGES.includes(x));
  return ARCHIVES.includes(e) || (sources && e === "zip") ? "archive" : "file";
}

/** A file mode's empty listing names what it lists: PS5 images and archives, or ".zip files". */
function noFiles(t: Messages["picker"], o: PickOptions): string {
  const ext = o.extensions;
  if (!ext) return t.nothing;
  return ext.some((e) => IMAGES.includes(e)) ? t.noImages : t.noFiles(ext);
}

/** "Drives", then the root holding `path`, then each folder below it. */
function crumbs(drives: string, path: string | null, roots: string[]): { label: string; path: string | null }[] {
  const out: { label: string; path: string | null }[] = [{ label: drives, path: null }];
  if (path === null) return out;
  const root = roots
    .filter((r) => path === r || path.startsWith(r.endsWith("/") ? r : `${r}/`))
    .sort((a, b) => b.length - a.length)[0];
  if (!root) return [...out, { label: path, path }];
  out.push({ label: root, path: root });
  let at = root;
  for (const part of path.slice(root.length).split("/").filter(Boolean)) {
    at = at.endsWith("/") ? at + part : `${at}/${part}`;
    out.push({ label: part, path: at });
  }
  return out;
}

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])';

function Picker({ o, multiple, done: give }: { o: PickOptions; multiple: boolean; done: (paths: string[] | null) => void }) {
  const t = useMessages().picker;
  const done = (path: string | null) => give(path === null ? null : [path]);
  /** `multiple`: the files selected in the folder shown (a new folder starts with none). */
  const [chosen, setChosen] = useState<string[]>([]);
  const toggle = (path: string) => setChosen((c) => (c.includes(path) ? c.filter((p) => p !== path) : [...c, path]));
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [roots, setRoots] = useState<string[]>([]);
  // A listing is on its way: what shows is still the previous folder's, so it can't be chosen.
  const [pending, setPending] = useState(true);
  const seq = useRef(0);
  const box = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);

  // Focus moves in at once, so keys (Esc, the trap) reach the dialog while the first listing
  // loads or after it fails; old WebKit has no `inert` to keep them from the app behind.
  useEffect(() => cancel.current?.focus(), []);

  const go = async (path: string | null, fallBack = false) => {
    const mine = ++seq.current;
    setError(null);
    setPending(true);
    setChosen([]);
    try {
      const l = await call<Listing>("list_dir", { path });
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
    const start = o.start ?? lastDir;
    if (start !== null) {
      // The crumbs need the roots too.
      call<Listing>("list_dir", { path: null }).then(
        (l) => setRoots(l.entries.map((e) => e.path)),
        () => {},
      );
      void go(start, true);
    } else void go(null);
  }, []);


  // A new listing: focus its first entry (else Up, else the dialog's buttons).
  useEffect(() => {
    if (!listing) return;
    const rows = list.current?.querySelectorAll<HTMLElement>(".pick-row");
    const first = list.current?.querySelector<HTMLElement>(".pick-row.entry") ?? rows?.[0];
    (first ?? box.current?.querySelector<HTMLElement>(FOCUSABLE))?.focus();
    if (list.current) list.current.scrollTop = 0;
  }, [listing]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      done(null);
    } else if (e.key === "Tab" && box.current) {
      // Focus stays in the dialog (old WebKit has no `inert`).
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
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && list.current) {
      // Arrow keys walk the rows.
      const rows = [...list.current.querySelectorAll<HTMLElement>(".pick-row")];
      const i = rows.indexOf(document.activeElement as HTMLElement);
      if (i < 0) return;
      e.preventDefault();
      rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
    }
  };

  const here = listing?.path ?? null;
  const entries = listing ? listing.entries.filter((e) => shows(o, e)) : [];
  const trail = crumbs(t.drives, here, roots);

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) done(null);
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
            <Icon name={o.directory || o.folderToo ? "folder" : "archive"} />
          </span>
          <h2 id="pick-title">{o.title}</h2>
        </div>
        <nav className="crumbs" aria-label={t.location}>
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
        <ul className="rows pick-list" ref={list} aria-label={here ?? t.drives} aria-busy={!listing}>
          {here !== null && (
            <li>
              <button type="button" className="pick-row" onClick={() => go(listing?.parent ?? null)}>
                <Icon name="up" />
                <span className="pick-name">{listing?.parent ? t.parent : t.allDrives}</span>
              </button>
            </li>
          )}
          {entries.map((e) => (
            <li key={e.name}>
              <button
                type="button"
                className={chosen.includes(e.path) ? "pick-row entry chosen" : "pick-row entry"}
                aria-pressed={multiple && !e.dir ? chosen.includes(e.path) : undefined}
                onClick={() => (e.dir ? go(e.path) : multiple ? toggle(e.path) : done(e.path))}
              >
                <Icon name={e.dir ? "folder" : chosen.includes(e.path) ? "check" : fileIcon(o, e.name)} />
                <span className={here === null ? "pick-name mono" : "pick-name"}>
                  {here === null ? e.path : e.name}
                </span>
                {e.size !== null && <span className="muted">{prettyBytes(e.size)}</span>}
              </button>
            </li>
          ))}
          {listing?.truncated && <li className="muted">{t.truncated(10000)}</li>}
          {!listing && !error && (
            <li className="muted">
              {t.loading}
              {/* common's Dots, inline: this file stays free of common's imports. */}
              <span className="dots" aria-hidden="true">
                <span>.</span>
                <span>.</span>
                <span>.</span>
              </span>
            </li>
          )}
          {listing && entries.length === 0 && (
            <li className="muted">
              {here === null ? t.noDrives : o.directory ? t.noFolders : noFiles(t, o)}
            </li>
          )}
        </ul>
        {error && (
          <p className="bad pick-error" role="alert">
            {error}
          </p>
        )}
        <div className="row end pick-foot">
          <button type="button" ref={cancel} onClick={() => done(null)}>
            {t.cancel}
          </button>
          {multiple && (
            <button type="button" className="primary" disabled={chosen.length === 0 || pending} onClick={() => give(chosen)}>
              {t.chooseFiles(chosen.length)}
            </button>
          )}
          {(o.directory || o.folderToo) && (
            <button
              type="button"
              className="primary"
              disabled={here === null || pending || error !== null}
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
