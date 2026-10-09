import { useId } from "react";
import "@/app/stats/charts-time.css";
import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { dayLabel, shortLabel } from "./time-utils";
import { tipProps, type Tip } from "./tip";
import { ordinal, rankOf, times } from "./chart-tip-utils";

const LEFT = 36;
const TOP = 10;
const PLOT = 170;
const BOT = 18;
const W = 1000;
const H = TOP + PLOT + BOT;
export const TARGET_H = 7.5;

const hours = (s: number) => Math.max(0, s || 0) / 3600;

/** Y-axis maths: whole-hour ceiling that always shows the target, plus a tidy tick step. */
export function scale(daily: DailyStat[]) {
  const peak = Math.max(0, ...daily.map((d) => hours(d.work_seconds) + hours(d.personal_seconds) + hours(d.ignored_seconds)));
  const top = Math.max(Math.ceil(peak), Math.ceil(TARGET_H));
  const step = Math.max(1, Math.ceil(top / 4));
  return { top, step };
}

/** Hover card for one day's stacked bar. */
export function dayTip(d: DailyStat, all: DailyStat[]): Tip {
  const total = d.work_seconds + d.personal_seconds + d.ignored_seconds;
  const worked = all.filter((x) => x.work_seconds > 0);
  const avg = worked.length ? worked.reduce((a, x) => a + x.work_seconds, 0) / worked.length : 0;
  const target = TARGET_H * 3600;
  const diff = d.work_seconds - target;
  const vs = Math.abs(diff) < 30 ? "on target" : diff > 0 ? `+${formatDuration(diff)} over` : `${formatDuration(-diff)} under`;
  let note: string;
  if (total <= 0) note = "Nothing tracked this day.";
  else if (d.work_seconds <= 0) note = "No work logged, only personal or ignored time.";
  else if (d.personal_seconds > d.work_seconds) note = "More personal time than work today.";
  else if (diff > 0) note = `Cleared the ${TARGET_H}h target by ${formatDuration(diff)}.`;
  else note = `${times(d.work_seconds / avg)} your average worked day.`;
  return {
    title: dayLabel(d.day),
    sub: d.work_seconds > 0 ? `${ordinal(rankOf(d.work_seconds, all.map((x) => x.work_seconds)))} most work of ${all.length} days` : `no work, ${all.length} days shown`,
    rows: [
      ["Work", formatDuration(d.work_seconds)],
      ["Personal", formatDuration(d.personal_seconds)],
      ["Ignored", formatDuration(d.ignored_seconds)],
      ["Total", formatDuration(total)],
      [`vs ${TARGET_H}h target`, vs],
    ],
    bar: { value: d.work_seconds, max: target, label: `${Math.round((d.work_seconds / target) * 100)}% of the ${TARGET_H}h target` },
    note,
    accent: "var(--sage)",
  };
}

function Bars({ daily, top, slot, hatch }: { daily: DailyStat[]; top: number; slot: number; hatch: string }) {
  const yv = (h: number) => (h / top) * PLOT;
  const bw = Math.max(1, slot * 0.72);
  return (
    <>
      {daily.map((d, i) => {
        const parts = [
          [hours(d.work_seconds), "var(--sage)"],
          [hours(d.personal_seconds), "var(--violet)"],
          [hours(d.ignored_seconds), `url(#${hatch})`],
        ] as const;
        let acc = 0;
        return (
          <g
            key={d.day}
            className="st-daybar"
            {...tipProps(dayTip(d, daily))}
            {...(d.work_seconds + d.personal_seconds + d.ignored_seconds > 0
              ? { tabIndex: 0, "aria-label": `${dayLabel(d.day)}: ${formatDuration(d.work_seconds)} work` }
              : {})}
          >
            <rect className="st-hit" x={LEFT + i * slot} y={TOP} width={slot} height={PLOT} />
            <g className="art-grow-y" style={{ animationDelay: `${Math.min(i * 14, 600)}ms` }}>
            {parts.map(([h, c], k) => {
              const rect = (
                <rect key={k} x={LEFT + i * slot + (slot - bw) / 2} y={TOP + PLOT - yv(acc + h)} width={bw} height={yv(h)} fill={c} />
              );
              acc += h;
              return h > 0 ? rect : null;
            })}
            </g>
          </g>
        );
      })}
    </>
  );
}

