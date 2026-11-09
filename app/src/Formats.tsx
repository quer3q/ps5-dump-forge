// About: the features, then one comparison of the targets and the plain folder (console speeds
// from ShadowMountPlus 1.7's README and release notes; the LZ4 packed folder's from its
// upstream authors, labelled as theirs), then the credits; the language selector sits in the
// About card's head. On the PS5, one switch saves job notices through the server.

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { APP_NAME, jobNotifications, type Format } from "./api";
import { CardHead, FormatPill, Prose } from "./common";
import { getChoice, type Lang, LANGS, setLang, useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon, type IconName } from "./icons";
import {
  DEFAULT_NAMING,
  MAX_NAMING,
  namingProblem,
  PARTS,
  render,
  SAMPLE,
  setNaming,
  STEM_MAX,
  bytes,
  cutBytes,
  type NamePart,
  type NamingProblem,
} from "./naming";
import { useNaming } from "./target";

export type Rating = "good" | "ok" | "bad";

/** Each rating's icon: the colour is never the only cue (its spoken prefix is a message). */
const RATING_ICON: Record<Rating, IconName> = { good: "check", ok: "minus", bad: "x" };

/** The features' icons by position (`formats.features`, the same nine in every language):
 * Convert, Transfer, Edit / Streamed, Archives, USB and SD cards / LZ4, Storage, Safe. */
const FEATURE_ICONS: IconName[] = [
  "convert", "move", "pencil",
  "stream", "archive", "sdcard",
  "bolt", "library", "lock",
];

/** The comparison's rows in order, with each cell's rating; the words are messages (`rows`). */
const ROWS: { kind: Format | "zstd"; rates: [Rating, Rating] }[] = [
  { kind: "pkg", rates: ["good", "good"] },
  { kind: "zstd", rates: ["good", "good"] },
  { kind: "lz4", rates: ["good", "good"] },
  { kind: "ffpkg", rates: ["good", "ok"] },
  { kind: "exfat", rates: ["good", "ok"] },
  { kind: "ffpfs", rates: ["good", "ok"] },
  { kind: "ffpfsc", rates: ["bad", "good"] },
  { kind: "folder", rates: ["good", "ok"] },
];

export type Row = { tagline: string; cells: [string, string] };
/** The nine features (name, a few words), in `FEATURE_ICONS`' order: tsc holds every language to nine. */
type Feature = [string, string];
export type Features = [Feature, Feature, Feature, Feature, Feature, Feature, Feature, Feature, Feature];

