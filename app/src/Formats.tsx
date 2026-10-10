// About: what the app is and (http build) how to reach the page at its own address, then
// one comparison table of the targets and the plain folder (console speeds
// from ShadowMountPlus 1.7's README and release notes; the LZ4 packed folder's from its
// upstream authors, labelled as theirs) and the LZ4 credits. Nothing here talks to Rust.

import { serverAddress } from "./api";
import { AddressQr, CardHead, FormatPill, Modal, openOverlay, Prose } from "./common";
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
    kind: "lz4",
    tagline: "Experimental, for games that use AMPR",
    cells: [
      ["30–40% faster loading, upstream's numbers", "good"],
      ["About half the game size, upstream's numbers", "good"],
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

/** About's feature list: name, then a few words. */
const FEATURES: [string, string][] = [
  ["Convert", "between a folder, ffpkg, exfat, ffpfs, ffpfsc and fpkg, both ways."],
  ["Inspect", "a dump's cover, firmware, backport, DLC and file name problems before you build."],
  ["Verify", "every output after writing: a fast sample by default, every byte on request."],
  ["Delete", "a dump from the disk, after you confirm."],
  ["LZ4", "trace an AMPR game, then pack its assets (experimental)."],
  ["AMPR runtime", "install or update ampr_emu in the converted copy."],
  ["PS5 payload", "the same app on the console, open from any device on your network."],
];

/** The header's address button: the big QR code and where to open the page, in a dialog (only
 * the http build has an address to show). */
export function openConnect() {
  const address = serverAddress();
  openOverlay((close) => (
    <Modal close={close} className="modal card connect-modal" labelledBy="connect-title">
      <div className="modal-head">
        <span className="head-icon">
          <Icon name="power" />
        </span>
        <h2 id="connect-title">How to connect</h2>
      </div>
      <div className="connect">
        <AddressQr size={240} />
        <p className="connect-text">
          Feel free to connect any device running on your local network via{" "}
          <span className="path url">http://{address}</span> or scan this code.
        </p>
      </div>
      <div className="row end">
        <button type="button" autoFocus onClick={close}>
          Close
        </button>
      </div>
    </Modal>
  ));
}

export function Formats() {
  const address = serverAddress();
  return (
    <div className="screen">
      {/* The PS5 page: About and How to connect side by side; the app, with no address, About only. */}
      <div className={address ? "about-row" : undefined}>
        <section className="card" aria-labelledby="f-about">
          <CardHead icon="info" title="About" id="f-about" />
          <p className="about-text">
            This tool can convert any dump format to another one, on any platform, including PS5.
          </p>
          <ul className="about-text features">
            {FEATURES.map(([name, what]) => (
              <li key={name}>
                <b>{name}</b> <Prose text={what} />
              </li>
            ))}
          </ul>
        </section>
        {address && (
          <section className="card about-connect" aria-labelledby="f-connect">
            <CardHead title="How to connect on any device" id="f-connect" />
            <AddressQr size={200} />
            <p className="about-text">
              Use <span className="path url">http://{address}</span> or scan this code.
            </p>
          </section>
        )}
      </div>

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
        <div className="credits">
          <p className="muted">
            Upstream's numbers: what the authors of ampr_emu and Lazy_AMPR report, not measured by
            PS5 Dump Forge; they vary by game. ampr_emu 0.4.2.1 is an upstream test build; known
            issue: some games crash when saving.
          </p>
          <p className="muted">
            LZ4 packed folders run on ampr_emu 0.4.2.1 by drakmor (GPL-3.0), bundled unmodified.
            Source: <span className="path">https://github.com/drakmor/ampr_emu</span>
          </p>
          <p className="muted">
            The trace-then-pack workflow follows Lazy_AMPR by Nazky:{" "}
            <span className="path">https://github.com/Nazky/Lazy_AMPR</span>
          </p>
        </div>
      </section>
    </div>
  );
}
