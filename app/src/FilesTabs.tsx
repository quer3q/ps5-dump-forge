// The File explorer's device tabs and drags. On a computer: This device, then each online paired
// PS5 (its `flink://<key_id>` share root), each tab its own explorer keeping its own folder.
// Files dropped from the system, or rows dragged onto another device's tab, copy as one transfer
// job (listed under Connect); a row dragged onto a folder row of its own tab moves there.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";

import { api, errorText, type DirEntry } from "./api";
import { tabId } from "./common";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import type { Action } from "./jobs";
import { readable } from "./peers";
import { askReplace } from "./replace";
import { transferRequest } from "./Transfer";
import { useConnectState } from "./useConnectState";

/** The tab list's prefix; `LOCAL` is this device's tab (key ids are hex, so it can't clash). */
export const FILES_TABS = "files-dev";
export const LOCAL = "local";

/** Another device's tab: a paired PS5. */
export interface DeviceTab {
  id: string;
  name: string;
  root: string;
}

/** Every paired PS5 seen online since the view mounted (its explorer stays mounted, so its
 * folder survives a poll that missed it) and the ones online now. */
export function usePs5Tabs(enabled: boolean): { all: DeviceTab[]; online: Set<string> } {
  const devices = useConnectState().state?.devices;
  const seen = useRef(new Map<string, DeviceTab>());
  const online = new Set<string>();
  if (enabled)
    for (const d of devices ?? [])
      if (d.online && d.platform === "ps5") {
        online.add(d.key_id);
        seen.current.set(d.key_id, { id: d.key_id, name: d.name, root: `flink://${d.key_id}` });
      }
  return { all: [...seen.current.values()], online };
}

/** The device tab under `el`, if it is one of the explorer's. */
export function tabAt(el: Element): string | undefined {
  const b = el.closest<HTMLElement>(".files-tabs [role=tab]");
  const prefix = tabId(FILES_TABS, "");
  return b?.id.startsWith(prefix) ? b.id.slice(prefix.length) : undefined;
}

export type CopyLine = { ok: true; text: string } | { ok: false; lead: string; text: string };

/** Copies as a transfer job: names already in `dest` are asked about first (Replace). */
export function useCopy(dispatch: (a: Action) => void) {
  const t = useMessages().storage;
  const [line, setLine] = useState<CopyLine | null>(null);
  const busy = useRef(false);
  const copy = async (paths: string[], dest: string | null) => {
    if (busy.current || paths.length === 0) return;
    if (dest === null) return setLine({ ok: false, lead: t.cantCopy, text: t.copyNoFolder });
    busy.current = true;
    try {
      const request = await askReplace(transferRequest(paths, dest));
      if (!request) return;
      const id = await api.startJob(request);
      dispatch({ type: "started", id, request });
      const to = readable(dest);
      const name = paths[0].split(/[\\/]/).filter(Boolean).pop() ?? paths[0];
      setLine({ ok: true, text: paths.length === 1 ? t.copyOne(name, to) : t.copyMany(paths.length, to) });
    } catch (e) {
      setLine({ ok: false, lead: t.cantCopy, text: errorText(e) });
    } finally {
      busy.current = false;
    }
  };
  return { copy, line };
}

/** The started copy (with Open Connect) or why it didn't start. Always mounted: announced. */
export function CopyStatus(props: { line: CopyLine | null; onConnect: () => void }) {
  const t = useMessages().storage;
  const l = props.line;
  return (
    <div role="status" className="files-copy">
      {l?.ok === true && (
        <p className="note-line">
          <Icon name="move" />
          <span className="grow">{l.text}</span>
          <button type="button" className="small" onClick={props.onConnect}>
            {t.openConnect}
          </button>
        </p>
      )}
      {l?.ok === false && (
        <p className="note-line block">
          <Icon name="alert" />
          <span>
            {l.lead} <span className="backend">{l.text}</span>
          </span>
        </p>
      )}
    </div>
  );
}

/** Where a dragged row would land: another device's tab, or a folder row of its own tab. */
export type DragTarget = { tab: string } | { dir: DirEntry };

/** A row dragged by the mouse (pointer events: Tauri's drag and drop takes HTML5's on Windows).
 * ponytail: mouse only; touch scrolls, and Move to does it there. */
export function useRowDrag(opts: {
  target: (x: number, y: number, e: DirEntry) => DragTarget | null;
  drop: (e: DirEntry, to: DragTarget) => void;
}) {
  const [drag, setDrag] = useState<{ entry: DirEntry; x: number; y: number; over: DragTarget | null } | null>(null);
  const start = useRef<{ entry: DirEntry; row: HTMLElement; x: number; y: number; id: number; on: boolean } | null>(null);
  const live = useRef(opts);
  live.current = opts;
  const onPointerDown = (ev: ReactPointerEvent<HTMLElement>, entry: DirEntry) => {
    if (ev.pointerType !== "mouse" || ev.button !== 0) return;
    if ((ev.target as HTMLElement).closest("button, input, [role=menu]")) return;
    start.current = { entry, row: ev.currentTarget, x: ev.clientX, y: ev.clientY, id: ev.pointerId, on: false };
  };
  useEffect(() => {
    const end = () => {
      start.current = null;
      document.body.classList.remove("files-dragging");
      setDrag(null);
    };
    const move = (ev: PointerEvent) => {
      const s = start.current;
      if (!s || ev.pointerId !== s.id) return;
      if (!s.on && Math.hypot(ev.clientX - s.x, ev.clientY - s.y) < 6) return;
      if (!s.on) {
        s.on = true;
        // Captured once it moves (not on the press, so clicks stay as they were): the release
        // arrives even outside the window.
        try {
          s.row.setPointerCapture(s.id);
        } catch {
          // the pointer is gone; its pointercancel ends the drag
        }
        document.body.classList.add("files-dragging");
        getSelection()?.removeAllRanges();
      }
      setDrag({ entry: s.entry, x: ev.clientX, y: ev.clientY, over: live.current.target(ev.clientX, ev.clientY, s.entry) });
    };
    const up = (ev: PointerEvent) => {
      const s = start.current;
      if (!s || ev.pointerId !== s.id) return;
      const over = s.on ? live.current.target(ev.clientX, ev.clientY, s.entry) : null;
      end();
      if (over) live.current.drop(s.entry, over);
    };
    const key = (ev: KeyboardEvent) => ev.key === "Escape" && start.current?.on && end();
    // Once a row is pressed, no text selection starts from it.
    const select = (ev: Event) => start.current && ev.preventDefault();
    const on: [string, (e: never) => void][] = [["pointermove", move], ["pointerup", up], ["pointercancel", end], ["keydown", key], ["selectstart", select]];
    for (const [n, f] of on) window.addEventListener(n, f as EventListener);
    return () => {
      for (const [n, f] of on) window.removeEventListener(n, f as EventListener);
      document.body.classList.remove("files-dragging");
    };
  }, []);
  const ghost =
    drag &&
    createPortal(
      <div className="files-ghost" style={{ left: drag.x + 14, top: drag.y + 14 }} aria-hidden="true" dir="ltr">
        <Icon name={drag.entry.dir ? "folder" : "file"} />
        <span>{drag.entry.name}</span>
      </div>,
      document.body,
    );
  return { onPointerDown, over: drag?.over ?? null, dragging: drag?.entry ?? null, ghost };
}
