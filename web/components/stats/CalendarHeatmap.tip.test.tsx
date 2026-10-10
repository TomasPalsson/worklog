import { describe, expect, it } from "bun:test";
import { cellTip } from "./CalendarHeatmap";
import { day } from "./time-fixtures";

describe("cellTip", () => {
  const all = [
    day({ day: "2026-10-05", work_seconds: 3600, prompts: 1200 }),
    day({ day: "2026-10-06", work_seconds: 10800, personal_seconds: 1800 }),
    day({ day: "2026-10-07", personal_seconds: 600 }),
    day({ day: "2026-10-08" }),
  ];
  it("busiest day", () => {
    const t = cellTip(all[1], all, 10800);
    expect(t.title).toBe("Tue 6 Oct");
    expect(t.sub).toBe("1st busiest of 2 worked days · Tuesday");
    expect(t.rows).toEqual([["Work", "3h"], ["Personal", "30m"], ["Prompts", "0"], ["Shade", "5 of 5"]]);
    expect(t.note).toBe("Your busiest day in this range.");
    expect(t.bar).toEqual({ value: 10800, max: 10800, label: "100% of your busiest day (3h)" });
  });
  it("lighter day compares with the worked-day average", () => {
    const t = cellTip(all[0], all, 10800);
    expect(t.sub).toBe("2nd busiest of 2 worked days · Monday");
    expect(t.rows![2]).toEqual(["Prompts", "1,200"]);
    expect(t.note).toBe("0.5× your average worked day.");
  });
  it("zero days: personal-only and empty", () => {
    expect(cellTip(all[2], all, 10800).note).toBe("No work, but personal time was logged.");
    const t = cellTip(all[3], all, 10800);
    expect(t.sub).toBe("Thursday · no work");
    expect(t.note).toBe("Nothing logged: a proper day off.");
    expect(t.rows![0]).toEqual(["Work", "none"]);
  });
});
