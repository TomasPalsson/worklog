"use server";

// Billing-line text Server Actions — hand-edit + regenerate (spec 006,
// FR-26/FR-31/FR-33/FR-35). Split out of actions.ts (at its line-count
// limit). Same ActionResult/revalidate shape as the rest of that file;
// `run` isn't exported so it isn't itself treated as an action.

import { revalidatePath } from "next/cache";
import {
  lineTextStatus as daemonLineTextStatus,
  regenerateLine as daemonRegenerateLine,
  setLineText as daemonSetLineText,
  type LineTextKey,
  type LineTextStatusResult,
  type RegenerateLineResult,
  type SetLineTextInput,
} from "@/lib/daemonLineText";
import type { ActionResult } from "./actions";

async function run<T>(fn: () => Promise<T>, day: string): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    try {
      revalidatePath(`/${day}`);
    } catch (e) {
      return {
        ok: false,
        error: `write succeeded but page refresh failed: ${(e as Error).message}`,
      };
    }
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

/** Save a hand-edited billing-line text (FR-31); an empty `text` resets
 * the line back to its generated/fallback text. */
export async function saveLineText(input: SetLineTextInput): Promise<ActionResult<{ ok: true }>> {
  return run(() => daemonSetLineText(input), input.day);
}

/** Starts text generation for one billing line in the background (FR-33)
 * — poll `lineTextStatusAction` for the outcome. */
export async function regenerateLineText(
  input: LineTextKey,
): Promise<ActionResult<RegenerateLineResult>> {
  return run(() => daemonRegenerateLine(input), input.day);
}

/** Poll a billing line's regenerate job state (spec change set:
 * background regenerate). No `revalidatePath` — this is a pure poll;
 * the caller refreshes the page itself once the job settles. */
export async function lineTextStatusAction(
  input: LineTextKey,
): Promise<ActionResult<LineTextStatusResult>> {
  try {
    return { ok: true, data: await daemonLineTextStatus(input) };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}
