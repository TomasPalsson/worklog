import type { ReactNode } from "react";
import Link from "next/link";
import { mondayOf, monthOf, shiftDay, shiftMonth, shiftWeek } from "@/lib/format";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";
import { DayViewIcon, MonthViewIcon, NextIcon, PrevIcon, WeekViewIcon } from "./icons";
import { dayLabel, dayTitle, hours, monthTitle, weekTitle } from "./LoggedEntries";

type View = "month" | "week" | "day";
type Props = {
  view: View;
  /** The month (YYYY-MM), Monday or day the page shows. */
  id: string;
  range: LoggedRange;
  /** The LoggedFetch client component: sits at the right of the stats row. */
  fetch?: ReactNode;
};

const NAMES = { month: ["This month", "month"], week: ["This week", "week"], day: ["Today", "day"] } as const;
const TITLE = { month: monthTitle, week: weekTitle, day: dayTitle };
const shiftBy = { month: shiftMonth, week: shiftWeek, day: shiftDay };

function Tools({ view, id, today }: { view: View; id: string; today: string }) {
  const href = (v: string) => `/logged/${view}/${v}`;
  const here = { month: monthOf(today), week: mondayOf(today), day: today }[view];
  const anchor = { month: here === id ? today : `${id}-01`, week: here === id ? today : id, day: id }[view];
  const [thisLabel, noun] = NAMES[view];
  const shift = shiftBy[view];
  const views = [
    ["Month view", "month", `/logged/month/${monthOf(anchor)}`, MonthViewIcon, "Month"],
    ["Week view", "week", `/logged/week/${mondayOf(anchor)}`, WeekViewIcon, "Week"],
    ["Day view", "day", `/logged/day/${anchor}`, DayViewIcon, "Day"],
  ] as const;
  return (
    <nav className="logged-tools" aria-label={`${noun} navigation`}>
      {views.map(([label, v, to, Icon, text]) => (
        <Link
          key={v}
          href={to}
          className="logged-tool"
          aria-label={label}
          data-labelled=""
          aria-current={v === view ? "page" : undefined}
        >
          <Icon size={18} />
          <span>{text}</span>
        </Link>
      ))}
      <span className="logged-tools-gap" />
      <Link href={href(shift(id, -1))} className="logged-tool" aria-label={`previous ${noun}`} data-tip={`Previous ${noun}`}>
        <PrevIcon size={18} />
      </Link>
      {here === id ? (
        <span className="logged-this" aria-disabled="true" aria-current="true">
          {thisLabel}
        </span>
      ) : (
        <Link href={href(here)} className="logged-this">
          {thisLabel}
        </Link>
      )}
      <Link href={href(shift(id, 1))} className="logged-tool" aria-label={`next ${noun}`} data-tip={`Next ${noun}`}>
        <NextIcon size={18} />
      </Link>
    </nav>
  );
}

function Stat({ label, value, children, tone }: { label: string; value: string | number; children?: ReactNode; tone?: "amber" | "muted" }) {
  return (
    <div className="logged-stat" data-tone={tone}>
      <dt>{label}</dt>
      <dd>{value}</dd>
      {children}
    </div>
  );
}

function Stats({ shown, showShort }: { shown: LoggedDay[]; showShort: boolean }) {
  const logged = shown.reduce((s, d) => s + d.logged_seconds, 0);
  const required = shown.reduce((s, d) => s + (d.required_seconds ?? 0), 0);
  const short = shown.filter((d) => d.state === "under");
  return (
    <dl className="logged-stat-list">
      <Stat label="Logged" value={hours(logged)} />
      <Stat label="Target" value={hours(required)} />
      {showShort && (
        <Stat label="Short" value={short.length} tone={short.length > 0 ? "amber" : "muted"}>
          {short.length > 0 && (
            <dd className="logged-short-days">
              {short.map((d, i) => (
                <span key={d.day}>
                  {i > 0 && ", "}
                  <Link href={`/logged/day/${d.day}`}>{dayLabel(d.day, "short")}</Link>
                </span>
              ))}
            </dd>
          )}
        </Stat>
      )}
    </dl>
  );
}

export function LoggedHeader({ view, id, range, fetch }: Props) {
  const shown = view === "month" ? range.days.filter((d) => monthOf(d.day) === id) : range.days;
  return (
    <header className="day-header logged-head">
      <div className="logged-title-row">
        <h1>{TITLE[view](id)}</h1>
        <Tools view={view} id={id} today={range.today} />
      </div>
      <div className="logged-stats">
        {view !== "day" ? <Stats shown={shown} showShort /> : <span />}
        {fetch}
      </div>
    </header>
  );
}
