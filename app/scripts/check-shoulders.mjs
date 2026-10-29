// Checks src/shoulders.ts's pure parts (the tab order wrapping, the keys) with a
// plain `node scripts/check-shoulders.mjs` (type stripping).

import assert from "node:assert/strict";

const S = await import("../src/shoulders.ts");

// ---- order: the bar's, wrapping at the ends ----
const order = ["convert", "edit", "pack", "storage", "connect", "formats"];
assert.equal(S.stepTab(order, "convert", 1), "edit");
assert.equal(S.stepTab(order, "storage", 1), "connect");
assert.equal(S.stepTab(order, "connect", 1), "formats");
assert.equal(S.stepTab(order, "formats", 1), "convert", "R2 on the last wraps to the first");
assert.equal(S.stepTab(order, "convert", -1), "formats", "L2 on the first wraps to the last");
assert.equal(S.stepTab(order, "pack", -1), "edit");
let at = "convert";
for (let i = 0; i < order.length; i++) at = S.stepTab(order, at, 1);
assert.equal(at, "convert", "a full round comes back");
assert.equal(S.stepTab(order, "nope", 1), "convert", "unknown: the first");
console.log("order: Convert, Edit, Pack, Storage, Connect, About, wrapping both ways");

// ---- keys: the PS5 browser's L2 (F7) and R2 (F8), as the debug build's /probe saw them ----
assert.equal(S.keyStep({ key: "F7" }), -1, "L2");
assert.equal(S.keyStep({ key: "F8" }), 1, "R2");
assert.equal(S.keyStep({ key: "F5" }), null, "L1/R1 are the browser's");
assert.equal(S.keyStep({ key: "ArrowLeft" }), null);
console.log("keys: L2 = F7, R2 = F8");
