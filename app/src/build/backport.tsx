// Build's Backport step: Remove backport (fakelib/ libraries left out), a plain conversion only.

import { emulatorNames } from "../common";
import { msg, useMessages } from "../i18n";
import { defineStep, type BuildCtx, type DeriveCtx, type SummaryLine } from "./steps";
import { Toggle } from "./fields";

/** The switch applies: a plain conversion of a game with backport libraries. */
const backportApplies = (c: DeriveCtx) => c.packs === "keep" && !!c.ins && c.ins.backport.length > 0;

/** Removing it is refused when the executables' SDK was lowered (core says why). */
const removable = (c: DeriveCtx) => backportApplies(c) && !c.ins!.backport_blocked;

const removesBackport = (c: DeriveCtx, on: boolean) => removable(c) && on;

function BackportSwitch({ ctx, on, set }: { ctx: DeriveCtx; on: boolean; set: (on: boolean) => void }) {
  const t = useMessages().convert;
  const bm = useMessages().build;
  const ins = ctx.ins!;
  const ok = removable(ctx);
  const emus = emulatorNames(ins);
  const why = ins.backport.length === 0 ? bm.backportNone : backportApplies(ctx) ? null : bm.backportPlain;
  const tip =
    why ??
    (ins.backport_blocked
      ? t.cantRemove(ins.backport_blocked.replace(/^remove backport: /, ""))
      : t.removeHint(ins.backport.length, emus.filter((n) => n !== "Other"), emus.includes("Other")));
  return (
    <>
      <Toggle
        className="backport"
        label={t.removeBackport}
        checked={ok && on}
        disabled={!ok}
        onChange={set}
        tip={typeof tip === "string" ? tip : undefined}
      />
      {why && <p className="muted b-why">{why}</p>}
    </>
  );
}

const backportSummary = (c: BuildCtx): SummaryLine[] =>
  c.removeBackport ? [{ text: msg().build.backportLeftOut(c.ins!.backport.length) }] : [];

/** A packed source's index or manifest lists the backport: core refuses it until unpacked. */
function backportFindings(c: BuildCtx): string[] {
  if (!c.removeBackport) return [];
  if (c.ins?.zstd_packed) return [msg().build.zstdBackport];
  if (c.ins?.lz4?.packed) return [msg().build.lz4Backport];
  return [];
}

export const backportStep = defineStep<{ removeBackport: boolean }>({
  id: "backport",
  title: () => msg().build.backportTitle,
  initial: { removeBackport: false },
  available: (c) => c.ins !== null && !c.ins.patch,
  onSource: () => ({ removeBackport: false }),
  context: (c, s) => ({ removeBackport: removesBackport(c, s.removeBackport) }),
  Body: ({ ctx, state, set }) => <BackportSwitch ctx={ctx} on={state.removeBackport} set={(on) => set({ removeBackport: on })} />,
  request: (c, _s, req) => ({ ...req, remove_backport: c.removeBackport }),
  summary: (c) => backportSummary(c),
  findings: (c) => backportFindings(c),
});
