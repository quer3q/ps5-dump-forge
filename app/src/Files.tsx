// The Storage tab's File explorer: one folder at a time, drawn as Edit's explorer, where every
// action happens at once (no pending list, no Apply): delete (after the shared question),
// rename in place, move to a folder the picker chooses, and open a folder or an image/archive
// in Edit or Convert. On a computer it browses the chosen folder (shared with the Library; Up
// stops there), until one is chosen the home folder (the app; not shared, so the Library doesn't
// scan it) or the server's roots (the http build); on the PS5 the server's browse roots. Links
// are listed and can be opened, never changed. The folder is listed again after each action and
// whenever the view shows. From 880px a folder tree beside the list shows the way from the root
// to the folder shown, each folder on it with its subfolders (listed lazily, kept while the view
// shows, dropped on Refresh); narrower, the crumbs do that. On a computer each online paired PS5
// gets a tab of its own (FilesTabs.tsx): its files, changed as these are; drags copy between them.

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { api, errorText, onDrop, pick, prettyBytes, runtimePlatform, web, type DirEntry, type DirListing, type RuntimePlatform } from "./api";
import {
  ARCHIVE_EXTENSIONS,
  basename,
  ChooseMenu,
  dirname,
  Dots,
  IMAGE_EXTENSIONS,
  isMac,
  isWithin,
  joinPath,
  notifyDeleted,
  onDeleted,
  panelId,
  PathLine,
  tabId,
  TabList,
  type SourceSelection,
} from "./common";
import { confirmDelete, type DeletedFile } from "./delete";
import { installHere } from "./InstallDialog";
import type { Action } from "./jobs";
import { readable } from "./peers";
import { confirmReplace } from "./replace";
import { MAX_ROWS, NameInput } from "./Edit";
import { CopyStatus, FILES_TABS, LOCAL, tabAt, useCopy, usePs5Tabs, useRowDrag, type DeviceTab, type DragTarget } from "./FilesTabs";
import { FreeSpace } from "./Library";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import type { HandOff } from "./Storage";

/** One folder of the way down from the explorer's root; `path` null: the PS5's browse roots. */
interface Crumb {
  path: string | null;
  name: string;
  /** The row it was opened from (a link's own path; `path` is then its target), focused on the way back up. */
  row?: string;
}

const extOf = (name: string) => {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
};

/** What Edit and Convert would open a file as, by extension; null: not a source. */
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

/** From 880px the folder tree shows beside the list. The PS5's old WebKit has `addListener` only. */
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

/** Core gives a link's resolved target as its `path`, which can name another row of the same
 * folder; a link row keeps its own path (unique; links get no actions) and its `target`, where
 * opening it goes. */
const rowsOf = (l: DirListing): DirListing => ({
  ...l,
  entries: l.entries.map((e) =>
    e.link && l.path !== null ? { ...e, path: joinPath(l.path, e.name), target: e.path } : e,
  ),
});

/** Storage's File explorer view: one explorer, or on a computer with a paired PS5 online, a tab
 * per device (each kept mounted, so it keeps its folder). */
export function Files(props: {
  visible: boolean;
  folder: string | null;
  onFolder: (folder: string) => void;
  onEdit: HandOff;
  onConvert: HandOff;
  dispatch: (a: Action) => void;
  /** Opens the Connect tab (a started copy's button). */
  onConnect: () => void;
  switcher?: ReactNode;
}) {
  const t = useMessages().storage;
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  useEffect(() => {
    runtimePlatform().then(setPlatform, () => {});
  }, []);
  const peers = usePs5Tabs(platform === "host");
  const [tab, setTab] = useState(LOCAL);
  const [overTab, setOverTab] = useState<string | null>(null);
  // Each tab's folder shown (null: none to copy into), where a drag onto its tab lands.
  const heres = useRef(new Map<string, string | null>());
  const { copy, line } = useCopy(props.dispatch);
  const online = peers.all.filter((d) => peers.online.has(d.id));
  // A device gone offline: its explorer stays (and its folder), its tab goes, This device shows.
  const shown = online.some((d) => d.id === tab) ? tab : LOCAL;
  const tabs =
    online.length === 0 ? undefined : (
      <TabList
        label={t.devices}
        prefix={FILES_TABS}
        className="files-tabs"
        tabs={[
          { id: LOCAL, label: t.thisDevice, className: overTab === LOCAL ? "drop" : undefined },
          ...online.map((d) => ({ id: d.id, label: <span title={d.name}>{d.name}</span>, className: overTab === d.id ? "drop" : undefined })),
        ]}
        value={shown}
        onChange={setTab}
      />
    );
  const pane = (key: string, device?: DeviceTab) => (
    <div key={key} hidden={key !== shown} {...(tabs && { role: "tabpanel", id: panelId(FILES_TABS, key), "aria-labelledby": tabId(FILES_TABS, key) })}>
      <Explorer
        {...props}
        visible={props.visible && key === shown}
        device={device}
        tabKey={key}
        // One tab bar in the page (its buttons' ids are unique): only the shown pane renders it.
        tabs={key === shown ? tabs : undefined}
        copy={(paths, dest) => void copy(paths, dest)}
        folderOf={(k) => heres.current.get(k) ?? null}
        copyLine={<CopyStatus line={line} onConnect={props.onConnect} />}
        // A PS5's root (`/`: list only, data and drives below) takes no copy: none to drop into.
        onHere={(p) => heres.current.set(key, device && p === device.root ? null : p)}
        onOverTab={setOverTab}
      />
    </div>
  );
  return (
    <>
      {pane(LOCAL)}
      {peers.all.map((d) => pane(d.id, d))}
    </>
  );
}

