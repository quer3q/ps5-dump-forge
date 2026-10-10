// Inspect: what a source holds (files, param.json, format details, preflight findings), and
// leftover .part files near it.

import { useEffect, useMemo, useRef, useState } from "react";

import { api, errorText, prettyBytes, type Inspection } from "./api";
import {
  CardHead,
  dirname,
  Modal,
  isWithin,
  notifyDeleted,
  openOverlay,
  panelId,
  SourceCard,
  tabId,
  TabList,
} from "./common";
import { Icon } from "./icons";

// ponytail: a cap instead of a virtualized list; the filter narrows big games.
const MAX_ROWS = 2000;

export function Inspect(props: {
  /** Folders this session wrote outputs to. */
  outputDirs: string[];
  /** The tab is showing: re-check for leftovers (jobs may have ended meanwhile). */
  visible: boolean;
}) {
  const [source, setSource] = useState<string | null>(null);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");
  const [view, setView] = useState<View>("files");
  const seq = useRef(0);
  const picked = useRef<string | null>(null); // `source`, for a delete that ends after a pick

  const pick = async (path: string) => {
    const mine = ++seq.current;
    picked.current = path;
    setSource(path);
    setIns(null);
    setError(null);
    setDeleted(null);
    setBusy(true);
    try {
      const found = await api.inspect(path);
      if (mine === seq.current) setIns(found);
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    } finally {
      if (mine === seq.current) setBusy(false);
    }
  };

  // Delete the inspected source, after asking. `deleting` also covers the open question, so a
  // second click can't ask twice; the path asked about is the one deleted.
  const [deleting, setDeleting] = useState(false);
  const deletingRef = useRef(false);
  const [deleted, setDeleted] = useState<string | null>(null);
  const remove = async () => {
    if (!source || !ins || deletingRef.current) return;
    const path = source;
    const mine = seq.current;
    deletingRef.current = true;
    try {
      if (!(await confirmDelete(path, ins.kind === "folder"))) return;
      if (mine !== seq.current) return; // another source meanwhile
      setDeleting(true);
      setError(null);
      await api.deletePath(path);
      notifyDeleted(path);
      // Also when the same source, or one inside it, was picked again meanwhile.
      if (mine === seq.current || (picked.current !== null && isWithin(picked.current, path))) {
        seq.current++; // a late answer for this path never lands
        picked.current = null;
        setSource(null);
        setIns(null);
        setFilter("");
        setBusy(false);
        setDeleted(path);
        document.getElementById("i-source")?.focus();
      }
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    } finally {
      deletingRef.current = false;
      setDeleting(false);
    }
  };

  const files = useMemo(() => {
    if (!ins) return [];
    const f = filter.trim().toLowerCase();
    return f ? ins.files.filter((x) => x.path.toLowerCase().includes(f)) : ins.files;
  }, [ins, filter]);

  const tabs = [
    { id: "files" as const, label: <>Files <span className="count">{ins?.files.length.toLocaleString()}</span></> },
    { id: "param" as const, label: "param.json" },
    { id: "details" as const, label: "Details" },
  ];

  return (
    <div className="screen">
      <div className={ins ? "cols inspect" : "cols one"}>
        <div className="stack">
          <SourceCard
            path={source}
            onPick={pick}
            busy={busy}
            error={error}
            ins={ins}
            full
            actions={
              ins && (
                <button
                  className="small danger icon-only"
                  onClick={remove}
                  disabled={deleting}
                  aria-label={deleting ? "Deleting the source…" : "Delete the source"}
                  title="Delete the source"
                >
                  <Icon name="trash" />
                </button>
              )
            }
          />
          {deleted && (
            <p className="muted deleted-note" role="status">
              Deleted <span className="path">{deleted}</span>
            </p>
          )}
          <Leftovers dirs={[source && dirname(source), ...props.outputDirs]} visible={props.visible} />
        </div>

        {ins && (
          <section className="card contents" aria-label="Source contents">
            <TabList
              label="Source contents"
              prefix="ins"
              className="sub"
              tabs={tabs}
              value={view}
              onChange={setView}
            />

            <div {...panel("files", view)} className="panel">
              <div className="row nowrap filter">
                <input
                  type="search"
                  className="grow"
                  placeholder="Filter by path"
                  aria-label="Filter files by path"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  spellCheck={false}
                />
                <span className="muted count-text">
                  {files.length > MAX_ROWS
                    ? `first ${MAX_ROWS.toLocaleString()} of ${files.length.toLocaleString()}`
                    : `${files.length.toLocaleString()} file${files.length === 1 ? "" : "s"}`}
                </span>
              </div>
              <div className="panel-body" tabIndex={0}>
                <table className="files">
                  <tbody>
                    {files.slice(0, MAX_ROWS).map((f) => (
                      <tr key={f.path}>
                        <td className="path">{f.path}</td>
                        <td className="size">{prettyBytes(f.size)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {files.length === 0 && (
                  <p className="muted empty-note">No files match “{filter.trim()}”.</p>
                )}
                {ins.empty_dirs.length > 0 && (
                  <>
                    <h3>Empty folders ({ins.empty_dirs.length.toLocaleString()})</h3>
                    <ul className="rows path">
                      {ins.empty_dirs.slice(0, MAX_ROWS).map((d) => (
                        <li key={d}>{d}</li>
                      ))}
                    </ul>
                  </>
                )}
              </div>
            </div>

            <div {...panel("param", view)} className="panel">
              <div className="panel-body" tabIndex={0}>
                {ins.param_json == null ? (
                  <p className="muted">No sce_sys/param.json.</p>
                ) : (
                  <pre className="code">{JSON.stringify(ins.param_json, null, 2)}</pre>
                )}
              </div>
            </div>

            <div {...panel("details", view)} className="panel">
              <div className="panel-body" tabIndex={0}>
                <ul className="rows">
                  <li>
                    <span className="muted">Reader</span> {ins.describe}
                  </li>
                  {ins.details.map((d, i) => (
                    <li key={i}>{d}</li>
                  ))}
                </ul>
              </div>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

type View = "files" | "param" | "details";

/** A sub-view's tabpanel attributes; hidden ones keep their state but leave the tab order. */
function panel(id: View, view: View) {
  return {
    id: panelId("ins", id),
    role: "tabpanel",
    "aria-labelledby": tabId("ins", id),
    hidden: id !== view,
  };
}

/**
 * Leftover `.part` files where outputs land (the source's folder, this session's output
 * folders). Listed only, never deleted. Re-checked whenever the tab shows (jobs may have
 * ended meanwhile). The card shows only when some were found: nothing while checking, when
 * there are none or nothing to check, or when the check fails.
 */
function Leftovers(props: { dirs: (string | null)[]; visible: boolean }) {
  const dirs = [...new Set(props.dirs.filter((d): d is string => !!d))];
  const key = JSON.stringify(dirs);
  const [parts, setParts] = useState<string[]>([]);

  useEffect(() => {
    if (!props.visible) return;
    setParts([]);
    const list: string[] = JSON.parse(key);
    if (list.length === 0) return;
    let live = true;
    api.staleParts(list).then(
      (p) => live && setParts(p),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [key, props.visible]);

  if (parts.length === 0) return null;
  return (
    <section className="card compact" aria-labelledby="i-leftovers">
      <CardHead icon="file" title="Leftover .part files" id="i-leftovers">
        <span className="tag orange">{parts.length} found</span>
      </CardHead>
      <ul className="rows path">
        {parts.map((p) => (
          <li key={p}>{p}</li>
        ))}
      </ul>
      <p className="muted">
        Unfinished outputs, usually from interrupted jobs. Remove only the ones you recognize,
        and only when no job is running; PS5 Dump Forge never deletes them itself.
      </p>
      <details className="fold">
        <summary>Folders checked ({dirs.length})</summary>
        <ul className="rows path">
          {dirs.map((d) => (
            <li key={d}>{d}</li>
          ))}
        </ul>
      </details>
    </section>
  );
}

/** Asks before deleting `path` for good; true: Yes. Two buttons, No first and focused. */
function confirmDelete(path: string, folder: boolean): Promise<boolean> {
  return new Promise((resolve) => {
    let yes = false;
    openOverlay(
      (close) => (
        <Modal
          close={close}
          className="modal card"
          role="alertdialog"
          labelledBy="del-title"
          describedBy="del-text"
        >
          <div className="modal-head">
            <span className="head-icon warn">
              <Icon name="trash" />
            </span>
            <h2 id="del-title">Delete this {folder ? "folder" : "image"}?</h2>
          </div>
          <div id="del-text">
            <p className="out-box path">{path}</p>
            <p className="muted">
              {folder ? "The folder and everything in it" : "The image file"} will be deleted for
              good: no Trash, no undo. If deleting stops part way, some of it may already be gone.
            </p>
          </div>
          <div className="row end">
            <button autoFocus onClick={close}>
              No
            </button>
            <button
              className="danger"
              onClick={() => {
                yes = true;
                close();
              }}
            >
              Yes, delete
            </button>
          </div>
        </Modal>
      ),
      () => resolve(yes),
    );
  });
}
