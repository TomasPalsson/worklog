import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { getTopics } from "@/lib/daemonStats";
import { timeoutMs } from "@/lib/daemonTimeout";
import type { TopicsReport } from "@/lib/topics_contract";
import { TopicsSection } from "./TopicsSection";

afterEach(cleanup);

const DOWN = "Topic stats need the local helper (`worklog verdict`).";
const FEW = "Not enough described work in this range yet.";

const base: TopicsReport = {
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  topics: [
    { id: 0, name: "Fix login", name_ticket: "ABC-1", seconds: 7200, blocks: 4, top_tickets: [] },
    { id: 1, name: "Billing export", name_ticket: null, seconds: 3600, blocks: 2, top_tickets: [] },
  ],
  switches: [{ day: "2026-10-02", switches: 5, work_seconds: 18000, longest_run_seconds: 5400 }],
  drift: [{ topic_id: 0, name: "Fix login", seconds_now: 7200, seconds_before: 1800 }],
  sprawl: [{ ticket: "ABC-9", summary: "Misc chores", blocks: 6, spread: 0.42 }],
  similar_days: [
    { day: "2026-09-20", similarity: 0.87, work_seconds: 14400, top_tickets: [{ label: "ABC-1", value: 3600 }] },
  ],
  embedded_blocks: 6,
  skipped_blocks: 0,
};

const text = (r: TopicsReport) => render(<TopicsSection report={r} />).container.textContent ?? "";

describe("timeoutMs", () => {
  it("topics route gets 90s; plain /stats stays 30s", () => {
    expect(timeoutMs("/stats/topics?from=a&to=b")).toBe(90_000);
    expect(timeoutMs("/stats?from=a")).toBe(30_000);
  });
});

describe("getTopics", () => {
  it("GETs /stats/topics with from/to and omits absent params", async () => {
    const paths: string[] = [];
    // mock.module is process-wide: put back whatever was installed before.
    const prev = { ...(await import("@/lib/daemon")) };
    mock.module("@/lib/daemon", () => ({
      ...prev,
      call: async (_method: string, path: string) => {
        paths.push(path);
        return base;
      },
    }));
    try {
      const r = await getTopics("2026-10-01", "2026-10-07");
      await getTopics();
      expect(r.status).toBe("ready");
    } finally {
      mock.module("@/lib/daemon", () => prev);
    }
    expect(paths).toEqual(["/stats/topics?from=2026-10-01&to=2026-10-07", "/stats/topics"]);
  });
});

describe("TopicsSection", () => {
  it("null report and helper_down show the helper message only", () => {
    for (const report of [null, { ...base, status: "helper_down" as const }]) {
      const { getByText, container } = render(<TopicsSection report={report} />);
      expect(getByText(DOWN)).toBeTruthy();
      expect(container.textContent).not.toContain("Fix login");
      cleanup();
    }
  });

  it("too_few_blocks shows the not-enough message, not the helper one", () => {
    const { getByText, container } = render(<TopicsSection report={{ ...base, status: "too_few_blocks" }} />);
    expect(getByText(FEW)).toBeTruthy();
    expect(container.textContent).not.toContain(DOWN);
  });

  it("ready renders topics with hours", () => {
    const t = text(base);
    expect(t).toContain("Fix login");
    expect(t).toContain("2h");
    expect(t).toContain("Billing export");
    expect(t).toContain("1h");
  });

  it("ready renders switches with longest run", () => {
    const t = text(base);
    expect(t).toContain("2026-10-02");
    expect(t).toContain("5 switches");
    expect(t).toContain("1h 30m");
  });

  it("ready renders drift now vs before", () => {
    const t = text(base);
    expect(t).toContain("now 2h");
    expect(t).toContain("before 30m");
  });

  it("ready renders catch-all tickets with key, summary and spread", () => {
    const t = text(base);
    expect(t).toContain("ABC-9");
    expect(t).toContain("Misc chores");
    expect(t).toContain("0.42");
  });

  it("sprawl summary null falls back to key only, no 'null'", () => {
    const t = text({ ...base, sprawl: [{ ...base.sprawl[0], summary: null }] });
    expect(t).toContain("ABC-9");
    expect(t).not.toContain("null");
  });

  it("ready renders similar days with rounded percent and top tickets", () => {
    const t = text(base);
    expect(t).toContain("2026-09-20");
    expect(t).toContain("87%");
    expect(t).toContain("ABC-1");
  });

  it("ready with empty lists has no NaN/undefined", () => {
    const t = text({ ...base, topics: [], switches: [], drift: [], sprawl: [], similar_days: [] });
    expect(t).not.toContain("NaN");
    expect(t).not.toContain("undefined");
  });
});
