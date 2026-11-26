// Edit: a game's card and an explorer whose changes stack up until Apply.

import { useEffect, useId, useMemo, useReducer, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import {
  api,
  APP_NAME,
  errorText,
  onDrop,
  pick as pickPath,
  pickFiles,
  prettyBytes,
  type ConvertRequest,
  type Inspection,
  type JobId,
} from "./api";
import {
  CardHead,
  dirname,
  Dots,
  InspectSummary,
  isArchive,
  isMac,
  isNetwork,
  isPeer,
  isNas,
  isWithin,
  notifyDeleted,
  onDeleted,
  type SourceSelection,
  SourceCard,
} from "./common";
import { confirmDelete } from "./delete";
import { Toggle } from "./build/fields";
import { askReplace } from "./replace";
import {
  chosenMode,
  confirmApply,
  editRequest,
  hostPath,
  Lines,
  ModeBox,
  UploadLine,
  uploaded,
  useEditCheck,
  type Mode,
} from "./EditApply";
import { askConflicts, confirmDiscard, dropped, fromHost, PendingBar, type AddSource, type Incoming, type NewChange } from "./EditChanges";
import {
  findDir,
  focusIndex,
  fold,
  join,
  pendingReducer,
  planAdds,
  type PendingAction,
  type PendingState,
  listDir,
  parentOf,
  search,
  split,
  stagingNames,
  toEdits,
  validName,
  type DirNode,
  type Entry,
  type Mark,
  type TreeNode,
} from "./explorer";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import type { Action, Job } from "./jobs";
import { inspectSource } from "./password";

// ponytail: a cap instead of a virtualized list; the filter narrows big folders.
export const MAX_ROWS = 2000;

/** Staging folders of edit jobs started here: they outlive the page reset, discarded when the job ends. */
const stagedJobs = new Map<JobId, string>();

export function Edit(props: {
  outputDirs: string[];
  /** Re-check for leftovers when shown (jobs may have ended meanwhile). */
  visible: boolean;
  /** A game the File explorer opened here; each new `id` is read once. */
  selection?: SourceSelection | null;
  jobs: Job[];
  dispatch: (a: Action) => void;
}) {
  const messages = useMessages();
  const t = messages.edit;
  const [source, setSource] = useState<string | null>(null);
  const [ins, setIns] = useState<Inspection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const seq = useRef(0);
  const picked = useRef<string | null>(null); // `source`, for a delete that ends after a pick
  const pw = useRef<string | null>(null); // the archive password the source opened with, for jobs

  /** The changes not applied yet; a new source starts a new generation, and an older one's action queues nothing. */
  const [pending, dispatch] = useReducer(
    pendingReducer as (s: PendingState<NewChange>, a: PendingAction<NewChange>) => PendingState<NewChange>,
    { gen: 0, next: 0, changes: [] },
  );
  const changes = pending.changes;
  const live = useRef(pending);
  live.current = pending;

  /** Apply: the mode asked for, core's check (asked again on `nonce`), the Apply in progress and its uploads. */
  const [want, setWant] = useState<Mode>("in_place");
  const [nonce, setNonce] = useState(0);
  const [applying, setApplying] = useState(false);
  const [fullCheck, setFullCheck] = useState(false);
  const applyingRef = useRef(false);
  const [upload, setUpload] = useState<{ i: number; n: number; sent: number; total: number } | null>(null);
  const abort = useRef<AbortController | null>(null);
  const [applyError, setApplyError] = useState<{ lead: string; text?: string; lines?: string[] } | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const rereading = useRef(false);
  /** While an Apply runs the source stays: a pick or delete is refused, a Storage hand-off dropped with a note. */
  const holding = useRef(false);
  const [holdNote, setHoldNote] = useState<{ path: string | null } | null>(null);

  const pick = async (path: string) => {
    if (holding.current) {
      setHoldNote({ path: null });
      return;
    }
    setHoldNote(null);
    const mine = ++seq.current;
    picked.current = path;
    setSource(path);
    setIns(null);
    dispatch({ type: "reset" });
    setError(null);
    setDeleteError(null);
    setDeleted(null);
    setWant("in_place");
    setApplyError(null);
    setRefreshing(false);
    rereading.current = false;
    setBusy(true);
    try {
      const { ins: found, password } = await inspectSource(path, null, () => mine === seq.current);
      if (mine === seq.current) {
        setIns(found);
        pw.current = password;
      }
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    } finally {
      if (mine === seq.current) setBusy(false);
    }
  };

  // ponytail: notifications compare path spellings; a source picked through a symlink alias
  // may need Refresh. Storage handoffs keep the entry's path exactly.
  const visibleRef = useRef(props.visible);
  visibleRef.current = props.visible;
  const reset = () => {
    seq.current++; // a late answer for the old source never lands
    picked.current = null;
    setSource(null);
    setIns(null);
    dispatch({ type: "reset" });
    setBusy(false);
    setError(null);
    setRefreshing(false);
    rereading.current = false;
    setWant("in_place");
    setApplyError(null);
    setDeleteError(null);
    setDeleted(null);
    setHoldNote(null);
  };
  const clear = (path: string) => {
    abort.current?.abort(); // an Apply's uploads stop: their source is gone
    reset();
    setDeleted(path);
    if (visibleRef.current) document.getElementById("e-source")?.focus();
  };
  const clearRef = useRef(clear);
  clearRef.current = clear;
  // The File explorer deleted this source or its folder (Edit's own delete clears first).
  useEffect(
    () =>
      onDeleted((gone) => {
        if (picked.current !== null && isWithin(picked.current, gone)) clearRef.current(gone);
      }),
    [],
  );

  const pickRef = useRef(pick);
  pickRef.current = pick;
  const consumed = useRef(0);
  useEffect(() => {
    const sel = props.selection;
    if (!sel || sel.id === consumed.current) return;
    consumed.current = sel.id;
    if (holding.current) setHoldNote({ path: sel.path });
    else void pickRef.current(sel.path);
  }, [props.selection]);

  // `deleting` also covers the open question, so a second click can't ask twice.
  const [deleting, setDeleting] = useState(false);
  const deletingRef = useRef(false);
  const [deleted, setDeleted] = useState<string | null>(null);
  const remove = async () => {
    if (!source || !ins || deletingRef.current || holding.current) return;
    const path = source;
    const mine = seq.current;
    deletingRef.current = true;
    try {
      if (!(await confirmDelete(path, ins.kind === "folder", isArchive(ins.kind) ? "archive" : "image"))) return;
      if (mine !== seq.current) return; // another source meanwhile
      setDeleting(true);
      setDeleteError(null);
      await api.deletePath(path);
      if (mine === seq.current || (picked.current !== null && isWithin(picked.current, path))) clear(path);
      notifyDeleted(path);
    } catch (e) {
      if (mine === seq.current) setDeleteError(errorText(e));
    } finally {
      deletingRef.current = false;
      setDeleting(false);
    }
  };

  // Restore metadata online: sent with the changes or alone; the job fails, nothing changed, when it can't download.
  const [restoreMeta, setRestoreMeta] = useState(false);
  const metaOk = !!ins && !isArchive(ins.kind) && !ins.patch && !isNetwork(source);
  const meta = restoreMeta && metaOk;

  const folded = useMemo(() => (ins ? fold(ins.files, ins.empty_dirs, changes) : null), [ins, changes]);
  const onChanges = (gen: number, list: NewChange[]) => dispatch({ type: "add", gen, list });
  // Add `items` into `dir` as one batch; every clash is asked about once: Replace / Skip / Cancel.
  const onAdd = async (gen: number, dir: string, items: Incoming[], shown: string) => {
    if (!ins || gen !== live.current.gen || items.length === 0) return;
    // An add read while Apply began, or during a re-read, is refused rather than lost.
    if (holding.current || rereading.current) throw new Error(t.applyRunning);
    const plan = planAdds(ins.files, ins.empty_dirs, live.current.changes, dir, items);
    const answer = plan.clashes.length > 0 ? await askConflicts(plan.clashes, shown) : "replace";
    if (answer === "cancel" || gen !== live.current.gen) return;
    if (holding.current || rereading.current) throw new Error(t.applyRunning);
    // Planned again on the list as it is now (the question may have taken a while).
    dispatch({ type: "add", gen, list: planAdds(ins.files, ins.empty_dirs, live.current.changes, dir, items).changes(answer) });
  };
  const discard = async () => {
    const gen = live.current.gen;
    if (changes.length > 1 && !(await confirmDiscard(changes.length))) return;
    if (gen !== live.current.gen) return;
    dispatch({ type: "discard" });
    setApplyError(null);
  };

  // An edit job of this source not finished yet (a reload's, another viewer's): the changes wait.
  const running = props.jobs.some((j) => !j.result && changesSource(j) && j.request!.source === source);
  const locked = running || applying || refreshing;
  holding.current = applying || applyingRef.current;
  const check = useEditCheck(source, ins, changes, nonce, !locked);
  const kind = ins?.kind ?? "folder";
  const mode = chosenMode(check.state, check.current, kind, want);

  const reread = async (path: string) => {
    const mine = ++seq.current;
    setRefreshing(true);
    rereading.current = true;
    try {
      const { ins: found } = await inspectSource(path, pw.current, () => mine === seq.current);
      if (mine !== seq.current) return;
      setIns(found);
      setError(null);
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    } finally {
      if (mine === seq.current) {
        setRefreshing(false);
        rereading.current = false;
      }
    }
  };

  const extract = async (paths: string[]) => {
    if (!source || !ins || paths.length === 0) return;
    const path = source;
    const output = await pickPath({ directory: true, title: t.pickExtract, devices: true });
    if (output === null || picked.current !== path) return;
    const request = await askReplace(extractRequest(path, output, paths, pw.current), () => picked.current === path);
    if (!request) return;
    const id = await api.startJob(request);
    props.dispatch({ type: "started", id, request });
  };

  // Apply: ask, upload (web page), check again, start the job; a failure before the job keeps the changes.
  const apply = async () => {
    if (!source || !ins || mode === null || locked || applyingRef.current) return;
    const path = source;
    const base = ins;
    const gen = live.current.gen;
    const list = live.current.changes;
    const folder = base.kind === "folder";
    applyingRef.current = true;
    setApplying(true);
    setApplyError(null);
    setHoldNote(null);
    let staged = false;
    try {
      const n = list.length;
      const restores = !!base.interrupted_edit;
      // With the metadata switch a .ffpfs is rebuilt (it adds no file in place; core's rule).
      const shown = meta && base.kind === "ffpfs" ? "rebuild" : mode;
      if (!(await confirmApply({ n, mode: shown, folder, name: path, restores, metadata: meta })) || gen !== live.current.gen) return;
      const ctl = new AbortController();
      abort.current = ctl;
      const ups = uploadsOf(list);
      const stageAt = stagingNames();
      const handles = new WeakMap<object, Map<string, File>>();
      let i = 0;
      let sent = 0;
      const edits = await toEdits(base.files, base.empty_dirs, list, uploaded, async (src: AddSource, rel, to) => {
        if ("path" in src) return hostPath(src, rel);
        let file: File | undefined;
        if ("file" in src) file = src.file;
        else {
          let m = handles.get(src.folder);
          if (!m) handles.set(src.folder, (m = new Map(src.folder.files.map((f) => [f.path, f.file]))));
          file = m.get(rel ?? "");
        }
        if (!file) throw new Error(t.droppedGone(to));
        const at = stageAt(to);
        staged = true;
        const done = sent;
        setUpload({ i: ++i, n: ups.files, sent: done, total: ups.bytes });
        const got = await api.stage(path, at, file, (s) => setUpload({ i, n: ups.files, sent: done + s, total: ups.bytes }), ctl.signal);
        sent += file.size;
        return got;
      });
      setUpload(null);
      // An empty list would start a conversion (with the metadata switch on the edit job still runs).
      if (edits.length === 0 && !meta) {
        setApplyError({ lead: t.nothingToChange });
        return;
      }
      const request = withMetadata(editRequest(path, base.kind, edits, mode === "rebuild"), meta);
      if (staged) {
        setFullCheck(true);
        const full = await api.editCheck(request);
        if (gen !== live.current.gen) throw new Error("cancelled");
        check.settle(full);
        const why = folder || mode === "in_place" ? full.in_place : full.rebuild;
        if (why.length > 0) {
          setApplyError({ lead: t.applyRefused, lines: why });
          void api.discardStaged(path).catch(() => {});
          staged = false;
          return;
        }
      }
      const id = await api.startJob(request);
      if (staged) stagedJobs.set(id, path); // the job's now; discarded once it ends
      staged = false;
      props.dispatch({ type: "started", id, request });
      reset();
      if (visibleRef.current) document.getElementById("e-source")?.focus();
    } catch (e) {
      const text = errorText(e);
      if (gen === live.current.gen)
        setApplyError(text === "aborted" ? { lead: t.uploadCancelled } : { lead: t.applyFailed, text });
    } finally {
      if (staged) void api.discardStaged(path).catch(() => {});
      abort.current = null;
      setUpload(null);
      setFullCheck(false);
      applyingRef.current = false;
      setApplying(false);
    }
  };

  // The metadata switch alone: Sony's metadata applied as an edit in the format's own way (core decides).
  const applyMeta = async () => {
    if (!source || !ins || locked || applyingRef.current) return;
    const path = source;
    const base = ins;
    const gen = live.current.gen;
    const inPlace = base.kind === "folder" || base.kind === "exfat" || base.kind === "ffpkg";
    applyingRef.current = true;
    setApplying(true);
    setApplyError(null);
    try {
      const mode = inPlace ? "in_place" : "rebuild";
      const restores = !!base.interrupted_edit;
      const asked = { n: 0, mode, folder: base.kind === "folder", name: path, restores, metadata: true } as const;
      if (!(await confirmApply(asked)) || gen !== live.current.gen) return;
      const request = withMetadata(editRequest(path, base.kind, [], !inPlace), true);
      const id = await api.startJob(request);
      props.dispatch({ type: "started", id, request });
      reset();
      if (visibleRef.current) document.getElementById("e-source")?.focus();
    } catch (e) {
      if (gen === live.current.gen) setApplyError({ lead: t.applyFailed, text: errorText(e) });
    } finally {
      applyingRef.current = false;
      setApplying(false);
    }
  };

  useEffect(() => {
    for (const [id, path] of stagedJobs) {
      if (!props.jobs.find((x) => x.id === id)?.result) continue;
      stagedJobs.delete(id);
      void api.discardStaged(path).catch(() => {});
    }
  }, [props.jobs]);

  // An unfinished edit job of the source open now: when it ends the source is read again, the changes kept.
  const others = useRef(new Map<JobId, string>());
  useEffect(() => {
    for (const j of props.jobs) {
      if (!changesSource(j)) continue;
      if (!j.result) {
        others.current.set(j.id, j.request!.source);
      } else if (others.current.has(j.id)) {
        const path = others.current.get(j.id)!;
        others.current.delete(j.id);
        if (picked.current === path) void reread(path);
      }
    }
  }, [props.jobs]);

  return (
    <div className="screen">
      <div className="cols edit">
        <div className="stack">
          <SourceCard
            path={source}
            onPick={pick}
            locked={applying}
            busy={busy}
            error={error}
            ins={ins}
            stacked
            prefix="e"
            onUnlock={() => source && void pick(source)}
            actions={
              ins &&
              !isNetwork(source) && (
                <button
                  className="small danger icon-only"
                  onClick={remove}
                  // While deleting it keeps focus (aria-disabled); remove() ignores clicks then.
                  disabled={running || applying}
                  aria-disabled={deleting || undefined}
                  aria-label={deleting ? t.deleting : t.delete}
                  title={deleting ? t.deleting : t.delete}
                >
                  <Icon name="trash" />
                </button>
              )
            }
          >
            {ins?.interrupted_edit && (
              <p className="note-line warn interrupted-note">
                <Icon name="warn" />
                <span>{t.interrupted}</span>
              </p>
            )}
            {ins && <GameFolds ins={ins} />}
            {metaOk && (
              <Toggle
                label={messages.metadata.label}
                checked={restoreMeta}
                disabled={locked}
                onChange={setRestoreMeta}
                tip={messages.metadata.editHint}
              />
            )}
          </SourceCard>
          {holdNote && (running || applying) && (
            <p className="note-line warn source-note" role="status">
              <Icon name="warn" />
              <span>{holdNote.path ? t.notOpened(<span className="path">{holdNote.path}</span>) : t.sourceLocked}</span>
            </p>
          )}
          {deleteError && (
            <p className="source-note" role="alert">
              <span className="tag orange">{messages.library.cantDelete}</span> <span className="backend">{deleteError}</span>
            </p>
          )}
          {deleted && (
            <p className="muted source-note" role="status">
              {t.deleted} <span className="path">{deleted}</span>
            </p>
          )}
          <Leftovers dirs={[source && !isNetwork(source) ? dirname(source) : null, ...props.outputDirs]} visible={props.visible} />
        </div>

        {/* A new source starts at its root with no filter (the key); a re-read keeps the folder. */}
        {ins && folded ? (
          <Explorer
            key={source ?? ""}
            ins={ins}
            tree={folded.tree}
            editable={!ins.archive && !ins.patch && !isNetwork(source) && !locked}
            nas={isNas(source)}
            peer={isPeer(source)}
            visible={props.visible}
            gen={pending.gen}
            onChanges={onChanges}
            onAdd={onAdd}
            onExtract={locked ? undefined : extract}
            extractNote={changes.length > 0}
            pending={
              changes.length === 0 ? (
                meta && (
                  <section className="pending-bar" aria-labelledby="e-meta">
                    <div className="row pending-head">
                      <Icon name="download" />
                      <b id="e-meta" className="grow">
                        {messages.metadata.editOnly}
                      </b>
                      <button
                        className="small primary"
                        onClick={() => void applyMeta()}
                        aria-disabled={applying || running || undefined}
                        disabled={refreshing}
                        title={messages.metadata.editHint}
                      >
                        <Icon name="check" />
                        {t.apply}
                      </button>
                    </div>
                    {running && (
                      <p className="note-line" role="status">
                        <Icon name="info" />
                        <span>{t.applyRunning}</span>
                      </p>
                    )}
                    {applyError && (
                      <div className="error-box" role="alert">
                        <strong className="error-title">{applyError.lead}</strong>
                        {applyError.text && <span className="backend">{applyError.text}</span>}
                      </div>
                    )}
                  </section>
                )
              ) : (
                <PendingBar
                  changes={changes}
                  findings={folded.findings}
                  onUndo={() => {
                    dispatch({ type: "undo" });
                    setApplyError(null);
                  }}
                  onDiscard={() => void discard()}
                  disabled={locked}
                  actions={
                    <button
                      className="small primary"
                      onClick={() => void apply()}
                      disabled={refreshing || mode === null || folded.findings.some((f) => f.length > 0)}
                      aria-disabled={applying || running || undefined}
                      title={t.applyTip}
                    >
                      <Icon name="check" />
                      {t.apply}
                    </button>
                  }
                >
                  {running || refreshing ? (
                    <p className="note-line" role="status">
                      <Icon name="info" />
                      <span>{running ? t.applyRunning : messages.common.reading}</span>
                    </p>
                  ) : (
                    <ModeBox
                      kind={kind}
                      state={check.state}
                      current={check.current && !fullCheck}
                      want={want}
                      onWant={setWant}
                      onRetry={() => setNonce((n) => n + 1)}
                      disabled={locked}
                      size={ins.total_bytes}
                    />
                  )}
                  {meta && (
                    <p className="note-line" role="status">
                      <Icon name="download" />
                      <span>{messages.metadata.editAlso}</span>
                    </p>
                  )}
                  {upload && <UploadLine {...upload} onCancel={() => abort.current?.abort()} />}
                  {applyError && (
                    <div className="error-box" role="alert">
                      <strong className="error-title">{applyError.lead}</strong>
                      {applyError.text && <span className="backend">{applyError.text}</span>}
                      {applyError.lines && <Lines lines={applyError.lines} label={applyError.lead} />}
                    </div>
                  )}
                </PendingBar>
              )
            }
          />
        ) : source !== null && busy ? (
          <section className="card explorer" aria-labelledby="e-files">
            <CardHead icon="folder" title={t.explorer} id="e-files" />
            <div className="library-busy" role="status" aria-live="polite">
              <span className="spinner" aria-hidden="true" />
              <span>
                {messages.common.reading}
                <Dots />
              </span>
            </div>
          </section>
        ) : (
          <HowItWorks />
        )}
      </div>
    </div>
  );
}

function extractRequest(source: string, output: string, paths: string[], password: string | null): ConvertRequest {
  return {
    source,
    format: "folder",
    output,
    compression_threads: null,
    inner: null,
    remove_backport: false,
    full_verify: false,
    archive_password: password,
    extract: paths,
    edits: [],
  };
}

function HowItWorks() {
  const t = useMessages().edit;
  return (
    <section className="card how" aria-labelledby="e-how">
      <CardHead icon="info" title={t.howTitle} id="e-how" />
      {t.how(APP_NAME).map((p, i) => (
        <p key={i}>{p.split("**").map((part, j) => (j % 2 ? <b key={j}>{part}</b> : part))}</p>
      ))}
    </section>
  );
}

function GameFolds({ ins }: { ins: Inspection }) {
  const t = useMessages().edit;
  return (
    <div className="folds game-folds">
      <details className="fold">
        <summary>{t.facts}</summary>
        <InspectSummary ins={ins} full rest />
      </details>
      <details className="fold">
        <summary>param.json</summary>
        {ins.param_json == null ? (
          <p className="muted">{t.noParam}</p>
        ) : (
          <pre className="code fold-box" tabIndex={0} role="region" aria-label="param.json">
            {JSON.stringify(ins.param_json, null, 2)}
          </pre>
        )}
      </details>
      <details className="fold">
        <summary>{t.details}</summary>
        <ul className="rows fold-box" tabIndex={0} aria-label={t.details}>
          <li>
            <span className="muted">{t.reader}</span> {ins.describe}
          </li>
          {ins.details.map((d, i) => (
            <li key={i}>{d}</li>
          ))}
        </ul>
      </details>
    </div>
  );
}

/** The game's files one folder at a time; editable rows queue changes in the pending bar until Apply. */
function Explorer(props: {
  ins: Inspection;
  tree: DirNode;
  editable: boolean;
  nas: boolean;
  peer: boolean;
  /** The tab is showing (drops on a hidden tab are not ours). */
  visible: boolean;
  /** The changes' generation this explorer shows; every action passes the one it began in. */
  gen: number;
  onChanges: (gen: number, list: NewChange[]) => void;
  onAdd: (gen: number, dir: string, items: Incoming[], shown: string) => Promise<void>;
  /** Missing: no Extract. */
  onExtract?: (paths: string[]) => Promise<void>;
  extractNote: boolean;
  pending: ReactNode;
}) {
  const { ins, tree, editable } = props;
  const t = useMessages().edit;
  const [cwd, setCwd] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<string | null>(null);
  const [addError, setAddError] = useState<string | null>(null);
  const card = useRef<HTMLElement>(null);
  const [filter, setFilter] = useState("");
  const [focus, setFocus] = useState(0);
  const want = useRef<string | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  const extractable = !!props.onExtract;
  /** Every path on disk: only these can be extracted (a pending new or renamed one is not there yet). */
  const onDisk = useMemo(() => {
    const s = new Set<string>();
    const addUp = (p: string) => {
      for (; p !== "" && !s.has(p); p = parentOf(p)) s.add(p);
    };
    for (const f of ins.files) addUp(f.path);
    for (const d of ins.empty_dirs) addUp(d);
    return s;
  }, [ins]);

  let here = cwd;
  while (here !== "" && !findDir(tree, here)) here = parentOf(here);
  const dir = findDir(tree, here) ?? tree;
  const query = filter.trim();
  const entries = useMemo(() => (query ? search(dir, here, query) : listDir(dir, here)), [dir, here, query]);
  const shown = entries.slice(0, MAX_ROWS);
  const atState = Math.min(focus, Math.max(shown.length - 1, 0));

  const go = (path: string, focusPath: string | null = null) => {
    setCwd(path);
    setFilter("");
    setFocus(0);
    setRenaming(null); // a name being typed is dropped with the folder it was in
    want.current = focusPath ?? "";
  };
  const up = () => here !== "" && go(parentOf(here), here);
  const open = (e: Entry) => e.node.kind === "dir" && go(e.path);
  const shownPath = (p: string) => (p === "" ? t.root : `${t.root}/${p}`);

  // Read what is added, then hand it over with the generation it began in: a changed source drops it.
  const add = async (gen: number, dir: string, read: () => Promise<Incoming[]>) => {
    setAddError(null);
    try {
      await props.onAdd(gen, dir, await read(), shownPath(dir));
    } catch (e) {
      if (gen === live.current.gen) setAddError(errorText(e));
    }
  };
  const addPicked = async (folder: boolean) => {
    const at = here;
    const gen = props.gen;
    const paths = folder
      ? await pickPath({ directory: true, title: t.pickAddFolder, devices: true, peersOnly: true }).then((p) => (p === null ? null : [p]))
      : await pickFiles({ directory: false, title: t.pickAddFiles, devices: true, peersOnly: true });
    if (paths !== null) await add(gen, at, () => Promise.all(paths.map(fromHost)));
  };
  const extract = async (paths: string[]) => {
    setAddError(null);
    try {
      await props.onExtract?.(paths);
    } catch (e) {
      setAddError(errorText(e));
    }
  };
  const folderPaths = here === "" ? [...onDisk].filter((p) => !p.includes("/")) : onDisk.has(here) ? [here] : [];
  // A failed add or Extract is said until the next change (or a new generation).
  useEffect(() => setAddError(null), [props.gen]);
  const remove = (e: Entry) => {
    setAddError(null);
    const i = shown.findIndex((x) => x.path === e.path);
    const next = shown[i + 1] ?? shown[i - 1];
    want.current = next ? next.path : "";
    props.onChanges(props.gen, [{ op: "delete", path: e.path }]);
  };
  const renamed = (e: Entry, name: string | null) => {
    setRenaming(null);
    if (name !== null) setAddError(null);
    const to = name === null ? e.path : join(parentOf(e.path), name);
    want.current = to;
    if (name !== null) props.onChanges(props.gen, [{ op: "rename", from: e.path, to }]);
  };

  const live = useRef({ here, editable, visible: props.visible, gen: props.gen, add });
  live.current = { here, editable, visible: props.visible, gen: props.gen, add };
  useEffect(() => {
    const hit = (x: number, y: number): string | null => {
      const l = live.current;
      if (!l.editable || !l.visible) return null;
      const el = document.elementFromPoint(x, y);
      if (!el || !card.current?.contains(el)) return null;
      const row = el.closest<HTMLElement>("[data-path]");
      return row?.dataset.dir ? (row.dataset.path ?? l.here) : l.here;
    };
    return onDrop(
      (d) => {
        const at = hit(d.x, d.y);
        if (at === null) return;
        void live.current.add(live.current.gen, at, () => dropped(d.items));
      },
      (pt) => setDropAt(pt ? hit(pt.x, pt.y) : null),
    );
  }, []);

  useEffect(() => {
    if (want.current === null) return;
    const i = focusIndex(shown, want.current);
    want.current = null;
    if (i === null) {
      // An empty folder: the grid itself, so Backspace and Alt+Up still go up.
      grid.current?.focus();
      return;
    }
    setFocus(i);
    grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]")[i]?.focus();
  });

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    // A row's own button takes Enter and Space (the name input stops its keys itself).
    if ((e.target as HTMLElement).tagName === "BUTTON" && (e.key === "Enter" || e.key === " ")) return;
    // The row the key came from (the focus state can lag a programmatic focus), else the state's.
    const from = (e.target as HTMLElement).closest?.("[role=row][data-path]") as HTMLElement | null;
    const fromAt = from ? shown.findIndex((x) => x.path === from.dataset.path) : -1;
    const at = fromAt >= 0 ? fromAt : atState;
    const move = (i: number) => {
      e.preventDefault();
      if (shown.length === 0) return;
      const n = Math.min(Math.max(i, 0), shown.length - 1);
      setFocus(n);
      grid.current?.querySelectorAll<HTMLElement>("[role=row][data-path]")[n]?.focus();
    };
    if (e.key === "ArrowDown") move(at + 1);
    else if (e.key === "ArrowUp" && !e.altKey) move(at - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(shown.length - 1);
    else if (e.key === "PageDown") move(at + 10);
    else if (e.key === "PageUp") move(at - 10);
    else if (e.key === "Enter" && shown[at]) {
      e.preventDefault();
      open(shown[at]);
    } else if (e.key === "Backspace" || (e.key === "ArrowUp" && e.altKey)) {
      e.preventDefault();
      up();
    } else if (editable && e.key === "F2" && shown[at]) {
      e.preventDefault();
      setRenaming(shown[at].path);
    } else if (editable && e.key === "Delete" && shown[at]) {
      e.preventDefault();
      remove(shown[at]);
    }
  };

  const crumbs = split(here);
  const dirs = shown.filter((e) => e.node.kind === "dir").length;
  const count = dropAt !== null
    ? t.dropInto(shownPath(dropAt))
    : query
    ? entries.length > MAX_ROWS
      ? t.firstOf(MAX_ROWS, entries.length)
      : t.matches(entries.length)
    : entries.length > MAX_ROWS
      ? t.firstOf(MAX_ROWS, entries.length)
      : t.inFolder(dirs, entries.length - dirs);
  const rel = (path: string) => (here === "" ? path : path.slice(here.length + 1));

  return (
    <section className="card explorer" aria-labelledby="e-files" ref={card}>
      <CardHead icon="folder" title={t.explorer} id="e-files">
        <span className="tag">{t.fileCount(ins.files.length)}</span>
      </CardHead>
      {(ins.archive || ins.patch || props.nas || props.peer) && (
        <p className="note-line explorer-note">
          <Icon name="info" />
          <span>{props.nas ? t.nasNote : props.peer ? t.peerNote : ins.patch ? t.patchNote : t.archiveNote}</span>
        </p>
      )}
      {props.pending}
      <div className="row nowrap explorer-nav">
        <button className="small" onClick={up} disabled={here === ""} title={t.upTip}>
          <Icon name="up" />
          {t.up}
        </button>
        <nav className="crumbs grow" aria-label={t.location}>
          {crumbs.length === 0 ? (
            <span className="here" aria-current="location">
              {t.root}
            </span>
          ) : (
            <button type="button" className="link" onClick={() => go("", crumbs[0])}>
              {t.root}
            </button>
          )}
          {crumbs.map((c, i) => {
            const path = crumbs.slice(0, i + 1).join("/");
            return (
              <span key={path} className="crumb">
                <span className="muted" aria-hidden="true">
                  /
                </span>
                {i === crumbs.length - 1 ? (
                  <span className="here" aria-current="location">
                    {c}
                  </span>
                ) : (
                  <button type="button" className="link" onClick={() => go(path, crumbs.slice(0, i + 2).join("/"))}>
                    {c}
                  </button>
                )}
              </span>
            );
          })}
        </nav>
      </div>
      {(editable || extractable) && (
        <div className="row explorer-tools">
          {editable && (
            <>
              <button className="small" onClick={() => void addPicked(false)}>
                <Icon name="file-plus" />
                {t.addFiles}
              </button>
              <button className="small" onClick={() => void addPicked(true)}>
                <Icon name="folder-in" />
                {t.addFolder}
              </button>
            </>
          )}
          {extractable && (
            <button
              className="small"
              onClick={() => void extract(folderPaths)}
              disabled={folderPaths.length === 0}
              title={here === "" ? t.extractGameTip : folderPaths.length === 0 ? t.extractNotOnDisk : t.extractFolderTip}
            >
              <Icon name="download" />
              {t.extractFolder}
            </button>
          )}
        </div>
      )}
      {extractable && props.extractNote && <p className="muted extract-note">{t.extractPending}</p>}
      {addError && (
        <p className="note-line block" role="alert">
          <Icon name="alert" />
          <span className="backend">{addError}</span>
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
        {/* Quiet during a drag: every row crossed rewrites it. */}
        <span className="muted count-text" role="status" aria-live={dropAt !== null ? "off" : undefined}>
          {count}
        </span>
      </div>
      <div className={dropAt !== null && dropAt === here ? "explorer-well drop" : "explorer-well"}>
        <div
          role="grid"
          ref={grid}
          className={`xgrid${editable ? " editable" : extractable ? " extractable" : ""}`}
          aria-label={t.explorer}
          aria-rowcount={shown.length + 1}
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
              <span className="visually-hidden">{t.actions}</span>
            </span>
          </div>
          {shown.map((e, i) => {
            const isDir = e.node.kind === "dir";
            const name = query ? rel(e.path) : e.node.name;
            const parent = findDir(tree, parentOf(e.path));
            const mine = i === atState ? 0 : -1;
            const cls = `xrow${isDir ? " dir" : ""}${dropAt === e.path ? " drop" : ""}`;
            return (
              <div
                key={e.path}
                role="row"
                data-path={e.path}
                data-dir={isDir ? "1" : undefined}
                aria-rowindex={i + 2}
                tabIndex={mine}
                className={cls}
                onFocus={(ev) => ev.target === ev.currentTarget && setFocus(i)}
                onDoubleClick={() => renaming !== e.path && open(e)}
              >
                <span role="gridcell" className="xname">
                  <Icon name={isDir ? "folder" : "file"} />
                  {renaming === e.path ? (
                    <NameInput
                      initial={e.node.name}
                      label={t.renameItem(e.node.name)}
                      taken={(n) => !!parent?.children.has(n)}
                      onDone={(n) => renamed(e, n)}
                    />
                  ) : (
                    <span className="xlabel">{name}</span>
                  )}
                  <MarkTag node={e.node} />
                </span>
                <span role="gridcell" className="xsize">
                  <Size node={e.node} />
                </span>
                <span role="gridcell" className="xact">
                  {editable && renaming !== e.path && (
                    <button
                      type="button"
                      className="small icon-only xbtn"
                      tabIndex={mine}
                      aria-label={t.renameItem(e.node.name)}
                      title={t.renameTip}
                      onClick={() => setRenaming(e.path)}
                    >
                      <Icon name="pencil" />
                    </button>
                  )}
                  {extractable && renaming !== e.path && onDisk.has(e.path) && (
                    <button
                      type="button"
                      className="small icon-only xbtn"
                      tabIndex={mine}
                      aria-label={t.extractItem(e.node.name)}
                      title={t.extractTip}
                      onClick={() => void extract([e.path])}
                    >
                      <Icon name="download" />
                    </button>
                  )}
                  {editable && renaming !== e.path && (
                    <button
                      type="button"
                      className="small icon-only xbtn danger"
                      tabIndex={mine}
                      aria-label={t.removeItem(e.node.name)}
                      title={isMac ? t.removeTipMac : t.removeTip}
                      onClick={() => remove(e)}
                    >
                      <Icon name="trash" />
                    </button>
                  )}
                  {isDir && (
                    <button
                      type="button"
                      className="small icon-only xopen"
                      tabIndex={-1}
                      aria-label={t.open(name)}
                      title={t.open(name)}
                      onClick={() => open(e)}
                    >
                      <Icon name="chevron" />
                    </button>
                  )}
                </span>
              </div>
            );
          })}
        </div>
        {shown.length === 0 && (
          <p className="muted empty-note">
            {ins.files.length === 0 && ins.archive
              ? t.noListing
              : query
                ? t.noMatch(query)
                : t.emptyFolder}
          </p>
        )}
      </div>
    </section>
  );
}

function uploadsOf(list: readonly NewChange[]): { files: number; bytes: number } {
  let files = 0;
  let bytes = 0;
  for (const c of list) {
    if (c.op !== "add" || !c.src || !uploaded(c.src)) continue;
    const sizes = c.what.kind === "file" ? [c.what.size] : c.what.files.map((f) => f.size);
    files += sizes.length;
    for (const s of sizes) bytes += s;
  }
  return { files, bytes };
}

const MARK_CLASS: Record<Mark, string> = { new: "green", replaced: "orange", renamed: "blue" };

function MarkTag({ node }: { node: TreeNode }) {
  const t = useMessages().edit;
  if (!node.mark) return null;
  const label = node.mark === "new" ? t.markNew : node.mark === "replaced" ? t.markReplaced : t.markRenamed;
  return (
    <span className={`tag xmark ${MARK_CLASS[node.mark]}`} title={node.from ? t.wasAt(node.from) : undefined}>
      {label}
      {node.from && <span className="visually-hidden">{t.wasAt(node.from)}</span>}
    </span>
  );
}

/** A file's size, a folder's entry count; "—" for a folder from a listing cut short. */
function Size({ node }: { node: TreeNode }) {
  const t = useMessages().edit;
  if (node.kind === "dir" && node.host)
    return (
      <>
        <span title={t.sizeLater} aria-hidden="true">
          —
        </span>
        <span className="visually-hidden">{t.sizeLater}</span>
      </>
    );
  return <>{node.kind === "dir" ? t.items(node.children.size) : prettyBytes(node.size)}</>;
}

/** Rename in place: Enter keeps, Escape drops, blur keeps a valid name; a bad name says why and stays open. */
export function NameInput(props: {
  initial: string;
  label: string;
  taken: (name: string) => boolean;
  onDone: (name: string | null) => void;
  dir?: boolean;
}) {
  const t = useMessages().edit;
  const [value, setValue] = useState(props.initial);
  const [error, setError] = useState<string | null>(null);
  const done = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const errId = useId();
  useEffect(() => {
    const i = input.current;
    if (!i) return;
    i.focus();
    const dot = props.initial.lastIndexOf(".");
    i.setSelectionRange(0, dot > 0 && !props.dir ? dot : props.initial.length);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    props.onDone(name);
  };
  const commit = (leaving: boolean) => {
    if (value === props.initial) return finish(null);
    const why =
      value === "" ? t.nameEmpty : !validName(value) ? t.nameBad : props.taken(value) ? t.nameTaken(value) : null;
    if (why === null) return finish(value);
    if (leaving) return finish(null);
    setError(why);
  };
  return (
    <span className="xedit">
      <input
        ref={input}
        className="xinput"
        value={value}
        aria-label={props.label}
        aria-invalid={error !== null}
        aria-describedby={error ? errId : undefined}
        spellCheck={false}
        autoComplete="off"
        autoCapitalize="none"
        autoCorrect="off"
        onChange={(e) => {
          setValue(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => {
          e.stopPropagation(); // the grid's keys (Backspace, arrows) are the input's here
          if (e.key === "Enter") {
            e.preventDefault();
            commit(false);
          } else if (e.key === "Escape") {
            e.preventDefault();
            finish(null);
          }
        }}
        onBlur={() => commit(true)}
        onDoubleClick={(e) => e.stopPropagation()}
      />
      {error && (
        <span className="field-error bad" id={errId} role="alert">
          {error}
        </span>
      )}
    </span>
  );
}

/** Leftover `.part`s where outputs land: listed only, never deleted; the card shows only when some were found. */
function Leftovers(props: { dirs: (string | null)[]; visible: boolean }) {
  const t = useMessages().edit;
  const dirs = [...new Set(props.dirs.filter((d): d is string => !!d))];
  const key = JSON.stringify(dirs);
  const [found, setFound] = useState<{ key: string; parts: string[] }>({ key: "[]", parts: [] });
  const parts = found.key === key ? found.parts : [];

  useEffect(() => {
    if (!props.visible) return;
    const list: string[] = JSON.parse(key);
    if (list.length === 0) return;
    let live = true;
    api.staleParts(list).then(
      (p) => live && setFound({ key, parts: p }),
      () => live && setFound({ key, parts: [] }),
    );
    return () => {
      live = false;
    };
  }, [key, props.visible]);

  if (parts.length === 0) return null;
  return (
    <section className="card compact" aria-labelledby="e-leftovers">
      <CardHead icon="file" title={t.leftovers} id="e-leftovers">
        <span className="tag orange">{t.found(parts.length)}</span>
      </CardHead>
      <ul className="rows path fold-box" tabIndex={0} aria-label={t.leftovers}>
        {parts.map((p) => (
          <li key={p}>{p}</li>
        ))}
      </ul>
      <p className="muted">{t.leftoversNote(APP_NAME)}</p>
      <details className="fold">
        <summary>{t.checked(dirs.length)}</summary>
        <ul className="rows path">
          {dirs.map((d) => (
            <li key={d}>{d}</li>
          ))}
        </ul>
      </details>
    </section>
  );
}

function changesSource(j: Job): boolean {
  return !!j.request?.edits?.length || !!j.request?.recover_metadata;
}

function withMetadata(request: ConvertRequest, on: boolean): ConvertRequest {
  return on ? { ...request, recover_metadata: { url: null, original_runtime: false } } : request;
}
