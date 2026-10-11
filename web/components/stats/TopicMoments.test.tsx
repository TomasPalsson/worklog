import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { Highlight, Topic, TopicsReport } from "@/lib/topics_contract";
import { parseTip } from "./tip";
import { momentTip, stripFraction, TopicMoments } from "./TopicMoments";

afterEach(cleanup);

const topic = (id: number, name: string): Topic => ({
  id,
  name,
  description: null,
  named_by: "ai",
  keywords: [],
  seconds: 3600,
  blocks: 2,
  days_active: 1,
  first_day: "2026-10-05",
  last_day: "2026-10-09",
  top_tickets: [],
});

const highlight = (over: Partial<Highlight> = {}): Highlight => ({
  block_id: 1,
  day: "2026-10-07",
  label: "Merge billing view",
  ticket: "ABC-1",
  topic_id: 0,
  seconds: 5400,
  score: 0.85,
  ...over,
});

const report = (over: Partial<TopicsReport> = {}): TopicsReport => ({
  status: "ready",
  from: "2026-10-05",
  to: "2026-10-09",
  naming: "ai",
  topics: [topic(0, "Billing export"), topic(1, "Atlas map")],
  points: [],
  bin: "day",
  river: [],
  days: [],
  day_similarity: [],
  first_times: [highlight(), highlight({ block_id: 2, label: "Second", score: 0.6, topic_id: 1 })],
  oddballs: [highlight({ block_id: 3, label: "Typed state rows", score: 0.231, seconds: 1800 }), highlight({ block_id: 4, label: "Other", score: 0.1 })],
  oddball_median: 0.567,
  embedded_blocks: 8,
  skipped_blocks: 0,
  ...over,
});

describe("stripFraction", () => {
  it("first day of the range is 0 (catches a +1 offset)", () => {
    expect(stripFraction("2026-10-05", "2026-10-05", "2026-10-09")).toBe(0);
  });
  it("last day of the range is 1 (catches dividing by day count instead of span)", () => {
    expect(stripFraction("2026-10-09", "2026-10-05", "2026-10-09")).toBe(1);
  });
  it("middle is exact (catches index-based rather than date-based position)", () => {
    expect(stripFraction("2026-10-07", "2026-10-05", "2026-10-09")).toBe(0.5);
    expect(stripFraction("2026-10-06", "2026-10-05", "2026-10-09")).toBe(0.25);
  });
  it("measures across a month end (catches day-of-month subtraction)", () => {
    expect(stripFraction("2026-10-01", "2026-09-29", "2026-10-03")).toBe(0.5);
  });
  it("a day outside the range clamps to 0 or 1 (catches overflowing the strip)", () => {
    expect(stripFraction("2026-10-01", "2026-10-05", "2026-10-09")).toBe(0);
    expect(stripFraction("2026-10-20", "2026-10-05", "2026-10-09")).toBe(1);
  });
  it("a one-day range is centred, not NaN (catches 0/0)", () => {
    expect(stripFraction("2026-10-05", "2026-10-05", "2026-10-05")).toBe(0.5);
  });
});

describe("momentTip", () => {
  it("first time: exact title and rows", () => {
    const tip = momentTip(highlight(), "first_time", "Billing export", 0.5);
    expect(tip.title).toBe("Merge billing view");
    expect(tip.rows).toEqual([
      ["Day", "Oct 7"],
      ["Time", "1h 30m"],
      ["Topic", "Billing export"],
      ["Newness", "0.85"],
    ]);
  });
  it("oddball: nearest score beside the median", () => {
    const tip = momentTip(highlight({ score: 0.231 }), "oddball", "Billing export", 0.567);
    expect(tip.rows).toContainEqual(["Closest match", "0.23"]);
    expect(tip.rows).toContainEqual(["Your median", "0.57"]);
  });
  it("missing topic drops the Topic row", () => {
    const tip = momentTip(highlight(), "first_time", undefined, 0.5);
    expect(tip.rows!.some(([label]) => label === "Topic")).toBe(false);
  });
});

