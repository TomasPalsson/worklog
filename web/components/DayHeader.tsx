import Link from "next/link";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { formatTotalHours, shiftDay, todayISO } from "@/lib/format";
import { DateJumper } from "./DateJumper";
import { StandupButton } from "./StandupButton";

/** Apró's billable-hours target (reikningshæfi), in percent. */
const BILLABLE_GOAL = 70;

interface Props {
  day: string;
  heading: string;
  totalHours: string;
  blockCount: number;
  unassigned: number;
  /** Optional personal-only summary suffix, e.g. "2.3h personal" — when
   * present, rendered muted after the work total. */
  personalSummary?: string;
  /** What the ticket cards add up to once rounded — the invoice number. */
  billedSeconds?: number;
  /** Work-only tracked time, to show how far rounding moved the bill. */
  trackedSeconds?: number;
  /** Share of line hours billable per Mirres; null when unknown. */
  billablePercent?: number | null;
}

export function DayHeader({
  day,
  heading,
  totalHours,
  blockCount,
  unassigned,
  personalSummary,
  billedSeconds,
  trackedSeconds = 0,
  billablePercent = null,
}: Props) {
  const today = todayISO();
  const prev = shiftDay(day, -1);
  const next = shiftDay(day, 1);
  const isToday = day === today;
  const billedDelta = (billedSeconds ?? 0) - trackedSeconds;

  return (
    <header className="day-header">
      <div className="day-title">
        <h1>{heading}</h1>
        <div className="day-total" aria-label="summary">
          {totalHours} · {blockCount} {blockCount === 1 ? "block" : "blocks"}
          {personalSummary && (
            <>
              {" · "}
              <span className="day-total-personal">{personalSummary}</span>
            </>
          )}
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
      {billedSeconds !== undefined && (
        <div className="day-billed" aria-label="billed hours">
          <div className="day-billed-figure">
            <span className="day-billed-value">{formatTotalHours(billedSeconds)}</span>
            <span className="day-billed-unit">billed</span>
          </div>
          {Math.abs(billedDelta) >= 60 && (
            <div className={`day-billed-delta${billedDelta > 0 ? " up" : ""}`}>
              {billedDelta > 0 ? "+" : "−"}
              {formatTotalHours(Math.abs(billedDelta))} vs tracked
            </div>
          )}
          {billablePercent != null && (
            <div className={`day-billable ${billablePercent >= BILLABLE_GOAL ? "ok" : "low"}`}>
              <span className="day-billable-text">
                <span className="day-billable-value">{billablePercent}% billable</span>
                <span className="day-billable-goal">· goal {BILLABLE_GOAL}%</span>
              </span>
              <span
                className="day-billable-meter"
                role="meter"
                aria-label="Billable share of billed hours"
                title={`${billablePercent}% billable · goal ${BILLABLE_GOAL}% (Mirres)`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={billablePercent}
                aria-valuetext={`${billablePercent}% of billed hours billable, goal ${BILLABLE_GOAL}%`}
              >
                <span className="day-billable-fill" style={{ width: `${Math.min(billablePercent, 100)}%` }} />
                <span className="day-billable-tick" style={{ left: `${BILLABLE_GOAL}%` }} />
              </span>
            </div>
          )}
        </div>
      )}
      <nav className="day-nav" aria-label="day navigation">
        <Link
          href={`/${prev}`}
          className="day-nav-btn"
          aria-label="previous day"
          data-tip="Previous day"
        >
          <ChevronLeft size={16} strokeWidth={1.75} />
        </Link>
        {/* Always rendered: the arrows must not shift under the cursor. */}
        {isToday ? (
          <span className="day-nav-btn today" aria-disabled="true" aria-current="date">
            Today
          </span>
        ) : (
          <Link href={`/${today}`} className="day-nav-btn today" data-tip="Jump to today">
            Today
          </Link>
        )}
        <Link
          href={`/${next}`}
          className="day-nav-btn"
          aria-label="next day"
          data-tip="Next day"
        >
          <ChevronRight size={16} strokeWidth={1.75} />
        </Link>
        <DateJumper focusedDay={day} view="day" />
      </nav>
      {isToday && <StandupButton />}
    </header>
  );
}
