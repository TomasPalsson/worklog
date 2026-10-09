import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { ActivityStream, bandPath, buildLayers } from "./ActivityStream";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("bandPath", () => {
  it("is a closed path without NaN for flat and spiky data", () => {
    const p = bandPath([[0, 5], [10, 5], [20, 1], [30, 9]], [[0, 10], [10, 10], [20, 10], [30, 10]]);
    expect(p.startsWith("M0.00 5.00")).toBe(true);
    expect(p.endsWith("Z")).toBe(true);
    expect(p).not.toContain("NaN");
  });
});

describe("buildLayers", () => {
  const daily = [
    day({ day: "2026-10-05", prompts: 10, commits: 1 }),
    day({ day: "2026-10-06", prompts: 40 }),
    day({ day: "2026-10-07", prompts: 20, commits: 3 }),
  ];
  it("drops all-zero series and records totals and peaks", () => {
    const l = buildLayers(daily);
    expect(l.map((x) => x.label)).toEqual(["prompts", "commits"]);
    expect(l[0].total).toBe(70);
    expect(l[0].peak).toBe(40);
    expect(l[0].peakDay).toBe("2026-10-06");
  });
  it("supports a single day", () => {
    const l = buildLayers([daily[0]]);
    expect(l.length).toBe(2);
    expect(l.every((x) => !x.path.includes("NaN"))).toBe(true);
  });
});

describe("ActivityStream", () => {
  it("renders legend totals, aria and the caption", () => {
    const { container, getByText } = render(
      <ActivityStream daily={[day({ day: "2026-10-05", prompts: 1200 }), day({ day: "2026-10-06", prompts: 3 })]} />,
    );
    expect(getByText("1,203")).toBeTruthy();
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("prompts 1,203");
    expect(container.textContent).toContain("Shape, not size");
  });
  it("empty state", () => {
    expect(render(<ActivityStream daily={[]} />).container.textContent).toContain("No data yet");
  });
});
