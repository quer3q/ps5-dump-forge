// Development only: drives dist-http and src/http-client.ts against the real
// `ps5-dump-forge serve` (built here with cargo, so it embeds the current dist-http) with
// plain node (no browser), on a temp root holding a small game: the bundle's files, a few
// parser checks, every route with JSON bodies (no auth: the contract has none), a real job
// (folder -> .exfat) and a cancelled one through the poller, a reload's restore, a server
// restart (the list rebuilt) and quit.
//
//   npm run build:http && node scripts/smoke.mjs      (FORGE_BIN=<serve binary> skips cargo)

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { request as httpRequest } from "node:http";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let bin = process.env.FORGE_BIN;
if (!bin) {
  const built = spawnSync("cargo", ["build", "--release", "-p", "ps5-dump-forge-cli"], { cwd: repo, stdio: "inherit" });
  assert.equal(built.status, 0, "cargo build");
  bin = join(process.env.CARGO_TARGET_DIR ?? join(repo, "target"), "release", "ps5-dump-forge");
}

// ---- a root with a small game, an output folder and a leftover .part ----
const root = realpathSync(mkdtempSync(join(tmpdir(), "forge-smoke-")));
const game = `${root}/homebrew/PPSA01234-app`;
const images = `${root}/images`;
mkdirSync(`${game}/sce_sys`, { recursive: true });
mkdirSync(`${game}/data`);
mkdirSync(images);
writeFileSync(`${game}/eboot.bin`, "\x7fELF fake eboot");
writeFileSync(
  `${game}/sce_sys/param.json`,
  JSON.stringify({ titleId: "PPSA01234", titleName: "Smoke Game", requiredSystemSoftwareVersion: "0x0700000000000000" }),
);
// Big enough that the second job is still queued behind the first when it is cancelled.
writeFileSync(`${game}/data/a.bin`, Buffer.alloc(64 << 20, 7));
writeFileSync(`${images}/Old.exfat.part`, "");

const PORT = await new Promise((ok) => {
  const s = createServer().listen(0, "127.0.0.1", () => {
    const { port } = s.address();
    s.close(() => ok(port));
  });
});
const HOST = `127.0.0.1:${PORT}`;
const ORIGIN = `http://${HOST}`;

let server = null;
let stderr = "";
async function startServer() {
  const p = spawn(bin, ["serve", "--port", String(PORT), "--root", root], { stdio: ["ignore", "inherit", "pipe"] });
  p.exited = new Promise((ok) => p.on("exit", (status, signal) => ok({ status, signal })));
  p.stderr.on("data", (d) => {
    stderr += d;
    process.stderr.write(d);
  });
  let failed = null;
  p.on("error", (e) => (failed = e));
  // Up once it answers.
  for (const end = Date.now() + 30000; Date.now() < end; await sleep(100)) {
    if (failed) throw failed;
    if (await raw("GET", "/api/session").then((r) => r.status === 200, () => false)) return p;
  }
  throw new Error("serve didn't answer");
}
process.on("exit", () => {
  server?.kill("SIGKILL");
  rmSync(root, { recursive: true, force: true });
});

async function waitFor(what, f, ms = 30000) {
  for (const end = Date.now() + ms; Date.now() < end; await sleep(50)) if (f()) return;
  throw new Error(`timed out: ${what}`);
}

/** A raw request (fetch can't set Host): status, headers, parsed JSON body. */
function raw(method, path, headers = {}, body) {
  return new Promise((ok, fail) => {
    const req = httpRequest({ host: "127.0.0.1", port: PORT, method, path, headers }, (res) => {
      let text = "";
      res.on("data", (d) => (text += d));
      res.on("end", () => {
        let data = null;
        try {
          data = JSON.parse(text);
        } catch {}
        ok({ status: res.statusCode, headers: res.headers, data });
      });
    });
    req.on("error", fail);
    req.end(body);
  });
}

server = await startServer();

// ---- the bundle: index.html and its AppCache manifest, then every file the manifest lists ----
const page = await fetch(`${ORIGIN}/`);
assert.equal(page.status, 200);
const html = await page.text();
const refs = [...html.matchAll(/(?:src|href)="([^"]+)"/g)].map((m) => m[1]);
assert.ok(refs.some((r) => r.endsWith(".js")) && refs.some((r) => r.endsWith(".css")), "script and stylesheet");
const manifestRef = html.match(/<html manifest="([^"]+)"/)?.[1];
assert.equal(new URL(manifestRef, `${ORIGIN}/`).pathname, "/forge.appcache");
const man = await fetch(new URL(manifestRef, `${ORIGIN}/`));
assert.equal(man.status, 200);
assert.equal(man.headers.get("content-type"), "text/cache-manifest");
assert.equal(man.headers.get("cache-control"), "no-cache");
const manText = await man.text();
const manLines = manText.split("\n");
const cachedFiles = manLines.slice(manLines.indexOf("CACHE:") + 1, manLines.indexOf("NETWORK:")).filter(Boolean);
for (const r of refs) assert.ok(cachedFiles.includes(new URL(r, `${ORIGIN}/`).pathname.slice(1)), `${r} in the manifest`);
assert.ok(cachedFiles.some((r) => r.endsWith(".png")), "the logo");
for (const r of cachedFiles) {
  const res = await fetch(new URL(r, `${ORIGIN}/`));
  assert.equal(res.status, 200, r);
  const text = await res.text();
  if (r.endsWith(".js")) assert.ok(!/__TAURI|plugin:dialog/.test(text), "no Tauri code in the http bundle");
  console.log(`bundle: ${r} ${res.status} ${res.headers.get("content-type")}`);
}
assert.equal((await fetch(`${ORIGIN}/nope.js`)).status, 404);

