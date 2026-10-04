import { formatTotalHours, shiftDay } from "@/lib/format";
import type { LoggedDay, LoggedEntry } from "@/lib/logged_contract";

/** "8h", "7.5h" — formatTotalHours without the trailing ".0". */
export const hours = (seconds: number) => formatTotalHours(seconds).replace(".0h", "h");

// Fixed English arrays, not toLocaleDateString: identical text on server and client.
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTHS_LONG = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_LONG = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

const split = (day: string) => {
  const [y, m, d] = day.split("-").map(Number);
  return { y, m: m - 1, d, wd: new Date(Date.UTC(y, m - 1, d)).getUTCDay() };
};

/** "Wed 30 Sep" (short) or "Wednesday 30 September" (long). */
export const dayLabel = (day: string, style: "short" | "long") => {
  const { m, d, wd } = split(day);
  return style === "short"
    ? `${WEEKDAYS[wd]} ${d} ${MONTHS[m]}`
    : `${WEEKDAYS_LONG[wd]} ${d} ${MONTHS_LONG[m]}`;
};

export const weekday = (day: string, style: "short" | "long") =>
  (style === "short" ? WEEKDAYS : WEEKDAYS_LONG)[split(day).wd];
export const dayNum = (day: string) => String(split(day).d);
export const monthTitle = (month: string) => {
  const { y, m } = split(`${month}-01`);
  return `${MONTHS_LONG[m]} ${y}`;
};
export const dayTitle = (day: string) => {
  const { y, m, d, wd } = split(day);
  return `${WEEKDAYS_LONG[wd]}, ${d} ${MONTHS_LONG[m]} ${y}`;
};
/** "Week of 28 Sep – 4 Oct", day-first. */
export const weekTitle = (monday: string) => {
  const a = split(monday);
  const b = split(shiftDay(monday, 6));
  return `Week of ${a.d} ${MONTHS[a.m]} – ${b.d} ${MONTHS[b.m]}`;
};

export function Flag({ d, today }: { d: LoggedDay; today: string }) {
  switch (d.state) {
    case "full":
      return (
        <span className="logged-flag">
          <span aria-hidden="true">✓</span>
          <span className="logged-vh">full</span>
        </span>
      );
    case "under":
      return <span className="logged-flag">Short</span>;
    case "dismissed":
      return <span className="logged-flag">{d.dismissal_reason}</span>;
    case "off":
      return <span className="logged-flag">off</span>;
    case "not_fetched":
      return <span className="logged-flag">not fetched</span>;
    case "pending":
      return d.day === today ? <span className="logged-flag logged-today">today</span> : null;
  }
}

export function LoggedEntries({ entries, heading }: { entries: LoggedEntry[]; heading?: boolean }) {
  const total = entries.reduce((s, e) => s + e.seconds, 0);
  return (
    <>
      {heading && (
        <h2 className="logged-entries-head">
          {entries.length} {entries.length === 1 ? "entry" : "entries"} · <span>{hours(total)}</span>
        </h2>
      )}
      <ul className="logged-entries">
        {entries.map((e) => (
          <li key={e.tempo_worklog_id}>
            <span className={e.jira_issue ? "logged-ticket" : "logged-ticket muted"}>
              {e.jira_issue ?? `Tempo #${e.issue_id}`}
            </span>
            <span className="logged-hours">{hours(e.seconds)}</span>
            {e.description ? (
              <span className="logged-desc">{e.description}</span>
            ) : (
              <span className="logged-desc subtle">No description</span>
            )}
            <span className={`source-badge ${e.owner}`}>
              {e.owner === "worklog" ? "sent by worklog" : "hand-logged"}
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}
