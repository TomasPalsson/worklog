import "@/app/stats/charts-ranked.css";
import "@/app/stats/mirror.css";
import { formatDuration, shortMonthDay } from "@/lib/format";
import type { TopicsReport } from "@/lib/topics_contract";
import { tipProps, type Tip } from "./tip";

const MIN_DAYS = 3;
const MINUS = "−";
const CARD_PX = 520; // .stats-two half width, when the real width is unknown
const PAD_PX = 32;
const GAP_PX = 2;
const LABEL_PX = 23;
const COMPACT_PX = 44;
const TAB_DAYS = 14;

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

export function diagTip(report: TopicsReport, index: number): Tip {
  const days = report.days.slice(-report.day_similarity.length);
  const others = days.map((_, other) => other).filter((other) => other !== index);
  const sim = (other: number) => report.day_similarity[index][other];
  const most = others.reduce((a, b) => (sim(b) > sim(a) ? b : a));
  const least = others.reduce((a, b) => (sim(b) < sim(a) ? b : a));
  const pick = (other: number) => `${shortMonthDay(days[other].day)} (${signed(sim(other))})`;
  return {
    title: shortMonthDay(days[index].day),
    note: `Most like: ${pick(most)} · Least like: ${pick(least)} · Top topic: ${topicName(report, days[index].top_topic)}`,
  };
}

export function DayMirror({ report }: { report: TopicsReport }) {
  const size = report.day_similarity.length;
  if (size < MIN_DAYS) return null;
  const days = report.days.slice(-size);
  let largest = 0;
  let alike: [number, number] = [0, 1];
  let opposite: [number, number] = [0, 1];
  for (let row = 0; row < size; row++) {
    for (let column = row + 1; column < size; column++) {
      const similarity = report.day_similarity[row][column];
      largest = Math.max(largest, Math.abs(similarity));
      if (similarity > report.day_similarity[alike[0]][alike[1]]) alike = [row, column];
      if (similarity < report.day_similarity[opposite[0]][opposite[1]]) opposite = [row, column];
    }
  }
  const pair = ([row, column]: [number, number]) =>
    `${shortMonthDay(days[row].day)} & ${shortMonthDay(days[column].day)} (${signed(report.day_similarity[row][column])})`;

  const cellPx = (CARD_PX - PAD_PX - GAP_PX * (size - 1)) / size;
  const compact = cellPx < COMPACT_PX;
  const step = Math.max(1, Math.ceil(LABEL_PX / (compact ? (CARD_PX - PAD_PX - GAP_PX * (size - 1)) / size : cellPx)));
  const offset = compact ? 2 : 1;
  const cells = [];
  if (compact) {
    for (let row = 0; row < size; row++) {
      if (row % step !== 0 && row !== size - 1) continue;
      cells.push(
        <span key={`label-${row}`} className="sx-mir-rowlabel" style={{ gridRow: row + 1, gridColumn: 1 }}>
          {shortMonthDay(days[row].day)}
        </span>,
      );
    }
  }
  for (let row = 0; row < size; row++) {
    for (let column = row; column < size; column++) {
      const position = { gridRow: row + 1, gridColumn: column + offset, "--i": row + column } as React.CSSProperties;
      const tip = tipProps(mirrorTip(report, row, column));
      if (row === column) {
        const diag = diagTip(report, row);
        const hue = topicHue(report, days[row].top_topic);
        cells.push(
          <div key={`${row}-${column}`} className="sx-mir-cell sx-mir-diag art-fade" style={position} tabIndex={0} aria-label={`${diag.title}. ${diag.note}`} {...tip}>
            <span className="sx-mir-chip" style={{ "--h": hue } as React.CSSProperties} />
            {!compact && <span className="art-label">{shortMonthDay(days[row].day)}</span>}
          </div>,
        );
        continue;
      }
      const similarity = report.day_similarity[row][column];
      const tabbable = row >= size - TAB_DAYS;
      const ring = row === alike[0] && column === alike[1] ? " sx-mir-ring-alike" : row === opposite[0] && column === opposite[1] ? " sx-mir-ring-opposite" : "";
      const percent = largest > 0 ? Math.round((Math.abs(similarity) / largest) * 100) : 0;
      cells.push(
        <div
          key={`${row}-${column}`}
          className={`sx-mir-cell art-fade ${similarity < 0 ? "sx-mir-neg" : "sx-mir-pos"}${ring}`}
          style={{ ...position, "--p": percent } as React.CSSProperties}
          {...(tabbable && {
            tabIndex: 0,
            "aria-label": `${shortMonthDay(days[row].day)} and ${shortMonthDay(days[column].day)}: ${Math.abs(similarity).toFixed(2)} ${similarity < 0 ? "opposite" : "alike"}`,
          })}
          {...tip}
        />,
      );
    }
  }

  return (
    <section className="sr-card sx-mir">
      <h3 className="sr-title">Day mirror</h3>
      <div className="sx-mir-legend">
        similar work <span className="sx-mir-swatch sx-mir-pos" style={{ "--p": 100 } as React.CSSProperties} /> · different work{" "}
        <span className="sx-mir-swatch sx-mir-neg" style={{ "--p": 100 } as React.CSSProperties} />
        <span className="sx-mir-strip">
          {[33, 66, 100].map((p) => (
            <span key={p} className="sx-mir-step" style={{ "--p": p } as React.CSSProperties} />
          ))}
        </span>
      </div>
      <p className="sx-mir-callout">
        Most alike days: {pair(alike)} · Most opposite: {pair(opposite)}
      </p>
      <div className="sx-mir-grid" data-k={step}
        style={{ gridTemplateColumns: `${compact ? "auto " : ""}repeat(${size}, minmax(0, 1fr))` }}>
        {cells}
      </div>
      <div className="sx-mir-how">
        How it works: Each day&apos;s typical work is compared with every other day&apos;s. Warm cells are alike, cool cells are opposite, and ordinary days sit near 0.
        <details>
          <summary>Method</summary>
          Cosine similarity of each day&apos;s hours-weighted average vector, centered so ordinary days score near 0.
        </details>
      </div>
    </section>
  );
}
