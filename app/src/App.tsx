// Shell: header with tabs, the job list (fed by events), and the close-while-running prompt.
// Nothing is saved: what the screens remember lasts until the app closes. The http build adds
// "Stop PS5 Dump Forge" to the header (there's no window to close) and rebuilds the job list
// from the server's.

import { useEffect, useReducer, useState } from "react";

import { api, errorText, events, prettyDuration, serverAddress, useOffline, web } from "./api";
import logo from "./assets/logo.png";
import { basename, closeOverlays, dirname, panelId, tabId, TabList } from "./common";
import { Convert } from "./Convert";
import { Formats, openConnect } from "./Formats";
import { Icon } from "./icons";
import { Inspect } from "./Inspect";
import { Lz4 } from "./Lz4";
import { jobsReducer, type Job } from "./jobs";

// Tabs: [Convert Inspect] | LZ4 | About (ids unchanged; the dividers are drawn only).
type Tab = "convert" | "lz4" | "inspect" | "formats";

export function App() {
  const [tab, setTab] = useState<Tab>("convert");
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const [closing, setClosing] = useState<"ask" | "stopping" | "stopped" | "failed" | null>(null);
  const offline = useOffline();
  // The server's host:port (http build), read once before the app rendered.
  const address = serverAddress();

  useEffect(() => {
    const subs = [
      events.progress((e) => dispatch({ type: "progress", e, at: Date.now() })),
      events.log((e) => dispatch({ type: "log", e })),
      events.done((e) => dispatch({ type: "done", e })),
      // The quit prompt never opens behind a dialog (help, delete): those close as cancelled.
      events.closeRequested(() => {
        closeOverlays();
        setClosing((c) => c ?? "ask");
      }),
      events.restore((e) => dispatch({ type: "restore", e, at: Date.now() })),
    ];
    return () => {
      subs.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);

  const unfinished = jobs.filter((j) => !j.result);
  const active = unfinished.length;

  const [quitError, setQuitError] = useState<string | null>(null);
  const quit = async () => {
    setClosing("stopping");
    setQuitError(null);
    try {
      await api.quitApp();
      // The app exits on its own; the http build's page stays, saying so.
      if (web) setClosing("stopped");
    } catch (e) {
      if (!web) return setClosing("ask");
      setQuitError(errorText(e));
      setClosing("failed");
    }
  };
  // http build: stopping the payload asks first only while jobs run.
  const askStop = () => (active > 0 ? setClosing("ask") : void quit());

  // Folders this session wrote outputs to: interrupted jobs leave `.part` files there.
  const [outputDirs, setOutputDirs] = useState<string[]>([]);
  useEffect(() => {
    setOutputDirs((prev) => {
      const next = new Set(prev);
      for (const j of jobs) if (j.request) next.add(dirname(j.request.output));
      return next.size === prev.length ? prev : [...next];
    });
  }, [jobs]);

  return (
    <div className="app">
      {/* inert: no clicks or keys reach the screens while the quit prompt is up. */}
      <div inert={closing !== null}>
        <header className="topbar">
          <div className="topbar-in">
            {/* The window title already names the app; the icon stands for it here. */}
            <div className="brand-row">
              <h1 className="brand">
                <img src={logo} alt="" width={30} height={30} />
                <span className="visually-hidden">PS5 Dump Forge</span>
              </h1>
              {/* The address is a button: How to connect, with the QR code, in a dialog. */}
              {address && (
                <button
                  type="button"
                  className="address-btn"
                  aria-label={`Address: http://${address}. Show how to connect, with a QR code`}
                  onClick={openConnect}
                >
                  <span className="address">{address}</span>
                </button>
              )}
            </div>
            <TabList
              label="Screens"
              prefix="screen"
              className="nav"
              value={tab}
              onChange={setTab}
              tabs={[
                {
                  id: "convert",
                  label: (
                    <>
                      Convert
                      {active > 0 && (
                        <>
                          <span className="count" aria-hidden="true">
                            {active}
                          </span>
                          <span className="visually-hidden">, {active} active</span>
                        </>
                      )}
                    </>
                  ),
                },
                { id: "inspect", label: "Inspect" },
                { id: "lz4", label: "LZ4", className: "tab-lz4", sep: true },
                { id: "formats", label: "About", sep: true },
              ]}
            />
            <div className="top-actions">
              {web && offline && (
                <span className="tag orange" role="status">
                  Offline, retrying…
                </span>
              )}
              {web && (
                <button className="small" onClick={askStop}>
                  <Icon name="power" />
                  Stop PS5 Dump Forge
                </button>
              )}
              <GitHubLink />
            </div>
          </div>
        </header>
        {/* Screens stay mounted so a half-filled form survives a tab switch. */}
        <main>
          <div
            id={panelId("screen", "convert")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "convert")}
            hidden={tab !== "convert"}
          >
            <Convert
              jobs={jobs}
              dispatch={dispatch}
              onCompare={() => {
                setTab("formats");
                // The link goes away with its screen: focus the tab that now shows.
                document.getElementById(tabId("screen", "formats"))?.focus();
              }}
            />
          </div>
          <div
            id={panelId("screen", "lz4")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "lz4")}
            hidden={tab !== "lz4"}
          >
            <Lz4 jobs={jobs} dispatch={dispatch} />
          </div>
          <div
            id={panelId("screen", "inspect")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "inspect")}
            hidden={tab !== "inspect"}
          >
            <Inspect outputDirs={outputDirs} visible={tab === "inspect"} />
          </div>
          <div
            id={panelId("screen", "formats")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "formats")}
            hidden={tab !== "formats"}
          >
            <Formats />
          </div>
        </main>
      </div>

      {closing && (
        <div className="modal-backdrop">
          <div
            className="modal card"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="quit-title"
            aria-describedby="quit-text"
          >
            {closing === "ask" ? (
              <>
                <div className="modal-head">
                  <span className="head-icon warn">
                    <Icon name="alert" />
                  </span>
                  <h2 id="quit-title">
                    {unfinished.length === 1 ? "A job is" : `${unfinished.length} jobs are`} still
                    running
                  </h2>
                </div>
                <div id="quit-text">
                  <ul className="quit-jobs">
                    {unfinished.map((j) => (
                      <li key={j.id}>
                        <span className="quit-name">
                          {j.request ? basename(j.request.output) : `Job ${j.id}`}
                        </span>
                        <span className="muted">{jobState(j)}</span>
                      </li>
                    ))}
                  </ul>
                  <p className="muted">
                    {web ? "Stopping PS5 Dump Forge" : "Quitting"} stops{" "}
                    {unfinished.length === 1 ? "it" : "them"} and deletes the unfinished output.
                  </p>
                </div>
                <div className="row end">
                  <button autoFocus onClick={() => setClosing(null)}>
                    Keep running
                  </button>
                  <button className="danger" onClick={quit}>
                    {web
                      ? "Stop PS5 Dump Forge"
                      : unfinished.length === 1
                        ? "Stop job and quit"
                        : "Stop jobs and quit"}
                  </button>
                </div>
              </>
            ) : closing === "stopping" ? (
              <div className="modal-head">
                <span className="spinner" aria-hidden="true" />
                <h2 id="quit-title">
                  <span id="quit-text">
                    {active > 0 || !web ? "Stopping jobs and cleaning up…" : "Stopping PS5 Dump Forge…"}
                  </span>
                </h2>
              </div>
            ) : closing === "failed" ? (
              <>
                <div className="modal-head">
                  <span className="head-icon warn">
                    <Icon name="alert" />
                  </span>
                  <h2 id="quit-title">Couldn't stop PS5 Dump Forge</h2>
                </div>
                <p className="muted" id="quit-text">
                  {quitError}
                </p>
                <div className="row end">
                  <button autoFocus onClick={() => setClosing(null)}>
                    Close
                  </button>
                  <button className="danger" onClick={quit}>
                    Try again
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="modal-head">
                  <span className="head-icon">
                    <Icon name="power" />
                  </span>
                  <h2 id="quit-title" tabIndex={-1} ref={(el) => el?.focus()}>
                    PS5 Dump Forge has stopped
                  </h2>
                </div>
                <p className="muted" id="quit-text">
                  Start the payload on the PS5 again to use it, then reload this page.
                </p>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

const REPO = "https://github.com/quer3q/ps5-dump-forge";

/** The project on GitHub: a link in the http build, the default browser in the app (a link
 * there would open inside its window). */
function GitHubLink() {
  const mark = (
    <svg className="icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M8 0c4.42 0 8 3.58 8 8a8.01 8.01 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A8 8 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  );
  const label = "PS5 Dump Forge on GitHub";
  return web ? (
    <a className="gh-link" href={REPO} target="_blank" rel="noreferrer" aria-label={label} title={label}>
      {mark}
    </a>
  ) : (
    <button type="button" className="gh-link" aria-label={label} title={label} onClick={() => void api.openRepo()}>
      {mark}
    </button>
  );
}

/** How far an unfinished job got, for the quit prompt. */
function jobState(j: Job): string {
  if (j.stage === undefined) return "queued";
  const left = j.etaMs !== undefined ? `, ${prettyDuration(j.etaMs)} left` : "";
  return `${Math.floor(100 * j.shown)}% done${left}`;
}
