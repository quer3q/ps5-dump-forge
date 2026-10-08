// Checks src/launcher.ts's decisions with a fake fetch and clock, and the http build's AppCache
// manifest, with a plain `node scripts/check-launcher.mjs` (type stripping) after
// `npm run build:http`.

import assert from "node:assert/strict";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { bundleFiles, MANIFEST, manifestText } from "./appcache.mjs";

const L = await import("../src/launcher.ts");

// ---- a fake console: the session's answers, elfldr's answers per path, a fake clock ----
function console5({ session, loader }) {
  const d = { now: 0, sessions: 0, loads: [], inits: [] };
  d.deps = {
    now: () => d.now,
    sleep: async (ms) => void (d.now += ms),
    nonce: () => `n${d.loads.length}`,
    fetch: async (url, init) => {
      if (url === "/api/session") {
        d.sessions++;
        assert.equal(init.cache, "no-store");
        const s = session(d, init);
        if (s instanceof Promise) return s;
        if (s === null) throw new TypeError("connection refused");
        return new Response(typeof s === "string" ? s : JSON.stringify(s), { status: 200 });
      }
      const u = new URL(url);
      assert.equal(u.origin, "http://127.0.0.1:9021");
      assert.equal(u.searchParams.get("args"), "serve");
      assert.equal(u.searchParams.get("pipe"), "0");
      assert.ok(u.searchParams.get("n"), "a nonce");
      assert.deepEqual(Object.keys(init).sort(), ["cache", "credentials", "mode", "signal"], "no headers, no body");
      assert.equal(init.mode, "cors");
      assert.equal(init.credentials, "omit");
      assert.equal(init.cache, "no-store");
      d.loads.push(u.pathname);
      d.inits.push(init);
      const r = loader(u.pathname, d);
      if (r === null) throw new TypeError("connection refused");
      return r instanceof Response || r instanceof Promise ? r : new Response(r, { status: 200 });
    },
  };
  return d;
}
const up = { app: "ps5-dump-forge", version: "x", platform: "ps5", instance: "i", stopping: false };
const MISSING = "[elfldr.elf] Error reading HTTP payload";
const run = async (d, host = "127.0.0.1", port = "8095") => {
  let starting = 0;
  const out = await L.launchOnce(host, port, d.deps, () => starting++);
  return { out, starting };
};

// Session answers: the app, nothing started.
let d = console5({ session: () => up, loader: () => assert.fail("elfldr asked") });
let r = await run(d);
assert.deepEqual(r.out, { ok: true, up: true });
assert.equal(r.starting, 0);
console.log("session up -> app, no launch");

// Session down: elfldr asked for the saved copy, then the server comes up on the 3rd poll.
let polls = 0;
d = console5({ session: (x) => (x.loads.length && ++polls >= 3 ? up : null), loader: () => "" });
r = await run(d);
assert.deepEqual(r.out, { ok: true, up: true });
assert.equal(r.starting, 1);
assert.deepEqual(d.loads, ["/data/ps5-dump-forge/ps5-dump-forge.elf"]);
assert.equal(d.now, 2000, "polled every second");
console.log("session down -> elfldr /data copy -> session up -> app");

// "Error reading": the USB drives in order, usb0..usb2, then usb2 is the one.
d = console5({
  session: (x) => (x.loads.length === 4 ? up : null),
  loader: (p) => (p === "/mnt/usb2/ps5-dump-forge.elf" ? "" : MISSING),
});
r = await run(d);
assert.deepEqual(r.out, { ok: true, up: true });
assert.deepEqual(d.loads, ["/data/ps5-dump-forge/ps5-dump-forge.elf", "/mnt/usb0/ps5-dump-forge.elf", "/mnt/usb1/ps5-dump-forge.elf", "/mnt/usb2/ps5-dump-forge.elf"]);
console.log("Error reading -> /mnt/usb0..7 in order");

// No copy anywhere.
d = console5({ session: () => null, loader: () => MISSING });
r = await run(d);
assert.deepEqual(r.out, { ok: false, failure: "missing", message: L.MESSAGES.missing });
assert.equal(d.loads.length, 9);
assert.equal(d.loads.at(-1), "/mnt/usb7/ps5-dump-forge.elf");
assert.match(L.MESSAGES.missing, /No saved copy of PS5 Dump Forge was found/);
console.log("no copy -> missing message");

