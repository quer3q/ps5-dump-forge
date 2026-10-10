// http build: the folder and file browser over `POST /api/list_dir` that stands in for the
// native dialog. A modal in its own React root; the app behind it is inert. Three modes: a
// folder, a file, or (`folderToo`) either: a file click picks it, "Choose this folder" the
// folder shown.

import { Fragment, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { createRoot } from "react-dom/client";

import { errorText, prettyBytes } from "./api";
import { call } from "./http-client";
import { Icon } from "./icons";
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
  return new Promise((resolve) => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const app = document.getElementById("root");
    app?.setAttribute("inert", "");
    app?.setAttribute("aria-hidden", "true");
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    let settled = false;
    const done = (path: string | null) => {
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
    root.render(<Picker o={o} done={done} />);
  });
}

function shows(o: PickOptions, e: Entry): boolean {
  if (e.dir) return true;
  if (o.directory) return false;
  const dot = e.name.lastIndexOf(".");
  const ext = dot > 0 ? e.name.slice(dot + 1).toLowerCase() : "";
  return (o.extensions ?? []).includes(ext);
}

const IMAGES = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];
function isImage(name: string): boolean {
  return IMAGES.includes(name.slice(name.lastIndexOf(".") + 1).toLowerCase());
}

/** The files a file mode lists, for its empty listing: "PS5 images", ".zip files". */
function fileKinds(o: PickOptions): string {
  const ext = o.extensions ?? [];
  if (ext.some((e) => IMAGES.includes(e))) return "PS5 images";
  return `${ext.map((e) => `.${e}`).join(" or ")} files`;
}

/** "Drives", then the root holding `path`, then each folder below it. */
function crumbs(path: string | null, roots: string[]): { label: string; path: string | null }[] {
  const out: { label: string; path: string | null }[] = [{ label: "Drives", path: null }];
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

function Picker({ o, done }: { o: PickOptions; done: (path: string | null) => void }) {
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [roots, setRoots] = useState<string[]>([]);
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
    try {
      const l = await call<Listing>("list_dir", { path });
      if (mine !== seq.current) return;
      setListing(l);
      if (l.path === null) setRoots(l.entries.map((e) => e.path));
      lastDir = l.path;
    } catch (e) {
      if (mine !== seq.current) return;
      // The last folder may be gone (a drive unplugged): start from the drives instead.
      if (fallBack) void go(null);
      else setError(errorText(e));
    }
  };

  useEffect(() => {
    if (lastDir !== null) {
      // The crumbs need the roots too.
      call<Listing>("list_dir", { path: null }).then(
        (l) => setRoots(l.entries.map((e) => e.path)),
        () => {},
      );
      void go(lastDir, true);
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
  const trail = crumbs(here, roots);

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
        onKeyDown={onKey}
      >
        <div className="modal-head">
          <span className="head-icon">
            <Icon name={o.directory || o.folderToo ? "folder" : "disc"} />
          </span>
          <h2 id="pick-title">{o.title}</h2>
        </div>
        <nav className="crumbs" aria-label="Location">
          {trail.map((c, i) => (
            <Fragment key={i}>
              {i > 0 && (
                <span className="muted" aria-hidden="true">
                  /
                </span>
              )}
              {i === trail.length - 1 ? (
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
        <ul className="rows pick-list" ref={list} aria-label={here ?? "Drives"} aria-busy={!listing}>
          {here !== null && (
            <li>
              <button type="button" className="pick-row" onClick={() => go(listing?.parent ?? null)}>
                <Icon name="up" />
                <span className="pick-name">{listing?.parent ? "Parent folder" : "All drives"}</span>
              </button>
            </li>
          )}
          {entries.map((e) => (
            <li key={e.name}>
              <button
                type="button"
                className="pick-row entry"
                onClick={() => (e.dir ? go(e.path) : done(e.path))}
              >
                <Icon name={e.dir ? "folder" : isImage(e.name) ? "disc" : "file"} />
                <span className={here === null ? "pick-name mono" : "pick-name"}>
                  {here === null ? e.path : e.name}
                </span>
                {e.size !== null && <span className="muted">{prettyBytes(e.size)}</span>}
              </button>
            </li>
          ))}
          {listing?.truncated && <li className="muted">Only the first 10,000 entries are shown.</li>}
          {!listing && !error && <li className="muted">Loading…</li>}
          {listing && entries.length === 0 && (
            <li className="muted">
              {here === null
                ? "No drives found."
                : o.directory
                  ? "No folders here."
                  : `No folders or ${fileKinds(o)} here.`}
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
            Cancel
          </button>
          {(o.directory || o.folderToo) && (
            <button
              type="button"
              className="primary"
              disabled={here === null}
              onClick={() => here !== null && done(here)}
            >
              Choose this folder
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
