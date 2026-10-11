import "@/app/stats/charts-ranked.css";
import "@/app/stats/heartbeats.css";
import { formatDuration, shortMonthDay } from "@/lib/format";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { RhythmIcon } from "./AtlasArt";
import { pct } from "./chart-tip-utils";
import { tipProps, type Tip } from "./tip";
import { topicColor, topicHue } from "./topicColor";

export type Rhythm = "always" | "sprint" | "one_off";

const MAX_CHIPS = 3;
const GENERIC = new Set(
  "tool tools code block blocks pull enable enabled page pages chat chats project projects update fix issue issues test tests config configuration setup work pptx prs pr file files boost thinking binding multi base revise pdf".split(" "),
);
const SPRINT_MAX_SPAN_DAYS = 6;
const SPRINT_MIN_DAYS = 3;
const MILLISECONDS_PER_DAY = 86_400_000;

const LABEL: Record<Rhythm, string> = { always: "Always on", sprint: "Sprint", one_off: "One-off" };
const NOTE: Record<Rhythm, string> = {
  always: "Always on: active on at least half your work days.",
  sprint: "Sprint: at least 3 days, all within one week.",
  one_off: "One-off: a single day.",
};

const dayNumber = (day: string) => Date.parse(`${day}T00:00:00Z`) / MILLISECONDS_PER_DAY;

export function rhythmOf(topic: Topic, workDays: number): Rhythm | null {
  if (topic.days_active >= 0.5 * workDays) return "always";
  const span = dayNumber(topic.last_day) - dayNumber(topic.first_day);
  if (span <= SPRINT_MAX_SPAN_DAYS && topic.days_active >= SPRINT_MIN_DAYS) return "sprint";
  if (topic.days_active === 1) return "one_off";
  return null;
}

export function heartbeatTip(topic: Topic, totalSeconds: number, workDays: number, rhythm: Rhythm | null): Tip {
  const share = totalSeconds > 0 ? topic.seconds / totalSeconds : 0;
  return {
    title: topic.name,
    sub: topic.description ?? undefined,
    rows: [
      ["Time", formatDuration(topic.seconds)],
      ["Share of topics", pct(share)],
      ["Days active", `${topic.days_active} of ${workDays} work days`],
      ["Blocks", String(topic.blocks)],
      ...(topic.top_tickets.length > 0 ? [["Tickets", topic.top_tickets.map((t) => t.label).join(", ")] as [string, string]] : []),
    ],
    bar: { value: topic.seconds, max: totalSeconds, label: `${pct(share)} of topic time` },
    note: rhythm ? NOTE[rhythm] : undefined,
    accent: "var(--slate)",
  };
}

interface RowProps {
  topic: Topic;
  index: number;
  topicCount: number;
  river: TopicsReport["river"];
  totalSeconds: number;
  maxTopicSeconds: number;
  maxBinSeconds: number;
  workDays: number;
}

function HeartbeatRow({ topic, index, topicCount, river, totalSeconds, maxTopicSeconds, maxBinSeconds, workDays }: RowProps) {
  const rhythm = rhythmOf(topic, workDays);
  const activeBins = river.filter((bin) => (bin.seconds[index] ?? 0) > 0);
  const dates = activeBins.length > 0 ? [activeBins[0].start, activeBins[activeBins.length - 1].start] : undefined;
  const color = topicColor(topic.id, topicCount);
  const tip = { ...heartbeatTip(topic, totalSeconds, workDays, rhythm), accent: color };
  return (
    <li
      className="hb-row"
      data-heartbeat-row
      style={{ ["--h" as string]: topicHue(topic.id, topicCount), ["--hb-color" as string]: color, ["--i" as string]: index }}
      tabIndex={0}
      aria-label={`${topic.name}: ${formatDuration(topic.seconds)}${topic.description ? `. ${topic.description}` : ""}`}
      {...tipProps(tip)}
    >
      <div className="hb-id">
        <span className="hb-swatch" aria-hidden="true" />
        <span className="hb-name" data-keywords={topic.named_by === "keywords" || undefined}>
          {topic.name}
        </span>
        {rhythm && (
          <span className="hb-badge">
            <RhythmIcon kind={rhythm} />
            {LABEL[rhythm]}
          </span>
        )}
      </div>
      <div className="hb-about">
        {topic.description && <p className="hb-description" title={topic.description}>{topic.description}</p>}
        <ul className="hb-chips">
          {topic.keywords.filter((keyword) => !GENERIC.has(keyword.toLowerCase())).slice(0, MAX_CHIPS).map((keyword) => (
            <li key={keyword} className="hb-chip">
              {keyword}
            </li>
          ))}
        </ul>
      </div>
      <div className="hb-data">
        <div className="hb-raster" aria-hidden="true" style={{ gridTemplateColumns: `repeat(${river.length}, 1fr)` }}>
          {river.map((bin) => (
            <span
              key={bin.start}
              className="hb-tick"
              data-tick
              style={{ height: `${maxBinSeconds > 0 ? ((bin.seconds[index] ?? 0) / maxBinSeconds) * 100 : 0}%` }}
            />
          ))}
        </div>
        {dates && (
          <div className="hb-dates">
            <span>
              active {shortMonthDay(dates[0])}
              {dates[0] !== dates[1] && ` – ${shortMonthDay(dates[1])}`}
            </span>
          </div>
        )}
        <span className="hb-hours sr-value">{formatDuration(topic.seconds)}</span>
        <span className="sr-track hb-track" aria-hidden="true">
          <span className="sr-fill hb-fill" style={{ display: "block", width: `${maxTopicSeconds > 0 ? (topic.seconds / maxTopicSeconds) * 100 : 0}%` }} />
        </span>
      </div>
    </li>
  );
}

export function TopicHeartbeats({ report }: { report: TopicsReport }) {
  const { topics, river } = report;
  const shared = {
    topicCount: topics.length,
    river,
    workDays: report.days.filter((d) => d.work_seconds > 0).length,
    totalSeconds: topics.reduce((sum, t) => sum + t.seconds, 0),
    maxTopicSeconds: Math.max(0, ...topics.map((t) => t.seconds)),
    maxBinSeconds: Math.max(0, ...river.flatMap((bin) => bin.seconds)),
  };
  return (
    <section className="sr-card">
      <h3 className="sr-title">Topic heartbeats</h3>
      <ul className="hb-rows">
        {topics.map((topic, index) => (
          <HeartbeatRow key={topic.id} topic={topic} index={index} {...shared} />
        ))}
      </ul>
      <p className="sr-caption hb-how">
        How it works: Ticks show hours per day in this topic. Always on = active on at least half your work days. Sprint = at least 3 days, all within one week.
      </p>
    </section>
  );
}
