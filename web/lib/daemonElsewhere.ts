// Daemon client fns for the per-day "done elsewhere" list (spec 006,
// FR-05/FR-06) — split out of lib/daemon.ts (at its line-count limit) —
// shares its `call` transport.

import { call } from "./daemon";

/** A row from the daemon's `/days/:day/elsewhere` list: an org commit/PR
 * whose sha is in no local clone (D-07). */
export interface ElsewhereItem {
  id: number;
  source: string;
  started_at: string; // ISO-8601
  title: string;
  repo: string | null;
}

/** The day's "done elsewhere" list, time-ordered. */
export async function elsewhereForDay(day: string): Promise<ElsewhereItem[]> {
  return call<ElsewhereItem[]>("GET", `/days/${day}/elsewhere`);
}

/** Move a "done elsewhere" item into a chosen block by hand (FR-06). */
export async function moveEventToBlock(eventId: number, blockId: number): Promise<void> {
  return call("POST", `/events/${eventId}/move`, { block_id: blockId });
}
