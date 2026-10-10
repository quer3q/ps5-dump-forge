// The LZ4 tab's "How LZ4 works": a short how-to in a modal over the inert app (common.tsx
// `openOverlay`). Text only.

import { Modal, openOverlay } from "./common";
import { Icon } from "./icons";

/** Opens the how-to; focus returns to the opener when it closes. */
export function openLz4Help() {
  openOverlay((close) => <Lz4Help close={close} />);
}

function Lz4Help({ close }: { close: () => void }) {
  return (
    <Modal close={close} className="modal card help" labelledBy="lz4-help-title">
      <div className="modal-head">
        <span className="head-icon lz4">
          <Icon name="info" />
        </span>
        <h2 id="lz4-help-title">How LZ4 works</h2>
      </div>
      {/* The body scrolls; the title and Close stay put. tabIndex: keys scroll it too. */}
      <div className="help-body" tabIndex={0} aria-label="How LZ4 works">
        <p className="note-line warn">
          <Icon name="warn" />
          <span>
            Experimental, for AMPR games only. ampr_emu 0.4.2.1 is a test build: some games crash
            when saving. Keep your original dump.
          </span>
        </p>

        <h3>1. Trace</h3>
        <ol>
          <li>
            Choose the game's <b>ffpkg</b> or <b>exfat</b>, then <b>Trace</b> → <b>Patch</b>.
          </li>
          <li>Turn off the fakelib updater: it overwrites the trace runtime.</li>
          <li>
            Mount it with <span className="path">image_rw=</span>, play one session, close the game.
          </li>
          <li>
            <b>Download traces</b> from the PS5 page, or copy ampr_commands.bin and ampr_emu.index.
          </li>
        </ol>

        <h3>2. Pack/Unpack</h3>
        <ol>
          <li>
            Choose the original dump, <b>Pack/Unpack</b>, and the traces (or a profile).
          </li>
          <li>Write a packed folder or a plain image; a folder converts to any format in Convert.</li>
          <li>A packed source shows <b>Unpack</b> instead: the loose files come back.</li>
        </ol>
      </div>
      <div className="row end">
        <button type="button" autoFocus onClick={close}>
          Close
        </button>
      </div>
    </Modal>
  );
}
