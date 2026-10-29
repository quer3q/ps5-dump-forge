// The Storage tab's Package installer view (the PS5 only): every `.pkg` under the server's roots,
// scanned on the first visit and on Refresh (core's `scan_packages`; nothing kept), each row with
// Install, which asks (InstallDialog.tsx) and starts an install job listed under Storage.

import { useEffect, useRef, useState, type ReactNode } from "react";

import { api, errorText, prettyBytes, type PackageContent, type PackageEntry, type PackageScan } from "./api";
import { basename, Dots, notifyDeleted, PathLine } from "./common";
import { confirmDelete } from "./delete";
import { gameTitle, useMessages } from "./i18n";
import { Icon } from "./icons";
import { installHere } from "./InstallDialog";
import type { Action } from "./jobs";
import { FreeSpace } from "./Library";

const name = (e: PackageEntry) => gameTitle(e.title_names, e.title_name) || e.title_id || basename(e.path);

const TONE: Record<PackageContent, string> = { game: "", patch: " orange", additional_content: " violet", unknown: "" };

export function Packages(props: { visible: boolean; dispatch: (a: Action) => void; switcher?: ReactNode }) {
  const t = useMessages().install;
  const lib = useMessages().library;
  const [result, setResult] = useState<PackageScan | null>(null);
  const [storage, setStorage] = useState<{ free: number; total: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [asking, setAsking] = useState<string | null>(null); // the row whose question is open
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null); // the row being deleted
  const [view, setView] = useState<"cards" | "list">("list");
  const scanning = useRef(false);
  const attempted = useRef(false);
  const wellRef = useRef<HTMLDivElement>(null);

  const scan = async () => {
    if (scanning.current) return;
    scanning.current = true;
    attempted.current = true;
    // The rows give way to the spinner: focus stays in the well, not on <body>.
    if (wellRef.current?.contains(document.activeElement)) wellRef.current.focus();
    setBusy(true);
    setError(null);
    setNotice(null);
    setInstallError(null);
    try {
      const [found, free] = await Promise.all([api.packageScan(), api.storage().catch(() => null)]);
      setResult(found);
      setStorage(free);
    } catch (e) {
      setError(errorText(e));
    } finally {
      scanning.current = false;
      setBusy(false);
    }
  };
  useEffect(() => {
    if (props.visible && !attempted.current) void scan();
  }, [props.visible]);

  const install = async (e: PackageEntry) => {
    if (asking !== null) return;
    setAsking(e.path);
    setInstallError(null);
    setNotice(null);
    try {
      if (await installHere(e, props.dispatch)) setNotice(t.started(name(e)));
    } catch (err) {
      setInstallError(errorText(err));
    } finally {
      setAsking(null);
    }
  };

  // A package deleted here leaves the list at once (no rescan); its question is delete.tsx's.
  const remove = async (e: PackageEntry) => {
    if (asking !== null || deleting !== null) return;
    setAsking(e.path);
    setInstallError(null);
    setDeleteError(null);
    setNotice(null);
    try {
      if (!(await confirmDelete(e.path, false, "file"))) return;
      setDeleting(e.path);
      await api.deletePath(e.path);
      notifyDeleted(e.path);
      setResult((r) => r && { ...r, entries: r.entries.filter((x) => x.path !== e.path) });
      setNotice(lib.deleted(e.path));
    } catch (err) {
      setDeleteError(errorText(err));
    } finally {
      setDeleting(null);
      setAsking(null);
    }
  };

  const entries = result?.entries ?? [];
  const body = busy ? (
    <div className="library-busy" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <span>
        {t.scanning}
        <Dots />
      </span>
    </div>
  ) : (
    <>
      {error && (
        <p className="library-msg" role="alert">
          <span className="tag orange">{t.cantScan}</span> <span className="backend">{error}</span>
        </p>
      )}
      {installError && (
        <p className="library-msg" role="alert">
          <span className="tag orange">{t.cantInstall}</span> <span className="backend">{installError}</span>
        </p>
      )}
      {deleteError && (
        <p className="library-msg" role="alert">
          <span className="tag orange">{lib.cantDelete}</span> <span className="backend">{deleteError}</span>
        </p>
      )}
      {notice && (
        <p className="library-msg muted" role="status">
          {notice}
        </p>
      )}
      {result?.truncated && (
        <p className="library-msg" role="status">
          <span className="tag orange">{t.truncated}</span> <span className="muted">{t.limits}</span>
        </p>
      )}
      {result && entries.length === 0 && (
        <div className="empty library-empty">
          <span className="empty-icon">
            <Icon name="package" />
          </span>
          <p className="empty-title">{t.none}</p>
          <p className="muted">{t.noneHint}</p>
        </div>
      )}
      {entries.length > 0 && (
        <ul className={view === "cards" ? "library-cards" : "library-rows"}>
          {entries.map((e) => (
            <Row
              key={e.path}
              entry={e}
              list={view === "list"}
              locked={asking !== null}
              deleting={deleting === e.path}
              onInstall={() => void install(e)}
              onRemove={() => void remove(e)}
            />
          ))}
        </ul>
      )}
    </>
  );

  return (
    <section className="card library storage-card" aria-label={t.view}>
      <div className="storage-head">
        {props.switcher}
        <div className="storage-actions">
          <FreeSpace storage={storage} />
          <button type="button" className="small" aria-disabled={busy || undefined} onClick={() => void scan()}>
            <Icon name="refresh" />
            {t.refresh}
          </button>
          {entries.length > 0 && (
            <div className="library-views" role="group" aria-label={lib.showAs}>
              <button type="button" className="small icon-only" aria-pressed={view === "cards"} aria-label={lib.cards} title={lib.cards} onClick={() => setView("cards")}>
                <Icon name="grid" />
              </button>
              <button type="button" className="small icon-only" aria-pressed={view === "list"} aria-label={lib.list} title={lib.list} onClick={() => setView("list")}>
                <Icon name="list" />
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="library-well" ref={wellRef} tabIndex={0} role="region" aria-label={t.packages}>
        {body}
      </div>
    </section>
  );
}

function Row(props: {
  entry: PackageEntry;
  list: boolean;
  locked: boolean;
  deleting: boolean;
  onInstall: () => void;
  onRemove: () => void;
}) {
  const t = useMessages().install;
  const lib = useMessages().library;
  const e = props.entry;
  const n = name(e);
  const cover = (
    <div className="cover none library-cover" aria-hidden="true">
      <Icon name="package" />
    </div>
  );
  const sub = (
    <div className="library-sub muted">
      {e.title_id && <span>{e.title_id}</span>}
      {e.version && <span>{e.version}</span>}
      <bdi>{prettyBytes(e.size)}</bdi>
    </div>
  );
  const kind = <span className={`tag${TONE[e.content]}`}>{t.content[e.content]}</span>;
  const actions = (
    <span className="library-actions">
      <button type="button" className="small" disabled={props.locked} aria-label={t.installName(n)} title={t.installName(n)} onClick={props.onInstall}>
        <Icon name="download" />
        {t.install}
      </button>
      <button
        type="button"
        className="small icon-only danger"
        // The row being deleted keeps focus (aria-disabled), as the Library's does.
        disabled={props.locked && !props.deleting}
        aria-disabled={props.deleting || undefined}
        aria-label={props.deleting ? lib.deleting(n) : lib.remove(n)}
        title={props.deleting ? lib.deleting(n) : lib.remove(n)}
        onClick={props.onRemove}
      >
        <Icon name="trash" />
      </button>
    </span>
  );
  return props.list ? (
    <li className="library-row">
      {cover}
      <div className="library-text">
        <div className="library-name">{n}</div>
        {sub}
      </div>
      <div className="library-kind">{kind}</div>
      <div className="library-path">
        <PathLine path={e.path} />
      </div>
      {actions}
    </li>
  ) : (
    <li className="library-card">
      <div className="library-cover-box">{cover}</div>
      <div>
        <div className="library-name">{n}</div>
        {sub}
        <PathLine path={e.path} />
      </div>
      <div className="library-foot">
        <span className="library-tags">{kind}</span>
        {actions}
      </div>
    </li>
  );
}
