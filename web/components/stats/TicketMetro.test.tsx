import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { TicketStat } from "@/lib/stats_contract";
import {
  assignLanes,
  hueAt,
  layoutLines,
  linePath,
  xOf,
  longestRide,
  metroAria,
  monthTicks,
  stationIdx,
  thickness,
  TicketMetro,
  lineTip,
  stationTip,
} from "./TicketMetro";

afterEach(cleanup);

const tk = (
  key: string,
  days: string[],
  seconds = 3600,
  extra: Partial<TicketStat> = {},
): TicketStat => ({
  key,
  summary: `${key} summary`,
  seconds,
  blocks: days.length,
  first_day: days[0],
  last_day: days[days.length - 1],
  days_active: days.length,
  days,
  ...extra,
});

describe("assignLanes", () => {
  it("shares a lane when lines do not overlap", () => {
    expect(
      assignLanes([
        { start: 0, end: 3 },
        { start: 5, end: 8 },
        { start: 2, end: 6 },
      ]),
    ).toEqual([0, 0, 1]);
  });
  it("respects the gap and handles empty", () => {
    expect(
      assignLanes(
        [
          { start: 0, end: 3 },
          { start: 4, end: 8 },
        ],
        2,
      ),
    ).toEqual([0, 1]);
    expect(assignLanes([])).toEqual([]);
  });
});

describe("helpers", () => {
  it("thickness stays in 3..9", () => {
    expect(thickness(10, 10)).toBe(9);
    expect(thickness(0, 10)).toBe(3);
    expect(thickness(5, 0)).toBe(3);
    expect(thickness(1e12, 10)).toBe(9);
  });
  it("hues spread evenly", () => {
    expect(hueAt(1, 4) - hueAt(0, 4)).toBe(90);
  });
  it("stationIdx falls back to first/last day and dedupes", () => {
    const base = { first_day: "2026-10-02", last_day: "2026-10-04" };
    expect(
      stationIdx(
        { ...base, days: undefined as unknown as string[] },
        "2026-10-01",
        10,
      ),
    ).toEqual([1, 3]);
    expect(
      stationIdx(
        { ...base, days: ["2026-10-02", "2026-10-02", "2026-09-01"] },
        "2026-10-01",
        10,
      ),
    ).toEqual([0, 1]);
  });
  it("linePath dips only on long quiet stretches", () => {
    expect(linePath([10, 20, 30], 5)).not.toContain(" 14 ");
    expect(linePath([10, 300], 5)).toContain(" 14.0 ");
    expect(linePath([], 5)).toBe("");
    // same day gap, different range lengths: dip depends on days, not pixels
    const at = (days: number[], n: number) =>
      linePath(days.map((k) => xOf(k, n)), 5, undefined, days);
    expect(at([0, 3], 30).split("L").length).toBe(2);
    expect(at([0, 20], 365).split("L").length).toBeGreaterThan(4);
  });
  it("monthTicks labels month starts without crowding", () => {
    expect(monthTicks("2026-09-28", 10, 20).map((m) => m.label)).toEqual([
      "Sep",
      "Oct",
    ]);
    expect(monthTicks("2026-09-28", 10, 2).map((m) => m.label)).toEqual([
      "Sep",
    ]);
  });
});

