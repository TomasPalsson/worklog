"use server";

// Tempo ticket-line Server Actions (spec 011). `run` is duplicated from
// actions-line-text.ts, where it isn't exported.

import { revalidatePath } from "next/cache";
import {
  regenerateTempoLine,
  setTempoLineHours,
  setTempoLineText,
} from "@/lib/daemonTempoLines";
import type { TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";
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

export async function saveTempoLineText(
  key: TempoLineKey,
  text: string,
): Promise<ActionResult<TempoLine>> {
  return run(() => setTempoLineText(key, text), key.day);
}

export async function saveTempoLineHours(
  key: TempoLineKey,
  seconds: number | null,
): Promise<ActionResult<TempoLine>> {
  return run(() => setTempoLineHours(key, seconds), key.day);
}

export async function regenerateTempoLineText(
  key: TempoLineKey,
): Promise<ActionResult<TempoLine>> {
  return run(() => regenerateTempoLine(key), key.day);
}
