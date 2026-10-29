// Connect's File Transfer dialog: What to send → Send to, side by side (stacked when narrow), each
// with a switch of the devices online (where its picker opens) and the paths chosen. Files or a
// folder from any device (this one, a paired Forge, a saved NAS) go into a folder on any of them,
// as one transfer job (core copies; it shows in the job list) and the dialog closes. The web build
// adds Upload to this device and Download from this device in the footer (the browser's own files
// over plain HTTP, for a phone); the console's own browser can't pick files, so there they are off.

import { useEffect, useRef, useState } from "react";

import { api, connect, errorText, pick, pickFiles, runtimePlatform, web, type ConnectState, type ConvertRequest } from "./api";
import { Head, Modal, openOverlay, usePeerPath, ViewSwitch } from "./common";
import { connectError, ErrorLine } from "./Connect";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import type { Action } from "./jobs";
import { isLoopback } from "./launcher";
import { nasRoot } from "./Picker";
import { askReplace, confirmReplace } from "./replace";
import { useConnectState } from "./useConnectState";

/** A transfer job: `paths` (on any device) copied into the folder `output`. */
export function transferRequest(paths: string[], output: string): ConvertRequest {
  return {
    source: "",
    format: "folder",
    output,
    compression_threads: null,
    inner: null,
    remove_backport: false,
    full_verify: false,
    transfer: paths,
  };
}

/** Whether `path` has a name to land under (core's `transfer::name_of`): not a peer's, a share's,
 * a drive's or the filesystem's root. */
function hasName(path: string): boolean {
  const inner = path.replace(/^(flink|ftp|nfs):\/\/[^/]*/i, "").replace(/^smb:\/\/[^/]*\/[^/]*/i, "");
  const last = inner.split(/[\\/]/).filter(Boolean).pop();
  return !!last && last !== "." && last !== ".." && !/^[A-Za-z]:$/.test(last);
}

/** File Transfer; `start` (a device's root, `flink://<key_id>` or a saved NAS's, from its Send)
 * is where the destination picker opens. */
export function openTransfer(dispatch: (a: Action) => void, start?: string) {
  // However it closes (Close, Escape, a click outside, another dialog closing them all), a Start
  // still checking the destination then starts nothing.
  let open = true;
  openOverlay(
    (close) => <TransferDialog start={start} dispatch={dispatch} close={close} isOpen={() => open} />,
    () => (open = false),
  );
}

/** A side's device when its paths' device isn't online: no switch option matches it. */
const GONE = "\u0000";

/** The device a path is on: the online device whose root holds it, else "" (this device). */
function deviceOf(path: string, roots: string[]): string {
  return roots.find((r) => path === r || path.startsWith(`${r}/`)) ?? "";
}

