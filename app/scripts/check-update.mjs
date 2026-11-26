// Checks src/update.ts's decisions (tag, version order, the release file, failures as silence)
// with a fake fetch, then http-client.ts's selfUpdate with a fake server and clock, with a
// plain `node scripts/check-update.mjs` (type stripping, no network).

import assert from "node:assert/strict";

const U = await import("../src/update.ts");

// ---- tags: `vX.Y.Z` only ----
assert.equal(U.tagVersion("v1.2.3"), "1.2.3");
assert.equal(U.tagVersion("v0.10.0"), "0.10.0");
for (const bad of ["1.2.3", "v1.2", "v1.2.3.4", "v1.2.3-pre1", "v1.2.3+b", " v1.2.3", "v1.2.3\n", "V1.2.3", "v1.x.3", "v1234567890.0.0", "", null, 1, undefined])
  assert.equal(U.tagVersion(bad), null, String(bad));
console.log("tags: vX.Y.Z only (no prerelease, at most 9 digits a part)");

// ---- order: by number, a prerelease before its release ----
const newer = [
  ["0.1.2", "0.1.1"],
  ["0.2.0", "0.1.9"],
  ["1.0.0", "0.99.99"],
  ["0.1.10", "0.1.9"],
  ["0.1.2", "0.1.2-pre4"],
  ["0.1.2", "0.1.2-rc.1+build5"],
  ["0.1.3", "0.1.2+build"],
];
for (const [l, c] of newer) assert.equal(U.isNewer(l, c), true, `${l} > ${c}`);
const notNewer = [
  ["0.1.1", "0.1.1"],
  ["0.1.0", "0.1.1"],
  ["0.1.9", "0.1.10"],
  ["0.1.1", "0.1.2-pre4"],
  ["0.1.2", "0.1.2+build"],
  ["0.1.2", "garbage"],
  ["0.1.2", ""],
  ["x", "0.1.1"],
];
for (const [l, c] of notNewer) assert.equal(U.isNewer(l, c), false, `${l} !> ${c}`);
console.log("order: numeric, a current prerelease is older than its release, junk is never newer");

// ---- the release file ----
const ps5 = { version: "0.1.1", asset: "ps5-dump-forge-{v}-ps5.elf" };
const mac = { version: "0.1.1", asset: "ps5-dump-forge-{v}-macos-arm64.dmg" };
const rel = (tag, names) => ({ tag_name: tag, assets: names.map((name) => ({ name, state: "uploaded", size: 1 })) });
const all = (v) =>
  ["ps5.elf", "macos-arm64.dmg", "windows-x86-64.exe", "linux-x86-64.AppImage"].map((a) => U.assetName(v, `ps5-dump-forge-{v}-${a}`));
assert.equal(U.assetName("0.2.0", "ps5-dump-forge-{v}-ps5.elf"), "ps5-dump-forge-0.2.0-ps5.elf");
assert.equal(U.offer(rel("v0.2.0", all("0.2.0")), ps5), "0.2.0");
assert.equal(U.offer(rel("v0.2.0", all("0.2.0")), mac), "0.2.0");
assert.equal(U.offer(rel("v0.2.0", ["ps5-dump-forge-0.2.0-macos-arm64.dmg"]), ps5), null, "no elf");
assert.equal(U.offer(rel("v0.2.0", all("0.1.9")), ps5), null, "another version's files");
assert.equal(U.offer(rel("v0.2.0", ["PS5-DUMP-FORGE-0.2.0-PS5.ELF"]), ps5), null, "exact name");
assert.equal(U.offer(rel("v0.2.0", []), ps5), null, "no files yet");
assert.equal(U.offer({ tag_name: "v0.2.0" }, ps5), null, "no assets field");
assert.equal(U.offer({ tag_name: "v0.2.0", assets: [null, 1, {}] }, ps5), null, "junk assets");
const elf = "ps5-dump-forge-0.2.0-ps5.elf";
assert.equal(U.offer({ tag_name: "v0.2.0", assets: [{ name: elf, state: "uploaded", size: 1 }] }, ps5), "0.2.0");
assert.equal(U.offer({ tag_name: "v0.2.0", assets: [{ name: elf, state: "starter", size: 1 }] }, ps5), null, "upload unfinished");
assert.equal(U.offer({ tag_name: "v0.2.0", assets: [{ name: elf, state: "uploaded", size: 0 }] }, ps5), null, "empty file");
assert.equal(U.offer({ tag_name: "v0.2.0", assets: [{ name: elf }] }, ps5), null, "no state or size");
assert.equal(U.offer(rel("v0.1.1", all("0.1.1")), ps5), null, "same version");
assert.equal(U.offer(rel("v0.1.0", all("0.1.0")), ps5), null, "older");
assert.equal(U.offer(rel("v0.2.0-pre1", all("0.2.0-pre1")), ps5), null, "prerelease tag");
assert.equal(U.offer(null, ps5), null);
assert.equal(U.offer("x", ps5), null);
assert.equal(U.offer(rel("v0.1.1", all("0.1.1")), { version: "0.1.1-pre2", asset: "ps5-dump-forge-{v}-ps5.elf" }), "0.1.1");
console.log("offer: a newer tag whose assets hold this platform's exact file name");

