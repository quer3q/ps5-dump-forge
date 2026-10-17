// Reading a source, and the one password question for an archive that needs one (built like
// the delete question). Edit, Convert and LZ4 read their source through `inspectSource`; the
// password it returns stays with that source in memory (never stored) and goes with its jobs.

import { useRef, useState } from "react";

import {
  api,
  ARCHIVE_PASSWORD_REQUIRED,
  ARCHIVE_PASSWORD_WRONG,
  errorText,
  type Inspection,
} from "./api";
import { Modal, openOverlay } from "./common";
import { useMessages } from "./i18n";
import { Icon } from "./icons";

export interface Opened {
  ins: Inspection;
  /** The password that opened it; null when none was needed. */
  password: string | null;
}

const isPasswordError = (e: unknown) => {
  const text = errorText(e);
  return text.includes(ARCHIVE_PASSWORD_REQUIRED) || text.includes(ARCHIVE_PASSWORD_WRONG);
};

/**
 * Inspects `path` (with `password`, one accepted before, on a re-read). An archive that needs a
 * password asks for it and is inspected again with each try until one opens it, unless
 * `current()` says the screen has moved on to another source by then. Rejects with inspect's
 * error; Cancel rejects with core's "needs a password" (`isPasswordRequired`).
 */
export async function inspectSource(
  path: string,
  password: string | null = null,
  current: () => boolean = () => true,
): Promise<Opened> {
  try {
    return { ins: await api.inspect(path, password ?? undefined), password };
  } catch (e) {
    if (!isPasswordError(e) || !current()) throw e;
    return askPassword(path);
  }
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
          <button type="submit" className="primary" disabled={!value || checking}>
            {checking ? t.checking : t.unlock}
          </button>
        </div>
      </form>
    </Modal>
  );
}
