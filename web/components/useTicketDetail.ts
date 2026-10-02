import { useEffect, useState } from "react";

import type { TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";

export type DetailLoad = { s: "loading" } | { s: "error"; error: string } | { s: "ok"; detail: TicketDetail; at: Date };

/** Loads the ticket's Jira detail on open / key change; `retry` loads again. `at` is when Jira last answered. */
export function useTicketDetail(key: string, actions: TaskActions) {
  const [load, setLoad] = useState<DetailLoad>({ s: "loading" });
  const [attempt, setAttempt] = useState(0);
  const loadDetail = actions.loadTicketDetail;
  useEffect(() => {
    let live = true;
    setLoad({ s: "loading" });
    loadDetail(key).then((res) => {
      if (live) setLoad(res.ok ? { s: "ok", detail: res.data, at: new Date() } : { s: "error", error: res.error });
    });
    return () => {
      live = false;
    };
  }, [key, attempt, loadDetail]);
  return { load, retry: () => setAttempt((n) => n + 1) };
}
