// The Storage tab's Library view: the game dumps under one folder, as cards or compact rows,
// filtered by kind chips (All, each kind present, LZ4, zstd, Additional content). On a
// computer the user chooses the folder (nothing scans before that; a refused folder gets an
// error screen), here or in the File explorer (one folder for both views); on the PS5 the first
// visit scans the server's roots. The list, the folder and the view live in memory; only the
// first visit (PS5), Refresh and a newly chosen folder scan. Covers load one at a time after
// the list.

import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { api, errorText, pick, runtimePlatform, type LibraryEntry, type LibraryScan, type RuntimePlatform } from "./api";
import { basename, Dots, FormatPill, isArchive, isWithin, kindLabel, notifyDeleted, onDeleted, PathLine } from "./common";
import { confirmDelete } from "./delete";
import { gameTitle, num, useMessages } from "./i18n";
import { Icon } from "./icons";

// Read at render: gameTitle follows the language, and the components re-render on a switch.
const name = (e: LibraryEntry) => gameTitle(e.title_names, e.title_name) || e.title_id || basename(e.path);

/** The kind chips' filter: "all", an entry's kind, or one of the three labels (LZ4,
 * zstd, Additional content), which can't clash with a kind. */
type Filter = string;
const LZ4 = "#lz4";
const ZSTD = "#zstd";
const ADDON = "#addon";
const KIND_ORDER = ["folder", "ffpkg", "exfat", "ffpfs", "ffpfsc", "pkg"];
/** The cards' row gap (`.library-cards`), and how much of the second row the well shows. */
const ROW_GAP = 16;
const PEEK = 56;
const matches = (e: LibraryEntry, f: Filter) =>
  f === "all" ||
  (f === LZ4
    ? e.lz4_packed === true
    : f === ZSTD
      ? e.zstd_packed === true
      : f === ADDON
        ? e.additional_content === true
        : e.kind === f);

/** Storage in decimal GB as the console's Settings shows it: whole numbers, one decimal under 10 GB. */
const gb = (bytes: number) => {
  // The precision follows the rounded value: 9.96 GB is "10 GB", not "10.0 GB".
  const tenths = Math.round(bytes / 1e8) / 10;
  return `${tenths < 10 ? num(tenths, 1) : num(Math.round(bytes / 1e9))} GB`;
};

/** The PS5's free space, first in a Storage view's head actions (before Refresh). */
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

