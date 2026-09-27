// Daemon client fn for the block Details view (spec 006, FR-20) — split
// out of lib/daemon.ts (at its line-count limit) — shares its `call`
// transport.

import { call } from "./daemon";
import type { DetailRow } from "./clues_contract";

/** Every event of a block, including helper/message activity in its
 * span, in time order. */
export async function blockDetails(id: number): Promise<DetailRow[]> {
  return call<DetailRow[]>("GET", `/blocks/${id}/details`);
}
