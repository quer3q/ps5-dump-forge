// The Connect screen; state from the shared 2 s poller (useConnectState.ts), every dialog in its own root (`openOverlay`).

import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";

import { api, connect, errorText, pick, prettyBytes, serverAddress, web, type ConnectDevice, type ConnectState, type NasDevice, type PairOutgoing, type PairRequest } from "./api";
import { AddressQr, basename, CardHead, ChooseMenu, Dots, Head, Modal, openOverlay, ViewSwitch } from "./common";
import { openConnectHelp } from "./ConnectHelp";
import { getLang, msg, num, useMessages } from "./i18n";
import { Icon } from "./icons";
import { confirmInstall } from "./InstallDialog";
import { NAS_PORT, readable, type NasProtocol } from "./peers";
import { nasRoot } from "./Picker";
import { ps5Release } from "./update";
import { refreshConnect, useConnectState } from "./useConnectState";

export function connectError(e: unknown): string {
  const t = msg().connect;
  const text = errorText(e);
  const codes: Record<string, string> = {
    denied: t.denied,
    timed_out: t.timedOut,
    busy: t.pairBusy,
    already_paired: t.alreadyPaired,
    too_many_attempts: t.tooManyAttempts,
    not_paired: t.notPaired,
  };
  return codes[text] ?? text;
}

export function platformName(p: string): string {
  const names: Record<string, string> = { ps5: "PS5", macos: "Mac", windows: "Windows", linux: "Linux", freebsd: "FreeBSD" };
  return names[p] ?? msg().connect.computer;
}

/** In the largest whole unit; null when never seen. */
function seenAgo(s: number, now: number): string | null {
  if (!s) return null;
  const t = msg().connect;
  const m = Math.floor(Math.max(0, now - s) / 60);
  if (m < 1) return t.seen(0, "now");
  if (m < 60) return t.seen(m, "m");
  if (m < 24 * 60) return t.seen(Math.floor(m / 60), "h");
  return t.seen(Math.floor(m / (24 * 60)), "d");
}

function useNow(): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now() / 1000), 60_000);
    return () => clearInterval(id);
  }, []);
  return now;
}

/** As the NAS dialog takes it: the protocol's own port left out. */
function hostPort(host: string, port: number, protocol: NasProtocol): string {
  if (port === NAS_PORT[protocol]) return host;
  return host.includes(":") ? `[${host}]:${port}` : `${host}:${port}`;
}

/** The page's own server address in the web build, else the service's. */
function deviceIp(fallback: string | null): string | null {
  const a = serverAddress();
  return a ? a.replace(/:\d+$/, "").replace(/^\[(.*)\]$/, "$1") : fallback;
}

function lastSeen(s: number): string {
  if (!s) return msg().connect.never;
  const lang = getLang();
  return new Date(s * 1000).toLocaleString(lang === "ar" ? "ar-u-nu-latn" : lang);
}

export function ErrorLine({ text, lead }: { text: string; lead?: string }) {
  return (
    <p className="note-line block" role="alert">
      <Icon name="alert" />
      {lead ? (
        <span>
          {lead} <span className="backend">{text}</span>
        </span>
      ) : (
        <span className="backend">{text}</span>
      )}
    </p>
  );
}

function OnlineDot({ online }: { online: boolean }) {
  return <span className={online ? "online-dot on" : "online-dot"} aria-hidden="true" />;
}

/** One at a time; results kept in memory until the page closes. `running`: the key id under way. */
type Speed = { result?: string; error?: string };
let speed: { running: string | null; byKey: Record<string, Speed> } = { running: null, byKey: {} };
const speedListeners = new Set<() => void>();
function setSpeed(next: typeof speed) {
  speed = next;
  for (const l of speedListeners) l();
}
function useSpeed() {
  return useSyncExternalStore(
    (l) => {
      speedListeners.add(l);
      return () => speedListeners.delete(l);
    },
    () => speed,
  );
}

/** The run id goes with its cancel, so a late cancel never stops another run. */
let speedCount = 0;
let speedRun = "";
let speedStopped = "";

/** Every close of the dialog while it runs (Stop, Escape, a click outside, `closeOverlays`) stops it. */
async function runSpeedTest(keyId: string, name: string) {
  if (speed.running) return;
  // Not `crypto.randomUUID`: a page over plain HTTP (the PS5's) has none.
  const run = `${++speedCount}-${Math.random().toString(36).slice(2)}`;
  speedRun = run;
  setSpeed({ running: keyId, byKey: { ...speed.byKey, [keyId]: {} } });
  openOverlay(
    (close) => <SpeedDialog keyId={keyId} name={name} run={run} close={close} />,
    () => stopSpeedTest(keyId, run),
  );
  let out: Speed;
  try {
    const r = await connect.speedTest(keyId, run);
    const mb = (b: number) => num(b / 1e6, b < 10e6 ? 1 : 0);
    out = { result: msg().connect.speedResult(mb(r.up_bytes_per_s), mb(r.down_bytes_per_s)) };
  } catch (e) {
    out = { error: readable(connectError(e)) };
  }
  if (speedStopped === run) out = {};
  setSpeed({ running: null, byKey: { ...speed.byKey, [keyId]: out } });
}

function stopSpeedTest(keyId: string, run: string) {
  if (speed.running !== keyId || speedRun !== run || speedStopped === run) return;
  speedStopped = run;
  connect.speedTestCancel(keyId, run).catch(() => {});
}

