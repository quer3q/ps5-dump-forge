// The http build's transport (`npm run build:http`): a browser on the LAN talking to
// `ps5-dump-forge serve`. No Tauri code reaches this bundle (vite.config.ts aliases
// `forge-transport` here).

import { useSyncExternalStore } from "react";

import { reloadForUpdate } from "./appcache";
import { assetName } from "./update";
import {
  call,
  connectCall,
  connectDownloadUrl,
  connectUpload,
  getStatus,
  listen,
  selfUpdate,
  serverAddress,
  serverDebug,
  serverNotifications,
  serverPlatform,
  serverUpdateTarget,
  setServerNotifications,
  upload,
  watchStatus,
} from "./http-client";
import { setServerSeparator } from "./paths";
import { picker, type SavedDevices } from "./Picker";
import type { SpeedTest } from "./api";
import type { DropEvent, DropItem, Transport, Unlisten } from "./transport";

const { pick, pickFiles } = picker({
  list: (path) => call("list_dir", { path }),
  saved: () => connectCall<SavedDevices>("state"),
});

// Paths are the server's: `/` until its session says otherwise (main-http.tsx asks it before
// the app renders), never the viewer's OS. The poller starts from main-http.tsx too.
setServerSeparator("/");

const hasFiles = (e: DragEvent) => !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");

/** HTML5 file drops on the whole page. The items are read inside the drop event (the browser
 * empties `dataTransfer` after it); folders keep their entry for walking later. */
function onDrop(f: (d: DropEvent) => void, over?: (at: { x: number; y: number } | null) => void): Unlisten {
  // dragenter/dragleave fire for every child crossed; the window is left when they balance.
  let depth = 0;
  const enter = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth++;
  };
  const move = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault(); // allows the drop (else the browser opens the file)
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    over?.({ x: e.clientX, y: e.clientY });
  };
  const leave = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) over?.(null);
  };
  const drop = (e: DragEvent) => {
    if (!hasFiles(e) || !e.dataTransfer) return;
    e.preventDefault();
    depth = 0;
    over?.(null);
    const items: DropItem[] = [];
    for (const it of Array.from(e.dataTransfer.items)) {
      if (it.kind !== "file") continue;
      const entry = it.webkitGetAsEntry?.() ?? undefined;
      const file = entry?.isDirectory ? undefined : (it.getAsFile() ?? undefined);
      const name = entry?.name ?? file?.name;
      if (name) items.push({ name, file, entry });
    }
    // A browser without `items`: the plain file list (no folders).
    if (e.dataTransfer.items.length === 0)
      for (const file of Array.from(e.dataTransfer.files)) items.push({ name: file.name, file });
    if (items.length > 0) f({ x: e.clientX, y: e.clientY, items });
  };
  const on: [string, (e: DragEvent) => void][] = [["dragenter", enter], ["dragover", move], ["dragleave", leave], ["drop", drop]];
  for (const [t, h] of on) document.addEventListener(t, h as EventListener);
  return () => {
    for (const [t, h] of on) document.removeEventListener(t, h as EventListener);
  };
}

export const transport: Transport = {
  web: true,
  call,
  listen,
  pick,
  pickFiles,
  hostTree: (path) => call("host_tree", { path }),
  useOffline: () => useSyncExternalStore(watchStatus, getStatus).offline,
  address: serverAddress,
  debug: serverDebug,
  platform: serverPlatform,
  notifications: serverNotifications,
  setNotifications: setServerNotifications,
  updateTarget: serverUpdateTarget,
  // The PS5: the new version serves a new bundle: AppCache swaps it in and reloads (appcache.ts,
  // when no dialog is open); otherwise the page just reloads. A computer: the CLI's file opens
  // in a new tab, as the app opens its own.
  startUpdate: async (version) => {
    if ((await serverPlatform()) !== "ps5") {
      const t = await serverUpdateTarget();
      if (!t) throw new Error("This server offers no update");
      const url = `https://github.com/quer3q/ps5-dump-forge/releases/download/v${version}/${assetName(version, t.asset)}`;
      // Not "noopener": with it window.open always returns null, and a blocked popup must show.
      const w = window.open(url, "_blank");
      if (!w) throw new Error(`The browser blocked the download: open ${url}`);
      w.opener = null;
      return;
    }
    await selfUpdate(version);
    reloadForUpdate();
  },
  onDrop,
  stage: upload,
  discardStaged: (source) => call<null>("edit_discard", { source }).then(() => undefined),
  connect: connectCall,
  connectUpload,
  connectDownloadUrl,
  packageScan: () => call("package_scan", {}),
  connectInstall: (keyId, path) => connectCall<{ job: number }>("install", { key_id: keyId, path }).then((r) => r.job),
  connectSpeedTest: (keyId, run) => connectCall<SpeedTest>("speed_test", { key_id: keyId, run }),
  connectSpeedTestCancel: (keyId, run) =>
    connectCall<null>("speed_test_cancel", { key_id: keyId, run }).then(() => undefined),
};
