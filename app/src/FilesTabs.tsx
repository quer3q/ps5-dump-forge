// The File explorer's device tabs and drags; `useRowDrag` is shared with the Transfer screen.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";

import type { ConnectState, DirEntry } from "./api";
import { Icon, type IconName } from "./icons";
import { nasRoot } from "./Picker";
import { useConnectState } from "./useConnectState";

/** The tab list's prefix; `LOCAL` is the PS5's own tab (device ids hold `://`, so it can't clash). */
export const FILES_TABS = "files-dev";
export const LOCAL = "local";

export interface FilesDevice {
  id: string;
  name: string;
  root: string;
  icon: IconName;
  tag?: string;
  online: boolean;
  /** The root lists shares or exports (SMB, NFS): nothing there can be changed. */
  fixedRoot: boolean;
  /** A NAS: Move to's picker offers NAS devices, not only peers. */
  nas: boolean;
}

/** A device's tab icon by platform. ponytail: no console or computer icon yet. */
export const platformIcon = (platform: string): IconName => (platform === "ps5" ? "disc" : "finder");

/** Every saved device and whether Connect has answered; the last good state stays while a poll fails. */
export function useFilesDevices(): { devices: FilesDevice[]; loaded: boolean } {
  const snap = useConnectState();
  const last = useRef<ConnectState | null>(null);
  if (snap.state) last.current = snap.state;
  const state = last.current;
  const devices: FilesDevice[] = [
    ...(state?.devices ?? []).map((d) => {
      const root = `flink://${d.key_id}`;
      return { id: root, name: d.name, root, icon: platformIcon(d.platform), online: d.online, fixedRoot: false, nas: false };
    }),
    ...(state?.nas ?? []).map((n) => {
      const root = nasRoot(n.host, n.port, n.protocol);
      const fixedRoot = n.protocol === "smb" || n.protocol === "nfs";
      return { id: root, name: n.name, root, icon: "drive" as const, tag: n.protocol.toUpperCase(), online: n.online, fixedRoot, nas: true };
    }),
  ];
  return { devices, loaded: state !== null || snap.error !== null };
}

/** A row dragged by the mouse (pointer events: Tauri's drag and drop takes HTML5's on Windows).
 * ponytail: mouse only; touch scrolls, and Move to does it there. */
export function useRowDrag<T>(opts: {
  target: (x: number, y: number, e: DirEntry) => T | null;
  drop: (e: DirEntry, to: T) => void;
  /** The ghost's text; the row's name if absent. */
  label?: (e: DirEntry) => string;
}) {
  const [drag, setDrag] = useState<{ entry: DirEntry; x: number; y: number; over: T | null } | null>(null);
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
        // Captured once it moves (not on the press, so clicks stay as they were): the release arrives even outside the window.
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
        <span>{live.current.label?.(drag.entry) ?? drag.entry.name}</span>
      </div>,
      document.body,
    );
  return { onPointerDown, over: drag?.over ?? null, dragging: drag?.entry ?? null, ghost };
}