function SpeedDialog(props: { keyId: string; name: string; run: string; close: () => void }) {
  const t = useMessages().connect;
  const running = useSpeed().running === props.keyId && speedRun === props.run;
  return (
    <Modal close={props.close} className="modal card" labelledBy="speed-title" describedBy="speed-text">
      <div className="modal-head">
        <span className="head-icon">
          <Icon name="bolt" />
        </span>
        <h2 id="speed-title" dir="auto">
          {t.speedTestWith(props.name)}
        </h2>
      </div>
      <div id="speed-text">
        {running ? (
          <>
            <p role="status">
              {t.speedTesting}
              <Dots />
            </p>
            <p className="muted">{t.speedNote}</p>
          </>
        ) : (
          <SpeedLine keyId={props.keyId} />
        )}
      </div>
      <div className="row end">
        {running ? (
          <button key="stop" autoFocus onClick={props.close}>
            <Icon name="x" />
            {t.stop}
          </button>
        ) : (
          <button key="close" autoFocus onClick={props.close}>
            {t.close}
          </button>
        )}
      </div>
    </Modal>
  );
}

function SpeedButton({ keyId, name, disabled }: { keyId: string; name: string; disabled?: boolean }) {
  const t = useMessages().connect;
  const s = useSpeed();
  const mine = s.running === keyId;
  return (
    <button
      type="button"
      aria-label={mine ? undefined : t.speedTestWith(name)}
      aria-disabled={mine || undefined}
      aria-busy={mine || undefined}
      disabled={disabled || (!!s.running && !mine)}
      aria-haspopup="dialog"
      onClick={() => void runSpeedTest(keyId, name)}
    >
      <Icon name="bolt" />
      {mine ? (
        <span>
          {t.speedTesting}
          <Dots />
        </span>
      ) : (
        t.speedTest
      )}
    </button>
  );
}

function SpeedLine({ keyId, row }: { keyId: string; row?: boolean }) {
  const t = useMessages().connect;
  const s = useSpeed().byKey[keyId];
  if (!s || (!s.result && !s.error)) return null;
  if (s.error) return <ErrorLine lead={t.speedFailed} text={s.error} />;
  return (
    <p className={row ? "dev-sub" : "note-line good"} role="status">
      {!row && <Icon name="bolt" />}
      <span>{s.result}</span>
    </p>
  );
}

type Show = "all" | "forge" | "nas";

