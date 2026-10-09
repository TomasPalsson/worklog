"use client";

import { useState } from "react";
import { CircleAlert, Plus } from "lucide-react";
import { addNoteBlock } from "@/app/actions-note-block";
import { formatClock } from "@/lib/format";
import { NOTE_MAX_CHARS, NOTE_MAX_MINUTES, TICKET_KEY_RE } from "@/lib/noteBlock";
import type { JiraTicket } from "@/lib/types";
import { useNoteJob } from "./useNoteJob";

const DEFAULT_START = "09:00";
const LIST_ID = "note-ticket-options";

interface Props {
  day: string;
  tickets: JiraTicket[];
  /** Latest block `ended_at` of the day; seeds the start field. */
  lastEnd: string | null;
}

/** "+" on the day page: saves a note block, then tracks the AI rewrite. */
export function AddNoteBlock({ day, tickets, lastEnd }: Props) {
  const [open, setOpen] = useState(false);
  const { track, running } = useNoteJob(day);

  return (
    <>
      <button
        type="button"
        className="action-btn"
        aria-label="Add note block"
        aria-expanded={open}
        title="Add a block from a rough note — the AI writes the description"
        onClick={() => setOpen((o) => !o)}
      >
        <Plus />
      </button>
      {running.size > 0 && (
        <span className="note-writing" role="status">
          Writing…
        </span>
      )}
      {open && (
        <NoteForm
          day={day}
          tickets={tickets}
          defaultStart={lastEnd ? formatClock(lastEnd) : DEFAULT_START}
          onSaved={(blockId) => {
            track(blockId);
            setOpen(false);
          }}
          onCancel={() => setOpen(false)}
        />
      )}
    </>
  );
}

type Field = "Start" | "Minutes" | "Ticket" | "Note";
const ERROR_ID = "note-form-error";

/** The first problem with the form, and the field it belongs to. */
function problem(f: { start: string; length: number; key: string; text: string }): [Field, string] | null {
  if (!/^\d\d:\d\d$/.test(f.start)) return ["Start", "Pick a start time."];
  if (!Number.isInteger(f.length) || f.length < 1 || f.length > NOTE_MAX_MINUTES)
    return ["Minutes", `Minutes must be 1 to ${NOTE_MAX_MINUTES}.`];
  if (!TICKET_KEY_RE.test(f.key)) return ["Ticket", "Invalid ticket key"];
  if (!f.text) return ["Note", "Write a note"];
  if (f.text.length > NOTE_MAX_CHARS) return ["Note", `Keep the note to ${NOTE_MAX_CHARS} characters.`];
  return null;
}

function NoteForm({
  day,
  tickets,
  defaultStart,
  onSaved,
  onCancel,
}: {
  day: string;
  tickets: JiraTicket[];
  defaultStart: string;
  onSaved: (blockId: number) => void;
  onCancel: () => void;
}) {
  const [start, setStart] = useState(defaultStart);
  const [minutes, setMinutes] = useState("30");
  const [ticket, setTicket] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [badField, setBadField] = useState<Field | null>(null);
  // The field a message belongs to is marked and points at the message.
  const mark = (name: Field) =>
    badField === name ? { "aria-invalid": true as const, "aria-describedby": ERROR_ID } : {};
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const key = ticket.trim();
    const text = note.trim();
    const length = minutes.trim() === "" ? NaN : Number(minutes);
    const bad = problem({ start, length, key, text });
    setBadField(bad?.[0] ?? null);
    setError(bad?.[1] ?? null);
    if (bad) {
      // Put the cursor where the fix goes.
      form.querySelector<HTMLInputElement>(`[data-field="${bad[0]}"]`)?.focus();
      return;
    }
    setBusy(true);
    const res = await addNoteBlock({ jira_issue: key, day, start, minutes: length, note: text });
    setBusy(false);
    if (!res.ok) return setError(res.error);
    onSaved(res.data.id);
  }

  return (
    <form
      className="note-form"
      aria-labelledby="note-form-title"
      onSubmit={submit}
      // Escape only closes an untouched form; typed text is discarded with Cancel, never by accident.
      onKeyDown={(e) => e.key === "Escape" && !ticket.trim() && !note.trim() && onCancel()}
      noValidate
    >
      <div className="note-form-head">
        <h2 id="note-form-title" className="task-label">
          New block
        </h2>
        <span className="note-form-hint">Write a rough note — the AI turns it into the Tempo text.</span>
      </div>
      <div className="note-form-fields">
        <label className="note-field">
          <span>Start</span>
          <input type="time" data-field="Start" {...mark("Start")} value={start} disabled={busy} onChange={(e) => setStart(e.target.value)} />
        </label>
        <label className="note-field">
          <span>Minutes</span>
          <input
            type="number"
            inputMode="numeric"
            data-field="Minutes"
            {...mark("Minutes")}
            min={1}
            max={NOTE_MAX_MINUTES}
            value={minutes}
            disabled={busy}
            onChange={(e) => setMinutes(e.target.value)}
          />
        </label>
        <label className="note-field note-field-ticket">
          <span>Ticket</span>
          <input
            list={LIST_ID}
            data-field="Ticket"
            {...mark("Ticket")}
            value={ticket}
            placeholder="ABC-123"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            disabled={busy}
            onChange={(e) => setTicket(e.target.value)}
          />
        </label>
        <datalist id={LIST_ID}>
          {tickets.map((t) => (
            <option key={t.key} value={t.key}>
              {t.summary ?? ""}
            </option>
          ))}
        </datalist>
        <label className="note-field note-field-note">
          <span>Note</span>
          <input
            data-field="Note"
            {...mark("Note")}
            value={note}
            placeholder="fixed login bug"
            disabled={busy}
            onChange={(e) => setNote(e.target.value)}
          />
        </label>
      </div>
      <div className="note-form-foot">
        {error && (
          <p role="alert" id={ERROR_ID} className="note-form-error">
            <CircleAlert aria-hidden="true" />
            {error}
          </p>
        )}
        <button type="button" className="action-btn" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="action-btn primary" disabled={busy}>
          {busy ? "Saving…" : "Add"}
        </button>
      </div>
    </form>
  );
}
