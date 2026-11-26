// The Edit tab's Apply: core's check of the changes, the mode, the confirmation, the edit request.
// A browser's files are uploaded on Apply: the check before it leaves them out.

import { useEffect, useRef, useState } from "react";

import { api, errorText, prettyBytes, prettyDuration, runtimePlatform, type ConvertRequest, type RuntimePlatform, type EditCheck, type Format, type Inspection } from "./api";
import { Modal, openOverlay, Prose } from "./common";
import type { AddSource, Pending } from "./EditChanges";
import { checkable, toEdits, type CoreEdit } from "./explorer";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { joinPath } from "./paths";
import { readable } from "./peers";

export type Mode = "in_place" | "rebuild";

/** A browser's files: on the server only once uploaded (Apply). A path is read by core itself. */
export const uploaded = (src: AddSource) => !("path" in src);

/** The path core reads for an add from a host path (`rel`: a file inside that folder). */
export function hostPath(src: { path: string }, rel: string | null): string {
  return rel === null ? src.path : rel.split("/").reduce((dir, name) => joinPath(dir, name), src.path);
}

/** An edit job (`output` and `format` are ignored by core). */
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

/** Core's answer for `for` under `nonce`. `check` null with no error: only uploads (or `deferred` on them),
 * or `nothing` to change; `left`: uploads the check left out; `full`: Apply's own check with them in. */
export interface CheckState {
  for: readonly Pending[];
  nonce: number;
  check: EditCheck | null;
  error: string | null;
  left: number;
  deferred?: boolean;
  nothing?: boolean;
  full?: boolean;
}

/** Core's check, ~500 ms after the last change; the latest answer wins; a new `nonce` asks again. */
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
        if (mine === seq.current) setState({ for: changes, nonce, check, error: null, left, nothing: edits.length === 0 && left === 0 });
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

/** Which ways the changes can go in; with no check, what the format allows. */
export function modes(state: CheckState, kind: string): { inPlace: boolean; rebuild: boolean } {
  const c = state.check;
  if (kind === "folder") return { inPlace: c ? c.in_place.length === 0 : true, rebuild: false };
  if (!c) return { inPlace: kind !== "ffpfsc" && kind !== "pkg", rebuild: true };
  return { inPlace: c.in_place.length === 0, rebuild: c.rebuild.length === 0 };
}

/** `want` when both work, else the one that does; null when neither (or no answer yet). */
export function chosenMode(state: CheckState | null, current: boolean, kind: string, want: Mode): Mode | null {
  if (!state || !current || state.error !== null || state.nothing) return null;
  const m = modes(state, kind);
  if (m.inPlace && m.rebuild) return want;
  return m.inPlace ? "in_place" : m.rebuild ? "rebuild" : null;
}

// Rates measured for issue #43. ponytail: fixed rates, not this device's own.
const REBUILD_PS5_PKG = 26e6; // bytes/s: a .pkg's Kraken on a PS5
const REBUILD_RATE: Record<RuntimePlatform, number> = { ps5: 60e6, host: 200e6 };

let platformOnce: Promise<RuntimePlatform> | null = null;
function usePlatform(): RuntimePlatform | null {
  const [p, setP] = useState<RuntimePlatform | null>(null);
  useEffect(() => {
    let live = true;
    (platformOnce ??= runtimePlatform()).then(
      (v) => live && setP(v),
      () => {
        platformOnce = null; // the next mount asks again
      },
    );
    return () => {
      live = false;
    };
  }, []);
  return p;
}

/** Issue #43: how long a rebuild takes, said before Apply. `size`: the game's bytes. */
function RebuildTime(props: { kind: string; size: number }) {
  const platform = usePlatform();
  const t = useMessages().edit;
  if (props.size <= 0) return null;
  const pkg = props.kind === "pkg";
  const time = (rate: number) => prettyDuration((props.size / rate) * 1000);
  const text = pkg
    ? t.rebuildTimePkg(time(REBUILD_PS5_PKG))
    : platform === "ps5"
      ? t.rebuildTimePs5(time(REBUILD_RATE.ps5))
      : platform
        ? t.rebuildTimeHost(time(REBUILD_RATE[platform]))
        : t.rebuildTimeAny(time(REBUILD_RATE.ps5));
  return (
    <p className="note-line warn">
      <Icon name="warn" />
      <span>{text}</span>
    </p>
  );
}

/** Core's findings, each once; a scrolling box, so it takes focus (`label` names it). */
export const Lines = ({ lines, label }: { lines: string[]; label: string }) => (
  <ul className="rows backend edit-why" tabIndex={0} aria-label={label}>
    {[...new Set(lines)].map((l) => (
      <li key={l}>{readable(l)}</li>
    ))}
  </ul>
);

