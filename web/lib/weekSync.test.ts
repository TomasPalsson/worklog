// B12, B15: Sync week loop — pull, days in order, pull; stop at the first failure.

import { describe, expect, it, mock } from "bun:test";
import type { PullReport } from "@/lib/types";
import { syncWeek } from "./weekSync";

const report: PullReport = { monday: "2026-09-21", worklogs: 0, outside: 0, schedule_days: 5, pulled_at: "t" };
const synced = (n: number) => ({ ok: true as const, data: { synced: n, skipped: 0, errors: [] as string[] } });

describe("syncWeek", () => {
  it("B12: pulls, syncs each pending day in order, then pulls again", async () => {
    const calls: string[] = [];
    const pull = mock(async (m: string) => {
      calls.push(`pull ${m}`);
      return { ok: true as const, data: report };
    });
    const syncDay = mock(async (d: string) => {
      calls.push(`sync ${d}`);
      return synced(2);
    });
    const out = await syncWeek("2026-09-21", ["2026-09-21", "2026-09-22", "2026-09-23"], { pull, syncDay });
    expect(calls).toEqual([
      "pull 2026-09-21",
      "sync 2026-09-21",
      "sync 2026-09-22",
      "sync 2026-09-23",
      "pull 2026-09-21",
    ]);
    expect(out).toEqual({ ok: true, synced: 6, skipped: 0 });
  });

  it("B15: stops at the failing day and never sends the next one", async () => {
    const pull = mock(async () => ({ ok: true as const, data: report }));
    const syncDay = mock(async (d: string) =>
      d === "2026-09-22" ? { ok: false as const, error: "Tempo 502" } : synced(1),
    );
    const out = await syncWeek("2026-09-21", ["2026-09-21", "2026-09-22", "2026-09-23"], { pull, syncDay });
    expect(syncDay.mock.calls.map((c) => c[0])).toEqual(["2026-09-21", "2026-09-22"]);
    expect(pull).toHaveBeenCalledTimes(1);
    expect(out).toEqual({ ok: false, day: "2026-09-22", error: "Tempo 502" });
  });

  it("B15: a day that reports sync errors also stops the loop", async () => {
    const pull = mock(async () => ({ ok: true as const, data: report }));
    const syncDay = mock(async () => ({ ok: true as const, data: { synced: 0, skipped: 0, errors: ["bad key"] } }));
    const out = await syncWeek("2026-09-21", ["2026-09-21", "2026-09-22"], { pull, syncDay });
    expect(syncDay).toHaveBeenCalledTimes(1);
    expect(out).toEqual({ ok: false, day: "2026-09-21", error: "bad key" });
  });

  it("a failed first pull sends nothing", async () => {
    const pull = mock(async () => ({ ok: false as const, error: "no token" }));
    const syncDay = mock(async () => synced(1));
    const out = await syncWeek("2026-09-21", ["2026-09-21"], { pull, syncDay });
    expect(syncDay).not.toHaveBeenCalled();
    expect(out).toEqual({ ok: false, day: null, error: "no token" });
  });
});
