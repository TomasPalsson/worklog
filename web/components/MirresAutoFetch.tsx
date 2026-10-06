"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { refreshMirresAction } from "@/app/actions-tempo-lines";

interface Props {
  day: string;
  /** Server-computed: the day has lines and none has Mirres data yet. */
  needsFetch: boolean;
  /** Test-only override of the server action. */
  fetchDay?: typeof refreshMirresAction;
}

/** Fetches Mirres once per browser session for a day that was never fetched. */
export function MirresAutoFetch({ day, needsFetch, fetchDay = refreshMirresAction }: Props) {
  const router = useRouter();
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (!needsFetch) return;
    const key = `mirres-auto-${day}`;
    try {
      if (sessionStorage.getItem(key)) return;
      sessionStorage.setItem(key, "1");
    } catch {
      // Storage blocked: fetch anyway rather than never.
    }
    setPending(true);
    fetchDay(day)
      .then((r) => {
        if (r.ok) router.refresh();
      })
      .catch(() => {})
      .finally(() => setPending(false));
  }, [day, needsFetch, fetchDay, router]);

  return pending ? (
    <span role="status" className="mirres-sr">
      Fetching Mirres…
    </span>
  ) : null;
}
