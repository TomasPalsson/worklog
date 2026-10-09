import Link from "next/link";
import type { ReactNode } from "react";
import { formatDayHeading, formatDuration } from "@/lib/format";
import type { CountRecord, StatsRecords } from "@/lib/stats_contract";

const nf = new Intl.NumberFormat("en-US");

type Fact = { key: string; label: string; value: string; note: string; day?: string; flame?: boolean };

export function factsOf(r: StatsRecords): Fact[] {
  return [...dayFacts(r), ...streakFacts(r)];
}

function streakFacts(r: StatsRecords): Fact[] {
  const streak = (n: number) => (n === 1 ? "1 day" : `${n} days`);
  return [
    {
      key: "longest",
      label: "Longest streak",
      value: streak(r.longest_streak),
      note: r.longest_streak >= 3 ? "Consecutive days with work." : "Consecutive days. Room to grow.",
      flame: r.longest_streak >= 3,
    },
    {
      key: "current",
      label: "Current streak",
      value: streak(r.current_streak),
      note: r.current_streak >= 3 ? "Still going. Keep it lit." : r.current_streak > 0 ? "A spark." : "Starts with the next work day.",
      flame: r.current_streak >= 3,
    },
  ];
}

function countFact(key: string, label: string, rec: CountRecord | null, none: string): Fact {
  return {
    key,
    label,
    value: rec ? nf.format(rec.n) : "—",
    note: rec ? formatDayHeading(rec.day) : none,
    day: rec?.day,
  };
}

function dayFacts(r: StatsRecords): Fact[] {
  return [
    {
      key: "busiest",
      label: "Busiest day",
      value: r.busiest_day ? formatDuration(r.busiest_day.seconds) : "—",
      note: r.busiest_day ? formatDayHeading(r.busiest_day.day) : "No work blocks yet",
      day: r.busiest_day?.day,
    },
    {
      key: "block",
      label: "Longest block",
      value: r.longest_block ? formatDuration(r.longest_block.seconds) : "—",
      note: r.longest_block
        ? `${r.longest_block.ticket ?? "no ticket"} · ${formatDayHeading(r.longest_block.day)}`
        : "No work blocks yet",
      day: r.longest_block?.day,
    },
    {
      key: "early",
      label: "Earliest start",
      value: r.earliest_start?.time ?? "—",
      note: r.earliest_start ? formatDayHeading(r.earliest_start.day) : "Nobody was up yet",
      day: r.earliest_start?.day,
    },
    {
      key: "late",
      label: "Latest finish",
      value: r.latest_finish?.time ?? "—",
      note: r.latest_finish ? formatDayHeading(r.latest_finish.day) : "Always home for dinner",
      day: r.latest_finish?.day,
    },
    countFact("prompts", "Most prompts in a day", r.most_prompts, "No prompts yet"),
    countFact("tools", "Most tool calls in a day", r.most_tools, "No tool calls yet"),
  ];
}

function Flame() {
  return (
    <svg className="stats-flame" viewBox="0 0 24 24" width="22" height="22" role="img" aria-label="on fire">
      <path
        d="M12 3c.5 3-3.5 5-3.5 9a3.5 3.5 0 0 0 7 0c0-1.2-.5-2-1-2.8.2 1.6-1 2.3-1.7 1.8C13.7 8 13 5 12 3z"
        style={{ fill: "var(--terracotta-bg)", stroke: "var(--terracotta-ink)", strokeWidth: 1.6, strokeLinejoin: "round" }}
      />
      <path d="M12 21a5.5 5.5 0 0 0 5.5-5.5c0-3-2-4.6-3-6.5" style={{ fill: "none", stroke: "var(--terracotta-ink)", strokeWidth: 1.6, strokeLinecap: "round" }} />
    </svg>
  );
}

export function FunFacts({ records }: { records: StatsRecords }) {
  return (
    <ul className="stats-facts" aria-label="Records">
      {factsOf(records).map((f, i) => {
        const body: ReactNode = (
          <>
            <span className="stats-fact-label">{f.label}</span>
            <span className="stats-fact-val">
              {f.value}
              {f.flame && <Flame />}
            </span>
            <span className="stats-fact-note">{f.note}</span>
          </>
        );
        return (
          <li key={f.key} className="stats-fact art-fade" style={{ animationDelay: `${i * 50}ms` }}>
            {f.day ? (
              <Link href={`/${f.day}`} className="stats-fact-link">
                {body}
              </Link>
            ) : (
              <div className="stats-fact-link">{body}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
