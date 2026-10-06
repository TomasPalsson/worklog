"use client";

// Today's page: auto-sent Tempo lines the Owner hasn't confirmed yet. Actions
// are injectable because @/app/actions is mock.module'd process-wide in tests.

import { useEffect, useRef, useState, useTransition } from "react";
import Link from "next/link";
import { Loader2 } from "lucide-react";
import * as actions from "@/app/actions";
import * as lineActions from "@/app/actions-tempo-lines";
import { shortMonthDay, shortWeekday } from "@/lib/format";
import type { ReviewLine } from "@/lib/verdict_contract";

type Result = { ok: true; data?: unknown } | { ok: false; error: string };

interface Actions {
  confirm?: (day: string, jiraIssue?: string) => Promise<Result>;
  sync?: (day: string, dryRun: boolean, jiraIssue?: string) => Promise<Result>;
  saveHours?: (key: { day: string; jira_issue: string }, seconds: number | null) => Promise<Result>;
  saveText?: (key: { day: string; jira_issue: string }, text: string) => Promise<Result>;
}

type Wired = Required<Actions>;

const hoursOf = (seconds: number) => Number((seconds / 3600).toFixed(2));
// The daemon answers 200 even when Tempo rejects or skips the line.
function syncFailure(r: Result): Result {
  if (!r.ok) return r;
  const d = r.data as { synced?: number; errors?: string[]; results?: { reason: string | null }[] } | undefined;
  if (!d) return r;
  const reasons = [...(d.errors ?? []), ...(d.results ?? []).map((x) => x.reason).filter(Boolean)] as string[];
  if (reasons.length === 0 && d.synced !== 0) return r;
  return { ok: false, error: reasons[0] ?? "Tempo did not accept the line" };
}

const same = (a: ReviewLine, b: ReviewLine) => a.day === b.day && a.jira_issue === b.jira_issue;

function dayLabel(day: string) {
  const [month, date] = shortMonthDay(day).split(" ");
  return `${shortWeekday(day)} ${date} ${month}`;
}

function EditRow({
  line,
  act,
  onDone,
  onCancel,
}: {
  line: ReviewLine;
  act: Wired;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [hours, setHours] = useState(String(hoursOf(line.seconds)));
  const [text, setText] = useState(line.text);
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();
  const hoursRef = useRef<HTMLInputElement>(null);
  useEffect(() => hoursRef.current?.focus(), []);
  const key = { day: line.day, jira_issue: line.jira_issue };
  const hoursChanged = Number(hours) !== hoursOf(line.seconds);
  const textChanged = text !== line.text;
  const valid = Number(hours) >= 0.25;

  const save = () => {
    setError(null);
    start(async () => {
      const steps: (() => Promise<Result>)[] = [];
      if (hoursChanged) steps.push(() => act.saveHours(key, Math.round(Number(hours) * 3600)));
      if (textChanged) steps.push(() => act.saveText(key, text));
      steps.push(async () => syncFailure(await act.sync(line.day, false, line.jira_issue)));
      steps.push(() => act.confirm(line.day, line.jira_issue));
      for (const step of steps) {
        const r = await step();
        if (!r.ok) return setError(r.error);
      }
      onDone();
    });
  };

  const id = `review-edit-${line.day}-${line.jira_issue}`;
  return (
    <li className="review-sec-line review-sec-edit">
      <span className="review-sec-ticket">{line.jira_issue}</span>
      <div className="review-sec-fields">
        <label htmlFor={`${id}-h`}>Hours</label>
        <input
          id={`${id}-h`}
          ref={hoursRef}
          aria-invalid={!valid}
          type="number"
          step="0.25"
          min="0.25"
          value={hours}
          onChange={(e) => setHours(e.target.value)}
        />
        <label htmlFor={`${id}-t`}>Text</label>
        <textarea id={`${id}-t`} rows={3} value={text} onChange={(e) => setText(e.target.value)} />
        {!valid && <p className="review-sec-error">Hours must be at least 0.25.</p>}
      </div>
      <div className="review-sec-actions">
        <button
          type="button"
          className="action-btn primary"
          disabled={pending || !valid || !(hoursChanged || textChanged)}
          onClick={save}
        >
          {pending && <Loader2 className="spin" size={13} />}
          {pending ? "Saving…" : "Save to Tempo"}
        </button>
        <button type="button" className="action-btn" disabled={pending} onClick={onCancel}>
          Cancel
        </button>
        {error && <p className="review-sec-error">{error}</p>}
      </div>
    </li>
  );
}

function Row({
  line,
  act,
  onGone,
  onSent,
}: {
  line: ReviewLine;
  act: Wired;
  onGone: (announcement: string) => void;
  onSent: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const editRef = useRef<HTMLButtonElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (!editing && refocus.current) editRef.current?.focus();
    refocus.current = false;
  }, [editing]);
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();

  if (editing) {
    return (
      <EditRow
        line={line}
        act={act}
        onDone={() => onGone(`Saved ${line.jira_issue} to Tempo`)}
        onCancel={() => {
          refocus.current = true;
          setEditing(false);
        }}
      />
    );
  }

  const run = (call: () => Promise<Result>, done: () => void) => {
    setError(null);
    start(async () => {
      const r = await call();
      if (!r.ok) return setError(r.error);
      done();
    });
  };

  const sent = line.status === "sent";
  return (
    <li className="review-sec-line" data-status={line.status}>
      <span className="review-sec-ticket">{line.jira_issue}</span>
      <span className="review-sec-hours">{hoursOf(line.seconds)} h</span>
      <div className="review-sec-text">
        {line.text}
        {line.status === "not_sent" && (
          <p className="review-sec-error">Not sent: {line.error}</p>
        )}
        {error && <p className="review-sec-error">{error}</p>}
      </div>
      <div className="review-sec-actions">
        {sent ? (
          <>
            <button
              type="button"
              className="review-toggle"
              disabled={pending}
              onClick={() => run(() => act.confirm(line.day, line.jira_issue), () => onGone(`Confirmed ${line.jira_issue}`))}
            >
              {pending && <Loader2 className="spin" size={13} />}
              {pending ? "Confirming…" : "Looks right"}
            </button>
            <button ref={editRef} type="button" className="review-toggle" disabled={pending} onClick={() => setEditing(true)}>
              Edit
            </button>
          </>
        ) : (
          <button
            type="button"
            className="action-btn"
            disabled={pending}
            onClick={() => run(async () => syncFailure(await act.sync(line.day, false, line.jira_issue)), onSent)}
          >
            {pending && <Loader2 className="spin" size={13} />}
            {pending ? "Sending…" : "Send again"}
          </button>
        )}
      </div>
    </li>
  );
}

