"use client";

import { useEffect, useRef, useState } from "react";
import { Pencil, Type, UploadCloud } from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { SyncPreview } from "./TaskSyncPreview";

const HALF_HOUR = 1800;
const DAY_SECONDS = 24 * 3600;

interface Common {
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
  return day.line_seconds !== day.tracked_seconds ? `Rounded from ${tracked} tracked` : null;
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

/** Sage confirmation strip ("Sent to Tempo", "Logged 30m"); stays until the panel closes. */
export function DaySent({ children }: { children: React.ReactNode }) {
  return (
    <p role="status" className="task-day-sent">
      {children}
    </p>
  );
}

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
        aria-label={`Edit hours for ${label}`}
        title={hoursNote(day) ?? "Change the hours sent to Tempo"}
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
      <>
        {children}
        <button
          ref={editBtn}
          type="button"
          className="task-link-btn"
          aria-label={`Edit Tempo text for ${label}`}
          onClick={() => {
            setDraft(day.line_text);
            setError(null);
          }}
        >
          <Type size={12} aria-hidden="true" />
          Edit Tempo text
        </button>
      </>
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

type Step =
  | { s: "idle" }
  | { s: "running" }
  | { s: "preview" }
  | { s: "sent"; hours: string }
  | { s: "nothing"; msg: string }
  | { s: "error"; msg: string };

type SyncData = { results?: { status: string; reason: string | null }[] };

/** Plain words for a sync that sent nothing: the first reason the daemon gave, else the likely causes. */
function nothingSent(taskKey: string, label: string, data: SyncData): string {
  const reason = data.results?.find((r) => r.reason)?.reason?.trim().replace(/\.$/, "");
  const head = `Nothing was sent to Tempo for ${taskKey} on ${label}`;
  return reason ? `${head}: ${reason}.` : `${head}. It may already be in Tempo, or have no hours.`;
}

/** Two-step Tempo sync for one ticket-day: dry run, preview, then send on confirm. */
export function SyncTool({ taskKey, actions, onSaved, onAnnounce, label, day, inTempo, changed }: Common & { inTempo: boolean; changed: boolean }) {
  const [step, setStep] = useState<Step>({ s: "idle" });
  const [sending, setSending] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (step.s === "idle" && refocus.current) trigger.current?.focus();
    refocus.current = false;
  }, [step.s]);
  const cancel = () => {
    refocus.current = true;
    setStep({ s: "idle" });
  };

  async function dryRun() {
    setStep({ s: "running" });
    const res = await actions.runSync(day.day, true, taskKey);
    if (!res.ok) return setStep({ s: "error", msg: res.error });
    if (res.data.errors.length > 0) return setStep({ s: "error", msg: res.data.errors.join("; ") });
    if (res.data.synced === 0) return setStep({ s: "nothing", msg: nothingSent(taskKey, label, res.data) });
    setStep({ s: "preview" });
  }

  async function send() {
    setSending(true);
    const res = await actions.runSync(day.day, false, taskKey);
    setSending(false);
    if (!res.ok) return setStep({ s: "error", msg: res.error });
    if (res.data.errors.length > 0) return setStep({ s: "error", msg: res.data.errors.join("; ") });
    if (res.data.synced === 0) return setStep({ s: "nothing", msg: nothingSent(taskKey, label, res.data) });
    const hours = formatDuration(day.line_seconds);
    setStep({ s: "sent", hours });
    onAnnounce?.(`Sent ${hours} to Tempo for ${taskKey} on ${label}.`);
    onSaved();
  }

  const busy = step.s === "running" || sending;
  return (
    <div className="task-day-sync">
      {!inTempo && step.s !== "sent" && step.s !== "preview" && (
        <button
          ref={trigger}
          type="button"
          className="task-link-btn"
          aria-label={changed ? `Preview update ${label} in Tempo` : `Preview sync ${label} to Tempo`}
          disabled={busy}
          onClick={dryRun}
        >
          <UploadCloud size={12} aria-hidden="true" />
          {step.s === "running" ? "Checking…" : changed ? "Preview update" : "Preview sync"}
        </button>
      )}
      {step.s === "preview" && (
        <SyncPreview taskKey={taskKey} label={label} day={day} changed={changed} sending={sending} onSend={send} onCancel={cancel} />
      )}
      {step.s === "sent" && (
        <DaySent>{`Sent to Tempo · ${step.hours}`}</DaySent>
      )}
      {step.s === "nothing" && <p role="status">{step.msg}</p>}
      {step.s === "error" && (
        <p role="alert" className="task-error">
          {step.msg}
        </p>
      )}
    </div>
  );
}
