import "@/app/stats/garden.css";
import { formatDuration } from "@/lib/format";
import type { Ranked } from "@/lib/stats_contract";
import { byDepth, iso, isoBox, tones } from "./iso";
import { tipProps, type Tip } from "./tip";

const U = 16; // px per grid step
const COLS = 5;
const SX = 3.2;
const SY = 3.8;
const SHOWN = 10;
const X0 = -3.6;
const Y0 = -1;
const X1 = (COLS - 1) * SX + 1.8;
const BH = 0.5; // soil bed thickness
const H_MIN = 1.6;
const H_MAX = 7.5;
const PW = 2.8; // sign plate width
// amber toward black (like iso tones), so it reads as soil in light and dark
const SOIL = "color-mix(in oklch, var(--amber) 50%, black)";
const WOOD = "color-mix(in oklch, var(--amber) 55%, var(--bg-sunk))";

const num = new Intl.NumberFormat("en-US");
const f = (n: number) => n.toFixed(2);
const dur = (min: number) => formatDuration(min * 60);

export interface Plant {
  label: string;
  minutes: number;
  rank: number; // 1-based
  n: number;
  gx: number;
  gy: number;
  h: number;
  leaves: number;
}

/** Plant height grows with sqrt(minutes), so a 9x site is ~3x taller, not 9x. */
export function plantHeight(minutes: number, max: number): number {
  return max > 0 && minutes > 0 ? H_MIN + (H_MAX - H_MIN) * Math.sqrt(Math.min(1, minutes / max)) : 0;
}

const top10 = (rows: Ranked[]) =>
  rows
    .filter((r) => r.value > 0)
    .sort((a, b) => b.value - a.value)
    .slice(0, SHOWN);

/** Depth of the bed (grid units) for n plants. */
export const bedY1 = (n: number) => (Math.max(1, Math.ceil(n / COLS)) - 1) * SY + 2.6;

export function layout(rows: Ranked[]): { plants: Plant[]; bounds: { x: number; y: number; w: number; h: number } } {
  const sorted = top10(rows);
  const max = sorted[0]?.value ?? 0;
  const plants = sorted.map((r, i): Plant => {
    const h = plantHeight(r.value, max);
    return {
      label: r.label,
      minutes: r.value,
      rank: i + 1,
      n: sorted.length,
      gx: (i % COLS) * SX,
      gy: Math.floor(i / COLS) * SY,
      h,
      leaves: Math.floor(h / 1.7),
    };
  });
  const Y1 = bedY1(plants.length);
  const x0 = iso(X0, Y1, 0, U)[0] - 10;
  const x1 = iso(X1, Y0, 0, U)[0] + 10;
  const tops = plants.map((p) => iso(p.gx, p.gy, BH + p.h + 1, U)[1] - 10);
  const top = Math.min(iso(X0, Y0, BH, U)[1] - 8, ...tops);
  const bottom = iso(X1, Y1, 0, U)[1] + 10;
  return { plants, bounds: { x: x0, y: top, w: x1 - x0, h: bottom - top } };
}

