// The Storage tab: a view switch (Library | File explorer, and on the PS5 Package installer)
// first in the head of the view it picks. The views stay mounted, so a list or a folder
// survives a switch; on a computer Library and File explorer share one chosen folder. The view
// lasts until the app closes.

import { useEffect, useRef, useState } from "react";

import { runtimePlatform, type RuntimePlatform } from "./api";
import { ViewSwitch, type SourceSelection } from "./common";
import { Files } from "./Files";
import { useMessages } from "./i18n";
import type { Action } from "./jobs";
import { Library } from "./Library";
import { Packages } from "./Packages";

type View = "library" | "files" | "packages";

export type HandOff = (e: { path: string; kind: SourceSelection["kind"] }) => void;

export function Storage(props: {
  visible: boolean;
  onEdit: HandOff;
  onConvert: HandOff;
  dispatch: (a: Action) => void;
  /** Opens the Connect tab (the File explorer's started copy). */
  onConnect: () => void;
}) {
  const t = useMessages().storage;
  const packages = useMessages().install.view;
  const [view, setView] = useState<View>("files");
  // The Package installer is the PS5's only (it installs there); null until the backend says.
  const [platform, setPlatform] = useState<RuntimePlatform | null>(null);
  useEffect(() => {
    runtimePlatform().then(setPlatform, () => {});
  }, []);
  const ps5 = platform === "ps5";
  const [folder, setFolder] = useState<string | null>(null);
  const screen = useRef<HTMLDivElement>(null);
  // Each view's head has its own copy of the switch and the old one is hidden on a switch, so
  // focus that was on a switch moves to the shown copy's checked option (keys and clicks alike).
  const refocus = useRef(false);
  const pick = (v: View) => {
    refocus.current = !!document.activeElement?.closest(".view-switch");
    setView(v);
  };
  useEffect(() => {
    if (!refocus.current) return;
    refocus.current = false;
    screen.current?.querySelector<HTMLElement>(":scope > div:not([hidden]) .view-switch [role=radio][aria-checked=true]")?.focus();
  }, [view]);
  // One switch, rendered first in the shown view's card head (each view owns its card).
  const switcher = (
    <ViewSwitch
      label={t.views}
      value={view}
      onChange={pick}
      options={[
        // Under 400px the labels hide (still the options' names): fr/es/pt overflow a phone's card.
        { id: "library", icon: "disc", label: <span className="view-label">{t.library}</span> },
        { id: "files", icon: "folder", label: <span className="view-label">{t.files}</span> },
        ...(ps5 ? [{ id: "packages" as const, icon: "package" as const, label: <span className="view-label">{packages}</span> }] : []),
      ]}
    />
  );
  return (
    <div className="screen storage" ref={screen}>
      <div hidden={view !== "library"}>
        <Library
          visible={props.visible && view === "library"}
          folder={folder}
          onFolder={setFolder}
          onEdit={props.onEdit}
          onConvert={props.onConvert}
          switcher={switcher}
        />
      </div>
      <div hidden={view !== "files"}>
        <Files
          visible={props.visible && view === "files"}
          dispatch={props.dispatch}
          onConnect={props.onConnect}
          folder={folder}
          onFolder={setFolder}
          onEdit={props.onEdit}
          onConvert={props.onConvert}
          switcher={switcher}
        />
      </div>
      {ps5 && (
        <div hidden={view !== "packages"}>
          <Packages visible={props.visible && view === "packages"} dispatch={props.dispatch} switcher={switcher} />
        </div>
      )}
    </div>
  );
}
