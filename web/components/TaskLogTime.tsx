"use client";

import { useState } from "react";

import { formatDuration, todayISO } from "@/lib/format";
import type { TaskActions } from "./TaskCard";

const CHIPS: [string, number][] = [
  ["15m", 15],
  ["30m", 30],
  ["1h", 60],
  ["2h", 120],
];

function defaultStart(): string {
  const d = new Date();
  const m = Math.floor(d.getMinutes() / 15) * 15;
  return `${String(d.getHours()).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

/** First problem with the form, or null. */
function problem(f: { day: string; start: string; minutes: number; description: string }): string | null {
  if (!f.description) return "Say what you did.";
  if (f.description.length > 500) return "Keep it to 500 characters.";
  if (!Number.isInteger(f.minutes) || f.minutes < 1 || f.minutes > 720) return "Length must be 1 to 720 minutes.";
  if (!f.day || f.day > todayISO()) return "Pick today or an earlier day.";
  if (!/^\d\d:\d\d$/.test(f.start)) return "Pick a start time.";
  return null;
}

/** Inline "Log time" form: creates a manual block on the ticket. */
export function TaskLogTime({
  taskKey,
  actions,
  onLogged,
  onClose,
}: {
  taskKey: string;
  actions: TaskActions;
  /** Called after the block is created, with the announcement text. */
  onLogged: (message: string) => void;
  onClose: () => void;
}) {
  const [day, setDay] = useState(todayISO);
  const [start, setStart] = useState(defaultStart);
  const [length, setLength] = useState("60");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const minutes = length.trim() === "" ? NaN : Number(length);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const form = { day, start, minutes, description: description.trim() };
    const bad = problem(form);
    if (bad) return setError(bad);
    setBusy(true);
    setError(null);
    const res = await actions.logTicketTime(taskKey, form);
    setBusy(false);
    if (!res.ok) return setError(res.error);
    onLogged(`Logged ${formatDuration(minutes * 60)} on ${taskKey}.`);
  }

  return (
    <form className="task-log-form" onSubmit={submit} noValidate>
      <div className="task-log-row">
        <label>
          Day
          <input type="date" value={day} max={todayISO()} disabled={busy} onChange={(e) => setDay(e.target.value)} />
        </label>
        <label>
          Start
          <input type="time" value={start} disabled={busy} onChange={(e) => setStart(e.target.value)} />
        </label>
        <label>
          Length (minutes)
          <input type="number" min={1} max={720} value={length} disabled={busy} onChange={(e) => setLength(e.target.value)} />
        </label>
      </div>
      <div className="task-log-chips">
        {CHIPS.map(([text, m]) => (
          <button key={text} type="button" className="task-btn-secondary" disabled={busy} onClick={() => setLength(String(m))}>
            {text}
          </button>
        ))}
      </div>
      <label>
        What you did
        <textarea rows={3} maxLength={500} value={description} disabled={busy} onChange={(e) => setDescription(e.target.value)} />
      </label>
      {error && (
        <p role="alert" className="task-error">
          {error}
        </p>
      )}
      <div className="task-log-actions">
        <button type="submit" className="task-btn-primary" disabled={busy}>
          {busy ? "Logging…" : `Log ${Number.isFinite(minutes) ? formatDuration(minutes * 60) : ""}`.trim()}
        </button>
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={onClose}>
          Cancel
        </button>
      </div>
    </form>
  );
}
