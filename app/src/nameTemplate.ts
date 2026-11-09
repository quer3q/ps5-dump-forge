// The generated-name template (issue #33): only the assembly is customizable; core fixes how
// each field is derived and how a name is shortened (the title only). Kept beside the UI
// language in localStorage; unreadable or invalid storage is the built-in template.

import { useSyncExternalStore } from "react";

export const DEFAULT_NAME_TEMPLATE = "{id}-{title}-{version}-{kind}";

const KEY = "forge.nameTemplate";
const MAX_BYTES = 256;

export type TemplateProblem =
  | { kind: "unknown"; token: string }
  | { kind: "unclosed" }
  | { kind: "nested" }
  | { kind: "char"; char: string }
  | { kind: "long" };

/** The checks core's `parse_name_template` runs; null for a usable template (blank is the
 * built-in one). */
export function templateProblem(value: string): TemplateProblem | null {
  const template = value.trim();
  if (!template) return null;
  if (new TextEncoder().encode(template).length > MAX_BYTES) return { kind: "long" };
  let rest = template;
  while (true) {
    const open = rest.indexOf("{");
    if (open < 0) break;
    const literal = literalProblem(rest.slice(0, open));
    if (literal) return literal;
    const after = rest.slice(open + 1);
    const close = after.indexOf("}");
    if (close < 0) return { kind: "unclosed" };
    const name = after.slice(0, close).trim();
    if (name.includes("{")) return { kind: "nested" };
    if (!["id", "title", "version", "kind"].includes(name)) return { kind: "unknown", token: name };
    rest = after.slice(close + 1);
  }
  return literalProblem(rest);
}

function literalProblem(literal: string): TemplateProblem | null {
  for (const c of literal.normalize("NFC")) {
    if (!(/\p{L}|\p{N}/u.test(c) || " -_.,'()&+![]".includes(c))) return { kind: "char", char: c };
  }
  return null;
}

function read(): string {
  try {
    const v = localStorage.getItem(KEY);
    if (v === null) return DEFAULT_NAME_TEMPLATE;
    const t = v.trim();
    return t && !templateProblem(t) ? t : DEFAULT_NAME_TEMPLATE;
  } catch {
    return DEFAULT_NAME_TEMPLATE;
  }
}

let current = read();
const listeners = new Set<() => void>();

export function getNameTemplate(): string {
  return current;
}

/** Save a usable template; blank removes the key, so the built-in one applies again. Returns
 * whether the value was accepted. */
export function setNameTemplate(value: string): boolean {
  const t = value.trim();
  if (t && templateProblem(t)) return false;
  current = t || DEFAULT_NAME_TEMPLATE;
  try {
    if (t) localStorage.setItem(KEY, t);
    else localStorage.removeItem(KEY);
  } catch {
    // Storage blocked or full: the template lasts until the page closes.
  }
  listeners.forEach((f) => f());
  return true;
}

export function useNameTemplate(): string {
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    getNameTemplate,
    () => DEFAULT_NAME_TEMPLATE,
  );
}