function Legend({ w, p, i }: { w: number; p: number; i: number }) {
  return (
    <ul className="st-legend">
      <li><span className="st-swatch" style={{ background: "var(--sage)" }} />work <span className="st-num">{formatDuration(w)}</span></li>
      <li><span className="st-swatch" style={{ background: "var(--violet)" }} />personal <span className="st-num">{formatDuration(p)}</span></li>
      <li><span className="st-swatch st-swatch-hatch" />ignored <span className="st-num">{formatDuration(i)}</span></li>
    </ul>
  );
}

function Ticks({ ticks, top }: { ticks: number[]; top: number }) {
  return (
    <>
      {ticks.map((t) => {
        const yy = TOP + PLOT - (t / top) * PLOT;
        return (
          <g key={t}>
            <line x1={LEFT} x2={W - 6} y1={yy} y2={yy} className="st-grid" />
            <text x={LEFT - 5} y={yy + 3} textAnchor="end" className="art-label">
              {t}h
            </text>
          </g>
        );
      })}
    </>
  );
}

export function DailyStack({ daily }: { daily: DailyStat[] }) {
  const hatch = `st-hatch-${useId().replace(/:/g, "")}`;
  const sum = (k: "work_seconds" | "personal_seconds" | "ignored_seconds") => daily.reduce((a, d) => a + (d[k] || 0), 0);
  const total = sum("work_seconds") + sum("personal_seconds") + sum("ignored_seconds");
  if (daily.length === 0 || total <= 0) {
    return (
      <div className="st-chart">
        <div className="st-empty">No data yet. Bars appear once a day has tracked time.</div>
      </div>
    );
  }
  const { top, step } = scale(daily);
  const slot = (W - LEFT - 6) / daily.length;
  const over = daily.filter((d) => hours(d.work_seconds) > TARGET_H).length;
  const ty = TOP + PLOT - (TARGET_H / top) * PLOT;
  const ticks = Array.from({ length: Math.floor(top / step) + 1 }, (_, i) => i * step);
  const labelAt = Array.from(new Set([0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(f * (daily.length - 1)))));
  return (
    <div className="st-chart">
      <div className="st-big">{formatDuration(sum("work_seconds"))}</div>
      <div className="st-scroll" style={{ marginTop: 12 }}>
        <svg
          className="st-svg"
          viewBox={`0 0 ${W} ${H}`}
          style={{ width: "100%", minWidth: 600 }}
          role="img"
          aria-label={`Daily time over ${daily.length} days: ${formatDuration(sum("work_seconds"))} work, ${formatDuration(sum("personal_seconds"))} personal, ${formatDuration(sum("ignored_seconds"))} ignored. ${over} days above ${TARGET_H} hours.`}
        >
          <defs>
            <pattern id={hatch} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="6" height="6" fill="var(--bg-sunk)" />
              <line x1="0" y1="0" x2="0" y2="6" stroke="var(--border-strong)" strokeWidth="3" />
            </pattern>
          </defs>
          <Ticks ticks={ticks} top={top} />
          <Bars daily={daily} top={top} slot={slot} hatch={hatch} />
          <line x1={LEFT} x2={W - 6} y1={ty} y2={ty} stroke="var(--amber)" style={{ strokeDasharray: "5 4", strokeWidth: 1.3 }} />
          <text x={W - 8} y={ty - 4} textAnchor="end" className="art-label" style={{ fill: "var(--amber-ink)" }}>
            {`${TARGET_H}h target`}
          </text>
          {labelAt.map((i) => (
            <text
              key={i}
              x={LEFT + i * slot + slot / 2}
              y={H - 4}
              textAnchor={i === 0 ? "start" : i === daily.length - 1 ? "end" : "middle"}
              className="art-label"
             
            >
              {shortLabel(daily[i].day)}
            </text>
          ))}
        </svg>
      </div>
      <Legend w={sum("work_seconds")} p={sum("personal_seconds")} i={sum("ignored_seconds")} />
      <p className="st-caption">
        {over === 0 ? "No day cleared the target yet." : <>You cleared <b>{TARGET_H}h</b> of work on <b>{over}</b> {over === 1 ? "day" : "days"}.</>}
      </p>
    </div>
  );
}
