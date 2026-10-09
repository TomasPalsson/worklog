"use client";

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { loadDayProgress } from "@/app/actions-progress";
import { pendingSeconds } from "@/lib/progress";
import type { Block, TicketProgress } from "@/lib/types";
import { EstimateBar } from "./EstimateBar";

type State = { tickets: Map<string, TicketProgress> | null; failed: boolean };
interface Value extends State {
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

  return <Ctx.Provider value={{ ...state, retry }}>{children}</Ctx.Provider>;
}

/** The block's ticket bar; nothing for personal/unassigned blocks, outside a provider, or once loaded without that ticket. */
export function BlockEstimate({ block }: { block: Block }) {
  const ctx = useContext(Ctx);
  const ticketKey = block.jira_issue;
  if (!ctx || !ticketKey || block.is_personal) return null;
  const ticket: TicketProgress | undefined = ctx.failed
    ? { key: ticketKey, estimate_seconds: null, people: [], logged_seconds: 0, pulled_at: null, error: "jira_unavailable" }
    : ctx.tickets?.get(ticketKey);
  if (ctx.tickets && !ticket) return null;
  return <EstimateBar ticket={ticket} pending={pendingSeconds(block)} onRetry={() => ctx.retry(ticketKey)} />;
}
