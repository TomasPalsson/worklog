import "@/app/stats/charts-ranked.css";
import type { Ranked } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { shortTool } from "./toolName";
import { tipProps, type Tip } from "./tip";
import { ordinal, pct, times } from "./chart-tip-utils";

export type RankUnit = "count" | "minutes" | "seconds";

const num = (n: number) => (Number.isFinite(n) ? Math.max(0, n) : 0);

export function formatValue(unit: RankUnit, v: number): string {
  const n = num(v);
  if (unit === "count") return Math.round(n).toLocaleString("en-US");
  return formatDuration(unit === "minutes" ? n * 60 : n);
}

/** Hover card for one ranked row; `full` is the untruncated label. */
export function rowTip(rows: Ranked[], i: number, unit: RankUnit, full: string = rows[i].label): Tip {
  const v = num(rows[i].value);
  const sorted = rows.map((r) => num(r.value)).sort((a, b) => b - a);
  const total = sorted.reduce((a, b) => a + b, 0);
  const rank = sorted.filter((x) => x > v).length + 1;
  const max = sorted[0];
  const next = sorted.find((x) => x < v);
  const ties = sorted.filter((x) => x === v).length - 1;
  const rankRows: [string, string][] = [
    ["Value", formatValue(unit, v)],
    ["Share of list", total > 0 ? pct(v / total) : "0%"],
    ["Rank", `${rank} of ${rows.length}`],
  ];
  if (rank === 1 && ties === 0 && next !== undefined) rankRows.push(["Lead over next", formatValue(unit, v - next)]);
  else if (rank > 1) rankRows.push(["Behind the leader", formatValue(unit, max - v)]);
  let note: string;
  if (v <= 0) note = "Nothing here yet.";
  else if (rank === 1 && rows.length === 1) note = "The only one on the list.";
  else if (rank === 1 && ties > 0) note = `Tied for ${ordinal(rank)} with ${ties} other${ties === 1 ? "" : "s"}.`;
  else if (rank === 1) note = next! > 0 ? `${times(v / next!)} the runner-up.` : "The only one with any activity.";
  else note = `${pct(v / max)} of the leader's ${formatValue(unit, max)}.`;
  return {
    title: full,
    sub: rank === 1 ? (ties > 0 ? "joint top of the list" : "top of the list") : `${ordinal(rank)} place`,
    rows: rankRows,
    bar: { value: v, max, label: `${pct(max > 0 ? v / max : 0)} of the leader` },
    note,
    accent: rank === 1 ? "var(--amber)" : "var(--slate)",
  };
}

export function RankedBars({ title, unit, rows: raw, shorten }: { title: string; unit: RankUnit; rows: Ranked[]; shorten?: boolean }) {
  const rows = shorten ? raw.map((r) => ({ ...r, label: shortTool(r.label) })) : raw;
  const max = Math.max(0, ...rows.map((r) => num(r.value)));
  const topIdx = max > 0 ? rows.findIndex((r) => num(r.value) === max) : -1;
  const top = topIdx >= 0 ? rows[topIdx] : null;
  const label = top
    ? `${title}: ${rows.map((r) => `${r.label} ${formatValue(unit, r.value)}`).join(", ")}`
    : `${title}: nothing here yet`;

  return (
    <section className="sr-card">
      <h3 className="sr-title">{title}</h3>
      {!top ? (
        <p className="sr-empty">nothing here yet</p>
      ) : (
        <>
          <p className="sr-caption">{`${top.label} leads with ${formatValue(unit, top.value)}`}</p>
          <ul className="sr-rows" role="img" aria-label={label}>
            {rows.map((r, i) => (
              <li key={`${r.label}-${i}`} className="sr-row" data-top={i === topIdx || undefined} style={{ ["--i" as string]: i }} tabIndex={0} aria-label={`${raw[i].label}: ${formatValue(unit, r.value)}`} {...tipProps(rowTip(rows, i, unit, raw[i].label))}>
                <span className="sr-rank">
                  {i === topIdx ? (
                    <svg className="sr-crown" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M2.5 12.5h11M2.5 12.5L2 5.5l3.5 3L8 3.5l2.5 5 3.5-3-.5 7" />
                    </svg>
                  ) : (
                    i + 1
                  )}
                </span>
                <span className="sr-label">{r.label}</span>
                <span className="sr-value">{formatValue(unit, r.value)}</span>
                <span className="sr-track" aria-hidden="true">
                  <span className="sr-fill" style={{ display: "block", width: `${(num(r.value) / max) * 100}%` }} />
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
