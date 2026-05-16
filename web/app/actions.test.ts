// Tests for the Server Action wrapper. We can't easily hit the full
// server-action pipeline from a unit test (it'd need a Next.js runtime),
// but `_runActionForTests` is the pure logic and testing it directly
// exercises every branch the exported wrappers rely on.
//
// Critical regression target: a fn throwing OR a revalidatePath throwing
// must NEVER escape as an unhandled promise rejection — useTransition
// would swallow it and leave the UI silently in a "success" state.

import { afterAll, beforeAll, describe, expect, it, mock } from "bun:test";

// Next.js's `next/cache` only works inside a Next runtime; in bun:test
// the module loads but calling `revalidatePath` throws. We mock the
// whole module so we can control its behaviour per-test.
const revalidateImpl = mock((_p: string) => {
  /* happy default */
});
mock.module("next/cache", () => ({
  revalidatePath: (p: string) => revalidateImpl(p),
}));

// Stub the daemon so we never make real network calls from the unit test.
mock.module("@/lib/daemon", () => ({
  assignTicket: async () => ({}),
  setDuration: async () => ({}),
  setDescription: async () => ({}),
  setPersonal: async () => ({}),
  deleteBlock: async () => ({}),
  runInfer: async () => ({ day: "x", blocks: 0, minutes: 0 }),
  runEstimate: async () => ({ day: "x", estimated: 0, skipped: 0, failed: 0 }),
  runSync: async () => ({ day: "x", dry_run: true, synced: 0, skipped: 0, errors: [] }),
  refreshJira: async () => ({ tickets_written: 0, source: "jira" }),
  listBlockEvents: async () => [],
  listBlockCommits: async () => [],
}));

let _runActionForTests: <T>(
  fn: () => Promise<T>,
  revalidateOn?: string,
) => Promise<{ ok: true; data: T } | { ok: false; error: string }>;

beforeAll(async () => {
  const mod = await import("./actions");
  _runActionForTests = mod._runActionForTests;
});

afterAll(() => {
  revalidateImpl.mockReset();
});

describe("runAction (ActionResult wrapper)", () => {
  it("returns ok=true with the resolved value on happy path", async () => {
    revalidateImpl.mockImplementation(() => {});
    const r = await _runActionForTests(async () => 42, "/2026-04-18");
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.data).toBe(42);
  });

  it("calls revalidatePath on success", async () => {
    revalidateImpl.mockReset();
    await _runActionForTests(async () => "ok", "/2026-04-18");
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-04-18");
  });

  it("skips revalidatePath when revalidateOn is undefined", async () => {
    revalidateImpl.mockReset();
    const r = await _runActionForTests(async () => "ok");
    expect(r.ok).toBe(true);
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("returns ok=false with the thrown message when fn throws", async () => {
    // The fn itself failed — analogous to the daemon rejecting a write.
    // Previously useTransition would have eaten this silently.
    const r = await _runActionForTests(async () => {
      throw new Error("daemon 500: foo");
    }, "/2026-04-18");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toBe("daemon 500: foo");
  });

  it("returns ok=false when fn throws a non-Error value", async () => {
    const r = await _runActionForTests(async () => {
      // eslint-disable-next-line no-throw-literal
      throw "raw string";
    });
    expect(r.ok).toBe(false);
    // "raw string" has no .message — the fallback kicks in.
    if (!r.ok) expect(r.error).toBe("unknown error");
  });

  it("returns a partial-failure ActionResult when revalidatePath throws", async () => {
    // Regression for the round-2 finding: the daemon write succeeded
    // but the cache-invalidation layer is misconfigured. Previously
    // this escaped as an unhandled exception → swallowed by
    // useTransition → UI looked successful but was stale.
    revalidateImpl.mockImplementation(() => {
      throw new Error("cache unavailable");
    });
    const r = await _runActionForTests(async () => "success", "/2026-04-18");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("page refresh failed");
      expect(r.error).toContain("cache unavailable");
    }
    revalidateImpl.mockImplementation(() => {});
  });

  it("does not call revalidatePath when the fn throws", async () => {
    revalidateImpl.mockReset();
    await _runActionForTests(async () => {
      throw new Error("nope");
    }, "/2026-04-18");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});
