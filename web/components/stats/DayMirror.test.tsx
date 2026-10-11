import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { DayRow, Topic, TopicsReport } from "@/lib/topics_contract";
import { diagTip, DayMirror, mirrorTip } from "./DayMirror";

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
  it("shows the plain How line, keeps jargon in a Method details, and caps the line length", () => {
    const { container } = render(<DayMirror report={full} />);
    const how = container.querySelector(".sx-mir-how")!;
    expect(how.textContent).toContain("How it works:");
    expect(how.textContent).not.toContain("How:");
    // catches: jargon left in the visible caption
    const visible = how.cloneNode(true) as HTMLElement;
    visible.querySelector("details")!.remove();
    expect(visible.textContent!.toLowerCase()).not.toContain("cosine");
    const details = how.querySelector("details")!;
    expect(details.querySelector("summary")!.textContent).toBe("Method");
    expect(details.textContent).toContain("Cosine similarity of each day's hours-weighted average vector");
  });
  it("uses the shared atlas card and small-caps title", () => {
    const { container } = render(<DayMirror report={full} />);
    expect(container.querySelector("section.sr-card.sx-mir")).toBeTruthy();
    expect(container.querySelector("h3.sr-title")!.textContent).toBe("Day mirror");
  });
  it("shows a legend with both swatches and a 3-step strip", () => {
    const { container } = render(<DayMirror report={full} />);
    const legend = container.querySelector(".sx-mir-legend")!;
    expect(legend.textContent).toContain("alike");
    expect(legend.textContent).toContain("opposite");
    expect(legend.querySelector(".sx-mir-swatch.sx-mir-pos")).toBeTruthy();
    expect(legend.querySelector(".sx-mir-swatch.sx-mir-neg")).toBeTruthy();
    expect(legend.querySelectorAll(".sx-mir-step").length).toBe(3);
    // catches: legend swatches counted as matrix cells
    expect(container.querySelectorAll(".sx-mir-cell").length).toBe(6);
  });
  it("names the most alike and most opposite pair from the off-diagonal only", () => {
    // catches: taking the diagonal 1 as max, or the largest |value| as the most alike
    const four = report(
      [[1, 0.2, -0.1, 0.6], [0.2, 1, -0.7, 0.1], [-0.1, -0.7, 1, 0.3], [0.6, 0.1, 0.3, 1]],
      days,
    );
    const callout = render(<DayMirror report={four} />).container.querySelector(".sx-mir-callout")!;
    expect(callout.textContent).toBe("Most alike days: Oct 4 & Oct 7 (+0.60) · Most opposite: Oct 5 & Oct 6 (−0.70)");
  });
  it("uses row-header labels, not diagonal labels, once a cell is under 44px", () => {
    // 11 days -> cell 43.3px; 10 days -> 47.2px
    // catches: > for >= / the old 22px threshold
    const some = (n: number) => render(<DayMirror report={wide(n)} />).container;
    expect(some(11).querySelectorAll(".sx-mir-diag .art-label").length).toBe(0);
    cleanup();
    expect(some(10).querySelectorAll(".sx-mir-diag .art-label").length).toBe(10);
  });
});

// 30 days, similarity decays with distance so every pair is distinct and nonzero.
const wide = (count: number) => {
  const rows = Array.from({ length: count }, (_, i) => dayRow(`2026-09-${String(i + 1).padStart(2, "0")}`, i % 2, 3600));
  const matrix = rows.map((_, r) => rows.map((__, c) => (r === c ? 1 : 0.9 - Math.abs(r - c) * 0.05 + (r + c) * 0.001)));
  return report(matrix, rows);
};

describe("diagTip", () => {
  it("names the most and least similar other day and the top topic", () => {
    const tip = diagTip(full, 0);
    expect(tip.title).toBe("Oct 5");
    // catches: including the self-similarity 1 as "most like"
    expect(tip.note).toBe("Most like: Oct 6 (+0.50) · Least like: Oct 7 (−0.25) · Top topic: Billing export");
  });
  it("reads the column for a later day (symmetric matrix)", () => {
    // catches: reading only the row to the right of the diagonal
    expect(diagTip(full, 2).note).toBe("Most like: Oct 6 (+0.10) · Least like: Oct 5 (−0.25) · Top topic: Tempo sync");
  });
});

describe("DayMirror at 30 days", () => {
  const html = () => render(<DayMirror report={wide(30)} />).container;
  it("has exactly one tab stop per day", () => {
    // catches: tabIndex on every similarity cell (465 stops)
    expect(html().querySelectorAll("[tabindex]").length).toBe(30);
  });
  it("similarity cells carry no tabindex or aria-label but keep data-stip", () => {
    const cells = [...html().querySelectorAll(".sx-mir-cell:not(.sx-mir-diag)")];
    expect(cells.length).toBe(435);
    expect(cells.some((c) => c.hasAttribute("tabindex") || c.hasAttribute("aria-label"))).toBe(false);
    expect(cells.every((c) => c.hasAttribute("data-stip"))).toBe(true);
  });
  it("thins labels to every k-th day, keeps first and last, and puts them outside the matrix", () => {
    const c = html();
    const labels = [...c.querySelectorAll(".sx-mir-rowlabel")].map((l) => l.textContent);
    const k = Number(c.querySelector(".sx-mir-grid")!.getAttribute("data-k"));
    expect(k).toBeGreaterThan(1);
    expect(labels.length).toBeLessThanOrEqual(Math.ceil(30 / k) + 1);
    expect(labels[0]).toBe("Sep 1");
    expect(labels[labels.length - 1]).toBe("Sep 30");
    // catches: labels left on the diagonal where the cell is too small
    expect(c.querySelectorAll(".sx-mir-diag .art-label").length).toBe(0);
  });
  it("labels every day on the diagonal when cells are large", () => {
    const c = render(<DayMirror report={full} />).container;
    expect(c.querySelectorAll(".sx-mir-rowlabel").length).toBe(0);
    expect(c.querySelectorAll(".sx-mir-diag .art-label").length).toBe(3);
  });
  it("diagonal chip has an aria-label with the tip text", () => {
    const chip = html().querySelector<HTMLElement>(".sx-mir-diag")!;
    expect(chip.getAttribute("aria-label")).toContain("Most like:");
  });
});
