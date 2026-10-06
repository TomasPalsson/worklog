import { describe, expect, it } from "bun:test";
import { daySummary, formatFetchedAt, projectsFrom, statusReason, stripCustomer, totals, warningHelp } from "./mirresOverview";
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
    expect(totals(days)).toEqual({ projects: 1, tickets: 1, days: 1, attention: 0, seconds: 7200, billablePercent: 50 });
  });

  it("handles empty input", () => {
    expect(totals([])).toEqual({ projects: 0, tickets: 0, days: 0, attention: 0, seconds: 0, billablePercent: null });
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
  it("is dd.mm.yyyy hh:mm in local time", () => {
    expect(formatFetchedAt(new Date(2026, 9, 6, 15, 59).toISOString())).toBe("06.10.2026 15:59");
    expect(formatFetchedAt(new Date(2026, 0, 2, 3, 4).toISOString())).toBe("02.01.2026 03:04");
  });
});