// elfldr unreachable.
d = console5({ session: () => null, loader: () => null });
r = await run(d);
assert.deepEqual(r.out, { ok: false, failure: "loader", message: L.MESSAGES.loader });
assert.deepEqual(d.loads, ["/data/ps5-dump-forge/ps5-dump-forge.elf"]);
assert.match(L.MESSAGES.loader, /Can't reach the payload loader on port 9021/);
console.log("9021 unreachable -> loader message");

// Started, never answers: gives up at 30 s of (fake) wall clock.
d = console5({ session: () => null, loader: () => "" });
r = await run(d);
assert.deepEqual(r.out, { ok: false, failure: "timeout", message: L.MESSAGES.timeout });
assert.equal(d.now, 30000);
assert.equal(d.sessions, 1 + 1 + 31, "first check, re-check, 31 polls over 30 s");
console.log(`no answer -> timeout after ${d.now / 1000} s (${d.sessions} session calls)`);

// A PC on the LAN never launches, whatever the session says.
for (const host of ["192.168.1.20", "ps5.local", "127.0.0.2"]) {
  d = console5({ session: () => assert.fail("session asked"), loader: () => assert.fail("elfldr asked") });
  r = await run(d, host);
  assert.deepEqual(r.out, { ok: true, up: false });
  assert.equal(r.starting, 0);
}
assert.ok(L.isLoopback("localhost") && L.isLoopback("127.0.0.1"));
console.log("non-loopback host -> app as is, never launches");

// Another port (`serve --port 9000`, or none): a relaunch would listen on 8095, so never.
for (const port of ["9000", "", "80"]) {
  d = console5({ session: () => assert.fail("session asked"), loader: () => assert.fail("elfldr asked") });
  r = await run(d, "127.0.0.1", port);
  assert.deepEqual(r.out, { ok: true, up: false });
  assert.equal(r.starting, 0);
}
assert.equal(L.DEFAULT_PORT, "8095");
console.log("loopback on another port -> app as is, never launches");

// Something else answers (another program, a 503 while busy): the app's business, no launch.
d = console5({ session: () => ({ app: "other" }), loader: () => assert.fail("elfldr asked") });
assert.deepEqual((await run(d)).out, { ok: true, up: false });
d = console5({ session: () => "not json", loader: () => assert.fail("elfldr asked") });
assert.deepEqual((await run(d)).out, { ok: true, up: false });
console.log("someone else answering -> no launch");

// A Stop still cleaning up: wait until it's gone, then launch.
let gone = false;
d = console5({
  session: (x) => (x.loads.length ? up : gone ? null : ((gone = x.sessions >= 3), { ...up, stopping: true })),
  loader: () => "",
});
r = await run(d);
assert.deepEqual(r.out, { ok: true, up: true });
assert.equal(d.loads.length, 1);
console.log("stopping -> waits until gone -> launch");

// One launch per page: a second call while one runs shares it.
let release;
const gate = new Promise((ok) => (release = ok));
d = console5({ session: (x) => (x.loads.length ? up : null), loader: () => gate.then(() => new Response("")) });
const a = L.launchOnce("127.0.0.1", "8095", d.deps, () => {});
await new Promise((ok) => setTimeout(ok, 0));
assert.ok(L.launching());
const b = L.launchOnce("127.0.0.1", "8095", d.deps, () => {});
release();
assert.deepEqual(await a, await b);
assert.equal(d.loads.length, 1, "elfldr asked once");
assert.ok(!L.launching());
console.log("concurrent launches -> one elfldr request");

// Timeouts (the real timers, shortened): a session that hangs counts as down; elfldr headers
// with a body that never ends counts as a requested start.
const realSetTimeout = globalThis.setTimeout;
globalThis.setTimeout = (f, ms, ...rest) =>
  realSetTimeout(f, ms === L.SESSION_TIMEOUT_MS || ms === L.LOADER_TIMEOUT_MS ? 5 : ms, ...rest);
const hang = (signal) => new Promise((_, no) => signal.addEventListener("abort", () => no(new DOMException("aborted", "AbortError"))));
let hung = 0;
d = console5({
  session: (x, init) => (x.loads.length && x.sessions > 3 ? up : (hung++, hang(init.signal))),
  loader: () => new Response(new ReadableStream({ start() {} })),
});
r = await run(d);
globalThis.setTimeout = realSetTimeout;
assert.deepEqual(r.out, { ok: true, up: true });
assert.ok(hung >= 2 && d.loads.length === 1);
console.log("hanging session -> down; elfldr body never ending -> start requested");

// ---- the AppCache manifest of dist-http ----
const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "../dist-http");
assert.ok(existsSync(join(dist, MANIFEST)), "npm run build:http first");
const text = readFileSync(join(dist, MANIFEST), "utf8");
assert.equal(text, manifestText(dist), "the manifest matches the bundle on disk");
const lines = text.split("\n");
assert.equal(lines[0], "CACHE MANIFEST");
assert.match(lines[1], /^# [0-9a-f]{64}$/);
const cached = lines.slice(lines.indexOf("CACHE:") + 1, lines.indexOf("NETWORK:")).filter(Boolean);
const files = bundleFiles(dist);
assert.ok(files.includes("index.html") && files.some((f) => f.startsWith("assets/")));
assert.deepEqual(cached, ["/", ...files], "/ and every file of dist-http");
assert.ok(!cached.includes(MANIFEST));
assert.deepEqual(lines.slice(lines.indexOf("NETWORK:"), lines.indexOf("NETWORK:") + 2), ["NETWORK:", "*"]);
assert.match(readFileSync(join(dist, "index.html"), "utf8"), /<html manifest="forge\.appcache"/);
const tauri = join(here, "../dist/index.html");
if (existsSync(tauri)) assert.doesNotMatch(readFileSync(tauri, "utf8"), /manifest=/, "the Tauri page has no manifest");
// Any changed byte, in any file, changes the manifest.
const tmp = mkdtempSync(join(tmpdir(), "forge-appcache-"));
try {
  cpSync(dist, tmp, { recursive: true });
  for (const f of files) {
    const p = join(tmp, f);
    const bytes = readFileSync(p);
    const flipped = Buffer.from(bytes);
    flipped[flipped.length >> 1] ^= 1;
    writeFileSync(p, flipped);
    assert.notEqual(manifestText(tmp), text, `a byte of ${f}`);
    writeFileSync(p, bytes);
  }
  assert.equal(manifestText(tmp), text);
  writeFileSync(join(tmp, "assets/extra.js"), "");
  assert.ok(manifestText(tmp).includes("\nassets/extra.js\n") && manifestText(tmp) !== text, "a new file");
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
console.log(`appcache: ok (${files.length} files + /, NETWORK *, any byte changes the hash; Tauri page has none)`);
