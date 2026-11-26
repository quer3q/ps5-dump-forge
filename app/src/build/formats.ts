import type { Format } from "../api";

export const TARGETS: Format[] = ["folder", "pkg", "exfat", "ffpkg", "ffpfsc", "ffpfs"];
/** Additional content (no eboot.bin): core takes it into a folder or an fpkg only. */
export const AC_TARGETS: Format[] = ["folder", "pkg"];

export type Engine = "zstd" | "lz4";

/** The levels, in picker order, with each kind's number (zstd 1..22; LZ4 0 or HC 3..12). */
export type PackLevel = "fast" | "medium" | "max";
export const LEVEL_OF: Record<Engine, Record<PackLevel, number>> = {
  zstd: { fast: 5, medium: 9, max: 19 },
  lz4: { fast: 0, medium: 9, max: 12 },
};

/** Pack targets (`lz4`: LZ4's packed folder). No .ffpfsc here (core and the CLI still take it). */
export const ZSTD_PACK_FORMATS: Format[] = ["folder", "exfat", "ffpkg", "ffpfs"];
export const LZ4_PACK_FORMATS: Format[] = ["lz4", "exfat", "ffpkg", "ffpfs"];
