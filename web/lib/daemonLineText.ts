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
  started: boolean;
  /** Why the job didn't start — e.g. `"already running"`. Present only
   * when `started` is false. */
  reason?: string;
}

/** Starts text generation for exactly one billing line in the
 * background and returns immediately (FR-33) — poll `lineTextStatus`
 * for the outcome. A second call for a key already running comes back
 * `started: false` with `reason: "already running"`. */
export async function regenerateLine(input: LineTextKey): Promise<RegenerateLineResult> {
  return call("POST", "/billing/lines/regenerate", input);
}

export type LineTextJobState = "idle" | "running" | "done" | "failed";

export interface LineTextStatusResult {
  state: LineTextJobState;
  /** Present only when `state` is `"failed"` — e.g. `"hand-edited"`. */
  reason?: string;
}

/** Polled while a regenerate is in flight. `"idle"` means nothing has
 * ever been requested for this key this daemon run. */
export async function lineTextStatus(key: LineTextKey): Promise<LineTextStatusResult> {
  const q = new URLSearchParams({
    day: key.day,
    folder: key.folder,
    customer: key.customer,
  }).toString();
  return call("GET", `/billing/lines/status?${q}`);
}
