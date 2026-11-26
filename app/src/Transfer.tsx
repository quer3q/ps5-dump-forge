// The Transfer screen: device tabs over two panes (This device left, the chosen device right);
// a side's selection copies into the other pane's folder as a transfer job, .pkg files install on a PS5.

import { useEffect, useRef, useState, type ReactNode } from "react";

import {
  api,
  connect,
  onDrop,
  runtimePlatform,
  web,
  type ConnectState,
  type ConvertRequest,
  type DirEntry,
  type RuntimePlatform,
} from "./api";
import { Dots } from "./common";
import { connectError, ErrorLine } from "./Connect";
import { DeviceTabs, tabPanelProps, type DeviceTab } from "./DeviceTabs";
import { useRowDrag } from "./FilesTabs";
import { useMessages } from "./i18n";
import { Icon } from "./icons";
import { confirmInstall, installRequest } from "./InstallDialog";
import type { Action } from "./jobs";
import { isLoopback } from "./launcher";
import { readable } from "./peers";
import { nasRoot } from "./Picker";
import { askReplace, confirmReplace } from "./replace";
import { dragPaths, TransferPane, transferDropTarget } from "./TransferPane";
import { refreshConnect, useConnectState } from "./useConnectState";
import "./transfer.css";

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

/** Whether `path` has a name to land under (core's `transfer::name_of`): not a device, share or filesystem root. */
function hasName(path: string): boolean {
  const inner = path.replace(/^(flink|ftp|nfs):\/\/[^/]*/i, "").replace(/^smb:\/\/[^/]*\/[^/]*/i, "");
  const last = inner.split(/[\\/]/).filter(Boolean).pop();
  return !!last && last !== "." && last !== ".." && !/^[A-Za-z]:$/.test(last);
}

const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

export type StatusLine = { ok: true; text: string } | { ok: false; lead: string; text: string };

/** Copy (Replace asked first) and install, one action at a time; `line` says what started or why not. */
function useActions(dispatch: (a: Action) => void) {
  const m = useMessages();
  const [line, setLine] = useState<StatusLine | null>(null);
  const busy = useRef(false);
  const run = async (lead: string, f: () => Promise<boolean>): Promise<boolean> => {
    if (busy.current) return false;
    busy.current = true;
    try {
      return await f();
    } catch (e) {
      setLine({ ok: false, lead, text: connectError(e) });
      return false;
    } finally {
      busy.current = false;
    }
  };
  const copy = (paths: string[], dest: string | null): Promise<boolean> => {
    const lead = m.storage.cantCopy;
    if (paths.length === 0) return Promise.resolve(false);
    if (dest === null) return (setLine({ ok: false, lead, text: m.storage.copyNoFolder }), Promise.resolve(false));
    if (!paths.every(hasName)) return (setLine({ ok: false, lead, text: m.connect.wholeRoot }), Promise.resolve(false));
    return run(lead, async () => {
      const request = await askReplace(transferRequest(paths, dest));
      if (!request) return false;
      const id = await api.startJob(request);
      dispatch({ type: "started", id, request });
      const to = readable(dest);
      setLine({ ok: true, text: paths.length === 1 ? m.storage.copyOne(nameOf(paths[0]), to) : m.storage.copyMany(paths.length, to) });
      return true;
    });
  };
  /** Each package asked in turn (Cancel stops the rest), then installed by `peer` over FLINK, else here (the PS5). */
  const install = (pkgs: DirEntry[], peer: { keyId: string; name: string } | null): Promise<boolean> =>
    run(m.install.cantInstall, async () => {
      let started = false;
      for (const p of pkgs) {
        if (!(await confirmInstall({ path: p.path, size: p.size, device: peer?.name }))) break;
        if (peer) {
          await connect.install(peer.keyId, p.path);
          setLine({ ok: true, text: m.install.sent(peer.name) });
          void refreshConnect();
        } else {
          const request = installRequest(p.path);
          const id = await api.startJob(request);
          dispatch({ type: "started", id, request });
          setLine({ ok: true, text: m.install.started(p.name) });
        }
        started = true;
      }
      return started;
    });
  return { copy, install, line, setLine };
}

/** What started (with Open Queue) or why not. Always mounted: announced. */
export function CopyStatus(props: { line: StatusLine | null; onQueue: () => void }) {
  const t = useMessages().build;
  const l = props.line;
  return (
    <div role="status" className="files-copy">
      {l?.ok === true && (
        <p className="note-line">
          <Icon name="move" />
          <span className="grow">{l.text}</span>
          <button type="button" className="small" onClick={props.onQueue}>
            {t.openQueue}
          </button>
        </p>
      )}
      {l?.ok === false && (
        <p className="note-line block">
          <Icon name="alert" />
          <span>
            {l.lead} <span className="backend">{l.text}</span>
          </span>
        </p>
      )}
    </div>
  );
}

