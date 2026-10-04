import type { CSSProperties } from "react";
import Link from "next/link";
import type { Block } from "@/lib/types";
import { formatDuration, formatRange, todayISO } from "@/lib/format";
import { FlagIcon } from "./icons";
import { assignLanes, blockSpan, hourHeight, timelineRange } from "@/lib/weekTimeline";

interface DayColumn {
  day: string;
  blocks: Block[];
  totalSeconds: number;
}

const pad = (h: number) => `${String(h).padStart(2, "0")}:00`;

function status(b: Block): "synced" | "dirty" | "ticket" | "none" {
  if (b.tempo_worklog_id) return b.dirty ? "dirty" : "synced";
  return b.jira_issue ? "ticket" : "none";
}

function label(b: Block) {
  const ticket = b.jira_issue ?? "No ticket";
  return `${formatRange(b.started_at, b.ended_at)} · ${ticket} · ${formatDuration(b.duration_seconds)}${b.description ? ` · ${b.description}` : ""}`;
}

type Range = ReturnType<typeof timelineRange>;

function DayCol({ col, range, hh, today }: { col: DayColumn; range: Range; hh: number; today: string }) {
  const placed = col.blocks.flatMap((b) => {
    const s = b.is_personal ? null : blockSpan(b, range);
    return s && s.length >= 1 ? [{ b, s }] : [];
  });
  const lanes = assignLanes(placed.map(({ s }) => ({ start: s.top, end: s.top + s.length })));
  return (
    <div className="week-col" data-today={col.day === today ? "" : undefined} role="list" aria-label={col.day}>
      {col.blocks
        .filter((b) => b.is_personal)
        .map((b) => {
          const s = blockSpan(b, range);
          if (!s) return null;
          return (
            <Link
              key={b.id}
              href={`/${col.day}`}
              className="week-block week-personal"
              role="listitem"
              title={label(b)}
              aria-label={label(b)}
              style={{ top: `calc(${s.top / 60} * var(--hour))`, height: `max(calc(${s.length / 60} * var(--hour)), 6px)` }}
            />
          );
        })}
      {placed.map(({ b, s }, i) => {
        const { lane, lanes: n } = lanes[i];
        const px = Math.max((s.length / 60) * hh, 10);
        const m = Math.min(n, 2);
        const stacked = lane >= 2;
        const tiny = px < 18;
        const left = stacked ? `calc(${(lane % 2) * 50}% + ${6 * (lane - 1)}px)` : `${(lane / m) * 100}%`;
        const width = stacked ? "50%" : `${100 / m}%`;
        return (
          <Link
            key={b.id}
            href={`/${col.day}`}
            className="week-block"
            role="listitem"
            data-status={status(b)}
            data-lanes={m}
            data-tiny={tiny ? "" : undefined}
            title={label(b)}
            aria-label={label(b)}
            style={{
              top: `calc(${s.top / 60} * var(--hour))`,
              height: `max(calc(${s.length / 60} * var(--hour)), 10px)`,
              left: tiny ? `calc(${left} + 2px)` : left,
              width: tiny ? `calc(${width} - 4px)` : width,
            }}
          >
            {px >= 18 && (
              <span className="week-block-row">
                {b.jira_issue ? (
                  <span className="week-block-ticket">{b.jira_issue}</span>
                ) : (
                  <span className="week-block-flagged">
                    <FlagIcon size={10} />
                    <span>{b.description || "No ticket"}</span>
                  </span>
                )}
                <span className="week-block-dur">{formatDuration(b.duration_seconds)}</span>
              </span>
            )}
            {px >= 44 && b.jira_issue && b.description && <span className="week-block-desc">{b.description}</span>}
          </Link>
        );
      })}
    </div>
  );
}

/**
 * Read-only week calendar: each block sits at its real start time with a
 * height proportional to its length. Clicking a block opens that day.
 * Personal blocks are a hatched backdrop; work blocks share lanes on overlap.
 */
export function WeekGrid({ days }: { days: DayColumn[] }) {
  const today = todayISO();
  const range = timelineRange(days.flatMap((d) => d.blocks));
  const hours = range.endHour - range.startHour;
  const hh = hourHeight(hours);
  const empty = days.every((d) => d.blocks.length === 0);
  const hourMarks = Array.from({ length: hours + 1 }, (_, i) => range.startHour + i);

  return (
    <>
      <ul className="week-legend" aria-label="legend">
        <li data-k="synced">Synced</li>
        <li data-k="ticket">Ticket, not synced</li>
        <li data-k="none">No ticket</li>
        <li data-k="dirty">Edited since sync</li>
        <li data-k="personal">Personal</li>
      </ul>
      <div className="week-timeline" style={{ "--hour": `${hh}px`, "--hours": hours } as CSSProperties}>
        <div className="week-gutter" aria-hidden="true">
          {hourMarks.map((h) => (
            <span key={h} style={{ top: `calc(${h - range.startHour} * var(--hour))` }}>
              {pad(h)}
            </span>
          ))}
        </div>
        {days.map((col) => (
          <DayCol key={col.day} col={col} range={range} hh={hh} today={today} />
        ))}
        {empty && <p className="week-empty">Nothing tracked this week.</p>}
      </div>
    </>
  );
}
