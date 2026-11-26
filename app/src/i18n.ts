// UI languages: `locales/en.tsx` is the source and defines `Messages`; every other file is typed as it. Each loads as its own chunk before it shows (`initLang`, `setLang`). Components use `useMessages()`, other code `msg()` at call time, never at module load. A new language: copy en.tsx, translate it, add its line below.

import { useSyncExternalStore } from "react";

// English is in the main bundle; ponytail: en.tsx imports num/plural/dotted back from here, a cycle safe while no message runs at module load.
import en, { type Messages } from "./locales/en";

/** `numbers`: the Intl locale for digits when it isn't the tag (Arabic keeps Latin digits). */
type Info = { name: string; load: () => Promise<{ default: Messages }>; rtl?: boolean; numbers?: string; tag?: string };

/** `rtl`: the page gets `dir="rtl"`; `tag` where `<html lang>` needs more than the key. */
const LANGUAGES = {
  en: { name: "English", load: async () => ({ default: en }) },
  ru: { name: "Русский", load: () => import("./locales/ru") },
  zh: { name: "简体中文", load: () => import("./locales/zh"), tag: "zh-Hans" },
  de: { name: "Deutsch", load: () => import("./locales/de") },
  es: { name: "Español", load: () => import("./locales/es") },
  fr: { name: "Français", load: () => import("./locales/fr") },
  ja: { name: "日本語", load: () => import("./locales/ja") },
  pt: { name: "Português (Brasil)", load: () => import("./locales/pt"), tag: "pt-BR" },
  it: { name: "Italiano", load: () => import("./locales/it") },
  ko: { name: "한국어", load: () => import("./locales/ko") },
  ar: { name: "العربية", load: () => import("./locales/ar"), rtl: true, numbers: "ar-u-nu-latn" },
} satisfies Record<string, Info>;

export type Lang = keyof typeof LANGUAGES;

export const LANGS = Object.fromEntries(Object.entries(LANGUAGES).map(([l, v]) => [l, v.name])) as Record<
  Lang,
  string
>;

const isLang = (v: string): v is Lang => Object.prototype.hasOwnProperty.call(LANGUAGES, v);

const KEY = "forge.lang";

function stored(): Lang | null {
  try {
    const v = localStorage.getItem(KEY);
    return v !== null && isLang(v) ? v : null;
  } catch {
    return null;
  }
}

function systemLangs(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : navigator.language ? [navigator.language] : [];
}

/** The same tag (`pt-BR`), else its language (`pt`). */
function match(tag: string): Lang | null {
  const t = tag.toLowerCase();
  const langs = Object.keys(LANGUAGES) as Lang[];
  return langs.find((l) => l.toLowerCase() === t) ?? langs.find((l) => l.toLowerCase() === t.split("-")[0]) ?? null;
}

function detect(): Lang {
  for (const tag of systemLangs()) {
    const l = match(tag);
    if (l) return l;
  }
  return "en";
}

let choice: Lang | null = stored();
let lang: Lang = choice ?? detect();
let messages: Messages | undefined;
const loaded = new Map<Lang, Messages>();
const subs = new Set<() => void>();
/** Bumped on every switch: `choice` can change while `lang` stays (System → English). */
let version = 0;
/** The latest switch; an older one that loads later is dropped. */
let switchSeq = 0;

async function load(l: Lang): Promise<Messages> {
  let m = loaded.get(l);
  if (!m) loaded.set(l, (m = (await LANGUAGES[l].load()).default));
  return m;
}

function apply() {
  if (typeof document === "undefined") return;
  const info: Info = LANGUAGES[lang];
  document.documentElement.lang = info.tag ?? lang;
  document.documentElement.dir = info.rtl ? "rtl" : "ltr";
}

/** English (bundled, never fails) if the language in effect fails; await it before the first render. */
export async function initLang(): Promise<void> {
  try {
    messages = await load(lang);
  } catch {
    if (choice) choice = "en";
    lang = "en";
    messages = en;
  }
  apply();
}

export const getLang = (): Lang => lang;
export const getChoice = (): Lang | null => choice;

/** Null follows the system again; a file that can't load leaves the language as it was. */
export async function setLang(l: Lang | null): Promise<void> {
  const seq = ++switchSeq;
  const next = l ?? detect();
  const m = await load(next);
  if (seq !== switchSeq) return;
  choice = l;
  lang = next;
  messages = m;
  try {
    if (l) localStorage.setItem(KEY, l);
    else localStorage.removeItem(KEY);
  } catch {
  }
  apply();
  version++;
  subs.forEach((f) => f());
}

function subscribe(f: () => void) {
  subs.add(f);
  return () => subs.delete(f);
}

export function useLang(): Lang {
  useSyncExternalStore(subscribe, () => version);
  return lang;
}

export function useMessages(): Messages {
  useLang();
  return msg();
}

export function msg(): Messages {
  if (!messages) throw new Error("i18n: initLang() has not finished");
  return messages;
}

export const dotted = (ext: string[], or: string) => ext.map((e) => `.${e}`).join(or);

const rules = new Map<Lang, Intl.PluralRules>();

export function plural(n: number, forms: Partial<Record<Intl.LDMLPluralRule, string>> & { other: string }): string {
  let r = rules.get(lang);
  if (!r) rules.set(lang, (r = new Intl.PluralRules(lang)));
  return forms[r.select(n)] ?? forms.other;
}

const numberFormats = new Map<string, Intl.NumberFormat>();

export function num(n: number, digits = 0): string {
  const info: Info = LANGUAGES[lang];
  const locale = info.numbers ?? lang;
  const key = `${locale}|${digits}`;
  let f = numberFormats.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });
    numberFormats.set(key, f);
  }
  return f.format(n);
}

/** Best first: Chinese by script, a bare tag with the region PS5 titles usually carry (en-US, pt-BR; es-419 outside Spain). */
function titleTags(tag: string): string[] {
  const [lang, region = ""] = tag.toLowerCase().split("-");
  if (lang === "zh") return [/^(tw|hk|mo|hant)$/.test(region) ? "zh-Hant" : "zh-Hans"];
  if (lang === "es") return [tag, region && region !== "es" ? "es-419" : "es-ES"];
  const usual: Record<string, string> = { en: "en-US", pt: "pt-BR", fr: "fr-FR" };
  return usual[lang] ? [tag, usual[lang]] : [tag];
}

/** From `localizedParameters` in the UI's language, else the system languages, else English, else `fallback`. */
export function gameTitle(names: Record<string, string> | null | undefined, fallback: string | null): string | null {
  if (names) {
    const tags = Object.keys(names);
    for (const group of [...(choice ? [choice] : []), ...systemLangs(), "en-US"].map(titleTags))
      for (const same of [(t: string, w: string) => t === w, (t: string, w: string) => t.split("-")[0] === w.split("-")[0]])
        for (const w of group) {
          const hit = tags.find((t) => same(t.toLowerCase(), w.toLowerCase()));
          if (hit && names[hit]) return names[hit];
        }
  }
  return fallback;
}