function TransferDialog(props: { start?: string; dispatch: (a: Action) => void; close: () => void; isOpen: () => boolean }) {
  const t = useMessages().connect;
  const live = useConnectState().state;
  // The last good state stays while a poll fails (a service hiccup), so the switches and Send to
  // don't flicker away; only a real poll drops a side that went offline (below).
  const last = useRef<ConnectState | null>(null);
  if (live) last.current = live;
  const state = live ?? last.current;
  const [sources, setSources] = useState<string[]>([]);
  const [output, setOutput] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ lead?: string; text: string } | null>(null);
  /** A browser upload is running (Start would close the dialog and cut it short). */
  const [uploading, setUploading] = useState(false);
  /** `uploading` as of now, for a Start that awaited its check meanwhile. */
  /** Set by the upload itself as it starts and ends: `uploading` as of now, for a Start that
   * awaited its check meanwhile (the state arrives a render later). */
  const uploadingNow = useRef(false);
  const shown = usePeerPath();
  // The devices either side can be on: this one ("") and each online paired Forge and saved NAS.
  const devices = [
    ...(state?.devices ?? []).filter((d) => d.online).map((d) => ({ id: `flink://${d.key_id}`, label: <span title={d.name}>{d.name}</span> })),
    ...(state?.nas ?? []).filter((n) => n.online).map((n) => ({ id: nasRoot(n.host, n.port, n.protocol), label: <span title={n.name}>{n.name}</span> })),
  ];
  const roots = devices.map((d) => d.id);
  // Nothing to send between while no other device is online (a paired Forge or a NAS).
  const alone = devices.length === 0;
  // A remote path whose device isn't online now (gone offline, a failed poll, or a pick that
  // raced a change): its side shows no device and Start waits.
  const offline = (p: string) => /^(flink|smb|ftp|nfs):\/\//i.test(p) && deviceOf(p, roots) === "";
  // Which device each side shows: the chosen paths' own device, else the one the switch set (where
  // its picker opens). GONE matches no option, so the switch shows none.
  const [from, setFrom] = useState("");
  const [to, setTo] = useState(() => props.start ?? "");
  const sideOf = (paths: string[], picked: string) =>
    paths.length === 0 ? (roots.includes(picked) ? picked : "") : offline(paths[0]) ? GONE : deviceOf(paths[0], roots);
  const fromOn = sideOf(sources, from);
  const toOn = sideOf(output ? [output] : [], to);
  const unsettled = sources.some(offline) || (output !== null && offline(output));
  // What Start would send now: a Start checking the destination starts nothing once it changes.
  const selection = JSON.stringify([sources, output, unsettled]);
  const now = useRef(selection);
  now.current = selection;
  // A side's device gone offline: what was chosen there goes too. Only on a real poll: a failed
  // one keeps the last good state.
  const key = roots.join("\n");
  useEffect(() => {
    if (!live) return;
    if (sources.some(offline)) setSources([]);
    if (output && offline(output)) setOutput(null);
    // Run when the online devices change (the paths are read as they are then).
  }, [key, live !== null]);
  // Switching a side to another device drops what was chosen on the old one, so Start never
  // sends from or to a device the switch no longer shows. Not while Start runs (the switches are off).
  const showFrom = (d: string) => {
    if (busy) return;
    setFrom(d);
    if (sources.length > 0 && fromOn !== d) setSources([]);
  };
  const showTo = (d: string) => {
    if (busy) return;
    setTo(d);
    if (output && toOn !== d) setOutput(null);
  };
  /** Runs a picker; a pick (one or more paths) goes to `set`, a cancel changes nothing. */
  const choose = async (picked: Promise<string[] | string | null>, set: (paths: string[]) => void) => {
    setError(null);
    try {
      const p = await picked;
      if (p?.length) set(typeof p === "string" ? [p] : p);
    } catch (e) {
      setError({ text: errorText(e) });
    }
  };
  const setFromPaths = (paths: string[]) => {
    setSources(paths);
    setFrom(deviceOf(paths[0], roots));
  };
  // Sources and the destination on any device: this one, a paired Forge or a NAS.
  const sourceStart = roots.includes(fromOn) ? fromOn : undefined;
  const chooseFiles = () => choose(pickFiles({ directory: false, title: t.pickSendFiles, devices: true, nas: true, start: sourceStart, local: fromOn === "" }), setFromPaths);
  const chooseFolder = () =>
    choose(pick({ directory: true, title: t.pickSendFolder, devices: true, nas: true, start: sourceStart, local: fromOn === "" }), (paths) => {
      if (paths.every(hasName)) setFromPaths(paths);
      else setError({ text: t.wholeRoot });
    });
  // The chosen folder when it is on the device shown, else that device's root.
  const outputStart = output && !offline(output) && deviceOf(output, roots) === toOn ? output : roots.includes(toOn) ? toOn : undefined;
  const chooseOutput = () =>
    choose(pick({ directory: true, title: t.pickSaveTo, devices: true, start: outputStart, local: toOn === "" }), ([p]) => {
      setOutput(p);
      setTo(deviceOf(p, roots));
    });
  const start = async () => {
    if (busy || !output || sources.length === 0 || unsettled) return;
    setBusy(true);
    setError(null);
    // The dialog closed, or what it sends changed (a device went offline), while it checked:
    // nothing starts.
    const asked = selection;
    const still = () => props.isOpen() && now.current === asked;
    try {
      // Names already there: asked once; Replace sends `replace`, Cancel starts nothing.
      const request = await askReplace(transferRequest(sources, output), still);
      if (!request || !still()) return setBusy(false);
      const id = await api.startJob(request);
      props.dispatch({ type: "started", id, request });
      // An upload began while it checked: the dialog stays open for it (closing would cut it
      // short); the started job's sources are cleared so it isn't started twice.
      if (uploadingNow.current) {
        setSources([]);
        setBusy(false);
      } else props.close();
    } catch (e) {
      setError({ lead: t.cantStart, text: errorText(e) });
      setBusy(false);
    }
  };
  // An upload in the footer would be cut short: only its Cancel stops it, not Close, Escape or a
  // click outside.
  const closeIdle = () => {
    if (!uploading) props.close();
  };
  const tabs = devices.length > 0 ? [{ id: "", label: t.thisDevice }, ...devices] : [];
  return (
    <Modal close={closeIdle} className="modal card transfer-modal" labelledBy="transfer-title">
      <Head id="transfer-title" icon="move" title={t.fileTransfer} />
      <p className="muted">{t.transferIntro}</p>
      <div className="transfer-body" role="region" tabIndex={0} aria-label={t.fileTransfer}>
        <div className="transfer-ends">
          <section className="transfer-end" aria-labelledby="transfer-what">
            <h3 id="transfer-what">{t.sendWhat}</h3>
            {tabs.length > 0 && <ViewSwitch className="devices" label={t.sendWhat} value={fromOn} onChange={showFrom} options={tabs} disabled={busy} />}
            <Paths paths={sources.map(shown)} empty={t.nothingChosen} label={t.sendWhat} />
            <div className="row">
              <button type="button" className="small" disabled={busy} onClick={() => void chooseFiles()}>
                <Icon name="file-plus" />
                {t.chooseFiles}
              </button>
              <button type="button" className="small" disabled={busy} onClick={() => void chooseFolder()}>
                <Icon name="folder-plus" />
                {t.chooseFolder}
              </button>
            </div>
          </section>
          <span className="transfer-arrow" aria-hidden="true">
            <Icon name="arrow" />
          </span>
          <section className="transfer-end" aria-labelledby="transfer-to">
            <h3 id="transfer-to">{t.sendTo}</h3>
            {tabs.length > 0 && <ViewSwitch className="devices" label={t.sendTo} value={toOn} onChange={showTo} options={tabs} disabled={busy} />}
            <Paths paths={output ? [shown(output)] : []} empty={t.nothingChosen} label={t.sendTo} />
            <div className="row">
              <button
                type="button"
                className="small"
                disabled={busy || alone}
                aria-describedby={alone ? "transfer-alone" : undefined}
                onClick={() => void chooseOutput()}
              >
                <Icon name="folder" />
                {t.chooseFolder}
              </button>
            </div>
            {alone && (
              <p className="muted note" id="transfer-alone">
                {t.sendNeedsDevice}
              </p>
            )}
          </section>
        </div>
        {error && <ErrorLine lead={error.lead} text={error.text} />}
      </div>
      <div className="transfer-foot">
        {web && <BrowserFiles onBusy={setUploading} busyNow={uploadingNow} />}
        <div className="row end transfer-actions">
          <button type="button" autoFocus aria-disabled={uploading || undefined} onClick={closeIdle}>
            {t.close}
          </button>
          <button
            type="button"
            className="primary"
            // Off while starting by aria-disabled, not disabled: focus stays on it for a refusal.
            aria-disabled={busy || undefined}
            aria-busy={busy || undefined}
            disabled={uploading || unsettled || !output || sources.length === 0}
            onClick={() => void start()}
          >
            <Icon name="move" />
            {t.startTransfer}
          </button>
        </div>
      </div>
    </Modal>
  );
}

