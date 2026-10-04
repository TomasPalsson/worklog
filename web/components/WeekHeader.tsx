import Link from "next/link";
import { mondayOf, shiftWeek, todayISO } from "@/lib/format";
import { DateJumper } from "./DateJumper";
import { NextIcon, PrevIcon } from "./icons";
import { weekTitle } from "./LoggedEntries";

/** Title row + week navigation; the stats and actions live in WeekCloseout. */
export function WeekHeader({ monday }: { monday: string }) {
  const thisMonday = mondayOf(todayISO());
  const isCurrentWeek = monday === thisMonday;

  return (
    <header className="week-head">
      <h1>{weekTitle(monday)}</h1>
      <nav className="logged-tools" aria-label="week navigation">
        <Link
          href={`/week/${shiftWeek(monday, -1)}`}
          className="logged-tool"
          aria-label="previous week"
          data-tip="Previous week"
        >
          <PrevIcon size={18} />
        </Link>
        {/* Always rendered: the arrows must not shift under the cursor. */}
        {isCurrentWeek ? (
          <span className="logged-this" aria-disabled="true" aria-current="true">
            This week
          </span>
        ) : (
          <Link href={`/week/${thisMonday}`} className="logged-this">
            This week
          </Link>
        )}
        <Link
          href={`/week/${shiftWeek(monday, 1)}`}
          className="logged-tool"
          aria-label="next week"
          data-tip="Next week"
        >
          <NextIcon size={18} />
        </Link>
        <DateJumper focusedDay={monday} view="week" />
      </nav>
    </header>
  );
}
