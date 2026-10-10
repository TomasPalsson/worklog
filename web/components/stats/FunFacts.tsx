import Link from "next/link";
import type { ReactNode } from "react";
import { formatDayHeading, formatDuration, shiftDay, shortMonthDay, shortWeekday } from "@/lib/format";
import type { CountRecord, DailyStat, StatsRecords } from "@/lib/stats_contract";
import { tipProps, type Tip } from "./tip";

const nf = new Intl.NumberFormat("en-US");
const dayName = (d: string) => `${shortWeekday(d)} ${shortMonthDay(d)}`;
const times = (x: number) => `${x.toFixed(1)}\u00d7`;

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

/** Runs of consecutive worked days, oldest first. */
export function streakRuns(daily: DailyStat[]): { start: string; end: string; n: number }[] {
  const runs: { start: string; end: string; n: number }[] = [];
  for (const d of daily) {
    if (d.work_seconds <= 0) continue;
    const last = runs[runs.length - 1];
    if (last && shiftDay(last.end, 1) === d.day) {
      last.end = d.day;
      last.n += 1;
    } else runs.push({ start: d.day, end: d.day, n: 1 });
  }
  return runs;
}

function streakTip(label: string, n: number, run: { start: string; end: string } | undefined, accent: string): Tip {
  const rows: [string, string][] = [["Length", n === 1 ? "1 day" : `${n} days`]];
  if (run) rows.push(["From", dayName(run.start)], ["To", dayName(run.end)]);
  return {
    title: label,
    sub: run ? `${dayName(run.start)} \u2192 ${dayName(run.end)}` : undefined,
    rows,
    bar: { value: Math.min(n, 7), max: 7, label: "Progress to a full week" },
    note: n >= 7 ? `${Math.floor(n / 7)} full week${n >= 14 ? "s" : ""} without a day off.` : n > 0 ? `${7 - n} more to a full week.` : "Starts with the next work day.",
    accent,
  };
}

const median = (v: string[]) => (v.length ? [...v].sort()[Math.floor(v.length / 2)] : null);