export function Library(props: {
  visible: boolean;
  /** The folder both Storage views share (computer); a new one from the File explorer is
   * scanned once this view shows, and dropped (null) if refused. */
  folder: string | null;
  onFolder: (folder: string | null) => void;
  onEdit: (entry: LibraryEntry) => void;
  onConvert: (entry: LibraryEntry) => void;
  /** Storage's Library | File explorer switch, first in this card's head. */
  switcher?: ReactNode;
}) {
  const t = useMessages().library;
  const c = useMessages().common;
  const loading = useMessages().picker.loading;
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  const [folder, setFolderState] = useState<string | null>(null); // the accepted folder (computer)
  const folderRef = useRef<string | null>(null); // the same, read at call time by scan
  const setFolder = (f: string | null) => {
    folderRef.current = f;
    setFolderState(f);
    props.onFolder(f);
  };
  // The shared folder as the File explorer last set it, read when a scan ends.
  const shared = useRef(props.folder);
  shared.current = props.folder;
  const [target, setTarget] = useState<string | null>(null); // the folder being scanned
  const [refused, setRefused] = useState(false); // a newly chosen folder failed: the error screen
  const [view, setView] = useState<"cards" | "list">("cards");
  const [filter, setFilter] = useState<Filter>("all");
  const filterRef = useRef<Filter>("all"); // the filter as last rendered, read by the delete listener
  const [result, setResult] = useState<LibraryScan | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null); // the path just deleted
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [, setCoverVersion] = useState(0);

  const scanning = useRef(false); // scan admission
  const attempted = useRef(false); // a first scan was tried
  const generation = useRef(0); // bumped by every scan; older cover answers are dropped
  // The list as of the last commit(): updated at once, so a scan that settles and a delete that
  // follows before React renders both see (and build on) the same list.
  const resultRef = useRef<LibraryScan | null>(null);
  const commit = (next: LibraryScan) => {
    resultRef.current = next;
    setResult(next);
  };
  const goneDuring = useRef<string[] | null>(null); // deletions while a scan runs
  // ponytail: every cover of the scan is kept in memory (<= 512 KiB each) and loaded top to
  // bottom; viewport-first loading and an LRU budget if libraries get huge.
  const covers = useRef(new Map<string, string>());
  const tried = useRef(new Set<string>());
  const pumping = useRef(false);
  const wellRef = useRef<HTMLDivElement>(null);
  const refreshRef = useRef<HTMLButtonElement>(null);
  const focusAt = useRef<number | null>(null);
  const deleting = useRef(false);
  const [deletingPath, setDeletingPath] = useState<string | null>(null); // shown while a delete runs
  // Covers that failed (a 429 while the read cap is full) get a few more tries, a while later.
  const retry = useRef<{ timer: number | undefined; left: number }>({ timer: undefined, left: 5 });

  // The platform, once; a failed lookup is retried by Refresh.
  const lookup = async (): Promise<RuntimePlatform | null> => {
    try {
      const p = await runtimePlatform();
      setPlatform(p);
      return p;
    } catch (e) {
      setError(errorText(e));
      return null;
    }
  };
  useEffect(() => void lookup(), []);

  const visibleRef = useRef(props.visible);
  visibleRef.current = props.visible;

  const pump = async () => {
    if (pumping.current) return;
    pumping.current = true;
    const failed = new Set<string>();
    try {
      for (;;) {
        const mine = generation.current;
        // The kind chip's games first (each turn reads the chip as last rendered), then the rest.
        const wanted = (e: LibraryEntry) => e.has_cover && !tried.current.has(e.path) && !failed.has(e.path);
        const list = resultRef.current?.entries ?? [];
        const next = list.find((e) => wanted(e) && matches(e, filterRef.current)) ?? list.find(wanted);
        if (scanning.current || !visibleRef.current || !next) return;
        tried.current.add(next.path);
        let url: string | null = null;
        try {
          url = await api.libraryCover(next.path);
        } catch {
          // A failed request (a 429, say) is retried by the next pump, not by this one.
          tried.current.delete(next.path);
          failed.add(next.path);
        }
        if (url && mine === generation.current && resultRef.current?.entries.some((e) => e.path === next.path)) {
          covers.current.set(next.path, url);
          setCoverVersion((v) => v + 1);
        }
      }
    } finally {
      pumping.current = false;
      if (failed.size > 0 && retry.current.timer === undefined && retry.current.left > 0) {
        retry.current.left -= 1;
        retry.current.timer = window.setTimeout(() => {
          retry.current.timer = undefined;
          void pump();
        }, 3000);
      }
    }
  };

  // root: the folder to scan (a computer always has one); none: the PS5's roots. `fresh`: a newly
  // chosen folder, kept only if it is accepted.
  const scan = async (root: string | undefined = folderRef.current ?? undefined, fresh = false) => {
    if (scanning.current) return;
    scanning.current = true;
    attempted.current = true;
    goneDuring.current = [];
    window.clearTimeout(retry.current.timer);
    retry.current = { timer: undefined, left: 5 };
    // The well's panel buttons give way to the spinner: focus stays in the well, not on <body>.
    if (wellRef.current?.contains(document.activeElement)) wellRef.current.focus();
    setBusy(true);
    setTarget(root ?? null);
    setError(null);
    setNotice(null);
    setDeleteError(null);
    // A folder chosen in the File explorer while this scan ran wins: this one's outcome then
    // leaves the shared folder alone (the newer one is scanned next).
    const asked = shared.current;
    const superseded = () => shared.current !== asked && shared.current !== (root ?? null);
    try {
      const r = await api.libraryScan(root);
      const gone = goneDuring.current ?? [];
      // The old covers go only now, with the old list; a failed scan keeps both.
      generation.current += 1;
      covers.current = new Map();
      tried.current = new Set();
      commit({ ...r, entries: r.entries.filter((e) => !gone.some((g) => isWithin(e.path, g))) });
      if (fresh && !superseded()) setFolder(root ?? null);
      setRefused(false);
    } catch (e) {
      setError(errorText(e));
      if (fresh && !superseded()) {
        // The old folder and list no longer match what the user asked for.
        generation.current += 1;
        covers.current = new Map();
        tried.current = new Set();
        resultRef.current = null;
        setResult(null);
        setFolder(null);
        setRefused(true);
      }
    } finally {
      goneDuring.current = null;
      scanning.current = false;
      setBusy(false);
    }
  };

  const refresh = async () => {
    if (scanning.current) return;
    const p = platform ?? (await lookup());
    if (!p) return;
    setError(null);
    if (p === "host" && !folderRef.current) return;
    void scan();
  };

  const choose = async () => {
    if (scanning.current) return;
    const picked = await pick({ directory: true, title: t.pickTitle });
    if (!picked || scanning.current) return;
    void scan(picked, true);
  };

  useEffect(() => {
    if (props.visible && platform === "ps5" && !attempted.current) void scan();
  }, [props.visible, platform]);

  // A folder chosen in the File explorer: scanned as if chosen here (`busy`: once a scan that
  // ran meanwhile has ended).
  useEffect(() => {
    if (props.visible && platform === "host" && !busy && props.folder !== null && props.folder !== folderRef.current)
      void scan(props.folder, true);
  }, [props.visible, platform, busy, props.folder]);

  // Covers load once a list is there and no scan runs (the effect sees committed state).
  useEffect(() => {
    if (result && !busy && props.visible) void pump();
  }, [result, busy, props.visible]);

  // Deleted anywhere (here or Edit): drop what is gone.
  useEffect(
    () =>
      onDeleted((path) => {
        goneDuring.current?.push(path);
        const cur = resultRef.current;
        if (!cur) return;
        if (!cur.entries.some((e) => isWithin(e.path, path))) return;
        // The row's place among those shown, so focus lands on its neighbour.
        const i = Math.max(0, cur.entries.filter((e) => matches(e, filterRef.current)).findIndex((e) => isWithin(e.path, path)));
        if (wellRef.current?.contains(document.activeElement)) focusAt.current = i;
        for (const e of cur.entries) {
          if (isWithin(e.path, path)) {
            covers.current.delete(e.path);
            tried.current.add(e.path);
          }
        }
        commit({ ...cur, entries: cur.entries.filter((e) => !isWithin(e.path, path)) });
      }),
    [],
  );
  useEffect(() => {
    const i = focusAt.current;
    if (i === null || !result) return;
    focusAt.current = null;
    const items = wellRef.current?.querySelectorAll(".library-cards > li, .library-rows > li");
    const li = items?.[Math.min(i, items.length - 1)];
    (li?.querySelector("button") ?? refreshRef.current)?.focus();
  }, [result]);

  // Stable, so a cover arriving re-renders only its own Item.
  const remove = useCallback(async (entry: LibraryEntry) => {
    if (deleting.current) return;
    deleting.current = true;
    const path = entry.path;
    try {
      if (!(await confirmDelete(path, entry.kind === "folder"))) return;
      setNotice(null);
      setDeleteError(null);
      setDeletingPath(path);
      await api.deletePath(path);
      notifyDeleted(path);
      setNotice(path);
    } catch (e) {
      setNotice(null);
      setDeleteError(errorText(e));
    } finally {
      deleting.current = false;
      setDeletingPath(null);
    }
  }, []);

  const entries = result?.entries ?? [];
  const host = platform === "host";
  const storage = platform === "ps5" ? result?.storage : undefined; // the PS5's /data, as of the last scan
  const empty = host && !folder && !props.folder && !refused; // a computer with no folder chosen yet
  // The kind chips: All, then each kind present in a fixed order (any other after), then the
  // packs (LZ4, zstd) and Additional content; each with its count.
  const counts = new Map<Filter, number>();
  const add = (k: Filter) => counts.set(k, (counts.get(k) ?? 0) + 1);
  for (const e of entries) {
    add(e.kind);
    if (e.lz4_packed === true) add(LZ4);
    if (e.zstd_packed === true) add(ZSTD);
    if (e.additional_content === true) add(ADDON);
  }
  const kinds = [...KIND_ORDER.filter((k) => counts.has(k)), ...[...counts.keys()].filter((k) => !KIND_ORDER.includes(k) && k !== LZ4 && k !== ZSTD && k !== ADDON)];
  // A chosen kind that is gone (deleted, or not in a new scan) shows everything.
  const shown: Filter = filter === "all" || counts.has(filter) ? filter : "all";
  filterRef.current = shown;
  useEffect(() => {
    if (shown !== filter) setFilter(shown);
  }, [shown, filter]);
  const visibleEntries = shown === "all" ? entries : entries.filter((e) => matches(e, shown));
  // The well is at least one whole card row tall, plus the top of the second row when there is
  // one: a card's height follows its column's width (a square cover), so it is measured again
  // on a resize and whenever other cards may lead (another scan, filter or count).
  const cardsShown = props.visible && view === "cards" ? visibleEntries.length : 0;
  useLayoutEffect(() => {
    const well = wellRef.current;
    if (!well) return;
    const fit = () => {
      const cards = well.querySelectorAll<HTMLElement>(".library-cards > li");
      const first = cards[0];
      if (!first) return well.style.removeProperty("--fit");
      const second = Array.from(cards).some((c) => c.offsetTop > first.offsetTop);
      well.style.setProperty("--fit", `${first.offsetHeight + (second ? ROW_GAP + PEEK : 0)}px`);
    };
    fit();
    // The well's own width too (a scrollbar coming or going); old WebKit without ResizeObserver
    // follows the window only.
    const watch = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    watch?.observe(well);
    window.addEventListener("resize", fit);
    return () => {
      watch?.disconnect();
      window.removeEventListener("resize", fit);
    };
  }, [cardsShown, shown, result]);
  const chip = (id: Filter, cls: string, label: string, n: number) => (
    <button key={id} type="button" className={`kind-chip ${cls}`} aria-pressed={shown === id} onClick={() => setFilter(id)}>
      {label} <span className="n">{num(n)}</span>
    </button>
  );
  const listed = !busy && !refused && !empty && result !== null;
  const body = platform === null && !error ? (
    <div className="library-busy">
      <span className="spinner" aria-hidden="true" />
      <span>
        {loading}
        <Dots />
      </span>
    </div>
  ) : busy ? (
    <div className="library-busy" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <span>
        {target ? t.scanningIn(basename(target)) : t.scanning}
        <Dots />
      </span>
    </div>
  ) : refused ? (
    <div className="empty library-empty" role="alert">
      <span className="empty-icon bad">
        <Icon name="alert" />
      </span>
      <p className="empty-title">{t.cantScanFolder}</p>
      <p className="muted backend">{error}</p>
      <div className="row center">
        <button type="button" onClick={() => void choose()}>
          <Icon name="folder" />
          {t.chooseAnother}
        </button>
      </div>
    </div>
  ) : empty ? (
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
  ) : (
    <>
      {error && (
        <p className="library-msg" role="alert">
          <span className="tag orange">{t.cantScan}</span> <span className="backend">{error}</span>
        </p>
      )}
      {deleteError && (
        <p className="library-msg" role="alert">
          <span className="tag orange">{t.cantDelete}</span> <span className="backend">{deleteError}</span>
        </p>
      )}
      {notice && (
        <p className="library-msg muted" role="status">
          {t.deleted(notice)}
        </p>
      )}
      {result && result.truncated && (
        <p className="library-msg" role="status">
          <span className="tag orange">{t.truncated}</span> <span className="muted">{t.limits}</span>
        </p>
      )}
      {result && entries.length === 0 && (
        <div className="empty library-empty">
          <span className="empty-icon">
            <Icon name="folder" />
          </span>
          <p className="muted">{result.roots.length === 0 ? t.noRoots : t.none}</p>
          {host && (
            <div className="row center">
              <button type="button" onClick={() => void choose()}>
                <Icon name="folder" />
                {t.chooseAnother}
              </button>
            </div>
          )}
        </div>
      )}
      {visibleEntries.length > 0 && (
        <ul className={view === "cards" ? "library-cards" : "library-rows"}>
          {visibleEntries.map((e) => (
            <Item
              key={e.path}
              entry={e}
              cover={covers.current.get(e.path)}
              list={view === "list"}
              onEdit={props.onEdit}
              onConvert={props.onConvert}
              remove={remove}
              deleting={deletingPath === e.path}
              locked={deletingPath !== null}
            />
          ))}
        </ul>
      )}
      {result && result.warnings.length > 0 && (
        <details className="fold library-note">
          <summary>{t.skipped}</summary>
          <ul className="muted">
            {result.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </>
  );

  return (
    <section className="card library storage-card" aria-label={t.library}>
      <div className="storage-head">
        {props.switcher}
        <div className="storage-actions">
          <FreeSpace storage={storage} />
          {(platform !== null || error !== null) && !empty && !refused && (
            <button type="button" className="small" ref={refreshRef} aria-disabled={busy || undefined} onClick={() => void refresh()}>
              <Icon name="refresh" />
              {t.refresh}
            </button>
          )}
          {host && folder && (
            <button type="button" className="small" aria-disabled={busy || undefined} onClick={() => void choose()}>
              <Icon name="folder" />
              {t.choose}
            </button>
          )}
          {platform !== null && !empty && !refused && (
            <div className="library-views" role="group" aria-label={t.showAs}>
              <button type="button" className="small icon-only" aria-pressed={view === "cards"} aria-label={t.cards} title={t.cards} onClick={() => setView("cards")}>
                <Icon name="grid" />
              </button>
              <button type="button" className="small icon-only" aria-pressed={view === "list"} aria-label={t.list} title={t.list} onClick={() => setView("list")}>
                <Icon name="list" />
              </button>
            </div>
          )}
        </div>
      </div>
      {/* A computer's count names the chosen folder; the PS5 has none (the chips count). */}
      {listed && host && folder && (
        <div className="library-meta">
          <div className="library-count">{t.countIn(entries.length, <PathLine path={folder} />)}</div>
        </div>
      )}
      {listed && entries.length > 0 && (
        <div className="kind-chips" role="group" aria-label={t.kinds}>
          {chip("all", "all", t.all, entries.length)}
          {kinds.map((k) => chip(k, `fmt-${isArchive(k) ? "archive" : k}`, kindLabel(k), counts.get(k) ?? 0))}
          {counts.has(LZ4) && chip(LZ4, "lz4", t.lz4Chip, counts.get(LZ4) ?? 0)}
          {counts.has(ZSTD) && chip(ZSTD, "lz4", t.zstdChip, counts.get(ZSTD) ?? 0)}
          {counts.has(ADDON) && chip(ADDON, "addon", c.additionalContent, counts.get(ADDON) ?? 0)}
        </div>
      )}
      <div className="library-well" ref={wellRef} tabIndex={0} role="region" aria-label={t.games}>
        {body}
      </div>
    </section>
  );
}

const Item = memo(function Item(props: {
  entry: LibraryEntry;
  cover: string | undefined;
  list: boolean;
  onEdit: (e: LibraryEntry) => void;
  onConvert: (e: LibraryEntry) => void;
  remove: (e: LibraryEntry) => void;
  /** This entry is being deleted: its actions are off. */
  deleting: boolean;
  /** A delete runs: every trash button is off. */
  locked: boolean;
}) {
  const t = useMessages().library;
  const c = useMessages().common;
  const { entry: e } = props;
  const n = name(e);
  const cover = props.cover ? (
    <img className="cover library-cover" src={props.cover} alt="" />
  ) : (
    <div className="cover none library-cover" aria-hidden="true">
      <Icon name="disc" />
    </div>
  );
  const actions = (
    <span className="library-actions">
      <button type="button" className="small icon-only" disabled={props.deleting} aria-label={t.edit(n)} title={t.edit(n)} onClick={() => props.onEdit(e)}>
        <Icon name="pencil" />
      </button>
      <button type="button" className="small icon-only" disabled={props.deleting} aria-label={t.convert(n)} title={t.convert(n)} onClick={() => props.onConvert(e)}>
        <Icon name="convert" />
      </button>
      <button
        type="button"
        className="small icon-only danger"
        // The row being deleted keeps focus (aria-disabled), so it can move on to a neighbour.
        disabled={props.locked && !props.deleting}
        aria-disabled={props.deleting || undefined}
        aria-label={props.deleting ? t.deleting(n) : t.remove(n)}
        title={props.deleting ? t.deleting(n) : t.remove(n)}
        onClick={() => props.remove(e)}
      >
        <Icon name="trash" />
      </button>
    </span>
  );
  const packed = e.zstd_packed === true ? "zstd" : e.lz4_packed === true ? "lz4" : null;
  const ac = e.additional_content === true && <span className="tag violet">{c.additionalContent}</span>;
  const box = (
    <div className="library-cover-box">
      {cover}
      {packed && <PackBadge engine={packed} />}
    </div>
  );
  return props.list ? (
    <li className="library-row">
      {cover}
      <div className="library-text">
        <div className="library-name">{n}</div>
        <div className="library-sub muted">
          <span>{e.title_id}</span>
          {e.version && <span>{e.version}</span>}
          {packed && <PackBadge engine={packed} />}
          {ac}
        </div>
      </div>
      <div className="library-kind">
        <FormatPill kind={e.kind} />
      </div>
      <div className="library-path">
        <PathLine path={e.path} />
      </div>
      {actions}
    </li>
  ) : (
    <li className="library-card">
      {box}
      <div>
        <div className="library-name">{n}</div>
        <div className="library-sub muted">
          {e.title_id}
          {e.version && <span>{e.version}</span>}
        </div>
        {/* Copies of one game differ only here: the path shows, not just in a tooltip. */}
        <PathLine path={e.path} />
      </div>
      <div className="library-foot">
        <span className="library-tags">
          <FormatPill kind={e.kind} />
          {ac}
        </span>
        {actions}
      </div>
    </li>
  );
});

/** A packed dump's badge: "LZ4" or "zstd" in LZ4 yellow on a liquid-glass pill. */
function PackBadge({ engine }: { engine: "lz4" | "zstd" }) {
  const t = useMessages().library;
  const tip = engine === "zstd" ? t.zstdTip : t.lz4Tip;
  return (
    <span className="lz4-badge" title={tip}>
      <span aria-hidden="true">{engine === "zstd" ? "zstd" : "LZ4"}</span>
      <span className="visually-hidden">{tip}</span>
    </span>
  );
}
