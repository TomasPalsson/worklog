import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { cityAria, cityHeadline, floorsFor, gardenShare, layoutCity, plotTitle, towerHeight, windowCount, windowSlot, WorkdayCity } from "./WorkdayCity";
import { day } from "./time-fixtures";

afterEach(cleanup);

// Mon 21 Sep .. Sun 27 Sep 2026, then Mon 28
const daily = [
  day({ day: "2026-09-21", work_seconds: 3600, prompts: 30 }),
  day({ day: "2026-09-22", work_seconds: 40800, prompts: 198, personal_seconds: 3600 }),
  day({ day: "2026-09-23" }),
  day({ day: "2026-09-24" }),
  day({ day: "2026-09-25" }),
  day({ day: "2026-09-26" }),
  day({ day: "2026-09-27", work_seconds: 600 }),
  day({ day: "2026-09-28", work_seconds: 7200 }),
];

describe("helpers", () => {
  it("height scales to 9 with a visible minimum", () => {
    expect(towerHeight(100, 100)).toBe(9);
    expect(towerHeight(1, 1e9)).toBe(0.3);
    expect(towerHeight(0, 100)).toBe(0);
    expect(towerHeight(5, 0)).toBe(0);
  });
  it("windows are capped to what fits", () => {
    expect(windowCount(198, 9)).toBe(20);
    expect(windowCount(100000, 9)).toBe(4 * floorsFor(9));
    expect(windowCount(100, 0.3)).toBe(0);
    expect(windowCount(0, 9)).toBe(0);
  });
  it("window slots alternate faces and climb", () => {
    expect(windowSlot(0).face).toBe("left");
    expect(windowSlot(1).face).toBe("right");
    expect(windowSlot(4).z).toBeGreaterThan(windowSlot(0).z);
  });
  it("garden share", () => {
    expect(gardenShare(3600, 3600)).toBe(0.5);
    expect(gardenShare(0, 0)).toBe(0);
  });
});

describe("layoutCity", () => {
  const plots = layoutCity(daily, "2026-09-28");
  it("grids by weekday and week", () => {
    expect(plots[1]).toMatchObject({ col: 1, row: 0, busiest: true });
    expect(plots[7]).toMatchObject({ col: 0, row: 1, today: true });
  });
  it("classifies weekend park vs weekday lot", () => {
    expect(plots[2].kind).toBe("lot");
    expect(plots[5].kind).toBe("park");
    expect(plots[6].kind).toBe("tower");
  });
  it("keeps only the latest 13 weeks", () => {
    const long = Array.from({ length: 140 }, (_, i) =>
      day({ day: new Date(Date.UTC(2026, 0, 5 + i)).toISOString().slice(0, 10), work_seconds: 60 }),
    );
    expect(Math.max(...layoutCity(long, "x").map((p) => p.row))).toBe(12);
  });
  it("headline and title use exact numbers", () => {
    expect(cityHeadline(plots)).toContain("Tuesday 22 Sep is your skyscraper: 11h 20m");
    expect(plotTitle(plots[1])).toBe("Tue 22 Sep · 11h 20m work · 198 prompts · 1h personal");
  });
});

describe("WorkdayCity", () => {
  it("renders aria and no NaN", () => {
    const { container } = render(<WorkdayCity daily={daily} today="2026-09-28" />);
    const label = container.querySelector("svg")!.getAttribute("aria-label")!;
    expect(label).toContain("Tallest is Tue 22 Sep at 11h 20m");
    expect(label).toBe(cityAria(layoutCity(daily, "2026-09-28")));
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.textContent).toContain("trees = days off");
  });
  it("empty state", () => {
    const { container } = render(<WorkdayCity daily={[]} today="2026-09-28" />);
    expect(container.textContent).toContain("nothing built yet");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("no-work days are an empty city", () => {
    const { container } = render(<WorkdayCity daily={[day({ day: "2026-09-26" })]} today="2026-09-26" />);
    expect(container.textContent).toContain("nothing built yet");
  });
  it("single huge day", () => {
    const { container } = render(<WorkdayCity daily={[day({ day: "2026-09-22", work_seconds: 1e9, prompts: 1e7 })]} today="2026-09-22" />);
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.textContent).toContain("skyscraper");
  });
});
