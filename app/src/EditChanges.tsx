// Edit's pending changes: what an add brings, the conflict question and the pending bar (the fold is explorer.ts's).

import type { ReactNode } from "react";

import { api, dropContents, type DropItem } from "./api";
import { basename, Modal, openOverlay } from "./common";
import { withoutJunk, type AddItem, type Finding, type Planned } from "./explorer";
import { msg, useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";
import { readable } from "./peers";

/** An add's bytes: a path on the machine that runs the job, or a browser drop's files uploaded on Apply. */
export type AddSource =
  | { path: string }
  | { file: File }
  | { folder: { files: { path: string; file: File }[]; emptyDirs: string[] } };

/** A change as an action makes it, before it joins the list. */
export type NewChange = Planned<AddSource>;
/** One pending change; `batch` groups one action's changes, which Undo takes back together. */
export type Pending = NewChange & { batch: number };

export type Incoming = AddItem<AddSource>;

/** A path on the job's machine as core will add it (`host_tree`); a `\` in a macOS or Linux name stays, core refuses it. */
export async function fromHost(path: string): Promise<Incoming> {
  // A paired device's root (`flink://<key_id>`) has no name to land under but its key id.
  if (/^flink:\/\/[^/\\]*[/\\]*$/i.test(path)) throw new Error(msg().edit.peerRoot);
  const t = await api.hostTree(path);
  return {
    name: basename(path),
    what: t.kind === "file" ? { kind: "file", size: t.size } : { kind: "dir", files: t.files, emptyDirs: t.empty_dirs, truncated: t.truncated },
    src: { path },
  };
}

/** A drop's items: host paths in the app, the browser's files on the web page (uploaded on Apply). */
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

/** "N items already exist here": Cancel (focused), Skip, Replace; Escape and a click outside cancel. */
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
    <Modal close={props.close} className="modal card" role="alertdialog" labelledBy="cf-title" describedBy="cf-in cf-text">
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="warn" />
        </span>
        <h2 id="cf-title">{t.conflictTitle(props.names.length)}</h2>
      </div>
      <div>
        <p id="cf-in" className="conflict-in">{t.conflictIn(<span className="path">{props.folder}</span>)}</p>
        <ul className="rows path fold-box conflict-names" tabIndex={0} aria-label={t.conflictTitle(props.names.length)}>
          {props.names.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
        <p id="cf-text" className="muted">
          {t.conflictText}
        </p>
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
  const p = <P>{f.path === "" ? '""' : f.path}</P>;
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

function sourceText(src: AddSource | undefined): string | null {
  if (!src) return null;
  if ("path" in src) return src.path;
  return "file" in src ? src.file.name : null;
}

/** Over the explorer while changes are pending: count, Undo, Discard all, then each change and why it won't apply. */
export function PendingBar(props: {
  changes: Pending[];
  findings: Finding[][];
  onUndo: () => void;
  onDiscard: () => void;
  disabled?: boolean;
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
                {from && <span className="muted change-from">{t.opFrom(<P>{readable(from)}</P>)}</span>}
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
