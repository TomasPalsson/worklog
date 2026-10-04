import { formatTotalHours } from "@/lib/format";
import type { LoggedDay, LoggedEntry } from "@/lib/logged_contract";

/** "8h", "7.5h" — formatTotalHours without the trailing ".0". */
export const hours = (seconds: number) => formatTotalHours(seconds).replace(".0h", "h");

const part = (day: string, opts: Intl.DateTimeFormatOptions) => {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString("en-GB", { timeZone: "UTC", ...opts });
};

/** "Wed 30 Sep" (short) or "Wednesday 30 September" (long). */
export const dayLabel = (day: string, style: "short" | "long") =>
  `${part(day, { weekday: style })} ${part(day, { day: "numeric", month: style })}`;

export const weekday = (day: string, style: "short" | "long") => part(day, { weekday: style });
export const dayNum = (day: string) => part(day, { day: "numeric" });
export const monthTitle = (month: string) => part(`${month}-01`, { month: "long", year: "numeric" });
export const dayTitle = (day: string) =>
  `${part(day, { weekday: "long" })}, ${part(day, { day: "numeric", month: "long", year: "numeric" })}`;

export function Flag({ d, today }: { d: LoggedDay; today: string }) {
  switch (d.state) {
    case "full":
      return (
        <span className="logged-cell-flag">
          <span aria-hidden="true">✓</span>
          <span className="logged-vh">full</span>
        </span>
      );
    case "under":
      return <span className="logged-cell-flag">Short</span>;
    case "dismissed":
      return <span className="logged-cell-flag">{d.dismissal_reason}</span>;
    case "off":
      return <span className="logged-cell-flag">off</span>;
    case "not_fetched":
      return <span className="logged-cell-flag">not fetched</span>;
    case "pending":
      return <span className="logged-cell-flag">{d.day === today ? "today" : ""}</span>;
  }
}

export function LoggedEntries({ entries }: { entries: LoggedEntry[] }) {
  return (
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
  );
}
