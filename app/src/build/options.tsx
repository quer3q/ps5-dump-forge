// Build's options step: Restore metadata online, Delete the source, full verification.

import { useId } from "react";

import { isNas } from "../common";
import { msg, useMessages } from "../i18n";
import { Advanced } from "../prefs";
import { Toggle } from "./fields";
import { defineStep, type DeriveCtx } from "./steps";

export interface OptionsState {
  fullVerify: boolean;
  deleteSource: boolean;
  /** Off by default: the only switch that goes online. */
  restoreMetadata: boolean;
}

/** A NAS's game can't be deleted from here; a patch can't be converted at all. */
const deletes = (c: DeriveCtx, s: OptionsState) => s.deleteSource && !!c.source && !isNas(c.source) && !c.ins?.patch;

/** Sony's metadata goes into the output of any source but a patch (which isn't converted). */
const restorable = (c: DeriveCtx) => !!c.ins && !c.ins.patch;
const restores = (c: DeriveCtx, s: OptionsState) => s.restoreMetadata && restorable(c);

/** Toggle with a "Recommended" notice at the row's end (the Target chips' green dot and its word). */
function RecToggle(props: { label: string; checked: boolean; onChange: (on: boolean) => void; tip: string }) {
  const id = useId();
  return (
    <div className="field verify b-has-rec" title={props.tip}>
      <label className="switch">
        <input
          type="checkbox"
          className="visually-hidden"
          checked={props.checked}
          aria-describedby={id}
          onChange={(e) => props.onChange(e.target.checked)}
        />
        <span className="switch-track" aria-hidden="true" />
        <span>{props.label}</span>
      </label>
      <span className="b-rec" id={id}>
        <span className="b-best" aria-hidden="true" />
        {useMessages().build.recommended}
      </span>
    </div>
  );
}

export const optionsStep = defineStep<OptionsState>({
  id: "options",
  title: () => msg().build.options,
  initial: { fullVerify: false, deleteSource: false, restoreMetadata: false },
  available: (c) => c.ins !== null,
  Body({ ctx, state, set }) {
    const t = useMessages().target;
    const bm = useMessages().build;
    const mm = useMessages().metadata;
    const nas = isNas(ctx.source);
    return (
      <>
        {restorable(ctx) && (
          <RecToggle label={mm.label} checked={state.restoreMetadata} onChange={(on) => set({ restoreMetadata: on })} tip={mm.buildHint} />
        )}
        {!ctx.ins?.patch && (
          <Toggle
            className="delete-source"
            label={t.deleteSource}
            checked={state.deleteSource && !nas}
            disabled={nas}
            onChange={(on) => set({ deleteSource: on })}
            tip={nas ? t.deleteSourceNas : t.deleteSourceHint}
          />
        )}
        <Advanced title={bm.advanced} changed={state.fullVerify}>
          <Toggle label={t.fullVerify} checked={state.fullVerify} onChange={(on) => set({ fullVerify: on })} tip={t.fullVerifyHint} />
        </Advanced>
      </>
    );
  },
  request: (c, s, req) => ({
    ...req,
    full_verify: s.fullVerify,
    ...(deletes(c, s) && { delete_source: true }),
    ...(restores(c, s) && { restore_metadata: true }),
  }),
  summary: (c, s) => [
    { text: s.fullVerify ? msg().build.fullVerify : msg().build.fastVerify, tone: "muted" as const },
    ...(restores(c, s) ? [{ text: msg().metadata.buildSummary, tone: "muted" as const }] : []),
    ...(deletes(c, s) ? [{ text: msg().build.deleted, tone: "warn" as const }] : []),
  ],
});
