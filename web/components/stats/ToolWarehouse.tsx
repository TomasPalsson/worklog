import "@/app/stats/warehouse.css";
import type { Ranked } from "@/lib/stats_contract";
import { byDepth, iso, isoBox, tones } from "./iso";
import { tipProps, type Tip } from "./tip";
import { doorTip, forkliftTip, palletTip, pegsTip, ranked } from "./WarehouseTips";

const U = 12; // px per grid step
const P = 2.2; // pallet footprint (2x2 crates + 0.1 margin)
const PH = 0.3; // pallet thickness
const COLS = 4;
const SX = 5;
const SY_MIN = 5.4;
const DECAL_W = 4.8; // floor name-plate, in grid units (SX = 5 leaves a gap between neighbours)
const DECAL_Y0 = P + 0.2;
const DECAL_Y1 = P + 2.5;
const X_MIN = -4;
const X_MAX = (COLS - 1) * SX + P / 2 + DECAL_W / 2 + 0.2;
const Y_MIN = -0.6;
const MAX_CRATES = 24; // 2x2 footprint, 6 high: well inside the 4x4x6 budget
const MAX_PEGS = 16;
const SHOWN = 8;

export const fmt = (n: number) => Math.round(n).toLocaleString("en-US");
const f = (n: number) => n.toFixed(2);

/** "mcp__plugin_chrome-devtools-mcp_chrome-devtools__evaluate_script" -> "devtools·evaluate_script". */
export function shortTool(name: string): string {
  if (!name.startsWith("mcp__")) return name;
  const rest = name.slice(5);
  const cut = rest.lastIndexOf("__");
  const server = (cut < 0 ? rest : rest.slice(0, cut))
    .split("_")
    .pop()!
    .toLowerCase()
    .replace(/^chrome-/, "")
    .replace(/-mcp$/, "");
  return cut < 0 || !rest.slice(cut + 2) ? server : `${server}·${rest.slice(cut + 2)}`;
}

/** Smallest 1/2/5 x 10^k that fits `max` into MAX_CRATES crates. */
export function niceUnit(max: number): number {
  const need = Math.max(1, max / MAX_CRATES);
  const mag = 10 ** Math.floor(Math.log10(need));
  return [1, 2, 5, 10].map((m) => m * mag).find((u) => u >= need)!;
}

export interface Crate {
  x: number;
  y: number;
  z: number;
  h: number;
  layer: number;
}

/** Crates fill a 2x2 footprint, then climb; a partial last load is a half-height crate. */
export function stackCrates(value: number, unit: number): Crate[] {
  const n = value > 0 ? Math.ceil(value / unit) : 0;
  const partial = n > 0 && value % unit !== 0;
  const out: Crate[] = [];
  for (let k = 0; k < n; k++) {
    const cell = k % 4;
    const layer = Math.floor(k / 4);
    out.push({ x: cell % 2, y: cell >> 1, z: layer, h: partial && k === n - 1 ? 0.5 : 1, layer });
  }
  return out.sort((a, b) => a.layer - b.layer || byDepth(a, b));
}

export function pegCount(total: number): { pegs: number; per: number } {
  const per = 1000 * Math.max(1, Math.ceil(total / 1000 / MAX_PEGS));
  return { pegs: total > 0 ? Math.max(1, Math.round(total / per)) : 0, per };
}

interface Pallet {
  tool: Ranked;
  i: number;
  gx: number;
  gy: number;
  crates: Crate[];
  height: number;
}

export interface Layout {
  unit: number;
  pallets: Pallet[];
  yMax: number;
  yWork: number;
  sy: number;
  bounds: { x: number; y: number; w: number; h: number };
}

