"use client";

import { useState, type ReactNode } from "react";

import { pullTempoWeek } from "@/app/actions-hub";
import { formatDuration, mondayOf } from "@/lib/format";
import { localToday } from "@/lib/taskBoard";
import { toast } from "@/lib/toast";
import type { TaskRow, TicketBlocks, TicketDetail, TodayTotals } from "@/lib/types";
import type { BlocksLoad } from "./useWorkLog";

export function Row({ term, children }: { term: string; children: ReactNode }) {
  return (
    <div className="task-dl-row">
      <dt>{term}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** A day with this much more work than Tempo reads as a gap worth a warning. */
const GAP_SECONDS = 15 * 60;

export interface TodayView {
  /** `about 2h 15m`: said as an estimate. */
  worked: string;
  /** `1h 30m in Tempo`, or `not pulled yet`. */
  tempo: string;
  /** The Tempo figure on its own: `1h 30m`, or `Not pulled yet`. */
  tempoValue: string;
  /** `45m not in Tempo yet` when the day is behind by a quarter hour or more; else null. */
  gap: string | null;
  /** The pulled day is level with (or past) the work: said in the ok tone. */
  level: boolean;
  ticket: string | null;
}

const inTempo = (s: number | null) => (s === null ? "not pulled yet" : `${formatDuration(s)} in Tempo`);

/** Today's worked-vs-Tempo numbers in words; null when there is nothing to say yet. */
export function todayView(t: TodayTotals | undefined): TodayView | null {
  if (!t || (t.worked_seconds <= 0 && !t.in_tempo_seconds)) return null;
  const pulled = t.in_tempo_seconds !== null;
  const behind = pulled ? t.worked_seconds - (t.in_tempo_seconds as number) : 0;
  const ticketAny = t.ticket_worked_seconds > 0 || !!t.ticket_in_tempo_seconds;
  return {
    worked: `about ${formatDuration(t.worked_seconds)}`,
    tempo: inTempo(t.in_tempo_seconds),
    tempoValue: pulled ? formatDuration(t.in_tempo_seconds as number) : "Not pulled yet",
    gap: behind >= GAP_SECONDS ? `${formatDuration(behind)} not in Tempo yet` : null,
    level: pulled && behind <= 0,
    ticket: ticketAny ? `${formatDuration(t.ticket_worked_seconds)} worked · ${inTempo(t.ticket_in_tempo_seconds)}` : null,
  };
}

/** The gap (warn) or Up to date (ok) under the Today numbers; nothing when Tempo is close but not level. */
export function TodayGap({ view }: { view: TodayView }) {
  if (view.gap)
    return (
      <span className="task-tempo" data-tone="changed">
        {view.gap}
      </span>
    );
  return view.level ? (
    <span className="task-tempo" data-tone="ok">
      Up to date
    </span>
  ) : null;
}

function Figure({ value, label }: { value: string; label: string }) {
  return (
    <span className="task-fig">
      <span className="task-fig-value">{value}</span>
      <span className="task-fig-label">{label}</span>
    </span>
  );
}

/** The card's focal point: what was worked today against what Tempo has, side by side. */
function TodayRow({ view }: { view: TodayView }) {
  return (
    <Row term="Today">
      <span className="task-today">
        <span className="task-figs">
          <Figure value={view.worked} label="Worked" />
          <Figure value={view.tempoValue} label="In Tempo" />
        </span>
        <TodayGap view={view} />
        {view.ticket && <span className="task-today-ticket">{`This ticket: ${view.ticket}`}</span>}
      </span>
    </Row>
  );
}

function LoggedRow({ data }: { data: TicketBlocks | null }) {
  const you = data?.in_tempo_total_seconds ?? null;
  return you === null ? null : <Row term="In Tempo (you)">{formatDuration(you)}</Row>;
}

function EstimateRow({ detail }: { detail: TicketDetail | null }) {
  const estimate = detail?.original_estimate_seconds;
  if (!estimate) return null;
  const spent = detail.time_spent_seconds ?? 0;
  const over = spent - estimate;
  const left = detail.remaining_estimate_seconds ?? Math.max(0, estimate - spent);
  return (
    <Row term="Estimate">
      <span className="task-estimate">
        <span
          className="task-meter"
          role="meter"
          aria-label={`Time logged against the estimate of ${formatDuration(estimate)}`}
          aria-valuemin={0}
          aria-valuemax={estimate}
          aria-valuenow={Math.min(spent, estimate)}
          aria-valuetext={`${formatDuration(spent)} of ${formatDuration(estimate)}`}
          data-over={over > 0 || undefined}
        >
          <span style={{ transform: `scaleX(${Math.min(1, spent / estimate)})` }} />
        </span>
        <span>{`${formatDuration(spent)} of ${formatDuration(estimate)}`}</span>
        {over > 0 ? (
          <span className="task-tempo" data-tone="changed">{`${formatDuration(over)} over`}</span>
        ) : (
          <span>{`${formatDuration(left)} left`}</span>
        )}
      </span>
    </Row>
  );
}

const clock = (iso: string) => {
  const d = new Date(/[zZ]|[+-]\d\d:?\d\d$/.test(iso) ? iso : `${iso}Z`);
  return Number.isNaN(d.getTime()) ? null : d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
};

type Pull = typeof pullTempoWeek;

/** Where the Tempo numbers come from, and a way to bring them up to date from here. */
function PullFoot({ at, pull, onPulled, onAnnounce }: { at: string | null; pull: Pull; onPulled?: () => void | Promise<void>; onAnnounce?: (message: string) => void }) {
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      const res = await pull(mondayOf(localToday()));
      if (res.ok) {
        await onPulled?.();
        onAnnounce?.("Tempo numbers updated");
      } else toast.error(res.error);
    } finally {
      setBusy(false);
    }
  };
  return (
    <p className="task-time-foot">
      {at ? `Tempo numbers from ${clock(at) ?? "an earlier pull"}` : "Tempo not pulled yet"}
      <span aria-hidden="true">{" · "}</span>
      <button type="button" className="task-review" disabled={busy} onClick={run}>
        {busy ? "Refreshing…" : at ? "Refresh this week from Tempo" : "Pull this week from Tempo"}
      </button>
    </p>
  );
}

