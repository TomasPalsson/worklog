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
      day,
      folder: "genai-infra",
      customer: "Sjúkra",
      // Never guessed — a real unresolved accounting key.
      verkefni: null as string | null,
      ticket: "GENAI-1219",
      seconds: 19800,
      hours: 5.5,
      billable: true,
      invoice_text: "Document analyzer work",
    },
  ],
  rendered: {
    text: "23.07.2026  Sjúkra  —  5,5 hrs  Reikningshæft  Document analyzer work",
    csv:
      "dagsetning,vidskiptamadur,verkefni,tegund_skraningar,taxti,timar,reikningshaefi,texti_a_reikning\n" +
      '23.07.2026,Sjúkra,,Almenn skráning,Dagvinna,"5,5",Reikningshæft,Document analyzer work',
    json: '[{"vidskiptamadur":"Sjúkra"}]',
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
  loadBillingRegistry: async () => ({
    customers: [{ id: 1, name: "Sjúkra", aliases: ["Sjukra"] }],
    folders: [
      {
        id: 1,
        folder: "genai-infra",
        customer: null,
        verkefni: null,
        billable: true,
      },
    ],
    unmapped: [{ folder: "autofixer", events: 42 }],
  }),
  saveBillingCustomer: async () => ({ id: 1 }),
  deleteBillingCustomer: async () => ({ removed: true }),
  saveBillingFolder: async () => ({ id: 1 }),
  deleteBillingFolder: async () => ({ removed: true }),
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
type Fail = { ok: false; error: string };
type Ok<T> = { ok: true; data: T };

let actions: {
  exportBilling: (day: string) => Promise<
    | Ok<{
        day: string;
        exported_at: string | null;
        rows: { customer: string | null; verkefni: string | null }[];
        rendered: { text: string; csv: string; json: string };
      }>
    | Fail
  >;
  markExported: (
    day: string,
  ) => Promise<Ok<{ day: string; marked: number; exported_at: string | null }> | Fail>;
  fetchBillingRegistry: () => Promise<
    | Ok<{
        customers: { name: string; aliases: string[] }[];
        folders: { folder: string; customer: string | null }[];
        unmapped: { folder: string; events: number }[];
      }>
    | Fail
  >;
  saveBillingCustomer: (
    c: { name: string; aliases: string[] },
    day: string,
  ) => Promise<Ok<{ id: number }> | Fail>;
  saveBillingFolder: (
    f: {
      folder: string;
      customer: string | null;
      verkefni: string | null;
      billable: boolean;
    },
    day: string,
  ) => Promise<Ok<{ id: number }> | Fail>;
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
    expect(r.data.rows[0].customer).toBe("Sjúkra");
    // The accounting key is never guessed — null means "user picks".
    expect(r.data.rows[0].verkefni).toBeNull();
    expect(r.data.rendered.csv.split("\n")[0]).toBe(
      "dagsetning,vidskiptamadur,verkefni,tegund_skraningar,taxti,timar,reikningshaefi,texti_a_reikning",
    );
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

describe("billing registry actions", () => {
  it("fetchBillingRegistry returns customers, folder pins and unmapped folders", async () => {
    revalidateImpl.mockReset();
    const r = await actions.fetchBillingRegistry();
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.data.customers[0].name).toBe("Sjúkra");
    // A null customer marks a shared folder (resolve per line from text).
    expect(r.data.folders[0].customer).toBeNull();
    expect(r.data.unmapped[0].folder).toBe("autofixer");
    // Read-only — must not invalidate the page.
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("registry writes revalidate the day, since mappings change the export", async () => {
    revalidateImpl.mockReset();
    revalidateImpl.mockImplementation(() => {});
    await actions.saveBillingFolder(
      { folder: "autofixer", customer: "APRÓ", verkefni: null, billable: true },
      "2026-07-23",
    );
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-07-23");
  });

  it("surfaces a registry write failure as ok=false", async () => {
    revalidateImpl.mockImplementation(() => {
      throw new Error("cache unavailable");
    });
    const r = await actions.saveBillingCustomer(
      { name: "Sensa", aliases: [] },
      "2026-07-23",
    );
    expect(r.ok).toBe(false);
    revalidateImpl.mockImplementation(() => {});
  });
});
