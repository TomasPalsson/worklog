import type { PullReport } from "@/lib/types";

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

export interface WeekSyncDeps {
  pull: (monday: string) => Promise<Result<PullReport>>;
  syncDay: (day: string) => Promise<Result<{ synced: number; skipped: number; errors: string[] }>>;
}

export type WeekSyncOutcome =
  | { ok: true; synced: number; skipped: number }
  | { ok: false; day: string | null; error: string };

export async function syncWeek(
  _monday: string,
  _pendingDays: string[],
  _deps: WeekSyncDeps,
): Promise<WeekSyncOutcome> {
  throw new Error("not implemented");
}
