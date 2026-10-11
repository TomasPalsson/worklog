import "@/app/stats/mirror.css";
import { formatDuration, shortMonthDay } from "@/lib/format";
import type { TopicsReport } from "@/lib/topics_contract";
import { tipProps, type Tip } from "./tip";

const MIN_DAYS = 3;
const MINUS = "−";

const signed = (similarity: number) => `${similarity < 0 ? MINUS : "+"}${Math.abs(similarity).toFixed(2)}`;

const topicHue = (report: TopicsReport, topicId: number) => 25 + topicId * (360 / report.topics.length);

const topicName = (report: TopicsReport, topicId: number) => report.topics.find((t) => t.id === topicId)?.name ?? "";

export function mirrorTip(report: TopicsReport, row: number, column: number): Tip {
  const days = report.days.slice(-report.day_similarity.length);
  const dayRow = (index: number): [string, string] => [
    shortMonthDay(days[index].day),
    `${topicName(report, days[index].top_topic)} · ${formatDuration(days[index].work_seconds)}`,
  ];
  return {
    title: `${shortMonthDay(days[row].day)} ↔ ${shortMonthDay(days[column].day)}`,
    rows: [["Similarity", signed(report.day_similarity[row][column])], dayRow(row), dayRow(column)],
    note: `+1 = same kind of work, ${MINUS}1 = opposite`,
  };
}

export function DayMirror({ report }: { report: TopicsReport }) {
  const size = report.day_similarity.length;
  if (size < MIN_DAYS) return null;
  const days = report.days.slice(-size);
  let largest = 0;
  for (let row = 0; row < size; row++) {
    for (let column = row + 1; column < size; column++) largest = Math.max(largest, Math.abs(report.day_similarity[row][column]));
  }

  const cells = [];
  for (let row = 0; row < size; row++) {
    for (let column = row; column < size; column++) {
      const position = { gridRow: row + 1, gridColumn: column + 1, "--i": row + column } as React.CSSProperties;
      const tip = tipProps(mirrorTip(report, row, column));
      const label = `${shortMonthDay(days[row].day)} and ${shortMonthDay(days[column].day)}`;
      if (row === column) {
        const hue = topicHue(report, days[row].top_topic);
        cells.push(
          <div key={`${row}-${column}`} className="sx-mir-cell sx-mir-diag art-fade" style={position} tabIndex={0} aria-label={label} {...tip}>
            <span className="sx-mir-chip" style={{ "--h": hue } as React.CSSProperties} />
            <span className="art-label">{shortMonthDay(days[row].day)}</span>
          </div>,
        );
        continue;
      }
      const similarity = report.day_similarity[row][column];
      const percent = largest > 0 ? Math.round((Math.abs(similarity) / largest) * 100) : 0;
      cells.push(
        <div
          key={`${row}-${column}`}
          className={`sx-mir-cell art-fade ${similarity < 0 ? "sx-mir-neg" : "sx-mir-pos"}`}
          style={{ ...position, "--p": percent } as React.CSSProperties}
          tabIndex={0}
          aria-label={label}
          {...tip}
        />,
      );
    }
  }

  return (
    <section className="sx-mir">
      <h3 className="sx-mir-title">Day mirror</h3>
      <div className="sx-mir-grid" style={{ gridTemplateColumns: `repeat(${size}, minmax(0, 1fr))` }}>
        {cells}
      </div>
      <p className="sx-mir-how">How: Cosine similarity of each day&apos;s hours-weighted average vector, centered so ordinary days score near 0.</p>
    </section>
  );
}