// ---- checkUpdate: the request, and every failure is null ----
const reply = (body, status = 200) => async (url, init) => {
  assert.equal(url, "https://api.github.com/repos/quer3q/ps5-dump-forge/releases/latest");
  assert.deepEqual(Object.keys(init).sort(), ["cache", "credentials", "signal"], "no headers, no body");
  assert.equal(init.cache, "no-store");
  assert.equal(init.credentials, "omit");
  return new Response(typeof body === "string" ? body : JSON.stringify(body), { status });
};
assert.equal(await U.checkUpdate(ps5, reply(rel("v0.2.0", all("0.2.0")))), "0.2.0");
assert.equal(await U.checkUpdate(ps5, reply(rel("v0.2.0", all("0.2.0")), 403)), null, "rate limited");
assert.equal(await U.checkUpdate(ps5, reply(rel("v0.2.0", all("0.2.0")), 404)), null, "no release");
assert.equal(await U.checkUpdate(ps5, reply("<html>")), null, "not JSON");
assert.equal(await U.checkUpdate(ps5, async () => { throw new TypeError("offline"); }), null, "offline");
const realSetTimeout = globalThis.setTimeout;
globalThis.setTimeout = (f, ms, ...rest) => realSetTimeout(f, ms === U.CHECK_TIMEOUT_MS ? 5 : ms, ...rest);
const hang = async (_, init) =>
  new Promise((_ok, no) => init.signal.addEventListener("abort", () => no(new DOMException("aborted", "AbortError"))));
assert.equal(await U.checkUpdate(ps5, hang), null, "no answer within the timeout");
const slowBody = async (_, init) =>
  new Response(
    new ReadableStream({
      start(c) {
        c.enqueue(new TextEncoder().encode("{"));
        init.signal.addEventListener("abort", () => c.error(new DOMException("aborted", "AbortError")));
      },
    }),
    { status: 200 },
  );
assert.equal(await U.checkUpdate(ps5, slowBody), null, "a body that never ends");
globalThis.setTimeout = realSetTimeout;
console.log("checkUpdate: GET without cookies or cache; HTTP errors, junk, offline and timeouts are null");

// ---- ps5Release: this version's release when it carries the ELF, else the latest's ----
const releases = (byUrl) => async (url) => {
  const body = byUrl[url];
  return body === undefined ? new Response("{}", { status: 404 }) : new Response(JSON.stringify(body));
};
const tag = U.tagUrl("0.3.1");
assert.equal(tag, "https://api.github.com/repos/quer3q/ps5-dump-forge/releases/tags/v0.3.1");
const elfOf = (v) => rel(`v${v}`, [U.assetName(v, "ps5-dump-forge-{v}-ps5.elf")]);
assert.equal(await U.ps5Release("0.3.1", releases({ [tag]: elfOf("0.3.1"), [U.LATEST_URL]: elfOf("0.4.0") })), "0.3.1", "own version");
assert.equal(await U.ps5Release("0.3.1", releases({ [U.LATEST_URL]: elfOf("0.4.0") })), "0.4.0", "unreleased: latest");
assert.equal(await U.ps5Release("0.3.1", releases({ [tag]: rel("v0.3.1", []), [U.LATEST_URL]: elfOf("0.4.0") })), "0.4.0", "own release without the ELF");
assert.equal(await U.ps5Release("0.3.1-pre4", async (url) => { assert.equal(url, U.LATEST_URL); return new Response(JSON.stringify(elfOf("0.3.0"))); }), "0.3.0", "a prerelease asks for the latest only");
assert.equal(await U.ps5Release("0.3.1", releases({ [U.LATEST_URL]: rel("v0.4.0", ["x"]) })), null, "no ELF anywhere");
assert.equal(await U.ps5Release("0.3.1", async () => { throw new TypeError("offline"); }), null, "offline");
console.log("ps5Release: own version's ELF, else the latest release's; none or offline is null");

