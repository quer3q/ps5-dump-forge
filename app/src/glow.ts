// The game card's glow, tinted after its cover: the cover's one or two most saturated hues,
// dimmed until the card's brightest point keeps every text on it >= 4.5:1. Computed once per
// cover (a 24x24 canvas) and cached for the session.

import { useEffect, useReducer, useRef, type CSSProperties } from "react";

type Rgb = [number, number, number];

const CARD: Rgb = [0x17, 0x17, 0x1f]; // --card
// The brightest the glow may make the card: muted text (#9a9aae, the faintest text on it)
// stays >= 4.9:1 there, and the tinted tags >= 5:1. The brand glow peaks at 0.0248.
const MAX_L = 0.028;
const SIZE = 24;
const BINS = 12; // hue buckets, 30 degrees each

const cache = new Map<string, CSSProperties | null>();

export interface Glow {
  /** `--glow-1` / `--glow-2`; undefined: the brand colours. */
  style?: CSSProperties;
  /** False while the cover is unknown or still being read: the glow stays faded out. */
  ready: boolean;
}

/**
 * `cover`: a data: URL, null for no cover (brand glow), undefined while the source is still
 * being read. While not ready the last colours are kept, so the glow fades out in them
 * instead of flashing the brand colours.
 */
export function useCoverGlow(cover: string | null | undefined): Glow {
  const [, bump] = useReducer((n: number) => n + 1, 0);
  const last = useRef<CSSProperties | undefined>(undefined);
  useEffect(() => {
    if (!cover || cache.has(cover)) return;
    let live = true;
    void coverGlow(cover).then((g) => {
      cache.set(cover, g);
      if (live) bump();
    });
    return () => {
      live = false;
    };
  }, [cover]);
  if (cover === undefined || (cover !== null && !cache.has(cover))) {
    return { style: last.current, ready: false };
  }
  last.current = cover === null ? undefined : (cache.get(cover) ?? undefined);
  return { style: last.current, ready: true };
}

async function coverGlow(src: string): Promise<CSSProperties | null> {
  try {
    const img = new Image();
    img.src = src;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = SIZE;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(img, 0, 0, SIZE, SIZE);
    const hues = dominant(ctx.getImageData(0, 0, SIZE, SIZE).data);
    if (!hues) return null;
    let [a1, a2] = [0.18, 0.11];
    const [c1, c2] = hues.map(tame);
    // Both glows overlap at the corner: dim them until that point is no brighter than the
    // brand glow's.
    while (luminance(over(over(CARD, c1, a1), c2, a2)) > MAX_L) [a1, a2] = [a1 * 0.92, a2 * 0.92];
    return { "--glow-1": rgba(c1, a1), "--glow-2": rgba(c2, a2) } as CSSProperties;
  } catch {
    return null;
  }
}

/** The two most weighted saturated hues (the second at least 60 degrees from the first, else
 * the first again); the average colour when nothing is saturated; null when that is grey. */
function dominant(px: Uint8ClampedArray): [Rgb, Rgb] | null {
  const bins = Array.from({ length: BINS }, (_, i) => ({ i, w: 0, r: 0, g: 0, b: 0 }));
  const sum = [0, 0, 0];
  let n = 0;
  for (let p = 0; p < px.length; p += 4) {
    if (px[p + 3] < 128) continue;
    const c: Rgb = [px[p], px[p + 1], px[p + 2]];
    sum[0] += c[0];
    sum[1] += c[1];
    sum[2] += c[2];
    n++;
    const [h, s, l] = toHsl(c);
    if (s < 0.25 || l < 0.12 || l > 0.9) continue; // grey, near-black, near-white
    const w = s * (1 - Math.abs(2 * l - 1));
    const bin = bins[Math.floor(h * BINS) % BINS];
    bin.w += w;
    bin.r += c[0] * w;
    bin.g += c[1] * w;
    bin.b += c[2] * w;
  }
  if (n === 0) return null;
  const ranked = bins.filter((b) => b.w > 0).sort((x, y) => y.w - x.w);
  if (ranked.length === 0) {
    const avg: Rgb = [sum[0] / n, sum[1] / n, sum[2] / n];
    return toHsl(avg)[1] < 0.12 ? null : [avg, avg];
  }
  const first = ranked[0];
  const far = (i: number) => Math.min(Math.abs(i - first.i), BINS - Math.abs(i - first.i)) >= 2;
  const second = ranked.find((b) => far(b.i) && b.w >= first.w * 0.15) ?? first;
  const mean = (b: typeof first): Rgb => [b.r / b.w, b.g / b.w, b.b / b.w];
  return [mean(first), mean(second)];
}

/** A glow colour: vivid but not neon, mid lightness. */
function tame(c: Rgb): Rgb {
  const [h, s, l] = toHsl(c);
  return fromHsl(h, Math.min(0.95, Math.max(0.6, s)), Math.min(0.58, Math.max(0.45, l)));
}

function over(bg: Rgb, c: Rgb, a: number): Rgb {
  return [0, 1, 2].map((i) => bg[i] * (1 - a) + c[i] * a) as Rgb;
}

function luminance(c: Rgb): number {
  const lin = c.map((v) => {
    const x = v / 255;
    return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

function rgba(c: Rgb, a: number): string {
  return `rgba(${c.map(Math.round).join(", ")}, ${a.toFixed(3)})`;
}

function toHsl([r, g, b]: Rgb): [number, number, number] {
  const [x, y, z] = [r / 255, g / 255, b / 255];
  const max = Math.max(x, y, z);
  const min = Math.min(x, y, z);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === x ? ((y - z) / d + 6) % 6 : max === y ? (z - x) / d + 2 : (x - y) / d + 4;
  return [h / 6, s, l];
}

function fromHsl(h: number, s: number, l: number): Rgb {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = h * 6;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  const [r, g, b] =
    hp < 1 ? [c, x, 0] : hp < 2 ? [x, c, 0] : hp < 3 ? [0, c, x] : hp < 4 ? [0, x, c] : hp < 5 ? [x, 0, c] : [c, 0, x];
  const m = l - c / 2;
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}