/** The pending bar's mode: In place, Rebuild needed, Can't apply, Changed directly, or a choice. */
export function ModeBox(props: {
  kind: string;
  state: CheckState | null;
  current: boolean;
  want: Mode;
  onWant: (m: Mode) => void;
  onRetry: () => void;
  disabled?: boolean;
  /** The game's bytes, for the rebuild's time. */
  size?: number;
}) {
  const t = useMessages().edit;
  const { state, kind } = props;
  // Always there (its line kept, so nothing below jumps), so a screen reader hears "Checking"
  // and then the answer (`done`, the tag the box shows; hidden, as the tag says it).
  const checking = (done: string) => (
    <p className="muted edit-checking" role="status">
      {props.current ? <span className="visually-hidden">{done}</span> : t.checking}
    </p>
  );
  if (!state) return <div className="edit-mode">{checking("")}</div>;
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
        {checking("")}
      </div>
    );
  if (state.nothing)
    return (
      <div className="edit-mode">
        <p className="muted edit-mode-text">{t.nothingToChange}</p>
        {checking(t.nothingToChange)}
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
  let done;
  if (kind === "folder" && m.inPlace) {
    done = t.changedDirectly;
    body = (
      <p className="edit-mode-line">
        <span className="tag blue">{t.changedDirectly}</span> {t.folderText}
      </p>
    );
  } else if (!m.inPlace && !m.rebuild) {
    done = t.cantApply;
    body = (
      <>
        <p className="edit-mode-line">
          <span className="tag red">{t.cantApply}</span>
        </p>
        <Lines lines={[...(c?.in_place ?? []), ...(kind === "folder" ? [] : (c?.rebuild ?? []))]} label={t.cantApply} />
      </>
    );
  } else if (m.inPlace && m.rebuild) {
    done = chosen === "in_place" ? t.inPlace : t.rebuild;
    body = (
      <>
        <div className="seg formats inner" role="radiogroup" aria-label={t.modeLabel}>
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
        {chosen === "rebuild" && <RebuildTime kind={kind} size={props.size ?? 0} />}
      </>
    );
  } else if (m.inPlace) {
    done = t.inPlace;
    body = (
      <p className="edit-mode-line">
        <span className="tag green">{t.inPlace}</span> {inPlaceText}
      </p>
    );
  } else {
    const noRoom = need && free && c && c.needed_inside! > c.free_inside!;
    // Every reason but the room one, which the line says.
    const why = c ? (noRoom ? c.in_place.filter((l) => !l.includes("the changes need ")) : c.in_place) : [];
    done = t.rebuildNeeded;
    body = (
      <>
        <p className="edit-mode-line">
          <span className="tag red">{t.rebuildNeeded}</span>{" "}
          {kind === "ffpfsc" || kind === "pkg" ? <Prose text={t.alwaysRebuilt} /> : noRoom ? t.noRoom(need, free) : null}
        </p>
        {kind !== "ffpfsc" && kind !== "pkg" && why.length > 0 && <Lines lines={why} label={t.rebuildNeeded} />}
        <p className="muted edit-mode-text">{rebuildText}</p>
        <RebuildTime kind={kind} size={props.size ?? 0} />
      </>
    );
  }
  return (
    <div className="edit-mode">
      {body}
      {later}
      {checking(done)}
    </div>
  );
}

/** "Apply N changes?" (Cancel focused); true: apply. */
export function confirmApply(o: { n: number; mode: Mode; folder: boolean; name: string; restores: boolean; metadata?: boolean }): Promise<boolean> {
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
  metadata?: boolean;
  close: () => void;
  onYes: () => void;
}) {
  const t = useMessages().edit;
  const mm = useMessages().metadata;
  const name = <span className="path">{props.name}</span>;
  return (
    <Modal close={props.close} className="modal card" role="alertdialog" labelledBy="ap-title" describedBy="ap-text">
      <div className="modal-head">
        <span className="head-icon warn">
          <Icon name="warn" />
        </span>
        <h2 id="ap-title">{props.n === 0 && props.metadata ? mm.editConfirm : t.applyTitle(props.n)}</h2>
      </div>
      <div id="ap-text">
        <p>{props.mode === "rebuild" ? t.applyRebuild(name) : t.applyInPlace(name)}</p>
        {props.metadata && <p className="muted">{mm.editConfirmText}</p>}
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
      {/* Announced once per file; the bytes change on every progress event. */}
      <span className="visually-hidden" role="status">
        {t.uploadingFile(props.i, props.n)}
      </span>
      <div className="row nowrap">
        <span className="muted grow">
          {t.uploading(props.i, props.n, prettyBytes(props.sent), prettyBytes(props.total))}
        </span>
        <button className="small" onClick={props.onCancel}>
          {t.cancel}
        </button>
      </div>
    </div>
  );
}
