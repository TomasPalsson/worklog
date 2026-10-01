"use client";

import { useEffect, useId, useRef, useState, useTransition } from "react";
import { AlertCircle } from "lucide-react";
import type { saveTempoLineHours } from "@/app/actions-tempo-lines";
import { formatBilledHours } from "@/lib/format";
import { HALF_HOUR_SECONDS, type TempoLine, type TempoLineKey } from "@/lib/tempo_line_contract";
import { toast } from "@/lib/toast";

const DAY_SECONDS = 24 * 3600;

/** `""` → `null` (back to tracked); "1,5", "1.5" and "1.5h" all read 1.5 h; anything else is NaN. */
function parseHours(draft: string): number | null {
  const trimmed = draft.trim();
  if (trimmed === "") return null;
  const match = /^(\d+(?:[.,]\d+)?)\s*h?$/i.exec(trimmed);
  return match ? Math.round(Number(match[1].replace(",", ".")) * 3600) : NaN;
}

function isValidOverride(seconds: number | null): boolean {
  return (
    seconds === null ||
    (seconds > 0 && seconds <= DAY_SECONDS && seconds % HALF_HOUR_SECONDS === 0)
  );
}

/** "1.3h isn't a half-hour step — try 1.5", naming the nearest valid value. */
function invalidMessage(draft: string, seconds: number): string {
  const typed = draft.trim().replace(",", ".").replace(/\s*h$/i, "");
  if (!Number.isFinite(seconds) || seconds <= 0) return `Enter hours like 1.5 — "${typed}" isn't one`;
  if (seconds > DAY_SECONDS) return `${typed}h is more than a day — try 8`;
  const nearest = Math.max(HALF_HOUR_SECONDS, Math.round(seconds / HALF_HOUR_SECONDS) * HALF_HOUR_SECONDS);
  return `${typed}h isn't a half-hour step — try ${nearest / 3600}`;
}

/**
 * The billed hours of a Tempo line, edited where they are shown — same
 * click-to-edit move as a block card's duration. Enter saves, Esc cancels,
 * ↑/↓ step by half an hour, an empty box goes back to the tracked hours.
 * Leaving the box saves a valid value and drops an invalid one.
 */
export function LineHours({
  label,
  line,
  lineKey,
  saveHours,
}: {
  label: string;
  line: TempoLine;
  lineKey: TempoLineKey;
  saveHours: typeof saveTempoLineHours;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  // Disabling the box mid-save blurs it; this keeps that blur from saving twice.
  const saving = useRef(false);
  // Keyboard exits (Enter, Esc) put focus back on the figure; a click away doesn't.
  const refocus = useRef(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const noteId = useId();
  const billed = formatBilledHours(line.effective_seconds);
  const tracked = formatBilledHours(line.union_seconds);
  const overridden = line.hours_override_seconds !== null;

  useEffect(() => {
    if (!editing && refocus.current) {
      refocus.current = false;
      buttonRef.current?.focus();
    }
  }, [editing]);

  // A rejected save re-enables the box; put focus back so Enter and Esc work.
  useEffect(() => {
    if (editing && error && !pending) inputRef.current?.focus();
  }, [editing, error, pending]);

  const open = () => {
    setDraft(String(line.effective_seconds / 3600));
    setError(null);
    setEditing(true);
  };
  const close = (byKeyboard: boolean) => {
    refocus.current = byKeyboard;
    setEditing(false);
    setError(null);
  };

  const commit = (byKeyboard: boolean) => {
    if (saving.current) return;
    const seconds = parseHours(draft);
    if (seconds !== null && !isValidOverride(seconds)) {
      const message = invalidMessage(draft, seconds);
      if (byKeyboard) {
        setError(message);
      } else {
        close(false);
        toast.error(`Not saved — ${message}`);
      }
      return;
    }
    const unchanged =
      seconds === line.hours_override_seconds ||
      (!overridden && seconds === line.effective_seconds);
    if (unchanged) {
      close(byKeyboard);
      return;
    }
    saving.current = true;
    startTransition(async () => {
      const r = await saveHours(lineKey, seconds);
      saving.current = false;
      if (!r.ok) {
        setError(r.error);
        return;
      }
      close(byKeyboard);
      toast.ok(seconds === null ? `Hours back to ${tracked} tracked` : "Hours saved");
    });
  };

  // From an off-step value the first press lands on the next boundary that way.
  const step = (delta: 1 | -1) => {
    const seconds = parseHours(draft);
    const base = seconds !== null && Number.isFinite(seconds) ? seconds : line.effective_seconds;
    const steps = base / HALF_HOUR_SECONDS;
    const target = Number.isInteger(steps) ? steps + delta : delta > 0 ? Math.ceil(steps) : Math.floor(steps);
    const next = Math.min(DAY_SECONDS, Math.max(HALF_HOUR_SECONDS, target * HALF_HOUR_SECONDS));
    setDraft(String(next / 3600));
    setError(null);
  };

  // Inside <summary>: a click or Enter/Space here must not fold the group.
  const keepOpen = (e: React.SyntheticEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };

  if (editing) {
    return (
      <span className="line-hours" onClick={(e) => e.stopPropagation()}>
        <span className="line-hours-field" aria-busy={pending || undefined}>
          <input
            ref={inputRef}
            className="line-hours-input"
            type="text"
            inputMode="decimal"
            autoFocus
            aria-label={`Hours for ${label}`}
            aria-invalid={error ? true : undefined}
            aria-describedby={noteId}
            value={draft}
            disabled={pending}
            onFocus={(e) => e.currentTarget.select()}
            onChange={(e) => {
              setDraft(e.target.value);
              setError(null);
            }}
            onBlur={() => commit(false)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                keepOpen(e);
                commit(true);
              } else if (e.key === "Escape") {
                keepOpen(e);
                close(true);
              } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                keepOpen(e);
                step(e.key === "ArrowUp" ? 1 : -1);
              } else if (e.key === " ") {
                e.stopPropagation();
              }
            }}
          />
          <span className="line-hours-unit">h</span>
        </span>
        {error ? (
          <span id={noteId} role="alert" className="line-hours-error">
            <AlertCircle aria-hidden="true" />
            {error}
          </span>
        ) : (
          <span id={noteId} className="line-hours-note">
            {pending ? "Saving…" : `↵ save · esc cancel · ↑↓ ½h · empty = ${tracked}`}
          </span>
        )}
      </span>
    );
  }

  return (
    <span className="line-hours">
      <button
        ref={buttonRef}
        type="button"
        className="line-hours-btn"
        aria-label={`${billed} billed — Change billed hours for ${label}`}
        title="Change the hours sent to Tempo"
        onClick={(e) => {
          keepOpen(e);
          open();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            keepOpen(e);
            open();
          }
        }}
      >
        <span className="line-hours-value">{billed}</span>
        <span className="line-hours-unit">billed</span>
      </button>
      {overridden && <span className="line-hours-note">hours changed · {tracked} tracked</span>}
    </span>
  );
}
