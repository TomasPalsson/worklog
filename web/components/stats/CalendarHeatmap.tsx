import "@/app/stats/charts-time.css";
import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { dayLabel, dowMon, MONTHS, parseDay, WEEKDAYS } from "./time-utils";

const CELL = 12;
const PITCH = 15;
const LEFT = 26;
const TOP = 16;
const MIX = [0, 25, 45, 70, 100];

/** 0 = no work, 1..4 = quartiles of the busiest day. */
export function level(seconds: number, max: number): number {
  if (!(seconds > 0) || !(max > 0)) return 0;
  const r = seconds / max;
  return r <= 0.25 ? 1 : r <= 0.5 ? 2 : r <= 0.75 ? 3 : 4;
}

export const fill = (lv: number) =>
  lv === 0 ? "var(--bg-sunk)" : `color-mix(in oklch, var(--sage) ${MIX[lv]}%, var(--bg-sunk))`;

export function layout(daily: DailyStat[]) {
  const off = daily.length ? dowMon(daily[0].day) : 0;
  return daily.map((d, i) => ({ d, col: Math.floor((i + off) / 7), row: (i + off) % 7 }));
}

function monthLabels(cells: ReturnType<typeof layout>) {
  const months: { col: number; text: string }[] = [];
  let lastCol = -9;
  let lastMonth = -1;
  for (const { d, col } of cells) {
    const m = parseDay(d.day).getUTCMonth();
    if (m !== lastMonth && col - lastCol >= 3) {
      months.push({ col, text: MONTHS[m] });
      lastCol = col;
    }
    lastMonth = m;
  }
  return months;
}

function Cells({ cells, max, now }: { cells: ReturnType<typeof layout>; max: number; now: string }) {
  return (
    <>
    {cells.map(({ d, col, row }) => {
      const x = LEFT + col * PITCH;
      const yy = TOP + row * PITCH;
      return (
        <g key={d.day}>
          <rect
            className="st-cell art-fade"
            x={x}
            y={yy}
            width={CELL}
            height={CELL}
            rx={2.5}
            fill={fill(level(d.work_seconds, max))}
            style={{ animationDelay: `${Math.min(col * 12, 700)}ms` }}
          >
            <title>{`${dayLabel(d.day)} · ${d.work_seconds > 0 ? formatDuration(d.work_seconds) : "no work"}`}</title>
          </rect>
          {d.day === now && <rect className="st-today" x={x - 1} y={yy - 1} width={CELL + 2} height={CELL + 2} rx={3.5} />}
        </g>
      );
    })}
    </>
  );
}

export function CalendarHeatmap({ daily, today }: { daily: DailyStat[]; today?: string }) {
  const max = Math.max(0, ...daily.map((d) => d.work_seconds));
  if (daily.length === 0 || max === 0) {
    return (
      <div className="st-chart">
        <div className="st-empty">No data yet. The calendar lights up as you log work days.</div>
      </div>
    );
  }
  const now = today ?? new Date().toISOString().slice(0, 10);
  const cells = layout(daily);
  const cols = cells[cells.length - 1].col + 1;
  const W = LEFT + cols * PITCH;
  const H = TOP + 7 * PITCH;
  const worked = daily.filter((d) => d.work_seconds > 0).length;
  const months = monthLabels(cells);
  return (
    <div className="st-chart">
      <div className="st-big">
        {worked} <span style={{ fontSize: 14, color: "var(--fg-muted)" }}>of {daily.length} days worked</span>
      </div>
      <div className="st-scroll" style={{ marginTop: 12 }}>
        <svg
          className="st-svg"
          width={W}
          height={H}
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-label={`Calendar of ${daily.length} days, ${worked} with work. Busiest day ${formatDuration(max)}.`}
        >
          {months.map((m) => (
            <text key={m.col} x={LEFT + m.col * PITCH} y={10} className="art-label">
              {m.text}
            </text>
          ))}
          {[0, 2, 4].map((r) => (
            <text key={r} x={0} y={TOP + r * PITCH + CELL - 2} className="art-label">
              {WEEKDAYS[r]}
            </text>
          ))}
          <Cells cells={cells} max={max} now={now} />
        </svg>
      </div>
      <div className="st-scale" aria-hidden="true">
        less
        {[0, 1, 2, 3, 4].map((lv) => (
          <span key={lv} className="st-step" style={{ background: fill(lv) }} />
        ))}
        more
      </div>
    </div>
  );
}
