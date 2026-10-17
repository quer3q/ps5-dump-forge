// The Edit tab's Apply: core's check of the pending changes (debounced; how they can go in: in
// place, rebuilt beside the image, or not at all), the mode shown in the pending bar, the
// confirmation, and the request an edit job takes. The web page's dropped files are uploaded on
// Apply, so the check before it leaves them out (and says so); Apply checks again once they are in.

import { useEffect, useRef, useState } from "react";

import { api, errorText, prettyBytes, type ConvertRequest, type EditCheck, type Format, type Inspection } from "./api";
import { Modal, openOverlay, Prose } from "./common";
import type { AddSource, Pending } from "./EditChanges";
import { checkable, toEdits, type CoreEdit } from "./explorer";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { joinPath } from "./paths";

export type Mode = "in_place" | "rebuild";

/** A browser's files: on the server only once uploaded (Apply). A path is read by core itself. */
export const uploaded = (src: AddSource) => !("path" in src);

/** The path core reads for an add from a host path (`rel`: a file inside that folder), joined
 * by the platform's separators. */
export function hostPath(src: { path: string }, rel: string | null): string {
  return rel === null ? src.path : rel.split("/").reduce((dir, name) => joinPath(dir, name), src.path);
}

/** An edit job: the source changed in place (or, `rebuild`, an image rebuilt beside it and
 * swapped in). `output` and `format` are ignored by core; the job row names the source. */
export function editRequest(source: string, kind: string, edits: CoreEdit[], rebuild: boolean): ConvertRequest {
  return {
    source,
    format: kind as Format,
    output: source,
    compression_threads: null,
    inner: null,
    remove_backport: false,
    full_verify: false,
    edits,
    edit_rebuild: kind !== "folder" && rebuild,
  };
}

/** Core's answer for one list of changes (`for`, and the `nonce` it was asked under). `check`
 * null with no error: nothing could be checked before Apply (only uploads, or `deferred`: other
 * changes depend on them); `left`: the uploaded files the check left out. `full`: Apply's own
 * check with the uploads in, kept until the changes change. */
export interface CheckState {
  for: readonly Pending[];
  nonce: number;
  check: EditCheck | null;
  error: string | null;
  left: number;
  deferred?: boolean;
  full?: boolean;
}

/** Core's check of `changes`, ~500 ms after the last change; the latest answer wins. `nonce`:
 * asked again when it changes (a job ended, Check again). Off while `enabled` is false. */
export function useEditCheck(
  source: string | null,
  ins: Inspection | null,
  changes: readonly Pending[],
  nonce: number,
  enabled: boolean,
) {
  const [state, setState] = useState<CheckState | null>(null);
  const seq = useRef(0);
  const held = useRef(state);
  held.current = state;
  useEffect(() => {
    // Apply's full answer stands for these changes (a re-check would leave the uploads out).
    const h = held.current;
    if (h?.full && h.for === changes && h.nonce === nonce) return;
    const mine = ++seq.current;
    if (!source || !ins || changes.length === 0 || !enabled) return;
    const timer = setTimeout(async () => {
      const { changes: some, left } = checkable(changes, uploaded);
      if (some === null) {
        if (mine === seq.current) setState({ for: changes, nonce, check: null, error: null, left, deferred: true });
        return;
      }
      try {
        const edits = await toEdits(ins.files, ins.empty_dirs, some, uploaded, async (src, rel) =>
          hostPath(src as { path: string }, rel),
        );
        const check = edits.length > 0 ? await api.editCheck(editRequest(source, ins.kind, edits, false)) : null;
        if (mine === seq.current) setState({ for: changes, nonce, check, error: null, left });
      } catch (e) {
        if (mine === seq.current) setState({ for: changes, nonce, check: null, error: errorText(e), left });
      }
    }, 500);
    return () => clearTimeout(timer);
  }, [source, ins, changes, nonce, enabled]);
  const current = state !== null && state.for === changes && state.nonce === nonce;
  /** Apply's own check, with the uploads in: the bar shows it until the changes change. */
  const settle = (check: EditCheck) => {
    seq.current++;
    setState({ for: changes, nonce, check, error: null, left: 0, full: true });
  };
  return { state, current, settle };
}

