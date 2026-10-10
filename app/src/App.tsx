// Shell: header with tabs, the job list (fed by events), and the close-while-running prompt.
// Nothing is saved: what the screens remember lasts until the app closes. The http build adds
// "Stop PS5 Dump Forge" to the header (there's no window to close) and rebuilds the job list
// from the server's.

import { useEffect, useReducer, useState } from "react";

import { api, errorText, events, prettyDuration, useOffline, web } from "./api";
import logo from "./assets/logo.png";
import { basename, dirname, panelId, tabId, TabList } from "./common";
import { Convert } from "./Convert";
import { Formats } from "./Formats";
import { Icon } from "./icons";
import { Inspect } from "./Inspect";
import { Lz4 } from "./Lz4";
import { jobsReducer, type Job } from "./jobs";

type Tab = "convert" | "lz4" | "inspect" | "formats";

export function App() {
  const [tab, setTab] = useState<Tab>("convert");
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const [closing, setClosing] = useState<"ask" | "stopping" | "stopped" | "failed" | null>(null);
  const offline = useOffline();

  useEffect(() => {
    const subs = [
      events.progress((e) => dispatch({ type: "progress", e, at: Date.now() })),
      events.log((e) => dispatch({ type: "log", e })),
      events.done((e) => dispatch({ type: "done", e })),
      events.closeRequested(() => setClosing((c) => c ?? "ask")),
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
            <h1 className="brand">
              <img src={logo} alt="" width={30} height={30} />
              <span className="visually-hidden">PS5 Dump Forge</span>
            </h1>
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
                { id: "lz4", label: "LZ4", className: "tab-lz4" },
                { id: "inspect", label: "Inspect" },
                { id: "formats", label: "About formats" },
              ]}
            />
            {web && (
              <div className="top-actions">
                {offline && (
                  <span className="tag orange" role="status">
                    Offline, retrying…
                  </span>
                )}
                <button className="small" onClick={askStop}>
                  <Icon name="power" />
                  Stop PS5 Dump Forge
                </button>
              </div>
            )}
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

/** How far an unfinished job got, for the quit prompt. */
function jobState(j: Job): string {
  if (j.stage === undefined) return "queued";
  const left = j.etaMs !== undefined ? `, ${prettyDuration(j.etaMs)} left` : "";
  return `${Math.floor(100 * j.shown)}% done${left}`;
}