/** The browser's device id (device roots hold `://`, so it can't clash). */
const BROWSER = "browser";

/** A device tab: `id` its root or BROWSER; `ps5` a paired PS5's key id. */
interface Choice extends DeviceTab {
  ps5?: string;
}

type Side = "here" | "there";

/** The console's own browser (a loopback page of the PS5's server); null until known. */
function useOnConsole(): boolean | null {
  const [on, setOn] = useState<boolean | null>(() => (isLoopback(location.hostname) ? null : false));
  useEffect(() => {
    if (isLoopback(location.hostname))
      void runtimePlatform().then(
        (p) => setOn(p === "ps5"),
        () => {},
      );
  }, []);
  return on;
}

/** `start`: a device chosen from Connect's Send (its root); a new `id` chooses it again. */
export function Transfer(props: {
  visible: boolean;
  dispatch: (a: Action) => void;
  onQueue: () => void;
  onConnect: () => void;
  start: { id: number; root: string } | null;
}) {
  const m = useMessages();
  const t = m.transfer;
  const snap = useConnectState();
  // The last good state stays while a poll fails, so the panes don't flicker.
  const last = useRef<ConnectState | null>(null);
  if (snap.state) last.current = snap.state;
  const state = snap.state ?? last.current;
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  useEffect(() => void runtimePlatform().then(setPlatform, () => {}), []);
  const onConsole = useOnConsole();
  const choices: Choice[] = [
    ...(state?.devices ?? []).map((d) => ({
      id: `flink://${d.key_id}`,
      name: d.name,
      icon: "drive" as const,
      online: d.online,
      ps5: d.platform === "ps5" ? d.key_id : undefined,
    })),
    ...(state?.nas ?? []).map((n) => ({ id: nasRoot(n.host, n.port, n.protocol), name: n.name, icon: "drive" as const, tag: n.protocol.toUpperCase(), online: n.online })),
    ...(web && onConsole === false ? [{ id: BROWSER, name: m.connect.thisBrowser, icon: "file" as const, online: true }] : []),
  ];
  const [chosen, setChosen] = useState<string | null>(null);
  useEffect(() => {
    if (props.start) setChosen(props.start.root);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.start?.id]);
  const online = choices.filter((c) => c.online);
  const firstOnline = online.length > 0 ? online[0].id : null;
  // None chosen or no longer saved: the first online tab. One gone offline stays chosen, greyed.
  const known = chosen !== null && choices.some((c) => c.id === chosen);
  useEffect(() => {
    if (chosen === null ? firstOnline !== null : !known && state !== null) setChosen(firstOnline);
  }, [chosen, known, firstOnline, state]);
  // A chosen device not listed yet (Connect's Send before the first answer) reads as offline.
  const device: Choice | null = chosen === null ? (online[0] ?? null) : (choices.find((c) => c.id === chosen) ?? { id: chosen, name: readable(chosen), icon: "drive", online: false });
  const up = !!device?.online;
  const browser = device?.id === BROWSER;

  const [sel, setSel] = useState<Record<Side, string[]>>({ here: [], there: [] });
  const [folder, setFolder] = useState<Record<Side, string | null>>({ here: null, there: null });
  const [refresh, setRefresh] = useState(0);
  const entries = useRef(new Map<string, DirEntry>());
  /** The side whose batch changed last: its Copy is the card's one primary button. */
  const lastSide = useRef<Side>("here");
  const select = (side: Side) => (paths: string[], e?: DirEntry) => {
    if (e) entries.current.set(e.path, e);
    lastSide.current = side;
    setSel((s) => ({ ...s, [side]: paths }));
  };
  const setHere = (side: Side) => (p: string | null) => setFolder((f) => (f[side] === p ? f : { ...f, [side]: p }));
  useEffect(() => setSel((s) => (s.there.length ? { ...s, there: [] } : s)), [chosen]);
  useEffect(() => {
    if (snap.state && !up) setSel((s) => (s.there.length ? { ...s, there: [] } : s));
  }, [snap.state, up]);

  const { copy, install, line } = useActions(props.dispatch);
  const other = (s: Side): Side => (s === "here" ? "there" : "here");
  const sideName = (s: Side) => (s === "here" ? m.connect.thisDevice : (device?.name ?? ""));
  const copySide = async (side: Side, paths: string[], dest: string | null) => {
    if (await copy(paths, dest)) setSel((s) => ({ ...s, [side]: [] }));
  };
  // Who installs a .pkg: this device when it is the PS5, else the paired PS5 chosen.
  const installer =
    platform === "ps5"
      ? { peer: null, name: state?.device.name ?? m.connect.thisDevice }
      : platform === "host" && up && device?.ps5
        ? { peer: { keyId: device.ps5, name: device.name }, name: device.name }
        : null;
  const pkgsOf = (paths: string[]): DirEntry[] | null => {
    const es = paths.map((p) => entries.current.get(p));
    return es.length > 0 && es.every((e) => !!e && !e.dir && /\.pkg$/i.test(e.name)) ? (es as DirEntry[]) : null;
  };
  const installSide = async (side: Side, pkgs: DirEntry[]) => {
    if (installer && (await install(pkgs, installer.peer))) setSel((s) => ({ ...s, [side]: [] }));
  };

  const from = useRef<Side>("here");
  const drag = useRowDrag<{ pane: string; path: string }>({
    target: (x, y) => {
      const d = transferDropTarget(x, y);
      return d && d.pane !== from.current ? d : null;
    },
    drop: (e, to) => void copySide(from.current, dragPaths(e, sel[from.current]), to.path),
    label: (e) => {
      const n = dragPaths(e, sel[from.current]).length;
      return n > 1 ? t.dragItems(n) : e.name;
    },
  });
  const down = (side: Side) => (ev: Parameters<typeof drag.onPointerDown>[0], e: DirEntry) => {
    from.current = side;
    drag.onPointerDown(ev, e);
  };
  // The app: files dropped from the system copy into the folder row under them, else the folder shown.
  const [sysOver, setSysOver] = useState<{ pane: string; path: string } | null>(null);
  const dropLive = useRef({ visible: props.visible, copy });
  dropLive.current = { visible: props.visible, copy };
  useEffect(() => {
    if (web) return;
    return onDrop(
      (d) => {
        const at = dropLive.current.visible ? transferDropTarget(d.x, d.y) : null;
        const paths = d.items.flatMap((i) => (i.path ? [i.path] : []));
        if (at) void dropLive.current.copy(paths, at.path);
      },
      (pt) => {
        const at = pt && dropLive.current.visible ? transferDropTarget(pt.x, pt.y) : null;
        setSysOver((o) => (o?.pane === at?.pane && o?.path === at?.path ? o : at));
      },
    );
  }, []);
  const over = drag.over ?? sysOver;
  const dropAt = (s: Side) => (over?.pane === s ? over.path : null);

  const copies = (s: Side) => sel[s].length > 0 && !(s === "there" && (!up || browser)) && !(s === "here" && browser);
  const bar = (side: Side): ReactNode => {
    const paths = sel[side];
    if (paths.length === 0 || (side === "there" && (!up || browser))) return <div />;
    const to = other(side);
    const dest = folder[to];
    const toName = sideName(to);
    const pkgs = installer ? pkgsOf(paths) : null;
    const why = !up ? t.deviceOffline(toName) : dest === null ? t.copyNoFolder(toName) : null;
    const primary = !copies(to) || lastSide.current === side;
    return (
      <div className="row transfer-bar">
        {!(side === "here" && browser) && (
          <button
            type="button"
            className={primary ? "small primary" : "small"}
            aria-disabled={why !== null || undefined}
            title={why ?? undefined}
            onClick={() => why === null && void copySide(side, paths, dest)}
          >
            <Icon name="move" />
            {withDevice(t.copyTo(paths.length, DEV), toName)}
          </button>
        )}
        {pkgs && installer && (
          <button type="button" className="small" onClick={() => void installSide(side, pkgs)}>
            <Icon name="package" />
            {withDevice(t.installOn(DEV), installer.name)}
          </button>
        )}
      </div>
    );
  };
  const files = sel.here.flatMap((p) => {
    const e = entries.current.get(p);
    return e && !e.dir ? [e] : [];
  });
  return (
    <div className="screen transfer-screen">
      {state === null && !snap.error ? (
        <p className="muted" role="status">
          {t.loadingDevices}
          <Dots />
        </p>
      ) : online.length === 0 || !device ? (
        <section className="card">
          <div className="empty">
            <span className="empty-icon">
              <Icon name="drive" />
            </span>
            <p className="empty-title">{t.noneTitle}</p>
            <p className="muted">{t.noneHint}</p>
            <div className="row center">
              <button type="button" className="primary" onClick={props.onConnect}>
                <Icon name="link" />
                {t.openConnect}
              </button>
            </div>
          </div>
        </section>
      ) : (
        <div className="transfer-tabs">
          <DeviceTabs label={t.withTitle} prefix="transfer" tabs={choices} value={device.id} onChange={setChosen} onAdd={props.onConnect} />
          <div className="card dev-window transfer-window" {...tabPanelProps("transfer", device.id)}>
            <div className="transfer-panes">
              <TransferPane
                id="here"
                label={m.connect.thisDevice}
                icon="drive"
                root={null}
                selected={sel.here}
                onSelected={select("here")}
                onHere={setHere("here")}
                visible={props.visible}
                refreshKey={refresh}
                onRowPointerDown={down("here")}
                dropAt={dropAt("here")}
              />
              {browser ? (
                <BrowserPane folder={folder.here} files={files} onUploaded={() => setRefresh((n) => n + 1)} />
              ) : up ? (
                <TransferPane
                  key={device.id}
                  id="there"
                  label={device.name}
                  icon={device.icon}
                  root={device.id}
                  selected={sel.there}
                  onSelected={select("there")}
                  onHere={setHere("there")}
                  visible={props.visible}
                  onRowPointerDown={down("there")}
                  dropAt={dropAt("there")}
                />
              ) : (
                <section className="transfer-pane transfer-offline" aria-label={t.pane(device.name)}>
                  <div className="row nowrap transfer-head">
                    <Icon name={device.icon} />
                    <span className="transfer-device grow" dir="auto">
                      {device.name}
                    </span>
                  </div>
                  <p className="note-line">
                    <Icon name="info" />
                    <span>{t.deviceOffline(device.name)}</span>
                  </p>
                </section>
              )}
            </div>
            <div className="transfer-bars">
              {bar("here")}
              {bar("there")}
            </div>
          </div>
        </div>
      )}
      <CopyStatus line={line} onQueue={props.onQueue} />
      {drag.ghost}
    </div>
  );
}

