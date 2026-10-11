import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { galaxyLayout, galaxyTipDot, galaxyTipLabel, TopicGalaxy, wrapName } from "./TopicGalaxy";

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

describe("wrapName", () => {
  const a = (n: number) => "a".repeat(n);
  it("24 chars stays one line", () => {
    expect(wrapName(a(24))).toEqual([a(24)]); // catches >= instead of >
  });
  it("25 chars with a space at index 12 splits there", () => {
    expect(wrapName(`${a(12)} ${a(12)}`)).toEqual([a(12), a(12)]); // catches no wrap for 25
  });
  it("wraps at the last space at or before index 24, keeping whole words", () => {
    expect(wrapName("Code Interpreter and History Configuration")).toEqual(["Code Interpreter and", "History Configuration"]); // catches a hard cut at 24
  });
  it("a space exactly at index 24 is a valid split point", () => {
    expect(wrapName(`${a(24)} bbb`)).toEqual([a(24), "bbb"]); // catches < 24 instead of <= 24
  });
  it("a space at index 25 is too late: the first 24 chars are the first line", () => {
    expect(wrapName(`${a(25)} bbb`)).toEqual([a(24), `${a(1)} bbb`]); // catches searching past index 24
  });
  it("a 60-char spaced name gives two lines, each <= 24, second ending in an ellipsis", () => {
    const name = Array.from({ length: 10 }, () => "abcde").join(" "); // 59 chars
    const lines = wrapName(name);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toBe("abcde abcde abcde abcde"); // last space <= 24 is at 23
    expect(lines[1]).toBe(`${"abcde abcde abcde abcde abcde abcde".slice(0, 23)}…`); // catches an uncut second line
    for (const l of lines) expect(l.length).toBeLessThanOrEqual(24);
  });
  it("a 30-char name with no space is its first 24 chars then the rest", () => {
    expect(wrapName(a(30))).toEqual([a(24), a(6)]); // catches a cut to one line
  });
  it("a second line of exactly 24 is kept, 25 is cut", () => {
    expect(wrapName(`${a(10)} ${a(24)}`)).toEqual([a(10), a(24)]); // catches > 23 on the second line
    expect(wrapName(`${a(10)} ${a(25)}`)).toEqual([a(10), `${a(23)}…`]); // catches no cut on the second line
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
          point(2, { x: 0.25, seconds: 3600 }),
          point(3, { x: 0.5, seconds: 100000 }),
        ],
      }),
    );
    expect(l.labels[0].x).toBe(500);
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

  it("a long-named label on an island at x=1 ends fully inside [8, 992]", () => {
    const name = "Billing export · invoice · customer";
    const { labels } = galaxyLayout(
      report({ topics: [topic(0, { name })], points: [point(1, { x: 0, y: 0.5 }), point(2, { x: 0.001, y: 0.5 })] }),
    );
    const l = labels[0];
    expect(l.x - l.w / 2).toBeGreaterThanOrEqual(8);
    expect(l.x + l.w / 2).toBeLessThanOrEqual(992);
    expect(l.lines).toEqual(["Billing export · invoice", "· customer"]);
  });

  it("a long-named label at the right edge is clamped to x1 <= 992", () => {
    const { labels } = galaxyLayout(
      report({ topics: [topic(0, { name: "Billing export · invoice · customer" })], points: [point(1, { x: 1 })] }),
    );
    expect(labels[0].x + labels[0].w / 2).toBeLessThanOrEqual(992);
    expect(labels[0].x + labels[0].w / 2).toBeCloseTo(992, 6);
  });

  it("a 200-char name wraps to two lines, the second cut to 23 chars + ellipsis; the topic keeps the full name", () => {
    const name = "x".repeat(200);
    const { labels } = galaxyLayout(report({ topics: [topic(0, { name })], points: [point(1)] }));
    const l = labels[0];
    expect(l.lines).toEqual(["x".repeat(24), `${"x".repeat(23)}…`]);
    expect(l.topic.name).toBe(name);
    expect(l.x + l.w / 2).toBeLessThanOrEqual(992);
  });

  it("the 42-char name renders two tspans, no ellipsis, and the full aria-label", () => {
    const name = "Code Interpreter and History Configuration";
    const { container } = render(<TopicGalaxy report={report({ topics: [topic(0, { name })], points: [point(1)] })} />);
    const g = container.querySelector(".sx-gal-label")!;
    expect(g.getAttribute("aria-label")).toBe(`${name}, 1.0h`);
    const text = g.querySelector("text.sx-gal-name")!;
    const spans = text.querySelectorAll("tspan");
    expect(spans).toHaveLength(2); // catches a single-line render
    expect([...spans].map((s) => s.textContent)).toEqual(["Code Interpreter and", "History Configuration"]);
    expect(text.textContent).not.toContain("…"); // catches the old cut
    expect(spans[0].getAttribute("dy")).toBe("0");
    expect(spans[1].getAttribute("x")).toBe(spans[0].getAttribute("x")); // catches a second line that drifts off-centre
    const fs = Number(text.getAttribute("font-size"));
    expect(Number(spans[1].getAttribute("dy"))).toBeCloseTo(fs * 1.1, 6);
  });

  it("a wrapped label is taller than an unwrapped one at the same font size, by one 1.1 line", () => {
    const one = galaxyLayout(report({ topics: [topic(0, { name: "Short name" })], points: [point(1)] })).labels[0];
    const two = galaxyLayout(report({ topics: [topic(0, { name: "Code Interpreter and History Configuration" })], points: [point(1)] })).labels[0];
    expect(two.fontSize).toBeCloseTo(one.fontSize, 10);
    expect(two.h - one.h).toBeCloseTo(one.fontSize * 1.1, 10); // catches an unchanged box
  });

  it("a wrapped label's width is its widest line", () => {
    const l = galaxyLayout(report({ topics: [topic(0, { name: "Code Interpreter and History Configuration" })], points: [point(1)] })).labels[0];
    expect(l.w).toBeCloseTo(21 * l.fontSize * 0.62, 10); // second line (21) is wider than the first (20)
  });

  it("ten topics with two near-identical long-named centroids end with no overlapping boxes, all inside the map", () => {
    const names = ["Code Interpreter and History Configuration", "Sjúkra Claude Integration"];
    const topics = Array.from({ length: 10 }, (_, i) => topic(i, { name: names[i] ?? `Theme number ${i}`, seconds: 3600 * (i < 2 ? 3 : 1) }));
    const points = topics.map((t, i) =>
      point(i + 1, { topic_id: t.id, x: i < 2 ? 0.9 + i * 0.001 : (i - 2) / 8, y: i < 2 ? 0.5 + i * 0.001 : (i - 2) / 7 }),
    );
    const { labels, height } = galaxyLayout(report({ topics, points }));
    expect(labels).toHaveLength(10);
    for (let i = 0; i < labels.length; i++)
      for (let j = i + 1; j < labels.length; j++) expect(overlaps(labels[i], labels[j])).toBe(false);
    for (const l of labels) {
      expect(l.x - l.w / 2).toBeGreaterThanOrEqual(8 - 1e-9);
      expect(l.x + l.w / 2).toBeLessThanOrEqual(992 + 1e-9);
      expect(l.y - l.h / 2).toBeGreaterThanOrEqual(-1e-9);
      expect(l.y + l.h / 2).toBeLessThanOrEqual(height + 1e-9);
    }
  });

  it("label box height counts the hours line and width covers the hours text", () => {
    const { labels } = galaxyLayout(report({ topics: [topic(0, { name: "ab", seconds: 3600 * 100 })], points: [point(1)] }));
    const l = labels[0];
    expect(l.h).toBeCloseTo(l.fontSize + 16 + 13, 10); // catches a name-only box
    expect(l.w).toBeCloseTo("100.0h".length * 13 * 0.62, 10); // catches a name-only width
  });

  it("six labels crowded at the right edge stay inside the map and never overlap", () => {
    const topics = Array.from({ length: 6 }, (_, i) => topic(i, { name: `Billing export · invoice ${i}`, seconds: 3600 * (i + 1) }));
    const points = [
      ...topics.map((t, i) => point(i + 1, { topic_id: t.id, x: 1, y: 0.5 + i * 0.01 })),
      point(7, { topic_id: 0, x: 0, y: 0 }),
      point(8, { topic_id: 0, x: 0, y: 1 }),
    ]; // full y range: six two-line labels need more than the 360 minimum height
    const { labels } = galaxyLayout(report({ topics, points }));
    for (const l of labels) {
      expect(l.x - l.w / 2).toBeGreaterThanOrEqual(8);
      expect(l.x + l.w / 2).toBeLessThanOrEqual(992);
      expect(l.y - l.h / 2).toBeGreaterThanOrEqual(0);
      expect(l.y + l.h / 2).toBeLessThanOrEqual(620);
    }
    for (let i = 0; i < labels.length; i++)
      for (let j = i + 1; j < labels.length; j++) expect(overlaps(labels[i], labels[j])).toBe(false);
  });

  it("hue comes from the topic id, not array position: id 3 of K=10 is 133", () => {
    const topics = [topic(3), ...Array.from({ length: 9 }, (_, i) => topic(i < 3 ? i : i + 1))];
    const { dots } = galaxyLayout(report({ topics, points: [point(1, { topic_id: 3 })] }));
    expect(dots[0].fill).toBe("oklch(calc(var(--project-l) + -0.08) var(--project-c) 133)");
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
  // catches: a leftover synonym anywhere in the rendered markup, tips included
  it("renders none of the retired names", () => {
    const { container } = render(<TopicGalaxy report={report({ topics: [topic(0)], points: [point(1)] })} />);
    for (const old of ["New ground", "Unlike anything else", "Novelty"]) expect(container.innerHTML).not.toContain(old);
  });

  it("dot tip has exact title and rows", () => {
    const p = point(1, { label: "Fix login", day: "2026-10-03", seconds: 5400, ticket: "ABC-1", novelty: 0.426 });
    expect(galaxyTipDot(p, "auth")).toEqual({
      title: "Fix login",
      rows: [
        ["Day", "2026-10-03"],
        ["Hours", "1h 30m"],
        ["Ticket", "ABC-1"],
        ["Topic", "auth"],
        ["Newness", "43%"],
      ],
    });
  });

  it("dot tip says none for a missing ticket", () => {
    expect(galaxyTipDot(point(1), "t").rows!.find((r) => r[0] === "Ticket")![1]).toBe("none");
  });

  it("label tip carries description, keywords, hours and blocks", () => {
    const t = topic(0, { name: "Auth", description: "Login work", keywords: ["login", "token"], seconds: 9000, blocks: 4, first_day: "2026-10-01", last_day: "2026-10-03" });
    expect(galaxyTipLabel(t)).toEqual({
      title: "Auth",
      sub: "Login work",
      rows: [
        ["Keywords", "login, token"],
        ["Hours", "2.5h"],
        ["Blocks", "4 · 2026-10-01 … 2026-10-03"],
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

  it("dots are pointer-only (tabindex -1, still tipped) and labels are the tab stops", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const dots = container.querySelectorAll(".sx-gal-dot");
    expect(dots).toHaveLength(3);
    for (const d of dots) {
      expect(d.getAttribute("tabindex")).toBe("-1");
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
    expect(css.match(/opacity:\.12/g)).toHaveLength(2 * 3);
  });

  it("isolation also triggers on keyboard focus (focus-visible and focus-within)", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const css = container.querySelector("style")!.textContent!;
    expect(css).toContain('[data-t="0"]:focus-visible');
    expect(css).toContain('[data-t="1"]:focus-within');
  });

  it("shows the How line", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    expect(container.textContent).toContain("How it works: Each dot is a work block.");
  });

  const slim = report({
    topics: [topic(0), topic(1)],
    points: [point(1, { y: 0.2 }), point(2, { topic_id: 1, x: 0.2, y: 0.6 })],
  });

  it("viewBox height follows the y extent but never drops below 360 (y 0.2-0.6 gives 360, not 620)", () => {
    const { container } = render(<TopicGalaxy report={slim} />);
    expect(container.querySelector("svg.sx-gal-svg")!.getAttribute("viewBox")).toBe("0 0 1000 360");
  });

  it("viewBox height grows with the range: y 0-0.8 gives 0.8*540+80 = 512", () => {
    const r = report({ points: [point(1, { y: 0 }), point(2, { y: 0.8 })] });
    expect(galaxyLayout(r).height).toBeCloseTo(512, 10);
  });

  it("full y range keeps 620", () => {
    expect(galaxyLayout(report({ points: [point(1, { y: 0 }), point(2, { y: 1 })] })).height).toBe(620);
  });

  it("legend row has both marks and the plain-words text", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const legend = container.querySelector(".sx-gal-legend")!;
    expect(legend.textContent).toContain("dot size = hours");
    expect(legend.textContent).toContain("first time");
    expect(legend.textContent).toContain("oddball");
    expect(legend.querySelectorAll("svg")).toHaveLength(3);
  });

  it("headline names the two largest topics by seconds (not first two) with their rounded combined share", () => {
    const r = report({
      topics: [topic(0, { name: "Gamma", seconds: 1800 }), topic(1, { name: "Alpha", seconds: 7200 }), topic(2, { name: "Beta", seconds: 3600 })],
      points: [point(1, { topic_id: 0 }), point(2, { topic_id: 1, x: 0.2 }), point(3, { topic_id: 2, x: 0.8 })],
    });
    const { container } = render(<TopicGalaxy report={r} />);
    const h = container.querySelector(".sx-gal-headline")!;
    expect(h.textContent).toBe("3 topics · Alpha and Beta are 86% of your time"); // 10800/12600 = 85.7 rounds up; catches floor/unsorted
    expect(h.nextElementSibling!.classList.contains("sx-gal-map")).toBe(true); // catches headline below the map
  });

  it("headline with one topic is '1 topic · {A}' with no percentage", () => {
    const { container } = render(<TopicGalaxy report={report({ topics: [topic(0, { name: "Solo" })] })} />);
    expect(container.querySelector(".sx-gal-headline")!.textContent).toBe("1 topic · Solo"); // catches '1 topics' / 100%
  });

  it("no headline in the empty state", () => {
    const { container } = render(<TopicGalaxy report={report({ points: [] })} />);
    expect(container.querySelector(".sx-gal-headline")).toBeNull();
  });

  it("headline css is 16px in --fg", () => {
    const css = readFileSync(`${import.meta.dir}/../../app/stats/galaxy.css`, "utf8");
    expect(css).toMatch(/\.sx-gal-headline\s*\{[^}]*font-size:\s*16px[^}]*color:\s*var\(--fg\)/);
  });

  it("legend ends with the dot hint", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    expect(container.querySelector(".sx-gal-legend")!.textContent).toMatch(/· hover or tap a dot for the block$/);
  });

  it("topic name fill is the topic hue lifted toward --fg", () => {
    const { container } = render(<TopicGalaxy report={slim} />);
    const names = container.querySelectorAll("text.sx-gal-name");
    const fill = (names[1] as SVGElement).getAttribute("style")!;
    expect(fill).toContain("oklch(calc(var(--project-l) + -0.08) var(--project-c) 205)");
    expect(fill).toContain("var(--fg) 35%");
  });

  it("every dot has a sibling hit circle r=12 behind it", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    for (const d of container.querySelectorAll(".sx-gal-dot")) {
      const hit = d.previousElementSibling!;
      expect(hit.classList.contains("sx-gal-hit")).toBe(true);
      expect(hit.getAttribute("r")).toBe("12");
      expect(hit.getAttribute("cx")).toBe(d.getAttribute("cx"));
      expect(hit.getAttribute("cy")).toBe(d.getAttribute("cy"));
    }
  });

  it("topic list has one focusable tipped row per topic with name and hours", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const rows = container.querySelectorAll(".sx-gal-list > li");
    expect(rows).toHaveLength(2);
    for (const r of rows) {
      expect(r.getAttribute("tabindex")).toBe("0");
      expect(r.getAttribute("data-stip")).toBeTruthy();
    }
    expect(rows[0].textContent).toContain("topic 0");
    expect(rows[0].textContent).toContain("1.0h");
  });

  it("map svg fits its width: meet aspect ratio, no scroll or min-width container", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const svg = container.querySelector(".sx-gal-map > svg.sx-gal-svg")!;
    expect(svg.getAttribute("preserveAspectRatio")).toBe("xMidYMid meet"); // catches an absent attribute
    expect(container.querySelector(".sx-gal-scroll")).toBeNull(); // catches the old scroll wrapper
    const css = readFileSync(`${import.meta.dir}/../../app/stats/galaxy.css`, "utf8");
    expect(css).not.toMatch(/min-width:\s*720/); // catches the retained forced min-width
    expect(css).not.toMatch(/overflow-x:\s*(auto|scroll)/); // catches retained horizontal scroll
  });

  it("below 600px the css hides on-map labels, shows the list and enlarges dots 1.6x", () => {
    const css = readFileSync(`${import.meta.dir}/../../app/stats/galaxy.css`, "utf8");
    const blocks = [...css.matchAll(/@media \(max-width: 599px\) \{([^@]*)\}\s*(?=\n|$)/g)].map((m) => m[1]).join("\n");
    expect(blocks).toMatch(/\.sx-gal-label\s*\{[^}]*display:\s*none/); // catches labels left visible on phone
    expect(blocks).toMatch(/\.sx-gal-list\s*\{[^}]*display:\s*block/); // catches list hidden on phone
    expect(blocks).toMatch(/\.sx-gal-dot[^{]*\{[^}]*transform:\s*scale\(1\.6\)/); // catches missing dot enlargement
    const desktop = css.replace(/@media \(max-width: 599px\) \{[^@]*\}\s*(?=\n|$)/g, "");
    expect(desktop).not.toMatch(/\.sx-gal-list\s*\{[^}]*display:\s*none/); // catches a keyboard-unreachable list
    expect(desktop).toMatch(/\.sx-gal-list\s*\{[^}]*clip-path:\s*inset\(50%\)/); // catches no visually-hidden pattern
    expect(desktop).toMatch(/\.sx-gal-list:focus-within/); // catches a list that never becomes visible on focus
  });

  it("the caption has no jargon outside details", () => {
    const { container } = render(<TopicGalaxy report={full} />);
    const p = container.querySelector(".sx-gal-how")!;
    expect(p.textContent).not.toMatch(/t-SNE|cosine|perplexity|average-linkage/);
    const det = container.querySelector("details")!;
    expect(det.querySelector("summary")!.textContent).toBe("Method");
    expect(det.textContent).toContain("t-SNE");
    expect(container.querySelector(".sr-title")!.textContent).toBe("Topic galaxy");
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

describe("phone tap targets and spacing", () => {
  const css = readFileSync(`${import.meta.dir}/../../app/stats/galaxy.css`, "utf8");
  const phone = [...css.matchAll(/@media \(max-width: 599px\) \{([^@]*)\}\s*(?=\n|$)/g)].map((m) => m[1]).join("\n");

  it("below 600px the hit circle is scaled x3 about its centre", () => {
    expect(phone).toMatch(/\.sx-gal-hit\s*\{[^}]*transform-box:\s*fill-box/); // catches missing fill-box (scales about the svg origin)
    expect(phone).toMatch(/\.sx-gal-hit\s*\{[^}]*transform-origin:\s*center/);
    expect(phone).toMatch(/\.sx-gal-hit\s*\{[^}]*transform:\s*scale\(3\)/); // catches no enlargement
  });

  it("below 600px the map container is at least 260px tall", () => {
    expect(phone).toMatch(/\.sx-gal-map\s*\{[^}]*min-height:\s*260px/); // catches a desktop-only or smaller min-height
  });

  it("two labels at the same median y end at least 8px apart including the hours line", () => {
    const topics = [topic(0, { name: "code interpreter infrastructure" }), topic(1, { name: "code interpreter and history configuration" })];
    const { labels } = galaxyLayout(report({ topics, points: [point(1, { topic_id: 0 }), point(2, { topic_id: 1 })] }));
    const [a, b] = [...labels].sort((p, q) => p.y - q.y);
    expect(a.h).toBeCloseTo(a.fontSize * 2.1 + 16 + 13, 10); // catches the name-only box (both names wrap to two lines)
    expect(b.y - b.h / 2 - (a.y + a.h / 2)).toBeGreaterThanOrEqual(8 - 1e-9); // catches the 2px pad
  });

  it("every caption and legend font rule is at least 13px", () => {
    for (const sel of ["sx-gal-hours", "sx-gal-chip", "sx-gal-legend", "sx-gal-list-hours", "sx-gal-how", "sx-gal-method"]) {
      const m = css.match(new RegExp(`\\.${sel}\\s*\\{[^}]*font-size:\\s*(\\d+(?:\\.\\d+)?)px`));
      expect(m).not.toBeNull();
      expect(Number(m![1])).toBeGreaterThanOrEqual(13); // catches a leftover 12px
    }
  });
});
