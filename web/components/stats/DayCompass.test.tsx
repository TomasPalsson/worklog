import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { DayRow, Topic, TopicsReport } from "@/lib/topics_contract";
import { DayCompass, compassLayout, compassTip } from "./DayCompass";

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

const dayRow = (day: string, focus: number | null, novelty: number, workSeconds = 3600): DayRow => ({
  day,
  work_seconds: workSeconds,
  focus,
  novelty,
  switches: 4,
  longest_run_seconds: 5400,
  top_topic: 1,
  topics_touched: 3,
});

const report = (days: DayRow[]): TopicsReport => ({
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  naming: "off",
  topics: [topic(0, "Billing export"), topic(1, "Tempo sync")],
  points: [],
  bin: "day",
  river: [],
  days,
  day_similarity: [],
  first_times: [],
  oddballs: [],
  oddball_median: 0,
  embedded_blocks: 0,
  skipped_blocks: 0,
});

// Odd count: medians are focus 0.3 and novelty 0.6; Oct 6 has one block so no focus.
const week = report([
  dayRow("2026-10-01", 0.1, 0.5),
  dayRow("2026-10-02", 0.2, 0.6),
  dayRow("2026-10-03", 0.3, 0.7),
  dayRow("2026-10-04", 0.4, 0.55),
  dayRow("2026-10-05", 0.5, 0.65),
  dayRow("2026-10-06", null, 0.9),
]);

describe("compassLayout", () => {
  it("puts the crosshairs at the medians of the plotted days only", () => {
    // Counting the None-focus day (novelty 0.9) would move the novelty median off Oct 2.
    const { medianX, medianY, plotted } = compassLayout(week);
    expect(plotted.find((p) => p.day === "2026-10-03")!.x).toBe(medianX);
    expect(plotted.find((p) => p.day === "2026-10-02")!.y).toBe(medianY);
  });
  it("uses the mean of the two middle values for an even count", () => {
    // focus 0.1 0.2 0.4 0.8 -> median 0.3; the upper (0.4) or lower (0.2) middle would be wrong.
    const even = report([dayRow("2026-10-01", 0.1, 0.5), dayRow("2026-10-02", 0.2, 0.5), dayRow("2026-10-03", 0.4, 0.5), dayRow("2026-10-04", 0.8, 0.5)]);
    const { medianX, plotted } = compassLayout(even);
    const x = (day: string) => plotted.find((p) => p.day === day)!.x;
    expect(medianX).toBeCloseTo((x("2026-10-02") + x("2026-10-03")) / 2, 6);
  });
  it("assigns quadrants, with a value equal to the median counting as deep / new", () => {
    const quadrant = Object.fromEntries(compassLayout(week).plotted.map((p) => [p.day, p.quadrant]));
    expect(quadrant).toEqual({
      "2026-10-01": "scattered-routine",
      "2026-10-02": "scattered-exploring", // novelty exactly at the median -> new
      "2026-10-03": "deep-new", // focus exactly at the median -> deep
      "2026-10-04": "deep-familiar",
      "2026-10-05": "deep-new",
    });
  });
  it("flags the day farthest from the crosshairs per quadrant, normalised by axis span", () => {
    const labelled = compassLayout(week).plotted.filter((p) => p.labelled).map((p) => p.day).sort();
    // deep-new holds Oct 3 (0, 0.5 of the novelty span) and Oct 5 (0.5, 0.25): Oct 5 is farther.
    expect(labelled).toEqual(["2026-10-01", "2026-10-02", "2026-10-04", "2026-10-05"]);
  });
  it("lists None-focus days as skipped and never plots them", () => {
    const { plotted, skipped } = compassLayout(week);
    expect(skipped).toEqual(["2026-10-06"]);
    expect(plotted.map((p) => p.day)).not.toContain("2026-10-06");
  });
  it("scales radius with the square root of hours", () => {
    const sized = report([dayRow("2026-10-01", 0.1, 0.5, 900), dayRow("2026-10-02", 0.2, 0.6, 3600)]);
    const [small, large] = compassLayout(sized).plotted;
    expect(large.radius / small.radius).toBeCloseTo(2, 6);
  });
  it("keeps every coordinate finite with one plotted day, none, or zero hours", () => {
    for (const days of [[dayRow("2026-10-01", 0.2, 0.5)], [dayRow("2026-10-01", null, 0.5)], [dayRow("2026-10-01", 0.2, 0.5, 0)]]) {
      const layout = compassLayout(report(days));
      for (const n of [layout.medianX, layout.medianY, ...layout.plotted.flatMap((p) => [p.x, p.y, p.radius])]) expect(Number.isFinite(n)).toBe(true);
    }
  });
});

