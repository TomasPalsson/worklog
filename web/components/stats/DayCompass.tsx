import "@/app/stats/charts-ranked.css";
import "@/app/stats/compass.css";
import { formatDuration, shortMonthDay, shortWeekday } from "@/lib/format";
import type { DayRow, TopicsReport } from "@/lib/topics_contract";
import { tipProps, type Tip } from "./tip";
import { topicColor } from "./topicColor";

const WIDTH = 480;
const HEIGHT = 320;
const PAD_X = 64;
const PAD_Y = 40;
const MAX_RADIUS = 12;
const LEADER_X = 20;
const LEADER_Y = 18;
const CHAR_W = 6.2;
const EDGE = 20;
const LABEL_H = 10;
const LABEL_TOP = 24;
const LABEL_BOTTOM = 296;

type Quadrant = "deep-new" | "deep-familiar" | "scattered-exploring" | "scattered-routine";

export interface CompassDot {
  day: string;
  x: number;
  y: number;
  radius: number;
  quadrant: Quadrant;
  labelled: boolean;
  topTopic: number;
}

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};


export function compassLayout(report: TopicsReport) {
  const plottedDays = report.days.filter((d): d is DayRow & { focus: number } => d.focus !== null);
  const skipped = report.days.filter((d) => d.focus === null).map((d) => d.day);
  const focusValues = plottedDays.map((d) => d.focus);
  const noveltyValues = plottedDays.map((d) => d.novelty);
  const range = (values: number[]) => [Math.min(...values), Math.max(...values)];
  const [focusLow, focusHigh] = range(focusValues);
  const [noveltyLow, noveltyHigh] = range(noveltyValues);
  const plotWidth = WIDTH - 2 * PAD_X;
  const plotHeight = HEIGHT - 2 * PAD_Y;
  const scaleX = (focus: number) => PAD_X + (focusHigh > focusLow ? ((focus - focusLow) / (focusHigh - focusLow)) * plotWidth : plotWidth / 2);
  const scaleY = (novelty: number) =>
    HEIGHT - PAD_Y - (noveltyHigh > noveltyLow ? ((novelty - noveltyLow) / (noveltyHigh - noveltyLow)) * plotHeight : plotHeight / 2);
  const medianFocus = plottedDays.length ? median(focusValues) : 0;
  const medianNovelty = plottedDays.length ? median(noveltyValues) : 0;
  const medianX = scaleX(medianFocus);
  const medianY = scaleY(medianNovelty);
  const mostSeconds = Math.max(0, ...plottedDays.map((d) => d.work_seconds));

  const plotted: CompassDot[] = plottedDays.map((d) => {
    const deep = d.focus >= medianFocus;
    const fresh = d.novelty >= medianNovelty;
    return {
      day: d.day,
      x: scaleX(d.focus),
      y: scaleY(d.novelty),
      radius: mostSeconds > 0 ? MAX_RADIUS * Math.sqrt(d.work_seconds / mostSeconds) : 0,
      quadrant: deep ? (fresh ? "deep-new" : "deep-familiar") : fresh ? "scattered-exploring" : "scattered-routine",
      labelled: false,
      topTopic: d.top_topic,
    };
  });
  const reach = (dot: CompassDot) => Math.hypot((dot.x - medianX) / plotWidth, (dot.y - medianY) / plotHeight);
  const farthest = new Map<Quadrant, CompassDot>();
  for (const dot of plotted) {
    const best = farthest.get(dot.quadrant);
    if (!best || reach(dot) > reach(best)) farthest.set(dot.quadrant, dot);
  }
  for (const dot of farthest.values()) dot.labelled = true;
  const hours = new Map(plottedDays.map((d) => [d.day, d.work_seconds]));
  const busiest = plotted.filter((d) => !d.labelled).reduce<CompassDot | null>((a, b) => (!a || hours.get(b.day)! > hours.get(a.day)! ? b : a), null);
  if (busiest) busiest.labelled = true;
  return { plotted, skipped, medianX, medianY };
}

// Flip to the inner side when the text would cross the plot edge; clamp so the box stays in the plot.
function leaderPlacement(dot: CompassDot) {
  const width = dayName(dot.day).length * CHAR_W;
  let sideX = dot.quadrant.startsWith("deep") ? 1 : -1;
  const reachX = (side: number) => dot.x + side * (LEADER_X + 3 + width);
  if (reachX(sideX) > WIDTH - EDGE || reachX(sideX) < EDGE) sideX = -sideX;
  const sideY = dot.quadrant.endsWith("new") || dot.quadrant.endsWith("exploring") ? -1 : 1;
  const drop = sideY < 0 ? 0 : LABEL_H;
  const labelY = Math.min(LABEL_BOTTOM, Math.max(LABEL_TOP + LABEL_H, dot.y + sideY * LEADER_Y + drop));
  return { sideX, endX: dot.x + sideX * LEADER_X, endY: labelY - drop, labelY };
}

