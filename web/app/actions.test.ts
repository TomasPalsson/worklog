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

// Capture daemon-call args so action tests can assert on them.
const mergeBlocksCalls: Array<[number, number[]]> = [];
const estimateBlockCalls: number[] = [];
const mergeBlocksImpl = mock(async (primary: number, absorb: number[]) => {
  mergeBlocksCalls.push([primary, absorb]);
  return { merged: { id: primary, duration_seconds: 3600 }, absorbed: absorb };
});
const estimateBlockImpl = mock(async (blockId: number) => {
  estimateBlockCalls.push(blockId);
  return {
    block_id: blockId,
    description: "Implement merge UX",
    minutes: 30,
    jira_issue: "PROJ-1",
  };
});

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
  searchTickets: async () => [],
  rememberExternalTicket: async () => ({}),
  mergeBlocks: (primary: number, absorb: number[]) => mergeBlocksImpl(primary, absorb),
  estimateBlock: (blockId: number) => estimateBlockImpl(blockId),
}));

let _runActionForTests: <T>(
  fn: () => Promise<T>,
  revalidateOn?: string,
) => Promise<{ ok: true; data: T } | { ok: false; error: string }>;
let mergeGroup: (
  primary: number,
  absorb: number[],
  day: string,
) => Promise<{ ok: true; data: undefined } | { ok: false; error: string }>;
let describeBlock: (
  blockId: number,
  day: string,
) => Promise<
  | {
      ok: true;
      data: {
        block_id: number;
        description: string;
        minutes: number;
        jira_issue: string | null;
      };
    }
  | { ok: false; error: string }
>;

beforeAll(async () => {
  const mod = await import("./actions");
  _runActionForTests = mod._runActionForTests;
  mergeGroup = mod.mergeGroup;
  describeBlock = mod.describeBlock;
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

describe("mergeGroup", () => {
  it("B8: folds the absorb blocks into primary via the daemon", async () => {
    mergeBlocksCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    const r = await mergeGroup(101, [102, 103], "2026-05-26");
    expect(r.ok).toBe(true);
    expect(mergeBlocksCalls).toEqual([[101, [102, 103]]]);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-05-26");
  });

  it("B14: surfaces a daemon merge error through the ActionResult", async () => {
    mergeBlocksCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    // Replace the impl just for this test — restore after.
    mergeBlocksImpl.mockImplementationOnce(async () => {
      throw new Error(
        "block 102 is already synced to Tempo — merging it away would orphan",
      );
    });
    const r = await mergeGroup(101, [102], "2026-05-26");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("already synced");
    }
    // On error, the page should NOT be revalidated — there was nothing
    // to revalidate. (Same discipline as the other CRUD actions.)
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});

describe("describeBlock", () => {
  it("B13: re-describes the block via the daemon and revalidates", async () => {
    estimateBlockCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    const r = await describeBlock(7, "2026-05-26");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.data.block_id).toBe(7);
      expect(r.data.description).toBe("Implement merge UX");
      expect(r.data.minutes).toBe(30);
      expect(r.data.jira_issue).toBe("PROJ-1");
    }
    expect(estimateBlockCalls).toEqual([7]);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-05-26");
  });

  it("surfaces a daemon error (e.g. 400 personal) through ActionResult", async () => {
    estimateBlockCalls.length = 0;
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    estimateBlockImpl.mockImplementationOnce(async () => {
      throw new Error(
        "block 7 is personal — toggle it back to work before re-describing",
      );
    });
    const r = await describeBlock(7, "2026-05-26");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toContain("personal");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});