describe("TopicMoments", () => {
  it("keeps the server order in both lists (catches re-sorting by score)", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const labels = (kind: string) => Array.from(container.querySelectorAll(`[data-moment="${kind}"] .tm-label`)).map((e) => e.textContent);
    expect(labels("first_time")).toEqual(["Merge billing view", "Second"]);
    expect(labels("oddball")).toEqual(["Typed state rows", "Other"]);
  });
  // catches: number-first card, wrong word on a card, or the number dropped
  it("each card leads with a badge word, then the small number", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const first = container.querySelector('[data-moment="first_time"]')!;
    const odd = container.querySelector('[data-moment="oddball"]')!;
    expect(first.querySelector(".tm-badge")!.textContent).toBe("New ground");
    expect(first.querySelector(".tm-num")!.textContent).toBe("85% new");
    expect(odd.querySelector(".tm-badge")!.textContent).toBe("Unlike anything else");
    expect(odd.querySelector(".tm-num")!.textContent).toBe("closest match 23% · your usual 57%");
    for (const card of [first, odd]) {
      expect(card.querySelector(".tm-badge")!.compareDocumentPosition(card.querySelector(".tm-num")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });
  // catches: jargon left in visible text or tips
  it("no cosine, novelty or Nearest anything in visible text or tips", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const visible = container.cloneNode(true) as HTMLElement;
    visible.querySelector("details")!.remove();
    expect(visible.textContent!).not.toMatch(/cosine|novelty|nearest anything/i);
    for (const card of container.querySelectorAll("[data-stip]")) expect(card.getAttribute("data-stip")).not.toMatch(/cosine|novelty|nearest anything/i);
  });
  it("shows day, hours and the topic chip", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const card = container.querySelector('[data-moment="first_time"]')!;
    expect(card.querySelector(".tm-when")!.textContent).toBe("Oct 7 · 1h 30m");
    expect(card.querySelector(".tm-chip")!.textContent).toBe("Billing export");
  });
  it("a topic id with no topic renders no chip", () => {
    const { container } = render(<TopicMoments report={report({ first_times: [highlight({ topic_id: 99 })] })} />);
    expect(container.querySelector('[data-moment="first_time"] .tm-chip')).toBeNull();
  });
  it("strip marker sits at the exact fraction of the range", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const marker = container.querySelector('[data-moment="first_time"] .tm-marker') as HTMLElement;
    expect(marker.style.left).toBe("50%");
  });
  it("cards are focusable, labelled and carry a tip", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const card = container.querySelector('[data-moment="oddball"]')!;
    expect(card.getAttribute("tabindex")).toBe("0");
    expect(card.getAttribute("aria-label")).toBe("Typed state rows: 0.23");
    expect(parseTip(card.getAttribute("data-stip"))!.title).toBe("Typed state rows");
  });
  it("empty lists render one empty line each and no cards", () => {
    const { container } = render(<TopicMoments report={report({ first_times: [], oddballs: [] })} />);
    expect(container.querySelectorAll("[data-moment]").length).toBe(0);
    expect(container.querySelectorAll(".sr-empty").length).toBe(2);
    expect(container.textContent).toContain("Nothing here stands apart; your work is well clustered.");
  });
  it("only the empty list gets the empty line", () => {
    const { container } = render(<TopicMoments report={report({ oddballs: [] })} />);
    expect(container.querySelectorAll(".sr-empty").length).toBe(1);
    expect(container.querySelectorAll('[data-moment="first_time"]').length).toBe(2);
  });
  it("How line carries the median to two decimals and says similarity, not cosine", () => {
    const { container } = render(<TopicMoments report={report()} />);
    expect(container.querySelector(".tm-how > p")!.textContent).toBe(
      "How it works: First time = lowest similarity to any earlier block, including the 60 days before the range. Your first 20 blocks are skipped — they have nothing earlier to compare with. Oddball = its closest block in this range is under 0.30 similarity; your median block's closest is 0.57.",
    );
    expect(container.querySelector(".tm-how > p")!.textContent).not.toMatch(/cosine/i);
  });
  it("cosine lives in a Method disclosure", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const d = container.querySelector(".tm-how > details")!;
    expect(d.querySelector("summary")!.textContent).toBe("Method");
    expect(d.textContent).toMatch(/cosine/);
  });
  // catches: subtitles swapped between the cards or missing
  it("each card title carries its reading direction", () => {
    const { container } = render(<TopicMoments report={report()} />);
    const subs = [...container.querySelectorAll("section.sr-card")].map((s) => [s.querySelector(".sr-title")!.textContent, s.querySelector(".tm-sub")!.textContent]);
    expect(subs).toEqual([["First times", "higher = newer ground"], ["Oddballs", "lower = more unusual"]]);
  });
  // catches: caption per card, caption missing on a column, or endpoints taken from a card's own day
  it("exactly one range caption per column, under the heading", () => {
    const { container } = render(<TopicMoments report={report()} />);
    expect(container.querySelectorAll(".tm-ends").length).toBe(0);
    const ranges = [...container.querySelectorAll("section.sr-card")].map((s) => s.querySelectorAll(".tm-range").length + ":" + s.querySelector(".tm-range")!.textContent);
    expect(ranges).toEqual(["1:Oct 5 … Oct 9", "1:Oct 5 … Oct 9"]);
  });
  it("the caption stays when a column is empty", () => {
    const { container } = render(<TopicMoments report={report({ oddballs: [] })} />);
    expect(container.querySelectorAll(".tm-range").length).toBe(2);
  });
  // catches: sub-13px text left in the stylesheets
  it("no font-size under 13px in moments.css or compass.css", () => {
    for (const f of ["../../app/stats/moments.css", "../../app/stats/compass.css"]) {
      const sizes = [...readFileSync(new URL(f, import.meta.url), "utf8").matchAll(/font-size:\s*(\d+(?:\.\d+)?)px/g)].map((m) => Number(m[1]));
      expect(sizes.length).toBeGreaterThan(0);
      for (const n of sizes) expect(n).toBeGreaterThanOrEqual(13);
    }
  });
  it("label carries the two-line clamp class and is the .tm-label element", () => {
    const { container } = render(<TopicMoments report={report()} />);
    expect(container.querySelector(".tm-label .tm-clamp")!.textContent).toBe("Merge billing view");
  });
  it("marks are drawn and nothing renders NaN, undefined or null", () => {
    const { container } = render(<TopicMoments report={report({ first_times: [highlight({ ticket: null })] })} />);
    expect(container.querySelectorAll("svg").length).toBeGreaterThanOrEqual(2);
    expect(container.innerHTML).not.toMatch(/NaN|undefined|null/);
  });
});
