"use client";

import { useEffect, useRef, useState } from "react";
import { Pencil, Type } from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";

const HALF_HOUR = 1800;
const DAY_SECONDS = 24 * 3600;

export interface Common {
  taskKey: string;
  actions: TaskActions;
  /** Refetch the ticket's blocks after a write. */
  onSaved: () => void;
  label: string;
  day: TicketDay;
  /** Panel live region. */
  onAnnounce?: (message: string) => void;
}

/** "1.5" / "1,5" / "1.5h" → seconds; NaN when it is not a number. */
function parseHours(draft: string): number {
  const m = /^(\d+(?:[.,]\d+)?)\s*h?$/i.exec(draft.trim());
  return m ? Math.round(Number(m[1].replace(",", ".")) * 3600) : NaN;
}

function hoursProblem(seconds: number): string | null {
  if (!Number.isFinite(seconds) || seconds <= 0) return "Enter hours like 1.5.";
  if (seconds > DAY_SECONDS) return "That is more than a day.";
  if (seconds % HALF_HOUR !== 0) return "Hours must be a half-hour step, like 1.5.";
  return null;
}

/** Why the day's hours are what they are; null when they simply equal the tracked time. */
export function hoursNote(day: TicketDay): string | null {
  const tracked = formatDuration(day.tracked_seconds);
  if (day.hours_set_by_hand) return `Set by hand · ${tracked} tracked`;
  return day.line_seconds !== day.tracked_seconds ? `Rounded to the nearest half hour from ${tracked} tracked` : null;
}

/** Focus returns to the opener when an editor closes: call `back()` right before closing it. */
function useReturnFocus(editing: boolean) {
  const btn = useRef<HTMLButtonElement>(null);
  const want = useRef(false);
  useEffect(() => {
    if (!editing && want.current) btn.current?.focus();
    want.current = false;
  }, [editing]);
  return { btn, back: () => void (want.current = true) };
}

/** Sage confirmation strip ("Sent to Tempo", "Logged 30m"); stays until the panel closes. `focus` moves focus to it on mount. */
export function DaySent({ children, focus, plain }: { children: React.ReactNode; focus?: boolean; plain?: boolean }) {
  const el = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (focus) el.current?.focus();
  }, [focus]);
  return (
    <p ref={el} role="status" tabIndex={focus ? -1 : undefined} className={plain ? "task-day-plain" : "task-day-sent"}>
      {children}
    </p>
  );
}

/** Tracked time rounded the way the daemon rounds a line: nearest half hour, below 15m is nothing. */
const roundedTracked = (day: TicketDay) => Math.round(day.tracked_seconds / HALF_HOUR) * HALF_HOUR;

/** The day's billed hours: a button that turns into a small hours input. */
export function HoursEdit({ taskKey, actions, onSaved, label, day }: Common) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { btn, back } = useReturnFocus(draft !== null);

  async function save() {
    const seconds = parseHours(draft ?? "");
    const problem = hoursProblem(seconds);
    if (problem) return setError(problem);
    await write(seconds);
  }

  /** `null` clears the override: the daemon goes back to the tracked, rounded hours. */
  async function write(seconds: number | null) {
    setBusy(true);
    const res = await actions.saveTempoLineHours({ day: day.day, jira_issue: taskKey }, seconds);
    setBusy(false);
    if (!res.ok) return setError(res.error);
    back();
    setDraft(null);
    onSaved();
  }

  if (draft === null) {
    return (
      <button
        ref={btn}
        type="button"
        className="task-day-hours"
        aria-label={`${formatDuration(day.line_seconds)} — edit hours for ${label}`}
        onClick={() => {
          setDraft(String(day.line_seconds / 3600));
          setError(null);
        }}
      >
        {formatDuration(day.line_seconds)}
        <Pencil size={12} aria-hidden="true" />
      </button>
    );
  }
  return (
    <HoursInput
      label={label}
      draft={draft}
      busy={busy}
      error={error}
      onChange={(v) => {
        setDraft(v);
        setError(null);
      }}
      onSave={save}
      onUseTracked={day.hours_set_by_hand ? () => write(null) : undefined}
      tracked={formatDuration(roundedTracked(day))}
      onCancel={() => {
        back();
        setDraft(null);
      }}
    />
  );
}

interface HoursInputProps {
  label: string;
  draft: string;
  busy: boolean;
  error: string | null;
  onChange: (v: string) => void;
  onSave: () => void;
  onCancel: () => void;
  /** Present only while the hours are set by hand. */
  onUseTracked?: () => void;
  tracked: string;
}

function HoursInput({ label, draft, busy, error, onChange, onSave, onCancel, onUseTracked, tracked }: HoursInputProps) {
  return (
    <span className="task-day-edit">
      <label>
        <span className="task-sr">{`Hours for ${label}`}</span>
        <input
          type="text"
          inputMode="decimal"
          aria-label={`Hours for ${label}`}
          value={draft}
          autoFocus
          disabled={busy}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onSave();
            else if (e.key === "Escape") {
              e.preventDefault();
              onCancel();
            }
          }}
        />
        <span aria-hidden="true">h</span>
      </label>
      <button type="button" className="task-btn-secondary" disabled={busy} onClick={onSave}>
        Save hours
      </button>
      {onUseTracked && (
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={onUseTracked}>
          {`Use tracked time (${tracked})`}
        </button>
      )}
      <button type="button" className="task-btn-secondary" disabled={busy} onClick={onCancel}>
        Cancel
      </button>
      {error && (
        <span role="alert" className="task-error">
          {error}
        </span>
      )}
    </span>
  );
}

/** "Edit" for the line text; opens a textarea that saves through the line-text action. */
export function TextEdit({ taskKey, actions, onSaved, label, day, children }: Common & { children?: React.ReactNode }) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { btn: editBtn, back } = useReturnFocus(draft !== null);
  const close = () => {
    back();
    setDraft(null);
  };

  // Esc cancels this edit only: the saved text stays and the panel stays open.
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    close();
  }

  async function save() {
    setBusy(true);
    const res = await actions.saveTempoLineText({ day: day.day, jira_issue: taskKey }, draft ?? "");
    setBusy(false);
    if (!res.ok) return setError(res.error);
    close();
    onSaved();
  }

  if (draft === null) {
    return (
      <div className="task-day-textrow">
        {children}
        <button
          ref={editBtn}
          type="button"
          className="task-icon-btn"
          aria-label={`Edit Tempo text for ${label}`}
          data-tip="Edit Tempo text"
          onClick={() => {
            setDraft(day.line_text);
            setError(null);
          }}
        >
          <Type size={14} aria-hidden="true" />
        </button>
      </div>
    );
  }
  return (
    <span className="task-day-text-edit">
      <textarea
        aria-label={`Line text for ${label}`}
        rows={3}
        value={draft}
        autoFocus
        disabled={busy}
        onKeyDown={onKeyDown}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
      />
      <span className="task-day-edit">
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={save}>
          Save text
        </button>
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={close}>
          Cancel
        </button>
        {error && (
          <span role="alert" className="task-error">
            {error}
          </span>
        )}
      </span>
    </span>
  );
}
