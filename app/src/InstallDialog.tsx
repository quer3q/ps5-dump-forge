// The one "Install this package?" question, shared by the Package installer, the File
// explorer's Install and Connect's Install pkg, and the install job it starts on this device.

import { api, prettyBytes, type ConvertRequest, type PackageContent } from "./api";
import { Modal, openOverlay } from "./common";
import { gameTitle, useMessages } from "./i18n";
import { Icon } from "./icons";
import type { Action } from "./jobs";
import { readable } from "./peers";

/** What the question shows; any fact left out is not shown (a File explorer row knows only
 * its name and size, Connect's pick only its path). */
export interface InstallInfo {
  path: string;
  size?: number | null;
  title_name?: string | null;
  title_names?: Record<string, string>;
  title_id?: string | null;
  version?: string | null;
  content?: PackageContent;
  /** Another device installs it (Connect's Install pkg): its name. */
  device?: string;
}

/** Asks before installing; true: Install. Cancel is first and focused. */
export function confirmInstall(info: InstallInfo): Promise<boolean> {
  return new Promise((resolve) => {
    let yes = false;
    openOverlay(
      (close) => (
        <InstallDialog
          info={info}
          close={close}
          onYes={() => {
            yes = true;
            close();
          }}
        />
      ),
      () => resolve(yes),
    );
  });
}

/** An install job's request (core ignores `source`, `output` and `format` for it). */
export function installRequest(path: string): ConvertRequest {
  return {
    source: "",
    format: "folder",
    output: "",
    compression_threads: null,
    inner: null,
    remove_backport: false,
    full_verify: false,
    install: path,
  };
}

/** Asks, then starts an install job of `info.path` on this device (the PS5); false: cancelled.
 * Rejects with the start's error. */
export async function installHere(info: InstallInfo, dispatch: (a: Action) => void): Promise<boolean> {
  if (!(await confirmInstall(info))) return false;
  const request = installRequest(info.path);
  const id = await api.startJob(request);
  dispatch({ type: "started", id, request });
  return true;
}

function InstallDialog(props: { info: InstallInfo; close: () => void; onYes: () => void }) {
  const t = useMessages().install;
  const i = props.info;
  const title = gameTitle(i.title_names, i.title_name ?? null);
  // [label, value, a path]: only the facts known.
  const known: [string, string | null | undefined, boolean][] = [
    [t.file, readable(i.path), true],
    [t.game, title, false],
    [t.titleId, i.title_id, false],
    [t.version, i.version, false],
    [t.size, typeof i.size === "number" ? prettyBytes(i.size) : null, false],
  ];
  const facts = known.filter((f): f is [string, string, boolean] => !!f[1]);
  // A patch or an add-on installs beside the game; a game (or a package Forge can't read) may
  // replace the one installed.
  const reinstall = i.content !== "patch" && i.content !== "additional_content";
  return (
    <Modal close={props.close} className="modal card" role="alertdialog" labelledBy="inst-title" describedBy="inst-text">
      <div className="modal-head">
        <span className="head-icon">
          <Icon name="package" />
        </span>
        <h2 id="inst-title" dir="auto">
          {i.device ? t.titleOn(i.device) : t.title}
        </h2>
      </div>
      <dl className="tiles device-facts">
        {facts.map(([label, value, path]) => (
          <div className={path || label === t.game ? "tile wide" : "tile"} key={label}>
            <dt>{label}</dt>
            <dd className={path ? "mono" : undefined} dir={path ? "ltr" : "auto"}>
              {value}
            </dd>
          </div>
        ))}
      </dl>
      <div id="inst-text">
        <p className="muted">{t.text}</p>
        {reinstall && (
          <p className="note-line warn">
            <Icon name="warn" />
            <span>{t.reinstall}</span>
          </p>
        )}
      </div>
      <div className="row end">
        <button autoFocus onClick={props.close}>
          {t.cancel}
        </button>
        <button className="primary" onClick={props.onYes}>
          <Icon name="download" />
          {t.confirm}
        </button>
      </div>
    </Modal>
  );
}
