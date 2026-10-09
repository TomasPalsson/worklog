import "@/app/stats/podium.css";
import { formatDuration } from "@/lib/format";
import type { Ranked } from "@/lib/stats_contract";
import { iso, isoBox, tones } from "./iso";
import { type Tip, tipProps } from "./tip";

const U = 11; // px per grid step
const W = 3.6; // podium step footprint (w = d)
const PITCH = 3.8;
const RW = 1.6; // runway block footprint
const RPITCH = 1.9;
const RUN_MAX_H = 2.2;
const RUN_MIN_H = 0.3;
const RY = 106; // runway baseline (screen y)
const SHOWN = 10;
const STEP_H: Record<number, number> = { 1: 4.4, 2: 3.2, 3: 2.2 };
const SLOTS = [
  { rank: 2, k: 0 },
  { rank: 1, k: 1 },
  { rank: 3, k: 2 },
];
const CONFETTI_COLORS = ["var(--sage)", "var(--violet)", "var(--amber)", "var(--terracotta)", "var(--slate)"];
const MEDAL: Record<number, string> = {
  1: "var(--amber)",
  2: "var(--slate)",
  3: "color-mix(in oklch, var(--amber) 55%, var(--terracotta))",
};
const METAL: Record<number, string> = { 1: "gold", 2: "silver", 3: "bronze" };

const f = (n: number) => n.toFixed(2);
const hue = (i: number, n: number) =>
  `oklch(var(--project-l) var(--project-c) ${Math.round((i * 360) / Math.max(1, n) + 25) % 360})`;

export const projectName = (label: string) => (label === "(no folder)" ? "no project" : label);
const hoursLabel = (sec: number) => `${sec >= 36000 ? Math.round(sec / 3600) : (sec / 3600).toFixed(1)}h`;
const short = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

/** Runway block height in grid units, proportional to hours (floor so tiny projects stay visible). */
export function runwayHeight(value: number, max: number): number {
  return max > 0 ? Math.max(RUN_MIN_H, (value / max) * RUN_MAX_H) : RUN_MIN_H;
}

/** Deterministic confetti rhombi (same on server and client). */
export function confetti(n: number, seed = 7): { x: number; y: number; r: number; c: string }[] {
  let s = seed;
  const rnd = () => (s = (s * 1664525 + 1013904223) % 4294967296) / 4294967296;
  return Array.from({ length: n }, (_, i) => ({
    x: Math.round((rnd() * 2 - 1) * 52 * 10) / 10,
    y: Math.round((-96 + rnd() * 42) * 10) / 10,
    r: Math.round((2 + rnd() * 1.8) * 10) / 10,
    c: CONFETTI_COLORS[i % CONFETTI_COLORS.length],
  }));
}

/** Rich hover card for one project: `rows` is the sorted, filtered list; `rank` is 1-based. */
export function podiumTip(rows: Ranked[], rank: number, total?: number, days?: number): Tip {
  const row = rows[rank - 1];
  const all = Math.max(total ?? 0, rows.reduce((s, r) => s + r.value, 0));
  // Only the top SHOWN folders are passed in, so a count is honest only when the list is complete.
  const complete = rows.length < SHOWN && (total === undefined || rows.reduce((s, r) => s + r.value, 0) >= total);
  const rankText = complete ? `#${rank} of ${rows.length}` : `#${rank}`;
  const pct = all > 0 ? Math.round((row.value / all) * 100) : 0;
  const prev = rows[rank - 2];
  const next = rows[rank];
  const out: [string, string][] = [
    ["Work time", formatDuration(row.value)],
    ["Hours", `${(row.value / 3600).toFixed(1)}h`],
    ["Share of work", `${pct}%`],
    ["Rank", rankText],
  ];
  if (days && days > 0) out.push(["Per worked day", formatDuration(row.value / days)]);
  if (prev) out.push([`Gap to #${rank - 1}`, `−${formatDuration(prev.value - row.value)}`]);
  else if (next) out.push(["Lead over #2", `+${formatDuration(row.value - next.value)}`]);

  const tied = (prev && prev.value === row.value) || (next && next.value === row.value);
  let note: string;
  if (tied) note = `Tied with #${prev && prev.value === row.value ? rank - 1 : rank + 1}.`;
  else if (rank === 1 && rows.length >= 3 && row.value > rows[1].value + rows[2].value) note = "More than #2 and #3 combined.";
  else if (rank === 1 && next && row.value >= next.value * 1.05) note = `${(row.value / next.value).toFixed(1)}× the runner-up.`;
  else if (rank === 1 && next) note = `Leads #2 by ${formatDuration(row.value - next.value)}.`;
  else if (rank === 1) note = "The only project on the board.";
  else if (prev.value - row.value < row.value * 0.1)
    note = `Only ${formatDuration(prev.value - row.value)} behind #${rank - 1}: one good afternoon swaps them.`;
  else note = `#${rank - 1} got ${(prev.value / row.value).toFixed(1)}× the time.`;

  return {
    title: projectName(row.label),
    sub: METAL[rank] ? `${METAL[rank]} · ${rankText}${complete ? " projects" : ""}` : `${rankText}${complete ? " projects" : ""}`,
    rows: out,
    bar: { value: row.value, max: all, label: "share of all work time" },
    note,
    accent: hue(rank - 1, rows.length),
  };
}

