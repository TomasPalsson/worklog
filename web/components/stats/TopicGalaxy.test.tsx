import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { galaxyLayout, galaxyTipDot, galaxyTipLabel, TopicGalaxy } from "./TopicGalaxy";

afterEach(cleanup);

const topic = (id: number, over: Partial<Topic> = {}): Topic => ({
  id,
  name: `topic ${id}`,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds: 3600,
  blocks: 1,
  days_active: 1,
  first_day: "2026-10-01",
  last_day: "2026-10-01",
  top_tickets: [],
  ...over,
});

const point = (block_id: number, over: Partial<MapPoint> = {}): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  topic_id: 0,
  day: "2026-10-01",
  seconds: 3600,
  label: `block ${block_id}`,
  ticket: null,
  novelty: 0.5,
  oddball: false,
  ...over,
});

const report = (over: Partial<TopicsReport> = {}): TopicsReport => ({
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-31",
  naming: "ai",
  topics: [topic(0)],
  points: [point(1)],
  bin: "day",
  river: [],
  days: [],
  day_similarity: [],
  first_times: [],
  oddballs: [],
  oddball_median: 0,
  embedded_blocks: 1,
  skipped_blocks: 0,
  ...over,
});

const overlaps = (a: { x: number; y: number; w: number; h: number }, b: typeof a) =>
  Math.abs(a.x - b.x) < (a.w + b.w) / 2 && Math.abs(a.y - b.y) < (a.h + b.h) / 2;

describe("galaxyLayout dots", () => {
  const lay = galaxyLayout(
    report({
      points: [
        point(1, { x: 0, y: 0, seconds: 400 }),
        point(2, { x: 1, y: 1, seconds: 100 }),
        point(3, { x: 0.5, y: 0.5, seconds: 0 }),
      ],
    }),
  );
  const dot = (id: number) => lay.dots.find((d) => d.point.block_id === id)!;

  it("maps x/y in [0,1] to the padded viewBox", () => {
    expect([dot(1).cx, dot(1).cy]).toEqual([40, 40]);
    expect([dot(2).cx, dot(2).cy]).toEqual([960, 580]);
  });

  it("radius is 2.5 + 6*sqrt(seconds/max): max gives 8.5, quarter gives 5.5, zero gives 2.5", () => {
    expect(dot(1).r).toBeCloseTo(8.5, 10);
    expect(dot(2).r).toBeCloseTo(5.5, 10);
    expect(dot(3).r).toBeCloseTo(2.5, 10);
  });

  it("rank is chronological by day then block id; delay is min(rank*3, 1200) ms", () => {
    const l = galaxyLayout(
      report({ points: [point(5, { day: "2026-10-02" }), point(9, { day: "2026-10-01" }), point(4, { day: "2026-10-02" })] }),
    );
    const rank = (id: number) => l.dots.find((d) => d.point.block_id === id)!.rank;
    expect([rank(9), rank(4), rank(5)]).toEqual([0, 1, 2]);
  });

  it("delay caps at 1200ms exactly at rank 400 and stays there past it", () => {
    const pts = Array.from({ length: 402 }, (_, i) => point(i, { day: "2026-10-01" }));
    const l = galaxyLayout(report({ points: pts }));
    const delay = (id: number) => l.dots.find((d) => d.point.block_id === id)!.delayMs;
    expect([delay(399), delay(400), delay(401)]).toEqual([1197, 1200, 1200]);
  });

  it("drops points whose topic is unknown instead of drawing NaN", () => {
    const l = galaxyLayout(report({ points: [point(1), point(2, { topic_id: 99 })] }));
    expect(l.dots.map((d) => d.point.block_id)).toEqual([1]);
  });
});

describe("galaxyLayout labels", () => {
  it("font size is 11 + 9*sqrt(share): whole share 20, quarter share 15.5", () => {
    const whole = galaxyLayout(report());
    expect(whole.labels[0].fontSize).toBeCloseTo(20, 10);
    const quarter = galaxyLayout(
      report({
        topics: [topic(0, { seconds: 1000 }), topic(1, { seconds: 3000 })],
        points: [point(1, { topic_id: 0 }), point(2, { topic_id: 1, x: 0.1, y: 0.9 })],
      }),
    );
    expect(quarter.labels.find((l) => l.topic.id === 0)!.fontSize).toBeCloseTo(15.5, 10);
  });

  it("sits at the hours-weighted median, not the mean or first member", () => {
    const l = galaxyLayout(
      report({
        points: [
          point(1, { x: 0, seconds: 3600 }),
          point(2, { x: 0.5, seconds: 3600 }),
          point(3, { x: 1, seconds: 100000 }),
        ],
      }),
    );
    expect(l.labels[0].x).toBe(960);
  });

  it("a topic with no points gets no label", () => {
    const l = galaxyLayout(report({ topics: [topic(0), topic(1)], points: [point(1)] }));
    expect(l.labels.map((x) => x.topic.id)).toEqual([0]);
  });

  it("labels do not overlap after nudging a crowded fixture", () => {
    const topics = Array.from({ length: 6 }, (_, i) => topic(i, { name: `crowded theme ${i}`, seconds: 3600 * (i + 1) }));
    const points = topics.map((t, i) => point(i + 1, { topic_id: t.id, x: 0.5, y: 0.5 + i * 0.01 }));
    const { labels } = galaxyLayout(report({ topics, points }));
    expect(labels).toHaveLength(6);
    for (let i = 0; i < labels.length; i++)
      for (let j = i + 1; j < labels.length; j++) expect(overlaps(labels[i], labels[j])).toBe(false);
    for (const l of labels) expect(l.y - l.h / 2).toBeGreaterThanOrEqual(0);
  });

  it("labels far apart horizontally are left where they are", () => {
    const { labels } = galaxyLayout(
      report({
        topics: [topic(0), topic(1)],
        points: [point(1, { x: 0.1, y: 0.5 }), point(2, { topic_id: 1, x: 0.9, y: 0.5 })],
      }),
    );
    expect(labels[0].y).toBe(labels[1].y);
  });
});