export interface TimeCardProps {
  task: TaskRow;
  detail: TicketDetail | null;
  load: BlocksLoad;
  /** Reload the work log after a Tempo pull. */
  onPulled?: () => void | Promise<void>;
  /** Try the work log load again after it failed. */
  onRetry?: () => void;
  /** Dialog live region. */
  onAnnounce?: (message: string) => void;
  pull?: Pull;
}

/** One line in the footer's place while the numbers load or when they could not: no bigger a jump than a row. */
function LoadFoot({ load, onRetry }: { load: BlocksLoad; onRetry?: () => void }) {
  if (load.s === "loading") return <p className="task-time-foot">Loading Tempo numbers…</p>;
  if (load.s !== "error") return null;
  return (
    <p className="task-time-foot" role="alert">
      {"Couldn't load Tempo numbers"}
      <span aria-hidden="true">{" · "}</span>
      <button type="button" className="task-review" onClick={onRetry}>
        Try again
      </button>
    </p>
  );
}

export function TimeCard({ task, detail, load, onPulled, onRetry, onAnnounce, pull = pullTempoWeek }: TimeCardProps) {
  const data = load.s === "ok" ? load.data : null;
  const view = todayView(data?.today);
  return (
    <section className="task-side-card task-time" aria-labelledby="task-time-label">
      <h3 id="task-time-label" className="task-label">
        Tempo
      </h3>
      <dl>
        {view ? <TodayRow view={view} /> : task.today_seconds > 0 && <Row term="Today">{formatDuration(task.today_seconds)}</Row>}
        <LoggedRow data={data} />
        <EstimateRow detail={detail} />
      </dl>
      {data ? <PullFoot at={data.pulled_at} pull={pull} onPulled={onPulled} onAnnounce={onAnnounce} /> : <LoadFoot load={load} onRetry={onRetry} />}
    </section>
  );
}
