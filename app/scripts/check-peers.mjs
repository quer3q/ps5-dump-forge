// Checks src/peers.ts with a plain `node` (type stripping): `nasPath` and `readable`.

import assert from "node:assert/strict";

const p = await import("../src/peers.ts");
const saved = [
  { name: "Home NAS", host: "192.168.1.5", port: 445, protocol: "smb" },
  { name: "Box", host: "box.local", port: 2121, protocol: "ftp" },
  { name: "Exports", host: "fd00::5", port: 2049, protocol: "nfs" },
];

assert.equal(p.nasPath("smb://192.168.1.5/games/x", saved), "Home NAS: /games/x");
assert.equal(p.nasPath("SMB://192.168.1.5", saved), "Home NAS: /");
assert.equal(p.nasPath("ftp://BOX.local:2121/out", saved), "Box: /out");
assert.equal(p.nasPath("nfs://[fd00::5]/srv/g", saved), "Exports: /srv/g");
// Another port, another protocol, an unsaved host or a host that only starts the same: as it is.
assert.equal(p.nasPath("ftp://box.local/out", saved), null);
assert.equal(p.nasPath("smb://box.local:2121/out", saved), null);
assert.equal(p.nasPath("smb://192.168.1.50/g", saved), null);
assert.equal(p.nasPath("/data/games", saved), null);
assert.equal(p.nasPath("flink://0123/x", saved), null);

p.setDevices([], saved);
assert.equal(p.readable("can't read smb://192.168.1.5/games/x"), "can't read Home NAS: /games/x");
assert.equal(p.readable("ftp://box.local:2121 is offline"), "Box is offline");

console.log("check-peers: ok");
