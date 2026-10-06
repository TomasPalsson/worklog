"use client";

import { useState, useTransition } from "react";
import { refreshMirresAction } from "@/app/actions-tempo-lines";

/** Re-pulls the day's billable status from Mirres; failures show inline. */
export function MirresRefresh({ day }: { day: string }) {
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  return (
    <div className="mirres-refresh">
      <button
        type="button"
        className="mirres-refresh-btn"
        disabled={pending}
        onClick={() =>
          startTransition(async () => {
            const r = await refreshMirresAction(day);
            setError(r.ok ? null : r.error);
          })
        }
      >
        {pending ? "Refreshing…" : "Refresh Mirres"}
      </button>
      {error && (
        <span role="alert" className="mirres-refresh-error">
          {error}
        </span>
      )}
    </div>
  );
}
