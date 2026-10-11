import "@/app/stats/charts-ranked.css";
import "@/app/stats/moments.css";
import { formatDuration, shortMonthDay } from "@/lib/format";
import { ODDBALL_MAX_COS, type Highlight, type Topic, type TopicsReport } from "@/lib/topics_contract";
import { FirstTimeMark, OddballMark } from "./AtlasArt";
import { tipProps, type Tip } from "./tip";

type Kind = "first_time" | "oddball";

const MILLISECONDS_PER_DAY = 86_400_000;
const EMPTY = "Nothing here stands apart; your work is well clustered.";

const dayNumber = (day: string) => Date.parse(`${day}T00:00:00Z`) / MILLISECONDS_PER_DAY;
const score = (value: number) => value.toFixed(2);

export function stripFraction(day: string, from: string, to: string): number {
  const span = dayNumber(to) - dayNumber(from);
  if (span <= 0) return 0.5;
  return Math.min(1, Math.max(0, (dayNumber(day) - dayNumber(from)) / span));
}

export function momentTip(item: Highlight, kind: Kind, topicName: string | undefined, median: number): Tip {
  const rows: [string, string][] = [
    ["Day", shortMonthDay(item.day)],
    ["Time", formatDuration(item.seconds)],
  ];
  if (topicName) rows.push(["Topic", topicName]);
  if (kind === "first_time") rows.push(["Novelty", score(item.score)]);
  else rows.push(["Nearest anything", score(item.score)], ["Your median", score(median)]);
  return { title: item.label, rows, accent: kind === "first_time" ? "var(--amber)" : "var(--fg-muted)" };
}

interface CardProps {
  item: Highlight;
  kind: Kind;
  topics: Topic[];
  from: string;
  to: string;
  median: number;
}

function MomentCard({ item, kind, topics, from, to, median }: CardProps) {
  const topicName = topics.find((t) => t.id === item.topic_id)?.name;
  return (
    <li
      className="tm-card"
      data-moment={kind}
      tabIndex={0}
      aria-label={`${item.label}: ${score(item.score)}`}
      {...tipProps(momentTip(item, kind, topicName, median))}
    >
      <span className="tm-score">{score(item.score)}</span>
      <span className="tm-label">
        {kind === "first_time" ? <FirstTimeMark /> : <OddballMark />}
        <span className="tm-clamp">{item.label}</span>
      </span>
      <span className="tm-when">
        {shortMonthDay(item.day)} · {formatDuration(item.seconds)}
      </span>
      {topicName && <span className="tm-chip">{topicName}</span>}
      {kind === "oddball" && (
        <span className="tm-nearest">
          closest match {score(item.score)} (typical {score(median)})
        </span>
      )}
      <span className="tm-strip" aria-hidden="true">
        <span className="tm-marker" style={{ left: `${stripFraction(item.day, from, to) * 100}%` }} />
      </span>
      <span className="tm-ends">
        <span>{shortMonthDay(from)}</span>
        <span>{shortMonthDay(to)}</span>
      </span>
    </li>
  );
}

export function TopicMoments({ report }: { report: TopicsReport }) {
  const { topics, from, to, oddball_median: median } = report;
  const column = (kind: Kind, title: string, subtitle: string, items: Highlight[]) => (
    <section className="sr-card">
      <h3 className="sr-title">{title}</h3>
      <p className="tm-sub">{subtitle}</p>
      {items.length === 0 ? (
        <p className="sr-empty">{EMPTY}</p>
      ) : (
        <ul className="tm-cards">
          {items.map((item) => (
            <MomentCard key={item.block_id} item={item} kind={kind} topics={topics} from={from} to={to} median={median} />
          ))}
        </ul>
      )}
    </section>
  );
  return (
    <div className="tm-wrap">
      <div className="stats-two">
        {column("first_time", "First times", "higher = newer ground", report.first_times)}
        {column("oddball", "Oddballs", "lower = more unusual", report.oddballs)}
      </div>
      <div className="tm-how">
        <p className="sr-caption">
          How it works: First time = lowest similarity to any earlier block, including the 60 days before the range. Your first 20 blocks are skipped — they have nothing earlier to compare with. Oddball = its closest block in this range is
          under {ODDBALL_MAX_COS.toFixed(2)} similarity; your median block&apos;s closest is {score(median)}.
        </p>
        <details>
          <summary>Method</summary>
          Similarity is the cosine similarity between block embeddings, 1 for identical meaning and near 0 for unrelated.
        </details>
      </div>
    </div>
  );
}
