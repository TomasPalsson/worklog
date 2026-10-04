import Link from "next/link";
import type { LoggedRange } from "@/lib/logged_contract";
import { DismissDay } from "./DismissDay";
import { Flag, LoggedEntries, dayLabel, hours } from "./LoggedEntries";

export function LoggedWeek({ range }: { range: LoggedRange }) {
  return (
    <ol className="logged-week">
      {range.days.map((d) => (
        <li key={d.day} className="logged-week-day" data-state={d.state}>
          <div className="logged-week-head">
            <Link href={`/logged/day/${d.day}`}>{dayLabel(d.day, "short")}</Link>
            {d.state !== "not_fetched" && (
              <span className="logged-week-hours">
                {hours(d.logged_seconds)}
                {(d.required_seconds ?? 0) > 0 && ` of ${hours(d.required_seconds ?? 0)}`}
              </span>
            )}
            <Flag d={d} today={range.today} />
          </div>
          {d.state === "not_fetched" ? (
            <p className="logged-muted">Not fetched from Tempo yet — use Refresh above.</p>
          ) : (
            <LoggedEntries entries={d.entries} />
          )}
          {(d.state === "under" || d.state === "dismissed") && (
            <DismissDay
              day={d.day}
              state={d.state}
              reason={d.dismissal_reason}
              loggedSeconds={d.logged_seconds}
              requiredSeconds={d.required_seconds}
            />
          )}
        </li>
      ))}
    </ol>
  );
}
