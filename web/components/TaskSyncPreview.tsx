"use client";

import { useEffect, useRef } from "react";

import { formatDuration } from "@/lib/format";
import type { TicketDay } from "@/lib/types";

/** Why a "Changed since sync" day differs from Tempo; the arrow part needs the last pulled Tempo hours. */
export function changedNote(day: TicketDay): string {
  const head = "Edited after it was sent to Tempo";
  if (day.in_tempo_seconds == null) return head;
  return `Edited after it was sent · In Tempo: ${formatDuration(day.in_tempo_seconds)} → now ${formatDuration(day.line_seconds)}`;
}

export function SyncPreview(p: {
  label: string;
  day: TicketDay;
  changed: boolean;
  sending: boolean;
  onSend: () => void;
  onCancel: () => void;
}) {
  const send = useRef<HTMLButtonElement>(null);
  useEffect(() => send.current?.focus(), []);
  const { day } = p;
  const known = p.changed && day.in_tempo_seconds != null;
  // Esc is scoped to this preview (never document-wide) and keeps the panel open.
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "Escape" || p.sending) return;
    e.preventDefault();
    e.stopPropagation();
    p.onCancel();
  }
  return (
    <div className="task-day-preview" onKeyDown={onKeyDown}>
      <h5>{p.changed ? "Preview — Tempo will be updated" : "Preview — nothing sent yet"}</h5>
      <dl>
        {known ? (
          <>
            <dt>In Tempo</dt>
            <dd>{formatDuration(day.in_tempo_seconds ?? 0)}</dd>
            <dt>Will be</dt>
            <dd>{formatDuration(day.line_seconds)}</dd>
          </>
        ) : (
          <>
            <dt>Hours</dt>
            <dd>{formatDuration(day.line_seconds)}</dd>
          </>
        )}
        <dt>Day</dt>
        <dd>{p.label}</dd>
        {day.line_text && (
          <>
            <dt>Text</dt>
            <dd>as shown above</dd>
          </>
        )}
      </dl>
      <p>Sends only this ticket&apos;s line for this day to Tempo.</p>
      <span className="task-day-edit">
        <button ref={send} type="button" className="task-btn-primary" disabled={p.sending} onClick={p.onSend}>
          {p.sending ? "Sending…" : p.changed ? "Update Tempo" : "Send to Tempo"}
        </button>
        <button type="button" className="task-btn-secondary" disabled={p.sending} onClick={p.onCancel}>
          Cancel
        </button>
      </span>
    </div>
  );
}
