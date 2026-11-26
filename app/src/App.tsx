// Shell: the side rail, the screens (all kept mounted) and the close-while-running prompt.

import { memo, useCallback, useEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { api, APP_NAME, debugBuild, errorText, events, prettyDuration, runtimePlatform, useOffline, web, type DoneEvent, type RuntimePlatform } from "./api";
import logo from "./assets/logo.png";
import { closeOverlays, dirname, isNetwork, Modal, notifyDeleted, panelId, tabId, usePeerPath, type SourceSelection } from "./common";
import { Connect, PairPrompt } from "./Connect";
import { Build } from "./build/Build";
import { Formats } from "./Formats";
import { useMessages } from "./i18n";
import type { Messages } from "./locales/en";
import { Icon } from "./icons";
import { Edit } from "./Edit";
import { Settings } from "./Settings";
import { Queue } from "./Queue";
import { setPrefs, usePrefs } from "./prefs";
import type { IconName } from "./icons";
import { Storage } from "./Storage";
import { Transfer } from "./Transfer";
import { stepTab, watchShoulders, type Step } from "./shoulders";
import { UpdateToast } from "./UpdateToast";
import { jobName, sourceLine } from "./JobList";
import { jobsReducer, remoteJobs, type Job } from "./jobs";
import { useOnlineCount, useRemoteJobs } from "./useConnectState";

type Tab = "build" | "edit" | "queue" | "storage" | "transfer" | "connect" | "settings" | "formats";
/** The rail's order, which L2 / R2 and the arrow keys step through. */
const TAB_ORDER: readonly Tab[] = ["build", "queue", "transfer", "edit", "storage", "connect", "settings", "formats"];
const TAB_ICON: Record<Tab, IconName> = {
  build: "convert",
  edit: "pencil",
  queue: "jobs",
  storage: "drive",
  transfer: "transfer",
  connect: "link",
  settings: "settings",
  formats: "info",
};
// Memoized screens: a progress, log or Connect poll event reaches only the Queue and the rail.
const BuildScreen = memo(Build);
const EditScreen = memo(Edit);
const StorageScreen = memo(Storage);
const TransferScreen = memo(Transfer);
const ConnectScreen = memo(Connect);

/** The same array until a job's id, request or result changes. */
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
  const rail = useMessages().rail;
  const prefs = usePrefs();
  const [tab, setTab] = useState<Tab>(() => (location.hash === "#connect" ? "connect" : "build"));
  const online = useOnlineCount();
  const [jobs, dispatch] = useReducer(jobsReducer, []);
  const settled = useSettled(jobs);
  // Peers' jobs using this device's files: listed and counted, never in the quit prompt.
  const remoteList = useRemoteJobs();
  const remoteRef = useRef<Job[]>([]);
  const remote = useMemo(() => (remoteRef.current = remoteJobs(remoteList, remoteRef.current, Date.now())), [remoteList]);
  const [closing, setClosing] = useState<"ask" | "stopping" | "stopped" | "failed" | null>(null);
  const [peerBusy, setPeerBusy] = useState(false);
  // Storage handoffs: a new id per action, so the same path is read again.
  const [editSel, setEditSel] = useState<SourceSelection | null>(null);
  const [convertSel, setConvertSel] = useState<SourceSelection | null>(null);
  const selId = useRef(0);
  const goTo = useCallback((to: Tab) => {
    setTab(to);
    document.getElementById(tabId("screen", to))?.focus();
  }, []);
  const handOff = useCallback(
    (to: "edit" | "build", e: { path: string; kind: SourceSelection["kind"] }) => {
      const sel = { id: ++selId.current, path: e.path, kind: e.kind };
      (to === "edit" ? setEditSel : setConvertSel)(sel);
      goTo(to);
    },
    [goTo],
  );
  const editPkg = useCallback((path: string) => handOff("edit", { path, kind: "pkg" }), [handOff]);
  const editEntry = useCallback((e: { path: string; kind: SourceSelection["kind"] }) => handOff("edit", e), [handOff]);
  const convertEntry = useCallback((e: { path: string; kind: SourceSelection["kind"] }) => handOff("build", e), [handOff]);
  const openFormats = useCallback(() => goTo("formats"), [goTo]);
  const openQueue = useCallback(() => goTo("queue"), [goTo]);
  const openConnect = useCallback(() => goTo("connect"), [goTo]);
  // A new id each time, so the same root is chosen again.
  const [transferStart, setTransferStart] = useState<{ id: number; root: string } | null>(null);
  const transferId = useRef(0);
  const openTransferTo = useCallback(
    (root?: string) => {
      if (root !== undefined) setTransferStart({ id: ++transferId.current, root });
      goTo("transfer");
    },
    [goTo],
  );
  // http build: null until the server answers (asked again each time it is back online).
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  const offline = useOffline();
  useEffect(() => {
    if (web && platform === null && !offline) runtimePlatform().then(setPlatform, () => {});
  }, [platform, offline]);
  const ps5 = platform !== "host";
  const shoulders = platform === "ps5";

  useEffect(() => {
    // A job that deleted its source: heard from its done event or a restored finished job; once per job.
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
      // The quit prompt never opens behind a dialog: those close as cancelled.
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
  const pending = active + remote.filter((j) => !j.result).length;
  const queueLabel = (
    <>
      {rail.queue}
      {pending > 0 && (
        <>
          <span className="count" aria-hidden="true">
            {pending}
          </span>
          <span className="visually-hidden">{t.active(pending)}</span>
        </>
      )}
    </>
  );
  const allJobs = [...remote, ...jobs];

  const [quitError, setQuitError] = useState<string | null>(null);
  const quit = async () => {
    setClosing("stopping");
    setQuitError(null);
    try {
      await api.quitApp();
      if (web) setClosing("stopped");
    } catch (e) {
      if (!web) return setClosing("ask");
      setQuitError(errorText(e));
      setClosing("failed");
    }
  };
  const askStop = async () => {
    if (platform === null) await runtimePlatform().then(setPlatform, () => {});
    setClosing("ask");
  };
  // Focus goes back to Stop (or what had it) when its prompt closes.
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
  // Once Stop is confirmed the dialog takes no keys: the browser keeps its own.
  const dismiss = () => setClosing(null);
  const shown = usePeerPath();
  const dismissable = closing === "ask" || closing === "failed";

  // Not while a dialog (the quit prompt included) is up.
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
  // PS5: L2 / R2 step through the screens (no badges), and `html.ps5` sets the larger text.
  useEffect(() => {
    if (!shoulders) return;
    document.documentElement.classList.add("ps5");
    return watchShoulders((s) => stepRef.current(s));
  }, [shoulders]);

  // Folders this session wrote outputs to: interrupted jobs leave `.part` files there.
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
      <div className="shell" inert={closing !== null}>
        <nav className="rail" aria-label={APP_NAME}>
          <div className="rail-head">
            {/* The window title already names the app; the icon stands for it here. */}
            <h1 className="brand">
              <img src={logo} alt="" width={30} height={30} />
              <span className="visually-hidden">{APP_NAME}</span>
            </h1>
            <button
              type="button"
              className="rail-toggle"
              aria-expanded={!prefs.railCollapsed}
              aria-label={prefs.railCollapsed ? rail.expand : rail.collapse}
              title={prefs.railCollapsed ? rail.expand : rail.collapse}
              onClick={() => setPrefs({ railCollapsed: !prefs.railCollapsed })}
            >
              <Icon name="collapse" />
            </button>
          </div>
          <RailNav
            label={t.screens}
            value={tab}
            onChange={setTab}
            items={[
              { id: "build", label: rail.build, title: rail.build },
              { id: "queue", label: queueLabel, title: rail.queue },
              { id: "transfer", label: rail.transfer, title: rail.transfer },
              { id: "edit", label: t.edit, title: t.edit },
              { id: "storage", label: t.storage, title: t.storage },
              // The badge: the phone bar's Connect status (the foot's is hidden there; display: none above 700px).
              { id: "connect", label: t.connect, title: t.connect, badge: (
                <span className={online > 0 ? "conn-badge on" : "conn-badge"}>
                  <span className="visually-hidden">, {online > 0 ? t.connected(online) : t.disconnected}</span>
                </span>
              ) },
              { id: "settings", label: rail.settings, title: rail.settings },
              { id: "formats", label: t.about, title: t.about },
            ]}
          />
          <div className="rail-foot">
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
            {/* Connect's state (online paired peers and saved NAS devices); a click opens Connect. */}
            <button
              type="button"
              className="rail-item conn-status"
              title={online > 0 ? t.connected(online) : t.disconnected}
              onClick={openConnect}
            >
              <span className={online > 0 ? "conn-dot on" : "conn-dot"} aria-hidden="true" />
              <span className="rail-label">
                <span className="visually-hidden">{t.connect}: </span>
                <span className={online > 0 ? "tag green" : "tag"}>{online > 0 ? t.connected(online) : t.disconnected}</span>
              </span>
            </button>
            {/* Debug builds only (never released): English, untranslated on purpose. */}
            {web && debugBuild() && (
              <button className="rail-item" onClick={() => location.assign("/probe")}>
                <span className="rail-label">Input probe</span>
              </button>
            )}
            {web && (
              <button className="rail-item" ref={stopButton} onClick={askStop} title={t.stop}>
                <Icon name="power" />
                <span className="rail-label stop-label">{t.stop}</span>
              </button>
            )}
            <GitHubLink />
          </div>
        </nav>
        <div className="content">
        {/* Screens stay mounted so a half-filled form survives a tab switch. */}
        <main>
          <div
            id={panelId("screen", "build")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "build")}
            hidden={tab !== "build"}
          >
            {/* onCompare: the link goes away with its screen, so focus moves to the tab now shown. */}
            <BuildScreen
              selection={convertSel}
              onEdit={editPkg}
              jobs={settled}
              dispatch={dispatch}
              onCompare={openFormats}
              active={tab === "build"}
              onQueue={openQueue}
            />
          </div>
          <div
            id={panelId("screen", "queue")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "queue")}
            hidden={tab !== "queue"}
          >
            <Queue jobs={allJobs} dispatch={dispatch} />
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
          </div>
          <div
            id={panelId("screen", "edit")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "edit")}
            hidden={tab !== "edit"}
          >
            <EditScreen outputDirs={outputDirs} visible={tab === "edit"} selection={editSel} jobs={settled} dispatch={dispatch} />
          </div>
          <div
            id={panelId("screen", "transfer")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "transfer")}
            hidden={tab !== "transfer"}
          >
            <TransferScreen
              visible={tab === "transfer"}
              dispatch={dispatch}
              onQueue={openQueue}
              onConnect={openConnect}
              start={transferStart}
            />
          </div>
          <div
            id={panelId("screen", "connect")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "connect")}
            hidden={tab !== "connect"}
          >
            <ConnectScreen onTransfer={openTransferTo} />
          </div>
          <div
            id={panelId("screen", "settings")}
            role="tabpanel"
            aria-labelledby={tabId("screen", "settings")}
            hidden={tab !== "settings"}
          >
            <Settings shown={tab === "settings"} />
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
        </div>
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

