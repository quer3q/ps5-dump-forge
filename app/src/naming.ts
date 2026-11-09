// How generated output names are made (About's Output names, by user decision): one template
// string with the tokens `{title}`, `{title_id}`, `{version}`, `{pack}`, `{region}`, `{fw}` and any text around
// them, kept in `localStorage` `forge.naming` beside the language and the shortcuts. A port of
// core's `naming_problem` and of its substitution and cleanup (`inspect.rs` `parse_naming`,
// `NameParts::render`). Unreadable, invalid or missing storage is the default, never an error.
// No runtime imports, so plain-node checks can load it.

/** One token of a naming template. */
export type NamePart = "title" | "title_id" | "version" | "pack" | "region" | "fw";

const KEY = "forge.naming";

/** Every token, in the order the About card's chips list them. */
export const PARTS: NamePart[] = ["title", "title_id", "version", "pack", "region", "fw"];

/** Core's default (`naming` null). */
export const DEFAULT_NAMING = "[{title}]-[{title_id}]";

/** Core's template limit, in UTF-8 bytes. */
export const MAX_NAMING = 128;

/** Why core refuses a template (`naming_problem`), one kind per message. */
export type NamingProblem =
  | { kind: "long" }
  | { kind: "open" }
  | { kind: "close" }
  | { kind: "unknown"; token: string }
  | { kind: "twice"; token: string }
  | { kind: "char"; char: string }
  | { kind: "control" }
  | { kind: "needsName" };

/** A template's text (one character each, NFC) and its tokens, in order. */
type Piece = { text: string } | { part: NamePart };

const FORBIDDEN = '/\\:*?"<>|';

function parse(template: string): Piece[] | NamingProblem {
  if (new TextEncoder().encode(template).length > MAX_NAMING) return { kind: "long" };
  const pieces: Piece[] = [];
  let text = "";
  const flush = () => {
    for (const c of text.normalize("NFC")) pieces.push({ text: c });
    text = "";
  };
  let rest = template;
  while (rest) {
    const c = String.fromCodePoint(rest.codePointAt(0)!);
    if (c === "{") {
      const close = rest.indexOf("}");
      if (close < 0) return { kind: "open" };
      const token = rest.slice(1, close);
      if (!(PARTS as string[]).includes(token)) return { kind: "unknown", token };
      // Each part once, as core refuses a repeat.
      if (pieces.some((p) => "part" in p && p.part === token)) return { kind: "twice", token };
      flush();
      pieces.push({ part: token as NamePart });
      rest = rest.slice(close + 1);
      continue;
    }
    if (c === "}") return { kind: "close" };
    if (/\p{Cc}/u.test(c)) return { kind: "control" };
    if (FORBIDDEN.includes(c)) return { kind: "char", char: c };
    text += c;
    rest = rest.slice(c.length);
  }
  flush();
  if (!pieces.some((p) => "part" in p && (p.part === "title" || p.part === "title_id"))) {
    return { kind: "needsName" };
  }
  return pieces;
}

/** Why `template` can't name an output, or null when it can. */
export function namingProblem(template: string): NamingProblem | null {
  const p = parse(template);
  return Array.isArray(p) ? null : p;
}

/**
 * `template` with each token's value (`value`; an empty one is left out), cleaned up as core
 * does, the template's own text only: an empty `[]` or `()` removed until none is left, a run
 * of one separator (`-`, `_`, a space) as one, `-`, `_`, `.` and spaces trimmed at both ends.
 * An invalid template gives "".
 */
export function render(template: string, value: (p: NamePart) => string): string {
  const parsed = parse(template);
  if (!Array.isArray(parsed)) return "";
  // [own text, characters]: a value is never cleaned up.
  let pieces: [boolean, string][] = parsed
    .map((p): [boolean, string] => ("text" in p ? [true, p.text] : [false, value(p.part)]))
    .filter(([own, s]) => own || s !== "");
  const own = (i: number) => (pieces[i]?.[0] ? pieces[i][1] : null);
  for (let i = 0; i + 1 < pieces.length; ) {
    const pair = `${own(i)}${own(i + 1)}`;
    if (own(i) !== null && own(i + 1) !== null && (pair === "[]" || pair === "()")) {
      pieces.splice(i, 2);
      i = Math.max(0, i - 1);
    } else {
      i += 1;
    }
  }
  const sep = (s: string | null) => s === "-" || s === "_" || s === " ";
  pieces = pieces.filter((_, i) => !(i > 0 && sep(own(i)) && own(i) === own(i - 1)));
  const end = (s: string | null) => sep(s) || s === ".";
  let from = 0;
  let to = pieces.length;
  while (to > 0 && end(own(to - 1))) to -= 1;
  while (from < to && end(own(from))) from += 1;
  return pieces
    .slice(from, to)
    .map(([, s]) => s)
    .join("");
}

/** `v` when it is a template core takes, else null. */
export function valid(v: unknown): string | null {
  return typeof v === "string" && !namingProblem(v) ? v : null;
}

function read(): string {
  try {
    const raw = localStorage.getItem(KEY);
    return (raw && valid(JSON.parse(raw))) || DEFAULT_NAMING;
  } catch {
    return DEFAULT_NAMING;
  }
}

let current: string | null = null;
const listeners = new Set<() => void>();

/** The template in force (read from storage once). */
export function getNaming(): string {
  return (current ??= read());
}

/** Keeps `n` (an invalid one is ignored) and tells every subscriber. */
export function setNaming(n: string) {
  if (!valid(n) || n === getNaming()) return;
  current = n;
  try {
    localStorage.setItem(KEY, JSON.stringify(n));
  } catch {
    // Storage blocked or full: the template lasts until the page closes.
  }
  listeners.forEach((l) => l());
}

export function subscribeNaming(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** ShadowMountPlus's limit on an image name without its extension (58 for `.ffpfsc`). */
export const STEM_MAX = 63;

/** UTF-8 bytes, as the limit counts them. */
export const bytes = (s: string) => new TextEncoder().encode(s).length;

/** `s` within `max` UTF-8 bytes, cut at a character boundary, no `.` or space left at its end
 * (core's `cut` and trim). */
export function cutBytes(s: string, max: number): string {
  let out = "";
  for (const c of s) {
    if (bytes(out + c) > max) break;
    out += c;
  }
  return out.replace(/[. ]+$/, "");
}

/** The About card's example game (`pack` as a zstd Pack names it). */
export const SAMPLE: Record<NamePart, string> = {
  title: "Astro Bot",
  title_id: "PPSA01234",
  version: "01.000.000",
  pack: "zstd",
  region: "USA",
  fw: "7.00",
};

/** `n` as core's `naming` (`null`: core's default). */
export const template = (n: string): string | null => (n === DEFAULT_NAMING ? null : n);
