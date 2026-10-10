// Plain (server-safe) half of the stats hover card: the Tip shape and the
// data-stip helpers. Server components spread tipProps(); the "use client"
// TipLayer in StatTip.tsx reads it back with parseTip().

export interface Tip {
  /** Big first line, e.g. "Tue 22 Sep". */
  title: string;
  /** Small line under the title, e.g. "your busiest day". */
  sub?: string;
  /** Label / value rows, in order. */
  rows?: [string, string][];
  /** A small meter: value out of max, with an optional caption. */
  bar?: { value: number; max: number; label?: string };
  /** A closing fun/insight sentence. */
  note?: string;
  /** Swatch colour (any CSS colour, use tokens). */
  accent?: string;
}

/** Spread onto any HTML or SVG element to give it a rich tip. */
export function tipProps(t: Tip): { "data-stip": string } {
  return { "data-stip": JSON.stringify(t) };
}

export function parseTip(raw: string | null | undefined): Tip | null {
  if (!raw) return null;
  try {
    const t = JSON.parse(raw) as Tip;
    return t && typeof t.title === "string" ? t : null;
  } catch {
    return null;
  }
}

const GAP = 14;

/** Keep the card inside the viewport: prefer below-right of the point, flip when it would spill. */
export function placeTip(
  x: number,
  y: number,
  w: number,
  h: number,
  vw: number,
  vh: number,
): { left: number; top: number } {
  let left = x + GAP;
  let top = y + GAP;
  if (left + w > vw - 8) left = x - GAP - w;
  if (top + h > vh - 8) top = y - GAP - h;
  return { left: Math.max(8, left), top: Math.max(8, top) };
}
