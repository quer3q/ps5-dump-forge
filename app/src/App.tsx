// Shell: header with tabs, the job list (fed by events), and the close-while-running prompt.
// Nothing is saved: what the screens remember lasts until the app closes. The http build adds
// "Stop Dump Forge" to the header (there's no window to close) and rebuilds the job list
// from the server's.

import { useEffect, useReducer, useRef, useState } from "react";

import { api, APP_NAME, errorText, events, prettyDuration, runtimePlatform, serverAddress, useOffline, web } from "./api";
import logo from "./assets/logo.png";
import { basename, closeOverlays, dirname, Modal, panelId, tabId, TabList, type SourceSelection } from "./common";
import { Convert } from "./Convert";
import { Formats, openConnect } from "./Formats";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";
import { Edit } from "./Edit";
import { Library } from "./Library";
import { Lz4 } from "./Lz4";
import { UpdateToast } from "./UpdateToast";
import { jobsReducer, type Job } from "./jobs";

// Tabs: [Convert Edit] | LZ4 | Library | About (the dividers are drawn only).
type Tab = "convert" | "library" | "lz4" | "edit" | "formats";

export function App() {
  const t = useMessages().app;
  const [tab, setTab] = useState<Tab>("convert");
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const [closing, setClosing] = useState<"ask" | "stopping" | "stopped" | "failed" | null>(null);
  const offline = useOffline();
  // Library handoffs: a new id per action, so the same path is read again.
  const [editSel, setEditSel] = useState<SourceSelection | null>(null);
  const [convertSel, setConvertSel] = useState<SourceSelection | null>(null);
  const selId = useRef(0);
  const handOff = (to: "edit" | "convert", e: { path: string; kind: SourceSelection["kind"] }) => {
    const sel = { id: ++selId.current, path: e.path, kind: e.kind };
    (to === "edit" ? setEditSel : setConvertSel)(sel);
    setTab(to);
    document.getElementById(tabId("screen", to))?.focus();
  };
  // http build: the payload on a PS5 (its tile starts it again), or `serve` on a computer.
  const [ps5, setPs5] = useState(true);
  useEffect(() => {
    if (web) runtimePlatform().then((p) => setPs5(p === "ps5"), () => {});
  }, []);
  // The server's host:port (http build), read once before the app rendered.
  const address = serverAddress();

  useEffect(() => {
    const subs = [
      events.progress((e) => dispatch({ type: "progress", e, at: Date.now() })),
      events.stages((e) => dispatch({ type: "stages", e })),
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
  // Focus goes back to Stop when its prompt closes.
  const stopButton = useRef<HTMLButtonElement>(null);
  const wasClosing = useRef(false);
  useEffect(() => {
    if (closing === null && wasClosing.current) stopButton.current?.focus();
    wasClosing.current = closing !== null;
  }, [closing]);
  // Escape, a click outside and Keep running/Close: only while a question is up. Once Stop is
  // confirmed (Stopping, has stopped) the dialog takes no keys: the browser keeps its own.
  const dismiss = () => setClosing(null);
  const dismissable = closing === "ask" || closing === "failed";

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
                <span className="visually-hidden">{APP_NAME}</span>
              </h1>
              {/* The address is a button: How to connect, with the QR code, in a dialog. */}
              {address && (
                <button
                  type="button"
                  className="address-btn"
                  aria-label={t.address(`http://${address}`)}
                  onClick={openConnect}
                >
                  <span className="address">{address}</span>
                </button>
              )}
            </div>
            <TabList
              label={t.screens}
              prefix="screen"
              className="nav"
              value={tab}
              onChange={setTab}
              tabs={[
                {
                  id: "convert",
                  label: (
                    <>
                      {t.convert}
                      {active > 0 && (
                        <>
                          <span className="count" aria-hidden="true">
                            {active}
                          </span>
                          <span className="visually-hidden">{t.active(active)}</span>
                        </>
                      )}
                    </>
                  ),
                },
                { id: "edit", label: t.edit },
                { id: "lz4", label: "LZ4", className: "tab-lz4", sep: true },
                {
                  id: "library",
                  label: (
                    <>
                      <Icon name="library" />
                      {t.library}
                    </>
                  ),
                  sep: true,
                },
                { id: "formats", label: t.about, sep: true },
              ]}
            />
            <div className="top-actions">
              {web && offline && (
                <span className="tag orange" role="status">
                  {t.offline}
                </span>
              )}
              {web && (
                <button className="small" ref={stopButton} onClick={askStop}>
                  <Icon name="power" />
                  <span className="stop-label">{t.stop}</span>
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
              selection={convertSel}
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
            id={panelId("screen", "library")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "library")}
            hidden={tab !== "library"}
          >
            <Library
              visible={tab === "library"}
              onEdit={(e) => handOff("edit", e)}
              onConvert={(e) => handOff("convert", e)}
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
            id={panelId("screen", "edit")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "edit")}
            hidden={tab !== "edit"}
          >
            <Edit outputDirs={outputDirs} visible={tab === "edit"} selection={editSel} jobs={jobs} dispatch={dispatch} />
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
        {/* A newer release (checked once per start); inert with the screens under the quit prompt. */}
        <UpdateToast />
      </div>

      {closing && (
        <Modal
          close={dismissable ? dismiss : undefined}
          className="modal card"
          role="alertdialog"
          labelledBy="quit-title"
          describedBy="quit-text"
        >
          {closing === "ask" && unfinished.length === 0 ? (
            <>
              <div className="modal-head">
                <span className="head-icon warn">
                  <Icon name="power" />
                </span>
                <h2 id="quit-title">{t.confirmStop(web)}</h2>
              </div>
              <p className="muted" id="quit-text">
                {t.stopNote(web, ps5)}
              </p>
              <div className="row end">
                <button autoFocus onClick={dismiss}>
                  {t.keepRunning}
                </button>
                <button className="danger" onClick={quit}>
                  {web ? t.stop : t.quit}
                </button>
              </div>
            </>
          ) : closing === "ask" ? (
            <>
              <div className="modal-head">
                <span className="head-icon warn">
                  <Icon name="alert" />
                </span>
                <h2 id="quit-title">{t.stillRunning(unfinished.length)}</h2>
              </div>
              <div id="quit-text">
                <ul className="quit-jobs">
                  {unfinished.map((j) => (
                    <li key={j.id}>
                      <span className="quit-name">
                        {j.request ? basename(j.request.output) : t.job(j.id)}
                      </span>
                      <span className="muted">{jobState(j, t)}</span>
                    </li>
                  ))}
                </ul>
                <p className="muted">{t.quitNote(web, unfinished.length)}</p>
              </div>
              <div className="row end">
                <button autoFocus onClick={dismiss}>
                  {t.keepRunning}
                </button>
                <button className="danger" onClick={quit}>
                  {web ? t.stop : t.stopAndQuit(unfinished.length)}
                </button>
              </div>
            </>
          ) : closing === "stopping" ? (
            <div className="modal-head">
              <span className="spinner" aria-hidden="true" />
              {/* Its button is gone: focus moves here, inside the dialog (old WebKit has no inert). */}
              <h2 id="quit-title" tabIndex={-1} ref={(el) => el?.focus()}>
                <span id="quit-text">
                  {active > 0 || !web ? t.stoppingJobs : t.stoppingForge}
                </span>
              </h2>
            </div>
          ) : closing === "failed" ? (
            <>
              <div className="modal-head">
                <span className="head-icon warn">
                  <Icon name="alert" />
                </span>
                <h2 id="quit-title">{t.couldntStop}</h2>
              </div>
              <p className="muted" id="quit-text">
                {quitError}
              </p>
              <div className="row end">
                <button autoFocus onClick={dismiss}>
                  {t.close}
                </button>
                <button className="danger" onClick={quit}>
                  {t.tryAgain}
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
                  {t.stopped}
                </h2>
              </div>
              <p className="muted" id="quit-text">
                {t.startAgain(ps5)}
              </p>
            </>
          )}
        </Modal>
      )}
    </div>
  );
}

const REPO = "https://github.com/quer3q/ps5-dump-forge";

/** The project on GitHub: a link in the http build, the default browser in the app (a link
 * there would open inside its window). */
function GitHubLink() {
  const t = useMessages().app;
  const mark = (
    <svg className="icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M8 0c4.42 0 8 3.58 8 8a8.01 8.01 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A8 8 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  );
  const label = t.github(APP_NAME);
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
function jobState(j: Job, t: Messages["app"]): string {
  if (j.stage === undefined) return t.queued;
  return t.progress(Math.floor(100 * j.shown), j.etaMs !== undefined ? prettyDuration(j.etaMs) : null);
}
