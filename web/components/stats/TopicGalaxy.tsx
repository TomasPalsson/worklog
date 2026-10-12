"use client";

import "@/app/stats/galaxy.css";
import dynamic from "next/dynamic";
import { useEffect, useId, useState } from "react";
import { formatDuration } from "@/lib/format";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { AtlasEmpty, FirstTimeMark, OddballMark } from "./AtlasArt";
import { type Tip, tipProps } from "./tip";
import { topicColor } from "./topicColor";
import { GAL_H, GAL_W, HOURS_FONT, hoursLabel, LABEL_LINE_GAP, NAME_LINE_H, galaxyLayout, type GalaxyDot, type GalaxyLabel, type GalaxyLayout } from "./galaxyLayout";
export { GAL_H, GAL_W, galaxyLayout, wrapName, type GalaxyDot, type GalaxyLabel, type GalaxyLayout } from "./galaxyLayout";

const NEBULA_R = 26;

const VIEW_KEY = "worklog.galaxyView";
const VIEWS = [
  ["map", "Map"],
  ["stars", "Star field"],
] as const;
type View = (typeof VIEWS)[number][0];

const StarField = dynamic(() => import("./StarField"), {
  ssr: false,
  loading: () => <p className="sx-gal-how">Loading star field…</p>,
});

const HOW =
  "How it works: Each dot is a work block. Blocks about similar things sit close together, and each island is one topic. Names come from your AI model (the one set in worklog setup), or from keywords if none is available. Distance between islands is only roughly meaningful.";
const METHOD =
  "The local embedding model turns each block's text into a vector, the vectors are centered, and t-SNE (perplexity 30) lays them out with a gentle pull toward each topic's centre, so a topic stays one island. Topics come from average-linkage clustering on cosine similarity; the number of topics (up to 20) is the cut with the best silhouette score where every topic has at least 3 blocks.";


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

function ViewToggle({ view, onChange }: { view: View; onChange: (v: View) => void }) {
  return (
    <div className="view-toggle sx-gal-view" role="group" aria-label="Galaxy view">
      {VIEWS.map(([id, label]) => (
        <button
          key={id}
          type="button"
          className={`view-toggle-btn${view === id ? " is-active" : ""}`}
          aria-pressed={view === id}
          onClick={() => onChange(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

export function TopicGalaxy({ report }: { report: TopicsReport }) {
  const layout = galaxyLayout(report);
  const [view, setView] = useState<View>("map");
  useEffect(() => {
    try {
      if (localStorage.getItem(VIEW_KEY) === "stars") setView("stars");
    } catch {}
  }, []);
  const choose = (v: View) => {
    setView(v);
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {}
  };
  return (
    <div className="sx-gal sr-card">
      <div className="sx-gal-head">
        <h3 className="sr-title">Topic galaxy</h3>
        <ViewToggle view={view} onChange={choose} />
      </div>
      {report.naming === "pending" && <span className="sx-gal-chip">AI is naming topics…</span>}
      {layout.dots.length === 0 ? (
        <div className="sx-gal-empty">
          <AtlasEmpty kind="no_map" />
          <p className="sr-empty">Map needs at least 10 described blocks.</p>
        </div>
      ) : (
        <>
          <p className="sx-gal-headline">{galaxyHeadline(report.topics)}</p>
          {view === "stars" ? (
            <StarField report={report} />
          ) : (
            <>
              <div className="sx-gal-map">
                <GalaxyMap report={report} layout={layout} />
              </div>
              <Legend />
            </>
          )}
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
