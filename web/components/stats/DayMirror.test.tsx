import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { DayRow, Topic, TopicsReport } from "@/lib/topics_contract";
import { DayMirror, mirrorTip } from "./DayMirror";

afterEach(cleanup);

const topic = (id: number, name: string): Topic => ({
  id,
  name,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds: 0,
  blocks: 0,
  days_active: 0,
  first_day: "2026-10-01",
  last_day: "2026-10-07",
  top_tickets: [],
});

const dayRow = (day: string, topTopic: number, workSeconds: number): DayRow => ({
  day,
  work_seconds: workSeconds,
  focus: null,
  novelty: 0,
  switches: 0,
  longest_run_seconds: 0,
  top_topic: topTopic,
  topics_touched: 1,
});

const report = (similarity: number[][], days: DayRow[]): TopicsReport => ({
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  naming: "off",
  topics: [topic(0, "Billing export"), topic(1, "Tempo sync")],
  points: [],
  bin: "day",
  river: [],
  days,
  day_similarity: similarity,
  first_times: [],
  oddballs: [],
  oddball_median: 0,
  embedded_blocks: 0,
  skipped_blocks: 0,
});

// 4 days listed, the matrix covers only the last 3 (Oct 5, 6, 7).
const days = [dayRow("2026-10-04", 0, 600), dayRow("2026-10-05", 0, 12000), dayRow("2026-10-06", 1, 3600), dayRow("2026-10-07", 1, 5400)];
const similarity = [
  [1, 0.5, -0.25],
  [0.5, 1, 0.1],
  [-0.25, 0.1, 1],
];
const full = report(similarity, days);

describe("mirrorTip", () => {
  it("builds the exact tip for a pair", () => {
    const tip = mirrorTip(full, 0, 2);
    expect(tip.title).toBe("Oct 5 ↔ Oct 7");
    expect(tip.rows).toEqual([
      ["Similarity", "−0.25"],
      ["Oct 5", "Billing export · 3h 20m"],
      ["Oct 7", "Tempo sync · 1h 30m"],
    ]);
    expect(tip.note).toBe("+1 = same kind of work, −1 = opposite");
  });
  it("signs a positive similarity with +", () => {
    expect(mirrorTip(full, 0, 1).rows![0]).toEqual(["Similarity", "+0.50"]);
  });
});

describe("DayMirror", () => {
  it("renders D(D+1)/2 cells over the last D days", () => {
    const { container } = render(<DayMirror report={full} />);
    expect(container.querySelectorAll(".sx-mir-cell").length).toBe(6);
    expect(container.querySelectorAll(".sx-mir-diag").length).toBe(3);
    expect(container.querySelector(".sx-mir-diag")!.textContent).toContain("Oct 5");
  });
  it("positive is amber family, negative slate family, scaled by the largest off-diagonal", () => {
    const { container } = render(<DayMirror report={full} />);
    const cell = (title: string) =>
      [...container.querySelectorAll<HTMLElement>(".sx-mir-cell[data-stip]")].find((el) => JSON.parse(el.getAttribute("data-stip")!).title === title)!;
    const positive = cell("Oct 5 ↔ Oct 6");
    expect(positive.classList.contains("sx-mir-pos")).toBe(true);
    expect(positive.getAttribute("style")).toContain("--p: 100");
    const negative = cell("Oct 5 ↔ Oct 7");
    expect(negative.classList.contains("sx-mir-neg")).toBe(true);
    expect(negative.classList.contains("sx-mir-pos")).toBe(false);
    expect(negative.getAttribute("style")).toContain("--p: 50");
  });
  it("is omitted below 3 days and shown at exactly 3", () => {
    const two = report([[1, 0.5], [0.5, 1]], days);
    expect(render(<DayMirror report={two} />).container.innerHTML).toBe("");
    cleanup();
    expect(render(<DayMirror report={full} />).container.querySelector(".sx-mir")).toBeTruthy();
  });
  it("renders no NaN, undefined or null, even when every pair is 0", () => {
    const flat = report([[1, 0, 0], [0, 1, 0], [0, 0, 1]], days);
    const html = render(<DayMirror report={flat} />).container.innerHTML;
    for (const bad of ["NaN", "undefined", "null"]) expect(html).not.toContain(bad);
  });
  it("shows the How line", () => {
    const { container } = render(<DayMirror report={full} />);
    expect(container.textContent).toContain("How: Cosine similarity of each day's hours-weighted average vector, centered so ordinary days score near 0.");
  });
});