export function Connect(props: { onTransfer: (root?: string) => void }) {
  const t = useMessages().connect;
  const { state, error } = useConnectState();
  // The last good state stays while a poll fails, so the list doesn't drop to "No saved devices yet".
  const last = useRef<ConnectState | null>(null);
  if (state) last.current = state;
  const shown = state ?? last.current;
  const me = shown?.device;
  const peers = shown?.devices ?? [];
  const saved = shown?.nas ?? [];
  // At most 9 in shown order (online first), so the cap never leaves out an online device.
  const kept: unknown[] = [
    ...peers.filter((d) => d.online),
    ...saved.filter((n) => n.online),
    ...peers.filter((d) => !d.online),
    ...saved.filter((n) => !n.online),
  ].slice(0, 9);
  const devices = peers.filter((d) => kept.includes(d));
  const nas = saved.filter((n) => kept.includes(n));
  const total = devices.length + nas.length;
  const online = devices.filter((d) => d.online).length + nas.filter((n) => n.online).length;
  const now = useNow();
  const ip = deviceIp(me?.ip ?? null);
  const seen = (on: boolean, last: number) => (on ? t.online : [t.offline, seenAgo(last, now)].filter(Boolean).join(" · "));
  const [show, setShow] = useState<Show>("all");
  const view: Show = devices.length > 0 && nas.length > 0 ? show : "all";
  // A search this tab asked for: Nearby and its outcome stay until the tab is left.
  const [asked, setAsked] = useState(false);
  const [nearbyError, setNearbyError] = useState<string | null>(null);
  const searching = state?.discovery.searching ?? false;
  // `asked` once the service has the search, so "No new devices found" can't flash before it runs.
  const discover = async () => {
    if (searching) return;
    setNearbyError(null);
    try {
      await connect.discoverStart();
      await refreshConnect();
    } catch (e) {
      setNearbyError(connectError(e));
    }
    setAsked(true);
  };
  const send = (root: string) => props.onTransfer(root);
  const inst = useMessages().install;
  const [installed, setInstalled] = useState<{ ok: string } | { err: string } | null>(null);
  const installing = useRef(false);
  const installOn = async (d: ConnectDevice) => {
    if (installing.current) return;
    installing.current = true;
    try {
      const path = await pick({ directory: false, title: inst.pickTitle, extensions: ["pkg"] });
      if (!path || !(await confirmInstall({ path, device: d.name }))) return;
      setInstalled(null);
      await connect.install(d.key_id, path);
      setInstalled({ ok: inst.sent(d.name) });
      void refreshConnect();
    } catch (e) {
      setInstalled({ err: connectError(e) });
    } finally {
      installing.current = false;
    }
  };
  const rows: ReactNode[] = [];
  for (const on of [true, false]) {
    if (view !== "nas")
      for (const d of devices.filter((x) => x.online === on))
        rows.push(
          <DeviceRow
            key={d.key_id}
            name={d.name}
            kind={t.forge}
            online={d.online}
            facts={[platformName(d.platform), d.address && <span className="mono" dir="ltr">{d.address}</span>, seen(d.online, d.last_seen)]}
            live={!!state}
            onSend={() => send(`flink://${d.key_id}`)}
            onInstall={d.platform === "ps5" ? () => void installOn(d) : undefined}
            speedKey={d.key_id}
            onDetails={() => openDevice(d.key_id, props.onTransfer)}
          />,
        );
    if (view !== "forge")
      for (const n of nas.filter((x) => x.online === on))
        rows.push(
          <DeviceRow
            key={n.id}
            name={n.name}
            kind={nasKind(n.protocol)}
            online={n.online}
            facts={[
              <span className="mono" dir="ltr">{hostPort(n.host, n.port, n.protocol)}</span>,
              n.protocol !== "nfs" && (n.user ? <span dir="auto">{t.userIs(n.user)}</span> : t.guest),
              seen(n.online, n.last_online),
            ]}
            live={!!state}
            onSend={() => send(nasRoot(n.host, n.port, n.protocol))}
            onDetails={() => openNasDetails(n.id)}
          />,
        );
  }
  return (
    <div className="screen connect-screen">
      <div className="connect-cols">
        <div className="connect-side">
          <section className="card connect-me" aria-labelledby="connect-me">
            <CardHead icon="lock" title={t.thisDevice} id="connect-me">
              <button type="button" className="small" aria-haspopup="dialog" onClick={openConnectHelp}>
                <Icon name="info" />
                {t.howItWorks}
              </button>
            </CardHead>
            {me ? (
              <>
                <p className="connect-name">
                  <b dir="auto">{me.name}</b> <span className="tag">{platformName(me.platform)}</span>
                </p>
                {/* The web build only: the desktop app has no address, so no QR code (nor its hint). */}
                <AddressQr size={220} connect className="connect-qr" />
                <dl className="tiles">
                  <div className="tile">
                    <dt>{t.ipAddress}</dt>
                    {/* "Unknown" is a word: the reader's direction and font, not the address's. */}
                    <dd className={ip ? "connect-ip mono" : "connect-ip"} dir={ip ? "ltr" : "auto"}>
                      {ip ?? t.unknown}
                    </dd>
                  </div>
                </dl>
                {me.listen_error && <ErrorLine text={t.listenError(me.listen_error)} />}
              </>
            ) : (
              !error && <Loading />
            )}
            {!state && error && <ErrorLine text={t.unavailable(error)} />}
          </section>
          {/* The desktop app only: the PS5's own page has nothing to install it on. */}
          {!web && <InstallPs5 devices={peers} />}
          <AddDevice ready={!!state} />
        </div>
        <section className="card connect-devices" aria-labelledby="connect-devices">
          <CardHead icon="grid" title={t.devices} id="connect-devices">
            {total > 0 && (
              <button type="button" className="small" disabled={!state} aria-disabled={searching || undefined} onClick={() => void discover()}>
                <Icon name="search" />
                {t.discover}
              </button>
            )}
            <button
              type="button"
              className={total > 0 ? "small primary" : "small"}
              disabled={!state}
              onClick={() => props.onTransfer()}
            >
              <Icon name="move" />
              {t.fileTransfer}
            </button>
          </CardHead>
          {total > 0 ? (
            <>
              <div className="connect-tools">
                {devices.length > 0 && nas.length > 0 ? (
                  <ViewSwitch
                    label={t.show}
                    value={view}
                    onChange={setShow}
                    options={[
                      { id: "all", label: <Counted label={t.all} n={total} /> },
                      { id: "forge", label: <Counted label={t.forge} n={devices.length} /> },
                      { id: "nas", label: <Counted label={t.kindNas} n={nas.length} /> },
                    ]}
                  />
                ) : (
                  <span />
                )}
                <span className="muted connect-online">{t.onlineOf(online, total)}</span>
              </div>
              <ul className="dev-list">{rows}</ul>
              {installed && "err" in installed && <ErrorLine lead={inst.cantInstall} text={installed.err} />}
              {installed && "ok" in installed && (
                <p className="note-line muted" role="status">
                  <Icon name="info" />
                  <span>{installed.ok}</span>
                </p>
              )}
            </>
          ) : !shown ? (
            !error && <Loading />
          ) : (
            <div className="empty">
              <span className="empty-icon">
                <Icon name="grid" />
              </span>
              <p className="empty-title">{t.savedEmpty}</p>
              <p className="muted">{t.savedEmptyHint}</p>
              <div className="row center">
                <button type="button" className="primary" disabled={!state} aria-disabled={searching || undefined} onClick={() => void discover()}>
                  <Icon name="search" />
                  {t.discover}
                </button>
              </div>
            </div>
          )}
          <Nearby state={shown} asked={asked} error={nearbyError} />
        </section>
      </div>
    </div>
  );
}

function Loading() {
  const t = useMessages().connect;
  return (
    <p className="note-line muted" role="status">
      <span className="spinner" aria-hidden="true" />
      <span>
        {t.loading}
        <Dots />
      </span>
    </p>
  );
}

function Counted({ label, n }: { label: string; n: number }) {
  return (
    <>
      {label} <span className="n">{num(n)}</span>
    </>
  );
}

