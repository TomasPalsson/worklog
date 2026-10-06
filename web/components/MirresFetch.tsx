"use client";

import { useId, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { refreshMirresAction } from "@/app/actions-tempo-lines";
import { todayISO } from "@/lib/format";
import { MirresIcon } from "./icons";

interface Props {
  /** Default date for the picker. Ignored when `fixedDay` is set. */
  day?: string;
  /** Fetch this one day; hides the date input (per-row "Fetch again"). */
  fixedDay?: string;
}

export function MirresFetch({ day, fixedDay }: Props) {
  const router = useRouter();
  const id = useId();
  const today = todayISO();
  const [picked, setPicked] = useState(day ?? today);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, start] = useTransition();
  const target = fixedDay ?? picked;

  function fetchDay() {
    setStatus(null);
    setError(null);
    start(async () => {
      const r = await refreshMirresAction(target);
      if (!r.ok) {
        setError(r.error);
        return;
      }
      const matched = r.data.filter((l) => l.billing).length;
      setStatus(`Fetched ${target} · ${matched} of ${r.data.length} lines matched`);
      router.refresh();
    });
  }

  return (
    <div className="mirres-fetch">
      <div className="mirres-fetch-row">
        {!fixedDay && (
          <>
            <label htmlFor={id}>Day</label>
            <input
              id={id}
              type="date"
              value={picked}
              max={today}
              onChange={(e) => e.target.value && setPicked(e.target.value)}
            />
          </>
        )}
        <button
          type="button"
          className="action-btn"
          disabled={pending}
          aria-busy={pending || undefined}
          aria-label={fixedDay ? `Fetch ${fixedDay} again from Mirres` : undefined}
          onClick={fetchDay}
        >
          <MirresIcon />
          {pending ? "Fetching…" : fixedDay ? "Fetch again" : "Fetch from Mirres"}
        </button>
      </div>
      <FetchMessages status={status} error={error} />
    </div>
  );
}

function FetchMessages({ status, error }: { status: string | null; error: string | null }) {
  return (
    <>
      {status && <p role="status" className="mirres-fetch-status">{status}</p>}
      {error && (
        <div role="alert" className="mirres-fetch-error">
          <p className="export-error">Couldn&apos;t fetch from Mirres — {error}</p>
          <p className="export-hint">
            Check the Mirres settings: run <code>worklog secret list</code> and look for the four mirres_* entries.
          </p>
        </div>
      )}
    </>
  );
}