function DayGroup({
  day,
  lines,
  act,
  onGone,
  onSent,
  onAllGone,
}: {
  day: string;
  lines: ReviewLine[];
  act: Wired;
  onGone: (l: ReviewLine, announcement: string) => void;
  onSent: (l: ReviewLine) => void;
  onAllGone: (day: string) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();
  const sent = lines.filter((l) => l.status === "sent").length;

  const confirmAll = () => {
    setError(null);
    start(async () => {
      const r = await act.confirm(day);
      if (r.ok) onAllGone(day);
      else setError(r.error);
    });
  };

  return (
    <div className="review-sec-group">
      <div className="review-sec-day">
        <h3>{dayLabel(day)}</h3>
        <Link href={`/${day}`} className="review-toggle">
          Open day
        </Link>
        {sent > 0 && (
          <button type="button" className="review-toggle" disabled={pending} onClick={confirmAll}>
            {pending ? "Confirming…" : `Confirm all ${sent}`}
          </button>
        )}
        {error && <span className="review-sec-error">{error}</span>}
      </div>
      <ul role="list">
        {lines.map((l) => (
          <Row key={l.jira_issue} line={l} act={act} onGone={(msg) => onGone(l, msg)} onSent={() => onSent(l)} />
        ))}
      </ul>
    </div>
  );
}

export function ReviewSection({
  lines: initial,
  confirm = actions.confirmReview,
  sync = actions.runSync,
  saveHours = lineActions.saveTempoLineHours,
  saveText = lineActions.saveTempoLineText,
}: { lines: ReviewLine[] } & Actions) {
  const [lines, setLines] = useState(initial);
  const [announcement, setAnnouncement] = useState("");
  if (lines.length === 0) return null;

  const act = { confirm, sync, saveHours, saveText };
  const days = [...new Set(lines.map((l) => l.day))].sort().reverse();
  return (
    <section className="review-sec" aria-labelledby="review-sec-title">
      <header className="review-sec-head">
        <h2 id="review-sec-title">Sent to Tempo — check these</h2>
        <span className="review-sec-count">{lines.length} lines</span>
        <p className="review-sec-hint">
          Confirm each line, or fix it; a fix updates the same Tempo worklog.
        </p>
      </header>
      {days.map((day) => (
        <DayGroup
          key={day}
          day={day}
          lines={lines.filter((l) => l.day === day)}
          act={act}
          onGone={(l, msg) => {
            setAnnouncement(msg);
            setLines((cur) => cur.filter((x) => !same(x, l)));
          }}
          onSent={(l) =>
            setLines((cur) =>
              cur.map((x) => {
                if (!same(x, l)) return x;
                const { day, jira_issue, seconds, text } = x;
                return { day, jira_issue, seconds, text, status: "sent" };
              }),
            )
          }
          onAllGone={(d) => setLines((cur) => cur.filter((x) => x.day !== d || x.status !== "sent"))}
        />
      ))}
      <div className="review-sec-live" aria-live="polite">
        {announcement}
      </div>
    </section>
  );
}
