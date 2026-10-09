import "@/app/stats/keycaps.css";
import type { Ranked } from "@/lib/stats_contract";
import { byDepth, iso, isoBox, tones } from "./iso";
import { tipProps, type Tip } from "./tip";

const U = 16; // px per grid step
const PITCH = 2.1; // one key slot
const GAP = 0.25;
const COLS = 4;
const SHOWN = 12;
const H_MIN = 0.5;
const H_MAX = 2.4;
const PRESS = 0.25; // the #1 key sits this much lower
const CAP_INSET = 0.15;
const CAP_H = 0.2;
const SLAB = 0.7;
const MARGIN = 0.5;
const STRIP = 1.5; // prompt strip in front of the keys

const nf = new Intl.NumberFormat("en-US");
const fmt = (n: number) => nf.format(Math.round(n));
const f = (n: number) => n.toFixed(2);
const pctText = (r: number) => (r > 0 && r < 0.01 ? "<1%" : `${Math.round(r * 100)}%`);

/** Clip text to `max` characters, ending in an ellipsis. */
export function ellipsize(text: string, max: number): string {
  if (max <= 0) return "";
  return text.length <= max ? text : max === 1 ? "…" : `${text.slice(0, max - 1)}…`;
}

/** Key travel in grid units: floor of H_MIN, the top key reaches H_MAX. */
export function keyHeight(value: number, max: number): number {
  return max > 0 && value > 0 ? H_MIN + (H_MAX - H_MIN) * Math.min(1, value / max) : H_MIN;
}

/** Rank band -> token colour: spacebar amber, then sage, violet, slate. */
export function keyColor(rank: number): string {
  if (rank === 1) return "var(--amber)";
  if (rank <= 4) return "var(--sage)";
  if (rank <= 8) return "var(--violet)";
  return "var(--slate)";
}

export interface Key {
  rank: number;
  label: string;
  value: number;
  slot: number; // column slot
  row: number;
  slots: number; // width in slots (1, or 2 for #1)
  x: number;
  y: number;
  w: number;
  d: number;
  h: number;
  color: string;
}

export interface Layout {
  keys: Key[];
  cols: number;
  rows: number;
  max: number;
  bounds: { x: number; y: number; w: number; h: number };
}

/** Rows of ranked keys, flowed left to right; #1 takes two slots. */
export function layout(rowsIn: Ranked[]): Layout {
  const rows = rowsIn
    .filter((r) => r.value > 0)
    .sort((a, b) => b.value - a.value)
    .slice(0, SHOWN);
  const max = rows[0]?.value ?? 0;
  let slot = 0;
  let row = 0;
  const keys = rows.map((r, i): Key => {
    const slots = i === 0 ? 2 : 1;
    if (slot + slots > COLS) {
      slot = 0;
      row++;
    }
    const k: Key = {
      rank: i + 1,
      label: r.label,
      value: r.value,
      slot,
      row,
      slots,
      x: slot * PITCH,
      y: row * PITCH,
      w: slots * PITCH - GAP,
      d: PITCH - GAP,
      h: keyHeight(r.value, max) - (i === 0 ? PRESS : 0),
      color: keyColor(i + 1),
    };
    slot += slots;
    return k;
  });
  const nRows = keys.length ? row + 1 : 2;
  const xMax = COLS * PITCH - GAP + MARGIN;
  const yMax = nRows * PITCH - GAP + MARGIN + STRIP;
  const x0 = iso(-MARGIN, yMax, 0, U)[0] - 8;
  const x1 = iso(xMax, -MARGIN, 0, U)[0] + 8;
  const top = iso(-MARGIN, -MARGIN, H_MAX + CAP_H, U)[1] - 8;
  const bottom = iso(xMax, yMax, -SLAB, U)[1] + 8;
  return { keys, cols: COLS, rows: nRows, max, bounds: { x: x0, y: top, w: x1 - x0, h: bottom - top } };
}

const closeNote = (v: number, other: Key) => v === other.value;

/** Rich hover card for one key. `all` = every shell command, `days` = worked days. */
export function keyTip(k: Key, keys: Key[], all: number, days = 0, count = keys.length): Tip {
  const top = keys[0];
  const next = keys[1];
  const rows: [string, string][] = [
    ["Runs", fmt(k.value)],
    ["Share of shell commands", pctText(all > 0 ? k.value / all : 0)],
    ["Rank", `#${k.rank} of ${count}`],
  ];
  if (days > 0) rows.push(["Per worked day", `${(k.value / days).toFixed(1)} runs`]);
  let note: string;
  if (keys.length === 1) note = "The only key you pressed.";
  else if (k.rank === 1) {
    const rest = keys.slice(1).reduce((s, x) => s + x.value, 0);
    note =
      k.value > rest
        ? keys.length === 2
          ? "More than the other key."
          : `More than all ${keys.length - 1} other keys combined.`
        : closeNote(k.value, next)
          ? `Tied with ${next.label} for most-pressed.`
          : `${(k.value / next.value).toFixed(1)}× as often as ${next.label}.`;
  } else if (k.value === top.value) note = `Tied with ${top.label} for most-pressed.`;
  else if (top.value / k.value < 1.05) note = `Neck and neck with ${top.label}.`;
  else note = `${(top.value / k.value).toFixed(1)}× less pressed than ${top.label}.`;
  return {
    title: k.label,
    sub: k.rank === 1 ? "the spacebar of your terminal" : `key #${k.rank} of your keyboard`,
    rows,
    bar: { value: k.value, max: top.value, label: `${pctText(k.value / top.value)} of ${top.label}` },
    note,
    accent: k.color,
  };
}