// ---- http-client's selfUpdate: a fake server and clock ----
const c = await import("../src/http-client.ts");
function server({ updateStatus = 200, updateError, startAfter = 3, dropCall = false, downFor = 0, sessionMs = 0 }) {
  const d = { now: 0, calls: [], sessions: 0 };
  const json = (v, status = 200) => new Response(JSON.stringify(v), { status });
  globalThis.fetch = async (path, init) => {
    if (path === "/api/session") {
      d.sessions++;
      d.now += sessionMs; // a slow answer
      if (d.sessions === 1) return json({ instance: "old" });
      if (d.sessions <= 1 + downFor) throw new TypeError("connection refused");
      return json({ instance: d.now >= startAfter * 1000 ? "new" : "old" });
    }
    assert.equal(path, "/api/self_update");
    assert.equal(init.method, "POST");
    d.calls.push(JSON.parse(init.body));
    if (dropCall) throw new TypeError("connection reset");
    return json(updateStatus === 200 ? {} : { error: updateError }, updateStatus);
  };
  // The 1 s poll sleeps advance the clock at once; request timeouts are real (and cleared).
  globalThis.setTimeout = (f, ms, ...rest) => {
    if (ms !== 1000) return realSetTimeout(f, ms, ...rest);
    d.now += ms;
    return realSetTimeout(f, 0, ...rest);
  };
  Date.now = () => d.now;
  return d;
}
const realNow = Date.now;
try {
  let d = server({});
  await c.selfUpdate("0.2.0");
  assert.deepEqual(d.calls, [{ version: "0.2.0" }]);
  assert.equal(d.now, 3000, "polled every second until the new instance answered");
  console.log("selfUpdate: POST {version}, then the session until its instance changes");

  d = server({ dropCall: true, downFor: 2 });
  await c.selfUpdate("0.2.0");
  assert.equal(d.calls.length, 1);
  console.log("selfUpdate: a dropped reply and a server that is down for a while are waited out");

  d = server({ updateStatus: 503, updateError: "elfldr isn't running on port 9021: load it, then try again" });
  await assert.rejects(c.selfUpdate("0.2.0"), (e) => e === "elfldr isn't running on port 9021: load it, then try again");
  assert.equal(d.now, 0, "no waiting after a refusal");
  console.log("selfUpdate: the server's refusal is the error, as is");

  d = server({ startAfter: 1e9 });
  await assert.rejects(c.selfUpdate("0.2.0"), (e) => e === U.UPDATE_TIMEOUT);
  assert.equal(d.now, c.UPDATE_WAIT_MS);
  console.log(`selfUpdate: no new instance -> UPDATE_TIMEOUT after ${c.UPDATE_WAIT_MS / 1000} s`);

  // Slow session answers count against the same minute, the first one included: a new
  // instance seen only after it is still a timeout.
  d = server({ startAfter: 61, sessionMs: 2999 });
  await assert.rejects(c.selfUpdate("0.2.0"), (e) => e === U.UPDATE_TIMEOUT);
  assert.ok(d.now <= c.UPDATE_WAIT_MS + 2999, `gave up at ${d.now} ms`);
  console.log("selfUpdate: the minute runs from the click, slow answers included");

  // The first answer comes after the minute: nothing is asked.
  d = server({ sessionMs: 61000 });
  await assert.rejects(c.selfUpdate("0.2.0"), (e) => e === U.UPDATE_TIMEOUT);
  assert.equal(d.calls.length, 0, "no POST once expired");
  console.log("selfUpdate: an expired minute never sends the update");

  globalThis.fetch = async () => {
    throw new TypeError("connection refused");
  };
  await assert.rejects(c.selfUpdate("0.2.0"), (e) => e === "unreachable");
  console.log("selfUpdate: an unreachable server is refused before anything is asked");
} finally {
  globalThis.setTimeout = realSetTimeout;
  Date.now = realNow;
}
