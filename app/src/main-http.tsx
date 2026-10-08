// The http build's entry (vite.config.ts puts it in place of main.tsx): the launcher check
// first (launcher.ts, outside React so StrictMode never runs it twice), then the app and its
// poller, loaded only once the server answers.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { checkForUpdate, setCanReload, watchUpdates } from "./appcache";
import { Icon } from "./icons";
import { browserDeps, launching, launchOnce, type Outcome } from "./launcher";
import { setServerSeparator } from "./paths";
import "./styles.css";

watchUpdates();

const root = createRoot(document.getElementById("root")!);
let appShown = false;

// An update reloads the page: never mid-launch, and in the app only while no dialog (quit,
// "has stopped", the picker) is open and no field has focus.
setCanReload(() => {
  if (!appShown) return !launching();
  const el = document.activeElement;
  return !document.querySelector(".modal-backdrop") && !(el && /^(INPUT|SELECT|TEXTAREA)$/.test(el.tagName));
});

type Failed = Extract<Outcome, { ok: false }> | { ok: false; failure: "load"; message: string };

function Launcher({ failed }: { failed: Failed | null }) {
  return (
    <div className="modal-backdrop">
      {failed ? (
        <div className="modal card" role="alert" aria-labelledby="launch-title" aria-describedby="launch-text">
          <div className="modal-head">
            <span className="head-icon warn">
              <Icon name="alert" />
            </span>
            <h2 id="launch-title">
              {failed.failure === "timeout" ? "PS5 Dump Forge didn't answer" : "Can't start PS5 Dump Forge"}
            </h2>
          </div>
          <p className="muted" id="launch-text">
            {failed.message}
          </p>
          <div className="row end">
            <button className="primary" autoFocus onClick={() => void run()}>
              Retry
            </button>
          </div>
        </div>
      ) : (
        <div className="modal card" role="status">
          <div className="modal-head">
            <span className="spinner" aria-hidden="true" />
            <h2>Starting PS5 Dump Forge…</h2>
          </div>
        </div>
      )}
    </div>
  );
}

const show = (failed: Failed | null) =>
  root.render(
    <StrictMode>
      <Launcher failed={failed} />
    </StrictMode>,
  );

async function run(): Promise<void> {
  if (appShown) return;
  const outcome = await launchOnce(location.hostname, location.port, browserDeps(), () => show(null));
  if (!outcome.ok) return show(outcome);
  let App, client;
  try {
    [{ App }, client] = await Promise.all([import("./App"), import("./http-client")]);
  } catch (e) {
    return show({ ok: false, failure: "load", message: `Couldn't load the page: ${e}` });
  }
  // The server's separator before anything renders a path. ponytail: a server that doesn't
  // answer now (a LAN viewer while it's down) keeps "/", the PS5's.
  const sep = await client.serverSeparator();
  if (appShown) return;
  if (sep) setServerSeparator(sep);
  appShown = true;
  client.start();
  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
  if (outcome.up) checkForUpdate();
}

void run();
