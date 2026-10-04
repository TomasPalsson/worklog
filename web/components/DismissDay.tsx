"use client";

import { useEffect, useId, useRef, useState, type MutableRefObject } from "react";
import { useRouter } from "next/navigation";
import { dismissLoggedDay, undismissLoggedDay } from "@/app/actions-logged";
import { DISMISS_REASON_MAX_CHARS } from "@/lib/logged_contract";
import { hours } from "./LoggedEntries";

type Props = {
  day: string;
  state: "under" | "dismissed";
  reason: string | null;
  loggedSeconds: number;
  requiredSeconds: number | null;
  /** Week cards: just the trigger, the sentence is already in the head. */
  compact?: boolean;
};

/** Which control should take focus the next time it mounts. */
type Want = MutableRefObject<"open" | "undo" | null>;

const length = (s: string) => [...s.trim()].length;

function validate(value: string): string | null {
  const n = length(value);
  if (n === 0) return "Write a short reason, like “dentist”.";
  if (n > DISMISS_REASON_MAX_CHARS) return `Keep it to ${DISMISS_REASON_MAX_CHARS} characters (now ${n}).`;
  return null;
}

function Dismissed({ day, reason, want }: { day: string; reason: string | null; want: Want }) {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const undoRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (want.current === "undo") {
      undoRef.current?.focus();
      want.current = null;
    }
  }, [want]);

  const undo = async () => {
    const res = await undismissLoggedDay(day);
    if (!res.ok) return setError(res.error);
    want.current = "open";
    router.refresh();
  };

  return (
    <div className="dismiss-day dismissed">
      <span>
        Marked fine: <strong>{reason}</strong>
      </span>
      <button type="button" ref={undoRef} className="link-btn" onClick={() => void undo()}>
        Undo
      </button>
      {error && (
        <span role="alert" className="dismiss-hint">
          {error}
        </span>
      )}
    </div>
  );
}

function FormTail({ n, saving, onClose }: { n: number; saving: boolean; onClose: () => void }) {
  return (
    <>
      <span className={n > DISMISS_REASON_MAX_CHARS ? "dismiss-count over" : "dismiss-count"}>
        {n}/{DISMISS_REASON_MAX_CHARS}
      </span>
      <button type="submit" className="action-btn" disabled={saving}>
        {saving ? "Saving…" : "Save reason"}
      </button>
      <button type="button" className="link-btn" onClick={onClose} disabled={saving}>
        Cancel
      </button>
    </>
  );
}

function DismissForm({ day, want, onClose }: { day: string; want: Want; onClose: () => void }) {
  const router = useRouter();
  const [value, setValue] = useState("");
  const [touched, setTouched] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const id = useId();

  useEffect(() => inputRef.current?.focus(), []);

  const hint = touched ? validate(value) : null;
  const message = error ?? hint;
  const n = length(value);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (validate(value)) return;
    setSaving(true);
    setError(null);
    const res = await dismissLoggedDay(day, value.trim());
    if (!res.ok) {
      setSaving(false);
      return setError(res.error);
    }
    // Stays "Saving…" until the refreshed props swap this form for the dismissed line.
    want.current = "undo";
    router.refresh();
  };

  return (
    <form
      className="dismiss-day under open"
      onSubmit={(e) => void submit(e)}
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <label htmlFor={`${id}-reason`}>Why is this day short?</label>
      <input
        id={`${id}-reason`}
        ref={inputRef}
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          setError(null);
        }}
        disabled={saving}
        onBlur={() => setTouched(true)}
        aria-invalid={hint ? "true" : undefined}
        aria-describedby={message ? `${id}-hint` : undefined}
      />
      <FormTail n={n} saving={saving} onClose={onClose} />
      {message && (
        <p id={`${id}-hint`} className="dismiss-hint" role={error ? "alert" : undefined}>
          {message}
        </p>
      )}
    </form>
  );
}

function Short({ day, loggedSeconds, requiredSeconds, compact, want }: Omit<Props, "state" | "reason"> & { want: Want }) {
  const [open, setOpen] = useState(false);
  const openRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open && want.current === "open") {
      openRef.current?.focus();
      want.current = null;
    }
  }, [open, want]);

  if (open) {
    return (
      <DismissForm
        day={day}
        want={want}
        onClose={() => {
          want.current = "open";
          setOpen(false);
        }}
      />
    );
  }
  return (
    <div className={compact ? "dismiss-day under compact" : "dismiss-day under"}>
      {!compact && (
        <span>
          <strong>Is this day filled out?</strong> {hours(loggedSeconds)} of {hours(requiredSeconds ?? 0)} logged.
        </span>
      )}
      <button type="button" ref={openRef} className="action-btn" onClick={() => setOpen(true)}>
        Mark as fine…
      </button>
    </div>
  );
}

export function DismissDay({ state, reason, ...rest }: Props) {
  const want: Want = useRef(null);
  return state === "dismissed" ? (
    <Dismissed day={rest.day} reason={reason} want={want} />
  ) : (
    <Short {...rest} want={want} />
  );
}
