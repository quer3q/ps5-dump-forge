// Development only: drives dist-http and src/http-client.ts against the real
// `ps5-dump-forge serve` (built here with cargo, so it embeds the current dist-http) with
// plain node (no browser), on a temp root holding a small game: the bundle's files, a real job
// (folder -> .exfat) and a cancelled one through the poller, delete_path's refusals, a
// reload's restore, a server restart (the list rebuilt) and quit.
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
const ORIGIN = `http://127.0.0.1:${PORT}`;

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

/** A raw request: status, headers, parsed JSON body. */
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

// ---- the bundle: index.html, then every file its AppCache manifest lists ----
const page = await fetch(`${ORIGIN}/`);
assert.equal(page.status, 200);
const manifestRef = (await page.text()).match(/<html manifest="([^"]+)"/)?.[1];
const manLines = (await (await fetch(new URL(manifestRef, `${ORIGIN}/`))).text()).split("\n");
const cachedFiles = manLines.slice(manLines.indexOf("CACHE:") + 1, manLines.indexOf("NETWORK:")).filter(Boolean);
for (const r of cachedFiles) assert.equal((await fetch(new URL(r, `${ORIGIN}/`))).status, 200, r);
console.log(`bundle: ${cachedFiles.length} files served`);
const json = { "Content-Type": "application/json" };

// ---- a browser: fetch on this origin ----
const realFetch = globalThis.fetch;
globalThis.fetch = (path, init = {}) => realFetch(new URL(path, ORIGIN), init);

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

// The outputs.
const name = "[Smoke Game]-[PPSA01234].exfat";
const out = await a.c.call("generated_output", { source: game, format: "exfat", dir: images, taken: [] });
assert.equal(out, `${images}/${name}`);
const out2 = await a.c.call("generated_output", { source: game, format: "exfat", dir: images, taken: [out] });

// Jobs: one runs to the end, one is cancelled while queued behind it.
const request = { source: game, format: "exfat", output: out, compression_threads: null, inner: null, remove_backport: false, full_verify: false, kraken_level: "fast", ffpfsc_level: 6, lz4: null };
const id = await a.c.call("start_job", { request });
const id2 = await a.c.call("start_job", { request: { ...request, output: out2 } });
assert.equal(await a.c.call("cancel_job", { id: id2 }), null);
// A job the page first sees finished (the queued one cancelled before a poll) arrives whole
// in a restore, not as a done event: either way, one result per job.
const results = (ev, job) => [
  ...ev.restore.flatMap((e) => e.jobs.filter((j) => j.id === job && j.done).map((j) => j.done.result)),
  ...ev.done.filter((e) => e.job === job).map((e) => e.result),
];
await waitFor("both done", () => results(a.ev, id).length && results(a.ev, id2).length, 120000);
await sleep(1500); // a later poll must not repeat a result
const snap = (await raw("GET", "/api/jobs")).data;
const total = snap.jobs.find((j) => j.id === id).log_total;
const restoredLines = a.ev.restore.flatMap((e) => e.jobs.filter((j) => j.id === id).flatMap((j) => j.log));
const lines = [...restoredLines, ...a.ev.log.filter((e) => e.job === id).map((e) => e.line)];
assert.deepEqual(lines, snap.jobs.find((j) => j.id === id).log, "every log line once, in order");
assert.equal(results(a.ev, id).length, 1, "done once");
assert.equal(results(a.ev, id)[0].Ok.output, out);
assert.deepEqual(results(a.ev, id2), [{ Err: "cancelled" }]);
assert.deepEqual(await a.c.call("stale_parts", { dirs: [images] }), [`${images}/Old.exfat.part`], "no .part left");
const listed = await a.c.call("list_dir", { path: images });
assert.ok(listed.entries.some((e) => e.name === name && e.size > 64 << 20), "the image");
console.log(`jobs: ${lines.length}/${total} log lines once each, done once, cancel -> {"Err":"cancelled"}, ${name} written`);

// A game folder the deletes below must leave alone, and a file to delete.
const kept = `${root}/kept game`;
mkdirSync(`${kept}/sce_sys`, { recursive: true });
writeFileSync(`${kept}/sce_sys/param.json`, JSON.stringify({ titleId: "PPSA05678", titleName: "Smoke: Kept" }));
const zipPath = `${root}/fixture.zip`;
writeFileSync(zipPath, "not a zip");
mkdirSync(`${root}/a folder`);
writeFileSync(`${root}/a folder/x.bin`, "x");
// Delete (Inspect's, after the page asks): disposable fixtures only. A file and a folder go;
// a browse root, a folder holding one, an empty path and a missing one are refused.
assert.equal(await a.c.call("delete_path", { path: zipPath }), null);
assert.equal(await a.c.call("delete_path", { path: `${root}/a folder` }), null);
assert.ok(!(await a.c.call("list_dir", { path: root })).entries.some((e) => e.name === "fixture.zip" || e.name === "a folder"), "both gone");
const delStatus = async (path) => (await raw("POST", "/api/delete_path", json, JSON.stringify({ path }))).status;
assert.equal(await delStatus(root), 400, "a browse root");
assert.equal(await delStatus(dirname(root)), 400, "a folder holding a browse root");
assert.equal(await delStatus(""), 400, "an empty path");
assert.equal(await delStatus(`${root}/a folder`), 404, "already gone");
assert.ok((await a.c.call("list_dir", { path: kept })).entries.length > 0, "the rest is untouched");
console.log("delete_path: a file and a folder deleted; browse root, its parent, empty 400; missing 404");

// A reload: the list restored whole.
const b = await page_("b");
await waitFor("restore", () => b.ev.restore.some((e) => e.replace));
const back = b.ev.restore.find((e) => e.replace).jobs;
assert.deepEqual(back.map((j) => j.id), [id, id2]);
assert.equal(back[0].done.result.Ok.output, out);
console.log("reload: every job restored with its result");

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
assert.match(stderr, /Dump Forge stopped\n$/);
console.log(`quit: resolved after the server exited 0 (${Date.now() - t0} ms)`);
server = null;

console.log("smoke: ok");
process.exit(0);
