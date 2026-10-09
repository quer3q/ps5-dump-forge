// About formats: one comparison table of the targets and the plain folder (console speeds from
// ShadowMountPlus 1.7's README and release notes). Static text; nothing here talks to Rust.

import { CardHead, FormatPill } from "./common";
import { Icon, type IconName } from "./icons";

type Rating = "good" | "ok" | "bad";

/** Each rating's icon and spoken prefix: the colour is never the only cue. */
const RATING: Record<Rating, { icon: IconName; say: string }> = {
  good: { icon: "check", say: "Good" },
  ok: { icon: "minus", say: "Trade-off" },
  bad: { icon: "x", say: "Drawback" },
};

const COLUMNS = ["Console speed", "Size on disk"];

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT: [string, Rating] = ["Equal to the game size", "ok"];

const ROWS: { kind: string; tagline: string; cells: [string, Rating][] }[] = [
  {
    kind: "pkg",
    tagline: "Best, if your firmware allows",
    cells: [
      ["Native, full speed", "good"],
      ["30–60% less, depending on the game", "good"],
    ],
  },
  {
    kind: "ffpkg",
    tagline: "Recommended for ShadowMountPlus",
    cells: [["Full image speed", "good"], FLAT],
  },
  {
    kind: "exfat",
    tagline: "Compatibility",
    cells: [["Full image speed", "good"], FLAT],
  },
  {
    kind: "ffpfs",
    tagline: "Experimental",
    cells: [["Full image speed", "good"], FLAT],
  },
  {
    kind: "ffpfsc",
    tagline: "Smallest, experimental",
    cells: [
      ["150–250 MB/s, may stutter", "bad"],
      ["Depends on the compression level, usually 30–60% less", "good"],
    ],
  },
  {
    kind: "folder",
    tagline: "Plain game files",
    cells: [["Full drive speed", "good"], FLAT],
  },
];

export function Formats() {
  return (
    <div className="screen">
      <section className="card formats-card" aria-labelledby="f-title">
        <CardHead icon="table" title="Compare formats" id="f-title" />
        <div className="table-wrap" tabIndex={0} role="region" aria-label="Format comparison">
          <table className="compare">
            <thead>
              <tr>
                <th scope="col">Format</th>
                {COLUMNS.map((c) => (
                  <th scope="col" key={c}>
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {ROWS.map((r) => (
                <tr key={r.kind}>
                  <th scope="row">
                    <FormatPill kind={r.kind} />
                    <span className="tagline">{r.tagline}</span>
                  </th>
                  {r.cells.map(([text, rate], i) => (
                    <td key={i}>
                      <span className={`chip ${rate}`}>
                        <Icon name={RATING[rate].icon} />
                        <span>
                          <span className="visually-hidden">{RATING[rate].say}: </span>
                          {text}
                        </span>
                      </span>
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
