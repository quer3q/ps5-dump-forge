// About: the features, then one comparison of the targets and the plain folder (console speeds
// from ShadowMountPlus 1.7's README and release notes; the LZ4 packed folder's from its
// upstream authors, labelled as theirs), then the credits; the language selector sits in the
// About card's head. On the PS5, one switch saves job notices through the server.

import { useEffect, useRef, useState } from "react";

import { APP_NAME, jobNotifications, type Format } from "./api";
import { CardHead, FormatPill, Prose } from "./common";
import { getChoice, type Lang, LANGS, setLang, useMessages } from "./i18n";
import { Icon, type IconName } from "./icons";

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
