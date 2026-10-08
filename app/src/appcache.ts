// The http build's AppCache (`<html manifest="forge.appcache">`, written by vite.config.ts):
// it keeps the page in the PS5 browser while nothing listens, so the tile can start the
// payload (launcher.ts). A newer payload serves a new manifest; once the server answers, the
// page asks for an update, then swaps it in and reloads once, while no dialog is open and no
// field has focus.

/** The bits of the old `window.applicationCache` used here (gone from modern lib types). */
interface AppCache {
  readonly status: number;
  update(): void;
  swapCache(): void;
  addEventListener(type: "updateready", f: () => void): void;
}

const UPDATEREADY = 4;

function cache(): AppCache | null {
  try {
    return (window as unknown as { applicationCache?: AppCache }).applicationCache ?? null;
  } catch {
    return null;
  }
}

let ready = false;
let reloading = false;
let canReload: () => boolean = () => true;

function tryReload(): void {
  if (!ready || reloading) return;
  if (!canReload()) {
    setTimeout(tryReload, 2000);
    return;
  }
  reloading = true;
  try {
    cache()?.swapCache();
  } catch {
    // Already swapped, or obsolete: the reload loads what the browser has.
  }
  location.reload();
}

/** Run first thing: a download that finished before this script ran, or one later. */
export function watchUpdates(): void {
  const c = cache();
  if (!c) return;
  const onReady = () => {
    ready = true;
    tryReload();
  };
  try {
    c.addEventListener("updateready", onReady);
    if (c.status === UPDATEREADY) onReady();
  } catch {
    // Not there in this browser.
  }
}

/** When a reload may happen now (the launcher and the app each say). */
export function setCanReload(f: () => boolean): void {
  canReload = f;
}

/** After a healthy session: fetch the payload's manifest now. */
export function checkForUpdate(): void {
  try {
    cache()?.update();
  } catch {
    // No cache yet (first load) or a check already running.
  }
}
