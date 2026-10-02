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
  monday: string,
  pendingDays: string[],
  { pull, syncDay }: WeekSyncDeps,
): Promise<WeekSyncOutcome> {
  const before = await pull(monday);
  if (!before.ok) return { ok: false, day: null, error: before.error };
  let synced = 0;
  let skipped = 0;
  for (const day of pendingDays) {
    const res = await syncDay(day);
    if (!res.ok) return { ok: false, day, error: res.error };
    if (res.data.errors.length > 0) return { ok: false, day, error: res.data.errors.join("; ") };
    synced += res.data.synced;
    skipped += res.data.skipped;
  }
  const after = await pull(monday);
  if (!after.ok) return { ok: false, day: null, error: after.error };
  return { ok: true, synced, skipped };
}
