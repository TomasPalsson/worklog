"use server";

// Read-only Server Action behind the day page's estimate bars. No
// revalidatePath: the client provider applies the result, and a
// revalidate would re-render the page it is decorating.

import { getDayProgress } from "@/lib/daemonProgress";
import type { DayProgress } from "@/lib/types";
import type { ActionResult } from "./actions";

export async function loadDayProgress(
  day: string,
  refresh?: string,
): Promise<ActionResult<DayProgress>> {
  try {
    return { ok: true, data: await getDayProgress(day, refresh) };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}
