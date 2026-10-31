// Shell: header with tabs, the job list (fed by events), and the close-while-running prompt.
// Nothing is saved: what the screens remember lasts until the app closes. The http build adds
// "Stop Dump Forge" to the header (there's no window to close) and rebuilds the job list
// from the server's.

import { memo, useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { api, APP_NAME, debugBuild, errorText, events, prettyDuration, runtimePlatform, useOffline, web, type DoneEvent, type RuntimePlatform } from "./api";
import logo from "./assets/logo.png";
import { closeOverlays, dirname, isNetwork, Modal, notifyDeleted, panelId, tabId, TabList, usePeerPath, type SourceSelection } from "./common";
import { Connect, PairPrompt } from "./Connect";
import { Convert } from "./Convert";
import { Formats } from "./Formats";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";
import { Edit } from "./Edit";
import { Pack } from "./Pack";
import { Storage } from "./Storage";
import { stepTab, watchShoulders, type Step } from "./shoulders";
import { UpdateToast } from "./UpdateToast";
import { jobName, JobsCard, sourceLine } from "./JobList";
import { jobsReducer, jobType, remoteJobs, type Job, type JobType } from "./jobs";
import { useOnlineCount, useRemoteJobs } from "./useConnectState";

// Tabs: [Convert Edit] | Pack | Storage | Connect | About (the dividers are drawn only).
type Tab = "convert" | "storage" | "pack" | "edit" | "connect" | "formats";
/** The bar's order, which L2 / R2 step through. */
const TAB_ORDER: readonly Tab[] = ["convert", "edit", "pack", "storage", "connect", "formats"];
/** The job type each tab lists under it (About lists none). */
const TAB_JOBS: Record<JobType, Tab> = { convert: "convert", edit: "edit", pack: "pack", transfer: "connect", install: "storage" };
/** The tab that lists and counts `j`: its type's, but a peer's install (a PS5 installing this
 * device's package) under Connect, with the transfers. Null until its request arrives (its
 * events can beat `startJob`'s reply). */
const tabOf = (j: Job): Tab | null => {
  if (!j.request) return null;
  const type = jobType(j.request);
  return type === "install" && j.remote ? "connect" : TAB_JOBS[type];
};

// The screens re-render only when their props change: a progress or log event, or a Connect
// poll, reaches the job lists and the header, not the explorers and grids kept mounted.
const ConvertScreen = memo(Convert);
const EditScreen = memo(Edit);
const PackScreen = memo(Pack);
const StorageScreen = memo(Storage);
const ConnectScreen = memo(Connect);

/** `jobs` as the screens read it (id, request, result): the same array until one of those
 * changes, so progress and log events don't re-render them. */
function useSettled(jobs: Job[]): Job[] {
  const ref = useRef(jobs);
  const prev = ref.current;
  if (
    prev !== jobs &&
    (prev.length !== jobs.length ||
      jobs.some((j, i) => j.id !== prev[i].id || j.request !== prev[i].request || j.result !== prev[i].result))
  )
    ref.current = jobs;
  return ref.current;
}

export function App() {
  const t = useMessages().app;
  // A phone that scanned the Connect card's QR lands on `#connect`.
  const [tab, setTab] = useState<Tab>(() => (location.hash === "#connect" ? "connect" : "convert"));
  const online = useOnlineCount();
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const settled = useSettled(jobs);
  // Paired peers' jobs that use this device's files (Connect's poll): listed and counted in their
  // tab like this device's, never in the quit prompt (they don't stop with this device).
  const remoteList = useRemoteJobs();
  const remoteRef = useRef<Job[]>([]);
  const remote = useMemo(() => (remoteRef.current = remoteJobs(remoteList, remoteRef.current, Date.now())), [remoteList]);
  const [closing, setClosing] = useState<"ask" | "stopping" | "stopped" | "failed" | null>(null);
  // The app's last close request: a paired device was reading or writing files here.
  const [peerBusy, setPeerBusy] = useState(false);
  // Storage handoffs (Library, File explorer): a new id per action, so the same path is read again.
  const [editSel, setEditSel] = useState<SourceSelection | null>(null);
  const [convertSel, setConvertSel] = useState<SourceSelection | null>(null);
  const selId = useRef(0);
  const goTo = useCallback((to: Tab) => {
    setTab(to);
    document.getElementById(tabId("screen", to))?.focus();
  }, []);
  const handOff = useCallback(
    (to: "edit" | "convert", e: { path: string; kind: SourceSelection["kind"] }) => {
      const sel = { id: ++selId.current, path: e.path, kind: e.kind };
      (to === "edit" ? setEditSel : setConvertSel)(sel);
      goTo(to);
    },
    [goTo],
  );
  // Stable handlers, so the memoized screens keep their props.
  const editPkg = useCallback((path: string) => handOff("edit", { path, kind: "pkg" }), [handOff]);
  const editEntry = useCallback((e: { path: string; kind: SourceSelection["kind"] }) => handOff("edit", e), [handOff]);
  const convertEntry = useCallback((e: { path: string; kind: SourceSelection["kind"] }) => handOff("convert", e), [handOff]);
  const openConnect = useCallback(() => goTo("connect"), [goTo]);
  const openFormats = useCallback(() => goTo("formats"), [goTo]);
  // http build: the payload on a PS5 (its tile starts it again), or `serve` on a computer;
  // null until the server answers (asked again each time the server is back online).
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  const offline = useOffline();
  useEffect(() => {
    if (web && platform === null && !offline) runtimePlatform().then(setPlatform, () => {});
  }, [platform, offline]);
  const ps5 = platform !== "host";
  // L2 / R2 (badges in the nav, the pad's triggers, which arrive as F7 / F8): only once the
  // server says PS5.
  const shoulders = platform === "ps5";

  useEffect(() => {
    // A job that deleted its source: Library, File explorer and Edit drop it. Heard from its
    // done event, or (http poller) from a restored job that had already finished; once per job.
    const notified = new Set<number>();
    const deleted = (d: DoneEvent) => {
      const gone = "Ok" in d.result ? sourceLine(d.result.Ok.checks) : null;
      if (!gone?.deleted || notified.has(d.job)) return;
      notified.add(d.job);
      notifyDeleted(gone.text);
    };
    const subs = [
      events.progress((e) => dispatch({ type: "progress", e, at: Date.now() })),
      events.stages((e) => dispatch({ type: "stages", e })),
      events.log((e) => dispatch({ type: "log", e })),
      events.done((e) => {
        dispatch({ type: "done", e });
        deleted(e);
      }),
      // The quit prompt never opens behind a dialog (help, delete): those close as cancelled.
      events.closeRequested((peer) => {
        if (!wasClosing.current && document.activeElement instanceof HTMLElement) quitOpener.current = document.activeElement;
        setPeerBusy(peer);
        closeOverlays();
        setClosing((c) => c ?? "ask");
      }),
      events.restore((e) => {
        dispatch({ type: "restore", e, at: Date.now() });
        for (const j of e.jobs) if (j.done) deleted(j.done);
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);

  const unfinished = jobs.filter((j) => !j.result);
  const active = unfinished.length;
  // A tab's label with its counter: that tab's unfinished jobs (one white counter on every tab).
  const counted = (label: string, at: Tab) => {
    const n = [...unfinished, ...remote.filter((j) => !j.result)].filter((j) => tabOf(j) === at).length;
    return (
      <>
        {label}
        {n > 0 && (
          <>
            <span className="count" aria-hidden="true">
              {n}
            </span>
            <span className="visually-hidden">{t.active(n)}</span>
          </>
        )}
      </>
    );
  };

  // Under Convert, Edit, Pack, Storage and Connect, inside the tab's panel: that tab's own jobs (`tabOf`),
  // as its count counts them; each list kept mounted (open logs stay open). A job whose request
  // hasn't arrived yet has no type: listed nowhere yet.
  const jobsOf = (at: Tab) => {
    // Newest first: this device's above the peers'.
    const mine = [...remote, ...jobs].filter((j) => tabOf(j) === at);
    return (
      mine.length > 0 && (
        <div className="screen jobs-screen">
          <JobsCard jobs={mine} dispatch={dispatch} id={`jobs-${at}`} />
        </div>
      )
    );
  };

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
  // The notes name the tile or the serve command: know which before stopping.
  const askStop = async () => {
    if (platform === null) await runtimePlatform().then(setPlatform, () => {});
    if (active > 0) setClosing("ask");
    else void quit();
  };
  // Focus goes back to Stop when its prompt closes (the desktop's, opened by the window's close
  // button: to what had focus then).
  const stopButton = useRef<HTMLButtonElement>(null);
  const quitOpener = useRef<HTMLElement | null>(null);
  const wasClosing = useRef(false);
  useEffect(() => {
    if (closing === null && wasClosing.current) {
      const to = stopButton.current ?? quitOpener.current;
      if (to?.isConnected) to.focus();
      quitOpener.current = null;
    }
    wasClosing.current = closing !== null;
  }, [closing]);
  // Escape, a click outside and Keep running/Close: only while a question is up. Once Stop is
  // confirmed (Stopping, has stopped) the dialog takes no keys: the browser keeps its own.
  const dismiss = () => setClosing(null);
  const shown = usePeerPath();
  const dismissable = closing === "ask" || closing === "failed";

  // L2 / R2: the previous or next tab in the bar's order, wrapping; focus follows as a click's
  // does. Not while a dialog (the quit prompt included) is up.
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const step = (s: Step) => {
    if (document.querySelector('[aria-modal="true"]')) return;
    const next = stepTab(TAB_ORDER, tabRef.current, s);
    setTab(next);
    document.getElementById(tabId("screen", next))?.focus();
  };
  const stepRef = useRef(step);
  stepRef.current = step;
  useEffect(() => (shoulders ? watchShoulders((s) => stepRef.current(s)) : undefined), [shoulders]);

  // Folders on this device this session wrote outputs to: interrupted jobs leave `.part` files there.
  const [outputDirs, setOutputDirs] = useState<string[]>([]);
  useEffect(() => {
    setOutputDirs((prev) => {
      const next = new Set(prev);
      for (const j of settled) if (j.request && !isNetwork(j.request.output)) next.add(dirname(j.request.output));
      return next.size === prev.length ? prev : [...next];
    });
  }, [settled]);

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
              {/* Connect's state (online paired peers and saved NAS devices); opens the Connect tab. */}
              <button type="button" className="conn-btn" onClick={() => goTo("connect")}>
                <span className="visually-hidden">{t.connect} </span>
                <span className={online > 0 ? "tag green" : "tag"}>{online > 0 ? t.connected(online) : t.disconnected}</span>
              </button>
            </div>
            <TabList
              label={t.screens}
              prefix="screen"
              className={shoulders ? "nav shoulders" : "nav"}
              value={tab}
              onChange={setTab}
              start={shoulders ? <Shoulder name="L2" label={t.prevTab} onClick={() => step(-1)} /> : undefined}
              end={shoulders ? <Shoulder name="R2" label={t.nextTab} onClick={() => step(1)} /> : undefined}
              tabs={[
                { id: "convert", label: counted(t.convert, "convert") },
                { id: "edit", label: counted(t.edit, "edit") },
                { id: "pack", label: counted(t.pack, "pack"), sep: true },
                { id: "storage", label: counted(t.storage, "storage"), sep: true },
                { id: "connect", label: counted(t.connect, "connect"), sep: true },
                { id: "formats", label: t.about, sep: true },
              ]}
            />
            <div className="top-actions">
              {/* The status region stays mounted, so its text is announced when it changes. */}
              {web && (
                <span className="visually-hidden" role="status">
                  {offline ? t.offline : ""}
                </span>
              )}
              {web && offline && (
                <span className="tag orange" aria-hidden="true">
                  {t.offline}
                </span>
              )}
              {/* Debug builds only (never released): English, untranslated on purpose. */}
              {web && debugBuild() && (
                <button className="small" onClick={() => location.assign("/probe")}>
                  Input probe
                </button>
              )}
              {web && (
                <button className="small" ref={stopButton} onClick={askStop} title={t.stop}>
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
            {/* onCompare: the link goes away with its screen, so focus moves to the tab now shown. */}
            <ConvertScreen selection={convertSel} onEdit={editPkg} jobs={settled} dispatch={dispatch} onCompare={openFormats} />
            {jobsOf("convert")}
          </div>
          <div
            id={panelId("screen", "storage")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "storage")}
            hidden={tab !== "storage"}
          >
            <StorageScreen
              visible={tab === "storage"}
              onEdit={editEntry}
              onConvert={convertEntry}
              dispatch={dispatch}
              onConnect={openConnect}
            />
            {jobsOf("storage")}
          </div>
          <div
            id={panelId("screen", "pack")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "pack")}
            hidden={tab !== "pack"}
          >
            <PackScreen jobs={settled} dispatch={dispatch} />
            {jobsOf("pack")}
          </div>
          <div
            id={panelId("screen", "edit")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "edit")}
            hidden={tab !== "edit"}
          >
            <EditScreen outputDirs={outputDirs} visible={tab === "edit"} selection={editSel} jobs={settled} dispatch={dispatch} />
            {jobsOf("edit")}
          </div>
          <div
            id={panelId("screen", "connect")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "connect")}
            hidden={tab !== "connect"}
          >
            <ConnectScreen dispatch={dispatch} />
            {jobsOf("connect")}
          </div>
          <div
            id={panelId("screen", "formats")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "formats")}
            hidden={tab !== "formats"}
          >
            <Formats shown={tab === "formats"} />
          </div>
        </main>
        {/* A newer release (checked once per start); inert with the screens under the quit prompt. */}
        <UpdateToast />
      </div>
      {/* Another device asks to pair: Allow or Deny, over whatever screen or dialog is open (a
          portal outside the root `openOverlay` makes inert); not under the quit prompt. */}
      {closing === null && <PairPrompt />}

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
                {peerBusy && !web ? t.peerBusy : t.stopNote(web, ps5)}
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
                        {jobName(j, shown, t.job)}
                      </span>
                      <span className="muted">{jobState(j, t)}</span>
                    </li>
                  ))}
                </ul>
                <p className="muted">{t.quitNote(web, unfinished.length)}</p>
                {peerBusy && !web && <p className="muted">{t.peerBusy}</p>}
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
                  {active > 0 ? t.stoppingJobs : web ? t.stoppingForge : t.quitting}
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

/** A trigger hint (L2 / R2) that also works as a button. Out of the Tab order: the arrow keys
 * already move through the tabs. */
function Shoulder(props: { name: "L2" | "R2"; label: string; onClick: () => void }) {
  return (
    <button type="button" className="shoulder" tabIndex={-1} aria-label={props.label} title={props.label} onClick={props.onClick}>
      <span aria-hidden="true">{props.name}</span>
    </button>
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
