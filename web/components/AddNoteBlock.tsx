"use client";

import { useState } from "react";
import { CircleAlert, CornerDownLeft, Plus, X } from "lucide-react";
import { addNoteBlock } from "@/app/actions-note-block";
import { formatClock } from "@/lib/format";
import { NOTE_MAX_CHARS, NOTE_MAX_MINUTES, TICKET_KEY_RE } from "@/lib/noteBlock";
import { toast } from "@/lib/toast";
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

/** A real 24-hour "HH:MM" (00:00–23:59). */
const isClock = (t: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(t);

/** "9:30" / "930" / "0930" → "09:30"; anything else is left for validation to name. */
function tidyClock(t: string): string {
  const m = /^(\d{1,2}):?(\d{2})$/.exec(t.trim());
  if (!m) return t;
  const tidy = `${m[1].padStart(2, "0")}:${m[2]}`;
  return isClock(tidy) ? tidy : t;
}
const ERROR_ID = "note-form-error";

/** The first problem with the form, and the field it belongs to. */
function problem(f: { start: string; length: number; key: string; text: string }): [Field, string] | null {
  if (!isClock(f.start)) return ["Start", "Pick a start time."];
  if (!Number.isInteger(f.length) || f.length < 1 || f.length > NOTE_MAX_MINUTES)
    return ["Minutes", `Minutes must be 1 to ${NOTE_MAX_MINUTES}.`];
  if (!TICKET_KEY_RE.test(f.key)) return ["Ticket", "Invalid ticket key"];
  if (!f.text) return ["Note", "Write a note"];
  if (f.text.length > NOTE_MAX_CHARS) return ["Note", `Keep the note to ${NOTE_MAX_CHARS} characters.`];
  return null;
}

/** "ends 10:30" from Start + Minutes (warn: past midnight, which the daemon refuses); null while either is unusable. */
function endLabel(start: string, minutes: string): { text: string; warn: boolean } | null {
  const len = minutes.trim() === "" ? NaN : Number(minutes);
  if (!isClock(start) || !Number.isInteger(len) || len < 1 || len > NOTE_MAX_MINUTES) return null;
  const end = Number(start.slice(0, 2)) * 60 + Number(start.slice(3)) + len;
  if (end > 1440) return { text: "ends after midnight", warn: true };
  if (end === 1440) return { text: "ends at midnight", warn: false };
  return { text: `ends ${String(Math.floor(end / 60)).padStart(2, "0")}:${String(end % 60).padStart(2, "0")}`, warn: false };
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
  // Editing the field that is marked wrong retires its message at once.
  const edit = (name: Field, set: (v: string) => void) => (e: React.ChangeEvent<HTMLInputElement>) => {
    set(e.target.value);
    if (badField === name) {
      setBadField(null);
      setError(null);
    }
  };
  const end = endLabel(start, minutes);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const key = ticket.trim();
    const text = note.trim();
    const length = minutes.trim() === "" ? NaN : Number(minutes);
    const at = tidyClock(start);
    setStart(at);
    const bad = problem({ start: at, length, key, text });
    setBadField(bad?.[0] ?? null);
    setError(bad?.[1] ?? null);
    if (bad) {
      // Put the cursor where the fix goes.
      form.querySelector<HTMLInputElement>(`[data-field="${bad[0]}"]`)?.focus();
      return;
    }
    setBusy(true);
    const res = await addNoteBlock({ jira_issue: key, day, start: at, minutes: length, note: text });
    setBusy(false);
    if (!res.ok) return setError(res.error);
    toast.ok(`Added ${length} min on ${key} — writing the description…`);
    onSaved(res.data.id);
  }

  // Reads as a sentence: "From 10:00 for 30 min, ends 10:30, on ABC-123 — fixed login bug  [Add]".
  // The field labels stay for screen readers; the sentence words carry them visually.
  return (
    <form
      className="note-form"
      aria-labelledby="note-form-title"
      onSubmit={submit}
      // Escape only closes an untouched form; typed text is discarded with Cancel, never by accident.
      onKeyDown={(e) => e.key === "Escape" && !ticket.trim() && !note.trim() && onCancel()}
      noValidate
    >
      <h2 id="note-form-title" className="sr-only">
        New block
      </h2>
      <div className="note-line">
        <span className="note-word" aria-hidden="true">From</span>
        <label className="note-chip note-chip-time">
          <span className="sr-only">Start</span>
          {/* Text, not type="time": the native picker follows the browser locale (10:00 PM); worklog is 24-hour. */}
          <input
            type="text"
            inputMode="numeric"
            placeholder="HH:MM"
            maxLength={5}
            autoComplete="off"
            data-field="Start"
            {...mark("Start")}
            value={start}
            disabled={busy}
            onChange={edit("Start", setStart)}
            onBlur={() => setStart((v) => tidyClock(v))}
          />
        </label>
        <span className="note-word" aria-hidden="true">for</span>
        <label className="note-chip note-chip-min">
          <span className="sr-only">Minutes</span>
          <input
            type="number"
            inputMode="numeric"
            data-field="Minutes"
            {...mark("Minutes")}
            min={1}
            max={NOTE_MAX_MINUTES}
            value={minutes}
            disabled={busy}
            onChange={edit("Minutes", setMinutes)}
          />
        </label>
        <span className="note-word" aria-hidden="true">min</span>
        {end && <span className={`note-form-end${end.warn ? " warn" : ""}`}>{end.text}</span>}
        <span className="note-word" aria-hidden="true">on</span>
        <label className="note-chip note-chip-ticket">
          <span className="sr-only">Ticket</span>
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
            onChange={edit("Ticket", setTicket)}
          />
        </label>
        <datalist id={LIST_ID}>
          {tickets.map((t) => (
            <option key={t.key} value={t.key}>
              {t.summary ?? ""}
            </option>
          ))}
        </datalist>
      </div>
      <div className="note-line">
        <label className="note-chip note-chip-note">
          <span className="sr-only">Note</span>
          <input
            data-field="Note"
            {...mark("Note")}
            value={note}
            placeholder="What did you do? A rough note is fine — the AI writes the Tempo text."
            disabled={busy}
            onChange={edit("Note", setNote)}
          />
        </label>
        <button type="submit" className="action-btn primary note-add" disabled={busy}>
          {busy ? "Saving…" : "Add"}
          {!busy && <CornerDownLeft aria-hidden="true" />}
        </button>
        <button type="button" className="note-cancel" aria-label="Cancel" title="Cancel (discards the note)" disabled={busy} onClick={onCancel}>
          <X aria-hidden="true" />
        </button>
      </div>
      {error && (
        <p role="alert" id={ERROR_ID} className="note-form-error">
          <CircleAlert aria-hidden="true" />
          {error}
        </p>
      )}
    </form>
  );
}
