"use client";

import type { ReactNode } from "react";

import { formatDuration } from "@/lib/format";
import { shortDate } from "@/lib/taskBoard";
import { relativeDay, ticketHours, type TicketHours } from "@/lib/ticketHours";
import { Skeleton } from "./TaskSkeleton";
import type { BlocksLoad } from "./useWorkLog";

function Figure({ value, label, tone, children }: { value: string; label: string; tone?: "ok" | "none"; children?: ReactNode }) {
  return (
    <span className="task-fig" data-tone={tone}>
      <span className="task-fig-value">{value}</span>
      <span className="task-fig-label">{label}</span>
      {children}
    </span>
  );
}

const plural = (n: number) => `${n} ${n === 1 ? "day" : "days"}`;

function Ledger({ h, today, onTempo }: { h: TicketHours; today: string; onTempo: () => void }) {
  const first = shortDate(h.first as string);
  const last = shortDate(h.last as string);
  return (
    <>
      <div className="task-hours-head">
        <span className="task-hours-total">{formatDuration(h.total)}</span>
        <span className="task-hours-of">on this ticket</span>
      </div>
      <p className="task-hours-sub">{`${plural(h.days)} · ${first === last && h.last === today ? "" : `${first === last ? first : `${first} – ${last}`} · `}Last worked ${relativeDay(h.last as string, today)}${h.tracked > 0 && h.tracked !== h.total ? ` · rounded up per day from ${formatDuration(h.tracked)} tracked` : ""}`}</p>
      <div className="task-hours-figs">
        <Figure value={formatDuration(h.week)} label="This week" />
        <Figure value={formatDuration(h.month)} label="This month" />
        <Figure value={h.unsent > 0 ? formatDuration(h.unsent) : "All sent"} label="To send to Tempo" tone={h.unsent > 0 ? "none" : "ok"}>
          {h.unsent > 0 && (
            <button type="button" className="task-review" onClick={onTempo}>
              {h.unsentDays === 1 ? "Show unsent day" : `Show ${h.unsentDays} unsent days`}
            </button>
          )}
        </Figure>
      </div>
    </>
  );
}

export interface TaskHoursProps {
  taskKey: string;
  load: BlocksLoad;
  today: string;
  onTempo: () => void;
  onRetry: () => void;
}

/** The time ledger: every hour on the ticket, this week and month, and what is still to send to Tempo. */
export function TaskHours({ taskKey, load, today, onTempo, onRetry }: TaskHoursProps) {
  let body: ReactNode;
  if (load.s === "loading") body = <Skeleton />;
  else if (load.s === "error")
    body = (
      <div className="task-load-error" role="alert">
        <p>{`Couldn't load your time on ${taskKey}`}</p>
        <button type="button" className="task-btn-secondary" onClick={onRetry}>
          Try again
        </button>
      </div>
    );
  else if (load.data.days.length === 0) return null; // the work log's empty row says it
  else body = <Ledger h={ticketHours(load.data, today)} today={today} onTempo={onTempo} />;
  return (
    <section className="task-hours" aria-labelledby="task-hours-label">
      <h3 id="task-hours-label" className="task-label">
        Your time
      </h3>
      {body}
    </section>
  );
}