/** WAI-ARIA tabs, vertical: one tab stop, Up / Down / Home / End. */
function RailNav(props: {
  label: string;
  value: Tab;
  onChange: (t: Tab) => void;
  items: { id: Tab; label: ReactNode; title: string; badge?: ReactNode }[];
}) {
  const ids = props.items.map((i) => i.id);
  const onKey = (e: KeyboardEvent) => {
    const i = ids.indexOf(props.value);
    const next =
      e.key === "ArrowDown" ? ids[(i + 1) % ids.length]
      : e.key === "ArrowUp" ? ids[(i - 1 + ids.length) % ids.length]
      : e.key === "Home" ? ids[0]
      : e.key === "End" ? ids[ids.length - 1]
      : undefined;
    if (next === undefined) return;
    e.preventDefault();
    props.onChange(next);
    document.getElementById(tabId("screen", next))?.focus();
  };
  return (
    <div className="rail-nav" role="tablist" aria-orientation="vertical" aria-label={props.label} onKeyDown={onKey}>
      {props.items.map((it) => {
        const on = it.id === props.value;
        return (
          <button
            key={it.id}
            type="button"
            role="tab"
            id={tabId("screen", it.id)}
            aria-selected={on}
            aria-controls={panelId("screen", it.id)}
            tabIndex={on ? 0 : -1}
            title={it.title}
            className="rail-item"
            onClick={() => props.onChange(it.id)}
          >
            <Icon name={TAB_ICON[it.id]} />
            <span className="rail-label">{it.label}</span>
            {it.badge}
          </button>
        );
      })}
    </div>
  );
}

const REPO = "https://github.com/quer3q/ps5-dump-forge";

/** A link in the http build; in the app, the default browser (a link would open inside its window). */
function GitHubLink() {
  const t = useMessages().app;
  const mark = (
    <svg className="icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M8 0c4.42 0 8 3.58 8 8a8.01 8.01 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A8 8 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  );
  const label = t.github(APP_NAME);
  return web ? (
    <a className="gh-link button rail-item" href={REPO} target="_blank" rel="noreferrer" aria-label={label} title={label}>
      {mark}
      <span className="rail-label" aria-hidden="true">GitHub</span>
    </a>
  ) : (
    <button type="button" className="gh-link rail-item" aria-label={label} title={label} onClick={() => void api.openRepo()}>
      {mark}
      <span className="rail-label" aria-hidden="true">GitHub</span>
    </button>
  );
}

function jobState(j: Job, t: Messages["app"]): string {
  if (j.stage === undefined) return t.queued;
  return t.progress(Math.floor(100 * j.shown), j.etaMs !== undefined ? prettyDuration(j.etaMs) : null);
}
