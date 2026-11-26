// Storage's File explorer: Edit's explorer where every action happens at once, a tab per device.
// Links are opened, never changed; nothing copies from here (the Transfer screen does).

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { api, errorText, pick, prettyBytes, runtimePlatform, type DirEntry, type DirListing, type RuntimePlatform } from "./api";
import { Bookmarks, BookmarkToggle } from "./bookmarks";
import {
  ARCHIVE_EXTENSIONS,
  ChooseMenu,
  dirname,
  Dots,
  FreeSpace,
  IMAGE_EXTENSIONS,
  isMac,
  isWithin,
  joinPath,
  notifyDeleted,
  onDeleted,
  type SourceSelection,
} from "./common";
import { confirmDelete, type DeletedFile } from "./delete";
import { DeviceTabs, tabPanelProps, type DeviceTab } from "./DeviceTabs";
import { installHere } from "./InstallDialog";
import type { Action } from "./jobs";
import { readable } from "./peers";
import { confirmReplace } from "./replace";
import { MAX_ROWS, NameInput } from "./Edit";
import { FILES_TABS, LOCAL, useFilesDevices, useRowDrag, type FilesDevice } from "./FilesTabs";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import type { HandOff } from "./Storage";

interface Crumb {
  path: string | null;
  name: string;
  /** The row it was opened from (a link's own path; `path` is then its target), focused on the way back up. */
  row?: string;
}

const isNet = (p: string) => /^(smb|ftp|nfs|flink):\/\//i.test(p);
/** Another device's paths always take `/` (a Windows app's `joinPath` takes `\`). */
const join = (dir: string, name: string) => (isNet(dir) ? `${dir.replace(/\/+$/, "")}/${name}` : joinPath(dir, name));

const extOf = (name: string) => {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
};

function sourceKind(name: string): SourceSelection["kind"] | null {
  const ext = extOf(name);
  if (IMAGE_EXTENSIONS.includes(ext)) return ext as SourceSelection["kind"];
  if (!ARCHIVE_EXTENSIONS.includes(ext)) return null;
  if (ext === "001") return extOf(name.slice(0, -4)) === "zip" ? "zip" : "7z";
  return ext === "z01" ? "zip" : (ext as SourceSelection["kind"]);
}

function deletedFile(name: string): DeletedFile {
  const kind = sourceKind(name);
  return kind === null ? "file" : ["zip", "7z", "rar"].includes(kind) ? "archive" : "image";
}

/** The PS5's old WebKit has `addListener` only. */
function useWide(): boolean {
  const query = "(min-width: 880px)";
  const [wide, setWide] = useState(() => typeof matchMedia === "function" && matchMedia(query).matches);
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const m = matchMedia(query);
    const on = () => setWide(m.matches);
    on();
    if ("addEventListener" in m) {
      m.addEventListener("change", on);
      return () => m.removeEventListener("change", on);
    }
    (m as MediaQueryList).addListener(on);
    return () => (m as MediaQueryList).removeListener(on);
  }, []);
  return wide;
}

/** A link's `path` is its resolved target (maybe another row): the row keeps its own path, the target as `target`. */
const rowsOf = (l: DirListing): DirListing => ({
  ...l,
  entries: l.entries.map((e) =>
    e.link && l.path !== null ? { ...e, path: join(l.path, e.name), target: e.path } : e,
  ),
});

