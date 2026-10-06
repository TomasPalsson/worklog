"use client";

// The day page's one-line Verdict state: quiet when it is off/starting, amber
// when it needs the Owner. Renders nothing while Verdict is running. Actions
// are injectable because @/app/actions is mock.module'd process-wide in tests.

import { useState, useTransition } from "react";
import { Loader2 } from "lucide-react";
import * as actions from "@/app/actions";
import type { VerdictState, VerdictStatus } from "@/lib/verdict_contract";

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

interface Props {
  status: VerdictStatus;
  setEnabled?: (on: boolean) => Promise<Result<VerdictState>>;
  retry?: () => Promise<Result<VerdictState>>;
}

function notChecked(n: number) {
  return n > 0 ? ` · ${n} event${n === 1 ? "" : "s"} not checked` : "";
}

export function VerdictBanner({
  status: initial,
  setEnabled = actions.setVerdictEnabled,
  retry = actions.retryVerdict,
}: Props) {
  const [status, setStatus] = useState(initial);
  const [pending, start] = useTransition();
  const [error, setError] = useState<string | null>(null);
  if (status.state === "running") return null;

  const count = notChecked(status.unchecked);
  const run = (call: () => Promise<Result<VerdictState>>, failed: string) => {
    setError(null);
    start(async () => {
      const r = await call();
      if (r.ok) setStatus((cur) => ({ ...cur, ...r.data }));
      else setError(`${failed}: ${r.error}`);
    });
  };

  let text: string;
  let tone: "quiet" | "warn" = "warn";
  let action: { label: string; busy: string; run: () => void } | null = null;
  switch (status.state) {
    case "off":
      tone = "quiet";
      text = `Verdict is off${count}`;
      action = {
        label: "Turn on",
        busy: "Turning on…",
        run: () => run(() => setEnabled(true), "Couldn't turn Verdict on"),
      };
      break;
    case "starting":
      tone = "quiet";
      text = "Verdict is starting";
      break;
    case "not_answering":
      text = `Verdict isn't answering${count}`;
      action = {
        label: "Turn on",
        busy: "Turning on…",
        run: () => run(retry, "Couldn't turn Verdict on"),
      };
      break;
    case "needs_uv":
      text = `Verdict needs uv${count}`;
      break;
    case "stopped":
      text = `Verdict stopped: ${status.error}`;
      action = {
        label: "Retry",
        busy: "Retrying…",
        run: () => run(retry, "Couldn't retry Verdict"),
      };
      break;
  }

  return (
    <div className="verdict-line review-line" data-tone={tone} role="status">
      <span className="review-summary">{text}</span>
      {action && (
        <button
          type="button"
          className="review-toggle"
          disabled={pending}
          onClick={action.run}
        >
          {pending && <Loader2 className="spin" size={13} />}
          {pending ? action.busy : action.label}
        </button>
      )}
      {error && <span className="verdict-line-error"> {error}</span>}
    </div>
  );
}
