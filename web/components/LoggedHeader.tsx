import Link from "next/link";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { formatWeekRange, mondayOf, monthOf, shiftDay, shiftMonth, shiftWeek } from "@/lib/format";
import type { LoggedRange } from "@/lib/logged_contract";
import { dayTitle, hours, monthTitle } from "./LoggedEntries";

type Props = {
  view: "month" | "week" | "day";
  /** The month (YYYY-MM), Monday or day the page shows. */
  id: string;
  range: LoggedRange;
};

const NAMES = { month: ["This month", "month"], week: ["This week", "week"], day: ["Today", "day"] } as const;

const TITLE = {
  month: monthTitle,
  week: (monday: string) => `Week of ${formatWeekRange(monday)}`,
  day: dayTitle,
};

export function LoggedHeader({ view, id, range }: Props) {
  const today = range.today;
  const shift = { month: shiftMonth, week: shiftWeek, day: shiftDay }[view];
  const href = (v: string) => `/logged/${view}/${v}`;
  const here = { month: monthOf(today), week: mondayOf(today), day: today }[view];
  const anchor = { month: here === id ? today : `${id}-01`, week: here === id ? today : id, day: id }[view];
  const [thisLabel, noun] = NAMES[view];

  const shown = view === "month" ? range.days.filter((d) => monthOf(d.day) === id) : range.days;
  const logged = shown.reduce((s, d) => s + d.logged_seconds, 0);
  const required = shown.reduce((s, d) => s + (d.required_seconds ?? 0), 0);
  const short = shown.filter((d) => d.state === "under").length;

  const views = [
    ["Month", "month", `/logged/month/${monthOf(anchor)}`],
    ["Week", "week", `/logged/week/${mondayOf(anchor)}`],
    ["Day", "day", `/logged/day/${anchor}`],
  ] as const;

  return (
    <header className="day-header">
      <div className="day-title">
        <h1>{TITLE[view](id)}</h1>
        {view !== "day" && (
          <div className="day-total">
            {hours(logged)} logged · {hours(required)} required
            {short > 0 && <span className="logged-short-count"> · {short} short</span>}
          </div>
        )}
      </div>
      <nav className="day-nav" aria-label={`${noun} navigation`}>
        <div className="logged-views">
          {views.map(([label, v, to]) => (
            <Link key={v} href={to} aria-current={v === view ? "page" : undefined}>
              {label}
            </Link>
          ))}
        </div>
        <Link href={href(shift(id, -1))} className="day-nav-btn" aria-label={`previous ${noun}`}>
          <ChevronLeft size={16} strokeWidth={1.75} />
        </Link>
        {here === id ? (
          <span className="day-nav-btn today" aria-disabled="true" aria-current="true">
            {thisLabel}
          </span>
        ) : (
          <Link href={href(here)} className="day-nav-btn today">
            {thisLabel}
          </Link>
        )}
        <Link href={href(shift(id, 1))} className="day-nav-btn" aria-label={`next ${noun}`}>
          <ChevronRight size={16} strokeWidth={1.75} />
        </Link>
      </nav>
    </header>
  );
}
