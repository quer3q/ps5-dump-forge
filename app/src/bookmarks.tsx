// Bookmarked folders (issue #18): any device's, newest last, at most 9, under the old shortcuts' key;
// unreadable storage is none. One store: every pane showing them updates together.

import { useLayoutEffect, useRef, useSyncExternalStore } from "react";

import "./devices.css";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { SEPARATORS } from "./paths";

const KEY = "forge.shortcuts";
export const MAX_BOOKMARKS = 9;

const isNet = (p: string) => /^(smb|ftp|nfs|flink):\/\//i.test(p);
const within = (p: string, root: string) => p === root || p.startsWith(`${root}/`);

export function lastPart(path: string): string {
  const parts = path.split(isNet(path) ? /\// : SEPARATORS).filter(Boolean);
  return parts.length > (isNet(path) ? 2 : 0) ? parts[parts.length - 1] : path;
}

function read(): string[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (!Array.isArray(v)) return [];
    const paths = v.filter((p): p is string => typeof p === "string" && p !== "");
    const kept = paths.filter((p, i) => paths.lastIndexOf(p) === i).slice(-MAX_BOOKMARKS);
    // An older build kept up to 20: the stored list is cut to the newest 9 too.
    if (kept.length !== v.length) write(kept);
    return kept;
  } catch {
    return [];
  }
}

function write(paths: string[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(paths));
  } catch {
    // Storage blocked or full: the bookmarks last until the page closes.
  }
}

let list: string[] | null = null;
const listeners = new Set<() => void>();
const get = () => (list ??= read());
const notify = () => listeners.forEach((f) => f());
function save(next: string[]) {
  list = next;
  write(next);
  notify();
}
// Another window of this page changed them.
const onStorage = (e: StorageEvent) => {
  if (e.key !== KEY) return;
  list = read();
  notify();
};
function subscribe(f: () => void) {
  if (listeners.size === 0) window.addEventListener("storage", onStorage);
  listeners.add(f);
  return () => {
    listeners.delete(f);
    if (listeners.size === 0) window.removeEventListener("storage", onStorage);
  };
}

export function useBookmarks(): string[] {
  return useSyncExternalStore(subscribe, get);
}

/** Whether a bookmark shows in a pane on `root`: null is this device, else a path within that root. */
export const reachable = (p: string, root: string | null) => (root === null ? !isNet(p) : within(p, root));

/** A pane's bookmarks as chips: the last name (`root` itself `rootName`), the folder shown bold; x removes. */
export function Bookmarks({
  root,
  rootName,
  here,
  onOpen,
}: {
  root: string | null;
  rootName?: string;
  here: string | null;
  onOpen: (path: string) => void;
}) {
  const t = useMessages().bookmarks;
  const shown = useBookmarks().filter((p) => reachable(p, root));
  const box = useRef<HTMLDivElement>(null);
  // After an x, focus goes to the chip that took its place (else the last one).
  const refocus = useRef<number | null>(null);
  useLayoutEffect(() => {
    const i = refocus.current;
    if (i === null) return;
    refocus.current = null;
    const xs = box.current?.querySelectorAll<HTMLElement>(".bm-x");
    if (xs && xs.length > 0) xs[Math.min(i, xs.length - 1)].focus();
  });
  if (shown.length === 0) return null;
  return (
    <div className="bm-strip" role="group" aria-label={t.label} ref={box}>
      {shown.map((path, i) => {
        const name = root !== null && path === root ? (rootName ?? path) : lastPart(path);
        const on = path === here;
        return (
          <span key={path} className={on ? "bm-chip here" : "bm-chip"}>
            <button type="button" className="bm-go" title={path} aria-current={on ? "location" : undefined} onClick={() => onOpen(path)}>
              <Icon name="folder" />
              <span className="bm-name">{name}</span>
            </button>
            <button
              type="button"
              className="bm-x"
              aria-label={t.removeNamed(name)}
              title={t.remove}
              onClick={() => {
                refocus.current = i;
                save(get().filter((p) => p !== path));
              }}
            >
              <Icon name="x" />
            </button>
          </span>
        );
      })}
    </div>
  );
}

/** Bookmarks the folder shown; at 9 a new one replaces the oldest `root` can't reach, else off.
 * ponytail: the one replaced goes without a word. */
export function BookmarkToggle({ path, root }: { path: string | null; root: string | null }) {
  const t = useMessages().bookmarks;
  const all = useBookmarks();
  const pinned = path !== null && all.includes(path);
  const hidden = all.find((p) => !reachable(p, root));
  const full = all.length >= MAX_BOOKMARKS && hidden === undefined;
  // aria-disabled, not disabled: the 9th keeps focus on the button.
  const off = path === null || (!pinned && full);
  const click = () => {
    if (path === null) return;
    if (pinned) return save(all.filter((p) => p !== path));
    if (full) return;
    const kept = all.length >= MAX_BOOKMARKS ? all.filter((p) => p !== hidden) : all;
    save([...kept, path]);
  };
  return (
    <button
      type="button"
      className="bm-toggle"
      aria-pressed={pinned}
      aria-label={pinned ? t.remove : t.add}
      title={pinned ? t.remove : !off ? t.add : path !== null ? t.full : undefined}
      aria-disabled={off || undefined}
      onClick={click}
    >
      <Icon name="bookmark" />
    </button>
  );
}
