import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { arcPath, SplitDonut } from "./SplitDonut";

afterEach(cleanup);

describe("SplitDonut", () => {
  it("shows biggest share, legend percentages and aria", () => {
    const { getByRole, getByText, container } = render(
      <SplitDonut title="Origin" unit="count" rows={[{ label: "event", value: 3 }, { label: "none", value: 1 }]} />,
    );
    expect(getByText("event takes 75%")).toBeTruthy();
    expect(getByRole("img").getAttribute("aria-label")).toBe("Origin: event 75%, none 25%");
    expect(container.querySelectorAll(".sr-leg").length).toBe(2);
    expect(container.querySelector("[data-hatch]")).toBeTruthy();
    expect(container.querySelectorAll("path.sr-seg").length).toBe(2);
  });

  it("a single 100% row still draws a full ring", () => {
    const { container } = render(<SplitDonut title="T" unit="seconds" rows={[{ label: "a", value: 7200 }]} />);
    expect(container.querySelectorAll("path.sr-seg").length).toBe(2);
    expect(container.textContent).toContain("2h");
  });

  it("zero total and empty rows render no data, no NaN", () => {
    for (const rows of [[], [{ label: "a", value: 0 }]]) {
      const { container, getByRole } = render(<SplitDonut title="T" unit="count" rows={rows} />);
      expect(getByRole("img").getAttribute("aria-label")).toBe("T: no data");
      expect(container.querySelectorAll("path.sr-seg").length).toBe(0);
      expect(container.innerHTML).not.toContain("NaN");
      cleanup();
    }
  });

  it("a tiny share never draws its arc backwards", () => {
    const { container } = render(<SplitDonut title="T" unit="count" rows={[{ label: "big", value: 999 }, { label: "tiny", value: 1 }]} />);
    const ang = (x: number, y: number) => (Math.atan2(x - 50, 50 - y) + 2 * Math.PI) % (2 * Math.PI);
    for (const p of container.querySelectorAll("path.sr-seg")) {
      const n = (p.getAttribute("d") ?? "").match(/-?\d+\.\d+/g)!.map(Number);
      const a0 = ang(n[0], n[1]);
      const a1 = ang(n[n.length - 2], n[n.length - 1]);
      expect(a1 >= a0 - 1e-6 || a1 < 0.01).toBe(true); // end at 12 o'clock wraps to ~0 or 2pi
    }
  });

  it("arcPath uses the large-arc flag past 180 degrees", () => {
    expect(arcPath(0, 1)).toContain(" 0 0 1 ");
    expect(arcPath(0, 4)).toContain(" 0 1 1 ");
  });
});
