// The Edit tab's pending changes: what an add brings (a path that the job reads itself, or a
// browser's files uploaded on Apply), the conflict question, and the bar over the explorer that
// lists the changes in order with Undo and Discard all. The fold itself is explorer.ts's.

import type { ReactNode } from "react";

import { api, dropContents, type DropItem } from "./api";
import { basename, Modal, openOverlay } from "./common";
import { withoutJunk, type AddItem, type Finding, type Planned } from "./explorer";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";

/** Where an add's bytes come from: a path on the machine that runs the job (the app's drops and
 * dialogs; the web page's dialog, which browses the server), or, from a browser drop, files that
 * are uploaded on Apply (a folder's paths relative to it, junk left out as core's scanner does). */
export type AddSource =
  | { path: string }
  | { file: File }
  | { folder: { files: { path: string; file: File }[]; emptyDirs: string[] } };

/** A change as an action makes it (`only`: a folder of which only the listed contents go in,
 * one add each, since some were skipped), before it joins the list. */
export type NewChange = Planned<AddSource>;
/** One pending change; `batch` groups what one action made (a drop of several items), which
 * Undo takes back together. */
export type Pending = NewChange & { batch: number };

/** Something to add, before it is placed: its name, what it brings, where from. */
export type Incoming = AddItem<AddSource>;

/** A path on the machine that runs the job, as core will add it (`host_tree`): a file's size,
 * a folder's contents. Its name by the platform's separators (a `\` in a macOS or Linux name
 * stays, and core refuses it). */
export async function fromHost(path: string): Promise<Incoming> {
  const t = await api.hostTree(path);
  return {
    name: basename(path),
    what: t.kind === "file" ? { kind: "file", size: t.size } : { kind: "dir", files: t.files, emptyDirs: t.empty_dirs, truncated: t.truncated },
    src: { path },
  };
}

/** A drop's items. The app's are host paths (listed through `host_tree`); the web page's are
 * the browser's files (a folder walked now, its files uploaded on Apply). */
export async function dropped(items: DropItem[]): Promise<Incoming[]> {
  const out: Incoming[] = [];
  for (const item of items) {
    if (item.path !== undefined) {
      out.push(await fromHost(item.path));
    } else if (item.entry?.isDirectory) {
      const { files, emptyDirs } = await dropContents(item);
      const pre = `${item.name}/`;
      const rel = (p: string) => (p.startsWith(pre) ? p.slice(pre.length) : "");
      const handles = new Map(files.map((f) => [rel(f.path), f.file]));
      const clean = withoutJunk(
        files.map((f) => ({ path: rel(f.path), size: f.file.size })).filter((f) => f.path !== ""),
        emptyDirs.map(rel).filter((d) => d !== ""),
      );
      out.push({
        name: item.name,
        what: { kind: "dir", files: clean.files, emptyDirs: clean.emptyDirs },
        src: { folder: { files: clean.files.map((f) => ({ path: f.path, file: handles.get(f.path)! })), emptyDirs: clean.emptyDirs } },
      });
    } else if (item.file) {
      out.push({ name: item.name, what: { kind: "file", size: item.file.size }, src: { file: item.file } });
    }
  }
  return out;
}

export type ConflictAnswer = "replace" | "skip" | "cancel";

/** "N items already exist here": the colliding names in `folder` (shown as given), then
 * Cancel (focused), Skip, Replace. Escape and a click outside cancel. */
export function askConflicts(names: string[], folder: string): Promise<ConflictAnswer> {
  return new Promise((resolve) => {
    let answer: ConflictAnswer = "cancel";
    openOverlay(
      (close) => (
        <ConflictDialog
          names={names}
          folder={folder}
          close={close}
          answer={(a) => {
            answer = a;
            close();
          }}
        />
      ),
      () => resolve(answer),
    );
  });
}

