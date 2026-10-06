"use client";

// Settings → Verdict: the on/off switch and its live state apply at once
// (they control a running process, not a stored preference); the auto-send
// box is a plain setting owned by the settings form and saved with Save.
// The actions are injectable (defaulting to the real ones) because
// @/app/actions is `mock.module`d process-wide by other test files.

import { useEffect, useState, useTransition } from "react";
import { Loader2 } from "lucide-react";
import * as actions from "@/app/actions";
import { toast } from "@/lib/toast";
import type { VerdictState, VerdictStatus } from "@/lib/verdict_contract";

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

interface Props {
  day: string;
  autoSend: boolean;
  onAutoSend: (on: boolean) => void;
  fetchStatus?: (day: string) => Promise<Result<VerdictStatus>>;
  setEnabled?: (on: boolean) => Promise<Result<VerdictState>>;
  retry?: () => Promise<Result<VerdictState>>;
}

function stateCopy(s: VerdictState) {
  switch (s.state) {
    case "off":
      return "Off";
    case "starting":
      return "Starting…";
    case "running":
      return "Running";
    case "not_answering":
      return "Not answering";
    case "needs_uv":
      return (
        <>
          Needs uv — install it with <code>brew install uv</code>, then press Retry
        </>
      );
    case "stopped":
      return `Stopped: ${s.error}`;
  }
}

export function VerdictControl({
  day,
  autoSend,
  onAutoSend,
  fetchStatus = actions.fetchVerdictStatus,
  setEnabled = actions.setVerdictEnabled,
  retry = actions.retryVerdict,
}: Props) {
  const [status, setStatus] = useState<VerdictStatus | null>(null);
  const [unknown, setUnknown] = useState(false);
  const [toggling, startToggle] = useTransition();
  const [retrying, startRetry] = useTransition();

  useEffect(() => {
    let live = true;
    (async () => {
      try {
        const r = await fetchStatus(day);
        if (!live) return;
        if (r.ok) setStatus(r.data);
        else setUnknown(true);
      } catch {
        if (live) setUnknown(true);
      }
    })();
    return () => {
      live = false;
    };
  }, [day, fetchStatus]);

  const apply = (s: VerdictState) => {
    setUnknown(false);
    setStatus((cur) => ({ unchecked: 0, scorecard: null, ...cur, ...s }));
  };

  const state = status?.state;
  const canRetry = state === "stopped" || state === "needs_uv";

  return (
    <section className="settings-section">
      <h3>Verdict</h3>
      <p className="settings-hint">
        Files Slack messages and browser tabs into projects, picks clear tickets and
        checks Tempo text before it is sent.
      </p>
      <label className="settings-field verdict-check">
        <input
          type="checkbox"
          aria-label="Run Verdict"
          checked={state !== undefined && state !== "off"}
          disabled={toggling || (status === null && !unknown)}
          onChange={(e) => {
            const on = e.target.checked;
            startToggle(async () => {
              const r = await setEnabled(on);
              if (r.ok) apply(r.data);
              else toast.error(r.error);
            });
          }}
        />
        <span>Run Verdict</span>
      </label>
      <p className="settings-hint">Turns on or off at once.</p>
      {status ? (
        <div className="verdict-state-row">
          <p className="verdict-state" data-state={status.state} role="status">
            {stateCopy(status)}
          </p>
          {canRetry && (
            <button
              type="button"
              className="action-btn"
              disabled={retrying}
              onClick={() =>
                startRetry(async () => {
                  const r = await retry();
                  if (r.ok) apply(r.data);
                  else toast.error(r.error);
                })
              }
            >
              {retrying && <Loader2 className="spin" size={13} />}
              {retrying ? "Retrying…" : "Retry"}
            </button>
          )}
        </div>
      ) : unknown ? (
        <p className="settings-hint verdict-unknown">
          {"Couldn't reach worklog — Verdict's state is unknown."}
        </p>
      ) : (
        <p className="settings-hint">Checking Verdict…</p>
      )}
      {status?.scorecard != null && (
        <p className="settings-hint verdict-scorecard">
          {`Last night's check: ${status.scorecard}`}
        </p>
      )}
      <label className="settings-field verdict-check">
        <input
          type="checkbox"
          aria-label="Send ready lines to Tempo at 17:00"
          checked={autoSend}
          onChange={(e) => onAutoSend(e.target.checked)}
        />
        <span>Send ready lines to Tempo at 17:00</span>
      </label>
      <p className="settings-hint">
        Only lines with a clear ticket and checked text are sent. The rest wait on
        their day. Saved with Save changes.
      </p>
    </section>
  );
}
