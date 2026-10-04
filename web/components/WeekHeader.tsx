import Link from "next/link";
import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  formatTotalHours,
  formatWeekRange,
  mondayOf,
  shiftWeek,
  todayISO,
} from "@/lib/format";
import { DateJumper } from "./DateJumper";

interface Props {
  monday: string;
  workSeconds: number;
  workBlocks: number;
  personalSummary?: string;
}

export function WeekHeader({
  monday,
  workSeconds,
  workBlocks,
  personalSummary,
}: Props) {
  const today = todayISO();
  const thisMonday = mondayOf(today);
  const prev = shiftWeek(monday, -1);
  const next = shiftWeek(monday, 1);
  const isCurrentWeek = monday === thisMonday;

  return (
    <header className="day-header week-header">
      <div className="day-title">
        <h1>Week of {formatWeekRange(monday)}</h1>
        <div className="day-total" aria-label="week summary">
          {formatTotalHours(workSeconds)} · {workBlocks}{" "}
          {workBlocks === 1 ? "block" : "blocks"}
          {personalSummary && (
            <>
              {" · "}
              <span className="day-total-personal">{personalSummary}</span>
            </>
          )}
        </div>
      </div>
      <nav className="day-nav" aria-label="week navigation">
        <Link
          href={`/week/${prev}`}
          className="day-nav-btn"
          aria-label="previous week"
        >
          <ChevronLeft size={16} strokeWidth={1.75} />
        </Link>
        {/* Always rendered: the arrows must not shift under the cursor. */}
        {isCurrentWeek ? (
          <span className="day-nav-btn today" aria-disabled="true" aria-current="true">
            This week
          </span>
        ) : (
          <Link href={`/week/${thisMonday}`} className="day-nav-btn today">
            This week
          </Link>
        )}
        <Link
          href={`/week/${next}`}
          className="day-nav-btn"
          aria-label="next week"
        >
          <ChevronRight size={16} strokeWidth={1.75} />
        </Link>
        <DateJumper focusedDay={monday} view="week" />
      </nav>
    </header>
  );
}
