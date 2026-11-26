// Reading a source, and the one archive password question; the password stays in memory with that source.

import { useRef, useState } from "react";

import {
  api,
  ARCHIVE_PASSWORD_REQUIRED,
  ARCHIVE_PASSWORD_WRONG,
  errorText,
  type Inspection,
} from "./api";
import { Dots, Modal, openOverlay } from "./common";
import { gameTitle, msg, useMessages } from "./i18n";
import { Icon } from "./icons";

export interface Opened {
  ins: Inspection;
  password: string | null;
}

const isPasswordError = (e: unknown) => {
  const text = errorText(e);
  return text.includes(ARCHIVE_PASSWORD_REQUIRED) || text.includes(ARCHIVE_PASSWORD_WRONG);
};

/** Inspects `path`, asking for a password until one opens it (unless `current()` moved on); Cancel rejects with "needs a password". */
export async function inspectSource(
  path: string,
  password: string | null = null,
  current: () => boolean = () => true,
): Promise<Opened> {
  let opened: Opened;
  try {
    opened = { ins: await api.inspect(path, password ?? undefined), password };
  } catch (e) {
    if (!isPasswordError(e) || !current()) throw e;
    opened = await askPassword(path);
  }
  // No title and no title ID: not a game. The screens show it as an error, its findings saying why.
  const { ins } = opened;
  if (!ins.title_id && !gameTitle(ins.title_names, ins.title_name)) {
    throw new Error(ins.findings.length > 0 ? ins.findings.join("\n") : msg().common.unknownTitle);
  }
  return opened;
}

/** The password dialog: resolves once a password opens the archive. */
function askPassword(path: string): Promise<Opened> {
  return new Promise((resolve, reject) => {
    let opened: Opened | null = null;
    let failed: unknown = ARCHIVE_PASSWORD_REQUIRED;
    openOverlay(
      (close) => (
        <PasswordDialog
          path={path}
          close={close}
          onOpened={(o) => {
            opened = o;
            close();
          }}
          onFailed={(e) => {
            failed = e;
            close();
          }}
        />
      ),
      () => (opened ? resolve(opened) : reject(failed)),
    );
  });
}

function PasswordDialog(props: {
  path: string;
  close: () => void;
  onOpened: (o: Opened) => void;
  /** Any error but a wrong password: the screen shows it. */
  onFailed: (e: unknown) => void;
}) {
  const t = useMessages().password;
  const [value, setValue] = useState("");
  const [wrong, setWrong] = useState(false);
  const [checking, setChecking] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const unlock = async () => {
    if (!value || checking) return;
    const password = value;
    // The prompt again while checking (a second "not correct" announced anew); focus to the input, as Unlock is disabled.
    setWrong(false);
    input.current?.focus();
    setChecking(true);
    try {
      props.onOpened({ ins: await api.inspect(props.path, password), password });
    } catch (e) {
      if (!isPasswordError(e)) return props.onFailed(e);
      // Stay open for another try: the text says so, the input keeps what was typed, selected.
      setWrong(true);
      setChecking(false);
      setTimeout(() => input.current?.select(), 0);
    }
  };

  return (
    <Modal close={props.close} className="modal card password" labelledBy="pw-title" describedBy="pw-text">
      <div className="modal-head">
        <span className="head-icon">
          <Icon name="lock" />
        </span>
        <h2 id="pw-title">{t.title}</h2>
      </div>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void unlock();
        }}
      >
        <p className="out-box path">{props.path}</p>
        <p id="pw-text" className={wrong ? "bad" : "muted"} aria-live="polite">
          {wrong ? t.wrong : t.text}
        </p>
        <input
          ref={input}
          type="password"
          className="pw-input"
          autoFocus
          autoComplete="off"
          aria-label={t.label}
          aria-describedby="pw-text"
          aria-invalid={wrong || undefined}
          placeholder={t.label}
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <div className="row end">
          <button type="button" onClick={props.close}>
            {t.cancel}
          </button>
          {/* Busy, not disabled, while checking: it keeps focus and full opacity; submit ignores it. */}
          <button
            type="submit"
            className="primary"
            disabled={!value}
            aria-disabled={checking || undefined}
            aria-busy={checking || undefined}
          >
            {checking ? (
              <>
                {t.checking}
                <Dots />
              </>
            ) : (
              t.unlock
            )}
          </button>
        </div>
      </form>
    </Modal>
  );
}