export function Files(props: {
  visible: boolean;
  onEdit: HandOff;
  onConvert: HandOff;
  dispatch: (a: Action) => void;
  switcher?: ReactNode;
  onConnect?: () => void;
}) {
  const t = useMessages().storage;
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const lookup = () =>
    runtimePlatform().then(
      (p) => {
        setLookupError(null);
        setPlatform(p);
      },
      (e) => setLookupError(errorText(e)),
    );
  useEffect(() => void lookup(), []);
  const { devices, loaded } = useFilesDevices();
  const ps5 = platform === "ps5";
  const all: (FilesDevice | null)[] = [...(ps5 ? [null] : []), ...devices];
  const idOf = (d: FilesDevice | null) => d?.id ?? LOCAL;
  const tabs: DeviceTab[] = all.map((d) =>
    d ? { id: d.id, name: d.name, icon: d.icon, online: d.online, tag: d.tag } : { id: LOCAL, name: t.thisDevice, icon: "disc", online: true },
  );
  const [tab, setTab] = useState<string | null>(null);
  const online = tabs.filter((d) => d.online);
  // The chosen tab while online, else the first online one, which then stays chosen.
  const shown = online.some((d) => d.id === tab) ? tab : (online[0]?.id ?? null);
  useEffect(() => {
    if (shown !== null && shown !== tab) setTab(shown);
  }, [shown, tab]);

  if (!platform)
    return lookupError ? (
      <section className="card explorer files storage-card bare" aria-label={t.title}>
        <div className="storage-head">
          <div className="storage-actions">
            <button
              type="button"
              className="small"
              onClick={() => !retrying && (setRetrying(true), void lookup().finally(() => setRetrying(false)))}
              aria-disabled={retrying || undefined}
              aria-busy={retrying || undefined}
            >
              <Icon name="refresh" />
              {t.refresh}
              {retrying && <Dots />}
            </button>
          </div>
        </div>
        <p className="note-line block" role="alert">
          <Icon name="alert" />
          <span>
            {t.cantList} <span className="backend">{lookupError}</span>
          </span>
        </p>
      </section>
    ) : null;

  const bar = tabs.length > 0 && <DeviceTabs label={t.devices} prefix={FILES_TABS} tabs={tabs} value={shown ?? ""} onChange={setTab} onAdd={props.onConnect} />;
  const none =
    shown === null &&
    (loaded ? (
      <section className={`card storage-card bare${bar ? " dev-window" : ""}`} aria-label={t.title}>
        <div className="empty library-empty">
          <span className="empty-icon">
            <Icon name="link" />
          </span>
          <p className="empty-title">{t.noDevice}</p>
          <p className="muted">{t.noDeviceHint}</p>
          {props.onConnect && (
            <div className="row center">
              <button type="button" className="primary" onClick={props.onConnect}>
                <Icon name="link" />
                {t.openConnect}
              </button>
            </div>
          )}
        </div>
      </section>
    ) : (
      <section className="card storage-card bare" aria-label={t.title}>
        <div className="library-busy" role="status" aria-live="polite">
          <span className="spinner" aria-hidden="true" />
          <span>
            {t.loadingDevices}
            <Dots />
          </span>
        </div>
      </section>
    ));
  return (
    <>
      {bar}
      {none}
      {all.map((d) => {
        const id = idOf(d);
        return (
          <Explorer
            key={id}
            onEdit={props.onEdit}
            onConvert={props.onConvert}
            dispatch={props.dispatch}
            platform={platform}
            visible={props.visible && id === shown}
            hidden={id !== shown}
            panel={tabPanelProps(FILES_TABS, id)}
            device={d ?? undefined}
            switcher={id === shown ? props.switcher : undefined}
          />
        );
      })}
    </>
  );
}

