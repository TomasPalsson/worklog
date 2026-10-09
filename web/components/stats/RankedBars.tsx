import "@/app/stats/charts-ranked.css";
import type { Ranked } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { shortTool } from "./toolName";

export type RankUnit = "count" | "minutes" | "seconds";

const num = (n: number) => (Number.isFinite(n) ? Math.max(0, n) : 0);

export function formatValue(unit: RankUnit, v: number): string {
  const n = num(v);
  if (unit === "count") return Math.round(n).toLocaleString("en-US");
  return formatDuration(unit === "minutes" ? n * 60 : n);
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
              <li key={`${r.label}-${i}`} className="sr-row" data-top={i === topIdx || undefined} style={{ ["--i" as string]: i }}>
                <span className="sr-rank">
                  {i === topIdx ? (
                    <svg className="sr-crown" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M2.5 12.5h11M2.5 12.5L2 5.5l3.5 3L8 3.5l2.5 5 3.5-3-.5 7" />
                    </svg>
                  ) : (
                    i + 1
                  )}
                </span>
                <span className="sr-label" title={r.label}>{r.label}</span>
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