/** Stands for the device's name in a message, so the name can be cut on its own. */
const DEV = "\u2063DEV\u2063";
function withDevice(text: string, name: string): ReactNode {
  const [a, ...rest] = text.split(DEV);
  if (rest.length === 0) return text;
  return (
    <>
      {a}
      <span className="t-dev" title={name}>
        {name}
      </span>
      {rest.join(name)}
    </>
  );
}

/** Web build: this browser's files uploaded one at a time into `folder`, and `files` downloaded. */
function BrowserPane({ folder, files, onUploaded }: { folder: string | null; files: DirEntry[]; onUploaded: () => void }) {
  const t = useMessages().connect;
  const tt = useMessages().transfer;
  const input = useRef<HTMLInputElement>(null);
  const [sending, setSending] = useState<{ name: string; i: number; n: number; sent: number; total: number } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  /** Files left out of the last upload: a folder or link of that name is there, or a dropped folder. */
  const [skipped, setSkipped] = useState<string | null>(null);
  const [error, setError] = useState<{ lead?: string; text: string } | null>(null);
  const [over, setOver] = useState(false);
  const depth = useRef(0);
  const abort = useRef<AbortController | null>(null);
  /** One upload at a time, so Cancel always stops the one running; the ref answers before the next render. */
  const [running, setRunning] = useState(false);
  const runningRef = useRef(false);
  useEffect(() => () => abort.current?.abort(), []);
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
  const upload = async (list: File[], left: string[] = []) => {
    if (runningRef.current) return;
    setError(null);
    setNote(null);
    setSkipped(left.length > 0 ? tt.foldersLeftOut(left.join(", ")) : null);
    if (list.length === 0) return;
    const dir = folder;
    if (dir === null) return setError({ lead: t.cantUpload, text: tt.uploadNoFolder });
    runningRef.current = true;
    setRunning(true);
    const ctl = (abort.current = new AbortController());
    let done = 0;
    let total = list.length;
    try {
      // A file is replaced once confirmed; a folder or link of that name can't be, so the file is left out.
      // ponytail: `listDir` stops at 10,000 entries; a name past that isn't asked about and the server refuses it.
      // Case-insensitive (a drive that folds case); an exact name wins.
      const listed = (await api.listDir(dir)).entries;
      const exact = new Map(listed.map((e) => [e.name, e]));
      const folded = new Map(listed.map((e) => [e.name.toLowerCase(), e]));
      const there = (name: string) => exact.get(name) ?? folded.get(name.toLowerCase());
      if (ctl.signal.aborted) return;
      const blocked = list.filter((f) => {
        const e = there(f.name);
        return !!e && (e.dir || !!e.link);
      });
      const send = list.filter((f) => !blocked.includes(f));
      const clash = send.filter((f) => !!there(f.name)).map((f) => f.name);
      if (clash.length > 0 && !(await confirmReplace(clash, dir))) return;
      if (ctl.signal.aborted) return;
      if (blocked.length > 0) setSkipped([t.uploadSkipped(blocked.map((f) => f.name).join(", ")), ...(left.length ? [tt.foldersLeftOut(left.join(", "))] : [])].join(" "));
      total = send.length;
      if (total === 0) return;
      for (const [i, f] of send.entries()) {
        setSending({ name: f.name, i: i + 1, n: send.length, sent: 0, total: f.size });
        await connect.upload(dir, f.name, f, (sent, size) => setSending((s) => s && { ...s, sent, total: size }), ctl.signal, clash.includes(f.name));
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
      setRunning(false);
      if (done > 0) onUploaded();
    }
  };
  const download = async () => {
    setError(null);
    for (const [i, f] of files.entries()) {
      const url = connect.downloadUrl(f.path);
      if (!url) return;
      // A pause between them: a browser drops downloads started in one burst.
      if (i > 0) await new Promise((r) => setTimeout(r, 400));
      const a = document.createElement("a");
      a.href = url;
      a.download = "";
      document.body.append(a);
      a.click();
      a.remove();
    }
  };
  const hasFiles = (e: { dataTransfer: DataTransfer | null }) => !!e.dataTransfer && [...e.dataTransfer.types].includes("Files");
  return (
    <section
      className={`transfer-pane transfer-browser-pane${over ? " drop" : ""}`}
      aria-label={t.thisBrowser}
      onDragEnter={(e) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth.current++;
        setOver(true);
      }}
      onDragOver={(e) => {
        if (!hasFiles(e)) return;
        e.preventDefault(); // allows the drop (else the browser opens the file)
        e.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={(e) => {
        if (!hasFiles(e)) return;
        depth.current = Math.max(0, depth.current - 1);
        if (depth.current === 0) setOver(false);
      }}
      onDrop={(e) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth.current = 0;
        setOver(false);
        const list: File[] = [];
        const left: string[] = [];
        for (const it of [...e.dataTransfer.items]) {
          if (it.kind !== "file") continue;
          const entry = it.webkitGetAsEntry?.();
          const f = it.getAsFile();
          if (entry && !entry.isFile) left.push(entry.name);
          else if (f) list.push(f);
        }
        void upload(list, left);
      }}
    >
      <div className="row nowrap transfer-head">
        <Icon name="file" />
        <span className="transfer-device grow">{t.thisBrowser}</span>
      </div>
      <input
        ref={input}
        type="file"
        multiple
        hidden
        aria-label={t.uploadPick}
        onChange={(e) => {
          const list = [...(e.target.files ?? [])];
          e.target.value = "";
          void upload(list);
        }}
      />
      <div className="transfer-dropzone">
        <Icon name="up" />
        <p className="transfer-drop-title">{tt.dropFiles}</p>
        <p className="muted" dir="auto">
          {folder ? tt.uploadInto(readable(folder)) : tt.uploadNoFolder}
        </p>
        {sending ? (
          <div className="upload-line">
            <progress value={sending.sent} max={Math.max(sending.total, 1)} aria-label={t.uploading(sending.name, sending.i, sending.n)} />
            <div className="row">
              <span className="muted grow" dir="auto">
                {t.uploading(sending.name, sending.i, sending.n)}
              </span>
              <button type="button" className="small" onClick={() => abort.current?.abort()}>
                {t.cancel}
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            className="small primary"
            // Off by aria-disabled while its names are checked: focus stays on it.
            aria-disabled={running || folder === null || undefined}
            aria-busy={running || undefined}
            title={folder === null ? tt.uploadNoFolder : undefined}
            onClick={() => !runningRef.current && folder !== null && input.current?.click()}
          >
            <Icon name="file-plus" />
            {t.uploadPick}
          </button>
        )}
      </div>
      <div className="row transfer-download">
        <button
          type="button"
          className="small"
          aria-disabled={files.length === 0 || undefined}
          title={files.length === 0 ? tt.downloadNone : undefined}
          onClick={() => files.length > 0 && void download()}
        >
          <Icon name="download" />
          {files.length > 0 ? tt.downloadFiles(files.length) : t.downloadHere}
        </button>
        {files.length === 0 && <span className="muted note">{tt.downloadNone}</span>}
      </div>
      {/* Mounted before its text, so a screen reader announces it. */}
      <p className="muted note" role="status">
        {note}
        {note && skipped && <br />}
        {skipped}
      </p>
      {error && <ErrorLine lead={error.lead} text={error.text} />}
    </section>
  );
}