/** `live`: false while a failed poll shows the last good state. */
function DeviceRow(props: {
  name: string;
  kind: string;
  online: boolean;
  live: boolean;
  facts: ReactNode[];
  onSend: () => void;
  onDetails: () => void;
  onInstall?: () => void;
  speedKey?: string;
}) {
  const t = useMessages().connect;
  const inst = useMessages().install;
  const more = useMessages().storage.more;
  const details = useMessages().edit.details;
  const s = useSpeed();
  const testing = !!props.speedKey && s.running === props.speedKey;
  return (
    <li className="dev-row">
      <OnlineDot online={props.online} />
      <div className="dev-text">
        <p className="dev-title">
          <span dir="auto">{props.name}</span> <span className="tag">{props.kind}</span>
        </p>
        <p className="dev-sub">
          {props.facts.filter(Boolean).map((f, i) => (
            <span key={i}>{f}</span>
          ))}
        </p>
        {props.speedKey && <SpeedLine keyId={props.speedKey} row />}
      </div>
      <div className="dev-actions">
        {props.online && props.onInstall && (
          <button type="button" className="small install" aria-haspopup="dialog" aria-label={inst.installPkgOn(props.name)} disabled={!props.live} onClick={props.onInstall}>
            <Icon name="package" />
            {inst.installPkg}
          </button>
        )}
        {props.online && (
          <button type="button" className="small" aria-label={t.sendFilesTo(props.name)} disabled={!props.live} onClick={props.onSend}>
            <Icon name="move" />
            {t.send}
          </button>
        )}
        <ChooseMenu
          className="small icon-only"
          label={<Icon name="dots" />}
          buttonLabel={more(props.name)}
          menuLabel={more(props.name)}
          items={[
            { id: "details" as const, icon: "info" as const, label: details },
            ...(props.online && props.speedKey
              ? [
                  {
                    id: "speed" as const,
                    icon: "bolt" as const,
                    label: testing ? (
                      <span>
                        {t.speedTesting}
                        <Dots />
                      </span>
                    ) : (
                      t.speedTest
                    ),
                    disabled: !props.live || !!s.running,
                    busy: testing,
                  },
                ]
              : []),
          ]}
          onChoose={(what) => {
            if (what === "details") props.onDetails();
            else if (props.speedKey) void runSpeedTest(props.speedKey, props.name);
          }}
        />
      </div>
    </li>
  );
}

function ps5Host(devices: ConnectDevice[]): string {
  const a = devices.find((d) => d.platform === "ps5" && d.address)?.address ?? "";
  const v6 = /^\[([^\]]+)\]/.exec(a);
  return v6 ? v6[1] : a.replace(/:\d+$/, "");
}

/** Install on PS5 (desktop app): elfldr downloads the release and starts it; Send a payload streams any ELF. */
function InstallPs5({ devices }: { devices: ConnectDevice[] }) {
  const t = useMessages().connect;
  const [address, setAddress] = useState("");
  const typed = useRef(false);
  const saved = ps5Host(devices);
  useEffect(() => {
    if (!typed.current && saved) setAddress(saved);
  }, [saved]);
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);
  const host = address.trim();
  const run = async (work: () => Promise<string | null>) => {
    if (host === "" || running.current) return;
    running.current = true;
    setDone(null);
    setError(null);
    try {
      setDone(await work());
    } catch (e) {
      setError(errorText(e));
    } finally {
      running.current = false;
      setBusy(null);
    }
  };
  const install = () =>
    run(async () => {
      setBusy(t.findingRelease);
      const current = (await api.updateTarget().catch(() => null))?.version ?? "";
      const v = await ps5Release(current);
      if (!v) throw t.noRelease;
      setBusy(t.installing(v, host));
      await connect.payloadInstall(host, v);
      return t.installed(v, host);
    });
  const send = () =>
    run(async () => {
      // Any name: payloads come as .elf, .bin or none; the backend checks the ELF header.
      const path = await pick({ directory: false, title: t.pickPayload });
      if (!path) return null;
      const name = basename(path);
      setBusy(t.sendingPayload(name, host));
      const bytes = await connect.payloadSend(host, path);
      return t.sentPayload(name, prettyBytes(bytes), host);
    });
  return (
    <section className="card connect-install" aria-labelledby="connect-install">
      <CardHead icon="power" title={t.installTitle} id="connect-install" />
      <form
        className="add-form"
        onSubmit={(e) => {
          e.preventDefault();
          void install();
        }}
      >
        {/* Read-only, not disabled, while it works: focus stays in the box. */}
        <input
          className="mono"
          dir="ltr"
          value={address}
          readOnly={!!busy}
          placeholder="192.168.1.20"
          onChange={(e) => {
            typed.current = true;
            setAddress(e.target.value);
          }}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          aria-label={t.ps5Address}
          aria-describedby="connect-install-hint"
        />
        <button type="submit" className="primary" disabled={host === ""} aria-disabled={!!busy || undefined}>
          <Icon name="download" />
          {t.install}
        </button>
      </form>
      <p id="connect-install-hint" className="hint muted add-hint">
        {t.installHint}
      </p>
      {busy ? (
        <p className="note-line add-status" role="status">
          <span className="spinner" aria-hidden="true" />
          <span>
            {busy}
            <Dots />
          </span>
        </p>
      ) : error ? (
        <ErrorLine text={error} />
      ) : (
        done && (
          <p className="note-line good add-status" role="status">
            <Icon name="checkCircle" />
            <span>{done}</span>
          </p>
        )
      )}
      <div className="add-manual">
        <span>{t.otherPayload}</span>
        <button type="button" className="small" disabled={host === ""} aria-disabled={!!busy || undefined} onClick={() => void send()}>
          <Icon name="file" />
          {t.sendPayload}
        </button>
      </div>
    </section>
  );
}

