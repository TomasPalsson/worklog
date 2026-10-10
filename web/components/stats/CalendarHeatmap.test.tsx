import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { CalendarHeatmap, heatZoom, layout, level } from "./CalendarHeatmap";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("level", () => {
  it("buckets into 5 steps", () => {
    expect([0, 1, 50, 51, 75, 76, 100].map((s) => level(s, 100))).toEqual([0, 1, 2, 3, 3, 4, 4]);
    expect(level(5, 0)).toBe(0);
  });
});

describe("layout", () => {
  it("puts Mon on row 0 and starts a new column each Monday", () => {
    const days = ["07", "08", "09", "10", "11", "12"].map((d) => day({ day: `2026-10-${d}` }));
    const l = layout(days);
    expect([l[0], l[4], l[5]].map((c) => [c.col, c.row])).toEqual([[0, 2], [0, 6], [1, 0]]);
  });
});

describe("CalendarHeatmap", () => {
  const daily = [day({ day: "2026-10-06", work_seconds: 27000 }), day({ day: "2026-10-07" })];
  it("tips cells (no native title), focusable only when worked, and outlines today", () => {
    const { container } = render(<CalendarHeatmap daily={daily} today="2026-10-07" />);
    expect(container.querySelectorAll("title").length).toBe(0);
    const tips = [...container.querySelectorAll("g[data-stip]")];
    expect(tips.length).toBe(2);
    expect(JSON.parse(tips[0].getAttribute("data-stip")!).title).toBe("Tue 6 Oct");
    expect(tips[0].getAttribute("tabindex")).toBe("0");
    expect(tips[0].getAttribute("aria-label")).toBe("Tue 6 Oct: 7h 30m work");
    expect(tips[1].getAttribute("tabindex")).toBeNull();
    expect(container.querySelectorAll(".st-today").length).toBe(1);
  });
  it("empty state for zeros and empty array", () => {
    expect(render(<CalendarHeatmap daily={[]} />).container.textContent).toContain("No data yet");
    cleanup();
    const { container } = render(<CalendarHeatmap daily={[day({ day: "2026-10-06" })]} />);
    expect(container.textContent).toContain("No data yet");
    expect(container.innerHTML).not.toContain("NaN");
  });
});

describe("heatZoom", () => {
  it("grows short ranges up to 3x and leaves a year at 1x", () => {
    expect(heatZoom(71)).toBe(3); // three weeks would be a 71px strip
    expect(heatZoom(210)).toBe(2);
    expect(heatZoom(806)).toBe(1); // a year of columns
    expect(heatZoom(0)).toBe(1);
  });
});
