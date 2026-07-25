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

// Controls the stubbed billing-export daemon calls per-test so we can
// exercise both the happy path and the daemon-unreachable branch.
const exportBillingImpl = mock(async (day: string) => ({
  day,
  exported_at: null as string | null,
  rows: [
    {
      repo: "genai-infra",
      description: "Create MCP server",
      kind: "Work" as const,
      seconds: 14400,
      hours: 4,
    },
  ],
  rendered: {
    text: "repo: genai-infra  description: Create MCP server  time: 4 hrs  type: Work",
    csv: "repo,description,hours,type\ngenai-infra,Create MCP server,4,Work",
    json: '[{"repo":"genai-infra"}]',
  },
}));
const markExportedImpl = mock(async (day: string) => ({
  day,
  marked: 3,
  exported_at: "2026-07-23T18:00:00.000Z",
}));

// Stub the daemon so we never make real network calls from the unit test.
mock.module("@/lib/daemon", () => ({
  exportBilling: (day: string) => exportBillingImpl(day),
  markExported: (day: string) => markExportedImpl(day),
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
  loadSettings: async () => ({
    personal: { work: [], personal: [] },
    secrets: [],
    timezone: "",
    personal_config_path: null,
  }),
  saveSettings: async () => ({
    personal: { work: [], personal: [] },
    secrets: [],
    timezone: "",
    personal_config_path: null,
    reclassified: null,
  }),
  listProjects: async () => [],
  listAccounts: async () => [],
  createTicket: async () => ({
    key: "PROJ-1",
    summary: "x",
    status: null,
    updated: null,
  }),
}));

let _runActionForTests: <T>(
  fn: () => Promise<T>,
  revalidateOn?: string,
) => Promise<{ ok: true; data: T } | { ok: false; error: string }>;
// The billing-export actions under test. Typed structurally rather than
// via `typeof import("./actions")` so the test doesn't need the "use
// server" module's full surface.
let actions: {
  exportBilling: (day: string) => Promise<
    | {
        ok: true;
        data: {
          day: string;
          exported_at: string | null;
          rows: { repo: string; kind: string }[];
          rendered: { text: string; csv: string; json: string };
        };
      }
    | { ok: false; error: string }
  >;
  markExported: (day: string) => Promise<
    | { ok: true; data: { day: string; marked: number; exported_at: string | null } }
    | { ok: false; error: string }
  >;
};

beforeAll(async () => {
  const mod = await import("./actions");
  _runActionForTests = mod._runActionForTests;
  actions = mod;
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

describe("billing export actions", () => {
  it("B15: exportBilling returns the day's rows and pre-rendered formats", async () => {
    revalidateImpl.mockImplementation(() => {});
    const r = await actions.exportBilling("2026-07-23");
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.data.day).toBe("2026-07-23");
    expect(r.data.rows).toHaveLength(1);
    expect(r.data.rows[0].repo).toBe("genai-infra");
    // `kind` (not `type`) is what the daemon serialises on the structured
    // rows — a mismatch here renders an empty column in the panel.
    expect(r.data.rows[0].kind).toBe("Work");
    expect(r.data.rendered.text).toContain("type: Work");
    expect(r.data.rendered.csv.split("\n")[0]).toBe("repo,description,hours,type");
  });

  it("is a read-only action — does not revalidate the page", async () => {
    revalidateImpl.mockReset();
    await actions.exportBilling("2026-07-23");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("B18: surfaces a daemon failure as ok=false instead of throwing", async () => {
    exportBillingImpl.mockImplementationOnce(async () => {
      throw new Error("daemon request to /export/2026-07-23 timed out");
    });
    const r = await actions.exportBilling("2026-07-23");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toContain("timed out");
  });

  it("markExported reports how many blocks were newly marked", async () => {
    revalidateImpl.mockImplementation(() => {});
    const r = await actions.markExported("2026-07-23");
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.data.marked).toBe(3);
    expect(r.data.exported_at).toBe("2026-07-23T18:00:00.000Z");
  });

  it("markExported is a mutation — revalidates the day page", async () => {
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    await actions.markExported("2026-07-23");
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-07-23");
  });
});
