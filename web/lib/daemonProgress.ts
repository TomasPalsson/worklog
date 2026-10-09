// Daemon client fn for the per-person estimate progress endpoint.

import { call } from "./daemon";
import type { DayProgress } from "./types";

/** `refresh` forces that one ticket's Jira numbers to be re-pulled. */
export async function getDayProgress(day: string, refresh?: string): Promise<DayProgress> {
  const q = refresh === undefined ? "" : `?refresh=${encodeURIComponent(refresh)}`;
  return call("GET", `/progress/${day}${q}`);
}