function AddDevice({ ready }: { ready: boolean }) {
  const t = useMessages().connect;
  const out = useConnectState().state?.outgoing ?? null;
  const [address, setAddress] = useState("");
  const [probing, setProbing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Set at once, so a second Enter before the next render can't start another probe.
  const busy = useRef(false);
  // The request before ours (`undefined`: none); the typed address stays until it pairs, so a refused one can be sent again.
  const sent = useRef<number | null | undefined>(undefined);
  useEffect(() => {
    if (sent.current === undefined || !out || out.id === sent.current || out.state === "asking" || out.state === "waiting") return;
    if (out.state === "paired") setAddress("");
    sent.current = undefined;
  }, [out]);
  const add = async () => {
    const typed = address.trim();
    if (typed === "" || busy.current || !ready) return;
    busy.current = true;
    setError(null);
    setProbing(typed);
    try {
      const p = await connect.probe(typed);
      if (p.kind === "forge") {
        const before = out?.id ?? null;
        if (await startPair(p.host.includes(":") ? `[${p.host}]:${p.port}` : `${p.host}:${p.port}`, before)) sent.current = before;
      } else {
        openNas({ protocol: p.kind, host: p.host, port: p.port, name: p.name ?? p.host });
        setAddress("");
      }
    } catch (e) {
      setError(connectError(e));
    } finally {
      busy.current = false;
      setProbing(null);
    }
  };
  return (
    <section className="card" aria-labelledby="connect-add">
      <CardHead icon="plus" title={t.addDevice} id="connect-add" />
      <form
        className="add-form"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        {/* Read-only, not disabled, while it checks: focus stays in the box. */}
        <input
          className="mono"
          dir="ltr"
          value={address}
          readOnly={!ready || !!probing}
          placeholder={t.addressExample}
          onChange={(e) => setAddress(e.target.value)}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          aria-label={t.address}
        />
        <button
          type="submit"
          className={address.trim() === "" ? undefined : "primary"}
          disabled={!ready || address.trim() === ""}
          aria-disabled={!!probing || undefined}
        >
          {t.connectTo}
        </button>
      </form>
      {/* The check's status or its error, under the box. */}
      {probing ? (
        <p className="note-line add-status" role="status">
          <span className="spinner" aria-hidden="true" />
          <span>
            {t.probing(probing)}
            <Dots />
          </span>
        </p>
      ) : (
        error && <ErrorLine text={error} />
      )}
      <div className="add-manual">
        <span>{t.addByProtocol}</span>
        {(["smb", "ftp", "nfs"] as const).map((p) => (
          <button key={p} type="button" className="small" disabled={!ready} aria-haspopup="dialog" onClick={() => openNas(p)}>
            {nasKind(p)}
          </button>
        ))}
      </div>
    </section>
  );
}

/** Autodiscovery's finds not saved here yet; shown while a search runs, when it found something, or after one this tab asked for. */
function Nearby(props: { state: ConnectState | null; asked: boolean; error: string | null }) {
  const t = useMessages().connect;
  const { state } = props;
  const [failed, setFailed] = useState<string | null>(null);
  const found = state?.discovery;
  const searching = found?.searching ?? false;
  useEffect(() => {
    if (searching) setFailed(null);
  }, [searching]);
  if (!state || !found) return null;
  const saved = new Set(state.nas.map((n) => `${n.protocol}:${n.host.toLowerCase()}:${n.port}`));
  const forge = found.forge.filter((f) => !f.paired);
  const nas = found.nas.filter(
    (n) => (n.service === "smb" || n.service === "ftp" || n.service === "nfs") && !saved.has(`${n.service}:${n.address.toLowerCase()}:${n.port}`),
  );
  const problem = failed ?? props.error ?? found.error;
  if (!found.searching && !props.asked && !problem && forge.length + nas.length === 0) return null;
  const stop = async () => {
    setFailed(null);
    try {
      await connect.discoverStop();
      await refreshConnect();
    } catch (e) {
      setFailed(connectError(e));
    }
  };
  const pair = async (address: string) => {
    setFailed(null);
    try {
      await startPair(address, state.outgoing?.id ?? null);
    } catch (e) {
      setFailed(connectError(e));
    }
  };
  return (
    <section className="nearby" aria-labelledby="connect-nearby">
      <div className="section-title">
        <h3 id="connect-nearby" className="grow">
          <Icon name="search" />
          {t.nearby}
        </h3>
        {found.searching && (
          <>
            <span className="searching" role="status">
              <span className="spinner" aria-hidden="true" />
              <span>
                {t.searching}
                <Dots />
              </span>
            </span>
            <button type="button" className="small" onClick={() => void stop()}>
              <Icon name="x" />
              {t.stop}
            </button>
          </>
        )}
      </div>
      {problem && <ErrorLine text={problem} />}
      {forge.length + nas.length > 0 ? (
        <ul className="dev-list">
          {forge.map((f) => (
            <li key={f.key_id} className="dev-row">
              <OnlineDot online />
              <div className="dev-text">
                <p className="dev-title">
                  <span dir="auto">{f.name}</span> <span className="tag">{t.forge}</span>
                </p>
                <p className="dev-sub">
                  <span>{platformName(f.platform)}</span>
                  <span className="mono" dir="ltr">
                    {f.address}
                  </span>
                </p>
              </div>
              <div className="dev-actions">
                <button type="button" className="small" onClick={() => void pair(f.address)}>
                  <Icon name="lock" />
                  {t.pair}
                </button>
              </div>
            </li>
          ))}
          {nas.map((n) => (
            <li key={`${n.service}:${n.address}:${n.port}`} className="dev-row">
              <OnlineDot online />
              <div className="dev-text">
                <p className="dev-title">
                  <span dir="auto">{n.name}</span> <span className="tag">{nasKind(n.service)}</span>
                </p>
                <p className="dev-sub">
                  <span className="mono" dir="ltr">
                    {hostPort(n.address, n.port, n.service as NasProtocol)}
                  </span>
                </p>
              </div>
              <div className="dev-actions">
                <button
                  type="button"
                  className="small"
                  aria-haspopup="dialog"
                  onClick={() => openNas({ name: n.name, host: n.address, port: n.port, protocol: n.service as NasProtocol })}
                >
                  <Icon name="plus" />
                  {t.save}
                </button>
              </div>
            </li>
          ))}
        </ul>
      ) : (
        !found.searching && <p className="muted note">{t.nearbyNone}</p>
      )}
    </section>
  );
}

// ---- Dialogs ----

function openDevice(keyId: string, onTransfer: (root: string) => void) {
  openOverlay((close) => <DeviceDetails keyId={keyId} onTransfer={onTransfer} close={close} />);
}

function DeviceDetails({ keyId, onTransfer, close }: { keyId: string; onTransfer: (root: string) => void; close: () => void }) {
  const t = useMessages().connect;
  const { state } = useConnectState();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const kept = useRef<ConnectDevice | null>(null);
  // The last known row stays while a poll fails, so the dialog stays put.
  const d = state?.devices.find((x) => x.key_id === keyId) ?? (state ? null : kept.current);
  kept.current = d;
  useEffect(() => {
    if (state && !d) close();
  }, [state, d, close]);
  const forget = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await connect.forget(keyId);
      await refreshConnect();
      close();
    } catch (e) {
      setError(connectError(e));
      setBusy(false);
    }
  };
  const rows: [string, ReactNode][] = d
    ? [
        [t.address, d.address ?? t.unknown],
        [t.keyId, d.key_id],
        [t.lastSeen, lastSeen(d.last_seen)],
        [t.status, d.online ? t.online : t.offline],
      ]
    : [];
  return (
    <Modal close={close} className="modal card device-modal" labelledBy="device-title">
      <Head id="device-title" icon="drive" title={d ? `${d.name} · ${platformName(d.platform)}` : t.unknown} />
      {d && (
        <dl className="tiles device-facts">
          {rows.map(([label, value], i) => (
            <div className={i < 2 ? "tile wide" : "tile"} key={label}>
              <dt>{label}</dt>
              <dd className={i < 2 && value !== t.unknown ? "mono" : undefined} dir={i < 2 && value !== t.unknown ? "ltr" : "auto"}>
                {i === 3 && <OnlineDot online={d.online} />}
                {value}
              </dd>
            </div>
          ))}
        </dl>
      )}
      <SpeedLine keyId={keyId} />
      <p className="muted">{t.forgetNote}</p>
      {error && <ErrorLine text={error} />}
      <div className="row end">
        <SpeedButton keyId={keyId} name={d?.name ?? keyId} disabled={!d || !d.online || !state} />
        <button
          type="button"
          disabled={!d || !d.online}
          onClick={() => {
            close();
            onTransfer(`flink://${keyId}`);
          }}
        >
          <Icon name="move" />
          {t.fileTransfer}
        </button>
        <button type="button" className="danger" disabled={!d} aria-disabled={busy || undefined} onClick={forget}>
          <Icon name="trash" />
          {t.forget}
        </button>
        <button type="button" autoFocus onClick={close}>
          {t.close}
        </button>
      </div>
    </Modal>
  );
}

