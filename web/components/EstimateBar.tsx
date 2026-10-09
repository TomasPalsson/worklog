import { formatClock, formatDuration } from "@/lib/format";
import { barModel, initials, type BarModel } from "@/lib/progress";
import type { TicketProgress } from "@/lib/types";
import { ProgressChart } from "./ProgressChart";
import { EstimateFlagIcon, PendingIcon, ToneIcon } from "./progressIcons";

interface Props {
  /** undefined while the numbers load. */
  ticket: TicketProgress | undefined;
  /** This block's not-yet-synced seconds. */
  pending: number;
  onRetry: () => void;
}

const pct = (seconds: number, scale: number) => `${((seconds / scale) * 100).toFixed(2)}%`;

export function EstimateBar({ ticket, pending, onRetry }: Props) {
  if (!ticket) {
    return (
      <div className="ep" aria-busy="true">
        <div className="ep-head">
          <span className="ep-used ep-muted">Loading hours from Jira…</span>
        </div>
        <div className="ep-bar ep-skel" />
      </div>
    );
  }
  if (ticket.error === "not_configured") {
    return <p className="ep-none">Connect Jira in Settings to see estimates.</p>;
  }
  if (ticket.error && !ticket.pulled_at) {
    return (
      <div className="ep" data-tone="warn">
        <p className="ep-err" role="alert">
          Couldn&apos;t load hours for {ticket.key} — Jira didn&apos;t answer. Your blocks are safe.{" "}
          <button type="button" className="ep-link" onClick={onRetry}>
            Try again
          </button>
        </p>
      </div>
    );
  }
  const m = barModel(ticket, pending);
  if (!m) {
    return (
      <p className="ep-none">
        No estimate on this ticket yet. Set “Original estimate” on {ticket.key} in Jira and the bar shows up here.
      </p>
    );
  }

  return <Ready ticket={ticket} m={m} pending={pending} onRetry={onRetry} />;
}

function Legend({ m, pending }: { m: BarModel; pending: number }) {
  return (
    <ul className="ep-legend">
      {m.segments.map((s, i) => (
        <li key={s.account_id} data-p={i}>
          <span className="ep-ini">{initials(s.name)}</span>
          {s.name} <b>{formatDuration(s.seconds)}</b>
        </li>
      ))}
      {pending > 0 && (
        <li>
          <PendingIcon />
          This block, not in Tempo yet <b>+{formatDuration(pending)}</b>
        </li>
      )}
    </ul>
  );
}

function Meter({ ticket, m, pending, verdict }: { ticket: TicketProgress; m: BarModel; pending: number; verdict: string }) {
  const scale = Math.max(m.estimate, m.used);
  const flagAt = pct(m.estimate, scale);
  return (
    <div
      className="ep-bar"
      role="meter"
      aria-label={`Hours on ${ticket.key} against its estimate`}
      aria-valuemin={0}
      aria-valuemax={scale}
      aria-valuenow={m.used}
      aria-valuetext={`${formatDuration(m.used)} of ${formatDuration(m.estimate)} estimate${pending > 0 ? " including this block" : ""}, ${verdict}`}
    >
      {m.over > 0 && <span className="ep-overzone" style={{ left: flagAt }} />}
      <span className="ep-fill">
        {m.segments.map((s, i) => (
          <i key={s.account_id} data-p={i} style={{ width: pct(s.seconds, scale) }} />
        ))}
        {pending > 0 && <i className="ep-pend" data-p={0} style={{ width: pct(pending, scale) }} />}
      </span>
      <span className="ep-flag" data-end={m.over > 0 ? undefined : ""} style={{ left: flagAt }}>
        <EstimateFlagIcon />
        <span className="ep-flag-l">{formatDuration(m.estimate)}</span>
      </span>
    </div>
  );
}

function Ready({ ticket, m, pending, onRetry }: Props & { ticket: TicketProgress; m: BarModel }) {
  const verdict = m.over > 0 ? `${formatDuration(m.over)} over` : `${formatDuration(m.left)} left`;
  return (
    <div className="ep" data-tone={m.tone}>
      <div className="ep-grid">
        <div className="ep-main">
          <div className="ep-head">
            <span className="ep-used">
              <b>{formatDuration(m.used)}</b> of {formatDuration(m.estimate)}
              {pending > 0 && <span className="ep-after"> once synced</span>}
            </span>
            <span className="ep-state">
              <ToneIcon tone={m.tone} />
              {verdict}
            </span>
          </div>
          <Meter ticket={ticket} m={m} pending={pending} verdict={verdict} />
          <Legend m={m} pending={pending} />
          {ticket.pulled_at && (
            <p className="ep-pulled">
              {`Jira numbers from ${formatClock(ticket.pulled_at)}`}
              {ticket.error && (
                <>
                  {" · couldn't refresh "}
                  <button type="button" className="ep-link" onClick={onRetry}>
                    Try again
                  </button>
                </>
              )}
            </p>
          )}
        </div>
        <ProgressChart ticket={ticket} pending={pending} />
      </div>
    </div>
  );
}
