"use server";

// "Done elsewhere" Server Actions — split out of actions.ts (at its
// line-count limit). Same ActionResult/revalidate shape as the rest of
// that file; `run` isn't exported so it isn't itself treated as an action.

import { revalidatePath } from "next/cache";
import { moveEventToBlock as daemonMoveEventToBlock } from "@/lib/daemonElsewhere";
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

/** Move a "done elsewhere" item into a chosen block by hand (FR-06). */
export async function moveElsewhereEvent(
  eventId: number,
  blockId: number,
  day: string,
): Promise<ActionResult> {
  return run(() => daemonMoveEventToBlock(eventId, blockId), day);
}
