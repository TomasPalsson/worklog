import Link from "next/link";
import { CheckCircle2, Circle, Flag, Pencil } from "lucide-react";
import type { Block } from "@/lib/types";
import {
  formatDuration,
  formatRange,
  formatTotalHours,
  shortMonthDay,
  shortWeekday,
  todayISO,
} from "@/lib/format";

interface DayColumn {
  day: string;
  blocks: Block[];
  totalSeconds: number;
}

interface Props {
  days: DayColumn[];
}

/**
 * Read-only 7-column grid. Clicking a column header or any block opens
 * the existing per-day edit page. Personal blocks live in a separate
 * dimmed list at the bottom of each column so the focus stays on work.
 */
export function WeekGrid({ days }: Props) {
  const today = todayISO();
  return (
    <div className="week-grid" role="list">
      {days.map((col) => {
        const isToday = col.day === today;
        const work = col.blocks.filter((b) => !b.is_personal);
        const personal = col.blocks.filter((b) => b.is_personal);
        const personalSeconds = personal.reduce(
          (acc, b) => acc + b.duration_seconds,
          0,
        );
        const workSeconds = Math.max(0, col.totalSeconds - personalSeconds);
        return (
          <section
            key={col.day}
            role="listitem"
            className={`week-day-col${isToday ? " is-today" : ""}`}
            aria-label={`${shortWeekday(col.day)} ${col.day}`}
          >
            <Link href={`/${col.day}`} className="week-day-header">
              <span className="week-day-weekday">{shortWeekday(col.day)}</span>
              <span className="week-day-date">{shortMonthDay(col.day)}</span>
              <span className="week-day-total" aria-label="day total">
                {formatTotalHours(workSeconds)}
              </span>
            </Link>

            {work.length === 0 && personal.length === 0 ? (
              <p className="week-day-empty">—</p>
            ) : (
              <ul className="week-day-blocks" role="list">
                {work.map((b) => (
                  <li key={b.id}>
                    <BlockRow block={b} day={col.day} />
                  </li>
                ))}
                {personal.length > 0 && (
                  <li className="week-day-personal-group">
                    <span className="week-day-personal-label">
                      {personal.length} personal · {formatTotalHours(personalSeconds)}
                    </span>
                    <ul className="week-day-blocks personal" role="list">
                      {personal.map((b) => (
                        <li key={b.id}>
                          <BlockRow block={b} day={col.day} />
                        </li>
                      ))}
                    </ul>
                  </li>
                )}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}

function BlockRow({ block, day }: { block: Block; day: string }) {
  const synced =
    block.tempo_worklog_id !== null && block.tempo_worklog_id !== "";
  const StatusIcon = block.is_personal
    ? Circle
    : synced
      ? block.dirty
        ? Pencil
        : CheckCircle2
      : block.jira_issue
        ? Circle
        : Flag;
  const statusClass = block.is_personal
    ? "is-personal"
    : synced
      ? block.dirty
        ? "is-dirty"
        : "is-synced"
      : block.jira_issue
        ? ""
        : "is-unassigned";
  const ticket = block.jira_issue ?? (block.is_personal ? "personal" : "—");
  const desc = block.description ?? "";
  return (
    <Link href={`/${day}`} className={`week-block ${statusClass}`}>
      <StatusIcon size={12} strokeWidth={1.75} className="week-block-icon" />
      <span className="week-block-time">
        {formatRange(block.started_at, block.ended_at)}
      </span>
      <span className="week-block-ticket">{ticket}</span>
      <span className="week-block-dur">
        {formatDuration(block.duration_seconds)}
      </span>
      {desc && <span className="week-block-desc">{desc}</span>}
    </Link>
  );
}
