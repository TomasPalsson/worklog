// Tests for the overlap-allocation Server Actions (actions-overlaps.ts).
// Mirrors app/actions.test.ts's pattern: mock next/cache and the daemon
// client, then exercise the exported actions directly.

import { afterAll, beforeAll, describe, expect, it, mock } from "bun:test";

const revalidateImpl = mock((_p: string) => {
  /* happy default */
});
mock.module("next/cache", () => ({
  revalidatePath: (p: string) => revalidateImpl(p),
}));

const allocateCalls: Array<[string, string, string, Record<string, number>]> = [];
const deleteCalls: Array<[string, string, string]> = [];
const allocateImpl = mock(
  async (day: string, started_at: string, ended_at: string, shares: Record<string, number>) => {
    allocateCalls.push([day, started_at, ended_at, shares]);
    return { day, blocks: 2, minutes: 60 };
  },
);
const deleteImpl = mock(async (day: string, started_at: string, ended_at: string) => {
  deleteCalls.push([day, started_at, ended_at]);
  return { day, blocks: 2, minutes: 60 };
});

mock.module("@/lib/daemonOverlaps", () => ({
  allocateOverlap: (day: string, s: string, e: string, shares: Record<string, number>) =>
    allocateImpl(day, s, e, shares),
  deleteOverlapAllocation: (day: string, s: string, e: string) => deleteImpl(day, s, e),
}));

let allocateOverlap: (
  day: string,
  started_at: string,
  ended_at: string,
  shares: Record<string, number>,
) => Promise<{ ok: true; data: undefined } | { ok: false; error: string }>;
let resetOverlapAllocation: (
  day: string,
  started_at: string,
  ended_at: string,
) => Promise<{ ok: true; data: undefined } | { ok: false; error: string }>;

beforeAll(async () => {
  const mod = await import("./actions-overlaps");
  allocateOverlap = mod.allocateOverlap;
  resetOverlapAllocation = mod.resetOverlapAllocation;
});

afterAll(() => {
  revalidateImpl.mockReset();
});

describe("allocateOverlap", () => {
  it("saves the split via the daemon and revalidates the day page", async () => {
    allocateCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    const r = await allocateOverlap("2026-09-24", "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", {
      "vitinn-infra": 0.7,
      lyfjastofnun: 0.3,
    });
    expect(r.ok).toBe(true);
    expect(allocateCalls).toEqual([
      [
        "2026-09-24",
        "2026-09-24T10:00:00Z",
        "2026-09-24T11:00:00Z",
        { "vitinn-infra": 0.7, lyfjastofnun: 0.3 },
      ],
    ]);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("surfaces a daemon validation error (e.g. bad shares) through ActionResult", async () => {
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    allocateImpl.mockImplementationOnce(async () => {
      throw new Error("shares must sum to 1.0 (±0.001), got 0.5");
    });
    const r = await allocateOverlap("2026-09-24", "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", {
      "vitinn-infra": 0.5,
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toContain("sum to 1.0");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});

describe("resetOverlapAllocation", () => {
  it("deletes the saved allocation via the daemon and revalidates", async () => {
    deleteCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    const r = await resetOverlapAllocation(
      "2026-09-24",
      "2026-09-24T10:00:00Z",
      "2026-09-24T11:00:00Z",
    );
    expect(r.ok).toBe(true);
    expect(deleteCalls).toEqual([
      ["2026-09-24", "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z"],
    ]);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });
});
