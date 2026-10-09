// Daemon client for the Statistics page (GET /stats).

import { call } from "./daemon";
import type { StatsReport } from "./stats_contract";

export async function getStats(from?: string, to?: string): Promise<StatsReport> {
  const q = new URLSearchParams();
  if (from) q.set("from", from);
  if (to) q.set("to", to);
  const qs = q.toString();
  return call("GET", "/stats" + (qs ? `?${qs}` : ""));
}
