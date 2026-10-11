import { afterEach, describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, render } from "@testing-library/react";
import type { DayRow, Topic, TopicsReport } from "@/lib/topics_contract";
import { parseTip } from "./tip";
import { heartbeatTip, rhythmOf, TopicHeartbeats } from "./TopicHeartbeats";

afterEach(cleanup);

const topic = (over: Partial<Topic> = {}): Topic => ({
  id: 0,
  name: "Billing export",
  description: "Invoice lines and rounding",
  named_by: "ai",
  keywords: ["invoice", "billing", "rounding"],
  seconds: 7200,
  blocks: 4,
  days_active: 3,
  first_day: "2026-10-05",
  last_day: "2026-10-07",
  top_tickets: [{ label: "ABC-1", value: 3600 }],
  ...over,
});

const day = (d: string, workSeconds: number): DayRow => ({
  day: d,
  work_seconds: workSeconds,
  focus: 0.5,
  novelty: 0.2,
  switches: 1,
  longest_run_seconds: 600,
  top_topic: 0,
  topics_touched: 1,
});

const report = (over: Partial<TopicsReport> = {}): TopicsReport => ({
  status: "ready",
  from: "2026-10-05",
  to: "2026-10-09",
  naming: "ai",
  topics: [topic(), topic({ id: 1, name: "Atlas map", seconds: 7200, days_active: 1, first_day: "2026-10-06", last_day: "2026-10-06" })],
  points: [],
  bin: "day",
  river: [
    { start: "2026-10-05", seconds: [3600, 0] },
    { start: "2026-10-06", seconds: [1800, 7200] },
    { start: "2026-10-07", seconds: [1800, 0] },
  ],
  days: [day("2026-10-05", 3600), day("2026-10-06", 3600), day("2026-10-07", 3600), day("2026-10-08", 3600), day("2026-10-09", 3600)],
  day_similarity: [],
  first_times: [],
  oddballs: [],
  oddball_median: 0.5,
  embedded_blocks: 8,
  skipped_blocks: 0,
  ...over,
});

// A 20-day span keeps the sprint rule out of the always/one_off cases.
const wide = { first_day: "2026-10-01", last_day: "2026-10-21" };
const spanning = (days: number, daysActive: number) =>
  topic({ days_active: daysActive, first_day: "2026-10-01", last_day: `2026-10-${String(1 + days).padStart(2, "0")}` });

describe("rhythmOf", () => {
  it("always: exactly half the work days qualifies (catches > for >=)", () => {
    expect(rhythmOf(topic({ ...wide, days_active: 5 }), 10)).toBe("always");
  });
  it("always: one day short of half does not (catches >= off by one)", () => {
    expect(rhythmOf(topic({ ...wide, days_active: 4 }), 10)).toBeNull();
  });
  it("always: half of an odd count is not rounded down (catches Math.floor)", () => {
    expect(rhythmOf(topic({ ...wide, days_active: 3 }), 5)).toBe("always");
    expect(rhythmOf(topic({ ...wide, days_active: 2 }), 5)).toBeNull();
  });
  it("sprint: a 6-day span with 3 active days qualifies (catches < for <=)", () => {
    expect(rhythmOf(spanning(6, 3), 100)).toBe("sprint");
  });
  it("sprint: a 7-day span does not (catches span <= 7)", () => {
    expect(rhythmOf(spanning(7, 3), 100)).toBeNull();
  });
  it("sprint: 2 active days in a 6-day span does not (catches days_active >= 2)", () => {
    expect(rhythmOf(spanning(6, 2), 100)).toBeNull();
  });
  it("sprint: spans are measured in days across a month end (catches day-of-month subtraction)", () => {
    const t = topic({ days_active: 3, first_day: "2026-09-29", last_day: "2026-10-05" });
    expect(rhythmOf(t, 100)).toBe("sprint");
    expect(rhythmOf({ ...t, last_day: "2026-10-06" }, 100)).toBeNull();
  });
  it("one_off: exactly one active day", () => {
    const t = topic({ days_active: 1, first_day: "2026-10-06", last_day: "2026-10-06" });
    expect(rhythmOf(t, 100)).toBe("one_off");
  });
  it("one_off: two active days is not one-off (catches <= 2)", () => {
    expect(rhythmOf(topic({ ...wide, days_active: 2 }), 100)).toBeNull();
  });
  it("one_off: zero active days is not one-off (catches <= 1)", () => {
    expect(rhythmOf(topic({ ...wide, days_active: 0 }), 100)).toBeNull();
  });
});

describe("heartbeatTip", () => {
  it("exact values: duration, share of all topics, days, blocks, rhythm note", () => {
    const tip = heartbeatTip(topic(), 14400, 5, "always");
    expect(tip.title).toBe("Billing export");
    expect(tip.sub).toBe("Invoice lines and rounding");
    expect(tip.rows).toEqual([
      ["Time", "2h"],
      ["Share of topics", "50%"],
      ["Days active", "3 of 5 work days"],
      ["Blocks", "4"],
    ]);
    expect(tip.bar).toEqual({ value: 7200, max: 14400, label: "50% of topic time" });
    expect(tip.note).toBe("Always on: active on at least half your work days.");
  });
  it("no rhythm means no note; no description means no sub", () => {
    const tip = heartbeatTip(topic({ description: null }), 14400, 5, null);
    expect(tip.note).toBeUndefined();
    expect(tip.sub).toBeUndefined();
  });
  it("zero total seconds does not divide by zero", () => {
    expect(heartbeatTip(topic({ seconds: 0 }), 0, 5, null).rows![1]).toEqual(["Share of topics", "0%"]);
  });
});

