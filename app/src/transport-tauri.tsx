// The desktop app's transport: Tauri IPC, Tauri events and the native file dialog.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

import { basename } from "./paths";
import type { DropEvent, PickOptions, Transport, Unlisten } from "./transport";

async function pick(o: PickOptions): Promise<string | null> {
  const p = o.directory
    ? await open({ directory: true, title: o.title, defaultPath: o.start })
    : await open({
        title: o.title,
        defaultPath: o.start,
        filters: o.extensions ? [{ name: o.filterName ?? o.title, extensions: o.extensions }] : undefined,
      });
  return typeof p === "string" ? p : null;
}

async function pickFiles(o: PickOptions): Promise<string[] | null> {
  const p = await open({
    multiple: true,
    title: o.title,
    defaultPath: o.start,
    filters: o.extensions ? [{ name: o.filterName ?? o.title, extensions: o.extensions }] : undefined,
  });
  return Array.isArray(p) && p.length > 0 ? p : null;
}

/** The webview's drag and drop (Tauri 2 keeps it on, so the page sees no HTML5 file drops).
 * The page wants CSS pixels. Tauri labels the position physical, but only WebView2 (Windows)
 * reports device pixels: wry passes macOS's NSView points and GTK's logical units through
 * unscaled, so dividing those by the pixel ratio put a Retina drop at half its place. */
const PHYSICAL_DROPS = /Windows/.test(navigator.userAgent);
function onDrop(f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten {
  let stop: Unlisten | null = null;
  let gone = false;
  void getCurrentWebview()
    .onDragDropEvent(({ payload: e }) => {
      if (e.type === "leave") return over?.(null);
      const r = PHYSICAL_DROPS ? window.devicePixelRatio || 1 : 1;
      const at = { x: e.position.x / r, y: e.position.y / r };
      if (e.type !== "drop") return over?.(at);
      over?.(null);
      const items = e.paths.map((path) => ({ name: basename(path), path }));
      if (items.length > 0) f({ ...at, items });
    })
    .then((u) => (gone ? u() : (stop = u)));
  return () => {
    gone = true;
    stop?.();
  };
}

export const transport: Transport = {
  web: false,
  call: (cmd, args) => invoke(cmd, args),
  listen: <T,>(event: string, f: (payload: T) => void): Promise<Unlisten> =>
    // The restore event is the http poller's; nothing in the app sends it.
    event === "jobs://restore" ? Promise.resolve(() => {}) : listen<T>(event, (e) => f(e.payload)),
  pick,
  pickFiles,
  hostTree: (path) => invoke("host_tree", { path }),
  useOffline: () => false,
  address: () => null,
  platform: () => Promise.resolve("host"),
  lz4Patch: (path) => invoke("lz4_patch", { path }),
  lz4Unpatch: (path) => invoke("lz4_unpatch", { path }),
  updateTarget: () => invoke("update_target"),
  startUpdate: (version) => invoke("open_update", { version }),
  onDrop,
  // A drop here is a host path: an `add` takes it as it is.
  stage: () => Promise.reject("Uploads are for the web page only."),
  discardStaged: () => Promise.resolve(),
};