/** Which ways the changes can go in. With no check (only uploads, checked on Apply): what the
 * format allows. A folder is only ever changed directly. */
export function modes(state: CheckState, kind: string): { inPlace: boolean; rebuild: boolean } {
  const c = state.check;
  if (kind === "folder") return { inPlace: c ? c.in_place.length === 0 : true, rebuild: false };
  if (!c) return { inPlace: kind !== "ffpfsc" && kind !== "pkg", rebuild: true };
  return { inPlace: c.in_place.length === 0, rebuild: c.rebuild.length === 0 };
}

/** The mode Apply uses: `want` when both work, else the one that does; null when neither does
 * (or there is no answer for these changes yet). */
export function chosenMode(state: CheckState | null, current: boolean, kind: string, want: Mode): Mode | null {
  if (!state || !current || state.error !== null) return null;
  const m = modes(state, kind);
  if (m.inPlace && m.rebuild) return want;
  return m.inPlace ? "in_place" : m.rebuild ? "rebuild" : null;
}

const Lines = ({ lines }: { lines: string[] }) => (
  <ul className="rows backend edit-why">
    {[...new Set(lines)].map((l) => (
      <li key={l}>{l}</li>
    ))}
  </ul>
);

/**
 * The pending bar's mode: In place ("written into the image; needs N of its M free"), Rebuild
 * needed (why, and what a rebuild needs beside the image), Can't apply (core's findings), or for
 * a folder Changed directly; when both ways work, a choice (In place first). While core is
 * asked, the last answer stays with "Checking".
 */
export function ModeBox(props: {
  kind: string;
  state: CheckState | null;
  current: boolean;
  want: Mode;
  onWant: (m: Mode) => void;
  onRetry: () => void;
  disabled?: boolean;
}) {
  const t = useMessages().edit;
  const { state, kind } = props;
  const checking = !props.current && (
    <p className="muted edit-checking" role="status">
      {t.checking}
    </p>
  );
  if (!state) return <div className="edit-mode">{checking}</div>;
  if (state.error !== null)
    return (
      <div className="edit-mode">
        <div className="note-line block" role="alert">
          <Icon name="alert" />
          <span>
            {t.checkFailed} <span className="backend">{state.error}</span>
          </span>
        </div>
        <button className="small" onClick={props.onRetry} disabled={props.disabled}>
          {t.checkAgain}
        </button>
        {checking}
      </div>
    );
  const c = state.check;
  const m = modes(state, kind);
  const sized = (n: number | null | undefined) => (n == null ? null : prettyBytes(n));
  // Nothing new inside (a delete, a rename): no size to name.
  const need = c?.needed_inside ? prettyBytes(c.needed_inside) : null;
  const free = sized(c?.free_inside);
  const inPlaceText = need && free ? t.inPlaceNeeds(need, free) : t.inPlaceText;
  const besideNeed = sized(c?.rebuild_needs);
  const besideFree = sized(c?.free_beside);
  const rebuildText = besideNeed && besideFree ? t.rebuildNeeds(besideNeed, besideFree) : t.rebuildText;
  const chosen = m.inPlace && m.rebuild ? props.want : m.inPlace ? "in_place" : "rebuild";
  const later = state.left > 0 && (
    <p className="note-line">
      <Icon name="info" />
      <span>{state.deferred ? t.decidedOnApply(state.left) : t.uploadLater(state.left)}</span>
    </p>
  );

  let body;
  if (kind === "folder" && m.inPlace) {
    body = (
      <p className="edit-mode-line">
        <span className="tag blue">{t.changedDirectly}</span> {t.folderText}
      </p>
    );
  } else if (!m.inPlace && !m.rebuild) {
    body = (
      <>
        <p className="edit-mode-line">
          <span className="tag red">{t.cantApply}</span>
        </p>
        <Lines lines={[...(c?.in_place ?? []), ...(kind === "folder" ? [] : (c?.rebuild ?? []))]} />
      </>
    );
  } else if (m.inPlace && m.rebuild) {
    body = (
      <>
        <div className="seg formats inner edit-pick" role="radiogroup" aria-label={t.modeLabel}>
          {(["in_place", "rebuild"] as const).map((v) => (
            <label key={v} className={`seg-choice mode-hue ${v === "in_place" ? "mode-green" : "mode-red"}${chosen === v ? " on" : ""}`}>
              <input
                type="radio"
                className="visually-hidden"
                name="e-mode"
                value={v}
                checked={chosen === v}
                disabled={props.disabled}
                onChange={() => props.onWant(v)}
              />
              <span className="seg-opt">{v === "in_place" ? t.inPlace : t.rebuild}</span>
            </label>
          ))}
        </div>
        <p className="muted edit-mode-text">{chosen === "in_place" ? inPlaceText : rebuildText}</p>
      </>
    );
  } else if (m.inPlace) {
    body = (
      <p className="edit-mode-line">
        <span className="tag green">{t.inPlace}</span> {inPlaceText}
      </p>
    );
  } else {
    const noRoom = need && free && c && c.needed_inside! > c.free_inside!;
    body = (
      <>
        <p className="edit-mode-line">
          <span className="tag red">{t.rebuildNeeded}</span>{" "}
          {kind === "ffpfsc" || kind === "pkg" ? <Prose text={t.alwaysRebuilt} /> : noRoom ? t.noRoom(need, free) : null}
        </p>
        {kind !== "ffpfsc" && kind !== "pkg" && !noRoom && c && <Lines lines={c.in_place} />}
        <p className="muted edit-mode-text">{rebuildText}</p>
      </>
    );
  }
  return (
    <div className="edit-mode" role="group" aria-label={t.modeLabel}>
      {body}
      {later}
      {checking}
    </div>
  );
}

