import Link from "next/link";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";
import { DismissDay } from "./DismissDay";
import { Flag, LoggedEntries, dayLabel, dayNum, hours, weekday } from "./LoggedEntries";
import { LoggedMeter } from "./LoggedMeter";

function Strip({ range }: { range: LoggedRange }) {
  return (
    <ol className="logged-week-strip">
      {range.days.map((d) => (
        <li key={d.day}>
          <a href={`#d-${d.day}`} data-state={d.state}>
            <span className="logged-strip-dow">{weekday(d.day, "short")}</span>
            <span className="logged-strip-num">{dayNum(d.day)}</span>
            {(d.required_seconds ?? 0) > 0 && d.state !== "not_fetched" ? (
              <LoggedMeter logged={d.logged_seconds} required={d.required_seconds} state={d.state} orientation="column" />
            ) : (
              <span className="logged-meter-gap" aria-hidden="true" />
            )}
            <span className="logged-strip-hours">
              {d.state === "not_fetched" ? "—" : d.logged_seconds > 0 || (d.required_seconds ?? 0) > 0 ? hours(d.logged_seconds) : ""}
            </span>
            <Flag d={d} today={range.today} />
          </a>
        </li>
      ))}
    </ol>
  );
}

function Card({ d, today }: { d: LoggedDay; today: string }) {
  const hasReq = (d.required_seconds ?? 0) > 0;
  const noEntries = d.entries.length === 0;
  const collapsed = noEntries && (d.state === "off" || (d.state === "pending" && (d.day > today || (d.day === today && !hasReq))));
  const dismissProps = {
    day: d.day,
    state: d.state as "under" | "dismissed",
    reason: d.dismissal_reason,
    loggedSeconds: d.logged_seconds,
    requiredSeconds: d.required_seconds,
  };
  return (
    <li id={`d-${d.day}`} className="logged-week-day" data-state={d.state} data-collapsed={collapsed ? "" : undefined}>
      <div className="logged-week-head">
        <div className="logged-week-title">
          <Link href={`/logged/day/${d.day}`}>
            <strong>{weekday(d.day, "long")}</strong>
            <span> · {dayLabel(d.day, "short").slice(4)}</span>
          </Link>
          {d.state === "dismissed" && <DismissDay {...dismissProps} compact />}
        </div>
        <div className="logged-week-meta">
          {d.state !== "not_fetched" && (hasReq || d.logged_seconds > 0) && (
            <span className="logged-week-hours">
              {hours(d.logged_seconds)}
              {hasReq && ` / ${hours(d.required_seconds ?? 0)}`}
            </span>
          )}
          {hasReq && d.state !== "not_fetched" && (
            <LoggedMeter logged={d.logged_seconds} required={d.required_seconds} state={d.state} />
          )}
          {d.state !== "dismissed" && <Flag d={d} today={today} />}
        </div>
        {d.state === "under" && <DismissDay {...dismissProps} compact />}
      </div>
      {!collapsed && (
        <div className="logged-week-body">
          {d.state === "not_fetched" ? (
            <p className="logged-muted">Not fetched from Tempo yet — use Refresh above.</p>
          ) : noEntries ? (
            <p className="logged-muted">Nothing logged in Tempo.</p>
          ) : (
            <LoggedEntries entries={d.entries} />
          )}
        </div>
      )}
    </li>
  );
}

export function LoggedWeek({ range }: { range: LoggedRange }) {
  return (
    <>
      <Strip range={range} />
      <ol className="logged-week">
        {range.days.map((d) => (
          <Card key={d.day} d={d} today={range.today} />
        ))}
      </ol>
    </>
  );
}