function countTip(label: string, noun: string, rec: CountRecord | null, per: (d: DailyStat) => number, daily: DailyStat[]): Tip {
  if (!rec) return { title: label, note: "Nothing recorded in this range yet." };
  const active = daily.filter((d) => per(d) > 0);
  const total = active.reduce((a, d) => a + per(d), 0);
  const avg = active.length ? total / active.length : 0;
  const rank = active.filter((d) => per(d) > rec.n).length + 1;
  const rows: [string, string][] = [["Count", `${nf.format(rec.n)} ${noun}`], ["Day", dayName(rec.day)]];
  if (avg > 0) rows.push(["Average active day", `${nf.format(Math.round(avg))} ${noun}`]);
  if (total > 0) rows.push(["Share of the range", `${Math.round((rec.n / total) * 100)}%`]);
  return {
    title: label,
    sub: formatDayHeading(rec.day),
    rows,
    bar: total > 0 ? { value: rec.n, max: total, label: `${rec.n === total ? "All" : Math.round((rec.n / total) * 100) + "%"} of ${noun} in range` } : undefined,
    note: avg > 0 ? `That's ${times(rec.n / avg)} your average active day${rank > 1 ? ` (ranks #${rank})` : ""}.` : undefined,
    accent: "var(--violet)",
  };
}

function busiestTip(r: StatsRecords, daily: DailyStat[]): Tip {
  const b = r.busiest_day;
  if (!b) return { title: "Busiest day", note: "No work blocks yet." };
  const d = daily.find((x) => x.day === b.day);
  const active = daily.filter((x) => x.work_seconds > 0);
  const avg = active.length ? active.reduce((a, x) => a + x.work_seconds, 0) / active.length : 0;
  const rows: [string, string][] = [["Worked", formatDuration(b.seconds)]];
  if (d) {
    rows.push(["Personal", formatDuration(d.personal_seconds)], ["Meetings", formatDuration(d.meeting_seconds)]);
    if (d.first_at && d.last_at) rows.push(["Active", `${d.first_at} \u2013 ${d.last_at}`]);
    rows.push(["Tickets", nf.format(d.tickets)], ["Folders", nf.format(d.folders)]);
  }
  if (avg > 0) rows.push(["Average active day", formatDuration(avg)]);
  return {
    title: "Busiest day",
    sub: formatDayHeading(b.day),
    rows,
    bar: avg > 0 ? { value: avg, max: b.seconds, label: "Average active day vs this one" } : undefined,
    note: avg > 0 ? `That's ${times(b.seconds / avg)} your average active day.` : undefined,
    accent: "var(--sage)",
  };
}

function blockTip(r: StatsRecords, daily: DailyStat[]): Tip {
  const b = r.longest_block;
  if (!b) return { title: "Longest block", note: "No work blocks yet." };
  const d = daily.find((x) => x.day === b.day);
  const rows: [string, string][] = [["Length", formatDuration(b.seconds)], ["Ticket", b.ticket ?? "none"]];
  if (d && d.work_seconds > 0) rows.push(["Day total", formatDuration(d.work_seconds)]);
  return {
    title: "Longest block",
    sub: formatDayHeading(b.day),
    rows,
    bar: d && d.work_seconds > 0 ? { value: Math.min(b.seconds, d.work_seconds), max: d.work_seconds, label: `${Math.round((Math.min(b.seconds, d.work_seconds) / d.work_seconds) * 100)}% of that day` } : undefined,
    note: b.seconds >= 3 * 3600 ? "Three hours or more without a break in focus." : b.ticket ? `All on ${b.ticket}.` : "No ticket attached.",
    accent: "var(--slate)",
  };
}

function clockTip(label: string, rec: { day: string; time: string } | null, pick: (d: DailyStat) => string | null, daily: DailyStat[], early: boolean): Tip {
  if (!rec) return { title: label, note: "Nothing recorded in this range yet." };
  // Same days the record itself considers (stats_records.rs: work_seconds > 0).
  const all = daily.filter((d) => d.work_seconds > 0).map(pick).filter((x): x is string => !!x);
  const mid = median(all);
  const rank = all.filter((t) => (early ? t < rec.time : t > rec.time)).length + 1;
  const rows: [string, string][] = [[early ? "Started" : "Finished", rec.time], ["Day", dayName(rec.day)]];
  if (mid) rows.push([`Typical ${early ? "start" : "finish"}`, mid]);
  if (all.length) rows.push(["Days compared", nf.format(all.length)]);
  return {
    title: label,
    sub: formatDayHeading(rec.day),
    rows,
    note: mid ? `Typically ${mid}; this one is #${rank} of ${all.length} days.` : undefined,
    accent: early ? "var(--amber)" : "var(--violet)",
  };
}

/** Rich hover tips per fact key; `daily` (the report's r.daily) unlocks the breakdowns and streak dates. */
export function tipsOf(r: StatsRecords, daily: DailyStat[] = []): Record<string, Tip> {
  const runs = streakRuns(daily);
  const longest = [...runs].reverse().find((x) => x.n === r.longest_streak);
  const lastRun = runs[runs.length - 1];
  const current = lastRun && lastRun.n === r.current_streak ? lastRun : undefined;
  return {
    busiest: busiestTip(r, daily),
    block: blockTip(r, daily),
    early: clockTip("Earliest start", r.earliest_start, (d) => d.first_at, daily, true),
    late: clockTip("Latest finish", r.latest_finish, (d) => d.last_at, daily, false),
    prompts: countTip("Most prompts in a day", "prompts", r.most_prompts, (d) => d.prompts, daily),
    tools: countTip("Most tool calls in a day", "tool calls", r.most_tools, (d) => d.tool_calls, daily),
    longest: streakTip("Longest streak", r.longest_streak, longest, "var(--terracotta)"),
    current: streakTip("Current streak", r.current_streak, current, "var(--sage)"),
  };
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

export function FunFacts({ records, daily }: { records: StatsRecords; daily?: DailyStat[] }) {
  const tips = tipsOf(records, daily);
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
              <Link href={`/${f.day}`} className="stats-fact-link" {...tipProps(tips[f.key])}>
                {body}
              </Link>
            ) : (
              <div className="stats-fact-link" tabIndex={0} {...tipProps(tips[f.key])}>
                {body}
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
