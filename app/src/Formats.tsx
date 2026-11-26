// About: one comparison of the targets and the plain folder (console speeds
// from ShadowMountPlus 1.7's README and release notes; the LZ4 packed folder's from its
// upstream authors, labelled as theirs), then the credits. The language, output names and the
// PS5's notification switch live in Settings.tsx.

import { APP_NAME, type Format } from "./api";
import { CardHead, FormatPill } from "./common";
import { useMessages } from "./i18n";
import { Icon, type IconName } from "./icons";

export type Rating = "good" | "ok" | "bad";

/** Each rating's icon: the colour is never the only cue (its spoken prefix is a message). */
const RATING_ICON: Record<Rating, IconName> = { good: "check", ok: "minus", bad: "x" };

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

export function Formats(_props: { shown: boolean }) {
  const t = useMessages().formats;
  return (
    <div className="screen">
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