const dayName = (day: string) => `${shortWeekday(day)} ${shortMonthDay(day)}`;

export function compassTip(day: DayRow): Tip {
  return {
    title: dayName(day.day),
    rows: [
      ["Hours", formatDuration(day.work_seconds)],
      ["Focus", day.focus === null ? "–" : day.focus.toFixed(2)],
      ["Newness", day.novelty.toFixed(2)],
      ["Switches", String(day.switches)],
      ["Longest run", formatDuration(day.longest_run_seconds)],
      ["Topics touched", String(day.topics_touched)],
    ],
  };
}

const KEY_MAX = 5;

export function DayCompass({ report }: { report: TopicsReport }) {
  const { plotted, skipped, medianX, medianY } = compassLayout(report);
  if (plotted.length === 0) return null;
  const dayRows = new Map(report.days.map((d) => [d.day, d]));
  const daysByTopic = new Map<number, number>();
  for (const dot of plotted) daysByTopic.set(dot.topTopic, (daysByTopic.get(dot.topTopic) ?? 0) + 1);
  const keyed = [...daysByTopic].sort((a, b) => b[1] - a[1] || a[0] - b[0]).slice(0, KEY_MAX);
  const plottedRows = plotted.map((dot) => dayRows.get(dot.day)!);
  const best = (value: (d: DayRow) => number) => plottedRows.reduce((a, b) => (value(b) > value(a) ? b : a));

  return (
    <section className="sr-card sx-cmp">
      <h3 className="sr-title">Day compass</h3>
      <p className="sx-cmp-callout">
        Most focused: {dayName(best((d) => d.focus!).day)} · Most new ground: {dayName(best((d) => d.novelty).day)}
      </p>
      <svg className="sx-cmp-svg" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label="Focus against novelty, one dot per day">
        <line className="sx-cmp-cross" x1={medianX} x2={medianX} y1={PAD_Y / 2} y2={HEIGHT - PAD_Y / 2} />
        <line className="sx-cmp-cross" x1={PAD_X / 2} x2={WIDTH - PAD_X / 2} y1={medianY} y2={medianY} />
        <text className="art-label sx-cmp-axis" x={WIDTH / 2} y={HEIGHT - 8} textAnchor="middle">
          Focus →
        </text>
        <text className="art-label sx-cmp-axis" transform={`translate(12 ${HEIGHT / 2}) rotate(-90)`} textAnchor="middle">
          Newness ↑
        </text>
        <text className="art-label sx-cmp-end" x={PAD_X} y={HEIGHT - 8} textAnchor="start">
          scattered
        </text>
        <text className="art-label sx-cmp-end" x={WIDTH - PAD_X} y={HEIGHT - 8} textAnchor="end">
          focused
        </text>
        <text className="art-label sx-cmp-end" x={8} y={HEIGHT - 22} textAnchor="start">
          familiar
        </text>
        <text className="art-label sx-cmp-end" x={8} y={16} textAnchor="start">
          new ground
        </text>
        {plotted.map((dot) => (
          <circle
            key={dot.day}
            className="sx-cmp-dot"
            cx={dot.x}
            cy={dot.y}
            r={dot.radius}
            stroke="var(--bg-raised)"
            strokeWidth={1.5}
            style={{ fill: topicColor(dot.topTopic, report.topics.length) }}
            tabIndex={0}
            aria-label={dayName(dot.day)}
            {...tipProps(compassTip(dayRows.get(dot.day)!))}
          />
        ))}
        {plotted
          .filter((dot) => dot.labelled)
          .map((dot) => {
            const { sideX, endX, endY, labelY } = leaderPlacement(dot);
            return (
              <g key={dot.day}>
                <line className="sx-cmp-leader" x1={dot.x} y1={dot.y} x2={endX} y2={endY} />
                <text className="art-label sx-cmp-leader-label" x={endX + sideX * 3} y={labelY} textAnchor={sideX > 0 ? "start" : "end"}>
                  {dayName(dot.day)}
                </text>
              </g>
            );
          })}
      </svg>
      <p className="sx-cmp-key">
        dot colour = that day&apos;s main topic
        {keyed.map(([id]) => (
          <span key={id} className="sx-cmp-key-item">
            <span className="sx-cmp-key-swatch" style={{ background: topicColor(id, report.topics.length) }} />
            {report.topics.find((t) => t.id === id)?.name ?? ""}
          </span>
        ))}
      </p>
      {skipped.length > 0 && <p className="sx-cmp-skipped">single-block days: {skipped.map(shortMonthDay).join(", ")}</p>}
      <div className="sx-cmp-how">
        How it works: Focus is how alike a day&apos;s blocks are. Newness is how unlike anything earlier they are. Dashed lines are your medians.
        <details>
          <summary>Method</summary>
          Focus = how alike that day&apos;s blocks are (hours-weighted mean pairwise cosine). Newness = how unlike anything you did before (1 − nearest earlier block).
        </details>
      </div>
    </section>
  );
}
