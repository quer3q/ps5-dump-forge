// The update check, once per start (UpdateToast.tsx): GitHub's latest release, when it is newer
// than this build and carries this platform's file. No setting and nothing stored; any failure
// is silence. No runtime imports, so a plain `node` can drive it (scripts/check-update.mjs).

export const LATEST_URL = "https://api.github.com/repos/quer3q/ps5-dump-forge/releases/latest";
export const CHECK_TIMEOUT_MS = 10000;

/** What this build updates to: its version and the release file's full name, `{v}` standing
 * for the version, from the transport's `updateTarget`. */
export interface Target {
  version: string;
  asset: string;
}

/** `startUpdate`'s rejection when the new version never answered (the UI words it). */
export const UPDATE_TIMEOUT = "update-timeout";
export type UpdateTimeout = typeof UPDATE_TIMEOUT;

/** A release tag `vX.Y.Z` as its version, else null (prereleases and anything else). At most 9
 * digits a part: what the server's `self_update` takes. */
export function tagVersion(tag: unknown): string | null {
  return typeof tag === "string" && /^v\d{1,9}\.\d{1,9}\.\d{1,9}$/.test(tag) ? tag.slice(1) : null;
}

/** `X.Y.Z` newer than `current` (`X.Y.Z`, maybe `-pre4` or `+build`): by number, and a current
 * prerelease is older than its release. False when `current` doesn't parse. */
export function isNewer(latest: string, current: string): boolean {
  const l = /^(\d+)\.(\d+)\.(\d+)$/.exec(latest);
  const c = /^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.exec(current);
  if (!l || !c) return false;
  for (let i = 1; i <= 3; i++) if (+l[i] !== +c[i]) return +l[i] > +c[i];
  return c[4] !== undefined;
}

/** The release file for a version: `asset` (a full file name) with `{v}` replaced by it. */
export function assetName(version: string, asset: string): string {
  return asset.split("{v}").join(version); // no replaceAll: ES2020 lib, the PS5 browser
}

/** The newer version to offer, from a `releases/latest` body; null unless its tag is a plain
 * version newer than ours and its assets list our file by its exact name, uploaded and not
 * empty (so the download can't 404 or come back empty). */
export function offer(release: unknown, target: Target): string | null {
  const v = tagVersion((release as { tag_name?: unknown } | null)?.tag_name);
  return v && isNewer(v, target.version) && carries(release, assetName(v, target.asset)) ? v : null;
}

/** Whether a release body lists the file `name`, uploaded and not empty. */
function carries(release: unknown, name: string): boolean {
  const assets = (release as { assets?: unknown } | null)?.assets;
  const ok = (a: unknown) => {
    const f = a as { name?: unknown; state?: unknown; size?: unknown } | null;
    return f?.name === name && f.state === "uploaded" && typeof f.size === "number" && f.size > 0;
  };
  return Array.isArray(assets) && assets.some(ok);
}

/** A release by its tag. */
export const tagUrl = (version: string) => `https://api.github.com/repos/quer3q/ps5-dump-forge/releases/tags/v${version}`;

/** GET `url` as JSON within `CHECK_TIMEOUT_MS` (no cookies, no cache); null on anything else. */
async function ask(url: string, f: typeof fetch): Promise<unknown> {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), CHECK_TIMEOUT_MS);
  try {
    const res = await f(url, { cache: "no-store", credentials: "omit", signal: abort.signal });
    return res.ok ? await res.json() : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** The release whose PS5 ELF Install on PS5 loads (Connect, desktop app): `current`'s own when
 * it is a plain X.Y.Z released with that file, else the latest release's; null when neither
 * carries it or GitHub doesn't answer. Never rejects. */
export async function ps5Release(current: string, f: typeof fetch = (i, o) => fetch(i, o)): Promise<string | null> {
  const urls = /^\d+\.\d+\.\d+$/.test(current) ? [tagUrl(current), LATEST_URL] : [LATEST_URL];
  for (const url of urls) {
    const r = await ask(url, f);
    const v = tagVersion((r as { tag_name?: unknown } | null)?.tag_name);
    if (v && carries(r, assetName(v, "ps5-dump-forge-{v}-ps5.elf"))) return v;
  }
  return null;
}

/** Ask GitHub (no cookies, no cache, `CHECK_TIMEOUT_MS` for the whole answer): the version to
 * offer, or null on anything else. Never rejects. */
export async function checkUpdate(target: Target, f: typeof fetch = (i, o) => fetch(i, o)): Promise<string | null> {
  return offer(await ask(LATEST_URL, f), target);
}