/** The paths chosen, one per line (as written: paths stay left to right). */
function Paths({ paths, empty, label }: { paths: string[]; empty: string; label: string }) {
  if (paths.length === 0) return <p className="out-box transfer-paths empty">{empty}</p>;
  // It scrolls past 120px: a Tab stop, so the keys can scroll it.
  return (
    <ul className="out-box transfer-paths" tabIndex={0} aria-label={label}>
      {paths.map((p) => (
        <li key={p} className="transfer-path" dir="ltr">
          {p}
        </li>
      ))}
    </ul>
  );
}

/** Web build: this browser's files into a folder on this device, one at a time (progress,
 * Cancel), and a file of this device to this browser. Off in the console's own browser. */
function BrowserFiles({ onBusy, busyNow }: { onBusy: (busy: boolean) => void; busyNow: { current: boolean } }) {
  const t = useMessages().connect;
  const input = useRef<HTMLInputElement>(null);
  const [sending, setSending] = useState<{ name: string; i: number; n: number; sent: number; total: number } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  /** Files left out of the last upload: a folder or link of that name is there. */
  const [skipped, setSkipped] = useState<string | null>(null);
  const [error, setError] = useState<{ lead?: string; text: string } | null>(null);
  // On a loopback page `null` (unknown) until the platform check answers: off, with no note yet.
  const [onConsole, setOnConsole] = useState<boolean | null>(() => (isLoopback(location.hostname) ? null : false));
  const abort = useRef<AbortController | null>(null);
  /** An upload runs, from its folder pick and name check on: one at a time, so Cancel (and closing
   * the dialog) always stops the one running. The ref answers before the next render does. */
  const [running, setRunning] = useState(false);
  const runningRef = useRef(false);
  useEffect(() => () => abort.current?.abort(), []);
  useEffect(() => onBusy(running || sending !== null), [running, sending, onBusy]);
  // A reload, Back or swipe-back would drop the upload: the browser asks first (iOS doesn't).
  const active = sending !== null;
  useEffect(() => {
    if (!active) return;
    const stay = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    addEventListener("beforeunload", stay);
    return () => removeEventListener("beforeunload", stay);
  }, [active]);
  useEffect(() => {
    // The console's own browser: a loopback page of the PS5's server (a computer's `serve` is "host").
    if (isLoopback(location.hostname))
      void runtimePlatform().then(
        (p) => setOnConsole(p === "ps5"),
        () => {},
      );
  }, []);
  const upload = async (files: File[]) => {
    if (runningRef.current) return;
    setError(null);
    setNote(null);
    setSkipped(null);
    if (files.length === 0) return;
    runningRef.current = true;
    busyNow.current = true;
    setRunning(true);
    const ctl = (abort.current = new AbortController());
    // Files finished before a failure or Cancel stay on this device.
    let done = 0;
    let total = files.length;
    try {
      const dir = await pick({ directory: true, title: t.uploadTo });
      if (!dir) return;
      // Names already there: a file is replaced once the user confirms; a folder or link of that
      // name can't be (the server refuses it), so that file is left out and said.
      // ponytail: `listDir` stops at 10,000 entries; a name past that isn't asked about, so its
      // upload goes without `replace` and the server refuses it ("already exists").
      // Names compared case-insensitively: on a drive that folds case they are the same file (one
      // that doesn't just gets a new file after a needless question).
      // An exact name wins over a case variant (a case-sensitive folder may hold both).
      const listed = (await api.listDir(dir)).entries;
      const exact = new Map(listed.map((e) => [e.name, e]));
      const folded = new Map(listed.map((e) => [e.name.toLowerCase(), e]));
      const there = { get: (name: string) => exact.get(name) ?? folded.get(name.toLowerCase()) };
      // The dialog closed meanwhile (its unmount aborts): nothing is asked or sent.
      if (ctl.signal.aborted) return;
      const skipped = files.filter((f) => {
        const e = there.get(f.name);
        return !!e && (e.dir || !!e.link);
      });
      const send = files.filter((f) => !skipped.includes(f));
      const clash = send.filter((f) => !!there.get(f.name)).map((f) => f.name);
      if (clash.length > 0 && !(await confirmReplace(clash, dir))) return;
      if (ctl.signal.aborted) return;
      if (skipped.length > 0) setSkipped(t.uploadSkipped(skipped.map((f) => f.name).join(", ")));
      total = send.length;
      if (total === 0) return;
      for (const [i, f] of send.entries()) {
        setSending({ name: f.name, i: i + 1, n: send.length, sent: 0, total: f.size });
        const replace = clash.includes(f.name);
        await connect.upload(dir, f.name, f, (sent, total) => setSending((s) => s && { ...s, sent, total }), ctl.signal, replace);
        done++;
      }
      setNote(t.uploaded);
    } catch (e) {
      if (!ctl.signal.aborted) setError({ lead: t.cantUpload, text: connectError(e) });
      if (done > 0) setNote(t.uploadedBefore(done, total));
    } finally {
      setSending(null);
      abort.current = null;
      runningRef.current = false;
      busyNow.current = false;
      setRunning(false);
    }
  };
  const download = async () => {
    setError(null);
    let path: string | null;
    try {
      path = await pick({ directory: false, title: t.pickDownload });
    } catch (e) {
      setError({ text: errorText(e) });
      return;
    }
    const url = path && connect.downloadUrl(path);
    if (!url) return;
    const a = document.createElement("a");
    a.href = url;
    a.download = "";
    document.body.append(a);
    a.click();
    a.remove();
  };
  return (
    <div className="transfer-browser" role="group" aria-label={t.thisBrowser} aria-describedby="transfer-browser-note">
      <input
        ref={input}
        type="file"
        multiple
        hidden
        aria-label={t.uploadPick}
        onChange={(e) => {
          const files = [...(e.target.files ?? [])];
          e.target.value = "";
          void upload(files);
        }}
      />
      {sending ? (
        <div className="upload-line">
          <progress value={sending.sent} max={Math.max(sending.total, 1)} aria-label={t.uploading(sending.name, sending.i, sending.n)} />
          <span className="muted" dir="auto">
            {t.uploading(sending.name, sending.i, sending.n)}
          </span>{" "}
          <button type="button" className="small" onClick={() => abort.current?.abort()}>
            {t.cancel}
          </button>
        </div>
      ) : (
        <div className="row">
          <button
            type="button"
            className="small"
            disabled={onConsole !== false}
            // Off by aria-disabled while its folder and names are checked: focus stays on it.
            aria-disabled={running || undefined}
            aria-busy={running || undefined}
            onClick={() => !runningRef.current && input.current?.click()}
          >
            <Icon name="up" />
            {t.uploadHere}
          </button>
          <button type="button" className="small" disabled={onConsole !== false} onClick={() => void download()}>
            <Icon name="download" />
            {t.downloadHere}
          </button>
        </div>
      )}
      <p className="muted note" id="transfer-browser-note">
        {onConsole ? t.consoleNoFiles : onConsole === false && t.browserHint}
      </p>
      {/* Mounted before its text, so a screen reader announces it. */}
      <p className="muted note" role="status">
        {note}
        {note && skipped && <br />}
        {skipped}
      </p>
      {error && <ErrorLine lead={error.lead} text={error.text} />}
    </div>
  );
}
