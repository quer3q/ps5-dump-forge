// One device's pane of the Transfer screen: the File explorer's listing with a checkbox batch kept across folders.
// `data-transfer-pane`, `data-here` and `data-drop` let the screen find where a drop lands (`transferDropTarget`).

import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";

import { api, errorText, prettyBytes, web, type DirEntry, type DirListing } from "./api";
import { basename, Dots, joinPath, usePeerPath } from "./common";
import { Bookmarks, BookmarkToggle } from "./bookmarks";
import { MAX_ROWS } from "./Edit";
import { useMessages } from "./i18n";
import { Icon, type IconName } from "./icons";
import { readable } from "./peers";
import "./transfer.css";

interface Crumb {
  path: string | null;
  name: string;
  row?: string;
}

const collator = typeof Intl !== "undefined" ? new Intl.Collator(undefined, { numeric: true, sensitivity: "base" }) : null;
const byName = (a: DirEntry, b: DirEntry) =>
  a.dir !== b.dir ? (a.dir ? -1 : 1) : collator ? collator.compare(a.name, b.name) : a.name < b.name ? -1 : a.name > b.name ? 1 : 0;

/** Folders first; a link row keeps its own path (unique, never selected), its target as `target`. */
const rowsOf = (l: DirListing): DirListing => ({
  ...l,
  entries: l.entries
    .map((e) => (e.link && l.path !== null ? { ...e, path: joinPath(l.path, e.name), target: e.path } : e))
    .sort(byName),
});

export function dragPaths(entry: DirEntry, selected: string[]): string[] {
  return selected.includes(entry.path) ? selected : [entry.path];
}

/** Where a drop at (x, y) lands: the folder row under it, else the pane's folder; null over no pane or a root. */
export function transferDropTarget(x: number, y: number): { pane: string; path: string } | null {
  const el = document.elementFromPoint(x, y);
  const pane = el?.closest<HTMLElement>("[data-transfer-pane]");
  if (!el || !pane) return null;
  const row = el.closest<HTMLElement>("[data-drop][data-path]");
  const path = row && pane.contains(row) ? row.dataset.path : pane.dataset.here;
  return path ? { pane: pane.dataset.transferPane ?? "", path } : null;
}

/** A device's pane. `root` null: this device. `onHere`: the folder things land in, null at a device root
 * that is no folder (the PS5's `/`, a NAS host's shares; an `ftp://` root is one). `dropAt`: ringed. */
