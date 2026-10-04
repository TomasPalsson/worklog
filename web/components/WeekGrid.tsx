import type { CSSProperties } from "react";
import Link from "next/link";
import type { Block } from "@/lib/types";
import { formatDuration, formatRange, todayISO } from "@/lib/format";
import { FlagIcon } from "./icons";
import { assignLanes, blockSpan, hourHeight, mergeRuns, runActivities, timelineRange, type Run } from "@/lib/weekTimeline";

interface DayColumn {
  day: string;
  blocks: Block[];
  totalSeconds: number;
}

const pad = (h: number) => `${String(h).padStart(2, "0")}:00`;

function label(r: Run<Block>) {
  const ticket = r.jira_issue ?? "No ticket";
  const head = `${formatRange(r.started_at, r.ended_at)} · ${ticket} · ${formatDuration(r.workSeconds)}`;
  return r.count > 1 ? `${head} across ${r.count} blocks` : head;
}

const mergeKeyOf = (r: Run<Block>) => `${r.is_personal ? "p" : r.status}|${r.jira_issue ?? ""}`;

function title(r: Run<Block>) {
  const members = r.blocks.map((b) => `${formatRange(b.started_at, b.ended_at)}${b.description ? ` ${b.description}` : ""}`);
  return [label(r), ...members].join("\n");
}

type Range = ReturnType<typeof timelineRange>;

function DayCol({ col, range, hh, today }: { col: DayColumn; range: Range; hh: number; today: string }) {
  const runs = mergeRuns(col.blocks);
  const placed = runs.flatMap((r) => {
    const s = r.is_personal ? null : blockSpan(r, range);
    return s && s.length >= 1 ? [{ r, s }] : [];
  });
  const lanes = assignLanes(placed.map(({ s }) => ({ start: s.top, end: s.top + s.length })));
  return (
    <div className="week-col" data-today={col.day === today ? "" : undefined} role="list" aria-label={col.day}>
      {runs
        .filter((r) => r.is_personal)
        .map((r) => {
          const s = blockSpan(r, range);
          if (!s) return null;
          return (
            <Link
              key={`${mergeKeyOf(r)}|${r.started_at}`}
              href={`/${col.day}`}
              className="week-block week-personal"
              role="listitem"
              title={title(r)}
              aria-label={label(r)}
              style={{ top: `calc(${s.top / 60} * var(--hour))`, height: `max(calc(${s.length / 60} * var(--hour)), 6px)` }}
            />
          );
        })}
      {placed.map(({ r, s }, i) => {
        const { lane, lanes: n } = lanes[i];
        const px = Math.max((s.length / 60) * hh, 10);
        const m = Math.min(n, 2);
        const stacked = lane >= 2;
        const tiny = px < 18;
        const left = stacked ? `calc(${(lane % 2) * 50}% + ${6 * (lane - 1)}px)` : `${(lane / m) * 100}%`;
        const width = stacked ? "50%" : `${100 / m}%`;
        return (
          <Link
            key={`${mergeKeyOf(r)}|${r.started_at}`}
            href={`/${col.day}`}
            className="week-block"
            role="listitem"
            data-status={r.status}
            data-lanes={m}
            data-tiny={tiny ? "" : undefined}
            title={title(r)}
            aria-label={label(r)}
            style={{
              top: `calc(${s.top / 60} * var(--hour))`,
              height: `max(calc(${s.length / 60} * var(--hour)), 10px)`,
              left: tiny ? `calc(${left} + 2px)` : left,
              width: tiny ? `calc(${width} - 4px)` : width,
            }}
          >
            {px >= 18 && (
              <span className="week-block-row">
                {r.jira_issue ? (
                  <span className="week-block-ticket">{r.jira_issue}</span>
                ) : (
                  <span className="week-block-flagged">
                    <FlagIcon size={10} />
                    <span>{r.description || "No ticket"}</span>
                  </span>
                )}
                {px < 40 && <span className="week-block-dur">{formatDuration(r.workSeconds)}</span>}
              </span>
            )}
            {px >= 40 && (
              <>
                <span className="week-block-meta">
                  {formatDuration(r.workSeconds)}
                  {r.count > 1 && ` · ${r.count} blocks`}
                </span>
                {runActivities(r)
                  .filter((a) => a.description !== (r.jira_issue ?? (r.description || "No ticket")))
                  .map((a) => (
                    <span key={a.description} className="week-block-desc">
                      <span className="week-block-act-dur">{formatDuration(a.seconds)}</span>
                      <span className="week-block-act-text">{a.description}</span>
                    </span>
                  ))}
              </>
            )}
          </Link>
        );
      })}
    </div>
  );
}

/**
 * Read-only week calendar: each run of same-ticket blocks sits at its real
 * start time with a height proportional to its span. Clicking a run opens that
 * day. Personal runs are a hatched backdrop; work runs share lanes on overlap.
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
