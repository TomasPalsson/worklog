"use client";

import { useState } from "react";

import { loadCloseout, pullTempoWeek } from "@/app/actions-hub";
import { runSync } from "@/app/actions";
import { formatTotalHours } from "@/lib/format";
import type { CloseoutDay, WeekCloseout as Closeout } from "@/lib/types";
import { syncWeek, type WeekSyncDeps } from "@/lib/weekSync";

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

function statusOf(d: CloseoutDay): string {
  if (isGap(d)) return "Gap";
  return d.pending_lines === 0 ? "✓" : "";
}

function DayRow({ d }: { d: CloseoutDay }) {
  return (
    <tr data-testid={`closeout-${d.day}`}>
      <td>{d.day}</td>
      <td>{formatTotalHours(d.logged_seconds)}</td>
      <td>{formatTotalHours(d.synced_seconds)}</td>
      <td>{formatTotalHours(d.tempo_seconds)}</td>
      <td>{d.required_seconds === null ? "not pulled" : formatTotalHours(d.required_seconds)}</td>
      <td>{formatTotalHours(d.unticketed_seconds)}</td>
      <td>{d.pending_lines}</td>
      <td>{statusOf(d)}</td>
    </tr>
  );
}

export function WeekCloseout({
  closeout,
  actions = realActions,
}: {
  closeout: Closeout;
  actions?: CloseoutActions;
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

  return (
    <section className="closeout">
      <div className="closeout-actions">
        <button type="button" disabled={busy !== null} onClick={onPull}>
          {busy === "pull" ? "Pulling…" : "Pull from Tempo"}
        </button>
        <button type="button" disabled={busy !== null} onClick={onSync}>
          {busy === "sync" ? "Syncing…" : confirming ? "Confirm sync week" : "Sync week"}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      <table>
        <thead>
          <tr>
            <th>Day</th>
            <th>Logged</th>
            <th>Synced</th>
            <th>In Tempo</th>
            <th>Required</th>
            <th>Unticketed</th>
            <th>Pending</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {current.days.map((d) => (
            <DayRow key={d.day} d={d} />
          ))}
        </tbody>
      </table>
    </section>
  );
}