describe("TopicHeartbeats", () => {
  it("renders one row per topic", () => {
    const { container } = render(<TopicHeartbeats report={report()} />);
    expect(container.querySelectorAll("[data-heartbeat-row]").length).toBe(2);
  });
  it("rows are keyboard focusable, labelled and carry the tip", () => {
    const { container } = render(<TopicHeartbeats report={report()} />);
    const row = container.querySelector("[data-heartbeat-row]")!;
    expect(row.getAttribute("tabindex")).toBe("0");
    expect(row.getAttribute("aria-label")).toBe("Billing export: 2h");
    const tip = parseTip(row.getAttribute("data-stip"));
    expect(tip!.rows![2]).toEqual(["Days active", "3 of 5 work days"]);
  });
  it("one tick per river bin per topic, heights proportional to hours against the largest bin", () => {
    const { container } = render(<TopicHeartbeats report={report()} />);
    const rows = container.querySelectorAll("[data-heartbeat-row]");
    const heights = (row: Element) => Array.from(row.querySelectorAll("[data-tick]")).map((t) => (t as HTMLElement).style.height);
    expect(heights(rows[1])).toEqual(["0%", "100%", "0%"]);
    expect(heights(rows[0])).toEqual(["50%", "25%", "25%"]);
  });
  it("caps keyword chips at 5", () => {
    const keywords = ["a1", "b2", "c3", "d4", "e5", "f6", "g7"];
    const { container } = render(<TopicHeartbeats report={report({ topics: [topic({ keywords })] })} />);
    expect(container.querySelectorAll(".hb-chip").length).toBe(5);
    expect(container.textContent).not.toContain("f6");
  });
  it("keyword-named topics are marked for muted italic; AI-named are not", () => {
    const { container } = render(
      <TopicHeartbeats report={report({ topics: [topic({ named_by: "keywords", name: "invoice · billing" }), topic({ id: 1 })] })} />,
    );
    const names = container.querySelectorAll(".hb-name");
    expect(names[0].getAttribute("data-keywords")).toBe("true");
    expect(names[1].hasAttribute("data-keywords")).toBe(false);
  });
  it("badge shows the rhythm text; none when no rhythm applies", () => {
    const topics = [topic({ days_active: 5, ...wide }), topic({ id: 1, days_active: 4, ...wide })];
    const days = Array.from({ length: 10 }, (_, i) => day(`2026-10-${String(i + 1).padStart(2, "0")}`, 3600));
    const { container } = render(<TopicHeartbeats report={report({ topics, days })} />);
    const rows = container.querySelectorAll("[data-heartbeat-row]");
    expect(rows[0].querySelector(".hb-badge")!.textContent).toBe("Always on");
    expect(rows[1].querySelector(".hb-badge")).toBeNull();
  });
  it("work days count only days with work", () => {
    const days = [day("2026-10-05", 3600), day("2026-10-06", 0)];
    const { container } = render(<TopicHeartbeats report={report({ days, topics: [topic({ days_active: 1, ...wide })] })} />);
    expect(container.querySelector(".hb-badge")!.textContent).toBe("Always on");
  });
  it("ticket keys appear without hours", () => {
    const { container } = render(<TopicHeartbeats report={report()} />);
    expect(container.querySelector(".hb-tickets")!.textContent).toBe("ABC-1");
  });
  it("shows the How line and no NaN, undefined or null", () => {
    const { container } = render(<TopicHeartbeats report={report({ topics: [topic({ description: null, top_tickets: [] })] })} />);
    expect(container.textContent).toContain("How: Ticks show hours per day in this topic.");
    expect(container.innerHTML).not.toMatch(/NaN|undefined|null/);
  });
  it("every row is the same three-column template, whatever the description length", () => {
    const long = "A very long description that goes on and on about invoice lines, rounding and everything else. ".repeat(4);
    const topics = [topic({ description: long }), topic({ id: 1, description: "Short" }), topic({ id: 2, description: null, top_tickets: [] })];
    const { container } = render(<TopicHeartbeats report={report({ topics })} />);
    const rows = Array.from(container.querySelectorAll("[data-heartbeat-row]"));
    expect(rows.length).toBe(3);
    for (const row of rows) {
      // catches a row whose optional children become extra grid cells
      expect(Array.from(row.children).map((c) => c.className)).toEqual(["hb-id", "hb-about", "hb-data"]);
      // catches per-row inline width overrides
      expect((row as HTMLElement).getAttribute("style")).not.toMatch(/width|grid-template/);
      expect(row.querySelector(".hb-data .hb-raster")).not.toBeNull();
      expect(row.querySelector(".hb-data .hb-track")).not.toBeNull();
    }
  });
  it("raster, bar and tickets sit in the third column, badge and name in the first", () => {
    const { container } = render(<TopicHeartbeats report={report()} />);
    const row = container.querySelector("[data-heartbeat-row]")!;
    expect(row.querySelector(".hb-id .hb-name")).not.toBeNull();
    expect(row.querySelector(".hb-id .hb-badge")).not.toBeNull();
    expect(row.querySelector(".hb-data .hb-tickets")).not.toBeNull();
  });
  it("css fixes the row template at desktop width and stacks below 800px", () => {
    const css = readFileSync(new URL("../../app/stats/heartbeats.css", import.meta.url), "utf8");
    expect(css).toContain("grid-template-columns: minmax(150px, 220px) minmax(0, 1fr) 360px;");
    expect(css).toMatch(/@media \(max-width: 799px\)\s*{\s*\.hb-row { grid-template-columns: minmax\(0, 1fr\);/);
  });
});
