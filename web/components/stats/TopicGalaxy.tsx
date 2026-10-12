import "@/app/stats/galaxy.css";
import { useId } from "react";
import { formatDuration } from "@/lib/format";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { AtlasEmpty, FirstTimeMark, OddballMark } from "./AtlasArt";
import { type Tip, tipProps } from "./tip";
import { topicColor } from "./topicColor";

export const GAL_W = 1000;
export const GAL_H = 620;
const MIN_H = 360;
const PAD = 40;
const NEBULA_R = 26;
const NUDGE_ITERATIONS = 50;
const MAX_DELAY_MS = 1200;
const NAME_CHAR_W = 0.62; // mono advance per font-size, including letter-spacing
const LABEL_MIN_X = 8;
const LABEL_MAX_X = GAL_W - 8;
const MIN_FONT = 11;
const LABEL_GAP = 8; // breathing room kept between nudged labels
const NAME_LINE_H = 1.1; // second name line step, per font-size
const LABEL_LINE_GAP = 16; // name-to-hours baseline step; the hours line is part of the label's box
const HOURS_FONT = 13; // keep equal to .sx-gal-hours font-size in galaxy.css
const MAX_LABEL_CHARS = 24;

const HOW =
  "How it works: Each dot is a work block. Blocks about similar things sit close together, and each cluster is a topic. Names come from AI (Haiku), or from keywords if no model is available. Distance between islands is only roughly meaningful.";
const METHOD =
  "The local embedding model turns each block's text into a vector, the vectors are centered, and t-SNE (perplexity 30) lays them out. Topics come from average-linkage clustering on cosine similarity.";

const hoursLabel = (sec: number) => `${(sec / 3600).toFixed(1)}h`;

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
  lines: string[];
  x: number;
  y: number;
  fontSize: number;
  w: number;
  h: number;
}

