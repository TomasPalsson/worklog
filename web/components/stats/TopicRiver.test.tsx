import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { parseTip } from "./tip";
import { TopicRiver, buildRiver, labelBox } from "./TopicRiver";

afterEach(cleanup);

const topic = (id: number, name: string): Topic => ({
  id, name, description: null, named_by: "keywords", keywords: [], seconds: 0, blocks: 1,
  days_active: 1, first_day: "2026-10-05", last_day: "2026-10-09", top_tickets: [],
});
const H = 3600;
const report = (rows: number[][], bin: "day" | "week" = "day"): TopicsReport => ({
  status: "ready", from: "2026-10-05", to: "2026-10-09", naming: "off",
  topics: (rows[0] ?? []).map((_, i) => topic(i, `T${i}`)),
  points: [], bin, days: [], day_similarity: [], first_times: [], oddballs: [],
  oddball_median: 0, embedded_blocks: 0, skipped_blocks: 0,
  river: rows.map((seconds, i) => ({ start: new Date(Date.UTC(2026, 9, 5 + i)).toISOString().slice(0, 10), seconds })),
});
// 12 bins, 4 topics with 20-char names peaking in adjacent bins: boxes (~130px) outrun the ~90px bin spacing
const crowded = () => {
  const rows = Array.from({ length: 12 }, (_, b) => Array.from<unknown, number>({ length: 4 }, (_, t) => (b === 4 + t ? H : 0)));
  rows[0][0] = 1; // keeps all 12 bins after trimming
  rows[11][0] = 1;
  const r = report(rows);
  r.topics = r.topics.map((t) => ({ ...t, name: `${"N".repeat(19)}${t.id}` }));
  return r;
};

describe("buildRiver", () => {
  // catches: heights normalised per layer instead of one shared scale
  it("band thickness per bin equals bin seconds times one shared scale", () => {
    const rows = [[2 * H, 1 * H], [4 * H, 0], [0, 3 * H]];
    const { layers, unit } = buildRiver(report(rows))!;
    expect(layers.length).toBe(2);
    expect(unit).toBeGreaterThan(0);
    layers.forEach((l) => l.upper.forEach((u, i) => {
      expect(l.lower[i][1] - u[1]).toBeCloseTo(unit * rows[i][l.topic.id], 6);
    }));
  });
  // catches: running total from a fixed zero baseline instead of a centred one
  it("is centred: each bin's stack is symmetric about the midline", () => {
    const { layers } = buildRiver(report([[2 * H, 1 * H], [4 * H, 0], [0, 3 * H]]))!;
    for (let i = 0; i < 3; i++) {
      const top = Math.min(...layers.map((l) => l.upper[i][1]));
      const bot = Math.max(...layers.map((l) => l.lower[i][1]));
      expect((top + bot) / 2).toBeCloseTo(120, 6);
    }
  });
  // catches: ordered by topic id or size instead of first active bin
  it("orders layers by first active bin, ties by id", () => {
    const { layers } = buildRiver(report([[0, 0, H], [0, H, H], [H, H, H]]))!;
    expect(layers.map((l) => l.topic.id)).toEqual([2, 1, 0]);
    const tie = buildRiver(report([[H, H], [H, H]]))!;
    expect(tie.layers.map((l) => l.topic.id)).toEqual([0, 1]);
  });
  // catches: never-active topics rendered as empty bands
  it("drops topics with zero total", () => {
    const { layers } = buildRiver(report([[H, 0], [H, 0]]))!;
    expect(layers.map((l) => l.topic.id)).toEqual([0]);
  });
  // catches: dividing by a zero bin total
  it("has no NaN with an all-zero bin", () => {
    const { layers } = buildRiver(report([[H, H], [0, 0], [H, H]]))!;
    expect(layers.length).toBe(2);
    expect(layers.every((l) => !l.path.includes("NaN"))).toBe(true);
  });
  // catches: scale = H/0 when nothing was active
  it("returns null when every bin is zero", () => {
    expect(buildRiver(report([[0], [0]]))).toBeNull();
  });
  // catches: < 2 written as <= 2, and the empty river crashing
  it("returns null for fewer than 2 bins, builds at exactly 2", () => {
    expect(buildRiver(report([[H, H]]))).toBeNull();
    expect(buildRiver(report([]))).toBeNull();
    expect(buildRiver(report([[H], [H]]))).not.toBeNull();
  });
  // catches: > 14 instead of >= 14, and thickness taken at the wrong bin
  it("labels a band exactly at 14px and not just under", () => {
    // total 456s -> unit 0.5 px/s (exact in binary): 28s is exactly 14px
    const at = buildRiver(report([[428, 28], [428, 28]]))!;
    const l = at.layers.find((x) => x.topic.id === 1)!;
    expect(l.thick).toBe(14);
    expect(l.labelled).toBe(true);
    const under = buildRiver(report([[429, 27], [429, 27]]))!;
    expect(under.layers.find((x) => x.topic.id === 1)!.labelled).toBe(false);
  });
  // catches: using the first bin or the widest of a different layer
  it("measures thickness at the layer's own widest bin", () => {
    const { layers, unit } = buildRiver(report([[H, 0], [3 * H, 0], [H, 0]]))!;
    expect(layers[0].thick).toBeCloseTo(3 * H * unit, 6);
    expect(layers[0].labelX).toBeCloseTo(500, 6);
  });
});

