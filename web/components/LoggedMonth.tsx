import Link from "next/link";
import { monthOf } from "@/lib/format";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";
import { Flag, dayLabel, dayNum, hours, weekday } from "./LoggedEntries";
import { LoggedMeter } from "./LoggedMeter";

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
  return (
    <ol className="logged-month">
      {MONDAY_FIRST.map((w) => (
        <li key={w} className="logged-month-head" aria-hidden="true">
          {w}
        </li>
      ))}
      {range.days.map((d) => {
        const isToday = d.day === today;
        const outside = monthOf(d.day) !== month ? "" : undefined;
        const required = d.required_seconds ?? 0;
        const showHours =
          outside === undefined && d.state !== "not_fetched" && (d.logged_seconds > 0 || (d.day < today && required > 0));
        return (
          <li key={d.day} data-outside={outside}>
            <Link
              className="logged-cell"
              data-state={d.state}
              data-outside={outside}
              href={`/logged/day/${d.day}`}
              aria-label={`${dayLabel(d.day, "long")}: ${stateWord(d)}`}
            >
              <span className="logged-cell-top">
                <span className="logged-cell-date">
                  <span className="logged-cell-dow">{weekday(d.day, "short")}</span>
                  <span
                    className="logged-cell-num"
                    aria-current={isToday ? "date" : undefined}
                    data-today={isToday ? "" : undefined}
                  >
                    {dayNum(d.day)}
                  </span>
                </span>
                {outside === undefined && <Flag d={d} today={today} />}
              </span>
              {outside === undefined && d.state === "not_fetched" && <span className="logged-cell-hours">—</span>}
              {showHours && <span className="logged-cell-hours">{hours(d.logged_seconds)}</span>}
              {d.state !== "not_fetched" && required > 0 && outside === undefined && !(d.state === "pending" && d.day > today) && (
                <LoggedMeter logged={d.logged_seconds} required={d.required_seconds} state={d.state} />
              )}
            </Link>
          </li>
        );
      })}
    </ol>
  );
}
