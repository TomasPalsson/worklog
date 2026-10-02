"use client";

import { useState } from "react";
import { Pencil, UploadCloud } from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";

const HALF_HOUR = 1800;
const DAY_SECONDS = 24 * 3600;

interface Common {
  taskKey: string;
  actions: TaskActions;
  /** Refetch the ticket's blocks after a write. */
  onSaved: () => void;
  label: string;
  day: TicketDay;
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

/** The day's billed hours: a button that turns into a small hours input. */
export function HoursEdit({ taskKey, actions, onSaved, label, day }: Common) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    const seconds = parseHours(draft ?? "");
    const problem = hoursProblem(seconds);
    if (problem) return setError(problem);
    setBusy(true);
    const res = await actions.saveTempoLineHours({ day: day.day, jira_issue: taskKey }, seconds);
    setBusy(false);
    if (!res.ok) return setError(res.error);
    setDraft(null);
    onSaved();
  }

  if (draft === null) {
    return (
      <button
        type="button"
        className="task-day-hours"
        aria-label={`Change hours for ${label}`}
        title="Change the hours sent to Tempo"
        onClick={() => {
          setDraft(String(day.line_seconds / 3600));
          setError(null);
        }}
      >
        {formatDuration(day.line_seconds)}
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
      onCancel={() => setDraft(null)}
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
}

function HoursInput({ label, draft, busy, error, onChange, onSave, onCancel }: HoursInputProps) {
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
export function TextEdit({ taskKey, actions, onSaved, label, day }: Common) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    const res = await actions.saveTempoLineText({ day: day.day, jira_issue: taskKey }, draft ?? "");
    setBusy(false);
    if (!res.ok) return setError(res.error);
    setDraft(null);
    onSaved();
  }

  if (draft === null) {
    return (
      <button
        type="button"
        className="task-link-btn"
        aria-label={`Edit text for ${label}`}
        onClick={() => {
          setDraft(day.line_text);
          setError(null);
        }}
      >
        <Pencil size={12} aria-hidden="true" />
        Edit
      </button>
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
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
      />
      <span className="task-day-edit">
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={save}>
          Save text
        </button>
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={() => setDraft(null)}>
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

type Step =
  | { s: "idle" }
  | { s: "running" }
  | { s: "preview" }
  | { s: "done"; msg: string }
  | { s: "error"; msg: string };

/** Two-step Tempo sync for one ticket-day: dry run, preview, then send on confirm. */
export function SyncTool({ taskKey, actions, onSaved, label, day }: Common) {
  const [step, setStep] = useState<Step>({ s: "idle" });
  const [sending, setSending] = useState(false);

  async function dryRun() {
    setStep({ s: "running" });
    const res = await actions.runSync(day.day, true, taskKey);
    if (!res.ok) return setStep({ s: "error", msg: res.error });
    if (res.data.errors.length > 0) return setStep({ s: "error", msg: res.data.errors.join("; ") });
    setStep({ s: "preview" });
  }

  async function send() {
    setSending(true);
    const res = await actions.runSync(day.day, false, taskKey);
    setSending(false);
    if (!res.ok) return setStep({ s: "error", msg: res.error });
    if (res.data.errors.length > 0) return setStep({ s: "error", msg: res.data.errors.join("; ") });
    setStep({
      s: "done",
      msg: res.data.synced > 0 ? "Sent to Tempo." : `Nothing sent (${res.data.skipped} skipped).`,
    });
    onSaved();
  }

  const busy = step.s === "running" || sending;
  return (
    <div className="task-day-sync">
      {(step.s === "idle" || step.s === "running" || step.s === "error" || step.s === "done") && (
        <button
          type="button"
          className="task-link-btn"
          aria-label={`Sync ${label} to Tempo`}
          disabled={busy}
          onClick={dryRun}
        >
          <UploadCloud size={12} aria-hidden="true" />
          {step.s === "running" ? "Syncing…" : "Sync to Tempo"}
        </button>
      )}
      {step.s === "preview" && (
        <SyncPreview taskKey={taskKey} day={day} sending={sending} onSend={send} onCancel={() => setStep({ s: "idle" })} />
      )}
      {step.s === "done" && <p role="status">{step.msg}</p>}
      {step.s === "error" && (
        <p role="alert" className="task-error">
          {step.msg}
        </p>
      )}
    </div>
  );
}

function SyncPreview(p: {
  taskKey: string;
  day: TicketDay;
  sending: boolean;
  onSend: () => void;
  onCancel: () => void;
}) {
  const { day } = p;
  return (
    <div className="task-day-preview">
      <p>
        {`Will send ${formatDuration(day.line_seconds)} to Tempo for ${p.taskKey} on ${day.day}${
          day.line_text ? `: “${day.line_text}”` : ""
        }`}
      </p>
      <span className="task-day-edit">
        <button type="button" className="task-btn-primary" disabled={p.sending} onClick={p.onSend}>
          {p.sending ? "Syncing…" : "Send to Tempo"}
        </button>
        <button type="button" className="task-btn-secondary" disabled={p.sending} onClick={p.onCancel}>
          Cancel
        </button>
      </span>
    </div>
  );
}
