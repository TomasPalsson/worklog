import Link from "next/link";
import { monthOf } from "@/lib/format";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";
import { Flag, dayLabel, dayNum, hours, weekday } from "./LoggedEntries";

const MONDAY_FIRST = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function stateWord(d: LoggedDay): string {
  const h = `${hours(d.logged_seconds)} of ${hours(d.required_seconds ?? 0)}`;
  switch (d.state) {
    case "full": return `${h}, full`;
    case "under": return `${h}, short`;
    case "dismissed": return `${h}, marked fine: ${d.dismissal_reason}`;
    case "off": return "day off";
    case "pending": return "pending";
    case "not_fetched": return "not fetched yet";
  }
}

export function LoggedMonth({ range, month }: { range: LoggedRange; month: string }) {
  const { today } = range;
  const short = range.days.filter((d) => d.state === "under" && monthOf(d.day) === month);
  return (
    <>
      {short.length > 0 && (
        <p className="logged-short">
          Short:{" "}
          {short.map((d, i) => (
            <span key={d.day}>
              {i > 0 && ", "}
              <Link href={`/logged/day/${d.day}`}>{dayLabel(d.day, "short")}</Link>
            </span>
          ))}
        </p>
      )}
      <ol className="logged-month">
        {MONDAY_FIRST.map((w) => (
          <li key={w} className="logged-month-head" aria-hidden="true">
            {w}
          </li>
        ))}
        {range.days.map((d) => {
          const isToday = d.day === today;
          const outside = monthOf(d.day) !== month ? "" : undefined;
          const showHours = d.state !== "not_fetched" && !(d.state === "off" && d.logged_seconds === 0);
          return (
            <li key={d.day} data-outside={outside}>
              <Link
                className="logged-cell"
                data-state={d.state}
                data-outside={outside}
                href={`/logged/day/${d.day}`}
                aria-label={`${dayLabel(d.day, "long")}: ${stateWord(d)}`}
              >
                <span className="logged-cell-dow">{weekday(d.day, "short")}</span>
                <span
                  className="logged-cell-num"
                  aria-current={isToday ? "date" : undefined}
                  data-today={isToday ? "" : undefined}
                >
                  {dayNum(d.day)}
                </span>
                {d.state === "not_fetched" && <span className="logged-cell-hours">—</span>}
                {showHours && <span className="logged-cell-hours">{hours(d.logged_seconds)}</span>}
                {d.state !== "not_fetched" && (d.required_seconds ?? 0) > 0 && (
                  <span className="logged-cell-req">of {hours(d.required_seconds ?? 0)}</span>
                )}
                <Flag d={d} today={today} />
              </Link>
            </li>
          );
        })}
      </ol>
    </>
  );
}
