"use server";

// Server Action for the pre-send checklist (spec 018, FR-11).

import type { ActionResult } from "@/app/actions";
import { call } from "./daemon";
import type { PreflightRow } from "./daily_helpers_contract";

export async function loadPreflight(day: string): Promise<ActionResult<PreflightRow[]>> {
  try {
    return { ok: true, data: await call("GET", `/preflight?from=${day}&to=${day}`) };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}
