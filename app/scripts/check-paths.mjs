// Checks src/paths.ts with a plain `node` (type stripping): desktop rules per OS, then the http build.

import assert from "node:assert/strict";

async function load(userAgent, tag) {
  Object.defineProperty(globalThis, "navigator", { value: { userAgent }, configurable: true });
  return import(`../src/paths.ts?${tag}`);
}

// macOS/Linux app: `\` is a name character.
const mac = await load("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", "mac");
assert.equal(mac.joinPath("/Volumes/odd\\", "x"), "/Volumes/odd\\/x");
assert.equal(mac.dirname("/game"), "/");
assert.equal(mac.basename("/a/b\\c"), "b\\c");

// Windows app: both separators, joins with `\`.
const win = await load("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "win");
assert.equal(win.joinPath("C:\\Games", "Game.ffpkg"), "C:\\Games\\Game.ffpkg");
assert.equal(win.dirname("C:\\game"), "C:\\");
assert.equal(win.basename("C:\\Games\\PPSA01234\\"), "PPSA01234");

// http build on a Windows viewer, PS5 server (separator "/"): `/` paths, `\` a name character.
win.setServerSeparator("/");
assert.equal(win.joinPath("/mnt/usb0/", "Game.exfat"), "/mnt/usb0/Game.exfat");
assert.equal(win.joinPath("/data/odd\\", "x"), "/data/odd\\/x");
assert.equal(win.dirname("/data"), "/");
assert.equal(win.dirname("/data/a\\b"), "/data");
assert.equal(win.basename("/data/a\\b/"), "a\\b");

// http build on a macOS viewer, Windows host server (separator "\\"): the Windows rules.
mac.setServerSeparator("\\");
assert.equal(mac.joinPath("C:\\Games", "Game.ffpkg"), "C:\\Games\\Game.ffpkg");
assert.equal(mac.dirname("C:\\game"), "C:\\");
// And back to "/" (a later session saying so).
mac.setServerSeparator("/");
assert.equal(mac.dirname("/data/a\\b"), "/data");

console.log("paths.ts: ok (macOS, Windows, http: PS5 server on a Windows viewer, Windows-host server on a macOS viewer)");