export function layout(toolsIn: Ranked[]): Layout {
  const tools = toolsIn
    .filter((t) => t.value > 0)
    .sort((a, b) => b.value - a.value)
    .slice(0, SHOWN);
  const unit = niceUnit(tools[0]?.value ?? 0);
  const rows = tools.length > COLS ? 2 : 1;
  const stacks = tools.map((tool) => {
    const crates = stackCrates(tool.value, unit);
    return { crates, height: crates.reduce((m, c) => Math.max(m, c.z + c.h), 0) };
  });
  // A front-row stack of height h hides floor points up to h units behind it, so the
  // back row's name plates stay clear only if the row gap exceeds plate depth + h.
  const frontH = Math.max(0, ...stacks.slice(COLS).map((s) => s.height));
  const sy = rows > 1 ? Math.max(SY_MIN, DECAL_Y1 + frontH + PH + 0.4) : SY_MIN;
  const pallets: Pallet[] = tools.map((tool, i) => ({
    tool,
    i,
    gx: (i % COLS) * SX,
    gy: Math.floor(i / COLS) * sy,
    ...stacks[i],
  }));
  const yWork = (rows - 1) * sy + DECAL_Y1 + 1.6;
  const yMax = yWork + 1.2;
  const x0 = iso(X_MIN, yMax, 0, U)[0] - 8;
  const x1 = iso(X_MAX, Y_MIN, 0, U)[0] + 8;
  const tops = pallets.map((p) => iso(p.gx, p.gy, PH + p.height, U)[1] - 6);
  const top = Math.min(iso(X_MIN, Y_MIN, 0, U)[1] - 8, iso(-3.6, 0.2, 2.8, U)[1] - 8, ...tops);
  const bottom = iso(X_MAX, yMax, 0, U)[1] + 32;
  return { unit, pallets, yMax, yWork, sy, bounds: { x: x0, y: top, w: x1 - x0, h: bottom - top } };
}

const hue = (i: number, n: number) =>
  `oklch(var(--project-l) var(--project-c) ${Math.round((i * 360) / Math.max(1, n) + 25) % 360})`;

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

const line = (a: [number, number], b: [number, number]) => `M${f(a[0])} ${f(a[1])}L${f(b[0])} ${f(b[1])}`;

function Peg({ x, y, fill }: { x: number; y: number; fill: string }) {
  return (
    <g aria-hidden="true">
      <rect x={x - 2.6} y={y - 7.5} width={5.2} height={7.5} rx={2.6} fill={fill} />
      <circle cx={x} cy={y - 10} r={2.4} fill={fill} />
    </g>
  );
}

function PalletDrawing({ p, n }: { p: Pallet; n: number }) {
  const color = hue(p.i, n);
  const t = tones(color);
  const wood = "color-mix(in oklch, var(--amber) 35%, var(--bg-sunk))";
  const slat: string[] = [];
  for (let s = 1; s < 5; s++) {
    const fr = (s / 5) * P;
    slat.push(line(iso(p.gx + fr, p.gy + P, 0, U), iso(p.gx + fr, p.gy + P, PH, U)));
    slat.push(line(iso(p.gx + P, p.gy + fr, 0, U), iso(p.gx + P, p.gy + fr, PH, U)));
  }
  const seam = `color-mix(in oklch, ${color} 40%, black)`;
  const initial = (shortTool(p.tool.label).match(/[a-z0-9]/i)?.[0] ?? "?").toUpperCase();
  const face = { stroke: "var(--bg)", strokeWidth: 0.75, strokeLinejoin: "round" as const };
  return (
    <>
      <Boxes list={[[p.gx, p.gy, 0, P, P, PH, wood]]} hair={false} />
      <path d={slat.join("")} stroke="var(--bg)" strokeWidth={0.75} fill="none" />
      {p.crates.map((c, k) => {
        const x = p.gx + 0.1 + c.x;
        const y = p.gy + 0.1 + c.y;
        const z = PH + c.z;
        const b = isoBox(x, y, z, 1, 1, c.h, U);
        const seams: string[] = [];
        for (const fr of c.h < 1 ? [0.5] : [1 / 3, 2 / 3]) {
          seams.push(line(iso(x, y + 1, z + c.h * fr, U), iso(x + 1, y + 1, z + c.h * fr, U)));
          seams.push(line(iso(x + 1, y, z + c.h * fr, U), iso(x + 1, y + 1, z + c.h * fr, U)));
        }
        const [ox, oy] = iso(x + 0.5, y + 1, z + 0.28, U);
        return (
          <g key={k}>
            <polygon points={b.left} fill={t.left} {...face} />
            <polygon points={b.right} fill={t.right} {...face} />
            <polygon points={b.top} fill={t.top} {...face} />
            <path d={seams.join("")} stroke={seam} strokeWidth={0.7} fill="none" opacity={0.7} />
            {c.h === 1 && (
              <text
                transform={`matrix(0.866 0.5 0 1 ${f(ox)} ${f(oy)})`}
                textAnchor="middle"
                aria-hidden="true"
                style={{
                  fontSize: 7.5,
                  fontWeight: 700,
                  fill: "var(--bg)",
                  fontFamily: "var(--font-mono)",
                  opacity: 0.85,
                  pointerEvents: "none",
                }}
              >
                {initial}
              </text>
            )}
          </g>
        );
      })}
    </>
  );
}