/** "Apply N changes?": what the mode does, no undo, close the game first; Cancel (focused) /
 * Apply. True: apply. */
export function confirmApply(o: { n: number; mode: Mode; folder: boolean; name: string; restores: boolean }): Promise<boolean> {
  return new Promise((resolve) => {
    let yes = false;
    openOverlay(
      (close) => (
        <ApplyDialog
          {...o}
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

function ApplyDialog(props: {
  n: number;
  mode: Mode;
  folder: boolean;
  name: string;
  restores: boolean;
  close: () => void;
  onYes: () => void;
}) {
  const t = useMessages().edit;
  const name = <span className="path">{props.name}</span>;
  return (
    <Modal close={props.close} className="modal card" role="alertdialog" labelledBy="ap-title" describedBy="ap-text">
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="warn" />
        </span>
        <h2 id="ap-title">{t.applyTitle(props.n)}</h2>
      </div>
      <div id="ap-text">
        <p>{props.mode === "rebuild" ? t.applyRebuild(name) : t.applyInPlace(name)}</p>
        {props.restores && <p className="muted">{t.applyRestores}</p>}
        <p className="muted">{props.folder ? t.closeFolder : t.closeImage}</p>
      </div>
      <div className="row end">
        <button autoFocus onClick={props.close}>
          {t.cancel}
        </button>
        <button className="primary" onClick={props.onYes}>
          {t.apply}
        </button>
      </div>
    </Modal>
  );
}

/** Uploads in progress (web page, on Apply): which file, how much, a bar; Cancel stops them. */
export function UploadLine(props: { i: number; n: number; sent: number; total: number; onCancel: () => void }) {
  const t = useMessages().edit;
  return (
    <div className="upload-line">
      <progress value={props.sent} max={Math.max(props.total, 1)} aria-label={t.uploadLabel} />
      <div className="row nowrap">
        <span className="muted grow" role="status">
          {t.uploading(props.i, props.n, prettyBytes(props.sent), prettyBytes(props.total))}
        </span>
        <button className="small" onClick={props.onCancel}>
          {t.cancel}
        </button>
      </div>
    </div>
  );
}
