// The update toast: checked once per start (update.ts); Later hides it until the app or page closes.

import { useEffect, useState } from "react";

import { api, errorText, runtimePlatform, web } from "./api";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { checkUpdate, UPDATE_TIMEOUT } from "./update";

// One check per start, StrictMode's second mount included; any failure is no toast.
let check: Promise<string | null> | null = null;
// The PS5's page: Update reloads it into the new version; elsewhere the download opens.
let reloads = false;
function newer(): Promise<string | null> {
  check ??= api
    .updateTarget()
    .then(async (t) => {
      if (!t) return null;
      reloads = web && (await runtimePlatform()) === "ps5";
      return checkUpdate(t);
    })
    .catch(() => null);
  return check;
}

type State = { step: "offer" | "updating" | "opened" } | { step: "failed"; reason: string };

export function UpdateToast() {
  const t = useMessages().update;
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<State>({ step: "offer" });
  useEffect(() => {
    let live = true;
    void newer().then((v) => live && setVersion(v));
    return () => {
      live = false;
    };
  }, []);
  // An always-mounted status region holds the title, so screen readers announce the offer and
  // each change of it (a region that mounts already filled is often not read).
  const announce = (text: string) => (
    <span className="visually-hidden" role="status">
      {text}
    </span>
  );
  // The same shape as below (the region, then the toast), so the region is kept, not remounted.
  if (!version) return <>{announce("")}{null}</>;

  const close = () => setVersion(null);
  const update = async () => {
    setState({ step: "updating" });
    try {
      await api.startUpdate(version);
      // The PS5's page reloads by itself; until then it stays "Updating".
      if (!reloads) setState({ step: "opened" });
    } catch (e) {
      setState({ step: "failed", reason: e === UPDATE_TIMEOUT ? t.timeout(version) : errorText(e) });
    }
  };
  // Opening the download answers at once: its buttons just wait, no spinner.
  const spinning = state.step === "updating" && reloads;
  const busy = state.step === "updating";
  const title = spinning ? t.updating : state.step === "failed" ? t.failed(version) : t.available(version);

  return (
    <>
      {announce(title)}
      <div className="update-toast card">
        <div className="update-head">
          {spinning ? (
            <span className="spinner" aria-hidden="true" />
          ) : (
            <span className={state.step === "failed" ? "head-icon warn" : "head-icon"}>
              <Icon name={state.step === "failed" ? "alert" : "download"} />
            </span>
          )}
          <span className="update-title">{title}</span>
        </div>
        {spinning && <p className="muted">{t.updatingNote}</p>}
        {state.step === "opened" && <p className="muted">{web ? t.downloadingCli : t.downloading}</p>}
        {state.step === "failed" && <p className="muted backend">{state.reason}</p>}
        {!spinning && (
          <div className="row end">
            {state.step === "offer" || state.step === "updating" ? (
              <>
                <button className="small" disabled={busy} onClick={close}>
                  {t.later}
                </button>
                <button className="small primary" disabled={busy} onClick={() => void update()}>
                  {t.update}
                </button>
              </>
            ) : state.step === "opened" ? (
              <button className="small" onClick={close}>
                {t.close}
              </button>
            ) : (
              <>
                <button className="small" onClick={close}>
                  {t.close}
                </button>
                <button className="small primary" onClick={() => void update()}>
                  {t.retry}
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </>
  );
}
