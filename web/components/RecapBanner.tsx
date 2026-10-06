"use client";

// Today's page: the recap after the 17:00 run. `resolve` is injected by the
// page (a server action) so tests need no module mocks.

import { useState, useTransition } from "react";
import { Loader2 } from "lucide-react";
import { formatBilledHours, formatRange } from "@/lib/format";
import { RECAP_TOP_GAPS, type GapAction, type Recap, type RecapGap } from "@/lib/daily_helpers_contract";

type Result = { ok: true; data?: Recap | null } | { ok: false; error: string };

export type ResolveGap = (day: string, startedAt: string, action: GapAction) => Promise<Result>;

function GapRow({ gap, onAct }: { gap: RecapGap; onAct: (action: GapAction) => Promise<string | null> }) {
  const [picking, setPicking] = useState(false);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();
  const run = (action: GapAction) =>
    start(async () => {
      setError(await onAct(action));
    });
  const id = `recap-key-${gap.started_at}`;

  return (
    <li className="review-sec-line recap-gap">
      <span className="review-sec-ticket">{formatRange(gap.started_at, gap.ended_at)}</span>
      <span className="review-sec-hours">{gap.minutes} min</span>
      <div className="review-sec-text">
        {picking && (
          <div className="review-sec-fields">
            <label htmlFor={id}>Ticket key</label>
            <input id={id} value={key} placeholder="AB-123" onChange={(e) => setKey(e.target.value)} />
          </div>
        )}
        {error && <p className="review-sec-error">{error}</p>}
      </div>
      <div className="review-sec-actions">
        {picking ? (
          <>
            <button
              type="button"
              className="action-btn primary"
              disabled={pending || key.trim() === ""}
              onClick={() => run({ action: "pick_ticket", jira_issue: key.trim().toUpperCase() })}
            >
              {pending && <Loader2 className="spin" size={13} />}
              Add time
            </button>
            <button type="button" className="review-toggle" disabled={pending} onClick={() => setPicking(false)}>
              Cancel
            </button>
          </>
        ) : (
          <>
            <button type="button" className="review-toggle" disabled={pending} onClick={() => run({ action: "personal" })}>
              Personal
            </button>
            <button type="button" className="review-toggle" disabled={pending} onClick={() => run({ action: "break" })}>
              Break
            </button>
            <button type="button" className="review-toggle" disabled={pending} onClick={() => setPicking(true)}>
              Pick a ticket
            </button>
          </>
        )}
      </div>
    </li>
  );
}

export function RecapBanner({ recap: initial, resolve }: { recap: Recap | null; resolve?: ResolveGap }) {
  const [recap, setRecap] = useState(initial);
  if (!recap) return null;

  const act = (gap: RecapGap) => async (action: GapAction) => {
    const r = await resolve!(recap.day, gap.started_at, action);
    if (!r.ok) return r.error;
    if (r.data) setRecap(r.data);
    return null;
  };
  const sentSeconds = recap.sent.reduce((n, l) => n + l.seconds, 0);
  const gaps = recap.gaps.slice(0, RECAP_TOP_GAPS);

  return (
    <section className="review-sec" aria-labelledby="recap-title">
      <header className="review-sec-head">
        <h2 id="recap-title">17:00 recap</h2>
        <span className="review-sec-count">{`${recap.sent.length} sent · ${formatBilledHours(sentSeconds)}`}</span>
        <span className="review-sec-count">{`${recap.coverage_percent}% covered`}</span>
      </header>
      {recap.held_back.length > 0 && (
        <div className="review-sec-group">
          <h3>Held back</h3>
          <ul role="list">
            {recap.held_back.map((l) => (
              <li key={l.jira_issue} className="review-sec-line" data-status="not_sent">
                <span className="review-sec-ticket">{l.jira_issue}</span>
                <span />
                <span className="review-sec-text">{l.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
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
