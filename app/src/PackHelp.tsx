// The Pack tab's "How pack works": a short how-to in a modal over the inert app (common.tsx
// `openOverlay`). Text only.


import { Modal, openOverlay } from "./common";
import { useMessages } from "./i18n";
import { Icon } from "./icons";

/** Opens the how-to; focus returns to the opener when it closes. */
export function openPackHelp() {
  openOverlay((close) => <PackHelp close={close} />);
}

function PackHelp({ close }: { close: () => void }) {
  const t = useMessages().packHelp;
  return (
    <Modal close={close} className="modal card help" labelledBy="pack-help-title">
      <div className="modal-head">
        <span className="head-icon lz4">
          <Icon name="info" />
        </span>
        <h2 id="pack-help-title">{t.title}</h2>
      </div>
      {/* The body scrolls; the title and Close stay put. tabIndex: keys scroll it too. */}
      <div className="help-body" role="region" tabIndex={0} aria-label={t.title}>
        {t.body()}
      </div>
      <div className="row end">
        <button type="button" autoFocus onClick={close}>
          {t.close}
        </button>
      </div>
    </Modal>
  );
}
