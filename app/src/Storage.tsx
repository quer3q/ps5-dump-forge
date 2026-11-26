// Storage: the File explorer and, on the PS5, a view switch to the Package installer; views stay mounted.

import { useEffect, useRef, useState } from "react";

import { runtimePlatform, type RuntimePlatform } from "./api";
import { ViewSwitch, type SourceSelection } from "./common";
import { Files } from "./Files";
import { useMessages } from "./i18n";
import type { Action } from "./jobs";
import { Packages } from "./Packages";

type View = "files" | "packages";

export type HandOff = (e: { path: string; kind: SourceSelection["kind"] }) => void;

export function Storage(props: {
  visible: boolean;
  onEdit: HandOff;
  onConvert: HandOff;
  dispatch: (a: Action) => void;
  onConnect?: () => void;
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
  const screen = useRef<HTMLDivElement>(null);
  // Each view head has its own switch copy: focus on the hidden one moves to the shown copy's checked option.
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
  // One switch, first in the shown view's card head; only with two views (the PS5).
  const switcher = ps5 && (
    <ViewSwitch
      label={t.views}
      value={view}
      onChange={pick}
      options={[
        // Under 400px the labels hide (still the options' names): fr/es/pt overflow a phone's card.
        { id: "files", icon: "folder", label: <span className="view-label">{t.files}</span> },
        { id: "packages", icon: "package", label: <span className="view-label">{packages}</span> },
      ]}
    />
  );
  return (
    <div className="screen storage" ref={screen}>
      <div hidden={view !== "files"}>
        <Files
          visible={props.visible && view === "files"}
          dispatch={props.dispatch}
          onConnect={props.onConnect}
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