function KeyDrawing({ k, keys, all, days, count, i }: { k: Key; keys: Key[]; all: number; days: number; count: number; i: number }) {
  const t = tones(k.color);
  const body = isoBox(k.x, k.y, 0, k.w, k.d, k.h, U);
  const ci = CAP_INSET;
  const cap = isoBox(k.x + ci, k.y + ci, k.h, k.w - 2 * ci, k.d - 2 * ci, CAP_H, U);
  const [ox, oy] = iso(k.x + ci, k.y + ci, k.h + CAP_H, U);
  const capW = (k.w - 2 * ci) * U;
  const size = 7.5;
  const text = ellipsize(k.label, Math.floor((capW - 2) / (size * 0.6)));
  const face = { stroke: "var(--bg)", strokeWidth: 0.75, strokeLinejoin: "round" as const };
  const tip = keyTip(k, keys, all, days, count);
  const ring = k.rank === 1 ? isoBox(k.x - 0.12, k.y - 0.12, 0, k.w + 0.24, k.d + 0.24, 0, U).top : null;
  return (
    <g
      className="sx-keycaps-key"
      style={{ animationDelay: `${i * 60}ms` }}
      tabIndex={0}
      aria-label={`${tip.title}: ${fmt(k.value)} runs`}
      {...tipProps(tip)}
    >
      {ring && <polygon points={ring} fill="none" stroke="var(--amber)" strokeWidth={1.5} strokeLinejoin="round" />}
      <g className="sx-keycaps-lift">
        <polygon points={body.left} fill={t.left} {...face} />
        <polygon points={body.right} fill={t.right} {...face} />
        <polygon points={body.top} fill={t.top} {...face} />
        <polygon points={cap.left} fill={t.left} {...face} />
        <polygon points={cap.right} fill={t.right} {...face} />
        <polygon points={cap.top} fill={t.top} {...face} />
        <text
          transform={`matrix(0.866 0.5 -0.866 0.5 ${f(ox)} ${f(oy)})`}
          x={capW / 2}
          y={(k.d - 2 * ci) * U * 0.62}
          textAnchor="middle"
          aria-hidden="true"
          style={{ fontSize: size, fontWeight: 700, fill: "var(--bg)", fontFamily: "var(--font-mono)", pointerEvents: "none" }}
        >
          {text}
        </text>
      </g>
    </g>
  );
}

function Base({ L }: { L: Layout }) {
  const w = L.cols * PITCH - GAP + 2 * MARGIN;
  const d = L.rows * PITCH - GAP + 2 * MARGIN + STRIP;
  const b = isoBox(-MARGIN, -MARGIN, -SLAB, w, d, SLAB, U);
  const t = tones("color-mix(in oklch, var(--slate) 45%, black)");
  const cy = L.rows * PITCH - GAP + 0.45;
  const cur = isoBox(0.1, cy, 0, 0.55, 0.55, 0.12, U);
  const ct = tones("var(--amber)");
  return (
    <>
      <polygon points={b.left} fill={t.left} />
      <polygon points={b.right} fill={t.right} />
      <polygon points={b.top} fill={t.top} />
      <g className="sx-keycaps-cursor" aria-hidden="true">
        <polygon points={cur.left} fill={ct.left} />
        <polygon points={cur.right} fill={ct.right} />
        <polygon points={cur.top} fill={ct.top} />
      </g>
    </>
  );
}

/** `rows` are the top shell commands; `total` is every shell command; `days` the worked days. */
export function ShellKeycaps({ rows, total, days }: { rows: Ranked[]; total?: number; days?: number }) {
  const L = layout(rows);
  const { bounds: b, keys } = L;
  const viewBox = `${f(b.x)} ${f(b.y)} ${f(b.w)} ${f(b.h)}`;
  if (keys.length === 0) {
    return (
      <figure style={{ margin: 0 }}>
        <p className="sx-keycaps-head">The keyboard is unplugged</p>
        <svg className="sx-keycaps-svg" viewBox={viewBox} role="img" aria-label="Shell keycaps: no commands yet.">
          <Base L={L} />
        </svg>
        <p className="sx-keycaps-legend">key height = times run, once there are some</p>
      </figure>
    );
  }
  const all = Math.max(total ?? 0, keys.reduce((s, k) => s + k.value, 0));
  const top = keys[0];
  const aria = `Shell keycaps: ${top.label} is your most-pressed key with ${fmt(top.value)} runs, ${pctText(top.value / all)} of shell commands. ${keys.length} keys shown, height = times run.`;
  const ordered = [...keys].sort(byDepth);
  return (
    <figure className="sx-keycaps-card">
      <p className="sx-keycaps-head">
        {top.label} is your most-pressed key: {fmt(top.value)} times
      </p>
      <svg className="sx-keycaps-svg" viewBox={viewBox} style={{ maxWidth: Math.round(b.w * 1.6) }} role="img" aria-label={aria}>
        <Base L={L} />
        {ordered.map((k) => (
          <KeyDrawing key={k.label} k={k} keys={keys} all={all} days={days ?? 0} count={rows.filter((r) => r.value > 0).length} i={k.rank - 1} />
        ))}
      </svg>
      <p className="sx-keycaps-legend">key height = times run · the wide key is your #1</p>
    </figure>
  );
}
