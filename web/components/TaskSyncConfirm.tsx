"use client";

import { UploadCloud } from "lucide-react";
import { useEffect, useRef } from "react";

import { formatDuration } from "@/lib/format";
import type { TicketDay } from "@/lib/types";

/** Why a "Changed since sync" day differs from Tempo; the arrow part needs the last pulled Tempo hours. */
export function changedNote(day: TicketDay): string {
  const head = "Edited after it was sent to Tempo";
  if (day.in_tempo_seconds == null) return head;
  return `Edited after it was sent · In Tempo: ${formatDuration(day.in_tempo_seconds)} → now ${formatDuration(day.line_seconds)}`;
}

export function SyncConfirm(p: {
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
  const hours = formatDuration(day.line_seconds);
  // Esc is scoped to this confirm (never document-wide) and keeps the dialog open.
  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "Escape" || p.sending) return;
    e.preventDefault();
    e.stopPropagation();
    p.onCancel();
  }
  return (
    <div className="task-day-confirm" role="group" aria-label={`Confirm sending ${p.label} to Tempo`} onKeyDown={onKeyDown}>
      <p className="task-day-confirm-q">
        <UploadCloud size={14} aria-hidden="true" />
        <span>
          {!p.changed ? (
            <>Send <strong>{hours}</strong>{day.line_text && " and the text below"} to Tempo for <span className="task-nowrap">{p.label}?</span></>
          ) : day.in_tempo_seconds != null ? (
            <>Update Tempo: <strong>{formatDuration(day.in_tempo_seconds)} → {hours}</strong>?</>
          ) : (
            <>Update <strong>{hours}</strong> in Tempo for <span className="task-nowrap">{p.label}?</span></>
          )}
        </span>
      </p>
      <span className="task-day-confirm-act">
        <button ref={send} type="button" className="task-btn-primary" disabled={p.sending} onClick={p.onSend}>
          {p.sending ? (p.changed ? "Updating…" : "Sending…") : p.changed ? "Update" : "Send"}
        </button>
        <button type="button" className="task-btn-secondary" disabled={p.sending} onClick={p.onCancel}>
          Cancel
        </button>
      </span>
    </div>
  );
}