// ---- the parser and the API's own replies ----
const json = { "Content-Type": "application/json" };
const checks = [
  ["unknown route", await raw("POST", "/api/nope", json, "{}"), 404],
  ["POST outside /api", await raw("POST", "/index.html", json, "{}"), 404],
  ["Transfer-Encoding", await raw("POST", "/api/list_dir", { ...json, "Transfer-Encoding": "chunked" }, "0\r\n\r\n"), 400],
  ["malformed JSON", await raw("POST", "/api/list_dir", json, "{"), 400],
];
for (const [what, res, want] of checks) {
  assert.equal(res.status, want, what);
  assert.equal(typeof res.data?.error, "string", `JSON error: ${what}`);
}
const sessionRes = await raw("GET", "/api/session");
assert.equal(sessionRes.headers["cache-control"], "no-store", "API replies are never cached");
const session = sessionRes.data;
assert.deepEqual(Object.keys(session).sort(), ["app", "instance", "platform", "self_copy", "separator", "stopping", "version"]);
assert.equal(session.separator, process.platform === "win32" ? "\\" : "/");
assert.equal(session.self_copy, "none"); // a host build carries no copy of itself
assert.equal(session.app, "ps5-dump-forge");
assert.equal(session.platform, "host");
assert.equal(session.stopping, false);
console.log(`parser: ${checks.map(([w, r]) => `${w} ${r.status}`).join(", ")}; session ${JSON.stringify(session)}`);

// ---- a browser: fetch on this origin (recorded) ----
const sent = [];
const realFetch = globalThis.fetch;
globalThis.fetch = (path, init = {}) => {
  sent.push({ method: init.method, path, headers: { ...init.headers } });
  return realFetch(new URL(path, ORIGIN), init);
};

async function page_(tag) {
  const c = await import(`../src/http-client.ts?${tag}`);
  const ev = { restore: [], progress: [], log: [], done: [] };
  for (const k of Object.keys(ev)) await c.listen(k === "restore" ? "jobs://restore" : `job://${k}`, (e) => ev[k].push(e));
  c.start();
  return { c, ev };
}

// First load: the list (empty) arrives whole.
const a = await page_("a");
await waitFor("first restore", () => a.ev.restore.some((e) => e.replace));
assert.deepEqual(a.ev.restore[0], { replace: true, jobs: [] });

// Commands.
const roots = await a.c.call("list_dir", { path: null });
assert.deepEqual(roots, { path: null, parent: null, entries: [{ name: root, path: root, dir: true, size: null }], truncated: false });
const top = await a.c.call("list_dir", { path: root });
assert.equal(top.parent, null);
assert.deepEqual(top.entries.map((e) => e.name), ["homebrew", "images"]);
const imgs = await a.c.call("list_dir", { path: images });
assert.equal(imgs.parent, root);
assert.deepEqual(imgs.entries, [{ name: "Old.exfat.part", path: `${images}/Old.exfat.part`, dir: false, size: 0 }]);
// No confinement: any absolute path lists; a core failure comes back as its error string.
const above = await a.c.call("list_dir", { path: dirname(root) });
assert.ok(above.entries.some((e) => e.path === root && e.dir), "the folder above the root");
await assert.rejects(a.c.call("inspect", { path: `${root}/missing` }), (e) => typeof e === "string" && e.length > 0);
const ins = await a.c.call("inspect", { path: game });
assert.equal(ins.title_id, "PPSA01234");
const name = "[Smoke Game]-[PPSA01234].exfat";
assert.equal(await a.c.call("default_output", { source: game, format: "exfat", dir: images }), `${images}/PPSA01234.exfat`);
const out = await a.c.call("generated_output", { source: game, format: "exfat", dir: images, taken: [] });
assert.equal(out, `${images}/${name}`);
const out2 = await a.c.call("generated_output", { source: game, format: "exfat", dir: images, taken: [out] });
assert.equal(out2, `${images}/${name.replace(".exfat", "-2.exfat")}`);
assert.deepEqual(await a.c.call("stale_parts", { dirs: [images, `${root}/gone`] }), [`${images}/Old.exfat.part`]);
assert.equal(await a.c.call("reveal", { id: 1 }), undefined);
console.log("commands: list_dir (roots, root, folder, above the root), inspect (and a missing path), default_output, generated_output, stale_parts, reveal (local)");

