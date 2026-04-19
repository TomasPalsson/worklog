import Link from "next/link";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { shiftDay, todayISO } from "@/lib/format";
import { ThemeToggle } from "./ThemeToggle";

interface Props {
  day: string;
  heading: string;
  totalHours: string;
  blockCount: number;
  unassigned: number;
}

export function DayHeader({ day, heading, totalHours, blockCount, unassigned }: Props) {
  const today = todayISO();
  const prev = shiftDay(day, -1);
  const next = shiftDay(day, 1);
  const isToday = day === today;

  return (
    <header className="day-header">
      <div className="day-title">
        <h1>{heading}</h1>
        <div className="day-total" aria-label="summary">
          {totalHours} · {blockCount} {blockCount === 1 ? "block" : "blocks"}
          {unassigned > 0 && (
            <>
              {" · "}
              <span style={{ color: "var(--amber-ink)" }}>
                {unassigned} unassigned
              </span>
            </>
          )}
        </div>
      </div>
      <nav className="day-nav" aria-label="day navigation">
        <Link href={`/${prev}`} className="day-nav-btn" aria-label="previous day">
          <ChevronLeft size={16} strokeWidth={1.75} />
        </Link>
        {!isToday && (
          <Link href={`/${today}`} className="day-nav-btn today">
            Today
          </Link>
        )}
        <Link href={`/${next}`} className="day-nav-btn" aria-label="next day">
          <ChevronRight size={16} strokeWidth={1.75} />
        </Link>
        <ThemeToggle />
      </nav>
    </header>
  );
}