const isLocal = (d: string) => /^(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(:\d+)?$/.test(d);

const hue = (i: number, n: number) =>
  `oklch(var(--project-l) var(--project-c) ${Math.round((i * 360) / Math.max(1, n) + 25) % 360})`;

/** Rich hover card for one plant; every number comes from the rows. */
export function plantTip(p: Plant, all: Ranked[]): Tip {
  const rows = top10(all);
  const total = rows.reduce((s, r) => s + r.value, 0);
  const lead = rows[0];
  const share = total > 0 ? (p.minutes / total) * 100 : 0;
  const ofLead = (p.minutes / lead.value) * 100;
  let note: string;
  if (isLocal(p.label)) {
    note =
      p.rank === 1
        ? `${p.label}: you were your own favourite website for ${dur(p.minutes)}.`
        : `${p.label}: ${dur(p.minutes)} of visiting your own work.`;
  } else if (p.rank === 1 && rows.length > 1 && rows[1].value === p.minutes) {
    note = `Tied with ${rows[1].label} for the tallest.`;
  } else if (p.rank === 1) {
    note =
      rows.length === 1
        ? "A one-plant garden: everything else stayed in the seed packet."
        : p.minutes > total - p.minutes
          ? `More than the other ${rows.length - 1} ${rows.length === 2 ? "site" : "sites"} combined.`
          : `${(p.minutes / rows[1].value).toFixed(1)}× the runner-up, ${rows[1].label}.`;
  } else if (p.minutes === lead.value) {
    note = `Tied with ${lead.label} for the tallest.`;
  } else {
    note = `${Math.round(ofLead)}% of ${lead.label}'s height, ${dur(lead.value - p.minutes)} short of the tallest.`;
  }
  return {
    title: p.label,
    sub: "1 heartbeat = 1 minute in a tab",
    rows: [
      ["Time in tab", dur(p.minutes)],
      ["Share of top sites", share > 0 && share < 1 ? "<1%" : `${Math.round(share)}%`],
      ["Rank", `#${p.rank} of ${rows.length}`],
      ["Heartbeats", `≈ ${num.format(Math.round(p.minutes))}`],
      ...(p.minutes < lead.value ? ([["Behind the tallest", dur(lead.value - p.minutes)]] as [string, string][]) : []),
    ],
    bar: { value: p.minutes, max: lead.value, label: `${Math.round(ofLead)}% of the tallest plant` },
    note,
    accent: p.rank === 1 ? "var(--amber)" : hue(p.rank - 1, rows.length),
  };
}

type Box = [number, number, number, number, number, number, string];

function Boxes({ list, hair = true }: { list: Box[]; hair?: boolean }) {
  const s = hair ? { stroke: "var(--bg)", strokeWidth: 0.75, strokeLinejoin: "round" as const } : {};
  return (
    <>
      {list.map(([x, y, z, w, d, h, c], k) => {
        const b = isoBox(x, y, z, w, d, h, U);
        const t = tones(c);
        return (
          <g key={k}>
            <polygon points={b.left} fill={t.left} {...s} />
            <polygon points={b.right} fill={t.right} {...s} />
            <polygon points={b.top} fill={t.top} {...s} />
          </g>
        );
      })}
    </>
  );
}

function Leaf({ at, side }: { at: [number, number]; side: 1 | -1 }) {
  const [x, y] = at;
  const t = tones("var(--sage)");
  const pt = (dx: number, dy: number) => `${f(x + side * dx)},${f(y + dy)}`;
  return (
    <g>
      <polygon points={`${pt(0, 0)} ${pt(4.5, -4.5)} ${pt(10, -4.5)}`} fill={t.top} />
      <polygon points={`${pt(0, 0)} ${pt(10, -4.5)} ${pt(4.5, 0.5)}`} fill={t.right} />
    </g>
  );
}

/** Cross of four petals around a core; back petals, core, then front petals. */
function Bloom({ p, z, sun }: { p: Plant; z: number; sun: boolean }) {
  const c = sun ? "var(--amber)" : hue(p.rank - 1, p.n);
  const pe = sun ? 0.6 : 0.5;
  const mid = sun ? 0.9 : 0.5;
  const cx = p.gx - mid / 2;
  const cy = p.gy - mid / 2;
  const core = sun ? "color-mix(in oklch, var(--terracotta) 55%, var(--fg-muted))" : "var(--amber)";
  return (
    <Boxes
      list={[
        [cx - pe, cy, z, pe, mid, 0.3, c],
        [cx, cy - pe, z, mid, pe, 0.3, c],
        [cx, cy, z, mid, mid, 0.42, core],
        [cx + mid, cy, z, pe, mid, 0.3, c],
        [cx, cy + mid, z, mid, pe, 0.3, c],
      ]}
    />
  );
}

function Sign({ p }: { p: Plant }) {
  const name = p.label.length > 8 ? `${p.label.slice(0, 7)}…` : p.label;
  const sx = p.gx - PW / 2;
  const sy = p.gy + 1.2;
  const z = BH + 0.5;
  const ph = 0.7;
  const inset = isoBox(sx + 0.12, sy, z + 0.08, PW - 0.24, 0.1, ph - 0.16, U).left;
  const [ox, oy] = iso(p.gx, sy + 0.1, z + 0.2, U);
  return (
    <g aria-hidden="true">
      <Boxes list={[[p.gx - 0.06, sy + 0.02, BH, 0.12, 0.06, 0.6, WOOD], [sx, sy, z, PW, 0.1, ph, WOOD]]} />
      <polygon points={inset} fill="var(--bg)" />
      <text
        transform={`matrix(0.866 0.5 0 1 ${f(ox)} ${f(oy)})`}
        textAnchor="middle"
        style={{ fontSize: 8.5, fontWeight: 600, fill: "var(--fg)", fontFamily: "var(--font-mono)", pointerEvents: "none" }}
      >
        {name}
      </text>
    </g>
  );
}

function PlantDrawing({ p }: { p: Plant }) {
  const zTop = BH + p.h;
  const [lx, ly] = iso(p.gx, p.gy, BH, U);
  const [, ty] = iso(p.gx, p.gy, zTop + 0.8, U);
  const leaves = Array.from({ length: p.leaves }, (_, k) => ({ k, z: BH + 0.7 + k * 1.5 })).filter((l) => l.z < zTop - 0.5);
  return (
    <>
      <rect className="sx-garden-ring" x={lx - 22} y={ty - 6} width={44} height={ly - ty + 34} rx={4} />
      <Boxes list={[[p.gx - 0.1, p.gy - 0.1, BH, 0.2, 0.2, p.h, "var(--sage)"]]} hair={false} />
      {leaves.map(({ k, z }) => (
        <Leaf key={k} at={iso(p.gx, p.gy, z, U)} side={k % 2 ? 1 : -1} />
      ))}
      <Bloom p={p} z={zTop} sun={p.rank === 1} />
      <Sign p={p} />
    </>
  );
}

function WateringCan() {
  const x = -2.8;
  const y = 0.2;
  const list: Box[] = [
    [x - 0.3, y + 0.4, BH, 0.15, 0.2, 0.6, "var(--slate)"], // handle
    [x, y, BH, 1.0, 1.0, 0.8, "var(--slate)"],
    [x + 1.0, y + 0.4, BH + 0.45, 0.6, 0.15, 0.15, "var(--slate)"], // spout
    [x + 1.6, y + 0.35, BH + 0.4, 0.15, 0.25, 0.25, "var(--fg-muted)"], // rose
  ];
  return (
    <g aria-hidden="true">
      <Boxes list={list} />
    </g>
  );
}

export function TabGarden({ rows }: { rows: Ranked[] }) {
  const { plants, bounds: b } = layout(rows);
  const Y1 = bedY1(plants.length);
  const bed = <Boxes list={[[X0, Y0, 0, X1 - X0, Y1 - Y0, BH, SOIL]]} hair={false} />;
  const viewBox = `${f(b.x)} ${f(b.y)} ${f(b.w)} ${f(b.h)}`;

  if (plants.length === 0) {
    const [cx, cy] = iso((X0 + X1) / 2, (Y0 + Y1) / 2, BH, U);
    return (
      <figure className="sx-garden-card">
        <p className="sx-garden-head">The garden is bare</p>
        <svg className="sx-garden-svg" viewBox={viewBox} role="img" aria-label="Browser garden: nothing planted yet.">
          {bed}
          <text x={cx} y={cy} textAnchor="middle" className="art-label" style={{ fontSize: 12 }}>
            nothing planted yet
          </text>
        </svg>
        <p className="sx-garden-legend">height = time in the tab, once there is some</p>
      </figure>
    );
  }

  const top = plants[0];
  const total = plants.reduce((s, p) => s + p.minutes, 0);
  const aria =
    `Browser garden: ${top.label} grew tallest with ${dur(top.minutes)} of browsing. ` +
    `${plants.length} sites, ${dur(total)} in total. Height is time in the tab.`;
  const parts = [...plants].sort((a, c) => byDepth({ x: a.gx, y: a.gy }, { x: c.gx, y: c.gy }));
  return (
    <figure className="sx-garden-card">
      <p className="sx-garden-head">
        {top.label} grew tallest: {dur(top.minutes)} of browsing
      </p>
      <svg className="sx-garden-svg" viewBox={viewBox} role="img" aria-label={aria}>
        {bed}
        <WateringCan />
        {parts.map((p) => (
          <g
            key={p.label}
            className="sx-garden-plant"
            style={{ animationDelay: `${(p.rank - 1) * 70}ms` }}
            tabIndex={0}
            aria-label={`${p.label}: ${dur(p.minutes)}`}
            {...tipProps(plantTip(p, rows))}
          >
            <PlantDrawing p={p} />
          </g>
        ))}
      </svg>
      <p className="sx-garden-legend">height = time in the tab · sunflower = your most-visited site</p>
    </figure>
  );
}
