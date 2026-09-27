// Daemon client fns for a single billing line's text (spec 006,
// FR-26/FR-31/FR-33/FR-35) — split out of lib/daemon.ts (at its
// line-count limit) — shares its `call` transport.

import { call } from "./daemon";

export interface LineTextKey {
  day: string;
  folder: string;
  /** `""` when the line's customer is unresolved. */
  customer: string;
}

export interface SetLineTextInput extends LineTextKey {
  text: string;
}

/** Hand a billing line's text back to the owner's own words (FR-31), or,
 * with an empty `text`, reset it back to the next generated/fallback
 * value. */
export async function setLineText(input: SetLineTextInput): Promise<{ ok: true }> {
  return call("POST", "/billing/lines/text", input);
}

export interface RegenerateLineResult {
  generated: boolean;
  /** Why nothing was (re)generated — e.g. `"hand-edited"`. Present only
   * when `generated` is false. */
  reason?: string;
}

/** Re-run text generation for exactly one billing line (FR-33). A hand-
 * edited line comes back `generated: false` with `reason: "hand-edited"`
 * and is left untouched. */
export async function regenerateLine(input: LineTextKey): Promise<RegenerateLineResult> {
  return call("POST", "/billing/lines/regenerate", input);
}
