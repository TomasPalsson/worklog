import "@/app/stats/river.css";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { bandPath } from "./ActivityStream";
import { dayLabel, shortLabel } from "./time-utils";
import { tipProps, type Tip } from "./tip";

const W = 1000;
const H = 240;
const PAD = 6;
const MIN_LABEL_PX = 14;
const COL_TIP_ROWS = 4;

type Pt = [number, number];

export interface RiverLayer {
  topic: Topic;
  color: string;
  total: number;
  peak: number;
  peakStart: string;
  active: number;
  upper: Pt[];
  lower: Pt[];
  path: string;
  /** Band thickness in viewBox px at its widest bin. */
  thick: number;
  labelled: boolean;
  labelX: number;
  labelY: number;
}

const hours = (s: number) => `${(s / 3600).toFixed(1)}h`;
const colorOf = (id: number, k: number) => `oklch(var(--project-l) var(--project-c) ${25 + id * (360 / k)})`;

/** Stacked, centred-baseline layers; one shared scale so thickness is proportional to seconds. */
export function buildRiver(report: TopicsReport): { layers: RiverLayer[]; unit: number; xs: number[] } | null {
  const { river, topics } = report;
  const n = river.length;
  if (n < 2) return null;
  const tot = river.map((b) => b.seconds.reduce((a, s) => a + s, 0));
  const maxTot = Math.max(...tot);
  if (maxTot <= 0) return null;
  const unit = (H - 2 * PAD) / maxTot;
  const xs = river.map((_, i) => PAD + (i / (n - 1)) * (W - 2 * PAD));
  const firstActive = (id: number) => river.findIndex((b) => b.seconds[id] > 0);
  const order = topics
    .filter((t) => firstActive(t.id) >= 0)
    .sort((a, b) => firstActive(a.id) - firstActive(b.id) || a.id - b.id);
  const edge = tot.map((t) => H / 2 + (t * unit) / 2);
  const layers = order.map((t): RiverLayer => {
    const sec = river.map((b) => b.seconds[t.id] ?? 0);
    const lower: Pt[] = xs.map((x, i) => [x, edge[i]]);
    sec.forEach((s, i) => (edge[i] -= s * unit));
    const upper: Pt[] = xs.map((x, i) => [x, edge[i]]);
    const peak = Math.max(...sec);
    const pi = sec.indexOf(peak);
    const thick = peak * unit;
    return {
      topic: t,
      color: colorOf(t.id, topics.length),
      total: sec.reduce((a, s) => a + s, 0),
      peak,
      peakStart: river[pi].start,
      active: sec.filter((s) => s > 0).length,
      upper,
      lower,
      path: bandPath(upper, lower),
      thick,
      labelled: thick >= MIN_LABEL_PX,
      labelX: Math.min(W - 70, Math.max(70, xs[pi])),
      labelY: (upper[pi][1] + lower[pi][1]) / 2,
    };
  });
  return { layers, unit, xs };
}

function bandTip(l: RiverLayer, bins: number): Tip {
  return {
    title: l.topic.name,
    rows: [
      ["Total", hours(l.total)],
      ["Peak bin", `${hours(l.peak)} on ${shortLabel(l.peakStart)}`],
      ["Active bins", `${l.active} of ${bins}`],
    ],
    accent: l.color,
  };
}

function columnTip(report: TopicsReport, i: number): Tip {
  const { start, seconds } = report.river[i];
  return {
    title: dayLabel(start),
    rows: report.topics
      .map((t) => [t.name, seconds[t.id] ?? 0] as const)
      .filter(([, s]) => s > 0)
      .sort((a, b) => b[1] - a[1])
      .slice(0, COL_TIP_ROWS)
      .map(([name, s]) => [name, hours(s)]),
  };
}

export function TopicRiver({ report }: { report: TopicsReport }) {
  const built = buildRiver(report);
  if (!built) return null;
  const { layers, xs } = built;
  const n = report.river.length;
  const step = (W - 2 * PAD) / (n - 1);
  const first = report.river[0].start;
  const last = report.river[n - 1].start;
  return (
    <div className="st-chart sx-river">
      <div className="st-scroll">
        <svg
          className="st-svg"
          viewBox={`0 0 ${W} ${H}`}
          style={{ width: "100%", minWidth: 600, maxHeight: 300 }}
          role="img"
          aria-label={`Topic river over ${n} bins: ${layers.map((l) => `${l.topic.name} ${hours(l.total)}`).join(", ")}.`}
        >
          {report.river.map((b, i) => (
            <rect
              key={b.start}
              className="sx-river-col"
              x={Math.max(0, xs[i] - step / 2)}
              y={0}
              width={Math.min(W, xs[i] + step / 2) - Math.max(0, xs[i] - step / 2)}
              height={H}
              {...tipProps(columnTip(report, i))}
            />
          ))}
          {layers.map((l) => (
            <path
              key={l.topic.id}
              className="sx-river-band art-grow-y"
              d={l.path}
              fill={l.color}
              tabIndex={0}
              aria-label={`${l.topic.name}: ${hours(l.total)}`}
              {...tipProps(bandTip(l, n))}
            />
          ))}
          {layers.filter((l) => l.labelled).map((l) => (
            <text key={l.topic.id} className="sx-river-name" x={l.labelX} y={l.labelY} textAnchor="middle" dominantBaseline="central">
              {l.topic.name}
            </text>
          ))}
          <text x={PAD} y={H - 4} className="art-label">{shortLabel(first)}</text>
          <text x={W - PAD} y={H - 4} textAnchor="end" className="art-label">{shortLabel(last)}</text>
        </svg>
      </div>
      <p className="st-caption">How: Hours per topic per day (per week when the range is longer than 90 days). Topics are fitted once over the whole range.</p>
    </div>
  );
}
