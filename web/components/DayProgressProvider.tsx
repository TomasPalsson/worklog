"use client";

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { loadDayProgress } from "@/app/actions-progress";
import { pendingSeconds } from "@/lib/progress";
import type { Block, TicketProgress } from "@/lib/types";
import { EstimateBar } from "./EstimateBar";

type State = { tickets: Map<string, TicketProgress> | null; failed: boolean };
interface Value extends State {
  day: string;
  retry: (key: string) => void;
}

const Ctx = createContext<Value | null>(null);

// The daemon stamps stale rows with the epoch; that is "unknown", not a time.
const known = (t: TicketProgress): TicketProgress =>
  t.pulled_at?.startsWith("1970-01-01") ? { ...t, pulled_at: null } : t;

/** Fetches the day's estimate numbers after first paint so nothing else waits on Jira. */
export function DayProgressProvider({ day, children }: { day: string; children: ReactNode }) {
  const [state, setState] = useState<State>({ tickets: null, failed: false });

  const load = useCallback(
    async (key?: string) => {
      const r = await loadDayProgress(day, key);
      setState((s) => {
        if (!r.ok) return key && s.tickets ? s : { tickets: null, failed: true };
        const next = key && s.tickets ? new Map(s.tickets) : new Map<string, TicketProgress>();
        for (const t of r.data.tickets) next.set(t.key, known(t));
        return { tickets: next, failed: false };
      });
    },
    [day],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const retry = (key: string) => {
    if (state.tickets) {
      void load(key);
    } else {
      setState({ tickets: null, failed: false });
      void load();
    }
  };

  return <Ctx.Provider value={{ ...state, day, retry }}>{children}</Ctx.Provider>;
}

/**
 * The ticket group's bar; nothing outside a provider or once loaded without that ticket.
 * With a Tempo line, "once synced" is what sync will send (the line's billed hours),
 * not the raw block sum.
 */
export function TicketEstimate({
  ticketKey,
  blocks,
  lineSeconds,
  syncState,
}: {
  ticketKey: string;
  blocks: Block[];
  lineSeconds?: number;
  syncState?: "synced" | "dirty" | "unsynced" | "mixed";
}) {
  const ctx = useContext(Ctx);
  if (!ctx) return null;
  const ticket: TicketProgress | undefined = ctx.failed
    ? { key: ticketKey, estimate_seconds: null, people: [], logged_seconds: 0, pulled_at: null, error: "jira_unavailable" }
    : ctx.tickets?.get(ticketKey);
  if (ctx.tickets && !ticket) return null;
  let pending: number;
  if (lineSeconds === undefined) {
    pending = blocks.reduce((n, b) => (b.is_personal ? n : n + pendingSeconds(b)), 0);
  } else if (syncState === "synced") {
    pending = 0;
  } else if (syncState === "unsynced") {
    pending = lineSeconds;
  } else {
    // ponytail: mineToday also counts Tempo time logged by hand outside worklog; upgrade path is
    // reading the line's synced Tempo worklog seconds once the daemon exposes them.
    const mineToday = ticket?.people.find((p) => p.is_you)?.by_day.find(([d]) => d === ctx.day)?.[1] ?? 0;
    pending = Math.max(0, lineSeconds - mineToday);
  }
  return <EstimateBar ticket={ticket} pending={pending} onRetry={() => ctx.retry(ticketKey)} />;
}
