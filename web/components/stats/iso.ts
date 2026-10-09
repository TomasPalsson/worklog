// Shared isometric kit for the stats signature graphics. One projection,
// one shading rule, so the city, terrain, warehouse and metro read as one
// set. Grid units: x runs down-right, y runs down-left, z is up.

export const COS30 = Math.cos(Math.PI / 6);
export const SIN30 = 0.5;

/** Project a grid point to screen (unit = px per grid step). */
export function iso(x: number, y: number, z = 0, unit = 10): [number, number] {
  return [(x - y) * COS30 * unit, ((x + y) * SIN30 - z) * unit];
}

const pts = (ps: [number, number][]) => ps.map(([a, b]) => `${a.toFixed(2)},${b.toFixed(2)}`).join(" ");

/** The three visible faces of a box at (x,y,z) sized (w,d,h), as polygon points. */
export function isoBox(x: number, y: number, z: number, w: number, d: number, h: number, unit = 10) {
  const p = (a: number, b: number, c: number) => iso(a, b, c, unit);
  const top = pts([p(x, y, z + h), p(x + w, y, z + h), p(x + w, y + d, z + h), p(x, y + d, z + h)]);
  // left face looks down-left (y = y+d), right face down-right (x = x+w)
  const left = pts([p(x, y + d, z), p(x + w, y + d, z), p(x + w, y + d, z + h), p(x, y + d, z + h)]);
  const right = pts([p(x + w, y, z), p(x + w, y + d, z), p(x + w, y + d, z + h), p(x + w, y, z + h)]);
  return { top, left, right };
}

/**
 * Three-tone shading from one token: top is the colour, the two sides are
 * mixed toward black. Pass a CSS colour or var(--token).
 */
export function tones(c: string) {
  return {
    top: c,
    left: `color-mix(in oklch, ${c} 76%, black)`,
    right: `color-mix(in oklch, ${c} 58%, black)`,
  };
}

/** Painter's order: draw far cells first (smaller x+y). */
export const byDepth = <T extends { x: number; y: number }>(a: T, b: T) => a.x + a.y - (b.x + b.y) || a.x - b.x;