describe("TicketMetro", () => {
  const tickets = [
    tk(
      "ABC-1",
      ["2026-10-01", "2026-10-02", "2026-10-03", "2026-10-20"],
      36000,
    ),
    tk("DEF-2", ["2026-10-05", "2026-10-06"], 7200),
    tk("GHI-3", ["2026-10-25"], 1800, { summary: null }),
  ];
  it("lays out lines and names the longest ride", () => {
    const { lines } = layoutLines(tickets, "2026-10-01", 31);
    expect(lines.map((l) => l.t.key)).toEqual(["ABC-1", "DEF-2", "GHI-3"]);
    expect(longestRide(lines)).toEqual({ key: "ABC-1", stations: 4, days: 20 });
  });
  it("renders headline, aria, titles, no NaN", () => {
    const { container } = render(
      <TicketMetro tickets={tickets} from="2026-10-01" to="2026-10-31" />,
    );
    expect(container.querySelector("p")!.textContent).toBe(
      "ABC-1 was your longest ride: 4 stations over 20 days",
    );
    expect(
      container.querySelector("svg")!.getAttribute("aria-label"),
    ).toContain("Longest ride ABC-1");
    expect(container.querySelector("title")).toBeNull();
    const g = container.querySelector("g.sx-metro-mark")!;
    expect(g.getAttribute("data-stip")).toContain("ABC-1 summary");
    expect(g.getAttribute("aria-label")).toBe("ABC-1, 10h");
    expect(container.querySelectorAll("circle[data-stip]").length).toBe(7);
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("single ticket, single day, range of one day", () => {
    const { container } = render(
      <TicketMetro
        tickets={[tk("X-1", ["2026-10-01"], 1e9)]}
        from="2026-10-01"
        to="2026-10-01"
      />,
    );
    expect(container.querySelector("p")!.textContent).toContain(
      "1 station over 1 day",
    );
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("caps at 10 lines", () => {
    const many = Array.from({ length: 14 }, (_, i) =>
      tk(`T-${i}`, ["2026-10-01", "2026-10-02"], 100 + i),
    );
    expect(layoutLines(many, "2026-10-01", 5).lines.length).toBe(10);
  });
  it("empty state", () => {
    const { container } = render(
      <TicketMetro tickets={[]} from="2026-10-01" to="2026-10-31" />,
    );
    expect(container.textContent).toContain("nothing built yet");
    expect(metroAria("", "", [], null)).toContain("nothing built yet");
  });
  it("bad range is empty, not a crash", () => {
    const { container } = render(
      <TicketMetro tickets={tickets} from="2026-10-31" to="2026-10-01" />,
    );
    expect(container.textContent).toContain("nothing built yet");
  });
});

describe("tips", () => {
  const t = tk("ABC-1", ["2026-10-01", "2026-10-02", "2026-10-03", "2026-10-20"], 36000);
  it("lineTip gives exact rows and a longest-ride note", () => {
    const tip = lineTip(t, 36000, 31, true, "red");
    expect(tip.title).toBe("ABC-1");
    expect(tip.sub).toBe("ABC-1 summary");
    expect(tip.rows).toEqual([
      ["Hours", "10h"],
      ["Blocks", "4"],
      ["Days active", "4 of 31"],
      ["First day", "Thu 1 Oct"],
      ["Last day", "Tue 20 Oct"],
      ["Span", "20 days"],
      ["Per active day", "2h 30m"],
    ]);
    expect(tip.bar!.label).toBe("the biggest ticket");
    expect(tip.note).toBe("The longest ride on the map: 4 stations over 20 days.");
  });
  it("lineTip edge cases: single day, no summary, zero top", () => {
    const one = tk("X-1", ["2026-10-05"], 1800, { summary: null });
    const tip = lineTip(one, 36000, 31, false);
    expect(tip.sub).toBe("no summary");
    expect(tip.note).toBe("A one-stop sprint: all of it in a single day.");
    expect(tip.bar!.label).toBe("5% of the biggest ticket");
    expect(lineTip(one, 0, 31, false).bar!.label).toBe("0% of the biggest ticket");
    expect(lineTip(t, 36000, 31, false).note).toBe("Rode it 4 days out of 31.");
    expect(JSON.stringify(lineTip(tk("H-1", ["2026-10-01"], 1e12), 1e12, 1, false))).not.toContain("NaN");
  });
  it("stationTip names the day, position and termini", () => {
    expect(stationTip("ABC-1", t.days, 0).rows).toEqual([["Station", "1 of 4"], ["Since previous", "none, this is the start"]]);
    const last = stationTip("ABC-1", t.days, 3);
    expect(last.title).toBe("ABC-1 · Tue 20 Oct");
    expect(last.rows![1]).toEqual(["Since previous", "17 days earlier"]);
    expect(last.sub).toBe("last station");
    expect(last.note).toBe("Terminus: an end of the line.");
    expect(stationTip("ABC-1", t.days, 1).rows![1][1]).toBe("the day before");
    expect(stationTip("A", ["2026-10-01"], 0).note).toContain("one stop");
    expect(stationTip("A", ["2026-10-01", "2026-10-02", "2026-10-20"], 2).note).toBe("Terminus: an end of the line.");
    expect(stationTip("A", ["2026-10-01", "2026-10-02", "2026-10-20", "2026-10-21"], 2).note).toBe("Back after a week or more off.");
  });
});