export interface GalaxyLayout {
  height: number;
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

export function wrapName(name: string): string[] {
  if (name.length <= MAX_LABEL_CHARS) return [name];
  const space = name.lastIndexOf(" ", MAX_LABEL_CHARS);
  const at = space > 0 ? space : MAX_LABEL_CHARS;
  const rest = name.slice(space > 0 ? at + 1 : at);
  return [name.slice(0, at), rest.length > MAX_LABEL_CHARS ? `${rest.slice(0, MAX_LABEL_CHARS - 1)}…` : rest];
}

/** Pure placement of dots and labels in a 1000-wide viewBox whose height follows the y extent. */
export function galaxyLayout(report: Pick<TopicsReport, "topics" | "points">): GalaxyLayout {
  const { topics } = report;
  const index = new Set(topics.map((t) => t.id));
  const points = report.points.filter((p) => index.has(p.topic_id));
  const maxSec = Math.max(1, ...points.map((p) => p.seconds));
  const order = [...points].sort((a, b) => a.day.localeCompare(b.day) || a.block_id - b.block_id);
  const rankOf = new Map(order.map((p, i) => [p.block_id, i]));

  const ys = points.map((p) => p.y);
  const yMin = Math.min(...ys);
  const span = (Math.max(...ys) - yMin) * (GAL_H - 2 * PAD);
  const height = points.length ? Math.max(MIN_H, span + 2 * PAD) : GAL_H;
  const top = (height - span) / 2;

  const dots = points.map((p) => {
    const rank = rankOf.get(p.block_id) ?? 0;
    return {
      point: p,
      cx: PAD + p.x * (GAL_W - 2 * PAD),
      cy: top + (p.y - yMin) * (GAL_H - 2 * PAD),
      r: 2.5 + 6 * Math.sqrt(p.seconds / maxSec),
      fill: topicColor(p.topic_id, topics.length),
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
    const fontSize = MIN_FONT + 9 * Math.sqrt(totalSec > 0 ? topic.seconds / totalSec : 0);
    const lines = wrapName(topic.name);
    const hoursW = hoursLabel(topic.seconds).length * HOURS_FONT * NAME_CHAR_W;
    const w = Math.max(Math.max(...lines.map((t) => t.length)) * fontSize * NAME_CHAR_W, hoursW);
    labels.push({
      topic,
      lines,
      x: weightedMedian(wts((d) => d.cx)),
      y: weightedMedian(wts((d) => d.cy)),
      fontSize,
      w,
      h: fontSize * (1 + (lines.length - 1) * NAME_LINE_H) + LABEL_LINE_GAP + HOURS_FONT,
    });
  }

  const clamp = (l: GalaxyLabel) => {
    l.x = Math.min(LABEL_MAX_X - l.w / 2, Math.max(LABEL_MIN_X + l.w / 2, l.x));
    l.y = Math.min(height - l.h / 2, Math.max(l.h / 2, l.y));
  };
  labels.forEach(clamp);
  for (let it = 0; it < NUDGE_ITERATIONS; it++) {
    let moved = false;
    const byY = [...labels].sort((p, q) => p.y - q.y);
    for (let i = 0; i < byY.length; i++) {
      for (let j = i + 1; j < byY.length; j++) {
        const a = byY[i];
        const b = byY[j];
        if (Math.abs(a.x - b.x) >= (a.w + b.w) / 2) continue;
        const need = (a.h + b.h) / 2 + LABEL_GAP - Math.abs(a.y - b.y);
        if (need <= 1e-9) continue;
        const dir = a.y <= b.y ? 1 : -1;
        a.y -= (dir * need) / 2;
        b.y += (dir * need) / 2;
        moved = true;
      }
    }
    labels.forEach(clamp);
    if (!moved) break;
  }
  return { height, dots, labels };
}

export function galaxyTipDot(p: MapPoint, topicName: string): Tip {
  return {
    title: p.label,
    rows: [
      ["Day", p.day],
      ["Hours", formatDuration(p.seconds)],
      ["Ticket", p.ticket ?? "none"],
      ["Topic", topicName],
      ["Newness", `${Math.round(p.novelty * 100)}%`],
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
      ["Blocks", `${t.blocks} · ${t.first_day} … ${t.last_day}`],
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
        className="sx-gal-hit"
        data-t={p.topic_id}
        cx={d.cx}
        cy={d.cy}
        r={12}
        fill="transparent"
        {...tipProps(galaxyTipDot(p, topicName))}
      />
      <circle
        className="sx-gal-dot art-fade"
        data-t={p.topic_id}
        cx={d.cx}
        cy={d.cy}
        r={d.r}
        fill={d.fill}
        tabIndex={-1}
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

function Label({ l, color }: { l: GalaxyLabel; color: string }) {
  const top = l.y - l.h / 2 + l.fontSize;
  return (
    <g
      className="sx-gal-label"
      data-t={l.topic.id}
      tabIndex={0}
      aria-label={`${l.topic.name}, ${hoursLabel(l.topic.seconds)}`}
      {...tipProps(galaxyTipLabel(l.topic))}
    >
      <text className="sx-gal-name" x={l.x} y={top} textAnchor="middle" fontSize={l.fontSize} style={{ fill: color }}>
        {l.lines.map((line, i) => (
          <tspan key={line} x={l.x} dy={i === 0 ? 0 : l.fontSize * NAME_LINE_H}>
            {line}
          </tspan>
        ))}
      </text>
      <text className="sx-gal-hours" x={l.x} y={top + (l.lines.length - 1) * l.fontSize * NAME_LINE_H + LABEL_LINE_GAP} textAnchor="middle">
        {hoursLabel(l.topic.seconds)}
      </text>
    </g>
  );
}

function GalaxyMap({ report, layout }: { report: TopicsReport; layout: GalaxyLayout }) {
  const uid = useId().replace(/:/g, "");
  const { dots, labels, height } = layout;
  const names = new Map(report.topics.map((t) => [t.id, t.name]));
  const dotByBlock = new Map(dots.map((d) => [d.point.block_id, d]));
  const firstTimes = report.first_times.flatMap((h) => dotByBlock.get(h.block_id) ?? []);
  const hover = report.topics
    .map(
      (t) =>
        [":hover", ":focus-visible", ":focus-within"].map(
          (state) =>
            `.sx-gal:has([data-t="${t.id}"]${state}) :is(.sx-gal-dot,.sx-gal-ring,.sx-gal-neb,.sx-gal-label,.sx-gal-mark):not([data-t="${t.id}"]){opacity:.12}`,
        ),
    )
    .flat()
    .join("");
  return (
    <svg className="sx-gal-svg" preserveAspectRatio="xMidYMid meet" viewBox={`0 0 ${GAL_W} ${height}`} role="group" aria-label="Topic map of your work blocks">
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
      <rect width={GAL_W} height={height} fill={`url(#${uid}-bg)`} />
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
        <Label
          key={l.topic.id}
          l={l}
          color={`color-mix(in oklch, ${topicColor(l.topic.id, report.topics.length)} 65%, var(--fg) 35%)`}
        />
      ))}
    </svg>
  );
}

export function galaxyHeadline(topics: Topic[]): string {
  const [a, b] = [...topics].sort((p, q) => q.seconds - p.seconds);
  if (!b) return `1 topic · ${a.name}`;
  const total = topics.reduce((s, t) => s + t.seconds, 0);
  const pct = total > 0 ? Math.round(((a.seconds + b.seconds) / total) * 100) : 0;
  return `${topics.length} topics · ${a.name} and ${b.name} are ${pct}% of your time`;
}

function Legend() {
  return (
    <div className="sx-gal-legend">
      <span className="sx-gal-key">
        <svg width="26" height="14" viewBox="0 0 26 14" aria-hidden="true">
          <circle cx="6" cy="7" r="3" fill="var(--fg-muted)" />
          <circle cx="18" cy="7" r="6" fill="var(--fg-muted)" />
        </svg>
        dot size = hours
      </span>
      <span className="sx-gal-key">
        <FirstTimeMark /> first time
      </span>
      <span className="sx-gal-key">
        <OddballMark /> oddball
      </span>
      <span className="sx-gal-key">· hover or tap a dot for the block</span>
    </div>
  );
}

function TopicList({ report }: { report: TopicsReport }) {
  return (
    <ul className="sx-gal-list">
      {report.topics.map((t) => (
        <li key={t.id} tabIndex={0} aria-label={`${t.name}, ${hoursLabel(t.seconds)}`} {...tipProps(galaxyTipLabel(t))}>
          <span className="sx-gal-swatch" style={{ background: topicColor(t.id, report.topics.length) }} />
          <span className="sx-gal-list-name">{t.name}</span>
          <span className="sx-gal-list-hours">{hoursLabel(t.seconds)}</span>
        </li>
      ))}
    </ul>
  );
}

export function TopicGalaxy({ report }: { report: TopicsReport }) {
  const layout = galaxyLayout(report);
  return (
    <div className="sx-gal sr-card">
      <h3 className="sr-title">Topic galaxy</h3>
      {report.naming === "pending" && <span className="sx-gal-chip">AI is naming topics…</span>}
      {layout.dots.length === 0 ? (
        <div className="sx-gal-empty">
          <AtlasEmpty kind="no_map" />
          <p className="sr-empty">Map needs at least 10 described blocks.</p>
        </div>
      ) : (
        <>
          <p className="sx-gal-headline">{galaxyHeadline(report.topics)}</p>
          <div className="sx-gal-map">
            <GalaxyMap report={report} layout={layout} />
          </div>
          <Legend />
          <TopicList report={report} />
        </>
      )}
      <p className="sx-gal-how">{HOW}</p>
      <details className="sx-gal-method">
        <summary>Method</summary>
        <p>{METHOD}</p>
      </details>
    </div>
  );
}
