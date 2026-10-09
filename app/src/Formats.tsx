// About formats: one comparison table of the targets (from ShadowMountPlus 1.7's README and
// release notes, plus this app's own measurements) and a few notes. Static text; nothing here talks to Rust.

import { CardHead, FormatPill } from "./common";
import { Icon, type IconName } from "./icons";

type Rating = "good" | "ok" | "bad" | "na";

/** Each rating's icon and spoken prefix: the colour is never the only cue. */
const RATING: Record<Rating, { icon?: IconName; say: string }> = {
  good: { icon: "check", say: "Good" },
  ok: { icon: "minus", say: "Trade-off" },
  bad: { icon: "x", say: "Drawback" },
  na: { say: "Not available" },
};

const COLUMNS = ["Runs as", "Console speed", "Size on disk", "Build here*", "Firmware"];

const ROWS: { kind: string; tagline: string; cells: [string, Rating][] }[] = [
  {
    kind: "pkg",
    tagline: "Best, if your firmware allows",
    cells: [
      ["Installed like a store game", "good"],
      ["Native, full speed", "good"],
      // Approximate: a partial run of an 89 GB game, not a measured full build.
      ["About the game size (Kraken; game data is mostly already compressed)", "ok"],
      ["~20 min", "bad"],
      ["11.60 or lower, with kstuff, fpkg-enable and ppr-patch", "bad"],
    ],
  },
  {
    kind: "ffpkg",
    tagline: "Recommended for ShadowMountPlus",
    cells: [
      ["Mounted by ShadowMountPlus", "good"],
      ["Full image speed", "good"],
      ["Game size + file-system overhead + 64 MiB spare", "ok"],
      ["About 2 min", "good"],
      ["Any with ShadowMountPlus", "good"],
    ],
  },
  {
    kind: "exfat",
    tagline: "Compatibility",
    cells: [
      ["Mounted, like external-drive content", "ok"],
      ["Full image speed", "good"],
      ["Game size + overhead + 0.5% spare (64–512 MiB)", "ok"],
      ["About 2 min", "good"],
      ["Any with ShadowMountPlus", "good"],
    ],
  },
  {
    kind: "ffpfs",
    tagline: "Experimental",
    cells: [
      ["Mounted by ShadowMountPlus, experimental", "ok"],
      ["Full image speed", "good"],
      ["Game size + overhead, uncompressed", "ok"],
      ["Not measured", "ok"],
      ["Any with ShadowMountPlus", "good"],
    ],
  },
  {
    kind: "ffpfsc",
    tagline: "Smallest, experimental",
    cells: [
      ["Mounted read-only, compressed", "ok"],
      ["150–250 MB/s, may stutter", "bad"],
      // MkPFS's figure; game data that is already compressed shrinks less.
      ["40–60% smaller (MkPFS)", "good"],
      ["Not measured", "ok"],
      ["Any with ShadowMountPlus", "good"],
    ],
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
                        {RATING[rate].icon && <Icon name={RATING[rate].icon} />}
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
        <p className="footnote muted">
          *Build times approximate, 89 GB game on Apple Silicon. Console speeds as the
          ShadowMountPlus 1.7 docs give them; ShadowMountPlus itself needs kstuff-lite 1.07+ and
          calls .ffpfs and .ffpfsc experimental. A .fpkg (debug FPKG) is saved as a .pkg file.
        </p>
        <ul className="notes">
          <li>
            Images mount read-only unless <code>image_rw=</code> in ShadowMountPlus's config.ini
            names them; a .ffpfsc always mounts read-only.
          </li>
          <li>
            A .ffpfsc holds one image: .exfat (the default; MkPFS calls it the most stable),
            .ffpkg or .ffpfs. Game data that is already compressed shrinks less.
          </li>
          <li>Keep the extension (it picks the driver) and the game at the image root.</li>
          <li>.exfat needs 64 KiB clusters; this app always writes them.</li>
          <li>
            ShadowMountPlus warns that mounting images can corrupt internal drives, more often on
            older firmware.
          </li>
        </ul>
        <p className="folder-line">
          <FormatPill kind="folder" /> Unpack an image back into files; ShadowMountPlus can also run
          a plain folder.
        </p>
      </section>
    </div>
  );
}
