"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { RefreshCw } from "lucide-react";
import { refreshLogged } from "@/app/actions-logged";

type Status = { s: "fetching" } | { s: "ok" } | { s: "failed"; error: string };

function reason(error: string): string {
  if (/token|401|403|unauthor/i.test(error)) return "Tempo didn't accept the token — check it in Settings.";
  if (/timed? ?out|timeout/i.test(error)) return "Tempo took too long — try Refresh again.";
  return "Try Refresh again in a minute.";
}

const clock = (iso: string) => (
  <time suppressHydrationWarning dateTime={iso}>
    {new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}
  </time>
);

export function LoggedFetch({ from, to, pulledAt }: { from: string; to: string; pulledAt: string | null }) {
  const router = useRouter();
  const [status, setStatus] = useState<Status>({ s: "fetching" });
  const [stored, setStored] = useState(pulledAt);
  const key = `${from}|${to}`;
  const current = useRef(key);
  const started = useRef<string | null>(null);

  const run = useCallback(async () => {
    setStatus({ s: "fetching" });
    const res = await refreshLogged(from, to);
    if (current.current !== `${from}|${to}`) return;
    if (res.ok) {
      setStored(res.data.pulled_at);
      setStatus({ s: "ok" });
      router.refresh();
    } else {
      setStatus({ s: "failed", error: res.error });
    }
  }, [from, to, router]);

  useEffect(() => {
    current.current = key;
    setStored(pulledAt);
    if (started.current === key) return;
    started.current = key;
    void run();
  }, [key, pulledAt, run]);

  const failed = status.s === "failed";
  const fetching = status.s === "fetching";
  return (
    <div className="logged-fetch">
      <p
        aria-live="polite"
        role={failed ? "alert" : undefined}
        className={failed ? "logged-fetch-warn" : undefined}
        title={failed ? status.error : undefined}
      >
        {fetching && <>Fetching from Tempo…{stored && <> Showing data from {clock(stored)}.</>}</>}
        {status.s === "ok" && (stored ? <>From Tempo · updated {clock(stored)}</> : "From Tempo")}
        {failed && (
          <>
            Couldn&apos;t reach Tempo — {stored ? <>showing data from {clock(stored)}.</> : "nothing stored yet."}{" "}
            {reason(status.error)}
          </>
        )}
      </p>
      <button type="button" className="action-btn" onClick={() => void run()} disabled={fetching} aria-busy={fetching}>
        <RefreshCw size={14} className={fetching ? "logged-spin" : undefined} />
        Refresh from Tempo
      </button>
    </div>
  );
}