describe("tips", () => {
  it("dot tip has exact title and rows", () => {
    const p = point(1, { label: "Fix login", day: "2026-10-03", seconds: 5400, ticket: "ABC-1", novelty: 0.426 });
    expect(galaxyTipDot(p, "auth")).toEqual({
      title: "Fix login",
      rows: [
        ["Day", "2026-10-03"],
        ["Hours", "1h 30m"],
        ["Ticket", "ABC-1"],
        ["Topic", "auth"],
        ["Novelty", "43%"],
      ],
    });
  });

  it("dot tip says none for a missing ticket", () => {
    expect(galaxyTipDot(point(1), "t").rows!.find((r) => r[0] === "Ticket")![1]).toBe("none");
  });

  it("label tip carries description, keywords, hours and blocks", () => {
    const t = topic(0, { name: "Auth", description: "Login work", keywords: ["login", "token"], seconds: 9000, blocks: 4 });
    expect(galaxyTipLabel(t)).toEqual({
      title: "Auth",
      sub: "Login work",
      rows: [
        ["Keywords", "login, token"],
        ["Hours", "2.5h"],
        ["Blocks", "4"],
      ],
    });
  });

  it("label tip omits sub without a description", () => {
    expect("sub" in galaxyTipLabel(topic(0))).toBe(false);
  });
});

describe("TopicGalaxy markup", () => {
  const full = report({
    topics: [topic(0), topic(1)],
    points: [point(1), point(2, { topic_id: 1, x: 0.2, y: 0.2, oddball: true }), point(3, { x: 0.8, y: 0.3 })],
    first_times: [{ block_id: 3, day: "2026-10-01", label: "x", ticket: null, topic_id: 0, seconds: 3600, score: 1 }],
  });

  it("every dot has tabIndex 0 and an aria-label", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const dots = container.querySelectorAll(".sx-gal-dot");
    expect(dots).toHaveLength(3);
    for (const d of dots) {
      expect(d.getAttribute("tabindex")).toBe("0");
      expect(d.getAttribute("aria-label")).toBeTruthy();
      expect(d.getAttribute("data-stip")).toBeTruthy();
    }
  });

  it("every label is focusable with a tip", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const labels = container.querySelectorAll(".sx-gal-label");
    expect(labels).toHaveLength(2);
    for (const l of labels) {
      expect(l.getAttribute("tabindex")).toBe("0");
      expect(l.getAttribute("aria-label")).toBeTruthy();
    }
  });

  it("markup never contains NaN, undefined or null", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    expect(container.innerHTML).not.toMatch(/NaN|undefined|null/);
  });

  it("draws a dashed ring at r+5 only for oddballs", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const rings = container.querySelectorAll(".sx-gal-ring");
    expect(rings).toHaveLength(1);
    const dot = full.points.find((p) => p.oddball)!;
    const d = galaxyLayout(full).dots.find((x) => x.point.block_id === dot.block_id)!;
    expect(Number(rings[0].getAttribute("r"))).toBeCloseTo(d.r + 5, 6);
  });

  it("pending chip shows only when naming is pending", () => {
    const pending = render(<TopicGalaxy report={{ ...full, naming: "pending" }} />);
    expect(pending.container.textContent).toContain("AI is naming topics…");
    pending.unmount();
    for (const naming of ["ai", "off"] as const) {
      const r = render(<TopicGalaxy report={{ ...full, naming }} />);
      expect(r.container.textContent).not.toContain("AI is naming topics…");
      r.unmount();
    }
  });

  it("emits one hover isolation rule per topic", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const css = container.querySelector("style")!.textContent!;
    expect(css).toContain('.sx-gal:has([data-t="0"]:hover)');
    expect(css).toContain('.sx-gal:has([data-t="1"]:hover)');
    expect(css.match(/opacity:\.12/g)).toHaveLength(2);
  });

  it("shows the How line", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    expect(container.textContent).toContain("How: Each dot is a work block.");
  });

  it("no points shows the empty state and message, with no svg map", () => {
    const { container } = render(<TopicGalaxy report={report({ points: [] })} />);
    expect(container.textContent).toContain("Map needs at least 10 described blocks.");
    expect(container.querySelector(".sx-gal-dot")).toBeNull();
    expect(container.querySelector('svg[role="img"]')).not.toBeNull();
  });

  it("contains no hard-coded hex colours", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    expect(container.innerHTML).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
  });
});
