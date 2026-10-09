import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { ariaLabel, barcode, fmtHours, fmtInt, insight, isEmpty, PeriodReceipt, receiptLines } from "./PeriodReceipt";
import type { StatsReport } from "@/lib/stats_contract";

afterEach(cleanup);

const report = (o: Partial<StatsReport["totals"]> = {}, unsynced = 0): StatsReport =>
  ({
    from: "2026-09-21",
    to: "2026-10-04",
    totals: { work_seconds: 138600, personal_seconds: 3600, ignored_seconds: 1800, days_worked: 9, prompts: 1200, tool_calls: 20000, helpers: 3, shell_commands: 40, commits: 12, prs: 2, slack_messages: 99, browser_minutes: 120, meeting_seconds: 5400, ...o },
    tools: [{ label: "Bash", value: 11778 }],
    sync: { synced_seconds: 0, unsynced_seconds: unsynced, exported_blocks: 0 },
  }) as unknown as StatsReport;

describe("helpers", () => {
  it("formats", () => {
    expect(fmtInt(11778)).toBe("11,778");
    expect(fmtHours(5400)).toBe("1.5h");
  });
  it("lines include top tool", () => {
    const ls = receiptLines(report());
    expect(ls[0].amount).toBe("38.5h");
    expect(ls.at(-1)).toMatchObject({ item: "Bash", amount: "11,778", qty: "1×" });
    expect(receiptLines({ ...report(), tools: [] }).length).toBe(10);
  });
  it("barcode is deterministic", () => {
    expect(barcode("a→b")).toEqual(barcode("a→b"));
    expect(barcode("a→b")).not.toEqual(barcode("a→c"));
    expect(barcode("x").every((w) => w >= 1 && w <= 3)).toBe(true);
  });
  it("insight and aria", () => {
    expect(insight(report())).toContain("38.5h rung up over 9 days");
    expect(ariaLabel(report())).toContain("Paid");
    expect(ariaLabel(report({}, 7200))).toContain("2.0h unsynced");
    expect(insight(report({ work_seconds: 3600, days_worked: 1 }))).toContain("1 day,");
    expect(insight(report({ work_seconds: 0, days_worked: 0 }))).not.toContain("NaN");
  });
});

describe("PeriodReceipt", () => {
  it("renders PAID", () => {
    const { container } = render(<PeriodReceipt report={report()} />);
    expect(container.textContent).toContain("WORKLOG MARKET");
    expect(container.textContent).toContain("till #9");
    expect(container.textContent).toContain("PAID");
    expect(container.textContent).not.toContain("UNPAID");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("renders UNPAID with hours", () => {
    const { container } = render(<PeriodReceipt report={report({}, 5400)} />);
    expect(container.textContent).toContain("UNPAID");
    expect(container.textContent).toContain("1.5h unsynced to Tempo");
  });
  it("empty state", () => {
    expect(isEmpty(report({ work_seconds: 0, days_worked: 0 }))).toBe(true);
    const { container } = render(<PeriodReceipt report={report({ work_seconds: 0, days_worked: 0 })} />);
    expect(container.textContent).toContain("nothing built yet");
  });
  it("huge values", () => {
    const { container } = render(<PeriodReceipt report={report({ work_seconds: 3.6e9, tool_calls: 1e9 })} />);
    expect(container.textContent).toContain("1,000,000,000");
  });
});