/** Name plate painted flat on the floor in front of the pallet, so no crate can float over it. */
function Label({ p, tip }: { p: Pallet; tip: Tip }) {
  const name = shortTool(p.tool.label);
  const shown = name.length > 9 ? `${name.slice(0, 8)}…` : name;
  const x0 = p.gx + P / 2 - DECAL_W / 2;
  const plate = [iso(x0, p.gy + DECAL_Y0, 0, U), iso(x0 + DECAL_W, p.gy + DECAL_Y0, 0, U), iso(x0 + DECAL_W, p.gy + DECAL_Y1, 0, U), iso(x0, p.gy + DECAL_Y1, 0, U)];
  const [ox, oy] = iso(p.gx + P / 2, p.gy + DECAL_Y0, 0, U);
  return (
    <g {...tipProps(tip)}>
      <polygon points={plate.map(([a, b]) => `${f(a)},${f(b)}`).join(" ")} fill="var(--bg)" stroke="var(--border-strong)" strokeWidth={1} strokeLinejoin="round" />
      <g transform={`matrix(0.866 0.5 -0.866 0.5 ${f(ox)} ${f(oy)})`} textAnchor="middle" aria-hidden="true">
        <text x={0} y={13} className="art-label" style={{ fontSize: 11, fill: "var(--fg)" }}>
          {shown}
        </text>
        <text x={0} y={25} className="art-label" style={{ fontSize: 10 }}>
          {fmt(p.tool.value)}
        </text>
      </g>
    </g>
  );
}

const FORKLIFT_REST: Box[] = [
  [-3.55, 0.2, 1.1, 0.12, 0.12, 1.5, "var(--slate)"],
  [-2.7, 0.2, 1.1, 0.12, 0.12, 1.5, "var(--slate)"],
  [-3.6, 0.2, 2.6, 1.0, 0.7, 0.1, "var(--slate)"], // small canopy: clear of the driver's head
  [-3.4, 1.5, 0, 0.5, 0.3, 0.5, "var(--fg-muted)"],
  [-2.4, 1.5, 0, 0.5, 0.3, 0.5, "var(--fg-muted)"],
  [-2.2, 0.2, 0.1, 0.2, 1.6, 2.2, "var(--slate)"],
  [-2.0, 0.4, 0.2, 1.4, 0.2, 0.08, "var(--slate)"],
  [-2.0, 1.4, 0.2, 1.4, 0.2, 0.08, "var(--slate)"],
];

function Forklift({ tip }: { tip: Tip }) {
  const [hx, hy] = iso(-3, 0.9, 1.1, U);
  return (
    <g {...tipProps(tip)} tabIndex={0} aria-label={`${tip.title}, ${tip.rows?.[0]?.[1] ?? ""}`}>
      <Boxes list={[[-3.6, 0.2, 0.3, 1.4, 1.6, 0.8, "var(--amber)"]]} />
      <Peg x={hx} y={hy} fill="var(--violet)" />
      <Boxes list={FORKLIFT_REST} />
    </g>
  );
}

const DOOR_X = -3.2;

function Door({ yMax, tip }: { yMax: number; tip: Tip }) {
  const frame: Box[] = [
    [DOOR_X, yMax - 0.3, 0, 0.25, 0.25, 3, "var(--slate)"],
    [DOOR_X + 2.35, yMax - 0.3, 0, 0.25, 0.25, 3, "var(--slate)"],
    [DOOR_X, yMax - 0.3, 2.8, 2.6, 0.25, 0.3, "var(--slate)"],
  ];
  const opening = isoBox(DOOR_X + 0.25, yMax - 0.05, 0, 2.1, 0.01, 2.8, U).left;
  return (
    <g className="art-fade" style={{ animationDelay: "200ms" }} {...tipProps(tip)} tabIndex={0} aria-label={`${tip.title}, ${tip.rows?.[0]?.[1] ?? ""}`}>
      <polygon points={opening} fill="color-mix(in oklch, var(--slate) 30%, var(--bg))" />
      <Boxes list={frame} />
    </g>
  );
}

function Workers({ total, yWork, tip }: { total: number; yWork: number; tip: Tip }) {
  const { pegs } = pegCount(total);
  if (pegs === 0) return null;
  return (
    <g className="art-fade" style={{ animationDelay: "400ms" }} {...tipProps(tip)} tabIndex={0} aria-label={`Subagents, ${fmt(total)}`}>
      {Array.from({ length: pegs }, (_, k) => {
        const [px, py] = iso(1 + k, yWork, 0, U);
        return <Peg key={k} x={px} y={py} fill="var(--violet)" />;
      })}
    </g>
  );
}