export interface PodiumLayout {
  rows: Ranked[];
  steps: { row: Ranked; rank: number; a: number; h: number }[];
  runway: { row: Ranked; rank: number; a: number; h: number }[];
}

export function layout(rowsIn: Ranked[]): PodiumLayout {
  const rows = rowsIn
    .filter((r) => r.value > 0)
    .sort((a, b) => b.value - a.value)
    .slice(0, SHOWN);
  const steps = SLOTS.filter((s) => rows[s.rank - 1]).map((s) => ({
    row: rows[s.rank - 1],
    rank: s.rank,
    a: (s.k - 1) * PITCH,
    h: STEP_H[s.rank],
  }));
  const rest = rows.slice(3);
  const max = rest[0]?.value ?? 0;
  const runway = rest.map((row, i) => ({
    row,
    rank: i + 4,
    a: (i - (rest.length - 1) / 2) * RPITCH,
    h: runwayHeight(row.value, max),
  }));
  return { rows, steps, runway };
}

function Box({ a, w, h, base, color }: { a: number; w: number; h: number; base: number; color: string }) {
  const b = isoBox(a, -a, 0, w, w, h, U);
  const t = tones(color);
  const s = { stroke: "var(--bg)", strokeWidth: 0.75, strokeLinejoin: "round" as const };
  return (
    <g transform={`translate(0 ${base})`}>
      <polygon points={b.left} fill={t.left} {...s} />
      <polygon points={b.right} fill={t.right} {...s} />
      <polygon points={b.top} fill={t.top} {...s} />
    </g>
  );
}

function Trophy({ x, y, rank }: { x: number; y: number; rank: number }) {
  const t = tones(MEDAL[rank]);
  const sc = rank === 1 ? 1.25 : 1;
  return (
    <g transform={`translate(${f(x)} ${f(y)}) scale(${sc})`} aria-hidden="true">
      <rect x={-3} y={-2} width={6} height={2} fill={t.right} />
      <rect x={-0.9} y={-5} width={1.8} height={3} fill={t.left} />
      <circle cx={-4.6} cy={-11} r={2} fill="none" stroke={t.left} strokeWidth={1} />
      <circle cx={4.6} cy={-11} r={2} fill="none" stroke={t.right} strokeWidth={1} />
      <path d="M-4 -14 L4 -14 L3 -9 Q0 -4.6 -3 -9 Z" fill={t.top} />
      <path d="M0 -14 L4 -14 L3 -9 Q1.6 -6.4 0 -5.6 Z" fill={t.right} />
    </g>
  );
}

function Step({ s, n, tip, delay }: { s: PodiumLayout["steps"][number]; n: number; tip: Tip; delay: number }) {
  const { a, h, rank, row } = s;
  const color = hue(rank - 1, n);
  const [cx] = iso(a + W / 2, -a + W / 2, 0, U);
  const [tx, ty] = iso(a + W / 2, -a + W / 2, h, U);
  const [fx, fy] = iso(a + W / 2, -a + W, h / 2, U);
  const [, by] = iso(a + W, -a + W, 0, U);
  const name = short(projectName(row.label), 10);
  return (
    <g
      className="sx-podium-rise sx-podium-mark"
      style={{ animationDelay: `${delay}ms` }}
      tabIndex={0}
      aria-label={`${tip.title}: #${rank}, ${formatDuration(row.value)}`}
      {...tipProps(tip)}
    >
      <Box a={a} w={W} h={h} base={0} color={color} />
      <text
        transform={`matrix(0.866 0.5 0 1 ${f(fx)} ${f(fy)})`}
        textAnchor="middle"
        aria-hidden="true"
        y={6}
        style={{ fontSize: 18, fontWeight: 700, fill: "var(--bg)", fontFamily: "var(--font-mono)", opacity: 0.9, pointerEvents: "none" }}
      >
        {rank}
      </text>
      <Trophy x={tx} y={ty} rank={rank} />
      <g aria-hidden="true">
        <rect className="sx-podium-plate" x={cx - 33} y={by + 4} width={66} height={24} rx={4} fill="var(--bg)" stroke="var(--border-strong)" strokeWidth={1} />
        <text x={cx} y={by + 14} textAnchor="middle" className="art-label" style={{ fontSize: 9.5, fill: "var(--fg)", pointerEvents: "none" }}>
          {name}
        </text>
        <text x={cx} y={by + 24} textAnchor="middle" className="art-label" style={{ fontSize: 9.5, pointerEvents: "none" }}>
          {formatDuration(row.value)}
        </text>
      </g>
    </g>
  );
}

