import "@/app/stats/galaxy.css";
import { useId } from "react";
import { formatDuration } from "@/lib/format";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { AtlasEmpty, FirstTimeMark, OddballMark } from "./AtlasArt";
import { type Tip, tipProps } from "./tip";

export const GAL_W = 1000;
export const GAL_H = 620;
const PAD = 40;
const NEBULA_R = 26;
const NUDGE_ITERATIONS = 20;
const MAX_DELAY_MS = 1200;
const NAME_CHAR_W = 0.62; // mono advance per font-size, including letter-spacing
const LABEL_GAP = 2; // breathing room kept between nudged labels
const LABEL_LINE_GAP = 14; // hours line under the name

const HOW =
  "How: Each dot is a work block. The local embedding model turns its text into a vector, the vectors are centered, and t-SNE (perplexity 30) lays them out so similar work sits close together. Topics come from average-linkage clustering. Names: AI (Haiku) from keywords plus examples, or keywords if no model is available. Distance between islands is only roughly meaningful.";

const hoursLabel = (sec: number) => `${(sec / 3600).toFixed(1)}h`;
const hue = (i: number, n: number) =>
  `oklch(var(--project-l) var(--project-c) ${Math.round((i * 360) / Math.max(1, n) + 25) % 360})`;

export interface GalaxyDot {
  point: MapPoint;
  cx: number;
  cy: number;
  r: number;
  fill: string;
  rank: number;
  delayMs: number;
}

export interface GalaxyLabel {
  topic: Topic;
  x: number;
  y: number;
  fontSize: number;
  w: number;
  h: number;
}

export interface GalaxyLayout {
  dots: GalaxyDot[];
  labels: GalaxyLabel[];
}

function weightedMedian(vals: { v: number; w: number }[]): number {
  const sorted = [...vals].sort((a, b) => a.v - b.v);
  const half = sorted.reduce((s, e) => s + e.w, 0) / 2;
  let acc = 0;
  for (const e of sorted) {
    acc += e.w;
    if (acc >= half) return e.v;
  }
  return sorted[0].v;
}

/** Pure placement of dots and labels in the 1000x620 viewBox. */
export function galaxyLayout(report: Pick<TopicsReport, "topics" | "points">): GalaxyLayout {
  const { topics } = report;
  const index = new Map(topics.map((t, i) => [t.id, i]));
  const points = report.points.filter((p) => index.has(p.topic_id));
  const maxSec = Math.max(1, ...points.map((p) => p.seconds));
  const order = [...points].sort((a, b) => a.day.localeCompare(b.day) || a.block_id - b.block_id);
  const rankOf = new Map(order.map((p, i) => [p.block_id, i]));

  const dots = points.map((p) => {
    const rank = rankOf.get(p.block_id) ?? 0;
    return {
      point: p,
      cx: PAD + p.x * (GAL_W - 2 * PAD),
      cy: PAD + p.y * (GAL_H - 2 * PAD),
      r: 2.5 + 6 * Math.sqrt(p.seconds / maxSec),
      fill: hue(index.get(p.topic_id) ?? 0, topics.length),
      rank,
      delayMs: Math.min(rank * 3, MAX_DELAY_MS),
    };
  });

  const totalSec = topics.reduce((s, t) => s + t.seconds, 0);
  const labels: GalaxyLabel[] = [];
  for (const topic of topics) {
    const mine = dots.filter((d) => d.point.topic_id === topic.id);
    if (mine.length === 0) continue;
    const wts = (f: (d: GalaxyDot) => number) => mine.map((d) => ({ v: f(d), w: d.point.seconds + 1 }));
    const fontSize = 11 + 9 * Math.sqrt(totalSec > 0 ? topic.seconds / totalSec : 0);
    labels.push({
      topic,
      x: weightedMedian(wts((d) => d.cx)),
      y: weightedMedian(wts((d) => d.cy)),
      fontSize,
      w: topic.name.length * fontSize * NAME_CHAR_W,
      h: fontSize + LABEL_LINE_GAP,
    });
  }

  for (let it = 0; it < NUDGE_ITERATIONS; it++) {
    const byY = [...labels].sort((p, q) => p.y - q.y);
    for (let i = 0; i < byY.length; i++) {
      for (let j = i + 1; j < byY.length; j++) {
        const a = byY[i];
        const b = byY[j];
        if (Math.abs(a.x - b.x) >= (a.w + b.w) / 2) continue;
        const overlap = (a.h + b.h) / 2 - Math.abs(a.y - b.y);
        if (overlap <= 0) continue;
        const dir = a.y <= b.y ? 1 : -1;
        const push = (overlap + LABEL_GAP) / 2;
        a.y -= dir * push;
        b.y += dir * push;
      }
    }
    for (const l of labels) l.y = Math.min(GAL_H - l.h / 2, Math.max(l.h / 2, l.y));
  }
  return { dots, labels };
}

export function galaxyTipDot(p: MapPoint, topicName: string): Tip {
  return {
    title: p.label,
    rows: [
      ["Day", p.day],
      ["Hours", formatDuration(p.seconds)],
      ["Ticket", p.ticket ?? "none"],
      ["Topic", topicName],
      ["Novelty", `${Math.round(p.novelty * 100)}%`],
    ],
  };
}

