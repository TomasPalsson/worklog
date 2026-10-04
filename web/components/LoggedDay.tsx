import type { LoggedRange } from "@/lib/logged_contract";
import { DismissDay } from "./DismissDay";
import { Flag, LoggedEntries, hours } from "./LoggedEntries";

export function LoggedDay({ range }: { range: LoggedRange }) {
  const d = range.days[0];
  const notFetched = d.state === "not_fetched";
  return (
    <div className="logged-day">
      <div className="logged-day-sum" data-state={d.state}>
        <span className="logged-day-hours">{notFetched ? "—" : hours(d.logged_seconds)}</span>
        {!notFetched && d.required_seconds != null && <span>of {hours(d.required_seconds)} required</span>}
        <Flag d={d} today={range.today} />
      </div>
      {(d.state === "under" || d.state === "dismissed") && (
        <DismissDay
          day={d.day}
          state={d.state}
          reason={d.dismissal_reason}
          loggedSeconds={d.logged_seconds}
          requiredSeconds={d.required_seconds}
        />
      )}
      {d.entries.length > 0 ? (
        <LoggedEntries entries={d.entries} />
      ) : (
        <div className="empty-state">
          <h2>{notFetched ? "Not fetched from Tempo yet." : "Nothing logged in Tempo for this day."}</h2>
          {notFetched && <p>Press Refresh from Tempo above to load it.</p>}
          {!notFetched && d.day < range.today && <p>Log time in Tempo, then press Refresh from Tempo.</p>}
        </div>
      )}
    </div>
  );
}
