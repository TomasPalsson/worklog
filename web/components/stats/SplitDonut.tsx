import { useId } from "react";
import "@/app/stats/charts-ranked.css";
import type { Ranked } from "@/lib/stats_contract";
import { formatValue } from "./RankedBars";

const COLORS = ["var(--sage)", "var(--amber)", "var(--violet)", "var(--slate)", "var(--terracotta)"];
const HATCH = new Set(["none", "not yet"]);
const R = 40;
const C = 50;
const GAP = 0.05; // radians between segments

export const isHatch = (label: string) => HATCH.has(label.trim().toLowerCase());

const pt = (a: number) => `${(C + R * Math.sin(a)).toFixed(3)} ${(C - R * Math.cos(a)).toFixed(3)}`;

/** Arc path from angle a0 to a1 (radians clockwise from 12 o'clock). */
export function arcPath(a0: number, a1: number): string {
  return `M ${pt(a0)} A ${R} ${R} 0 ${a1 - a0 > Math.PI ? 1 : 0} 1 ${pt(a1)}`;
}

function buildSegs<T extends { value: number; share: number }>(items: T[]) {
  let at = 0;
  return items
    .filter((it) => it.value > 0)
    .map((it, k) => {
      const a0 = at * 2 * Math.PI;
      at += it.share;
      const a1 = at * 2 * Math.PI;
      const single = it.share > 0.9999;
      // a lone 100% segment is drawn as two half arcs (one arc can't start and end at the same point)
      const g = Math.min(GAP / 2, (a1 - a0) / 4); // never trim past the segment's middle
      const d = arcPath(a0 + (single ? 0 : g), single ? a0 + Math.PI : a1 - g);
      return { it, k, d, d2: single ? arcPath(a0 + Math.PI, a0 + 2 * Math.PI) : null };
    });
}

export function SplitDonut({ title, rows, unit }: { title: string; rows: Ranked[]; unit: "count" | "seconds" }) {
  const pid = useId().replace(/:/g, "");
  const vals = rows.map((r) => (Number.isFinite(r.value) ? Math.max(0, r.value) : 0));
  const total = vals.reduce((a, b) => a + b, 0);
  let colorN = 0;
  const items = rows.map((r, i) => {
    const hatch = isHatch(r.label);
    return { ...r, value: vals[i], hatch, color: hatch ? "" : COLORS[colorN++ % COLORS.length], share: total > 0 ? vals[i] / total : 0 };
  });
  const big = items.reduce<(typeof items)[number] | null>((b, it) => (it.value > 0 && (!b || it.value > b.value) ? it : b), null);
  const pct = (s: number) => `${Math.round(s * 100)}%`;
  const label = big
    ? `${title}: ${items.map((it) => `${it.label} ${pct(it.share)}`).join(", ")}`
    : `${title}: no data`;

  const segs = buildSegs(items);

  return (
    <section className="sr-card">
      <h3 className="sr-title">{title}</h3>
      {big ? (
        <p className="sr-caption">{big.label} takes {pct(big.share)}</p>
      ) : (
        <p className="sr-empty">no data</p>
      )}
      <div className="sr-donut-wrap">
        <svg className="sr-donut" viewBox="0 0 100 100" role="img" aria-label={label}>
          <defs>
            <pattern id={`${pid}-h`} width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <line x1="0" y1="0" x2="0" y2="5" stroke="var(--fg-subtle)" strokeWidth="2" />
            </pattern>
          </defs>
          <circle className="sr-ring-bg" cx={C} cy={C} r={R} />
          {segs.map(({ it, k, d, d2 }) =>
            [d, d2].filter((x): x is string => !!x).map((p, j) => (
              <path key={`${k}-${j}`} className="sr-seg art-draw" pathLength={1} d={p} style={{ stroke: it.hatch ? `url(#${pid}-h)` : it.color, animationDelay: `${k * 90}ms` }}>
                <title>{`${it.label}: ${formatValue(unit, it.value)} (${pct(it.share)})`}</title>
              </path>
            )),
          )}
          <text className={big ? "sr-pct" : "sr-pct-sub"} x={C} y={C + 3}>{big ? pct(big.share) : "no data"}</text>
          {big && <text className="sr-pct-sub" x={C} y={C + 15}>{big.label.length > 14 ? `${big.label.slice(0, 13)}…` : big.label}</text>}
        </svg>
        <Legend items={items} unit={unit} pct={pct} />
      </div>
    </section>
  );
}

function Legend({ items, unit, pct }: { items: { label: string; value: number; hatch: boolean; color: string; share: number }[]; unit: "count" | "seconds"; pct: (s: number) => string }) {
  return (
    <ul className="sr-legend">
      {items.map((it, i) => (
        <li key={`${it.label}-${i}`} className="sr-leg">
          <span className="sr-swatch" data-hatch={it.hatch || undefined} style={{ ["--c" as string]: it.color }} aria-hidden="true" />
          <span className="sr-label" title={it.label}>{it.label}</span>
          <span className="sr-value">{formatValue(unit, it.value)}</span>
          <span className="sr-leg-pct">{pct(it.share)}</span>
        </li>
      ))}
    </ul>
  );
}
