// The Connect tab's "How it works": in the desktop app first a highlighted Install on PS5 box, then
// a few short paragraphs, no headings (pairing and the check number, a NAS, File Transfer, Speed test, phones
// in the web build, what the network can see), in a modal over the inert app (common.tsx
// `openOverlay`). Text only.

import { web } from "./api";
import { Head, Modal, openOverlay } from "./common";
import { useMessages } from "./i18n";
import { Icon } from "./icons";

/** Opens the how-to; focus returns to the opener when it closes. */
export function openConnectHelp() {
  openOverlay((close) => <ConnectHelp close={close} />);
}

function ConnectHelp({ close }: { close: () => void }) {
  const t = useMessages().connect;
  // Phones only where there is a page to open (the desktop app has no QR code).
  const parts = [...t.help, t.helpSpeed, ...(web ? [t.helpPhone] : []), t.helpSecurity];
  return (
    <Modal close={close} className="modal card help" labelledBy="connect-help-title">
      <Head id="connect-help-title" icon="info" title={t.howItWorks} />
      {/* The body scrolls; the title and Close stay put. tabIndex: keys scroll it too. */}
      <div className="help-body connect-help" role="region" tabIndex={0} aria-label={t.howItWorks}>
        {!web && (
          <div className="install-lead help-install">
            <Icon name="bolt" />
            <div>
              <strong>{t.helpInstallTitle}</strong>
              <p>{t.helpInstallText}</p>
            </div>
          </div>
        )}
        {parts.map((text) => (
          <p key={text}>{text}</p>
        ))}
      </div>
      <div className="row end">
        <button type="button" autoFocus onClick={close}>
          {t.close}
        </button>
      </div>
    </Modal>
  );
}
