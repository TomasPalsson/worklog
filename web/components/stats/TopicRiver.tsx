import "@/app/stats/river.css";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { bandPath } from "./ActivityStream";
import { shortMonthDay } from "@/lib/format";
import { dayLabel, dowMon, shortLabel } from "./time-utils";
import { tipProps, type Tip } from "./tip";
import { topicColor } from "./topicColor";

const W = 1000;
const H = 240;
const PAD = 6;
const GUTTER = 60;
const WEEK_TICK_EVERY = 4;
const MIN_LABEL_PX = 18;
const EDGE = 8;
const COL_TIP_ROWS = 4;
const CHAR_PX = 7.2;
const LABEL_H = 12;
const LABEL_PAD = 4;

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
  /** Band thickness at labelX, where the text sits. */
  labelThick: number;
}

export const labelBox = (l: RiverLayer) => {
  const w = l.topic.name.length * CHAR_PX;
  return { x: l.labelX - w / 2, y: l.labelY - LABEL_H / 2, w, h: LABEL_H };
};
const overlaps = (a: ReturnType<typeof labelBox>, b: ReturnType<typeof labelBox>) =>
  a.x < b.x + b.w + LABEL_PAD && b.x < a.x + a.w + LABEL_PAD && a.y < b.y + b.h + LABEL_PAD && b.y < a.y + a.h + LABEL_PAD;

const hours = (s: number) => `${(s / 3600).toFixed(1)}h`;
const scaleHours = (s: number) => `${Number((s / 3600).toFixed(1))}h`;

/** Stacked, centred-baseline layers; one shared scale so thickness is proportional to seconds. */
export function buildRiver(report: TopicsReport): { layers: RiverLayer[]; unit: number; xs: number[]; bins: TopicsReport["river"] } | null {
  const { topics } = report;
  const sums = report.river.map((b) => b.seconds.reduce((a, s) => a + s, 0));
  const river = report.river.slice(Math.max(0, sums.findIndex((t) => t > 0)), sums.findLastIndex((t) => t > 0) + 1);
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
    const w = t.name.length * CHAR_PX;
    const lx = Math.min(W - w / 2 - EDGE, Math.max(w / 2 + EDGE, xs[pi]));
    const k = Math.min(n - 2, Math.max(0, Math.floor((lx - PAD) / (xs[1] - xs[0]))));
    const f = (lx - xs[k]) / (xs[k + 1] - xs[k]);
    const at = (pts: Pt[]) => pts[k][1] + (pts[k + 1][1] - pts[k][1]) * f;
    return {
      topic: t,
      color: topicColor(t.id, topics.length),
      total: sec.reduce((a, s) => a + s, 0),
      peak,
      peakStart: river[pi].start,
      active: sec.filter((s) => s > 0).length,
      upper,
      lower,
      path: bandPath(upper, lower),
      thick,
      labelled: false,
      labelX: lx,
      labelY: (at(upper) + at(lower)) / 2,
      labelThick: at(lower) - at(upper),
    };
  });
  const placed: ReturnType<typeof labelBox>[] = [];
  [...layers].sort((a, b) => b.total - a.total).forEach((l) => {
    const box = labelBox(l);
    if (l.labelThick < MIN_LABEL_PX || box.w > W - 2 * EDGE || box.y < 0 || box.y + box.h > H || placed.some((p) => overlaps(box, p))) return;
    l.labelled = true;
    placed.push(box);
  });
  return { layers, unit, xs, bins: river };
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

function columnTip(report: TopicsReport, bins: TopicsReport["river"], i: number): Tip {
  const { start, seconds } = bins[i];
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
  const { layers, unit, xs, bins } = built;
  const n = bins.length;
  const step = (W - 2 * PAD) / (n - 1);
  const ticks = bins.flatMap((b, i) => ((report.bin === "week" ? i % WEEK_TICK_EVERY === 0 : dowMon(b.start) === 0) ? [{ x: xs[i], label: shortMonthDay(b.start) }] : []));
  const stack = H - 2 * PAD;
  const bx = W + 10;
  return (
    <section className="sr-card sx-river">
      <h3 className="sr-title">Topic river</h3>
      <div className="st-scroll">
        <svg
          className="st-svg"
          viewBox={`0 0 ${W + GUTTER} ${H}`}
          style={{ width: "100%", minWidth: 720, maxHeight: 300 }}
          role="img"
          aria-label={`Topic river over ${n} bins: ${layers.map((l) => `${l.topic.name} ${hours(l.total)}`).join(", ")}.`}
        >
          {bins.map((b, i) => (
            <rect
              key={b.start}
              className="sx-river-col"
              x={Math.max(0, xs[i] - step / 2)}
              y={0}
              width={Math.min(W, xs[i] + step / 2) - Math.max(0, xs[i] - step / 2)}
              height={H}
              {...tipProps(columnTip(report, bins, i))}
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
          <line className="sx-river-axis" x1={PAD} x2={W - PAD} y1={H - 1} y2={H - 1} />
          {ticks.map((t) => (
            <g key={t.x}>
              <line className="sx-river-tick" x1={t.x} x2={t.x} y1={H - 6} y2={H - 1} />
              <text className="art-label sx-river-tick-label" x={t.x + 3} y={H - 8}>{t.label}</text>
            </g>
          ))}
          <path className="sx-river-bracket" d={`M${bx - 4} ${PAD}H${bx}V${PAD + stack}H${bx - 4}`} />
          <text className="art-label sx-river-scale" x={bx + 4} y={H / 2} dominantBaseline="central">{scaleHours(stack / unit)} / {report.bin}</text>
        </svg>
      </div>
      <p className="st-caption">How it works: Hours per topic per day (per week when the range is longer than 90 days). Topics are fitted once over the whole range.</p>
    </section>
  );
}
