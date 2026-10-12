import { describe, expect, test } from "bun:test";
import { billablePercent, type BillingClass, type TempoLine } from "./tempo_line_contract";

function l(seconds: number, cls?: BillingClass, projectType: string | null = null): TempoLine {
  return {
    day: "2026-09-25",
    jira_issue: "P-1",
    text: null,
    text_origin: null,
    fallback_text: "",
    union_seconds: seconds,
    hours_override_seconds: null,
    effective_seconds: seconds,
    billing: cls
      ? { account_key: "A", project: null, project_type: projectType, class: cls, warning: null, customer: null, details: null }
      : null,
  };
}

describe("billablePercent", () => {
  test("null when no line has billing", () => {
    expect(billablePercent([l(3600), l(1800)])).toBeNull();
    expect(billablePercent([])).toBeNull();
  });
  test("mix of classes", () => {
    expect(
      billablePercent([l(3600, "billable"), l(1800, "included"), l(1800, "not_billable", "Innri vinna (Apró)")]),
    ).toBe(75);
  });
  test("fixed price and subscriptions count; only internal Apró work counts against", () => {
    expect(
      billablePercent([
        l(3600, "not_billable", "Fast verð"),
        l(1800, "not_billable", "Innifalið í þjónustu"),
        l(1800, "not_billable", "Innri vinna (Apró)"),
      ]),
    ).toBe(75);
  });
  test("line without billing counts in the denominator", () => {
    expect(billablePercent([l(3600, "billable"), l(3600)])).toBe(50);
  });
  test("zero total is null", () => {
    expect(billablePercent([l(0, "billable")])).toBeNull();
  });
});