/**
 * `totalCalls` / `totalHelpers` are the report's true totals (`totals.tool_calls`, `totals.helpers`);
 * `tools` / `helpers` are only the top-N rows, so summing them undercounts.
 */
export function ToolWarehouse({
  tools,
  helpers,
  totalCalls,
  totalHelpers,
  daysWorked,
}: {
  tools: Ranked[];
  helpers: Ranked[];
  totalCalls?: number;
  totalHelpers?: number;
  /** `totals.days_worked`: enables the per-day rows and notes. */
  daysWorked?: number;
}) {
  const L = layout(tools);
  const { bounds: b, pallets, unit } = L;
  const sum = (r: Ranked[]) => r.reduce((s, t) => s + Math.max(0, t.value), 0);
  const all = Math.max(totalCalls ?? 0, sum(tools));
  const total = Math.max(totalHelpers ?? 0, sum(helpers));
  const scope = totalCalls ? "of all" : "of your top tools";
  const floor = <Boxes list={[[X_MIN, Y_MIN, -0.15, X_MAX - X_MIN, L.yMax - Y_MIN, 0.15, "var(--bg-sunk)"]]} hair={false} />;
  const viewBox = `${f(b.x)} ${f(b.y)} ${f(b.w)} ${f(b.h)}`;

  if (pallets.length === 0) {
    const [cx, cy] = iso((X_MIN + X_MAX) / 2, (Y_MIN + L.yMax) / 2, 0, U);
    return (
      <figure style={{ margin: 0 }}>
        <p className="sx-warehouse-head">The warehouse is empty</p>
        <svg className="sx-warehouse-svg" viewBox={viewBox} role="img" aria-label="Tool warehouse: nothing built yet.">
          {floor}
          <text x={cx} y={cy} textAnchor="middle" className="art-label" style={{ fontSize: 12 }}>
            nothing built yet
          </text>
        </svg>
        <p className="sx-warehouse-legend">1 crate = a batch of tool calls, once there are some</p>
      </figure>
    );
  }

  const top = pallets[0];
  const pct = all > 0 ? Math.round((top.tool.value / all) * 100) : 0;
  const name = shortTool(top.tool.label);
  const list = ranked(tools);
  const tipOf = (p: Pallet) => palletTip(p.tool, list, all, unit, daysWorked, hue(p.i, pallets.length));
  const parts = [...pallets].sort((a, c) => byDepth({ x: a.gx, y: a.gy }, { x: c.gx, y: c.gy }));
  const aria =
    `Tool warehouse: ${name} leads with ${fmt(top.tool.value)} calls, ${pct}% ${scope}. ` +
    `${pallets.length} tools shown, 1 crate = ${fmt(unit)} calls.` +
    (total > 0 ? ` ${fmt(total)} subagents.` : "");
  return (
    <figure className="sx-warehouse-card">
      <p className="sx-warehouse-head">
        {name} runs the warehouse: {fmt(top.tool.value)} calls ({pct}% {scope})
      </p>
      <svg className="sx-warehouse-svg" viewBox={viewBox} role="img" aria-label={aria}>
        {floor}
        <Door yMax={L.yMax} tip={doorTip(all, list.length, list[0], daysWorked)} />
        <g className="art-fade">
          <Forklift tip={forkliftTip(top.tool, all, unit, daysWorked)} />
        </g>
        {parts.map((p) => (
          <g
            key={p.tool.label}
            className="sx-warehouse-stack"
            style={{ animationDelay: `${p.i * 70}ms` }}
            {...tipProps(tipOf(p))}
            tabIndex={0}
            aria-label={`${shortTool(p.tool.label)}, ${fmt(p.tool.value)} calls`}
          >
            <PalletDrawing p={p} n={pallets.length} />
          </g>
        ))}
        <Workers total={total} yWork={L.yWork} tip={pegsTip(helpers, total, pegCount(total).per, daysWorked)} />
        <g className="art-fade" style={{ animationDelay: "600ms" }}>
          {parts.map((p) => (
            <Label key={p.tool.label} p={p} tip={tipOf(p)} />
          ))}
        </g>
      </svg>
      <p className="sx-warehouse-legend">
        1 crate = {fmt(unit)} calls · half crate = a partial load
        {total > 0 && <> · {fmt(total)} subagents, 1 peg = {fmt(pegCount(total).per)}</>}
      </p>
    </figure>
  );
}
