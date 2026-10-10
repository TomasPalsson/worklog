import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { ariaLabel, barcode, barcodeTip, receiptTip, stampTip, totalsTip, fmtHours, fmtInt, insight, isEmpty, PeriodReceipt, receiptLines } from "./PeriodReceipt";
import type { StatsReport } from "@/lib/stats_contract";

afterEach(cleanup);

const report = (o: Partial<StatsReport["totals"]> = {}, unsynced = 0): StatsReport =>
  ({
    from: "2026-09-21",
    to: "2026-10-04",
    totals: { work_seconds: 138600, personal_seconds: 3600, ignored_seconds: 1800, days_worked: 9, prompts: 1200, tool_calls: 20000, helpers: 3, shell_commands: 40, commits: 12, prs: 2, slack_messages: 99, browser_minutes: 120, meeting_seconds: 5400, ...o },
    tools: [{ label: "Bash", value: 11778 }],
    daily: [
      { day: "2026-09-22", work_seconds: 3600, prompts: 10, tool_calls: 500, shell: 1, slack: 0, commits: 1, meeting_seconds: 0, browser_minutes: 5 },
      { day: "2026-09-23", work_seconds: 7200, prompts: 30, tool_calls: 900, shell: 2, slack: 4, commits: 0, meeting_seconds: 1800, browser_minutes: 15 },
    ],
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

describe("tips", () => {
  const r = report({}, 5400);
  const line = (item: string) => receiptLines(r).find((l) => l.item === item)!;
  it("receipt line: exact rows, best day, fun note", () => {
    const tip = receiptTip(r, line("Tool calls"));
    expect(tip.title).toBe("Tool calls");
    expect(tip.rows).toEqual([
      ["Total", "20,000"],
      ["Per worked day", "2,222"],
      ["Per calendar day", "1,429"],
      ["Best day", "Wed 23 Sep: 900"],
      ["Counts", "actions Claude took on your behalf (edits, reads, commands)"],
    ]);
    expect(tip.note).toBe("≈ one tool call every 6.9 seconds you were at work");
    expect(receiptTip(r, line("Claude prompts")).note).toBe("≈ one prompt every 1.9 minutes you were at work");
  });
  it("time lines show durations; browser scales minutes", () => {
    expect(receiptTip(r, line("Hours worked")).rows![0]).toEqual(["Total", "38h 30m"]);
    expect(receiptTip(r, line("Hours worked")).note).toBe("≈ 4.8 full 8-hour days");
    expect(receiptTip(r, line("Browser")).rows).toContainEqual(["Best day", "Wed 23 Sep: 15m"]);
    expect(receiptTip(r, line("Meetings")).note).toBe("4% of your work time was spent in meetings");
    expect(receiptTip(r, line("Bash")).note).toBe("59% of all tool calls");
    expect(receiptTip(r, line("Bash")).rows!.some(([k]) => k === "Best day")).toBe(false);
  });
  it("zero and missing daily do not break", () => {
    const z = report({ work_seconds: 0, days_worked: 0, tool_calls: 0 });
    const tip = receiptTip({ ...z, daily: undefined } as unknown as StatsReport, receiptLines(z)[2]);
    expect(tip.note).toBeUndefined();
    expect(tip.rows!.map(([k]) => k)).toEqual(["Total", "Per calendar day", "Counts"]);
    expect(JSON.stringify(tip)).not.toContain("NaN");
  });
  it("tax/discount note is relative to work time", () => {
    const x = report({ work_seconds: 36000, personal_seconds: 36000, ignored_seconds: 0 });
    expect(totalsTip(x, "discount").note).toBe("100% on top of 10h of work");
  });
  it("totals, stamp, barcode", () => {
    expect(totalsTip(r, "tax").rows![0]).toEqual(["Exact", "0.50 hours"]);
    expect(totalsTip(r, "tax").note).toBe("1% on top of 38h 30m of work");
    expect(totalsTip(r, "total").note).toBe("96% of the time worklog saw was work");
    expect(totalsTip(report({ personal_seconds: 0 }), "discount").note).toBe("None at all this period.");
    const s = stampTip(r);
    expect(s.title).toBe("UNPAID: hours waiting for Tempo");
    expect(s.note).toBe("1h 30m still to sync before this is fully paid");
    expect(stampTip(report()).title).toBe("PAID: everything is synced");
    expect(barcodeTip(r).sub).toBe("2026-09-21 → 2026-10-04");
    expect(barcodeTip(r).note).toContain("Not scannable :)");
  });
  it("renders data-stip on lines, stamp, barcode and no title", () => {
    const { container } = render(<PeriodReceipt report={r} />);
    expect(container.querySelector("title")).toBeNull();
    expect(container.querySelectorAll("[title]").length).toBe(0);
    expect(container.querySelectorAll(".sx-receipt-line[data-stip]").length).toBe(15);
    expect(container.querySelector(".sx-receipt-stamp")!.getAttribute("data-stip")).toContain("UNPAID");
    expect(container.querySelector(".sx-receipt-bars")!.getAttribute("data-stip")).toContain("barcode");
  });
});