export function Formats(props: { shown: boolean }) {
  const t = useMessages().formats;
  return (
    <div className="screen">
      <section className="card" aria-labelledby="f-about">
        <CardHead icon="info" title={t.about} id="f-about">
          <LanguagePicker />
        </CardHead>
        {/* Three sets of three, told apart by order and a wider gap only (styles.css). */}
        <ul className="feat-tiles">
          {t.features.map(([name, what], i) => (
            <li className="feat-tile" key={name}>
              <span className="feat-icon">
                <Icon name={FEATURE_ICONS[i]} />
              </span>
              <div>
                <b>{name}</b>{" "}
                <span>
                  <Prose text={what} />
                </span>
              </div>
            </li>
          ))}
        </ul>
        <NotificationSwitch shown={props.shown} />
      </section>

      <NamingCard shown={props.shown} />

      <section className="card" aria-labelledby="f-title">
        <CardHead icon="table" title={t.compare} id="f-title" />
        {/* A table by its roles, laid out as rows edged in each format's hue. The heading row is
            read always and shown from 760px; under it each rating shows its own label, which
            screen readers skip (the column header names it). */}
        <div className="fmt-table" role="table" aria-label={t.compareLabel}>
          <div className="fmt-row fmt-head" role="row">
            <span role="columnheader">{t.format}</span>
            {t.columns.map((c) => (
              <span role="columnheader" key={c}>
                {c}
              </span>
            ))}
          </div>
          {ROWS.map((r) => (
            <div className={`fmt-row fmt-${r.kind}`} role="row" key={r.kind}>
              <div className="fmt-what" role="rowheader">
                <FormatPill kind={r.kind} />
                <span className="tagline">{t.rows[r.kind].tagline}</span>
              </div>
              {r.rates.map((rate, i) => (
                <div className="fmt-rate" role="cell" key={i}>
                  <span className="fmt-q" aria-hidden="true">
                    {t.columns[i]}
                  </span>
                  <span className={`chip ${rate}`}>
                    <Icon name={RATING_ICON[rate]} />
                    <span>
                      <span className="visually-hidden">{t.rating[rate]}: </span>
                      {t.rows[r.kind].cells[i]}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </section>

      <section className="card compact" aria-labelledby="f-credits">
        <CardHead icon="heart" title={t.credits} id="f-credits" />
        {/* URLs as text to copy: neither the app nor the console page navigates away. */}
        <ul className="credit-list">
          <li>{t.upstream(APP_NAME)}</li>
          <li>
            {t.bundled} <span className="path" dir="ltr">https://github.com/drakmor/ampr_emu</span>
          </li>
          <li>
            {t.lazy} <span className="path" dir="ltr">https://github.com/Nazky/Lazy_AMPR</span>
          </li>
          <li>
            {t.ng} <span className="path" dir="ltr">https://github.com/quer3q/ampr-emu-ng</span>
          </li>
          <li>{t.unrar}</li>
          <li>
            {t.logo} <span className="path" dir="ltr">https://github.com/Xxcco</span>
          </li>
        </ul>
      </section>
    </div>
  );
}

/** Output names (by user decision): one template, typed freely, with chips that insert a
 * token at the cursor; kept by naming.ts. An invalid template says why and isn't saved. */
function NamingCard(props: { shown: boolean }) {
  const t = useMessages().naming;
  const naming = useNaming();
  const [draft, setDraft] = useState(naming);
  // Reset, or a save from elsewhere in this page: the box follows the saved template.
  const seen = useRef(naming);
  if (seen.current !== naming) {
    seen.current = naming;
    if (draft !== naming) setDraft(naming);
  }
  const input = useRef<HTMLInputElement>(null);
  // Where the caret goes once an inserted token renders.
  const [caret, setCaret] = useState<number | null>(null);
  useLayoutEffect(() => {
    if (caret === null || !input.current) return;
    input.current.focus();
    input.current.setSelectionRange(caret, caret);
    setCaret(null);
  }, [caret]);
  // Saved 500 ms after the last keystroke (each save makes open tabs name their output again,
  // which reads the source), and at once when the box loses focus, the tab hides, the page goes
  // or the card unmounts, so a job started right after an edit uses it.
  const pending = useRef<{ timer: number; value: string } | null>(null);
  const flush = () => {
    if (!pending.current) return;
    clearTimeout(pending.current.timer);
    setNaming(pending.current.value); // ignored while invalid
    pending.current = null;
  };
  useEffect(() => {
    window.addEventListener("pagehide", flush);
    return () => {
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, []);
  useEffect(() => {
    if (!props.shown) flush();
  }, [props.shown]);
  const change = (v: string) => {
    setDraft(v);
    if (pending.current) clearTimeout(pending.current.timer);
    pending.current = { timer: window.setTimeout(flush, 500), value: v };
  };
  const insert = (p: NamePart) => {
    const el = input.current;
    const at = el?.selectionStart ?? draft.length;
    const to = el?.selectionEnd ?? at;
    const token = `{${p}}`;
    change(draft.slice(0, at) + token + draft.slice(to));
    setCaret(at + token.length);
  };
  const problem = namingProblem(draft);
  const error = problem && problemText(t.errors, problem);
  const atDefault = draft === DEFAULT_NAMING && naming === DEFAULT_NAMING;
  // The example follows the box while it is valid, else the saved template; one rule for
  // every job, so a Pack's kind is in (a Convert leaves `{pack}` empty).
  const shown = problem ? naming : draft;
  // What the template takes without the title: the rest of the 63 bytes is the title's.
  // As core cuts: the version goes when the rest doesn't fit even without the title, then each
  // `{title}` gets an equal share of what is left.
  const titles = (shown.match(/\{title\}/g) ?? []).length;
  const dropVersion = bytes(render(shown, (p) => (p === "title" ? "" : SAMPLE[p]))) > STEM_MAX;
  const kept = render(shown, (p) => (p === "version" && dropVersion ? "" : SAMPLE[p]));
  const others = bytes(kept) - titles * bytes(SAMPLE.title);
  const room = titles ? Math.floor((STEM_MAX - others) / titles) : null;
  // No title to cut: the whole name is cut at 63 bytes.
  const cutAway = titles === 0 && bytes(kept) > STEM_MAX;
  // The example as core names it: the title cut to its share, then the whole within 63 bytes.
  const cutName =
    bytes(kept) <= STEM_MAX
      ? kept
      : cutBytes(
          render(shown, (p) =>
            p === "title" ? cutBytes(SAMPLE.title, Math.max(0, room ?? 0)) : p === "version" && dropVersion ? "" : SAMPLE[p],
          ),
          STEM_MAX,
        );
  // As core: a cut that took the game away (template text filling the room before the title
  // id) names it by its title id instead.
  const keptGame =
    (shown.includes("{title_id}") && cutName.includes(SAMPLE.title_id)) ||
    (shown.includes("{title}") && cutName.includes(SAMPLE.title[0]));
  const fitted = keptGame ? cutName : SAMPLE.title_id;
  // Anything cut or left out for the limit, and the one line that says what.
  const cutting = !keptGame || bytes(kept) > STEM_MAX || dropVersion || (room !== null && room <= 0);
  const why = !keptGame
    ? t.idOnly
    : cutAway
      ? t.cut
    : room !== null && room <= 0
      ? t.noRoom
      : dropVersion
        ? t.versionOut
        : room !== null
          ? t.room(room)
          : null;
  return (
    <section className="card" aria-labelledby="f-naming">
      <CardHead icon="file" title={t.title} id="f-naming">
        <button
          type="button"
          className="small"
          aria-disabled={atDefault || undefined}
          onClick={() => {
            if (atDefault) return;
            if (pending.current) clearTimeout(pending.current.timer);
            pending.current = null;
            setDraft(DEFAULT_NAMING);
            setNaming(DEFAULT_NAMING);
          }}
        >
          <Icon name="undo" />
          {t.reset}
        </button>
      </CardHead>
      <label className="field-label naming-field">
        {/* No visible caption (the card's title says it); screen readers still hear it. */}
        <span className="visually-hidden">{t.template}</span>
        <input
          ref={input}
          className="mono"
          dir="ltr"
          value={draft}
          onChange={(e) => change(e.target.value)}
          onBlur={flush}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? "naming-error" : undefined}
          spellCheck={false}
          autoCapitalize="none"
          autoCorrect="off"
        />
      </label>
      {error && (
        <p className="bad field-error" id="naming-error">
          {error} {t.notSaved}
        </p>
      )}
      <div className="naming-parts" role="group" aria-label={t.partsLabel}>
        {PARTS.map((p) => (
          <button
            type="button"
            key={p}
            className="kind-chip naming-chip"
            aria-label={draft.includes(`{${p}}`) ? t.inUse(t.parts[p]) : t.insert(t.parts[p])}
            // Each part once: its chip is off while the template has it.
            aria-disabled={draft.includes(`{${p}}`) || undefined}
            title={draft.includes(`{${p}}`) ? t.inUse(t.parts[p]) : undefined}
            // The input keeps focus (and its selection) through a click.
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => !draft.includes(`{${p}}`) && insert(p)}
          >
            <Icon name={draft.includes(`{${p}}`) ? "check" : "plus"} />
            {t.parts[p]}
          </button>
        ))}
      </div>
      <dl className="naming-example">
        <dt className="muted">{t.example}</dt>
        <dd className="mono" dir="ltr">
          {fitted}
        </dd>
        <dt className="muted">{t.length}</dt>
        <dd className={cutting ? "over" : undefined} aria-live="polite">
          <span className="naming-count">{t.bytes(bytes(fitted), STEM_MAX)}</span>
          {why && <span className="naming-why">{why}</span>}
        </dd>
      </dl>
      {/* The limit is a warning only while this template is being cut. */}
      <p className={`note-line naming-note ${cutting ? "warn" : "muted"}`}>
        <Icon name={cutting ? "warn" : "info"} />
        <span>
          <Prose text={t.limit} />
        </span>
      </p>
    </section>
  );
}

/** A template problem in this language. */
function problemText(e: Messages["naming"]["errors"], p: NamingProblem): string {
  switch (p.kind) {
    case "long":
      return e.long(MAX_NAMING);
    case "unknown":
      return e.unknown(p.token);
    case "twice":
      return e.twice(p.token);
    case "char":
      return e.char(p.char);
    default:
      return e[p.kind];
  }
}

/** The console keeps this one choice for jobs started here; read again each time About shows
 * (another viewer may have changed it). */
function NotificationSwitch({ shown }: { shown: boolean }) {
  const t = useMessages().formats;
  const [on, setOn] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!shown) return;
    let live = true;
    void jobNotifications.get().then((value) => {
      if (live) setOn(value);
    }).catch(() => {});
    return () => { live = false; };
  }, [shown]);

  if (on === null) return null;
  const change = (next: boolean) => {
    if (busy) return;
    const previous = on;
    setOn(next);
    setBusy(true);
    setFailed(false);
    void jobNotifications.set(next).then((saved) => {
      if (saved === null) throw new Error("Notification setting unavailable");
      setOn(saved);
    }).catch(() => {
      setOn(previous);
      setFailed(true);
    }).finally(() => setBusy(false));
  };

  return (
    <div className="field notifications">
      <label className="switch">
        <input type="checkbox" className="visually-hidden" checked={on}
          // Not `disabled`: a focused control that turns disabled drops focus to the page.
          aria-disabled={busy || undefined}
          onChange={(e) => change(e.target.checked)} />
        <span className="switch-track" aria-hidden="true" />
        <span>{t.notifications}<span className="muted hint">{t.notificationsHint}</span></span>
      </label>
      {failed && <p className="bad" role="alert">{t.notificationsFailed}</p>}
    </div>
  );
}

type Choice = Lang | "system";

/** The UI language, right in the About card's header: the system's, or one chosen here (kept by
 * i18n.ts). A native select drawn as a small pill (DESIGN.md), each language named in its own
 * words; it holds any number of them. */
function LanguagePicker() {
  const t = useMessages().formats;
  // Local too: the store switches only once the language's file has loaded.
  const [choice, setChoice] = useState<Choice>(getChoice() ?? "system");
  const latest = useRef(choice);
  const choose = (c: Choice) => {
    latest.current = c;
    setChoice(c);
    // A language file that can't load leaves the language as it was: so does the picker, unless
    // a later choice has replaced this one meanwhile.
    setLang(c === "system" ? null : c).catch(() => {
      if (latest.current === c) setChoice(getChoice() ?? "system");
    });
  };
  return (
    <label className="head-lang">
      <span>{t.language}</span>
      <select
        id="ui-language"
        className="lang-select"
        value={choice}
        onChange={(e) => choose(e.target.value as Choice)}
      >
        <option value="system">{t.system}</option>
        {(Object.entries(LANGS) as [Lang, string][]).map(([l, name]) => (
          <option key={l} value={l} lang={l}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}