function Explorer(props: {
  visible: boolean;
  /** The folder both Storage views share (computer). */
  folder: string | null;
  onFolder: (folder: string) => void;
  onEdit: HandOff;
  onConvert: HandOff;
  /** Starts an install job's row (a `.pkg` row's Install, the PS5 only). */
  dispatch: (a: Action) => void;
  /** Storage's Library | File explorer switch, first in this view's card. */
  switcher?: ReactNode;
  /** Another device's tab (a paired PS5): its share root, no Choose folder. */
  device?: DeviceTab;
  /** This explorer's tab id, and the device tabs (under the head; none without a peer online). */
  tabKey: string;
  tabs?: ReactNode;
  /** Copies `paths` into `dest` as a transfer job; `folderOf`: a tab's folder shown. */
  copy: (paths: string[], dest: string | null) => void;
  folderOf: (tab: string) => string | null;
  /** The started copy's line (shared by the tabs). */
  copyLine: ReactNode;
  /** The folder shown (null: the server's roots), for drags onto this tab. */
  onHere: (path: string | null) => void;
  /** The device tab a drag is over, to ring it. */
  onOverTab: (tab: string | null) => void;
}) {
  const device = props.device;
  const t = useMessages().storage;
  const inst = useMessages().install;
  const tabs = useMessages().app;
  const deleting = useMessages().library.deleting;
  const actionsHead = useMessages().edit.actions;
  const loading = useMessages().picker.loading;
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  // The app's start before a folder is chosen: undefined while asked, null when there is none.
  const [home, setHome] = useState<string | null | undefined>(undefined);
  const [trail, setTrail] = useState<Crumb[]>([]);
  // The listing and the trail it belongs to (another folder's shows as loading).
  // `n`: the load that answered, so focus waits for the listing it was asked against.
  const [listed, setListed] = useState<{ key: string; n: number; listing: DirListing } | null>(null);
  const [listError, setListError] = useState<{ key: string; n: number; text: string } | null>(null);
  const [filter, setFilter] = useState("");
  const [focus, setFocus] = useState(0);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [actError, setActError] = useState<{ lead: string; text: string } | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const seq = useRef(0);
  // A folder's spinner shows only once its listing takes over 150ms (key: that folder's trail),
  // so a fast one doesn't flash it.
  const [slowKey, setSlowKey] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [storage, setStorage] = useState<{ free: number; total: number } | null>(null);
  const acting = useRef(false);
  // The row an action is changing (shown, and every row's actions locked, until it ends).
  const [busy, setBusy] = useState<{ path: string; deleting: boolean } | null>(null);
  /** The row to focus ("" = the first) once load `after` (or a later one) has rendered. */
  const want = useRef<{ path: string; after: number } | null>(null);
  const focusAfterLoad = (path: string) => (want.current = { path, after: seq.current + 1 });
  const grid = useRef<HTMLDivElement>(null);
  const treeNav = useRef<HTMLElement>(null);
  const wide = useWide();
  // The tree's subfolders of each folder above the one shown (key: its path, "" the PS5's
  // roots); the folder shown takes its own from the listing. `gen`: the `treeGen` it was listed
  // in; an older one is listed again, and shown until its answer replaces it (so the tree keeps
  // its shape). `treeGen` also drops a stale answer.
  const [subdirs, setSubdirs] = useState<Map<string, { gen: number; dirs: DirEntry[] }>>(() => new Map());
  const treeGen = useRef(0);
  const treeBusy = useRef(false);
  /** Lists the tree again; `drop` (a new root) forgets it at once. Folders off the trail go. */
  const clearTree = (drop = false) => {
    treeGen.current++;
    treeBusy.current = false;
    const on = new Set(trailRef.current.map((c) => c.path ?? ""));
    setSubdirs((m) => (drop ? new Map() : new Map([...m].filter(([k]) => on.has(k)))));
  };

  const lookup = () =>
    runtimePlatform().then(
      (p) => {
        setLookupError(null);
        setPlatform(p);
      },
      (e) => setLookupError(errorText(e)),
    );
  useEffect(() => void lookup(), []);
  useEffect(() => {
    if (!web) api.homeFolder().then(setHome, () => setHome(null));
  }, []);

  // The root: the server's roots (named at render; the PS5, or a computer's http build before a
  // folder is chosen), the chosen folder, else the app's home; a new one starts the trail again.
  // undefined: not known yet (or none), so no trail.
  const root: string | null | undefined = device
    ? device.root
    : platform === "ps5"
      ? null
      : platform === "host"
        ? (props.folder ?? (web ? null : (home ?? undefined)))
        : undefined;
  useEffect(() => {
    setTrail(root === undefined ? [] : [{ path: root, name: root === null ? "" : device ? device.name : basename(root) || root }]);
    setFilter("");
    setFocus(0);
    setRenaming(null);
    setNotice(null);
    setActError(null);
    clearTree(true);
  }, [root]);
  const crumbName = (c: Crumb) => (c.path === null ? t.roots : c.name);

  const key = trail.map((c) => c.path ?? "").join("\0");
  const here = trail.length > 0 ? trail[trail.length - 1] : null;
  const onHere = props.onHere;
  useEffect(() => {
    onHere(here ? here.path : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [here?.path]);

  const load = async () => {
    if (!here) return;
    const mine = ++seq.current;
    const at = key;
    setSlowKey(null);
    const timer = setTimeout(() => mine === seq.current && setSlowKey(at), 150);
    try {
      const listing = rowsOf(await api.listDir(here.path));
      if (mine !== seq.current) return;
      // The PS5's free space, renewed with every listing (an action lists again); only the
      // newest listing's answer is kept.
      if (platform === "ps5") {
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

  // Deleted, renamed or moved elsewhere (Edit, the Library): leave a folder that went, else list
  // again. Heard while this view's own action runs (its own `gone` aside), it waits for the end.
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

  // Kept while the view shows; one folder listed at a time, from the root down.
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
  const roots = here !== null && here.path === null; // the PS5's browse roots: nothing to change
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
  // Not the row an action is changing (a folder being deleted or moved).
  const open = (e: DirEntry) =>
    e.dir && busy?.path !== e.path && go([...trail, { path: e.target ?? e.path, name: e.name, row: e.path }], null);
  const up = () => trail.length > 1 && go(trail.slice(0, -1), here ? (here.row ?? here.path) : null);

  // The tree keeps the folder shown in view (subfolders listed above it push it down); only the
  // tree scrolls, never the page.
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

  // Focus the row asked for, once the listing it is in has rendered.
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
    // The tree keeps the folders above the one shown: a change in the folder shown leaves them
    // as they are; a move (its target may be one of them) or a change elsewhere doesn't.
    const staleTree = () => {
      const s = trailRef.current;
      if (moving || s[s.length - 1]?.path !== dirname(gone)) clearTree();
    };
    const from = trailRef.current.map((c) => c.path ?? "").join("\0");
    try {
      const done = await run();
      if (done === null) return;
      // The user may have moved on while it ran (another folder, the other view, the filter):
      // no notice over another folder, and focus stays where they took it.
      const moved = trailRef.current.map((c) => c.path ?? "").join("\0") !== from;
      const el = document.activeElement;
      if (moved || !visibleRef.current || (el && el !== document.body && !grid.current?.contains(el))) want.current = null;
      setActError(null);
      setNotice(moved ? null : done);
      staleTree();
      // The folder shown now.
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
  // A `.pkg` row's Install (the PS5): asked, then an install job listed under Storage. Nothing
  // here changes, so the folder isn't listed again.
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
    const to = joinPath(dirname(e.path), name);
    // The input refuses a name of the other kind (or a link); one of the same kind is replaced
    // once the user confirms (a folder merged).
    // Case-insensitively (a drive may fold case), never itself (a case-only rename).
    const taken = all.some((x) => x.path !== e.path && x.name.toLowerCase() === name.toLowerCase());
    // The question's Cancel gives focus back to what had it: the row, not the input just removed.
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
  // `into`: a folder row it was dragged onto; else the picker asks (on a peer's tab, its tabs:
  // core moves within one device only).
  const move = (e: DirEntry, into?: string) =>
    act(
      t.cantMove,
      async () => {
        const dir =
          into ?? (await pick({ directory: true, title: t.moveTitle(e.name), start: here?.path ?? undefined, ...(device && { devices: true, peersOnly: true }) }));
        if (dir === null) return null;
        if (device && !isWithin(dir, device.root)) throw new Error(t.otherDevice);
        if (dirname(e.path) === dir) throw new Error(t.sameFolder);
        const to = joinPath(dir, e.name);
        // A file onto a file, a folder onto a folder (merged): asked, then replaced. Any other
        // clash (another kind, a link) goes without `replace`, so core refuses it as before.
        // ponytail: `listDir` stops at 10,000 entries; a name past that isn't asked about, so the
        // move goes without `replace` and the server refuses it ("already exists").
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
  const choose = async () => {
    const p = await pick({ directory: true, title: t.pickTitle, start: root ?? undefined });
    if (p === null) return;
    if (p === root) go([{ path: p, name: basename(p) || p }], null);
    props.onFolder(p);
  };
  // Busy until its answer: the button keeps focus (aria-disabled) and ignores presses meanwhile.
  const refresh = () => {
    if (refreshing) return;
    setNotice(null);
    setActError(null);
    clearTree();
    setRefreshing(true);
    void (platform ? load() : lookup()).finally(() => setRefreshing(false));
  };
  const handOff = (to: HandOff, e: DirEntry) => {
    const kind = e.dir ? "folder" : sourceKind(e.name);
    if (kind) to({ path: e.path, kind });
  };

  // Drags: a row onto a folder row of this tab moves it there, onto another device's tab copies
  // it into that tab's folder shown. Files dropped from the system copy into the folder row
  // under them, the tab under them, else the folder shown. ponytail: the web build's browser
  // files have no path here, so its drops are ignored (Upload under Connect sends them).
  const card = useRef<HTMLElement>(null);
  const folderRow = (el: Element, not?: DirEntry): DirEntry | undefined => {
    const row = el.closest<HTMLElement>("[role=row][data-path]");
    const d = row ? shown.find((x) => x.path === row.dataset.path) : undefined;
    return d && d.dir && !d.link && !d.mounted && d.path !== not?.path ? d : undefined;
  };
  const rowDrag = useRowDrag({
    target: (x, y, e) => {
      const el = document.elementFromPoint(x, y);
      if (!el) return null;
      const tab = tabAt(el);
      if (tab !== undefined) return tab !== props.tabKey ? { tab } : null;
      if (!card.current?.contains(el) || busy !== null || acting.current) return null;
      const d = folderRow(el, e);
      return d ? { dir: d } : null;
    },
    drop: (e, to) => ("tab" in to ? props.copy([e.path], props.folderOf(to.tab)) : void move(e, to.dir.path)),
  });
  type Landing = { tab: string } | { into: string | null };
  const [osOver, setOsOver] = useState<Landing | null>(null);
  const osHit = (x: number, y: number): Landing | null => {
    if (!props.visible || roots) return null;
    const el = document.elementFromPoint(x, y);
    if (!el || !card.current?.contains(el)) return null;
    const tab = tabAt(el);
    return tab !== undefined ? { tab } : { into: folderRow(el)?.path ?? here?.path ?? null };
  };
  const land = (l: Landing): string | null =>
    "tab" in l ? (l.tab === props.tabKey ? (here?.path ?? null) : props.folderOf(l.tab)) : l.into;
  const liveDrop = useRef({ osHit, land, copy: props.copy });
  liveDrop.current = { osHit, land, copy: props.copy };
  useEffect(() => {
    if (web) return;
    return onDrop(
      (d) => {
        const l = liveDrop.current;
        const at = l.osHit(d.x, d.y);
        setOsOver(null);
        const paths = d.items.flatMap((i) => (i.path ? [i.path] : []));
        if (at && paths.length > 0) l.copy(paths, l.land(at));
      },
      (pt) => setOsOver(pt ? liveDrop.current.osHit(pt.x, pt.y) : null),
    );
  }, []);
  const over: DragTarget | Landing | null = rowDrag.over ?? osOver;
  const overTab = over && "tab" in over ? over.tab : null;
  const onOverTab = props.onOverTab;
  useEffect(() => {
    if (props.visible) onOverTab(overTab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overTab, props.visible]);
  // The folder row (or the folder shown) a drop would land in, ringed, and what the count line says.
  const dropAt = over && "dir" in over ? over.dir.path : over && "into" in over ? over.into : null;
  const dropNote = !over
    ? null
    : "dir" in over
      ? t.dropMove(readable(over.dir.path))
      : (() => {
          const dest = land(over);
          return dest === null ? null : t.dropCopy(readable(dest));
        })();

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

  // A computer's own files (a peer's tab has its root: no Choose folder).
  const host = platform === "host" && !device;
  // A computer with no folder to show (no home, or the start can't be listed): choose one.
  const unrooted = host && !props.folder && (root === undefined ? home === null : !!error && trail.length === 1);
  if (unrooted)
    return (
      <section className="card explorer files storage-card bare" aria-label={t.title}>
        <div className="storage-head">{props.switcher}</div>
        {props.tabs}
        {error && (
          <p className="note-line block" role="alert">
            <Icon name="alert" />
            <span>
              {t.cantList} <span className="backend">{error}</span>
            </span>
          </p>
        )}
        <div className="empty library-empty">
          <span className="empty-icon">
            <Icon name="folder" />
          </span>
          <p className="empty-title">{t.pickPrompt}</p>
          <div className="row center">
            <button type="button" className="primary" onClick={() => void choose()}>
              <Icon name="folder" />
              {t.choose}
            </button>
          </div>
        </div>
      </section>
    );

  // Where the backend runs isn't known (the server didn't answer): Refresh asks again.
  if (!platform && lookupError)
    return (
      <section className="card explorer files storage-card bare" aria-label={t.title}>
        <div className="storage-head">
          {props.switcher}
          <div className="storage-actions">
            <button type="button" className="small" onClick={refresh} aria-disabled={refreshing || undefined} aria-busy={refreshing || undefined}>
              <Icon name="refresh" />
              {t.refresh}
              {refreshing && <Dots />}
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
    );

  const dirs = all.filter((e) => e.dir).length;
  const count = !listing
    ? ""
    : entries.length > MAX_ROWS
      ? t.firstOf(MAX_ROWS, entries.length)
      : query
        ? t.matches(entries.length)
        : t.inFolder(dirs, all.length - dirs);

  // The tree: the way from the root to the folder shown, each folder on it with its subfolders
  // (the first MAX_ROWS); the next folder on the way is always there, listed or not.
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

  // One tab stop (the folder shown); the arrows, Home and End move between the tree's folders.
  const treeKey = (e: KeyboardEvent<HTMLElement>) => {
    const all = [...e.currentTarget.querySelectorAll<HTMLElement>("button")];
    const i = all.indexOf(document.activeElement as HTMLElement);
    const n = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? all.length - 1 : -2;
    if (n === -2 || i < 0) return;
    e.preventDefault();
    all[Math.min(Math.max(n, 0), all.length - 1)]?.focus();
  };

  return (
    <section className="card explorer files storage-card" aria-label={t.title} ref={card}>
      {rowDrag.ghost}
      <div className="storage-head">
        {props.switcher}
        <div className="storage-actions">
          <FreeSpace storage={storage} />
          {host && root && (
            <div className="library-folder">
              <PathLine path={root} />
            </div>
          )}
          <button type="button" className="small" onClick={refresh} aria-disabled={refreshing || undefined} aria-busy={refreshing || undefined}>
            <Icon name="refresh" />
            {t.refresh}
            {refreshing && <Dots />}
          </button>
          {host && (
            <button type="button" className="small" onClick={() => void choose()}>
              <Icon name="folder" />
              {t.choose}
            </button>
          )}
        </div>
      </div>
      {props.tabs}
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
          </div>
          {actError && (
            <p className="note-line block" role="alert">
              <Icon name="alert" />
              <span>
                {actError.lead} <span className="backend">{actError.text}</span>
              </span>
            </p>
          )}
          {props.copyLine}
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
          <div className={dropAt !== null && dropAt === here?.path ? "explorer-well drop" : "explorer-well"}>
            {/* Mounted while a folder lists too (its head row only), so focus can rest on it. */}
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
                              { id: "convert", icon: "convert", label: tabs.convert },
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
