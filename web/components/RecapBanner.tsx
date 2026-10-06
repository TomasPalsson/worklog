"use client";

// Today's page: the recap after the 17:00 run. `resolve` is injected by the
// page (a server action) so tests need no module mocks.

import { useRef, useState, useTransition } from "react";
import { Loader2 } from "lucide-react";
import { formatBilledHours, formatRange } from "@/lib/format";
import { RECAP_TOP_GAPS, type GapAction, type Recap, type RecapGap } from "@/lib/daily_helpers_contract";

type Result = { ok: true; data?: Recap | null } | { ok: false; error: string };

export type ResolveGap = (day: string, startedAt: string, action: GapAction) => Promise<Result>;

function TicketField(p: { id: string; value: string; pending: boolean; onChange: (v: string) => void; onPick: () => void; onCancel: () => void }) {
  return (
    <div className="review-sec-fields">
      <label htmlFor={p.id}>Ticket key</label>
      <input
        id={p.id}
        value={p.value}
        placeholder="AB-123"
        autoFocus
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => p.onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && p.value.trim() !== "" && !p.pending) p.onPick();
          if (e.key === "Escape") p.onCancel();
        }}
      />
    </div>
  );
}

function GapActions(p: {
  rangeId: string;
  picking: boolean;
  pending: boolean;
  canAdd: boolean;
  run: (action: GapAction) => void;
  onPick: () => void;
  setPicking: (on: boolean) => void;
}) {
  const toggle = (label: string, onClick: () => void) => (
    <button type="button" className="review-toggle" disabled={p.pending} aria-describedby={p.rangeId} onClick={onClick}>
      {label}
    </button>
  );
  if (p.picking) {
    return (
      <div className="review-sec-actions">
        <button
          type="button"
          className="task-btn-primary"
          disabled={p.pending || !p.canAdd}
          aria-describedby={p.rangeId}
          onClick={p.onPick}
        >
          {p.pending && <Loader2 className="spin" size={13} />}
          Add time
        </button>
        {toggle("Cancel", () => p.setPicking(false))}
      </div>
    );
  }
  return (
    <div className="review-sec-actions">
      {toggle("Personal", () => p.run({ action: "personal" }))}
      {toggle("Break", () => p.run({ action: "break" }))}
      {toggle("Pick a ticket", () => p.setPicking(true))}
    </div>
  );
}

function GapRow({ gap, onAct }: { gap: RecapGap; onAct: (action: GapAction) => Promise<string | null> }) {
  const [picking, setPicking] = useState(false);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();
  const run = (action: GapAction) =>
    start(async () => {
      setError(await onAct(action));
    });
  const rangeId = `recap-range-${gap.started_at}`;
  const pick = () => run({ action: "pick_ticket", jira_issue: key.trim().toUpperCase() });

  return (
    <li className="review-sec-line recap-gap">
      <span id={rangeId} className="review-sec-ticket">
        {formatRange(gap.started_at, gap.ended_at)}
      </span>
      <span className="review-sec-hours">{gap.minutes} min</span>
      <div className="review-sec-text">
        {picking && (
          <TicketField
            id={`recap-key-${gap.started_at}`}
            value={key}
            pending={pending}
            onChange={setKey}
            onPick={pick}
            onCancel={() => setPicking(false)}
          />
        )}
        {error && (
          <p role="alert" className="review-sec-error">
            {error}
          </p>
        )}
      </div>
      <GapActions
        rangeId={rangeId}
        picking={picking}
        pending={pending}
        canAdd={key.trim() !== ""}
        run={run}
        onPick={pick}
        setPicking={setPicking}
      />
    </li>
  );
}

function HeldBack({ lines }: { lines: Recap["held_back"] }) {
  if (lines.length === 0) return null;
  return (
    <div className="review-sec-group">
      <h3>Held back</h3>
      <ul role="list">
        {lines.map((l) => (
          <li key={l.jira_issue} className="review-sec-line" data-status="not_sent">
            <span className="review-sec-ticket">{l.jira_issue}</span>
            <span aria-hidden="true" />
            <span className="review-sec-text">{l.reason}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function doneText(action: GapAction): string {
  if (action.action === "personal") return "marked Personal";
  if (action.action === "break") return "marked as a break";
  return `added to ${action.jira_issue}`;
}

export function RecapBanner({ recap: initial, resolve }: { recap: Recap | null; resolve?: ResolveGap }) {
  const [recap, setRecap] = useState(initial);
  const [announce, setAnnounce] = useState("");
  const heading = useRef<HTMLHeadingElement>(null);
  if (!recap) return null;

  const act = (gap: RecapGap) => async (action: GapAction) => {
    const r = await resolve!(recap.day, gap.started_at, action);
    if (!r.ok) return r.error;
    if (r.data) setRecap(r.data);
    // The resolved row unmounts with its focused button: say what happened and keep focus in the recap.
    setAnnounce(`${formatRange(gap.started_at, gap.ended_at)} ${doneText(action)}`);
    heading.current?.focus();
    return null;
  };
  const sentSeconds = recap.sent.reduce((n, l) => n + l.seconds, 0);
  const gaps = recap.gaps.slice(0, RECAP_TOP_GAPS);

  return (
    <section className="review-sec" aria-labelledby="recap-title">
      <header className="review-sec-head">
        <h2 id="recap-title" ref={heading} tabIndex={-1}>
          17:00 recap
        </h2>
        <span className="review-sec-count">{`${recap.sent.length} sent · ${formatBilledHours(sentSeconds)}`}</span>
        <span className="review-sec-count">{`${recap.coverage_percent}% covered`}</span>
        {gaps.length > 0 && (
          <p className="review-sec-hint">
            Fill each gap. Personal and Break are never billed or sent; Pick a ticket adds work time.
          </p>
        )}
        {gaps.length === 0 && recap.held_back.length === 0 && (
          <p className="review-sec-hint">Nothing held back and no gap of 15 min or more. Today is covered.</p>
        )}
      </header>
      <p className="review-sec-live" role="status" aria-live="polite">
        {announce}
      </p>
      <HeldBack lines={recap.held_back} />
      {gaps.length > 0 && (
        <div className="review-sec-group">
          <h3>Unexplained gaps</h3>
          <ul role="list">
            {gaps.map((g) => (
              <GapRow key={g.started_at} gap={g} onAct={act(g)} />
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
