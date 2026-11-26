// Checks src/shoulders.ts's pure parts (the rail's order wrapping, the keys) and App.tsx's rail order with a
// plain `node scripts/check-shoulders.mjs` (type stripping).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const S = await import("../src/shoulders.ts");

// ---- order: the rail's, wrapping at the ends ----
const order = ["build", "queue", "transfer", "edit", "storage", "connect", "settings", "formats"];
// App.tsx steps TAB_ORDER and renders the rail's items; both must be this order.
const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const tabOrder = app.match(/const TAB_ORDER[^=]*=\s*\[([^\]]*)\]/);
assert.ok(tabOrder, "App.tsx: TAB_ORDER");
assert.deepEqual([...tabOrder[1].matchAll(/"(\w+)"/g)].map((m) => m[1]), order, "TAB_ORDER");
const rail = app.slice(app.indexOf("<RailNav"));
const items = rail.slice(rail.indexOf("items={["), rail.indexOf("]}"));
assert.deepEqual([...items.matchAll(/\{ id: "(\w+)"/g)].map((m) => m[1]), order, "the rail's items");
assert.equal(S.stepTab(order, "build", 1), "queue");
assert.equal(S.stepTab(order, "queue", 1), "transfer");
assert.equal(S.stepTab(order, "transfer", 1), "edit");
assert.equal(S.stepTab(order, "edit", 1), "storage");
assert.equal(S.stepTab(order, "storage", 1), "connect");
assert.equal(S.stepTab(order, "connect", 1), "settings");
assert.equal(S.stepTab(order, "settings", 1), "formats");
assert.equal(S.stepTab(order, "formats", 1), "build", "R2 on the last wraps to the first");
assert.equal(S.stepTab(order, "build", -1), "formats", "L2 on the first wraps to the last");
assert.equal(S.stepTab(order, "queue", -1), "build");
let at = "build";
for (let i = 0; i < order.length; i++) at = S.stepTab(order, at, 1);
assert.equal(at, "build", "a full round comes back");
assert.equal(S.stepTab(order, "nope", 1), "build", "unknown: the first");
console.log("order: Build, Queue, Transfer, Edit, Storage, Connect, Settings, About (App.tsx's rail), wrapping both ways");

// ---- keys: the PS5 browser's L2 (F7) and R2 (F8), as the debug build's /probe saw them ----
assert.equal(S.keyStep({ key: "F7" }), -1, "L2");
assert.equal(S.keyStep({ key: "F8" }), 1, "R2");
assert.equal(S.keyStep({ key: "F5" }), null, "L1/R1 are the browser's");
assert.equal(S.keyStep({ key: "ArrowLeft" }), null);
console.log("keys: L2 = F7, R2 = F8");
