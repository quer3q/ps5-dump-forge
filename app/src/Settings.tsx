// Settings: appearance, advanced, the UI language, output names (issue #33) and, on the PS5, the job-end notification switch.

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { api, jobNotifications, runtimePlatform } from "./api";
import { CardHead } from "./common";
import { getChoice, type Lang, LANGS, setLang, useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";
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
import { SCHEMES, setPrefs, usePrefs, type Theme } from "./prefs";
import { useNaming } from "./target";

const THEMES: Theme[] = ["dark", "light"];

export function Settings(props: { shown: boolean }) {
  const prefs = usePrefs();
  const f = useMessages().formats;
  const S = useMessages().settings;
  const [ng, setNg] = useState<string | null>(null);
  const [ps5, setPs5] = useState(false);
  useEffect(() => {
    runtimePlatform().then((p) => setPs5(p === "ps5"), () => {});
  }, []);
  useEffect(() => {
    let gone = false;
    api
      .bundledNgVersion()
      .then((v) => !gone && setNg(v))
      .catch(() => !gone && setNg(null));
    return () => {
      gone = true;
    };
  }, []);
  return (
    <div className="screen settings-screen">
      <section className="card settings-group" aria-labelledby="s-appearance">
        <CardHead icon="settings" title={S.appearance} id="s-appearance" />
        <div className="settings-row">
          <div className="settings-text">
            <span className="settings-label">{S.theme}</span>
            <span className="muted hint">{S.themeHint}</span>
          </div>
          <div className="seg" role="group" aria-label={S.theme}>
            {THEMES.map((th) => (
              <button
                type="button"
                key={th}
                className={`seg-opt${prefs.theme === th ? " on" : ""}`}
                aria-pressed={prefs.theme === th}
                onClick={() => setPrefs({ theme: th })}
              >
                {th === "dark" ? S.dark : S.light}
              </button>
            ))}
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-text">
            <span className="settings-label">{S.scheme}</span>
            <span className="muted hint">{S.schemeHint}</span>
          </div>
          <div className="swatches" role="group" aria-label={S.scheme}>
            {SCHEMES.map((sc) => (
              <button
                type="button"
                key={sc}
                className="dot"
                data-scheme={sc}
                aria-pressed={prefs.scheme === sc}
                aria-label={S.schemes[sc]}
                title={S.schemes[sc]}
                onClick={() => setPrefs({ scheme: sc })}
              />
            ))}
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-text">
            <label className="settings-label" htmlFor="ui-language">
              {f.language}
            </label>
            <span className="muted hint">{S.languageHint}</span>
          </div>
          <LanguagePicker />
        </div>
      </section>


      <NamingCard shown={props.shown} />

      <NotificationGroup shown={props.shown} />

      <section className="card settings-group" aria-labelledby="s-advanced">
        <CardHead icon="scale" title={S.advanced} id="s-advanced" />
        <label className="settings-row">
          <span className="settings-text">
            <span className="settings-label">{S.advancedOpen}</span>
            <span className="muted hint">{S.advancedHint}</span>
          </span>
          <span className="switch">
            <input
              type="checkbox"
              className="visually-hidden"
              checked={prefs.advancedOpen}
              onChange={(e) => setPrefs({ advancedOpen: e.target.checked })}
            />
            <span className="switch-track" aria-hidden="true" />
          </span>
        </label>
        {/* The replace switch is sent with every Build request as `replace_ampr_emu`. */}
        {ng && (
          <div className="settings-row">
            <span className="settings-text">
              <span className="settings-label">ampr-emu-ng</span>
            </span>
            <span className="tag">{S.bundled(ng)}</span>
          </div>
        )}
        <label className="settings-row">
          <span className="settings-text">
            <span className="settings-label">{S.replaceAmprEmu}</span>
          </span>
          <span className="switch">
            <input
              type="checkbox"
              className="visually-hidden"
              checked={prefs.replaceAmprEmu}
              onChange={(e) => setPrefs({ replaceAmprEmu: e.target.checked })}
            />
            <span className="switch-track" aria-hidden="true" />
          </span>
        </label>
        {ps5 && (
          <label className="settings-row">
            <span className="settings-text">
              <span className="settings-label">{S.installedGames}</span>
              <span className="muted hint">{S.installedGamesHint}</span>
            </span>
            <span className="switch">
              <input
                type="checkbox"
                className="visually-hidden"
                checked={prefs.installedGames}
                onChange={(e) => setPrefs({ installedGames: e.target.checked })}
              />
              <span className="switch-track" aria-hidden="true" />
            </span>
          </label>
        )}
      </section>
    </div>
  );
}
/** An invalid template says why and isn't saved. */
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
  // Saved 500 ms after the last keystroke (each save renames open outputs, which reads the source), and at once on blur, hide or unmount.
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
  // The example follows the box while it is valid, else the saved template.
  const shown = problem ? naming : draft;
  // What the template takes without the title: the rest of the 63 bytes is the title's.
  // As core cuts: the version goes when the rest doesn't fit even without the title.
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
    <section className="card settings-group" aria-labelledby="f-naming">
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

/** Read again each time Settings shows (another viewer may have changed it). */
function NotificationGroup({ shown }: { shown: boolean }) {
  const t = useMessages().formats;
  const S = useMessages().settings;
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
    <section className="card settings-group" aria-labelledby="s-ps5">
      <CardHead icon="power" title={S.notificationsGroup} id="s-ps5" />
      <label className="settings-row">
        <span className="settings-text">
          <span className="settings-label">{t.notifications}</span>
          <span className="muted hint">{t.notificationsHint}</span>
        </span>
        <span className="switch">
          <input type="checkbox" className="visually-hidden" checked={on}
            // Not `disabled`: a focused control that turns disabled drops focus to the page.
            aria-disabled={busy || undefined}
            onChange={(e) => change(e.target.checked)} />
          <span className="switch-track" aria-hidden="true" />
        </span>
      </label>
      {failed && <p className="bad" role="alert">{t.notificationsFailed}</p>}
    </section>
  );
}

type Choice = Lang | "system";

/** A native select drawn as a field. */
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
  );
}