export function TransferPane(props: {
  id: string;
  label: string;
  icon: IconName;
  root: string | null;
  selected: string[];
  onSelected: (paths: string[], entry?: DirEntry) => void;
  onHere: (path: string | null) => void;
  visible: boolean;
  refreshKey?: number;
  onRowPointerDown?: (ev: ReactPointerEvent<HTMLElement>, entry: DirEntry) => void;
  dropAt?: string | null;
}) {
  const t = useMessages().transfer;
  const peerPath = usePeerPath();
  const [trail, setTrail] = useState<Crumb[]>([]);
  const [listed, setListed] = useState<{ key: string; n: number; listing: DirListing } | null>(null);
  const [listError, setListError] = useState<{ key: string; text: string } | null>(null);
  const [filter, setFilter] = useState("");
  const [focus, setFocus] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const seq = useRef(0);
  const want = useRef<{ path: string; after: number } | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    let gone = false;
    const begin = (c: Crumb) => {
      if (gone) return;
      setTrail([c]);
      setFilter("");
      setFocus(0);
    };
    if (props.root !== null) begin({ path: props.root, name: props.label });
    else if (web) begin({ path: null, name: "" });
    else
      api.homeFolder().then(
        (h) => begin(h ? { path: h, name: basename(h) || h } : { path: null, name: "" }),
        () => begin({ path: null, name: "" }),
      );
    return () => void (gone = true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.root]);

  const key = trail.map((c) => c.path ?? "").join("\0");
  const here = trail.length > 0 ? trail[trail.length - 1] : null;

  const load = async () => {
    if (!here) return;
    const mine = ++seq.current;
    const at = key;
    try {
      const listing = rowsOf(await api.listDir(here.path));
      if (mine !== seq.current) return;
      setListed({ key: at, n: mine, listing });
      setListError(null);
    } catch (e) {
      if (mine !== seq.current) return;
      setListError({ key: at, text: errorText(e) });
    }
  };
  useEffect(() => {
    if (props.visible && here) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.visible, key, props.refreshKey]);

  const error = listError && listError.key === key ? listError.text : null;
  const listing = !error && listed && listed.key === key ? listed.listing : null;
  const pending = !error && !listing;
  // Not the server's roots, nor an SMB/NFS root listing shares or exports (a peer's or FTP server's root is a folder).
  const atDeviceRoot = props.root !== null && listing !== null && listing.parent === null && !/^(ftp|flink):\/\//i.test(props.root);
  const hereFolder = listing && listing.path !== null && !atDeviceRoot ? listing.path : null;
  const onHere = useRef(props.onHere);
  onHere.current = props.onHere;
  useEffect(() => onHere.current(hereFolder), [hereFolder]);

  const query = filter.trim().toLowerCase();
  const all = listing?.entries ?? [];
  const entries = query ? all.filter((e) => e.name.toLowerCase().includes(query)) : all;
  const shown = entries.slice(0, MAX_ROWS);
  const atState = Math.min(focus, Math.max(shown.length - 1, 0));
  const chosen = new Set(props.selected);
  const selectable = (e: DirEntry) => hereFolder !== null && !e.link && !e.mounted;
  const toggle = (e: DirEntry) => {
    if (!selectable(e)) return;
    props.onSelected(chosen.has(e.path) ? props.selected.filter((p) => p !== e.path) : [...props.selected, e.path], e);
  };

  useEffect(() => {
    const w = want.current;
    if (!w || !listed || listed.key !== key || listed.n < w.after) return;
    want.current = null;
    const i = Math.max(shown.findIndex((e) => e.path === w.path), 0);
    setFocus(i);
    grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]")[i]?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listed, key]);

  const go = (next: Crumb[], focusPath?: string) => {
    setTrail(next);
    setFilter("");
    setFocus(0);
    if (focusPath !== undefined) want.current = { path: focusPath, after: seq.current + 1 };
  };
  const open = (e: DirEntry) => e.dir && !e.mounted && go([...trail, { path: e.target ?? e.path, name: e.name, row: e.path }], "");
  /** A bookmark: on another device the crumbs from its root down; here the folder alone (Up goes by `parent`). */
  const openPath = (path: string) => {
    if (path === here?.path) return;
    const root = props.root;
    if (root !== null) {
      const parts = path.slice(root.length).split("/").filter(Boolean);
      const chain: Crumb[] = [{ path: root, name: props.label }];
      for (const p of parts) chain.push({ path: `${chain[chain.length - 1].path}/${p}`, name: p });
      go(chain, "");
    } else {
      const c = { path, name: basename(path) || path };
      go(web ? [{ path: null, name: "" }, c] : [c], "");
    }
  };
  const parent = listing?.parent ?? null;
  const canUp = trail.length > 1 || parent !== null;
  const up = () => {
    if (trail.length > 1) go(trail.slice(0, -1), trail[trail.length - 1].row ?? trail[trail.length - 1].path ?? "");
    else if (parent !== null) go([{ path: parent, name: basename(parent) || parent }], here?.path ?? "");
  };
  const refresh = () => {
    if (refreshing) return;
    setRefreshing(true);
    void load().finally(() => setRefreshing(false));
  };
  const crumbName = (c: Crumb, i: number) => (c.path === null ? t.roots : i === 0 && props.root !== null ? props.label : c.name);

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.defaultPrevented) return;
    if ((e.target as HTMLElement).tagName === "BUTTON" && (e.key === "Enter" || e.key === " ")) return;
    const from = (e.target as HTMLElement).closest?.("[role=row][data-path]") as HTMLElement | null;
    const fromAt = from ? shown.findIndex((x) => x.path === from.dataset.path) : -1;
    const at = fromAt >= 0 ? fromAt : atState;
    const row = shown[at];
    const moveTo = (i: number) => {
      e.preventDefault();
      if (shown.length === 0) return;
      const n = Math.min(Math.max(i, 0), shown.length - 1);
      setFocus(n);
      grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]")[n]?.focus();
    };
    if (e.key === "ArrowDown") moveTo(at + 1);
    else if (e.key === "ArrowUp" && !e.altKey) moveTo(at - 1);
    else if (e.key === "Home") moveTo(0);
    else if (e.key === "End") moveTo(shown.length - 1);
    else if (e.key === "PageDown") moveTo(at + 10);
    else if (e.key === "PageUp") moveTo(at - 10);
    else if (e.key === " " && row) {
      e.preventDefault();
      toggle(row);
    } else if (e.key === "Enter" && row) {
      e.preventDefault();
      open(row);
    } else if (e.key === "Backspace" || (e.key === "ArrowUp" && e.altKey)) {
      e.preventDefault();
      up();
    }
  };
  // A folder's name opens it, elsewhere the row toggles; not after a drag, nor on the checkbox.
  const onRowClick = (ev: MouseEvent<HTMLElement>, e: DirEntry) => {
    const p = press.current;
    press.current = null;
    if (p && Math.hypot(ev.clientX - p.x, ev.clientY - p.y) >= 6) return;
    if ((ev.target as HTMLElement).closest("input, button")) return;
    if (e.dir && (ev.target as HTMLElement).closest(".xlabel")) open(e);
    else if (!e.dir) toggle(e);
  };

  const dirs = all.filter((e) => e.dir).length;
  const count = !listing
    ? ""
    : entries.length > MAX_ROWS
      ? t.firstOf(MAX_ROWS, entries.length)
      : query
        ? t.matches(entries.length)
        : t.inFolder(dirs, all.length - dirs);
  const dropAt = props.dropAt ?? null;
  const wellDrop = dropAt !== null && dropAt === hereFolder;

  return (
    <section className="explorer transfer-pane" aria-label={t.pane(props.label)} data-transfer-pane={props.id} data-here={hereFolder ?? undefined}>
      <div className="row nowrap transfer-head">
        <Icon name={props.icon} />
        <span className="transfer-device grow" dir="auto">
          {props.label}
        </span>
        <button type="button" className="small" onClick={refresh} aria-disabled={refreshing || undefined} aria-busy={refreshing || undefined}>
          <Icon name="refresh" />
          {t.refresh}
          {refreshing && <Dots />}
        </button>
      </div>
      <div className="row nowrap explorer-nav">
        <button type="button" className="small" onClick={up} disabled={!canUp} title={t.upTip}>
          <Icon name="up" />
          {t.up}
        </button>
        <nav className="crumbs grow" aria-label={t.location}>
          {trail.map((c, i) => {
            const last = i === trail.length - 1;
            return (
              <span key={`${i}:${c.path ?? ""}`} className="crumb">
                {i > 0 && (
                  <span className="muted" aria-hidden="true">
                    /
                  </span>
                )}
                {last ? (
                  <span className="here" aria-current="location">
                    {crumbName(c, i)}
                  </span>
                ) : (
                  <button type="button" className="link" title={c.path === null ? undefined : peerPath(c.path)} onClick={() => go(trail.slice(0, i + 1), trail[i + 1].row ?? trail[i + 1].path ?? "")}>
                    {crumbName(c, i)}
                  </button>
                )}
              </span>
            );
          })}
        </nav>
        <BookmarkToggle path={here?.path ?? null} root={props.root} />
      </div>
      <Bookmarks root={props.root} rootName={props.label} here={here?.path ?? null} onOpen={openPath} />
      {props.selected.length > 0 && (
        <div className="row nowrap transfer-selected" role="status">
          <Icon name="check" />
          <span className="grow">{t.selected(props.selected.length)}</span>
          <button type="button" className="small" title={t.clearTip} onClick={() => props.onSelected([])}>
            {t.clear}
          </button>
        </div>
      )}
      {listing?.truncated && (
        <p className="note-line warn explorer-note">
          <Icon name="warn" />
          <span>{t.truncated}</span>
        </p>
      )}
      {listing && hereFolder === null && listing.entries.length > 0 && (
        <p className="note-line explorer-note">
          <Icon name="info" />
          <span>{t.notHere}</span>
        </p>
      )}
      <div className="row nowrap filter">
        <input
          type="search"
          className="grow"
          placeholder={t.filter}
          aria-label={t.filterLabel}
          dir="auto"
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setFocus(0);
          }}
          spellCheck={false}
        />
        <span className="muted count-text" role="status">
          {dropAt !== null ? t.dropHere(readable(dropAt)) : count}
        </span>
      </div>
      <div className={`explorer-well${wellDrop ? " drop" : ""}`}>
        {/* Mounted while a folder lists too, so focus can rest on it. */}
        <div
          role="grid"
          ref={grid}
          className="xgrid transfer-grid"
          aria-label={here ? crumbName(here, trail.length - 1) : t.pane(props.label)}
          aria-multiselectable="true"
          aria-rowcount={shown.length + 1}
          aria-busy={pending || undefined}
          tabIndex={shown.length === 0 ? 0 : undefined}
          onKeyDown={onKey}
        >
          <div role="row" className="xrow head" aria-rowindex={1}>
            <span role="columnheader" className="xcheck" />
            <span role="columnheader" className="xname">
              {t.name}
            </span>
            <span role="columnheader" className="xsize">
              {t.size}
            </span>
          </div>
          {!error &&
            shown.map((e, i) => {
              const mine = i === atState ? 0 : -1;
              const can = selectable(e);
              const on = can && chosen.has(e.path);
              const target = e.dir && !e.link && !e.mounted;
              return (
                <div
                  key={e.name}
                  role="row"
                  data-path={e.path}
                  data-drop={target && hereFolder !== null ? "1" : undefined}
                  aria-rowindex={i + 2}
                  aria-selected={can ? on : undefined}
                  tabIndex={mine}
                  className={`xrow${e.dir ? " dir" : ""}${on ? " picked" : ""}${dropAt === e.path ? " drop" : ""}`}
                  onPointerDown={(ev) => {
                    press.current = { x: ev.clientX, y: ev.clientY };
                    if (can) props.onRowPointerDown?.(ev, e);
                  }}
                  onClick={(ev) => onRowClick(ev, e)}
                  onFocus={(ev) => ev.target === ev.currentTarget && setFocus(i)}
                  onDoubleClick={() => open(e)}
                >
                  <span role="gridcell" className="xcheck">
                    {can && <input type="checkbox" tabIndex={-1} checked={on} aria-label={t.select(e.name)} onChange={() => toggle(e)} />}
                  </span>
                  <span role="gridcell" className="xname">
                    <Icon name={e.dir ? "folder" : "file"} />
                    <span className={`xlabel${e.dir && !e.mounted ? " opens" : ""}`} title={e.dir ? t.open(e.name) : undefined} dir="ltr">
                      {e.name}
                    </span>
                    {e.link && (
                      <>
                        <span className="tag xmark" title={t.linkTip}>
                          {t.link}
                        </span>
                        <span className="visually-hidden">{t.linkTip}</span>
                      </>
                    )}
                    {e.mounted && (
                      <>
                        <span className="tag xmark" title={t.mountedTip}>
                          {t.mounted}
                        </span>
                        <span className="visually-hidden">{t.mountedTip}</span>
                      </>
                    )}
                  </span>
                  <span role="gridcell" className="xsize">
                    {e.size !== null && !e.dir ? prettyBytes(e.size) : ""}
                  </span>
                </div>
              );
            })}
        </div>
        {pending && here && (
          <div className="library-busy" role="status" aria-live="polite">
            <span className="spinner" aria-hidden="true" />
            <span>
              {t.loading}
              <Dots />
            </span>
          </div>
        )}
        {error ? (
          <p className="note-line block empty-note" role="alert">
            <Icon name="alert" />
            <span>
              {t.cantList} <span className="backend">{readable(error)}</span>
            </span>
          </p>
        ) : (
          listing &&
          shown.length === 0 && <p className="muted empty-note">{query ? t.noMatch(filter.trim()) : here?.path === null ? t.noRoots : t.emptyFolder}</p>
        )}
      </div>
    </section>
  );
}
