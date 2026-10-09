// Coverage donut + workday rail for the 17:00 recap. The same numbers sit
// beside it as text, but it carries its own aria-label.
import type { RecapGap } from "@/lib/daily_helpers_contract";

export function railSegments(gaps: Pick<RecapGap, "started_at" | "ended_at">[]) {
  const spans = gaps
    .map((g) => [Date.parse(g.started_at), Date.parse(g.ended_at)] as const)
    .filter(([a, b]) => Number.isFinite(a) && Number.isFinite(b) && b > a);
  if (spans.length === 0) return [];
  const lo = Math.min(...spans.map((s) => s[0]));
  const total = Math.max(...spans.map((s) => s[1])) - lo;
  return spans.map(([a, b]) => ({ left: ((a - lo) / total) * 100, width: ((b - a) / total) * 100 }));
}

const R = 16;
const RAYS = [0, 60, 120, 180, 240, 300];

export function CoverageRing({
  percent,
  gaps,
  heldBack,
}: {
  percent: number;
  gaps: Pick<RecapGap, "started_at" | "ended_at" | "minutes">[];
  heldBack: number;
}) {
  const pct = Math.min(100, Math.max(0, Number.isFinite(percent) ? percent : 0));
  const p = pct / 100;
  const done = pct >= 100 && gaps.length === 0 && heldBack === 0;
  const segs = railSegments(gaps);
  const label = `${Math.round(pct)}% covered, ${gaps.length} ${gaps.length === 1 ? "gap" : "gaps"}${heldBack > 0 ? `, ${heldBack} held back` : ""}`;

  return (
    <div className="art-ring" role="img" aria-label={label} data-done={done || undefined}>
      <svg className="art-ring-svg" viewBox="-4 -4 48 48" width="40" height="40" aria-hidden="true" fill="none" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="20" cy="20" r={R} stroke="var(--border)" />
        {pct > 0 && (
          <circle
            className="art-draw art-ring-arc"
            cx="20"
            cy="20"
            r={R}
            pathLength="1"
            stroke="var(--sage)"
            style={{ strokeDasharray: `${p} 2` }}
            transform="rotate(-90 20 20)"
          />
        )}
        {done ? (
          <path className="art-draw art-ring-check" d="M13.5 20.5l4.5 4.5 8.5-9.5" pathLength="1" stroke="var(--sage-ink)" />
        ) : (
          <text className="art-label" x="20" y="23.5" textAnchor="middle" style={{ fontSize: 13 }}>
            {Math.round(pct)}
          </text>
        )}
        {done &&
          RAYS.map((a) => (
            <line key={a} className="art-ring-ray" x1="20" y1="-2" x2="20" y2="-3.5" stroke="var(--sage)" transform={`rotate(${a} 20 20)`} />
          ))}
      </svg>
      <div className="art-ring-rail" aria-hidden="true">
        {segs.map((s, i) => (
          <span key={i} className="art-ring-gap" style={{ left: `${s.left}%`, width: `${s.width}%` }} />
        ))}
      </div>
    </div>
  );
}
