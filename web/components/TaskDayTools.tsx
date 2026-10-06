"use client";

import { useEffect, useRef, useState } from "react";

import { formatDuration } from "@/lib/format";
import { mergeGroup, undoLastChange } from "@/app/actions";
import { regenerateTempoLineText } from "@/app/actions-tempo-lines";
import type { TicketDay } from "@/lib/types";
import { toast } from "@/lib/toast";
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
  /** Dialog live region. */
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
  return day.line_seconds !== day.tracked_seconds ? `Rounded up to the next half hour from ${tracked} tracked` : null;
}

/** Sage confirmation strip ("Sent to Tempo", "Logged 30m"); stays until the dialog closes. `focus` moves focus to it on mount. */
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

/** Writes the day's billed hours; shared by the hours editor and the "Use tracked time" menu item. */
export function useHoursWrite({ taskKey, actions, onSaved, day }: Common) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** `null` clears the override: the daemon goes back to the tracked, rounded hours. True when it was saved. */
  async function write(seconds: number | null): Promise<boolean> {
    setBusy(true);
    setError(null);
    const res = await actions.saveTempoLineHours({ day: day.day, jira_issue: taskKey }, seconds);
    setBusy(false);
    if (!res.ok) {
      setError(res.error);
      return false;
    }
    onSaved();
    return true;
  }
  return { busy, error, setError, write };
}

export type HoursWrite = ReturnType<typeof useHoursWrite>;

/** The day's billed hours as a small input; mounted only while editing. */
export function HoursEdit({ label, day, io, onDone }: { label: string; day: TicketDay; io: HoursWrite; onDone: () => void }) {
  const [draft, setDraft] = useState(String(day.line_seconds / 3600));
  const [problem, setProblem] = useState<string | null>(null);

  async function save() {
    const seconds = parseHours(draft);
    const bad = hoursProblem(seconds);
    if (bad) return setProblem(bad);
    if (await io.write(seconds)) onDone();
  }

  const error = problem ?? io.error;
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
          disabled={io.busy}
          onChange={(e) => {
            setDraft(e.target.value);
            setProblem(null);
            io.setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") save();
            else if (e.key === "Escape") {
              e.preventDefault();
              onDone();
            }
          }}
        />
        <span aria-hidden="true">h</span>
      </label>
      <button type="button" className="task-btn-secondary" disabled={io.busy} onClick={save}>
        Save hours
      </button>
      <button type="button" className="task-btn-secondary" disabled={io.busy} onClick={onDone}>
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

/** Textarea for the Tempo line text; mounted only while editing, saves through the line-text action. */
export function TextEdit({ taskKey, actions, onSaved, label, day, onDone }: Common & { onDone: () => void }) {
  const [draft, setDraft] = useState(day.line_text);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Esc cancels this edit only: the saved text stays and the dialog stays open.
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    onDone();
  }

  async function save() {
    setBusy(true);
    const res = await actions.saveTempoLineText({ day: day.day, jira_issue: taskKey }, draft);
    setBusy(false);
    if (!res.ok) return setError(res.error);
    onDone();
    onSaved();
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
        <button type="button" className="task-btn-secondary" disabled={busy} onClick={onDone}>
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

/** The day menu's two heavier tools: write the Tempo text again with AI, and merge the day's blocks into one. */
export function useDayOps({ taskKey, onSaved, day }: Common) {
  const [busy, setBusy] = useState<string | null>(null);
  const hasLine = day.line_seconds > 0 || day.line_text !== "";

  /** Runs one busy op: a rejected call (daemon unreachable) toasts and never leaves the busy label stuck. */
  async function run(label: string, op: () => Promise<void>) {
    setBusy(label);
    try {
      await op();
    } catch {
      toast.error("Couldn't reach the worklog service");
    } finally {
      setBusy(null);
    }
  }

  const regenerate = () =>
    run("Writing…", async () => {
      const res = await regenerateTempoLineText({ day: day.day, jira_issue: taskKey });
      if (!res.ok) return void toast.error(`Couldn't write new text — ${res.error}`);
      toast.ok("New Tempo text written");
      onSaved();
    });

  /** The earliest block keeps its place; the rest fold into it. */
  const merge = () =>
    run("Merging…", async () => {
      const [primary, ...rest] = [...day.blocks].sort((a, b) => a.started_at.localeCompare(b.started_at));
      const res = await mergeGroup(primary.id, rest.map((b) => b.id), day.day);
      if (!res.ok) return void toast.error(`Merge failed — ${res.error}`);
      toast.undoable(`Merged ${day.blocks.length} blocks`, () => undoLastChange(day.day));
      onSaved();
    });

  return { busy, hasLine, regenerate, merge, blocks: day.blocks.length };
}

export type DayOps = ReturnType<typeof useDayOps>;
