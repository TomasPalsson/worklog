import { describe, expect, it } from "bun:test";
import { barPercent, keyKinds, customerStatusSplit, ledgerSummary, shortDay, customersFrom, dateRangeLabel, goalGap, hrs, sharedHelp, daySummary, ledgerOrder, ledgerSegments, formatFetchedAt, projectsFrom, statusReason, stripCustomer, totals, warningHelp } from "./mirresOverview";
import type { BillingClass, MirresDay, TempoLine } from "./tempo_line_contract";

function line(
  day: string,
  issue: string,
  seconds: number,
  b?: { key: string; cls?: BillingClass; warning?: string },
): TempoLine {
  return {
    day,
    jira_issue: issue,
    text: null,
    text_origin: null,
    fallback_text: "",
    union_seconds: seconds,
    hours_override_seconds: null,
    effective_seconds: seconds,
    billing: b
      ? {
          account_key: b.key,
          project: b.key,
          project_type: null,
          class: b.cls ?? "billable",
          warning: b.warning ?? null,
          customer: null,
          details: null,
        }
      : null,
  };
}
const d = (day: string, lines: TempoLine[]): MirresDay => ({ day, fetched_at: `${day}T10:00:00Z`, lines });

describe("projectsFrom", () => {
  it("groups by account with unique sorted tickets, newest-first days, summed seconds", () => {
    const rows = projectsFrom([
      d("2026-10-05", [line("2026-10-05", "B-2", 3600, { key: "ACME" }), line("2026-10-05", "A-1", 1800, { key: "ACME" })]),
      d("2026-10-06", [line("2026-10-06", "A-1", 1800, { key: "ACME" })]),
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ tickets: ["A-1", "B-2"], days: ["2026-10-06", "2026-10-05"], seconds: 7200 });
  });

  it("takes the newest day's class and warning", () => {
    const rows = projectsFrom([
      d("2026-10-05", [line("2026-10-05", "A-1", 1800, { key: "ACME", cls: "billable", warning: "old" })]),
      d("2026-10-06", [line("2026-10-06", "A-1", 1800, { key: "ACME", cls: "included" })]),
    ]);
    expect(rows[0].class).toBe("included");
    expect(rows[0].warning).toBeNull();
  });

  it("sorts warnings first, then seconds desc, then name", () => {
    const rows = projectsFrom([
      d("2026-10-05", [
        line("2026-10-05", "A-1", 3600, { key: "BIG" }),
        line("2026-10-05", "A-2", 1800, { key: "WARN", warning: "w" }),
        line("2026-10-05", "A-3", 1800, { key: "AAA" }),
        line("2026-10-05", "A-4", 1800, { key: "ZZZ" }),
      ]),
    ]);
    expect(rows.map((r) => r.account_key)).toEqual(["WARN", "BIG", "AAA", "ZZZ"]);
  });

  it("is empty for no input", () => {
    expect(projectsFrom([])).toEqual([]);
  });
});

describe("daySummary / totals", () => {
  const days = [
    d("2026-10-05", [line("2026-10-05", "A-1", 3600, { key: "ACME" }), line("2026-10-05", "A-2", 3600)]),
  ];

  it("counts unmatched lines in total but not matched", () => {
    expect(daySummary(days[0])).toEqual({ matched: 1, total: 2, billablePercent: 50 });
  });

  it("totals over all lines", () => {
    expect(totals(days)).toEqual({ projects: 1, tickets: 1, days: 1, attention: 0, seconds: 7200, billablePercent: 50, billedSeconds: 3600 });
  });

  it("handles empty input", () => {
    expect(totals([])).toEqual({ projects: 0, tickets: 0, days: 0, attention: 0, seconds: 0, billablePercent: null, billedSeconds: 0 });
  });
});

describe("projectsFrom customer/details", () => {
  it("carries the newest day's customer and details", () => {
    const withC = (day: string, customer: string) => {
      const l = line(day, "A-1", 1800, { key: "ACME" });
      l.billing!.customer = customer;
      return d(day, [l]);
    };
    const rows = projectsFrom([withC("2026-10-05", "Old"), withC("2026-10-06", "New")]);
    expect(rows[0].customer).toBe("New");
  });
});

describe("stripCustomer", () => {
  it("strips only the matching prefix", () => {
    expect(stripCustomer("Cori · Vefur", "Cori")).toBe("Vefur");
    expect(stripCustomer("Other · Vefur", "Cori")).toBe("Other · Vefur");
    expect(stripCustomer("Vefur", null)).toBe("Vefur");
    expect(stripCustomer(null, "Cori")).toBeNull();
  });
});

describe("statusReason", () => {
  it("explains quiet states only", () => {
    expect(statusReason("billable", "Fast verð", null)).toBeNull();
    expect(statusReason("included", null, null)).toBe("covered by contract");
    expect(statusReason("not_billable", "Fast verð (x)", null)).toBe("fixed price");
    expect(statusReason("not_billable", "Innifalið í þjónustu", null)).toBe("subscription");
    expect(statusReason("not_billable", "Innri vinna (Apró)", null)).toBe("internal");
    expect(statusReason("not_billable", "Annað", null)).toBeNull();
    expect(statusReason("not_billable", null, null)).toBeNull();
  });
});

