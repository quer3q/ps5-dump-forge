// Path helpers for what the UI shows and joins. No imports, so a plain `node` can check them
// (TypeScript type stripping).

// `\` is a legal name character on macOS and Linux; only Windows uses it as a separator.
const WINDOWS = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

function lastSep(p: string): number {
  return WINDOWS ? Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")) : p.lastIndexOf("/");
}

/** `path` without trailing separators. */
export function trimSep(path: string): string {
  return path.replace(WINDOWS ? /[\\/]+$/ : /\/+$/, "");
}

/** The folder holding `path`. */
export function dirname(path: string): string {
  const p = trimSep(path);
  const i = lastSep(p);
  // Keep the root separator: "/game" -> "/", "C:\game" -> "C:\".
  return i < 0 ? "" : p.slice(0, i === 0 || p[i - 1] === ":" ? i + 1 : i);
}

export function basename(path: string): string {
  const p = trimSep(path);
  return p.slice(lastSep(p) + 1);
}

/** `name` inside `dir`, with the platform's separator. A trailing `\` on macOS or Linux is
 * part of the folder's name, not a separator. */
export function joinPath(dir: string, name: string): string {
  if (!dir) return name;
  const ends = WINDOWS ? /[\\/]$/ : /\/$/;
  return ends.test(dir) ? dir + name : `${dir}${WINDOWS ? "\\" : "/"}${name}`;
}

/** The separator characters a file name can't contain. */
export const SEPARATORS = WINDOWS ? /[\\/]/ : /\//;