const spaced = (check: string) => `${check.slice(0, 3)} ${check.slice(3)}`;

/** One request at a time: a double click, or a probe ending after Nearby's Pair, can't send a second. */
let pairing = false;

/** `before`: the request before this one, so the dialog follows only the new one; false while another's dialog is open. */
async function startPair(address: string, before: number | null): Promise<boolean> {
  if (pairing) return false;
  pairing = true;
  try {
    await connect.pair(address);
  } catch (e) {
    pairing = false;
    throw e;
  }
  await refreshConnect();
  openOverlay((close) => <PairWait before={before} close={close} />);
  return true;
}

function PairWait({ before, close }: { before: number | null; close: () => void }) {
  const t = useMessages().connect;
  const { state } = useConnectState();
  const kept = useRef<PairOutgoing | null>(null);
  const out = state?.outgoing ?? null;
  const mine = state ? (out && out.id !== before ? out : null) : kept.current;
  kept.current = mine;
  const running = !mine || mine.state === "asking" || mine.state === "waiting";
  useEffect(() => {
    if (mine?.state === "paired" || mine?.state === "cancelled") close();
  }, [mine?.state, close]);
  const outcome = !running && mine.state !== "paired" && mine.state !== "cancelled" ? connectError(mine.state === "failed" ? (mine.error ?? "") : mine.state) : null;
  const waiting = useRef(running);
  waiting.current = running;
  const mounted = useRef(false);
  // Closing while it waits cancels the request.
  const dismiss = () => {
    if (waiting.current) void connect.pairCancel().then(refreshConnect, () => {});
    waiting.current = false;
    close();
  };
  // Closed from outside too (`closeOverlays`): taken back a tick later, so StrictMode's dev remount doesn't.
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      setTimeout(() => {
        if (mounted.current) return;
        pairing = false;
        if (waiting.current) void connect.pairCancel().then(refreshConnect, () => {});
      }, 0);
    };
  }, []);
  return (
    <Modal close={dismiss} className="modal card pair-modal" labelledBy="pair-title">
      <Head id="pair-title" icon="lock" title={t.pairTitle} />
      {outcome ? (
        <div className="stack">
          <ErrorLine text={outcome} />
          <div className="row end">
            <button type="button" autoFocus onClick={close}>
              {t.close}
            </button>
          </div>
        </div>
      ) : (
        <div className="stack" role="status">
          <p className="note-line">
            <span className="spinner" aria-hidden="true" />
            <span>
              {mine ? t.waitingFor(mine.address) : t.pairing}
              <Dots />
            </span>
          </p>
          {mine?.check && (
            <div className="tile pair-check">
              <span className="muted">{t.checkNumber}</span>
              <b className="connect-code" dir="ltr">
                {spaced(mine.check)}
              </b>
            </div>
          )}
          <p className="muted note">{t.checkHint}</p>
          <div className="row end">
            <button type="button" autoFocus onClick={dismiss}>
              {t.cancel}
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}

/** Over any screen and dialog, rendered into `body` outside `#root` (which an open dialog makes inert); closing it denies. */
export function PairPrompt() {
  const t = useMessages().connect;
  const { state } = useConnectState();
  // The last request shown stays while a poll fails, at most its 60 s, and not once answered.
  const kept = useRef<{ req: PairRequest; at: number } | null>(null);
  const shown = kept.current && Date.now() - kept.current.at < 60_000 ? kept.current.req : null;
  const req = state ? (state.pending[0] ?? null) : shown;
  if (state) kept.current = req ? { req, at: kept.current?.req.id === req.id ? kept.current.at : Date.now() } : null;
  const [busy, setBusy] = useState(false);
  // One answer per request: an Escape after Allow must not send a Deny that could land first.
  const answering = useRef(false);
  // Focus as the prompt came, given back when the last request goes (it isn't an `openOverlay`).
  const back = useRef<HTMLElement | null>(null);
  if (req && !back.current) back.current = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
  const open = req !== null;
  useEffect(() => {
    if (open || !back.current) return;
    const el = back.current;
    back.current = null;
    if (el.isConnected) el.focus();
  }, [open]);
  if (!req) return null;
  const answer = async (allow: boolean) => {
    if (answering.current) return;
    answering.current = true;
    setBusy(true);
    try {
      await connect.approve(req.id, allow);
    } catch {
    }
    kept.current = null;
    await refreshConnect();
    answering.current = false;
    setBusy(false);
  };
  return createPortal(
    // A fresh dialog per request: the next device's mounts with focus on Deny and is announced.
    <div className="pair-layer" key={req.id}>
      <Modal close={() => void answer(false)} className="modal card pair-modal" role="alertdialog" labelledBy="pair-ask-title">
        <Head id="pair-ask-title" icon="lock" title={t.pairPromptTitle(req.name)} />
        <p dir="auto">{t.pairPromptBody(req.name, platformName(req.platform), req.ip)}</p>
        <div className="tile pair-check">
          <span className="muted">{t.checkNumber}</span>
          <b className="connect-code" dir="ltr">
            {spaced(req.check)}
          </b>
        </div>
        <p className="muted note">{t.pairPromptCheck(req.name)}</p>
        <div className="row end">
          <button type="button" autoFocus aria-disabled={busy || undefined} onClick={() => void answer(false)}>
            {t.deny}
          </button>
          <button type="button" className="primary" aria-disabled={busy || undefined} onClick={() => void answer(true)}>
            {t.allow}
          </button>
        </div>
      </Modal>
    </div>,
    document.body,
  );
}

function nasKind(protocol: string): string {
  return protocol.toUpperCase();
}

function openNasDetails(id: string) {
  openOverlay((close) => <NasDetails id={id} close={close} />);
}

function NasDetails({ id, close }: { id: string; close: () => void }) {
  const t = useMessages().connect;
  const { state } = useConnectState();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const kept = useRef<NasDevice | null>(null);
  const n = state?.nas.find((x) => x.id === id) ?? (state ? null : kept.current);
  kept.current = n;
  useEffect(() => {
    if (state && !n) close();
  }, [state, n, close]);
  const forget = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await connect.nasForget(id);
      await refreshConnect();
      close();
    } catch (e) {
      setError(connectError(e));
      setBusy(false);
    }
  };
  const rows: [string, string, boolean][] = n
    ? [
        [t.address, hostPort(n.host, n.port, n.protocol), true],
        ...(n.protocol === "nfs"
          ? []
          : ([
              [t.nasUser, n.user || t.guest, n.user !== ""],
              [t.nasPassword, n.has_password ? t.passwordSaved : t.passwordNone, false],
            ] as [string, string, boolean][])),
        [t.status, n.online ? t.online : t.offline, false],
      ]
    : [];
  return (
    <Modal close={close} className="modal card device-modal" labelledBy="nas-title">
      <Head id="nas-title" icon="drive" title={n ? `${n.name} · ${t.kindNas}` : t.unknown} />
      {n && (
        <dl className="tiles device-facts">
          {rows.map(([label, value, mono], i) => (
            <div className={i === 0 ? "tile wide" : "tile"} key={label}>
              <dt>{label}</dt>
              <dd className={mono ? "mono" : undefined} dir={mono ? "ltr" : "auto"}>
                {label === t.status && <OnlineDot online={n.online} />}
                {value}
              </dd>
            </div>
          ))}
        </dl>
      )}
      <p className="muted">{t.nasForgetNote}</p>
      {error && <ErrorLine text={error} />}
      <div className="row end">
        <button
          type="button"
          disabled={!n}
          onClick={() => {
            close();
            if (n) openNas(n);
          }}
        >
          <Icon name="pencil" />
          {t.edit}
        </button>
        <button type="button" className="danger" disabled={!n} aria-disabled={busy || undefined} onClick={forget}>
          <Icon name="trash" />
          {t.forget}
        </button>
        <button type="button" autoFocus onClick={close}>
          {t.close}
        </button>
      </div>
    </Modal>
  );
}

