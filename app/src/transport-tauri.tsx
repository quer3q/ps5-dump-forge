// The desktop app's transport: Tauri IPC, Tauri events and Forge's own picker over `list_dir`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { basename } from "./paths";
import { picker, type Listing, type SavedDevices } from "./Picker";
import type { DropEvent, Transport, Unlisten } from "./transport";

/** This device's roots in the picker: the home folder and the volume folders there are (Windows:
 * each drive C: to Z:), checked for existence only by `local_roots`; a root lists when opened. */
async function localRoots(): Promise<Listing> {
  const [home, vols] = await Promise.all([invoke<string | null>("home_folder"), invoke<string[]>("local_roots")]);
  const paths = home === null ? vols : [home, ...vols];
  return { path: null, parent: null, entries: paths.map((p) => ({ name: basename(p) || p, path: p, dir: true, size: null })) };
}

/** Forge's picker: this device's folders and, while another device is online, a tab for each. */
const { pick, pickFiles } = picker({
  list: (path) => (path === null ? localRoots() : invoke("list_dir", { path })),
  saved: () => invoke<SavedDevices>("connect_state"),
});

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
    .then((u) => (gone ? u() : (stop = u)))
    .catch((e) => console.error("drag and drop unavailable", e));
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
  debug: () => false,
  platform: () => Promise.resolve("host"),
  notifications: () => Promise.resolve(null),
  setNotifications: () => Promise.resolve(null),
  updateTarget: () => invoke("update_target"),
  startUpdate: (version) => invoke("open_update", { version }),
  onDrop,
  // A drop here is a host path: an `add` takes it as it is.
  stage: () => Promise.reject("Uploads are for the web page only."),
  discardStaged: () => Promise.resolve(),
  connect: (cmd, args) => invoke(`connect_${cmd}`, args),
  connectUpload: () => Promise.reject("uploads come from the http build only"),
  connectDownloadUrl: () => null,
  packageScan: () => Promise.reject("packages install only on a PS5"),
  connectInstall: (keyId, path) => invoke("connect_install", { keyId, path }),
  connectSpeedTest: (keyId, run) => invoke("connect_speed_test", { keyId, run }),
  connectSpeedTestCancel: (keyId, run) => invoke("connect_speed_test_cancel", { keyId, run }),
};
