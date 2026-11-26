// The Build screen's step contract: each step owns its state, adds facts, patches the request.

import type { ComponentType, ReactNode } from "react";

import type { ConvertRequest, Format, Inspection, PackKind } from "../api";
import type { Job } from "../jobs";
import type { Output } from "../target";

/** The packs row's chip: the pack format the output gets (none: plain files). */
export type PacksChoice = "none" | "zstd" | "lz4";
/** The packs rule: `keep` a plain request (the game's own packs byte for byte), `unpack` None on a packed game, else pack. */
export type PackMode = "keep" | "unpack" | "zstd" | "lz4";

/** Facts the steps derive from, in fold order: each step's `context` adds to it. */
export interface DeriveCtx {
  source: string | null;
  sourceIsImage: boolean;
  ins: Inspection | null;
  packs: PackMode;
  /** For `unpack`: whose packs are decoded. */
  unpackKind: "zstd" | "lz4" | null;
  /** Packs made by the job (named in a generated output), null for none. */
  packKind: PackKind | null;
  formats: Format[];
  defaultFormat: Format;
  /** A target listed but refused for this source (Convert's folder for a folder source). */
  formatDisabled: (f: Format) => string | undefined;
  /** `lz4`: LZ4's packed folder. */
  format: Format;
  removeBackport: boolean;
}

export interface BuildCtx extends DeriveCtx {
  jobs: Job[];
  out: Output;
}

export interface SummaryLine {
  text: ReactNode;
  tone?: "warn" | "muted";
}

export interface StepProps<S> {
  ctx: BuildCtx;
  state: S;
  set: (patch: Partial<S>) => void;
}

export interface BuildStep<S> {
  id: string;
  /** Read at render time (the current language). */
  title: () => string;
  initial: S;
  /** Shown and folded at all (an unavailable step adds nothing to the request). */
  available(ctx: DeriveCtx): boolean;
  context?(ctx: DeriveCtx, state: S): Partial<DeriveCtx>;
  /** A new source, or the page emptied after a start (sticky choices kept). */
  onSource?(ctx: DeriveCtx, state: S): S;
  Body: ComponentType<StepProps<S>>;
  request(ctx: BuildCtx, state: S, req: ConvertRequest): ConvertRequest;
  summary(ctx: BuildCtx, state: S): SummaryLine[];
  /** Why Build can't start, each a sentence. */
  findings?(ctx: BuildCtx, state: S): string[];
}

export function defineStep<S>(s: BuildStep<S>): BuildStep<S> {
  return s;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyStep = BuildStep<any>;
