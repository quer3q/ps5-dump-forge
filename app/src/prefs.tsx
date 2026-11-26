// UI preferences (redesign prototype): theme, colour scheme, rail collapsed, advanced folds open.
// Kept in localStorage `forge.ui`; unreadable storage is the defaults. Applied to <html> as
// data-theme / data-scheme / .rail-collapsed, as kraken-mount does.

import { useSyncExternalStore, type ReactNode } from "react";

import { useMessages } from "./i18n";

export type Theme = "dark" | "light";
export const SCHEMES = ["cobalt", "abyss", "ember", "moss", "dusk", "rose", "sand", "volt", "mono"] as const;
export type Scheme = (typeof SCHEMES)[number];
export type Prefs = {
  theme: Theme;
  scheme: Scheme;
  railCollapsed: boolean;
  advancedOpen: boolean;
  /** Replace a game's ampr_emu with the bundled ampr-emu-ng: sent with every Build request as
   * `replace_ampr_emu` (PLAN.md D1). */
  replaceAmprEmu: boolean;
};

const KEY = "forge.ui";
const DEFAULTS: Prefs = { theme: "dark", scheme: "cobalt", railCollapsed: false, advancedOpen: false, replaceAmprEmu: true };

function load(): Prefs {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return {
      theme: v.theme === "light" ? "light" : "dark",
      scheme: SCHEMES.includes(v.scheme) ? v.scheme : "cobalt",
      railCollapsed: v.railCollapsed === true,
      advancedOpen: v.advancedOpen === true,
      replaceAmprEmu: v.replaceAmprEmu !== false,
    };
  } catch {
    return DEFAULTS;
  }
}

let current = load();
const subs = new Set<() => void>();

function apply(p: Prefs) {
  const el = document.documentElement;
  el.dataset.theme = p.theme;
  el.dataset.scheme = p.scheme;
  el.classList.toggle("rail-collapsed", p.railCollapsed);
}
apply(current);

export function setPrefs(patch: Partial<Prefs>) {
  current = { ...current, ...patch };
  apply(current);
  try {
    localStorage.setItem(KEY, JSON.stringify(current));
  } catch {
    // ponytail: unwritable storage keeps the choice for this page only
  }
  subs.forEach((f) => f());
}

/** The preferences now, for code outside React (a step's request). */
export const getPrefs = (): Prefs => current;

export function usePrefs(): Prefs {
  return useSyncExternalStore(
    (f) => (subs.add(f), () => void subs.delete(f)),
    () => current,
  );
}

/** A fold for pro options: closed by default, open when Settings' "Always show advanced
 * options" is on (the user can still close it; flipping the setting resets every fold).
 * `items`: what's inside, one muted line in the summary; `changed`: a "Changed" badge when a
 * value inside differs from its default, else "Defaults" (no badge when not given). */
export function Advanced(props: {
  title: string;
  hint?: string;
  items?: string[];
  changed?: boolean;
  children: ReactNode;
  className?: string;
}) {
  const { advancedOpen } = usePrefs();
  const t = useMessages().advanced;
  const joined = props.items?.length ? props.items.join(", ") : props.hint;
  const line = joined && joined[0].toUpperCase() + joined.slice(1);
  return (
    <details key={String(advancedOpen)} className={`advanced ${props.className ?? ""}`} open={advancedOpen || undefined}>
      <summary>
        <span className="advanced-title">{props.title}</span>
        {line && <span className="advanced-hint muted">{line}</span>}
        {props.changed !== undefined && (
          <span className={`tag advanced-badge ${props.changed ? "blue" : "muted"}`}>
            {props.changed ? t.changed : t.defaults}
          </span>
        )}
      </summary>
      <div className="advanced-body">{props.children}</div>
    </details>
  );
}