// Jobs: one runs to the end, one is cancelled while queued behind it.
const request = { source: game, format: "exfat", output: out, compression_threads: null, inner: null, remove_backport: false, full_verify: false, kraken_level: "fast", ffpfsc_level: 6 };
const id = await a.c.call("start_job", { request });
const id2 = await a.c.call("start_job", { request: { ...request, output: out2 } });
assert.equal(await a.c.call("cancel_job", { id: id2 }), null);
assert.equal(await a.c.call("cancel_job", { id: id2 }), null, "cancel is idempotent");
// A job the page first sees finished (the queued one cancelled before a poll) arrives whole
// in a restore, not as a done event: either way, one result per job.
const results = (ev, job) => [
  ...ev.restore.flatMap((e) => e.jobs.filter((j) => j.id === job && j.done).map((j) => j.done.result)),
  ...ev.done.filter((e) => e.job === job).map((e) => e.result),
];
await waitFor("both done", () => results(a.ev, id).length && results(a.ev, id2).length, 120000);
await sleep(1500); // a later poll must not repeat a result
const mine = a.ev.progress.filter((e) => e.job === id).map((e) => e.done);
assert.ok(mine.length >= 1, `progress events: ${mine.length}`);
assert.deepEqual(mine, [...mine].sort((x, y) => x - y), "progress only grows");
const snap = (await raw("GET", "/api/jobs")).data;
const total = snap.jobs.find((j) => j.id === id).log_total;
const restoredLines = a.ev.restore.flatMap((e) => e.jobs.filter((j) => j.id === id).flatMap((j) => j.log));
const lines = [...restoredLines, ...a.ev.log.filter((e) => e.job === id).map((e) => e.line)];
assert.ok(total > 0);
assert.deepEqual(lines, snap.jobs.find((j) => j.id === id).log, "every log line once, in order");
assert.equal(results(a.ev, id).length, 1, "done once");
assert.equal(results(a.ev, id)[0].Ok.output, out);
assert.deepEqual(results(a.ev, id2), [{ Err: "cancelled" }]);
assert.deepEqual(await a.c.call("stale_parts", { dirs: [images] }), [`${images}/Old.exfat.part`], "no .part left");
const listed = await a.c.call("list_dir", { path: images });
assert.ok(listed.entries.some((e) => e.name === name && e.size > 64 << 20), "the image");
console.log(`jobs: ${mine.length} progress events, ${lines.length}/${total} log lines once each, done once, cancel -> {"Err":"cancelled"}, ${name} written`);

// Every request: POSTs carry JSON; nothing else (no token, no custom header).
for (const s of sent) {
  if (s.method === "POST") assert.deepEqual(s.headers, { "Content-Type": "application/json" }, s.path);
  else assert.deepEqual(s.headers, {}, s.path);
}
const routes = [...new Set(sent.map((s) => `${s.method} ${s.path}`))];
console.log(`routes hit: ${routes.join(", ")}`);
assert.ok(!routes.some((x) => x.includes("reveal") || x.includes("quit_app")));

// A reload: the list restored whole.
const b = await page_("b");
await waitFor("restore", () => b.ev.restore.some((e) => e.replace));
const back = b.ev.restore.find((e) => e.replace).jobs;
assert.deepEqual(back.map((j) => j.id), [id, id2]);
assert.equal(back[0].done.result.Ok.output, out);
// The server resolves `compression_threads: null` to max(1, cores - 1).
assert.ok(back[0].request.compression_threads >= 1);
assert.deepEqual(back[0].request, { ...request, compression_threads: back[0].request.compression_threads });
console.log("reload: both jobs restored with request, log and result");

// The server restarts: offline, then a new instance's (empty) list replaces the old one; the
// old jobs don't end with "no longer reported" (that's for one instance's dropped history).
const replacesBefore = b.ev.restore.filter((e) => e.replace).length;
const doneBefore = b.ev.done.length;
server.kill("SIGKILL");
await server.exited;
await waitFor("offline", () => b.c.getStatus().offline);
server = await startServer();
await waitFor("rebuilt after restart", () => b.ev.restore.filter((e) => e.replace).length > replacesBefore, 60000);
assert.deepEqual(b.ev.restore.at(-1), { replace: true, jobs: [] });
assert.equal(b.ev.done.length, doneBefore);
await waitFor("online", () => !b.c.getStatus().offline);
console.log("restart: offline notice, then the new instance's list (empty) replaces the old one");

// Quit: resolves once the server is gone; the process exits 0 after its last notice.
const t0 = Date.now();
await b.c.call("quit_app");
const exit = await server.exited;
assert.deepEqual(exit, { status: 0, signal: null });
assert.match(stderr, /PS5 Dump Forge stopped\n$/);
console.log(`quit: POST /api/quit, resolved after the server exited 0 (${Date.now() - t0} ms)`);
assert.ok(sent.some((s) => s.path === "/api/quit" && s.method === "POST"));
server = null;

console.log("smoke: ok");
process.exit(0);
