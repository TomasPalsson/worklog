"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { refreshMirresAction } from "@/app/actions-tempo-lines";

export const MIRRES_REFRESH_MS = 5 * 60 * 1000;

interface Props {
  days: string[];
  /** Test-only override of the server action. */
  fetchDay?: typeof refreshMirresAction;
}

/** A day is due when never fetched, fetched > 5 min ago, or storage is blocked. */
function isDue(key: string): boolean {
  try {
    const t = Number(sessionStorage.getItem(key));
    return !(t > 1) || Date.now() - t > MIRRES_REFRESH_MS;
  } catch {
    return true;
  }
}

function mark(key: string) {
  try {
    sessionStorage.setItem(key, String(Date.now()));
  } catch {
    // Storage blocked: fetch anyway rather than never.
  }
}

/** Refreshes Mirres for the given days on mount, tab focus and visibility, at most every 5 min per day. */
export function MirresAutoFetch({ days, fetchDay = refreshMirresAction }: Props) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false);
  const daysKey = days.join(",");

  useEffect(() => {
    const list = daysKey ? daysKey.split(",") : [];
    const run = async () => {
      if (inFlight.current) return;
      const due = list.filter((d) => isDue(`mirres-auto-${d}`));
      if (due.length === 0) return;
      inFlight.current = true;
      setPending(true);
      // Marked before fetching so a failure still waits the full interval.
      for (const d of due) mark(`mirres-auto-${d}`);
      let anyOk = false;
      for (const d of due) {
        try {
          if ((await fetchDay(d)).ok) anyOk = true;
        } catch {
          // Per-day failures are silent.
        }
      }
      inFlight.current = false;
      setPending(false);
      if (anyOk) router.refresh();
    };
    const onVisible = () => {
      if (document.visibilityState === "visible") void run();
    };
    void run();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", run);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", run);
    };
  }, [daysKey, fetchDay, router]);

  return pending ? (
    <span role="status" className="mirres-sr">
      Fetching Mirres…
    </span>
  ) : null;
}
