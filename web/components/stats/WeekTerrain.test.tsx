import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { bandTip, heightOf, labelPlate, PLATE_W, peakOf, rampColor, shares, terrainTip, W, WeekTerrain, weekdayTip } from "./WeekTerrain";
import { parseTip } from "./tip";

afterEach(cleanup);

const blank = () => Array.from({ length: 7 }, () => Array<number>(24).fill(0));

describe("peakOf / shares", () => {
  it("finds the peak, first wins ties", () => {
    const g = blank();
    g[3][12] = 297;
    g[1][9] = 297;
    expect(peakOf(g)).toEqual({ d: 1, h: 9, v: 297 });
  });
  it("empty and ragged grids", () => {
    expect(peakOf(blank())).toBeNull();
    expect(peakOf([])).toBeNull();
    expect(shares([])).toEqual({ total: 0, nightPct: 0, earlyPct: 0 });
    const ragged = [[1, 2], [], [0, 0, 5]];
    expect(peakOf(ragged)).toEqual({ d: 2, h: 2, v: 5 });
    expect(shares(ragged).total).toBe(8);
  });
  it("night and early percentages", () => {
    const g = blank();
    g[0][21] = 1;
    g[0][6] = 1;
    g[0][12] = 2;
    expect(shares(g)).toEqual({ total: 4, nightPct: 25, earlyPct: 25 });
  });
});

describe("layout helpers", () => {
  it("heightOf: sqrt, capped, zero-safe", () => {
    expect(heightOf(0, 10)).toBe(0);
    expect(heightOf(10, 10)).toBe(6);
    expect(heightOf(2.5, 10)).toBeCloseTo(3);
    expect(heightOf(1e12, 10)).toBe(6);
  });
  it("rampColor spans slate to amber", () => {
    expect(rampColor(0)).toContain("var(--slate)");
    expect(rampColor(0.5)).toBe("color-mix(in oklch, var(--amber) 0%, var(--sage))");
    expect(rampColor(0.25)).toContain("var(--sage) 50%");
    expect(rampColor(1)).toContain("var(--amber) 100%");
  });
  it("labelPlate stays inside the viewBox", () => {
    expect(labelPlate(-50, 0).x).toBe(4);
    expect(labelPlate(9999, 500).x + PLATE_W).toBeLessThanOrEqual(W - 4);
    expect(labelPlate(100, 10).y).toBe(4);
  });
});

describe("WeekTerrain", () => {
  it("headline, aria and peak label", () => {
    const g = blank();
    g[3][12] = 297;
    g[0][22] = 3;
    const { container } = render(<WeekTerrain grid={g} />);
    expect(container.textContent).toContain("Your peak: Thursdays at 12:00");
    expect(container.textContent).toContain("Thu 12:00 · 297");
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("Peak Thu 12:00 with 297 events");
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.querySelectorAll("g.sx-terrain-pillar").length).toBe(2);
  });
  it("single event and huge values", () => {
    const g = blank();
    g[6][0] = 1;
    const { container } = render(<WeekTerrain grid={g} />);
    expect(container.textContent).toContain("Sundays at 00:00");
    cleanup();
    g[6][0] = 1e12;
    expect(render(<WeekTerrain grid={g} />).container.innerHTML).not.toContain("NaN");
  });
  it("empty state", () => {
    const { container } = render(<WeekTerrain grid={blank()} />);
    expect(container.textContent).toContain("nothing built yet");
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("nothing built yet");
    expect(container.querySelectorAll("g.sx-terrain-pillar").length).toBe(0);
  });
});

describe("terrain tips", () => {
  const g = () => {
    const x = blank();
    x[3][12] = 297;
    x[3][9] = 3;
    x[0][22] = 100;
    return x;
  };
  it("peak slot", () => {
    const t = terrainTip(g(), 3, 12);
    expect(t.title).toBe("Thursdays · 12:00–13:00");
    expect(t.sub).toBe("#1 of 168 weekday-hours");
    expect(t.rows).toEqual([
      ["Events this hour", "297"],
      ["Share of all activity", "74%"],
      ["Rank", "1 of 168"],
      ["Busiest hour on Thursdays", "12:00 (297)"],
      ["Avg hour on Thursdays", "12.5 events"],
    ]);
    expect(t.bar).toEqual({ value: 297, max: 297, label: "100% of the peak slot" });
    expect(t.note).toBe("Your summit: the busiest weekday-hour of them all.");
  });
  it("quietest active slot and mid slot", () => {
    const q = terrainTip(g(), 3, 9);
    expect(q.note).toBe("Quietest weekday-hour you still showed up for.");
    expect(q.rows![2]).toEqual(["Rank", "3 of 168"]);
    const m = terrainTip(g(), 0, 22);
    expect(m.note).toBe("Late-evening hour: 34% of your peak.");
  });
  it("empty slot and midnight wrap", () => {
    const t = terrainTip(g(), 0, 23);
    expect(t.title).toBe("Mondays · 23:00–00:00");
    expect(t.rows).toEqual([["Events", "0"]]);
  });
  it("single event, huge value, empty grid", () => {
    const one = blank();
    one[6][0] = 1;
    expect(terrainTip(one, 6, 0).note).toBe("Your summit: the busiest weekday-hour of them all.");
    one[6][0] = 1e12;
    expect(terrainTip(one, 6, 0).rows![0]).toEqual(["Events this hour", "1,000,000,000,000"]);
    expect(terrainTip(blank(), 0, 0).sub).toBe("Nothing happened here");
    expect(weekdayTip(blank(), 0).rows).toEqual([["Events", "0"], ["Share of all activity", "0%"]]);
  });
  it("weekday and band tips", () => {
    expect(weekdayTip(g(), 3)).toMatchObject({
      title: "Thursdays",
      rows: [["Events", "300"], ["Share of all activity", "75%"], ["Peak hour", "12:00 (297 events)"]],
      note: "Lunch hour is when Thursdays peak.",
    });
    expect(bandTip(g(), "sun").rows).toEqual([["Events", "3"], ["Share of all activity", "1%"]]);
    expect(bandTip(g(), "moon").note).toBe("25% of your activity happens after dark.");
    expect(bandTip(blank(), "moon").note).toBe("Nothing to compare yet.");
  });
  it("renders tips on marks and drops native titles", () => {
    const { container } = render(<WeekTerrain grid={g()} />);
    const pillar = container.querySelector("g.sx-terrain-tipped[tabindex]")!;
    expect(parseTip(pillar.getAttribute("data-stip"))!.title).toContain("·");
    expect(pillar.getAttribute("aria-label")).toContain("events");
    expect(container.querySelectorAll("svg title").length).toBe(0);
    expect(container.querySelectorAll("[data-stip]").length).toBeGreaterThan(168);
    // tab stops: 3 pillars + 7 weekday labels + sun + moon
    expect(container.querySelectorAll("[tabindex]").length).toBe(12);
  });
});
