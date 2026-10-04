"use server";

// Logged Server Actions (spec 014). `run` is duplicated from actions-hub.ts;
// reads skip revalidation.

import { revalidatePath } from "next/cache";
import * as logged from "@/lib/daemonLogged";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";
import type { ActionResult } from "./actions";

async function run<T>(fn: () => Promise<T>, revalidate = false): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    if (revalidate) {
      try {
        revalidatePath("/logged", "layout");
      } catch (e) {
        return {
          ok: false,
          error: `write succeeded but page refresh failed: ${(e as Error).message}`,
        };
      }
    }
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

export async function loadLogged(from: string, to: string): Promise<ActionResult<LoggedRange>> {
  return run(() => logged.getLogged(from, to));
}

export async function refreshLogged(from: string, to: string): Promise<ActionResult<LoggedRange>> {
  return run(() => logged.pullLogged(from, to), true);
}

export async function dismissLoggedDay(
  day: string,
  reason: string,
): Promise<ActionResult<LoggedDay>> {
  return run(() => logged.dismissDay(day, reason), true);
}

export async function undismissLoggedDay(day: string): Promise<ActionResult<LoggedDay>> {
  return run(() => logged.undismissDay(day), true);
}