export function galaxyTipLabel(t: Topic): Tip {
  return {
    title: t.name,
    ...(t.description ? { sub: t.description } : {}),
    rows: [
      ["Keywords", t.keywords.length ? t.keywords.join(", ") : "none"],
      ["Hours", hoursLabel(t.seconds)],
      ["Blocks", String(t.blocks)],
    ],
  };
}

function Nebula({ report, dots }: { report: TopicsReport; dots: GalaxyDot[] }) {
  return (
    <>
      {report.topics.map((t) => (
        <g key={t.id} className="sx-gal-neb" data-t={t.id} opacity={0.22}>
          {dots
            .filter((d) => d.point.topic_id === t.id)
            .map((d) => (
              <circle key={d.point.block_id} cx={d.cx} cy={d.cy} r={NEBULA_R} fill={d.fill} />
            ))}
        </g>
      ))}
    </>
  );
}

function Star({ d, topicName }: { d: GalaxyDot; topicName: string }) {
  const p = d.point;
  return (
    <g>
      <circle
        className="sx-gal-dot art-fade"
        data-t={p.topic_id}
        cx={d.cx}
        cy={d.cy}
        r={d.r}
        fill={d.fill}
        tabIndex={0}
        aria-label={`${p.label}, ${p.day}, ${formatDuration(p.seconds)}`}
        style={{ ["--i" as string]: d.rank, animationDelay: `${d.delayMs}ms` }}
        {...tipProps(galaxyTipDot(p, topicName))}
      />
      {p.oddball && (
        <>
          <circle className="sx-gal-ring" data-t={p.topic_id} cx={d.cx} cy={d.cy} r={d.r + 5} />
          <g className="sx-gal-mark" data-t={p.topic_id} transform={`translate(${d.cx + d.r + 3} ${d.cy + d.r - 3})`}>
            <OddballMark />
          </g>
        </>
      )}
    </g>
  );
}

function Label({ l }: { l: GalaxyLabel }) {
  const top = l.y - l.h / 2 + l.fontSize;
  return (
    <g
      className="sx-gal-label"
      data-t={l.topic.id}
      tabIndex={0}
      aria-label={`${l.topic.name}, ${hoursLabel(l.topic.seconds)}`}
      {...tipProps(galaxyTipLabel(l.topic))}
    >
      <text className="sx-gal-name" x={l.x} y={top} textAnchor="middle" fontSize={l.fontSize}>
        {l.topic.name}
      </text>
      <text className="sx-gal-hours" x={l.x} y={top + LABEL_LINE_GAP} textAnchor="middle">
        {hoursLabel(l.topic.seconds)}
      </text>
    </g>
  );
}

function GalaxyMap({ report, layout }: { report: TopicsReport; layout: GalaxyLayout }) {
  const uid = useId().replace(/:/g, "");
  const { dots, labels } = layout;
  const names = new Map(report.topics.map((t) => [t.id, t.name]));
  const dotByBlock = new Map(dots.map((d) => [d.point.block_id, d]));
  const firstTimes = report.first_times.flatMap((h) => dotByBlock.get(h.block_id) ?? []);
  const hover = report.topics
    .map(
      (t) =>
        `.sx-gal:has([data-t="${t.id}"]:hover) :is(.sx-gal-dot,.sx-gal-ring,.sx-gal-neb,.sx-gal-label,.sx-gal-mark):not([data-t="${t.id}"]){opacity:.12}`,
    )
    .join("");
  return (
    <svg className="sx-gal-svg" viewBox={`0 0 ${GAL_W} ${GAL_H}`} role="group" aria-label="Topic map of your work blocks">
      <style>{hover}</style>
      <defs>
        <radialGradient id={`${uid}-bg`} cx="50%" cy="50%" r="70%">
          <stop offset="0%" className="sx-gal-bg-in" />
          <stop offset="100%" className="sx-gal-bg-out" />
        </radialGradient>
        <filter id={`${uid}-blur`} x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="16" />
        </filter>
      </defs>
      <rect width={GAL_W} height={GAL_H} fill={`url(#${uid}-bg)`} />
      <g filter={`url(#${uid}-blur)`} aria-hidden="true">
        <Nebula report={report} dots={dots} />
      </g>
      {dots.map((d) => (
        <Star key={d.point.block_id} d={d} topicName={names.get(d.point.topic_id) ?? ""} />
      ))}
      {firstTimes.map((d) => (
        <g
          key={`ft-${d.point.block_id}`}
          className="sx-gal-mark"
          data-t={d.point.topic_id}
          transform={`translate(${d.cx + d.r + 2} ${d.cy - d.r - 16})`}
        >
          <FirstTimeMark />
        </g>
      ))}
      {labels.map((l) => (
        <Label key={l.topic.id} l={l} />
      ))}
    </svg>
  );
}

export function TopicGalaxy({ report }: { report: TopicsReport }) {
  const layout = galaxyLayout(report);
  return (
    <div className="sx-gal sx-gal-card">
      {report.naming === "pending" && <span className="sx-gal-chip">AI is naming topics…</span>}
      {layout.dots.length === 0 ? (
        <div className="sx-gal-empty">
          <AtlasEmpty kind="no_map" />
          <p className="sr-empty">Map needs at least 10 described blocks.</p>
        </div>
      ) : (
        <GalaxyMap report={report} layout={layout} />
      )}
      <p className="sx-gal-how">{HOW}</p>
    </div>
  );
}
