import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { RoundingRail, railLabel, railMath } from "./RoundingRail";

afterEach(cleanup);

describe("railMath", () => {
  it("scale is at least 1h and rounds up to the next half hour", () => {
    expect(railMath(0, 0).max).toBe(3600);
    expect(railMath(7800, 9000).max).toBe(9000);
    expect(railMath(7801, 5400).max).toBe(9000);
  });
  it("widths are proportional; zero stays zero", () => {
    const m = railMath(1800, 3600);
    expect(m.trackedW).toBe(m.billedW / 2);
    expect(railMath(0, 0).trackedW).toBe(0);
  });
  it("billed over scale never exceeds the rail", () => {
    expect(railMath(100, 99999).billedW).toBeLessThanOrEqual(60);
  });
});

describe("railLabel", () => {
  it("rounded up", () => expect(railLabel(7800, 9000, false)).toBe("Tracked 2h 10m, billed 2h 30m, rounded up 20m"));
  it("rounded down", () => expect(railLabel(5400, 3600, false)).toBe("Tracked 1h 30m, billed 1h, rounded down 30m"));
  it("by hand", () => expect(railLabel(1800, 7200, true)).toContain("set by hand"));
  it("equal", () => expect(railLabel(3600, 3600, false)).toBe("Tracked 1h, billed 1h"));
});

describe("RoundingRail", () => {
  it("is an img with the label; pencil only when overridden", () => {
    const { container, rerender } = render(<RoundingRail tracked={7800} billed={9000} overridden={false} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("rounded up 20m");
    expect(container.querySelector(".art-rail-pencil")).toBeNull();
    rerender(<RoundingRail tracked={7800} billed={9000} overridden />);
    expect(container.querySelector(".art-rail-pencil")).not.toBeNull();
  });
  it("hatches the cut part only when billed < tracked, with a unique pattern id", () => {
    const { container } = render(
      <>
        <RoundingRail tracked={5400} billed={3600} overridden />
        <RoundingRail tracked={3600} billed={5400} overridden={false} />
      </>,
    );
    expect(container.querySelectorAll(".art-rail-cut").length).toBe(1);
    const ids = [...container.querySelectorAll("pattern")].map((p) => p.id);
    expect(new Set(ids).size).toBe(2);
  });
});