function ConflictDialog(props: { names: string[]; folder: string; close: () => void; answer: (a: ConflictAnswer) => void }) {
  const t = useMessages().edit;
  return (
    <Modal close={props.close} className="modal card conflict" role="alertdialog" labelledBy="cf-title" describedBy="cf-text">
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="warn" />
        </span>
        <h2 id="cf-title">{t.conflictTitle(props.names.length)}</h2>
      </div>
      <div id="cf-text">
        <p className="conflict-in">{t.conflictIn(<span className="path">{props.folder}</span>)}</p>
        <ul className="rows path fold-box conflict-names" tabIndex={0} aria-label={t.conflictTitle(props.names.length)}>
          {props.names.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
        <p className="muted">{t.conflictText}</p>
      </div>
      <div className="row end">
        <button autoFocus onClick={() => props.answer("cancel")}>
          {t.cancel}
        </button>
        <button onClick={() => props.answer("skip")}>{t.skip}</button>
        <button className="danger" onClick={() => props.answer("replace")}>
          {t.replace}
        </button>
      </div>
    </Modal>
  );
}

/** "Discard all N changes?": No (focused) / Discard all. True: discard. */
export function confirmDiscard(n: number): Promise<boolean> {
  return new Promise((resolve) => {
    let yes = false;
    openOverlay(
      (close) => (
        <DiscardDialog
          n={n}
          close={close}
          onYes={() => {
            yes = true;
            close();
          }}
        />
      ),
      () => resolve(yes),
    );
  });
}

function DiscardDialog(props: { n: number; close: () => void; onYes: () => void }) {
  const t = useMessages().edit;
  return (
    <Modal close={props.close} className="modal card" role="alertdialog" labelledBy="dc-title" describedBy="dc-text">
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="warn" />
        </span>
        <h2 id="dc-title">{t.discardTitle(props.n)}</h2>
      </div>
      <p id="dc-text" className="muted">
        {t.discardText}
      </p>
      <div className="row end">
        <button autoFocus onClick={props.close}>
          {t.discardNo}
        </button>
        <button className="danger" onClick={props.onYes}>
          {t.discard}
        </button>
      </div>
    </Modal>
  );
}

const P = ({ children }: { children: ReactNode }) => <span className="path">{children}</span>;

/** Why a change won't apply, in the UI's words (core's preflight says it again on Apply). */
export function findingText(t: Messages["edit"], f: Finding): ReactNode {
  const p = <P>{f.path === "" ? "“”" : f.path}</P>;
  switch (f.why) {
    case "invalid":
      return t.whyInvalid(p);
    case "missing":
      return t.whyMissing(p);
    case "exists":
      return t.whyExists(p);
    case "file":
      return t.whyFile(p);
    case "folder":
      return t.whyFolder(p);
    case "into":
      return t.whyInto(p);
  }
}

function changeText(t: Messages["edit"], c: Pending): ReactNode {
  switch (c.op) {
    case "add":
      return c.what.kind === "dir" ? t.opAddFolder(<P>{c.to}</P>) : t.opAdd(<P>{c.to}</P>);
    case "delete":
      return t.opDelete(<P>{c.path}</P>);
    case "rename":
      return t.opRename(<P>{c.from}</P>, <P>{c.to}</P>);
    case "mkdir":
      return t.opMkdir(<P>{c.path}</P>);
  }
}

/** Where an add comes from, for its list entry. */
function sourceText(src: AddSource | undefined): string | null {
  if (!src) return null;
  if ("path" in src) return src.path;
  return "file" in src ? src.file.name : null;
}

/**
 * Over the explorer while changes are pending: "N changes not applied yet", Undo (the last
 * action's changes) and Discard all, then the changes in order (folded away), each with why it
 * won't apply when it doesn't. `actions`: Apply; `children`: the mode, uploads and Apply's errors.
 */
export function PendingBar(props: {
  changes: Pending[];
  findings: Finding[][];
  onUndo: () => void;
  onDiscard: () => void;
  disabled?: boolean;
  /** After Discard all in the head: Apply. */
  actions?: ReactNode;
  children?: ReactNode;
}) {
  const t = useMessages().edit;
  const n = props.changes.length;
  const failing = props.findings.filter((f) => f.length > 0).length;
  return (
    <section className="pending-bar" aria-labelledby="e-pending">
      <div className="row pending-head">
        <Icon name="pencil" />
        <b id="e-pending" className="grow" role="status">
          {t.pending(n)}
        </b>
        <button className="small" onClick={props.onUndo} disabled={props.disabled} title={t.undoTip}>
          <Icon name="undo" />
          {t.undo}
        </button>
        <button className="small danger" onClick={props.onDiscard} disabled={props.disabled} title={t.discardTip}>
          <Icon name="x" />
          {t.discard}
        </button>
        {props.actions}
      </div>
      {failing > 0 && (
        <p className="note-line warn">
          <Icon name="warn" />
          <span>{t.willNotApply(failing)}</span>
        </p>
      )}
      {props.children}
      <details className="fold">
        <summary>{t.changesList}</summary>
        <ol className="rows fold-box changes" tabIndex={0} aria-label={t.changesList}>
          {props.changes.map((c, i) => {
            const from = c.op === "add" ? sourceText(c.src) : null;
            return (
              <li key={i}>
                <span>{changeText(t, c)}</span>
                {from && <span className="muted change-from">{t.opFrom(<P>{from}</P>)}</span>}
                {props.findings[i]?.map((f, j) => (
                  <span key={j} className="note-line warn">
                    <Icon name="warn" />
                    <span>{findingText(t, f)}</span>
                  </span>
                ))}
              </li>
            );
          })}
        </ol>
      </details>
    </section>
  );
}