type Found = { name: string; host: string; port: number; protocol: NasProtocol };

/** Prefilled from Nearby or a probe (`Found`), or edit a saved one (its protocol kept). */
function openNas(from: NasDevice | Found | NasProtocol) {
  openOverlay((close) => <NasDialog from={from} close={close} />);
}

function NasDialog(props: { from: NasDevice | Found | NasProtocol; close: () => void }) {
  const t = useMessages().connect;
  const f = props.from;
  const n = typeof f !== "string" && "id" in f ? f : null;
  const found = typeof f !== "string" && !("id" in f) ? f : null;
  const from = n ?? found;
  const protocol: NasProtocol = typeof f === "string" ? f : f.protocol;
  const [name, setName] = useState(from?.name ?? "");
  const [address, setAddress] = useState(from ? hostPort(from.host, from.port, from.protocol) : "");
  const ftp = protocol === "ftp";
  const nfs = protocol === "nfs";
  const [user, setUser] = useState(n?.user ?? "");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ok = address.trim() !== "";
  const save = async () => {
    if (!ok || busy) return;
    setBusy(true);
    setError(null);
    try {
      // An empty password keeps the saved one, except for a guest (no user), who has none; NFS sends none.
      const u = user.trim();
      const login = nfs ? {} : { user: u, password: password !== "" || u === "" ? password : undefined };
      await connect.nasSave({ id: n?.id, name: name.trim(), host: address.trim(), protocol, ...login });
      await refreshConnect();
      props.close();
    } catch (e) {
      setError(connectError(e));
      setBusy(false);
    }
  };
  // The hint is outside the label (not the input's name); read-only, not disabled, while saving, so focus stays and Escape reaches the dialog.
  const field = (id: string, label: string, value: string, set: (v: string) => void, extra: { hint?: string; type?: string; ltr?: boolean; focus?: boolean }) => (
    <div className="field-label">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        dir={extra.ltr ? "ltr" : "auto"}
        type={extra.type ?? "text"}
        autoFocus={extra.focus}
        value={value}
        readOnly={busy}
        aria-describedby={extra.hint ? `${id}-hint` : undefined}
        onChange={(e) => set(e.target.value)}
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        autoComplete={extra.type === "password" ? "new-password" : "off"}
      />
      {extra.hint && (
        <span id={`${id}-hint`} className="muted note">
          {extra.hint}
        </span>
      )}
    </div>
  );
  return (
    <Modal close={props.close} className="modal card nas-modal" labelledBy="nas-edit-title">
      <Head id="nas-edit-title" icon="drive" title={n ? t.nasEditTitle(protocol.toUpperCase()) : t.nasAddTitle(protocol.toUpperCase())} />
      <form
        className="nas-form"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        {ftp && <p className="muted note">{t.ftpNote}</p>}
        {nfs && <p className="muted note">{t.nfsNote}</p>}
        {field("nas-address", t.address, address, setAddress, { hint: ftp ? t.ftpAddressHint : nfs ? t.nfsAddressHint : t.nasAddressHint, ltr: true, focus: !from })}
        {field("nas-name", t.nasName, name, setName, { focus: !!n || (nfs && !!found) })}
        {!nfs && field("nas-user", t.nasUser, user, setUser, { hint: ftp ? t.ftpUserHint : t.nasUserHint, ltr: true, focus: !!found })}
        {!nfs &&
          field("nas-password", t.nasPassword, password, setPassword, { hint: n?.has_password ? t.nasPasswordKeep : undefined, type: "password", ltr: true })}
        {error && <ErrorLine text={error} />}
        <div className="row end">
          <button type="button" onClick={props.close}>
            {t.cancel}
          </button>
          <button type="submit" className="primary" disabled={!ok} aria-disabled={busy || undefined}>
            {t.save}
          </button>
        </div>
      </form>
    </Modal>
  );
}
