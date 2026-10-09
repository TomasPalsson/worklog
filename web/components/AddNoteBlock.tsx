"use client";

import { useState } from "react";
import { Plus } from "lucide-react";
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
        title="Add a block from a rough note — the AI writes the description"
        onClick={() => setOpen((o) => !o)}
      >
        <Plus />
      </button>
      {running.size > 0 && <span className="note-writing">Writing…</span>}
      {open && (
        <NoteForm
          day={day}
          tickets={tickets}
          defaultStart={lastEnd ? formatClock(lastEnd) : DEFAULT_START}
          onSaved={(blockId) => {
            track(blockId);
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

function problem(f: { start: string; length: number; key: string; text: string }): string | null {
  if (!/^\d\d:\d\d$/.test(f.start)) return "Pick a start time.";
  if (!Number.isInteger(f.length) || f.length < 1 || f.length > NOTE_MAX_MINUTES)
    return `Minutes must be 1 to ${NOTE_MAX_MINUTES}.`;
  if (!TICKET_KEY_RE.test(f.key)) return "Invalid ticket key";
  if (!f.text) return "Write a note";
  if (f.text.length > NOTE_MAX_CHARS) return `Keep the note to ${NOTE_MAX_CHARS} characters.`;
  return null;
}

function NoteForm({
  day,
  tickets,
  defaultStart,
  onSaved,
}: {
  day: string;
  tickets: JiraTicket[];
  defaultStart: string;
  onSaved: (blockId: number) => void;
}) {
  const [start, setStart] = useState(defaultStart);
  const [minutes, setMinutes] = useState("30");
  const [ticket, setTicket] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const key = ticket.trim();
    const text = note.trim();
    const length = minutes.trim() === "" ? NaN : Number(minutes);
    const bad = problem({ start, length, key, text });
    setError(bad);
    if (bad) return;
    setBusy(true);
    const res = await addNoteBlock({ jira_issue: key, day, start, minutes: length, note: text });
    setBusy(false);
    if (!res.ok) return setError(res.error);
    onSaved(res.data.id);
  }

  return (
    <form className="note-form" onSubmit={submit} noValidate>
      <label>
        Start
        <input type="time" value={start} disabled={busy} onChange={(e) => setStart(e.target.value)} />
      </label>
      <label>
        Minutes
        <input type="number" min={1} max={NOTE_MAX_MINUTES} value={minutes} disabled={busy} onChange={(e) => setMinutes(e.target.value)} />
      </label>
      <label>
        Ticket
        <input list={LIST_ID} value={ticket} disabled={busy} onChange={(e) => setTicket(e.target.value)} />
      </label>
      <datalist id={LIST_ID}>
        {tickets.map((t) => (
          <option key={t.key} value={t.key}>
            {t.summary ?? ""}
          </option>
        ))}
      </datalist>
      <label className="note-form-note">
        Note
        <textarea rows={2} value={note} disabled={busy} onChange={(e) => setNote(e.target.value)} />
      </label>
      {error && (
        <p role="alert" className="note-form-error">
          {error}
        </p>
      )}
      <button type="submit" className="action-btn primary" disabled={busy}>
        {busy ? "Saving…" : "Add"}
      </button>
    </form>
  );
}