describe("compassTip", () => {
  it("lists day, hours, focus, novelty, switches, longest run and topics touched", () => {
    const tip = compassTip(week.days[2]);
    expect(tip.title).toBe("Sat Oct 3");
    expect(tip.rows).toEqual([
      ["Hours", "1h"],
      ["Focus", "0.30"],
      ["Novelty", "0.70"],
      ["Switches", "4"],
      ["Longest run", "1h 30m"],
      ["Topics touched", "3"],
    ]);
  });
});

describe("DayCompass", () => {
  it("draws one dot per plotted day with a tip, plus two crosshair lines and four captions", () => {
    const { container } = render(<DayCompass report={week} />);
    const dots = container.querySelectorAll(".sx-cmp-dot");
    expect(dots.length).toBe(5);
    for (const dot of dots) expect(dot.getAttribute("data-stip")).toBeTruthy();
    expect(container.querySelectorAll(".sx-cmp-cross").length).toBe(2);
    for (const caption of ["Deep · new ground", "Deep · familiar", "Scattered · exploring", "Scattered · routine"]) {
      expect(container.textContent).toContain(caption);
    }
  });
  it("labels the extreme day of each quadrant with weekday and date", () => {
    const { container } = render(<DayCompass report={week} />);
    const labels = [...container.querySelectorAll(".sx-cmp-leader-label")].map((el) => el.textContent).sort();
    expect(labels).toEqual(["Fri Oct 2", "Mon Oct 5", "Sun Oct 4", "Thu Oct 1"]);
    expect(container.querySelectorAll(".sx-cmp-leader").length).toBe(4);
  });
  it("lists the None-focus days under the chart", () => {
    const { container } = render(<DayCompass report={week} />);
    expect(container.querySelector(".sx-cmp-skipped")!.textContent).toBe("single-block days: Oct 6");
  });
  it("omits the skipped line when every day has a focus", () => {
    const { container } = render(<DayCompass report={report([dayRow("2026-10-01", 0.2, 0.5)])} />);
    expect(container.querySelector(".sx-cmp-skipped")).toBeNull();
  });
  it("renders nothing when no day can be plotted", () => {
    expect(render(<DayCompass report={report([dayRow("2026-10-01", null, 0.5)])} />).container.querySelector("svg")).toBeNull();
  });
  it("renders no NaN, undefined or null", () => {
    const html = render(<DayCompass report={week} />).container.innerHTML;
    for (const bad of ["NaN", "undefined", "null"]) expect(html).not.toContain(bad);
  });
  it("shows the How line", () => {
    const { container } = render(<DayCompass report={week} />);
    expect(container.textContent).toContain(
      "How: Focus = how alike that day's blocks are (hours-weighted mean pairwise cosine). Novelty = how unlike anything you did before (1 − nearest earlier block). Lines are your medians.",
    );
  });
  // catches: labels placed at a fixed offset with no edge flip, clamp or caption avoidance
  it("keeps every leader label box inside the plot and clear of the captions", () => {
    const edges = report([
      dayRow("2026-09-19", 0.1, 0.1), dayRow("2026-09-20", 0.5, 0.5), dayRow("2026-09-25", 1, 1),
      dayRow("2026-09-27", 1, 0.1), dayRow("2026-09-26", 0.1, 1),
    ]);
    const { container } = render(<DayCompass report={edges} />);
    const box = (el: Element) => {
      const x = Number(el.getAttribute("x")), y = Number(el.getAttribute("y"));
      const w = el.textContent!.length * 6.2;
      const x0 = el.getAttribute("text-anchor") === "end" ? x - w : x;
      return { x0, x1: x0 + w, y0: y - 10, y1: y };
    };
    const labels = [...container.querySelectorAll(".sx-cmp-leader-label")].map(box);
    const captions = [...container.querySelectorAll(".sx-cmp-caption")].map(box);
    expect(labels.length).toBe(4);
    for (const l of labels) {
      expect(l.x0).toBeGreaterThanOrEqual(8);
      expect(l.x1).toBeLessThanOrEqual(472);
      expect(l.y0).toBeGreaterThanOrEqual(24);
      expect(l.y1).toBeLessThanOrEqual(296);
      for (const c of captions) expect(l.x1 <= c.x0 || l.x0 >= c.x1 || l.y1 <= c.y0 || l.y0 >= c.y1).toBe(true);
    }
  });
});
