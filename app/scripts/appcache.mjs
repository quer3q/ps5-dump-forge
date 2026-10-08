// The http build's AppCache manifest (vite.config.ts writes it after the bundle; the PS5
// browser keeps the page offline with it, see src/appcache.ts). Plain node, so
// scripts/check-launcher.mjs runs the same code.

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const MANIFEST = "forge.appcache";

/** Every file under `dir` but the manifest, `/`-separated and sorted. */
export function bundleFiles(dir, prefix = "") {
  const out = [];
  for (const e of readdirSync(join(dir, prefix), { withFileTypes: true })) {
    const rel = prefix + e.name;
    if (e.isDirectory()) out.push(...bundleFiles(dir, `${rel}/`));
    else if (rel !== MANIFEST) out.push(rel);
  }
  return out.sort();
}

/** `CACHE MANIFEST`, a hash over every path and its bytes (any rebuild that changes a byte
 * changes the manifest, so the browser fetches the new page), `/` and every file, then
 * `NETWORK: *` (the API and elfldr). */
export function manifestText(dir) {
  const files = bundleFiles(dir);
  const hash = createHash("sha256");
  for (const f of files) {
    const bytes = readFileSync(join(dir, f));
    hash.update(`${f}\0${bytes.length}\0`);
    hash.update(bytes);
  }
  return ["CACHE MANIFEST", `# ${hash.digest("hex")}`, "", "CACHE:", "/", ...files.map(encodeURI), "", "NETWORK:", "*", ""].join("\n");
}

export function writeManifest(dir) {
  writeFileSync(join(dir, MANIFEST), manifestText(dir));
}
