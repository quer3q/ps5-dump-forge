// Checks src/paths.ts with a plain `node scripts/check-paths.mjs` (type stripping): the
// desktop rules per viewer OS, and the http build's server paths (`/` only) on a Windows viewer.

import assert from "node:assert/strict";

async function load(userAgent, tag) {
  Object.defineProperty(globalThis, "navigator", { value: { userAgent }, configurable: true });
  return import(`../src/paths.ts?${tag}`);
}

// macOS/Linux app: `\` is a name character.
const mac = await load("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", "mac");
assert.equal(mac.joinPath("/Volumes/USB", "Game.ffpkg"), "/Volumes/USB/Game.ffpkg");
assert.equal(mac.joinPath("/Volumes/odd\\", "x"), "/Volumes/odd\\/x");
assert.equal(mac.dirname("/Volumes/USB/Game"), "/Volumes/USB");
assert.equal(mac.dirname("/game"), "/");
assert.equal(mac.basename("/a/b\\c"), "b\\c");
assert.ok(!mac.SEPARATORS.test("a\\b"));

// Windows app: both separators, joins with `\`.
const win = await load("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "win");
assert.equal(win.joinPath("C:\\Games", "Game.ffpkg"), "C:\\Games\\Game.ffpkg");
assert.equal(win.dirname("C:\\game"), "C:\\");
assert.equal(win.basename("C:\\Games\\PPSA01234\\"), "PPSA01234");
assert.ok(win.SEPARATORS.test("a\\b"));

// http build on a Windows viewer, PS5 server (separator "/"): `/` paths, `\` a name character.
win.setServerSeparator("/");
assert.equal(win.joinPath("/data/homebrew", "Game.ffpkg"), "/data/homebrew/Game.ffpkg");
assert.equal(win.joinPath("/mnt/usb0/", "Game.exfat"), "/mnt/usb0/Game.exfat");
assert.equal(win.joinPath("/data/odd\\", "x"), "/data/odd\\/x");
assert.equal(win.dirname("/mnt/usb0/games/PPSA01234"), "/mnt/usb0/games");
assert.equal(win.dirname("/data"), "/");
assert.equal(win.dirname("/data/a\\b"), "/data");
assert.equal(win.basename("/data/a\\b/"), "a\\b");
assert.equal(win.trimSep("/data/x\\"), "/data/x\\");
assert.ok(!win.SEPARATORS.test("a\\b"));
assert.ok(win.SEPARATORS.test("a/b"));

// http build on a macOS viewer, Windows host server (separator "\\"): the Windows rules.
mac.setServerSeparator("\\");
assert.equal(mac.joinPath("C:\\Games", "Game.ffpkg"), "C:\\Games\\Game.ffpkg");
assert.equal(mac.dirname("C:\\Games\\PPSA01234"), "C:\\Games");
assert.equal(mac.dirname("C:\\game"), "C:\\");
assert.equal(mac.basename("C:\\Games\\PPSA01234\\"), "PPSA01234");
assert.ok(mac.SEPARATORS.test("a\\b"));
// And back to "/" (a later session saying so).
mac.setServerSeparator("/");
assert.equal(mac.dirname("/data/a\\b"), "/data");

console.log("paths.ts: ok (macOS, Windows, http: PS5 server on a Windows viewer, Windows-host server on a macOS viewer)");