function Runway({ r, n, tip, delay }: { r: PodiumLayout["runway"][number]; n: number; tip: Tip; delay: number }) {
  const [cx] = iso(r.a + RW / 2, -r.a + RW / 2, 0, U);
  const [, by] = iso(r.a, -r.a + RW, 0, U);
  return (
    <g
      className="sx-podium-rise sx-podium-mark"
      style={{ animationDelay: `${delay}ms` }}
      tabIndex={0}
      aria-label={`${tip.title}: #${r.rank}, ${formatDuration(r.row.value)}`}
      {...tipProps(tip)}
    >
      <Box a={r.a} w={RW} h={r.h} base={RY} color={hue(r.rank - 1, n)} />
      <g aria-hidden="true">
        <rect className="sx-podium-plate" x={cx - 17} y={RY + by + 2} width={34} height={22} rx={3} fill="var(--bg)" stroke="var(--border-strong)" strokeWidth={0.75} />
        <text x={cx} y={RY + by + 11} textAnchor="middle" className="art-label" style={{ fontSize: 9, fill: "var(--fg)", pointerEvents: "none" }}>
          {short(projectName(r.row.label), 5)}
        </text>
        <text x={cx} y={RY + by + 20} textAnchor="middle" className="art-label" style={{ fontSize: 9, pointerEvents: "none" }}>
          {hoursLabel(r.row.value)}
        </text>
      </g>
    </g>
  );
}

const VIEWBOX = `-140 -100 280 ${RY + 9 + 26 + 100}`;

/**
 * `total` is the report's true work seconds (`totals.work_seconds`); `days` is `totals.days_worked`.
 * `rows` are the top-N folders, so summing them undercounts.
 */
export function ProjectPodium({ rows: rowsIn, total, days }: { rows: Ranked[]; total?: number; days?: number }) {
  const L = layout(rowsIn);
  const n = L.rows.length;
  if (n === 0) {
    return (
      <figure className="sx-podium-card">
        <p className="sx-podium-head">The podium is empty</p>
        <svg className="sx-podium-svg" viewBox="-140 -20 280 40" role="img" aria-label="Project podium: no work yet.">
          <text x={0} y={0} textAnchor="middle" className="art-label" style={{ fontSize: 9 }}>
            no work folders yet
          </text>
        </svg>
        <p className="sx-podium-legend">height = work hours, once there is some</p>
      </figure>
    );
  }
  const top = L.rows[0];
  const name = projectName(top.label);
  const aria =
    `Project podium: ${name} takes gold with ${formatDuration(top.value)}. ` +
    L.rows
      .slice(1, 3)
      .map((r, i) => `${METAL[i + 2]} ${projectName(r.label)} ${formatDuration(r.value)}.`)
      .join(" ") +
    (L.runway.length ? ` ${L.runway.length} more on the runway.` : "");
  const tip = (rank: number) => podiumTip(L.rows, rank, total, days);
  return (
    <figure className="sx-podium-card">
      <p className="sx-podium-head">
        {name} takes gold with {formatDuration(top.value)}
      </p>
      <svg className="sx-podium-svg" viewBox={VIEWBOX} role="img" aria-label={aria}>
        <g className="art-fade" style={{ animationDelay: "500ms" }} aria-hidden="true">
          {confetti(14).map((c, i) => (
            <polygon
              key={i}
              points={`${c.x},${f(c.y - c.r * 0.57)} ${f(c.x + c.r)},${c.y} ${c.x},${f(c.y + c.r * 0.57)} ${f(c.x - c.r)},${c.y}`}
              fill={c.c}
            />
          ))}
        </g>
        {L.steps.map((s, i) => (
          <Step key={s.rank} s={s} n={n} tip={tip(s.rank)} delay={i * 90} />
        ))}
        {L.runway.length > 0 && (
          <text x={-136} y={RY - 30} className="art-label" style={{ fontSize: 9 }} aria-hidden="true">
            rest of the field
          </text>
        )}
        {L.runway.map((r, i) => (
          <Runway key={r.rank} r={r} n={n} tip={tip(r.rank)} delay={300 + i * 50} />
        ))}
      </svg>
      <p className="sx-podium-legend">height = work hours · steps are ranks, the runway is to scale</p>
    </figure>
  );
}
