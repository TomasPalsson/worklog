"use client";

import { useId, useRef, useState } from "react";

import type { RawBlock } from "@/lib/types";
import { formatDuration, todayISO } from "@/lib/format";
import type { TaskActions } from "./TaskCard";
import { dayLabel } from "./TaskDayGroup";

const CHIPS: [string, number][] = [
  ["15m", 15],
  ["30m", 30],
  ["1h", 60],
  ["2h", 120],
];

// The browser clock may be in another zone than the daemon's WORKLOG_TZ, so no clock-derived default.
const DEFAULT_START = "09:00";

const descProblem = (d: string) => (!d ? "Say what you did." : d.length > 500 ? "Keep it to 500 characters." : null);
const lengthProblem = (m: number) =>
  !Number.isInteger(m) || m < 1 || m > 720 ? "Length must be 1 to 720 minutes." : null;

/** First problem outside the two inline-validated fields, or null. */
function problem(f: { day: string; start: string }, today: string): string | null {
  if (!f.day || f.day > today) return "Pick today or an earlier day.";
  if (!/^\d\d:\d\d$/.test(f.start)) return "Pick a start time.";
  return null;
}

/** Inline "Log time" form: creates a manual block on the ticket. */
export function TaskLogTime({
  taskKey,
  actions,
  today: serverToday,
  onLogged,
  onClose,
}: {
  taskKey: string;
  /** The daemon's local date (WORKLOG_TZ); the browser's until it is known. */
  today?: string;
  actions: TaskActions;
  /** Called after the block is created, with the announcement text. */
  onLogged: (message: string, block: RawBlock, duration: string) => void;
  onClose: () => void;
}) {
  const today = serverToday ?? todayISO();
  const [day, setDay] = useState(today);
  const [start, setStart] = useState(DEFAULT_START);
  const [length, setLength] = useState("60");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [fieldErr, setFieldErr] = useState<{ length?: string | null; description?: string | null }>({});
  const minutes = length.trim() === "" ? NaN : Number(length);

  const [discarding, setDiscarding] = useState(false);
  const descRef = useRef<HTMLTextAreaElement>(null);
  const lengthId = useId();

  // Esc is scoped to this form (never document-wide). A typed description asks before it is thrown away.
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    if (discarding) return keepEditing();
    cancel();
  }
  function cancel() {
    if (description.trim()) return setDiscarding(true);
    onClose();
  }
  function keepEditing() {
    setDiscarding(false);
    descRef.current?.focus();
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const form = { day, start, minutes, description: description.trim() };
    const errs = { length: lengthProblem(minutes), description: descProblem(form.description) };
    setFieldErr(errs);
    if (errs.length || errs.description) return;
    const bad = problem(form, today);
    if (bad) return setError(bad);
    setBusy(true);
    setError(null);
    const res = await actions.logTicketTime(taskKey, form);
    setBusy(false);
    if (!res.ok) return setError(res.error);
    const duration = formatDuration(minutes * 60);
    onLogged(`Logged ${duration} on ${taskKey}.`, res.data, duration);
  }

  return (
    <form className="task-log-form" onSubmit={submit} onKeyDown={onKeyDown} noValidate>
      <div className="task-log-row">
        <label>
          Day
          <input type="date" value={day} max={today} autoFocus disabled={busy} onChange={(e) => setDay(e.target.value)} />
        </label>
        {/^\d{4}-\d\d-\d\d$/.test(day) && <span className="task-log-day">{dayLabel(day)}</span>}
        <label>
          Start
          <input type="time" value={start} disabled={busy} onChange={(e) => setStart(e.target.value)} />
        </label>
        <div className="task-log-field">
        <label htmlFor={lengthId}>Length</label>
        <span className="task-log-suffixed">
          <input
            id={lengthId}
            type="number"
            min={1}
            max={720}
            value={length}
            disabled={busy}
            aria-invalid={!!fieldErr.length}
            onChange={(e) => {
              setLength(e.target.value);
              setFieldErr((f) => ({ ...f, length: null }));
            }}
            onBlur={() => setFieldErr((f) => ({ ...f, length: lengthProblem(minutes) }))}
          />
          <span aria-hidden="true">min</span>
        </span>
        {fieldErr.length && (
          <span role="alert" className="task-error">
            {fieldErr.length}
          </span>
        )}
        </div>
      </div>
      <div className="task-log-chips">
        {CHIPS.map(([text, m]) => (
          <button
            key={text}
            type="button"
            className="task-btn-secondary"
            aria-pressed={minutes === m}
            disabled={busy}
            onClick={() => setLength(String(m))}>
            {text}
          </button>
        ))}
      </div>
      <div className="task-log-field">
      <label>
        What you did
        <textarea
          ref={descRef}
          rows={3}
          maxLength={500}
          value={description}
          disabled={busy}
          aria-invalid={!!fieldErr.description}
          onChange={(e) => {
            setDescription(e.target.value);
            setFieldErr((f) => ({ ...f, description: null }));
          }}
          onBlur={() => setFieldErr((f) => ({ ...f, description: descProblem(description.trim()) }))}
        />
      </label>
      {fieldErr.description && (
        <span role="alert" className="task-error">
          {fieldErr.description}
        </span>
      )}
      </div>
      {error && (
        <p role="alert" className="task-error">
          {error}
        </p>
      )}
      {discarding && (
        <p role="alert" className="task-log-discard">
          Discard this entry?
          <button type="button" className="task-btn-secondary" onClick={onClose}>
            Discard
          </button>
          <button type="button" className="task-btn-secondary" onClick={keepEditing}>
            Keep editing
          </button>
        </p>
      )}
      <div className="task-log-actions">
        <button type="submit" className="task-btn-primary" disabled={busy}>
          {busy ? "Logging…" : `Log ${Number.isFinite(minutes) ? formatDuration(minutes * 60) : ""}`.trim()}
        </button>
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={cancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
