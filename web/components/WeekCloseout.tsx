"use client";

import { useState, type ReactNode } from "react";
import Link from "next/link";

import { loadCloseout, pullTempoWeek } from "@/app/actions-hub";
import { runSync } from "@/app/actions";
import { formatTotalHours, todayISO } from "@/lib/format";
import type { CloseoutDay, WeekCloseout as Closeout } from "@/lib/types";
import { syncWeek, type WeekSyncDeps } from "@/lib/weekSync";
import { RefreshIcon } from "./icons";
import { dayLabel, hours, weekday } from "./LoggedEntries";
import { LoggedMeter } from "./LoggedMeter";

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

export interface CloseoutActions extends WeekSyncDeps {
  loadCloseout: (monday: string) => Promise<Result<Closeout>>;
}

const realActions: CloseoutActions = {
  pull: pullTempoWeek,
  syncDay: (day) => runSync(day, false),
  loadCloseout,
};

const isGap = (d: CloseoutDay) =>
  d.required_seconds !== null && d.required_seconds > 0 && d.tempo_seconds < d.required_seconds;

const meterState = (d: CloseoutDay) =>
  isGap(d) ? "under" : d.tempo_seconds >= (d.required_seconds ?? 0) ? "full" : "pending";

function DayCard({ d, today, worked }: { d: CloseoutDay; today: string; worked: number }) {
  const req = d.required_seconds;
  const future = d.day > today;
  const gap = isGap(d) && !future;
  const idle = worked === 0 && d.tempo_seconds === 0 && !req;
  const done = !gap && !future && d.pending_lines === 0 && d.unticketed_seconds === 0 && !idle;
  return (
    <Link
      href={`/${d.day}`}
      data-testid={`closeout-${d.day}`}
      className={`week-day-card${gap ? " closeout-gap" : ""}`}
      data-today={d.day === today ? "" : undefined}
      data-idle={idle ? "" : undefined}
    >
      <span className="week-day-top">
        <span className="week-day-dow">{weekday(d.day, "short")}</span>
        <span className="week-day-date">{dayLabel(d.day, "short").split(" ").slice(1).join(" ")}</span>
      </span>
      <span className="week-day-hours">{hours(worked)}</span>
      {req ? <LoggedMeter logged={d.tempo_seconds} required={req} state={future ? "pending" : meterState(d)} /> : null}
      <span className="week-day-sub">
        {req === null ? "not pulled" : `${hours(d.tempo_seconds)} / ${hours(req)} in Tempo`}
      </span>
      <span className="week-chips">
        {gap && <span className="week-chip" data-k="gap">Gap</span>}
        {d.pending_lines > 0 && <span className="week-chip" data-k="pending">{d.pending_lines} pending</span>}
        {d.unticketed_seconds > 0 && (
          <span className="week-chip" data-k="ticket">{hours(d.unticketed_seconds)} no ticket</span>
        )}
        {done && <span className="week-chip" data-k="done">✓ done</span>}
      </span>
    </Link>
  );
}

function Stat({ label, tone, children }: { label: string; tone?: "amber" | "muted"; children: ReactNode }) {
  return (
    <div className="logged-stat" data-tone={tone}>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

export function WeekCloseout({
  closeout,
  actions = realActions,
  workSeconds,
  personalSeconds = 0,
  daySeconds = {},
}: {
  closeout: Closeout;
  actions?: CloseoutActions;
  /** Work seconds of the week; defaults to the sum of the days' logged seconds. */
  workSeconds?: number;
  personalSeconds?: number;
  /** Work seconds per day (all blocks); falls back to the day's ticketed `logged_seconds`. */
  daySeconds?: Record<string, number>;
}) {
  const [current, setCurrent] = useState(closeout);
  const [busy, setBusy] = useState<"pull" | "sync" | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    const res = await actions.loadCloseout(current.monday);
    if (res.ok) setCurrent(res.data);
    else setError(res.error);
  }

  async function onPull() {
    setBusy("pull");
    setError(null);
    const res = await actions.pull(current.monday);
    if (res.ok) await refresh();
    else setError(res.error);
    setBusy(null);
  }

  async function onSync() {
    if (!confirming) {
      setConfirming(true);
      // Give the user 4s to confirm before reverting.
      setTimeout(() => setConfirming(false), 4000);
      return;
    }
    setConfirming(false);
    setBusy("sync");
    setError(null);
    const pending = current.days.filter((d) => d.pending_lines > 0).map((d) => d.day);
    const out = await syncWeek(current.monday, pending, actions);
    if (!out.ok) setError(out.day ? `${out.day}: ${out.error}` : out.error);
    await refresh();
    setBusy(null);
  }

  const sum = (f: (d: CloseoutDay) => number) => current.days.reduce((t, d) => t + f(d), 0);
  const tempo = sum((d) => d.tempo_seconds);
  const required = sum((d) => d.required_seconds ?? 0);
  const unticketed = sum((d) => d.unticketed_seconds);
  const worked = workSeconds ?? sum((d) => d.logged_seconds);
  const today = todayISO();

  return (
    <section className="week-closeout">
      <div className="logged-stats">
        <dl className="logged-stat-list">
          <Stat label="Worked">{hours(worked)}</Stat>
          <Stat label="In Tempo">
            {hours(tempo)}
            {current.days.some((d) => d.required_seconds !== null) && (
              <span className="week-stat-target"> / {formatTotalHours(required)}</span>
            )}
          </Stat>
          <Stat label="No ticket" tone={unticketed > 0 ? "amber" : "muted"}>{hours(unticketed)}</Stat>
          <Stat label="Personal" tone="muted">{hours(personalSeconds)}</Stat>
        </dl>
        <div className="week-actions">
          <button type="button" className="action-btn" disabled={busy !== null} onClick={onPull}>
            <RefreshIcon size={16} className={busy === "pull" ? "logged-spin" : undefined} />
            {busy === "pull" ? "Pulling…" : "Pull from Tempo"}
          </button>
          <button
            type="button"
            className="week-primary"
            data-confirm={confirming ? "" : undefined}
            disabled={busy !== null}
            onClick={onSync}
          >
            {busy === "sync" ? "Syncing…" : confirming ? "Confirm sync week" : "Sync week"}
          </button>
        </div>
      </div>
      {error && <p role="alert" className="week-error">{error}</p>}
      <div className="week-strip">
        {current.days.map((d) => (
          <DayCard key={d.day} d={d} today={today} worked={daySeconds[d.day] ?? d.logged_seconds} />
        ))}
      </div>
    </section>
  );
}