describe("river trimming and labels", () => {
  // catches: trimming nothing, or trimming only one end
  it("trims leading and trailing zero bins", () => {
    const { bins, xs, layers } = buildRiver(report([[0], [0], [H], [2 * H], [0]]))!;
    expect(bins.map((b) => b.start)).toEqual(["2026-10-07", "2026-10-08"]);
    expect(xs[0]).toBe(6);
    expect(xs[1]).toBe(994);
    expect(layers[0].peakStart).toBe("2026-10-08");
  });
  // catches: trimming interior zero bins too
  it("keeps interior zero bins", () => {
    const { bins } = buildRiver(report([[0], [H], [0], [H], [0]]))!;
    expect(bins.length).toBe(3);
  });
  // catches: trimming before the < 2 check being skipped (one data bin left)
  it("returns null when only one bin has data", () => {
    expect(buildRiver(report([[0], [H], [0]]))).toBeNull();
  });
  // catches: axis labels still reading the untrimmed river
  it("axis labels show the first and last non-zero bin", () => {
    const { container } = render(<TopicRiver report={report([[0], [0], [H], [H], [0]])} />);
    const labels = [...container.querySelectorAll("text.art-label")].map((t) => t.textContent);
    expect(labels).toEqual(["7 Oct", "8 Oct"]);
    expect(container.querySelectorAll("rect.sx-river-col").length).toBe(2);
  });
  // catches: labels placed without checking earlier boxes (or without the pad)
  it("placed label boxes are pairwise disjoint with a 4px pad, and some are dropped", () => {
    const { layers } = buildRiver(crowded())!;
    const placed = layers.filter((l) => l.labelled).map(labelBox);
    expect(placed.length).toBeGreaterThan(0);
    expect(placed.length).toBeLessThan(layers.length);
    for (let i = 0; i < placed.length; i++) for (let j = i + 1; j < placed.length; j++) {
      const a = placed[i], b = placed[j];
      expect(a.x + a.w + 4 <= b.x || b.x + b.w + 4 <= a.x || a.y + a.h + 4 <= b.y || b.y + b.h + 4 <= a.y).toBe(true);
    }
  });
  // catches: first-come placement instead of largest topic first
  it("the largest topic keeps its label", () => {
    const r = crowded();
    r.river[8].seconds[3] = 3 * H; // T3 now clearly largest
    const { layers } = buildRiver(r)!;
    expect(layers.find((l) => l.topic.id === 3)!.labelled).toBe(true);
  });
  // catches: label wider than the viewBox being clamped and drawn anyway
  it("skips a label wider than the viewBox and keeps one that fits", () => {
    const r = report([[H, H], [H, H]]);
    r.topics[0].name = "W".repeat(200);
    const { layers } = buildRiver(r)!;
    expect(layers[0].labelled).toBe(false);
    expect(layers[1].labelled).toBe(true);
    const b = labelBox(layers[1]);
    expect(b.x).toBeGreaterThanOrEqual(0);
    expect(b.x + b.w).toBeLessThanOrEqual(1000);
  });
  // catches: unlabelled topics vanishing from the chart
  it("every topic appears inline or in the legend", () => {
    const rep = crowded();
    const { container } = render(<TopicRiver report={rep} />);
    const inline = [...container.querySelectorAll("text.sx-river-name")].map((t) => t.textContent);
    const legend = [...container.querySelectorAll(".sx-river-legend-item")].map((t) => t.textContent);
    expect(inline.length).toBeGreaterThan(0);
    expect(legend.length).toBeGreaterThan(0);
    expect([...inline, ...legend].sort()).toEqual(rep.topics.map((t) => t.name).sort());
  });
  it("renders no legend when every topic is labelled inline", () => {
    const { container } = render(<TopicRiver report={report([[H], [H]])} />);
    expect(container.querySelector(".sx-river-legend")).toBeNull();
  });
});

