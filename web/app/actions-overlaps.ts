"use server";

// Overlap-allocation Server Actions — split out of actions.ts (at its
// line-count limit). Same ActionResult/revalidate shape as the rest of
// that file; `run` isn't exported so it isn't itself treated as an action.

import { revalidatePath } from "next/cache";
import {
  allocateOverlap as daemonAllocateOverlap,
  deleteOverlapAllocation as daemonDeleteOverlapAllocation,
} from "@/lib/daemonOverlaps";
import type { ActionResult } from "./actions";

async function run(fn: () => Promise<unknown>, day: string): Promise<ActionResult> {
  try {
    await fn();
    try {
      revalidatePath(`/${day}`);
    } catch (e) {
      return {
        ok: false,
        error: `write succeeded but page refresh failed: ${(e as Error).message}`,
      };
    }
    return { ok: true, data: undefined };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

/** Save the owner's manual split of an overlap window and re-run infer. */
export async function allocateOverlap(
  day: string,
  started_at: string,
  ended_at: string,
  shares: Record<string, number>,
): Promise<ActionResult> {
  return run(() => daemonAllocateOverlap(day, started_at, ended_at, shares), day);
}

/** Drop a saved allocation — the automatic split comes back. */
export async function resetOverlapAllocation(
  day: string,
  started_at: string,
  ended_at: string,
): Promise<ActionResult> {
  return run(() => daemonDeleteOverlapAllocation(day, started_at, ended_at), day);
}
