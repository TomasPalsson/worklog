import "@/app/stats/charts-ranked.css";
import { formatDuration } from "@/lib/format";
import type { TopicsReport } from "@/lib/topics_contract";
import { RankedBars } from "./RankedBars";

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="sr-card">
      <h3 className="sr-title">{title}</h3>
      {children}
    </section>
  );
}

export function TopicsSection({ report }: { report: TopicsReport | null }) {
  if (!report || report.status === "helper_down") {
    return <p className="sr-empty">Topic stats need the local helper (`worklog verdict`).</p>;
  }
  if (report.status === "too_few_blocks") {
    return <p className="sr-empty">Not enough described work in this range yet.</p>;
  }
  return (
    <div className="stats-stack">
      <RankedBars title="Topics" unit="seconds" rows={report.topics.map((t) => ({ label: t.name, value: t.seconds }))} />
      <Card title="Switches per day">
        <ul className="sr-rows">
          {report.switches.map((s) => (
            <li key={s.day} className="sr-label">
              {s.day} · {s.switches} {s.switches === 1 ? "switch" : "switches"} · longest run {formatDuration(s.longest_run_seconds)}
            </li>
          ))}
        </ul>
      </Card>
      <Card title="Drift">
        <ul className="sr-rows">
          {report.drift.map((d) => (
            <li key={d.topic_id} className="sr-label">
              {d.name} · now {formatDuration(d.seconds_now)} · before {formatDuration(d.seconds_before)}
            </li>
          ))}
        </ul>
      </Card>
      <Card title="Catch-all tickets">
        <ul className="sr-rows">
          {report.sprawl.map((s) => (
            <li key={s.ticket} className="sr-label">
              {s.ticket}
              {s.summary ? ` ${s.summary}` : ""} · spread {s.spread.toFixed(2)}
            </li>
          ))}
        </ul>
      </Card>
      <Card title="Similar days">
        <ul className="sr-rows">
          {report.similar_days.map((d) => (
            <li key={d.day} className="sr-label">
              {d.day} · {Math.round(d.similarity * 100)}% · {d.top_tickets.map((t) => t.label).join(", ")}
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