describe("TopicRiver", () => {
  const rep = report([[2 * H, 1 * H], [4 * H, 0], [0, 3 * H]]);
  it("renders one band per layer with a band tip", () => {
    const { container } = render(<TopicRiver report={rep} />);
    const bands = container.querySelectorAll("path.sx-river-band");
    expect(bands.length).toBe(2);
    expect(bands[0].getAttribute("class")).toContain("art-grow-y");
    const tip = parseTip(bands[0].getAttribute("data-stip"))!;
    expect(tip.title).toBe("T0");
    const rows = Object.fromEntries(tip.rows!);
    expect(rows["Total"]).toBe("6.0h");
    expect(rows["Peak bin"]).toBe("4.0h on 6 Oct");
    expect(rows["Active bins"]).toBe("2 of 3");
  });
  // catches: listing every topic, or unsorted
  it("column tip shows the bin's top 4 topics by hours", () => {
    const many = report([[1 * H, 5 * H, 3 * H, 2 * H, 4 * H, 0], [H, H, H, H, H, H]]);
    const { container } = render(<TopicRiver report={many} />);
    const cols = container.querySelectorAll("rect.sx-river-col");
    expect(cols.length).toBe(2);
    const tip = parseTip(cols[0].getAttribute("data-stip"))!;
    expect(tip.title).toBe("Mon 5 Oct");
    expect(tip.rows!.map((r) => r[0])).toEqual(["T1", "T4", "T2", "T3"]);
    expect(tip.rows![0][1]).toBe("5.0h");
  });
  // catches: column rects painted after the bands, swallowing every pointer
  it("paints the column rects before the bands so bands receive the pointer", () => {
    const { container } = render(<TopicRiver report={rep} />);
    const all = [...container.querySelectorAll("rect.sx-river-col, path.sx-river-band")];
    const lastCol = all.map((e) => e.tagName).lastIndexOf("rect");
    const firstBand = all.findIndex((e) => e.tagName === "path");
    expect(lastCol).toBeLessThan(firstBand);
    expect(container.querySelectorAll("path.sx-river-band[data-stip]").length).toBe(2);
  });
  it("shows axis labels in .art-label and the How line", () => {
    const { container, getByText } = render(<TopicRiver report={rep} />);
    const labels = [...container.querySelectorAll("text.art-label")].map((t) => t.textContent);
    expect(labels).toContain("5 Oct");
    expect(labels).toContain("7 Oct");
    expect(getByText(/^How:/).textContent).toContain("per week when the range is longer than 90 days");
  });
  it("renders nothing for fewer than 2 bins", () => {
    const { container } = render(<TopicRiver report={report([[H, H]])} />);
    expect(container.innerHTML).toBe("");
  });
});
