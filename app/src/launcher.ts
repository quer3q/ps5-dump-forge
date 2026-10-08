// The http build's launcher: the home-screen tile opens http://127.0.0.1:8095/ in the PS5
// browser, which shows this page from AppCache even when nothing listens (after a restart).
// Before the app loads, the page asks `GET /api/session`; if nothing answers, it asks elfldr
// (port 9021) to start the copy the payload saved of itself, then waits for the server.
// Only at page load and only on the console itself (a loopback host, port 8095): a PC on the LAN, the
// app's Stop and its poller never start anything. Type imports only and every dependency
// injected, so a plain `node` can drive it (scripts/check-launcher.mjs).

/** What the launcher needs from the page; `browserDeps()` is the real one. */
export interface Deps {
  fetch: typeof fetch;
  sleep(ms: number): Promise<void>;
  now(): number;
  nonce(): string;
}

export type Failure = "loader" | "missing" | "timeout";

/** `up`: the session answered healthy (an AppCache update check is worth it). */
export type Outcome = { ok: true; up: boolean } | { ok: false; failure: Failure; message: string };

export const ELFLDR = "http://127.0.0.1:9021";
/** Where the payload saves itself, then the USB drives, in this order. */
export const COPIES = [
  "/data/ps5-dump-forge/ps5-dump-forge.elf",
  ...[0, 1, 2, 3, 4, 5, 6, 7].map((i) => `/mnt/usb${i}/ps5-dump-forge.elf`),
];

export const SESSION_TIMEOUT_MS = 3000;
export const LOADER_TIMEOUT_MS = 10000;
export const POLL_MS = 1000;
export const START_WAIT_MS = 30000;

export const MESSAGES: Record<Failure, string> = {
  loader:
    "Can't reach the payload loader on port 9021. Run your autoloader (jailbreak) first, then open PS5 Dump Forge again.",
  missing:
    "No saved copy of PS5 Dump Forge was found (in /data/ps5-dump-forge/ or on a USB drive). Load it once from a PC; it saves a copy for this tile.",
  timeout:
    "PS5 Dump Forge was started but hasn't answered after 30 seconds. It may still be starting (a console waking from rest mode is slow).",
};

const fail = (failure: Failure): Outcome => ({ ok: false, failure, message: MESSAGES[failure] });

/** The tile's page: only the console's own browser may start the payload. */
export function isLoopback(hostname: string): boolean {
  return hostname === "127.0.0.1" || hostname === "localhost";
}

/** The port elfldr's `args=serve` start listens on. */
export const DEFAULT_PORT = "8095";

export function browserDeps(): Deps {
  return {
    fetch: (input, init) => fetch(input, init),
    sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    now: () => Date.now(),
    nonce: () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`,
  };
}

/** A fetch plus reading its body as text, all within `ms`. Rejects on a network failure or
 * the deadline; `headers` says whether a response arrived before it. */
async function fetchText(
  d: Deps,
  url: string,
  init: RequestInit,
  ms: number,
): Promise<{ status: number; text: string } | { headers: boolean }> {
  const abort = new AbortController();
  let headers = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      abort.abort();
      reject(new Error("timeout"));
    }, ms);
  });
  try {
    return await Promise.race([
      (async () => {
        const res = await d.fetch(url, { ...init, signal: abort.signal });
        headers = true;
        return { status: res.status, text: await res.text() };
      })(),
      deadline,
    ]);
  } catch {
    return { headers };
  } finally {
    clearTimeout(timer);
  }
}

export type SessionState = "up" | "stopping" | "down" | "other";

/** `GET /api/session`: ours and running, ours and quitting, nothing answering, or something
 * else answering. */
export async function session(d: Deps): Promise<SessionState> {
  const r = await fetchText(d, "/api/session", { cache: "no-store" }, SESSION_TIMEOUT_MS);
  if (!("status" in r)) return "down";
  try {
    const s = JSON.parse(r.text) as { app?: unknown; stopping?: unknown };
    if (r.status !== 200 || s?.app !== "ps5-dump-forge") return "other";
    return s.stopping === false ? "up" : "stopping";
  } catch {
    return "other";
  }
}

/** Polls the session every second until `want` holds, up to `START_WAIT_MS`. */
async function waitFor(d: Deps, want: (s: SessionState) => boolean): Promise<boolean> {
  const end = d.now() + START_WAIT_MS;
  for (;;) {
    if (want(await session(d))) return true;
    if (d.now() + POLL_MS > end) return false;
    await d.sleep(POLL_MS);
  }
}

/** Asks elfldr to start each saved copy in turn. A clean reply means a start was requested
 * (not that it worked); "Error reading" means that copy isn't there. */
async function requestStart(d: Deps): Promise<Outcome> {
  for (const path of COPIES) {
    const url = `${ELFLDR}${path}?args=serve&pipe=0&n=${encodeURIComponent(d.nonce())}`;
    const r = await fetchText(d, url, { mode: "cors", credentials: "omit", cache: "no-store" }, LOADER_TIMEOUT_MS);
    if (!("status" in r)) {
      // Headers came but the body never ended: elfldr took it and is piping the payload.
      if (r.headers) return { ok: true, up: false };
      return fail("loader");
    }
    if (!r.text.includes("Error reading")) return { ok: true, up: false };
  }
  return fail("missing");
}

/** The whole page-load check. `onStarting` runs once nothing answered, before elfldr is
 * asked (the page shows "Starting PS5 Dump Forge…"). */
export async function launch(d: Deps, onStarting: () => void): Promise<Outcome> {
  let s = await session(d);
  // Anything that answers but isn't ours running is the app's business (its offline notice).
  if (s === "up" || s === "other") return { ok: true, up: s === "up" };
  onStarting();
  // A Stop still cleaning up: a second copy now would only find the port taken and exit.
  if (s === "stopping" && !(await waitFor(d, (x) => x === "down" || x === "up"))) return fail("timeout");
  s = await session(d);
  if (s === "up") return { ok: true, up: true };
  const asked = await requestStart(d);
  if (!asked.ok) return asked;
  return (await waitFor(d, (x) => x === "up")) ? { ok: true, up: true } : fail("timeout");
}

let inFlight: Promise<Outcome> | null = null;

/** At most one launch per page at a time; Retry starts a new one once it settled. A host
 * that isn't loopback (a PC on the LAN), or a port other than 8095 (`serve --port`, which a
 * relaunch with `args=serve` would never reach), never launches: it gets the app as it is. */
export function launchOnce(hostname: string, port: string, d: Deps, onStarting: () => void): Promise<Outcome> {
  if (!isLoopback(hostname) || port !== DEFAULT_PORT) return Promise.resolve({ ok: true, up: false });
  if (!inFlight) inFlight = launch(d, onStarting).finally(() => (inFlight = null));
  return inFlight;
}

export function launching(): boolean {
  return inFlight !== null;
}
