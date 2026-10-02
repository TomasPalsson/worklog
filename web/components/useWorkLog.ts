import { useCallback, useEffect, useState } from "react";

import type { TicketBlocks } from "@/lib/types";
import type { TaskActions } from "./TaskCard";

export type BlocksLoad = { s: "loading" } | { s: "error"; error: string } | { s: "ok"; data: TicketBlocks };

/**
 * The ticket's last 14 days of work. Loads on open / key change; `refetch` swaps data in place (no skeleton)
 * after a write. Lives above the tabs so the sidebar and the tab count can read it, and so the days the user
 * expanded survive switching tabs.
 */
export function useWorkLog(key: string, actions: TaskActions) {
  const [load, setLoad] = useState<BlocksLoad>({ s: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const loader = actions.loadTicketBlocks;
  useEffect(() => {
    let live = true;
    setLoad({ s: "loading" });
    loader(key).then((res) => {
      if (live) setLoad(res.ok ? { s: "ok", data: res.data } : { s: "error", error: res.error });
    });
    return () => {
      live = false;
    };
  }, [key, attempt, loader]);
  const refetch = useCallback(async () => {
    const res = await loader(key);
    if (res.ok) setLoad({ s: "ok", data: res.data });
  }, [key, loader]);
  /** An explicit choice wins; otherwise only the newest day is open. */
  const isOpen = (day: string, newest: boolean) => expanded[day] ?? newest;
  const setOpen = useCallback((day: string, open: boolean) => setExpanded((e) => ({ ...e, [day]: open })), []);
  return { load, retry: () => setAttempt((n) => n + 1), refetch, isOpen, setOpen };
}

export type WorkLog = ReturnType<typeof useWorkLog>;

/** Seconds on the ticket's lines across the loaded days; null until loaded. */
export const workSeconds = (load: BlocksLoad): number | null =>
  load.s === "ok" ? load.data.days.reduce((sum, d) => sum + d.line_seconds, 0) : null;