function Explorer(props: {
  visible: boolean;
  hidden: boolean;
  panel: ReturnType<typeof tabPanelProps>;
  platform: RuntimePlatform;
  onEdit: HandOff;
  onConvert: HandOff;
  dispatch: (a: Action) => void;
  switcher?: ReactNode;
  /** None: the PS5's own roots. */
  device?: FilesDevice;
}) {
  const { device, platform } = props;
  const t = useMessages().storage;
  const inst = useMessages().install;
  const tabs = useMessages().app;
  const build = useMessages().rail.build;
  const deleting = useMessages().library.deleting;
  const actionsHead = useMessages().edit.actions;
  const loading = useMessages().picker.loading;
  const root: string | null = device ? device.root : null;
  const [trail, setTrail] = useState<Crumb[]>(() => [{ path: root, name: "" }]);
  // `n`: the load that answered, so focus waits for the listing it was asked against.
  const [listed, setListed] = useState<{ key: string; n: number; listing: DirListing } | null>(null);
  const [listError, setListError] = useState<{ key: string; n: number; text: string } | null>(null);
  const [filter, setFilter] = useState("");
  const [focus, setFocus] = useState(0);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [actError, setActError] = useState<{ lead: string; text: string } | null>(null);
  const seq = useRef(0);
  // A spinner only past 150ms, so a fast listing doesn't flash it.
  const [slowKey, setSlowKey] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [storage, setStorage] = useState<{ free: number; total: number } | null>(null);
  const acting = useRef(false);
  // The row an action is changing (every row's actions locked until it ends).
  const [busy, setBusy] = useState<{ path: string; deleting: boolean } | null>(null);
  const want = useRef<{ path: string; after: number } | null>(null);
  const focusAfterLoad = (path: string) => (want.current = { path, after: seq.current + 1 });
  const grid = useRef<HTMLDivElement>(null);
  const treeNav = useRef<HTMLElement>(null);
  const wide = useWide();
  // Subfolders of each folder above the one shown; an older `gen` is listed again, shown until its answer (the tree keeps its shape).
  const [subdirs, setSubdirs] = useState<Map<string, { gen: number; dirs: DirEntry[] }>>(() => new Map());
  const treeGen = useRef(0);
  const treeBusy = useRef(false);
  const clearTree = () => {
    treeGen.current++;
    treeBusy.current = false;
    const on = new Set(trailRef.current.map((c) => c.path ?? ""));
    setSubdirs((m) => new Map([...m].filter(([k]) => on.has(k))));
  };

  const crumbName = (c: Crumb) => (c.path === null ? t.roots : device && c.path === device.root ? device.name : c.name);

  const key = trail.map((c) => c.path ?? "").join("\0");
  const here = trail.length > 0 ? trail[trail.length - 1] : null;

  const load = async () => {
    if (!here) return;
    const mine = ++seq.current;
    const at = key;
    setSlowKey(null);
    const timer = setTimeout(() => mine === seq.current && setSlowKey(at), 150);
    try {
      const listing = rowsOf(await api.listDir(here.path));
      if (mine !== seq.current) return;
      // The PS5's free space (its own tab), renewed with every listing; only the newest answer kept.
      if (platform === "ps5" && !device) {
        const kept = (s: { free: number; total: number } | null) => mine === seq.current && setStorage(s);
        api.storage().then(kept, () => kept(null));
      }
      setListed({ key: at, n: mine, listing });
      setListError(null);
    } catch (e) {
      if (mine !== seq.current) return;
      setListError({ key: at, n: mine, text: errorText(e) });
    } finally {
      clearTimeout(timer);
    }
  };
  const loadRef = useRef(load);
  loadRef.current = load;
  const trailRef = useRef(trail);
  trailRef.current = trail;
  const visibleRef = useRef(props.visible);
  visibleRef.current = props.visible;

  // Listed whenever the folder changes or the view shows (jobs may have written meanwhile).
  useEffect(() => {
    if (props.visible && trail.length > 0) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.visible, key]);

  // Deleted or moved elsewhere: leave a folder that went, else list again; during this view's own action it waits.
  // Refs and setters only, so the subscription's first copy stays right.
  const ownGone = useRef<string | null>(null);
  const missed = useRef<string[]>([]);
  const heard = (gone: string) => {
    clearTree();
    const s = trailRef.current;
    const i = s.findIndex((c, k) => k > 0 && c.path !== null && isWithin(c.path, gone));
    if (i > 0) {
      // As go() does, without moving focus: this view may not be the one shown.
      setTrail(s.slice(0, i));
      setFilter("");
      setFocus(0);
      setRenaming(null);
      setNotice(null);
      setActError(null);
    } else void loadRef.current();
  };
  useEffect(
    () =>
      onDeleted((gone) => {
        if (!acting.current) heard(gone);
        else if (gone !== ownGone.current) missed.current.push(gone);
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  useEffect(() => {
    if (!props.visible) clearTree();
  }, [props.visible]);
  useEffect(() => {
    if (!props.visible || !wide || treeBusy.current) return;
    const gen = treeGen.current;
    const c = trail.slice(0, -1).find((x) => subdirs.get(x.path ?? "")?.gen !== gen);
    if (!c) return;
    treeBusy.current = true;
    const done = (dirs: DirEntry[]) => {
      if (gen !== treeGen.current) return;
      treeBusy.current = false;
      setSubdirs((m) => new Map(m).set(c.path ?? "", { gen, dirs }));
    };
    api.listDir(c.path).then(
      (l) => done(rowsOf(l).entries.filter((e) => e.dir)),
      () => done([]), // the tree shows the way down; the list says what failed
    );
  }, [props.visible, wide, key, subdirs]);

  // A load that succeeds clears the error, so one shown is always newer than the listing kept.
  const error = listError && listError.key === key ? listError.text : null;
  const listing = !error && listed && listed.key === key ? listed.listing : null;
  // The PS5's browse roots, or an SMB/NFS root's shares or exports: nothing to change there.
  const roots = here !== null && (here.path === null || (!!device?.fixedRoot && here.path === device.root));
  const query = filter.trim().toLowerCase();
  const all = listing?.entries ?? [];
  const entries = query ? all.filter((e) => e.name.toLowerCase().includes(query)) : all;
  const shown = entries.slice(0, MAX_ROWS);
  const atState = Math.min(focus, Math.max(shown.length - 1, 0));

  /** `focusPath` null: the first row; undefined: focus stays where it is (the tree). */
  const go = (next: Crumb[], focusPath: string | null | undefined) => {
    setTrail(next);
    setFilter("");
    setFocus(0);
    setRenaming(null);
    setNotice(null);
    setActError(null);
    if (focusPath === undefined) want.current = null;
    else focusAfterLoad(focusPath ?? "");
  };
  const open = (e: DirEntry) =>
    e.dir && busy?.path !== e.path && go([...trail, { path: e.target ?? e.path, name: e.name, row: e.path }], null);
  const up = () => trail.length > 1 && go(trail.slice(0, -1), here ? (here.row ?? here.path) : null);

  // Only the tree scrolls to keep the folder shown in view, never the page.
  useEffect(() => {
    const nav = treeNav.current;
    const b = nav?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!nav || !b) return;
    const n = nav.getBoundingClientRect();
    const r = b.getBoundingClientRect();
    if (r.top < n.top) nav.scrollTop -= n.top - r.top;
    else if (r.bottom > n.bottom) nav.scrollTop += r.bottom - n.bottom;
  }, [key, subdirs, wide]);

  // A folder opened from the list: its rows went, so focus waits on the grid (keys still work).
  const pending = !listing && !error;
  useEffect(() => {
    if (props.visible && pending && want.current !== null && document.activeElement === document.body) grid.current?.focus();
  });

  const shownN = listing ? listed?.n : error ? listError?.n : undefined;
  useEffect(() => {
    if (!props.visible || want.current === null || shownN === undefined || shownN < want.current.after) return;
    const w = want.current.path;
    want.current = null;
    if (shown.length === 0) {
      grid.current?.focus();
      return;
    }
    const i = Math.max(w === "" ? 0 : shown.findIndex((e) => e.path === w), 0);
    setFocus(i);
    grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]")[i]?.focus();
  });

  // One action at a time; the folder is listed again before the others hear of it.
  const act = async (lead: string, run: () => Promise<string | null>, gone: string, moving = false) => {
    if (acting.current) return;
    acting.current = true;
    // A change in the folder shown keeps the tree above it; a move or a change elsewhere doesn't.
    const staleTree = () => {
      const s = trailRef.current;
      if (moving || s[s.length - 1]?.path !== dirname(gone)) clearTree();
    };
    const from = trailRef.current.map((c) => c.path ?? "").join("\0");
    try {
      const done = await run();
      if (done === null) return;
      // The user may have moved on while it ran: no notice over another folder, focus left where they took it.
      const moved = trailRef.current.map((c) => c.path ?? "").join("\0") !== from;
      const el = document.activeElement;
      if (moved || !visibleRef.current || (el && el !== document.body && !grid.current?.contains(el))) want.current = null;
      setActError(null);
      setNotice(moved ? null : done);
      staleTree();
      await loadRef.current();
      ownGone.current = gone;
      if (gone) notifyDeleted(gone);
    } catch (e) {
      setNotice(null);
      setActError({ lead, text: errorText(e) });
      // A delete that failed part way may have removed some of it: show what is there now.
      staleTree();
      void loadRef.current();
    } finally {
      acting.current = false;
      ownGone.current = null;
      setBusy(null);
      for (const g of missed.current.splice(0)) heard(g);
    }
  };
  const nextOf = (e: DirEntry) => {
    const i = shown.findIndex((x) => x.path === e.path);
    return (shown[i + 1] ?? shown[i - 1])?.path ?? "";
  };
  // A `.pkg` row's Install (the PS5): nothing here changes, so the folder isn't listed again.
  const install = async (e: DirEntry) => {
    if (acting.current) return;
    acting.current = true;
    setActError(null);
    setNotice(null);
    try {
      if (await installHere({ path: e.path, size: e.size }, props.dispatch)) setNotice(inst.started(e.name));
    } catch (err) {
      setActError({ lead: inst.cantInstall, text: errorText(err) });
    } finally {
      acting.current = false;
    }
  };
  const remove = (e: DirEntry) =>
    act(
      t.cantDelete,
      async () => {
        if (!(await confirmDelete(e.path, e.dir, deletedFile(e.name)))) return null;
        setBusy({ path: e.path, deleting: true });
        await api.deletePath(e.path);
        focusAfterLoad(nextOf(e));
        return t.deleted(e.path);
      },
      e.path,
    );
  const renamed = (e: DirEntry, name: string | null) => {
    setRenaming(null);
    want.current = { path: e.path, after: 0 };
    if (name === null) return;
    const to = join(dirname(e.path), name);
    // Case-insensitively (a drive may fold case), never itself; one of the same kind is replaced once confirmed.
    const taken = all.some((x) => x.path !== e.path && x.name.toLowerCase() === name.toLowerCase());
    // The question's Cancel gives focus back to the row, not the input just removed.
    if (taken) [...(grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]") ?? [])].find((r) => r.dataset.path === e.path)?.focus();
    void act(
      t.cantRename,
      async () => {
        if (taken && !(await confirmReplace([name], dirname(e.path)))) return null;
        setBusy({ path: e.path, deleting: false });
        await api.movePath(e.path, to, taken);
        focusAfterLoad(to);
        return t.renamed(to);
      },
      e.path,
    );
  };
  // `into`: a folder row it was dragged onto; else the picker asks (core moves within one device only).
  const move = (e: DirEntry, into?: string) =>
    act(
      t.cantMove,
      async () => {
        const dir =
          into ?? (await pick({ directory: true, title: t.moveTitle(e.name), start: here?.path ?? undefined, ...(device && { devices: true, peersOnly: !device.nas }) }));
        if (dir === null) return null;
        if (device ? !isWithin(dir, device.root) : isNet(dir)) throw new Error(t.otherDevice);
        if (dirname(e.path) === dir) throw new Error(t.sameFolder);
        const to = join(dir, e.name);
        // Same kind: asked, then replaced (a folder merged); any other clash goes without `replace` and core refuses it.
        // ponytail: `listDir` stops at 10,000 entries; a name past that isn't asked about and the server refuses it.
        // An exact name wins over a case variant (a case-sensitive folder may hold both).
        const listed = (await api.listDir(dir)).entries;
        const there = listed.find((x) => x.name === e.name) ?? listed.find((x) => x.name.toLowerCase() === e.name.toLowerCase());
        const replace = !!there && there.dir === e.dir && !there.link && !there.mounted;
        if (replace && !(await confirmReplace([e.name], dir))) return null;
        setBusy({ path: e.path, deleting: false });
        await api.movePath(e.path, to, replace);
        focusAfterLoad(nextOf(e));
        return t.moved(to);
      },
      e.path,
      true,
    );
  // A bookmark: the way down to it from the root, so Up and the tree work as if opened folder by folder.
  const openAt = async (path: string) => {
    setNotice(null);
    setActError(null);
    try {
      let base: Crumb[];
      if (device) base = [{ path: device.root, name: device.name }];
      else {
        const holder = (await api.listDir(null)).entries
          .filter((r) => r.dir && isWithin(path, r.path))
          .sort((a, b) => b.path.length - a.path.length)[0];
        if (!holder) throw new Error(path);
        base = [{ path: null, name: "" }, { path: holder.path, name: holder.name, row: holder.path }];
      }
      const next = [...base];
      let at = base[base.length - 1].path ?? "";
      for (const name of path.slice(at.length).split("/").filter(Boolean)) {
        at = join(at, name);
        next.push({ path: at, name });
      }
      go(next, null);
    } catch (e) {
      setActError({ lead: t.cantList, text: errorText(e) });
    }
  };
  // Busy until its answer: the button keeps focus (aria-disabled) and ignores presses meanwhile.
  const refresh = () => {
    if (refreshing) return;
    setNotice(null);
    setActError(null);
    clearTree();
    setRefreshing(true);
    void load().finally(() => setRefreshing(false));
  };
  const handOff = (to: HandOff, e: DirEntry) => {
    const kind = e.dir ? "folder" : sourceKind(e.name);
    if (kind) to({ path: e.path, kind });
  };

  // Drags: a row onto a folder row of this tab moves it there.
  const card = useRef<HTMLElement>(null);
  const folderRow = (el: Element, not?: DirEntry): DirEntry | undefined => {
    const row = el.closest<HTMLElement>("[role=row][data-path]");
    const d = row ? shown.find((x) => x.path === row.dataset.path) : undefined;
    return d && d.dir && !d.link && !d.mounted && d.path !== not?.path ? d : undefined;
  };
  const rowDrag = useRowDrag({
    target: (x, y, e) => {
      const el = document.elementFromPoint(x, y);
      if (!el || !card.current?.contains(el) || busy !== null || acting.current) return null;
      const d = folderRow(el, e);
      return d ? { dir: d } : null;
    },
    drop: (e, to) => void move(e, to.dir.path),
  });
  const over = rowDrag.over;
  const dropAt = over ? over.dir.path : null;
  const dropNote = over ? t.dropMove(readable(over.dir.path)) : null;

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.defaultPrevented || (e.target as HTMLElement).closest?.("[role=menu]")) return; // a row's ⋯ menu
    if ((e.target as HTMLElement).tagName === "BUTTON" && (e.key === "Enter" || e.key === " ")) return;
    const from = (e.target as HTMLElement).closest?.("[role=row][data-path]") as HTMLElement | null;
    const fromAt = from ? shown.findIndex((x) => x.path === from.dataset.path) : -1;
    const at = fromAt >= 0 ? fromAt : atState;
    const row = shown[at];
    const changeable = row && !roots && !row.link && !row.mounted && busy === null;
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
    else if (e.key === "Enter" && row) {
      e.preventDefault();
      open(row);
    } else if (e.key === "Backspace" || (e.key === "ArrowUp" && e.altKey)) {
      e.preventDefault();
      up();
    } else if (e.key === "F2" && changeable) {
      e.preventDefault();
      setRenaming(row.path);
    } else if (e.key === "Delete" && changeable) {
      e.preventDefault();
      void remove(row);
    }
  };

  const dirs = all.filter((e) => e.dir).length;
  const count = !listing
    ? ""
    : entries.length > MAX_ROWS
      ? t.firstOf(MAX_ROWS, entries.length)
      : query
        ? t.matches(entries.length)
        : t.inFolder(dirs, all.length - dirs);

  // The tree: root to the folder shown, each with its subfolders (the first MAX_ROWS).
  const tree = wide && trail.length > 0;
  const treeItem = (key: string, name: string, icon: "folder" | "drive", current: boolean, onClick: () => void, nested?: ReactNode) => (
    <li key={key}>
      <button type="button" aria-current={current ? "true" : undefined} tabIndex={current ? 0 : -1} title={name} onClick={onClick}>
        <Icon name={icon} />
        <span>{name}</span>
      </button>
      {nested}
    </li>
  );
  const branch = (i: number): ReactNode => {
    const next = i + 1 < trail.length ? trail[i + 1] : null;
    const nextRow = next ? (next.row ?? next.path) : null;
    const known = i === trail.length - 1 ? listing?.entries.filter((e) => e.dir) : subdirs.get(trail[i].path ?? "")?.dirs;
    const dirs = (known ?? []).slice(0, MAX_ROWS);
    const items = dirs.map((e) =>
      e.path === nextRow
        ? chainItem(i + 1)
        : treeItem(e.path, e.name, "folder", false, () => go([...trail.slice(0, i + 1), { path: e.target ?? e.path, name: e.name, row: e.path }], undefined)),
    );
    if (next && !dirs.some((e) => e.path === nextRow)) items.push(chainItem(i + 1));
    return items.length > 0 ? <ul>{items}</ul> : null;
  };
  const chainItem = (i: number): ReactNode => {
    const c = trail[i];
    const last = i === trail.length - 1;
    const onClick = () => !last && go(trail.slice(0, i + 1), undefined);
    return treeItem(c.row ?? c.path ?? "", crumbName(c), c.path === null ? "drive" : "folder", last, onClick, branch(i));
  };

  // One tab stop; the arrows, Home and End move between the tree's folders.
  const treeKey = (e: KeyboardEvent<HTMLElement>) => {
    const all = [...e.currentTarget.querySelectorAll<HTMLElement>("button")];
    const i = all.indexOf(document.activeElement as HTMLElement);
    const n = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? all.length - 1 : -2;
    if (n === -2 || i < 0) return;
    e.preventDefault();
    all[Math.min(Math.max(n, 0), all.length - 1)]?.focus();
  };

  return (
    <section className="card dev-window explorer files storage-card" {...props.panel} hidden={props.hidden} ref={card}>
      {rowDrag.ghost}
      <div className="storage-head">
        {props.switcher}
        <div className="storage-actions">
          <FreeSpace storage={storage} />
          <button type="button" className="small" onClick={refresh} aria-disabled={refreshing || undefined} aria-busy={refreshing || undefined}>
            <Icon name="refresh" />
            {t.refresh}
            {refreshing && <Dots />}
          </button>
        </div>
      </div>
      <div className={`files-panes${tree ? " tree" : ""}`}>
        {tree && (
          <nav className="files-tree" ref={treeNav} aria-label={t.folders} onKeyDown={treeKey}>
            <p className="files-tree-label" aria-hidden="true">
              {t.folders}
            </p>
            <ul>{chainItem(0)}</ul>
          </nav>
        )}
        <div className="files-main">
          <div className="row nowrap explorer-nav">
            <button type="button" className="small" onClick={up} disabled={trail.length < 2} title={t.upTip}>
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
                        {crumbName(c)}
                      </span>
                    ) : (
                      <button type="button" className="link" title={c.path ?? undefined} onClick={() => go(trail.slice(0, i + 1), trail[i + 1].row ?? trail[i + 1].path)}>
                        {crumbName(c)}
                      </button>
                    )}
                  </span>
                );
              })}
            </nav>
            <BookmarkToggle path={here?.path ?? null} root={root} />
          </div>
          <Bookmarks root={root} rootName={device?.name} here={here?.path ?? null} onOpen={(p) => void openAt(p)} />
          {actError && (
            <p className="note-line block" role="alert">
              <Icon name="alert" />
              <span>
                {actError.lead} <span className="backend">{actError.text}</span>
              </span>
            </p>
          )}
          {/* Always there, so screen readers announce the text when it arrives. */}
          <p className="muted files-notice" role="status">
            {notice}
          </p>
          {listing?.truncated && (
            <p className="note-line warn explorer-note">
              <Icon name="warn" />
              <span>{t.truncated}</span>
            </p>
          )}
          {all.some((e) => e.mounted) && (
            <p className="note-line explorer-note">
              <Icon name="info" />
              <span>{t.mountedNote}</span>
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
              {dropNote ?? count}
            </span>
          </div>
          <div className="explorer-well">
            {/* Mounted while a folder lists too, so focus can rest on it. */}
            <div
              role="grid"
              ref={grid}
              className={`xgrid files${roots ? "" : " editable"}`}
              aria-label={here ? crumbName(here) : t.title}
              aria-rowcount={shown.length + 1}
              aria-busy={pending || undefined}
              tabIndex={shown.length === 0 ? 0 : undefined}
              onKeyDown={onKey}
            >
              <div role="row" className="xrow head" aria-rowindex={1}>
                <span role="columnheader" className="xname">
                  {t.name}
                </span>
                <span role="columnheader" className="xsize">
                  {t.size}
                </span>
                <span role="columnheader" className="xact">
                  <span className="visually-hidden">{actionsHead}</span>
                </span>
              </div>
              {!error &&
                shown.map((e, i) => {
                  const mine = i === atState ? 0 : -1;
                  const changeable = !roots && !e.link && !e.mounted;
                  const source = !roots && !e.link && (e.dir || sourceKind(e.name) !== null);
                  const pkg = platform === "ps5" && !roots && !e.link && !e.dir && extOf(e.name) === "pkg";
                  const editing = renaming === e.path;
                  // The row being changed keeps focus on its button (aria-disabled); the others' lock.
                  const acted = busy?.path === e.path;
                  const locked = busy !== null && !acted;
                  return (
                    <div
                      key={e.name}
                      role="row"
                      data-path={e.path}
                      data-dir={e.dir ? "1" : undefined}
                      aria-rowindex={i + 2}
                      tabIndex={mine}
                      className={`xrow${e.dir ? " dir" : ""}${dropAt === e.path ? " drop" : ""}`}
                      onPointerDown={(ev) => changeable && !editing && busy === null && rowDrag.onPointerDown(ev, e)}
                      onFocus={(ev) => ev.target === ev.currentTarget && setFocus(i)}
                      onDoubleClick={() => !editing && open(e)}
                    >
                      <span role="gridcell" className="xname">
                        <Icon name={e.dir ? "folder" : sourceKind(e.name) ? "archive" : "file"} />
                        {editing ? (
                          <NameInput
                            initial={e.name}
                            label={t.rename(e.name)}
                            taken={(n) => all.some((x) => x.path !== e.path && x.name.toLowerCase() === n.toLowerCase() && (x.dir !== e.dir || !!x.link || !!x.mounted))}
                            onDone={(n) => renamed(e, n)}
                            dir={e.dir}
                          />
                        ) : (
                          <span className="xlabel">{e.name}</span>
                        )}
                        {e.link && (
                          <>
                            <span className="tag xmark" title={t.linkTip}>
                              {t.link}
                            </span>
                            {/* The tip's reason for screen readers, keyboards and touch, which get no title. */}
                            <span className="visually-hidden">{t.linkTip}</span>
                          </>
                        )}
                        {e.mounted && (
                          <span className="tag xmark" title={t.mountedTip}>
                            {t.mounted}
                          </span>
                        )}
                      </span>
                      <span role="gridcell" className="xsize">
                        {e.size !== null && !e.dir ? prettyBytes(e.size) : ""}
                      </span>
                      <span role="gridcell" className="xact">
                        {changeable && !editing && (
                          <>
                            <button type="button" className="small icon-only xbtn" tabIndex={mine} disabled={locked} aria-disabled={acted || undefined} aria-label={t.rename(e.name)} title={t.renameTip} onClick={() => !acted && setRenaming(e.path)}>
                              <Icon name="pencil" />
                            </button>
                            <button type="button" className="small icon-only xbtn" tabIndex={mine} disabled={locked} aria-disabled={acted || undefined} aria-label={t.move(e.name)} title={t.moveTip} onClick={() => void move(e)}>
                              <Icon name="move" />
                            </button>
                            <button
                              type="button"
                              className="small icon-only xbtn danger"
                              tabIndex={mine}
                              disabled={locked}
                              aria-disabled={acted || undefined}
                              aria-label={acted && busy?.deleting ? deleting(e.name) : t.remove(e.name)}
                              title={acted && busy?.deleting ? deleting(e.name) : isMac ? t.removeTipMac : t.removeTip}
                              onClick={() => void remove(e)}
                            >
                              <Icon name="trash" />
                            </button>
                          </>
                        )}
                        {source && !editing && (
                          <ChooseMenu
                            className="small icon-only xbtn"
                            label={<Icon name="dots" />}
                            buttonLabel={t.more(e.name)}
                            menuLabel={t.more(e.name)}
                            tabIndex={mine}
                            disabled={acted}
                            items={[
                              { id: "edit", icon: "pencil", label: tabs.edit },
                              { id: "convert", icon: "convert", label: build },
                            ]}
                            onChoose={(what) => handOff(what === "edit" ? props.onEdit : props.onConvert, e)}
                          />
                        )}
                        {pkg && !editing && (
                          <button type="button" className="small icon-only xbtn" tabIndex={mine} disabled={locked || acted} aria-label={inst.installName(e.name)} title={inst.installName(e.name)} onClick={() => void install(e)}>
                            <Icon name="package" />
                          </button>
                        )}
                        {e.dir && (
                          <button type="button" className="small icon-only xopen" tabIndex={-1} disabled={acted} aria-label={t.open(e.name)} title={t.open(e.name)} onClick={() => open(e)}>
                            <Icon name="chevron" />
                          </button>
                        )}
                      </span>
                    </div>
                  );
                })}
            </div>
            {pending && (!here || slowKey === key) && (
              <div className="library-busy" role="status" aria-live="polite">
                <span className="spinner" aria-hidden="true" />
                <span>
                  {here ? crumbName(here) : loading}
                  <Dots />
                </span>
              </div>
            )}
            {error ? (
              <p className="note-line block empty-note" role="alert">
                <Icon name="alert" />
                <span>
                  {t.cantList} <span className="backend">{error}</span>
                </span>
              </p>
            ) : (
              listing &&
              shown.length === 0 && (
                <p className="muted empty-note">{query ? t.noMatch(filter.trim()) : roots ? t.noRoots : t.emptyFolder}</p>
              )
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