describe("warningHelp", () => {
  it("maps known warnings and nothing else", () => {
    expect(warningHelp("Samning vantar í Mirres")).toContain("No contract in Mirres");
    expect(warningHelp("Tímafjölda vantar á samning í Mirres")).toBe("The contract has no hours set in Mirres.");
    expect(warningHelp("Innifaldir tímar uppurnir")).toBe("Included hours are used up for this period.");
    expect(warningHelp("Innifaldir tímar að klárast (1,5 klst eftir)")).toBe("Included hours are almost used up.");
    expect(warningHelp("Ekki virkt Mirres-verkefni")).toContain("not an active Mirres project");
    expect(warningHelp("???")).toBeNull();
    expect(warningHelp(null)).toBeNull();
  });
});

describe("formatFetchedAt", () => {
  it("says fetched hh:mm today, with the day otherwise", () => {
    const now = new Date(2026, 9, 6, 20, 0);
    expect(formatFetchedAt(new Date(2026, 9, 6, 15, 59).toISOString(), now)).toBe("fetched 15:59");
    expect(formatFetchedAt(new Date(2026, 9, 5, 17, 13).toISOString(), now)).toBe("fetched 5 Oct 17:13");
  });
});

describe("customersFrom / ledgerOrder", () => {
  const withCustomer = (key: string, customer: string | null, seconds: number, cls: BillingClass = "billable") => {
    const l = line("2026-10-06", `${key}-1`, seconds, { key, cls });
    l.billing!.customer = customer;
    return l;
  };
  const days = [
    d("2026-10-06", [
      withCustomer("A", null, 9000),
      withCustomer("B", "Beta", 3600, "not_billable"),
      withCustomer("C", "Beta", 1800),
      withCustomer("D", "Delta", 1800, "included"),
      withCustomer("E", "Echo", 7200),
    ]),
  ];

  it("groups by customer, hours desc, unknown last", () => {
    const c = customersFrom(days);
    expect(c.map((g) => g.name)).toEqual(["Echo", "Beta", "Delta", "Unknown customer"]);
    const beta = c.find((g) => g.name === "Beta")!;
    expect(beta.projects.map((p) => p.account_key)).toEqual(["B", "C"]);
    expect(beta.seconds).toBe(5400);
    expect(beta.kind).toBe("not_billable"); // dominant project B
  });

  it("orders the bar billable, included, rest, and drops zero hours", () => {
    const order = ledgerOrder(customersFrom(days)).map((g) => g.name);
    expect(order).toEqual(["Unknown customer", "Echo", "Delta", "Beta"]);
    expect(ledgerOrder(customersFrom([d("2026-10-06", [withCustomer("Z", "Zed", 0)])]))).toEqual([]);
  });

  it("splits a mixed customer into one bar segment per status", () => {
    const segs = ledgerSegments(customersFrom(days)).map((s) => `${s.customer}:${s.kind}:${s.seconds}`);
    // Beta's billable half sits with the billable group, its rest after.
    expect(segs).toEqual([
      "Unknown customer:billable:9000",
      "Echo:billable:7200",
      "Beta:billable:1800",
      "Delta:included:1800",
      "Beta:not_billable:3600",
    ]);
  });
});

describe("hrs / dateRangeLabel / goalGap / sharedHelp", () => {
  it("formats compact hours", () => expect(hrs(12600)).toBe("3.5h"));
  it("formats ranges", () => {
    expect(dateRangeLabel(["2026-10-06"])).toBe("Tue 6 Oct");
    expect(dateRangeLabel(["2026-10-06", "2026-10-05"])).toBe("5–6 Oct");
    expect(dateRangeLabel(["2026-10-02", "2026-09-30"])).toBe("30 Sep – 2 Oct");
    expect(shortDay("2026-10-06")).toBe("Tue 6 Oct");
    expect(dateRangeLabel([])).toBe("");
  });
  it("computes the gap to goal", () => {
    expect(goalGap(0, 36000, 70)).toBe(25200);
    expect(goalGap(30000, 36000, 70)).toBe(0);
  });
  it("shares help only for identical warnings", () => {
    const w = "Samning vantar í Mirres";
    expect(sharedHelp([w, w], 2)).toContain("These 2 projects have no contract");
    expect(sharedHelp([w, "x"], 2)).toBeNull();
  });
});

describe("customerStatusSplit / ledgerSummary", () => {
  const l = (k: string, cls: BillingClass, w?: string) => line("2026-10-06", `${k}-1`, 3600, { key: k, cls, warning: w });
  const days = [d("2026-10-06", [l("A", "billable"), l("B", "not_billable"), l("C", "billable", "Samning vantar í Mirres")])];
  it("splits a customer's hours by status, billable first", () => {
    const c = customersFrom(days)[0];
    expect(customerStatusSplit(c).map((s) => s.kind)).toEqual(["billable", "not_billable", "missing"]);
    expect(customerStatusSplit(c).every((s) => s.seconds === 3600)).toBe(true);
  });
  it("summarises the bar", () => {
    const segs = ledgerSegments(customersFrom(days));
    expect(ledgerSummary(33, 10800, segs, 70)).toBe("33% billable of 3.0h: 1.0h billable, 1.0h other, 1.0h contract missing; goal 70%");
  });
});

describe("barPercent / keyKinds", () => {
  it("scales to the largest customer and guards zero", () => {
    expect(barPercent(1800, 3600)).toBe(50);
    expect(barPercent(3600, 3600)).toBe(100);
    expect(barPercent(10, 0)).toBe(0);
  });
  it("lists only present key kinds, merging the rest into other", () => {
    const seg = (kind: "billable" | "fixed" | "internal" | "missing") => ({ customer: "A", kind, seconds: 1 });
    expect(keyKinds([seg("missing"), seg("fixed"), seg("internal")])).toEqual(["other", "missing"]);
    expect(keyKinds([seg("billable")])).toEqual(["billable"]);
  });
});
