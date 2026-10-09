import { useId } from "react";
import type { MirresDay } from "@/lib/tempo_line_contract";
import { daySummary } from "@/lib/mirresOverview";

export type DayKind = "billable" | "included" | "missing" | "not_billable";
/** Stack order, bottom to top. */
export const DAY_KINDS: DayKind[] = ["billable", "included", "missing", "not_billable"];

/** Share (0..1) of a day's hours per billing class; lines Mirres has no row for are "missing". */
export function daySplit(day: MirresDay): { kind: DayKind; share: number }[] {
  const secs: Record<DayKind, number> = { billable: 0, included: 0, missing: 0, not_billable: 0 };
  for (const l of day.lines) secs[l.billing ? l.billing.class : "missing"] += l.effective_seconds;
  const total = DAY_KINDS.reduce((a, k) => a + secs[k], 0);
  if (total <= 0) return [];
  return DAY_KINDS.filter((k) => secs[k] > 0).map((k) => ({ kind: k, share: secs[k] / total }));
}

export const dayRowId = (day: string) => `mirres-day-${day}`;

const COL_W = 30;
const GAP = 12;
const H = 96;
const TOP = 14; // room for the % above each column
const LABEL_H = 14;
const TARGET_W = 32;
const TARGET = 0.7;

/** One column per fetched day, stacked by billing class, 70% target line. */
export function BillableDays({ days: newestFirst }: { days: MirresDay[] }) {
  const pat = useId();
  if (newestFirst.length === 0) return null;
  // A chart reads left to right in time, whatever order the list below uses.
  const days = [...newestFirst].sort((a, b) => a.day.localeCompare(b.day));
  const w = days.length * (COL_W + GAP) - GAP;
  const ty = TOP + H - H * TARGET;
  const label = `Billed share per fetched day: ${days
    .map((d) => {
      const p = daySummary(d).billablePercent;
      return `${d.day} ${p === null ? "no Mirres data" : Math.round(p) + "%"}`;
    })
    .join(", ")}; target 70%`;
  return (
    <div className="art-billing-days">
      <svg width={w + TARGET_W} height={TOP + H + LABEL_H} viewBox={`0 0 ${w + TARGET_W} ${TOP + H + LABEL_H}`} role="img" aria-label={label}>
        <pattern id={pat} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <rect width="6" height="6" className="art-billing-hatch-bg" />
          <rect width="3" height="6" className="art-billing-hatch-bar" />
        </pattern>
        {days.map((d, i) => {
          const x = i * (COL_W + GAP);
          let y = TOP + H;
          const pct = daySummary(d).billablePercent;
          return (
            <a key={d.day} href={`#${dayRowId(d.day)}`} className="art-billing-col-link">
              <title>{d.day}</title>
              <rect x={x} y={TOP} width={COL_W} height={H} rx="3" className="art-billing-slot" />
              {pct !== null && (
                <text x={x + COL_W / 2} y={TOP - 4} textAnchor="middle" className="art-label art-billing-pct" data-hit={pct >= TARGET * 100 || undefined}>
                  {Math.round(pct)}%
                </text>
              )}
              {daySplit(d).map((s) => {
                const hgt = s.share * H;
                y -= hgt;
                return (
                  <rect
                    key={s.kind}
                    data-kind={s.kind}
                    data-share={s.share}
                    className="art-billing-seg art-grow-y"
                    x={x}
                    y={y}
                    width={COL_W}
                    height={hgt}
                    style={{ animationDelay: `${i * 40}ms`, ...(s.kind === "missing" && { fill: `url(#${pat})` }) }}
                  />
                );
              })}
              <text x={x + COL_W / 2} y={TOP + H + 11} textAnchor="middle" className="art-label">
                {Number(d.day.slice(8, 10))}
              </text>
            </a>
          );
        })}
        <line x1="0" x2={w + 4} y1={ty} y2={ty} className="art-billing-target" strokeWidth="1.6" strokeLinecap="round" strokeDasharray="3 4" />
        <text x={w + 8} y={ty + 3.5} className="art-label">
          70%
        </text>
      </svg>
    </div>
  );
}

/** 20px ring: sage arc = matched share of tickets, hatched amber track = unmatched. */
export function TicketRing({ matched, total }: { matched: number; total: number }) {
  const pat = useId();
  if (total <= 0) return null;
  const f = Math.min(1, Math.max(0, matched / total));
  return (
    <svg className="art-billing-ring" width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true" data-fraction={f}>
      <pattern id={pat} width="4" height="4" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="2" height="4" className="art-billing-ring-hatch" />
      </pattern>
      <circle cx="10" cy="10" r="7.2" stroke={`url(#${pat})`} strokeWidth="3.6" />
      {f > 0 && (
        <circle
          cx="10"
          cy="10"
          r="7.2"
          className="art-billing-ring-arc art-draw"
          pathLength="1"
          strokeWidth="3.6"
          strokeLinecap={f < 1 ? "round" : "butt"}
          // inline style, so .art-draw's full-length dasharray can't override it
          style={{ strokeDasharray: `${f} 1` }}
          transform="rotate(-90 10 10)"
        />
      )}
    </svg>
  );
}
