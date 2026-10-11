import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { parseTip } from "./tip";
import { TopicRiver, buildRiver } from "./TopicRiver";

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
  river: rows.map((seconds, i) => ({ start: `2026-10-0${5 + i}`, seconds })),
});

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
