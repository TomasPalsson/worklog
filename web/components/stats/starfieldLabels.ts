export interface LabelCand {
  id: number;
  x: number;
  y: number;
  w: number;
  h: number;
  priority: number;
  force?: boolean;
}

export interface Placed {
  id: number;
  x: number;
  y: number;
}

interface Box {
  l: number;
  t: number;
  r: number;
  b: number;
}

const boxOf = (c: LabelCand, pad: number): Box => ({ l: c.x - c.w / 2 - pad, t: c.y - c.h / 2 - pad, r: c.x + c.w / 2 + pad, b: c.y + c.h / 2 + pad });
const hit = (a: Box, b: Box) => a.l < b.r && b.l < a.r && a.t < b.b && b.t < a.b;

// Candidates are (x, y)-centred boxes. A forced label (hovered topic) is placed first and ignores collisions,
// but every label must still sit wholly inside the canvas.
export function placeLabels(cands: LabelCand[], pad = 6, size?: { w: number; h: number }): Placed[] {
  const order = [...cands].sort((a, b) => Number(b.force ?? false) - Number(a.force ?? false) || b.priority - a.priority);
  const taken: Box[] = [];
  const out: Placed[] = [];
  for (const c of order) {
    const own = boxOf(c, 0);
    if (size && (own.l < 0 || own.t < 0 || own.r > size.w || own.b > size.h)) continue;
    const padded = boxOf(c, pad);
    if (!c.force && taken.some((t) => hit(padded, t))) continue;
    taken.push(own);
    out.push({ id: c.id, x: c.x, y: c.y });
  }
  return out;
}
