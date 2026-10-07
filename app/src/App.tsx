// Shell: header with tabs, the job list (fed by events), and the close-while-running prompt.
// Nothing is saved: what the screens remember lasts until the app closes.

import { useEffect, useReducer, useState } from "react";

import { api, events, prettyDuration } from "./api";
import logo from "./assets/logo.png";
import { basename, dirname, panelId, tabId, TabList } from "./common";
import { Convert } from "./Convert";
import { Formats } from "./Formats";
import { Icon } from "./icons";
import { Inspect } from "./Inspect";
import { jobsReducer, type Job } from "./jobs";

type Tab = "convert" | "inspect" | "formats";

export function App() {
  const [tab, setTab] = useState<Tab>("convert");
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const [closing, setClosing] = useState<"ask" | "stopping" | null>(null);

  useEffect(() => {
    const subs = [
      events.progress((e) => dispatch({ type: "progress", e, at: Date.now() })),
      events.log((e) => dispatch({ type: "log", e })),
      events.done((e) => dispatch({ type: "done", e })),
      events.closeRequested(() => setClosing((c) => c ?? "ask")),
    ];
    return () => {
      subs.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);

  const quit = async () => {
    setClosing("stopping");
    try {
      await api.quitApp();
    } catch {
      setClosing("ask");
    }
  };

  const unfinished = jobs.filter((j) => !j.result);
  const active = unfinished.length;

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
                { id: "inspect", label: "Inspect" },
                { id: "formats", label: "About formats" },
              ]}
            />
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
                    Quitting stops {unfinished.length === 1 ? "it" : "them"} and deletes the
                    unfinished output.
                  </p>
                </div>
                <div className="row end">
                  <button autoFocus onClick={() => setClosing(null)}>
                    Keep running
                  </button>
                  <button className="danger" onClick={quit}>
                    {unfinished.length === 1 ? "Stop job and quit" : "Stop jobs and quit"}
                  </button>
                </div>
              </>
            ) : (
              <div className="modal-head">
                <span className="spinner" aria-hidden="true" />
                <h2 id="quit-title">
                  <span id="quit-text">Stopping jobs and cleaning up…</span>
                </h2>
              </div>
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
