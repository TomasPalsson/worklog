// Daemon client fns for the overlap-allocation endpoints. Split out of
// lib/daemon.ts (at its line-count limit) — shares its `call` transport.

import { call } from "./daemon";

interface AllocationResponse {
  day: string;
  blocks: number;
  minutes: number;
}

/**
 * Save the owner's manual split of an overlap window. `shares` fractions
 * must be > 0 and sum to 1 (±0.001); every key must be one of the
 * overlap's projects — the daemon 400s otherwise. Re-runs infer, so the
 * response's `blocks`/`minutes` reflect the new split.
 */
export async function allocateOverlap(
  day: string,
  started_at: string,
  ended_at: string,
  shares: Record<string, number>,
): Promise<AllocationResponse> {
  return call("POST", `/days/${day}/allocations`, { started_at, ended_at, shares });
}

/** Drop a saved allocation — the automatic per-minute split comes back
 * for that window. Also re-runs infer. */
export async function deleteOverlapAllocation(
  day: string,
  started_at: string,
  ended_at: string,
): Promise<AllocationResponse> {
  return call("POST", `/days/${day}/allocations/delete`, { started_at, ended_at });
}
