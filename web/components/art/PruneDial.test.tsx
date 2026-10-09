import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { PruneDial, parseDay, sectorPath, windows } from "./PruneDial";

afterEach(cleanup);

describe("prune dial maths", () => {
  it("parses days strictly", () => {
    expect(parseDay("20")).toBe(20);
    expect(parseDay(" 5 ")).toBe(5);
    for (const bad of ["", "0", "32", "1.5", "x", "-3"]) expect(parseDay(bad)).toBeNull();
  });
  it("derives windows (defaults 20 / 23)", () => {
    expect(windows(20, 23, 9)).toEqual({ cycle: 21, editable: 4, prunable: 27 });
  });
  it("cycle arc runs start..today, wrapping past day 31", () => {
    expect(windows(20, 23, 25).cycle).toBe(6);
    expect(windows(20, 23, 20).cycle).toBe(1);
    expect(windows(20, 23, 19).cycle).toBe(31);
  });
  it("draws no editable/prunable window when close is before start", () => {
    expect(windows(20, 5, 9)).toEqual({ cycle: 21, editable: 0, prunable: 0 });
  });
  it("draws nothing it cannot derive", () => {
    expect(windows(null, 23, 9)).toEqual({ cycle: 0, editable: 0, prunable: 0 });
    expect(windows(20, null, 9).editable).toBe(0);
    expect(sectorPath(1, 0, 10, 20)).toBe("");
  });
});

describe("<PruneDial />", () => {
  it("states the windows in its label and redraws from props", () => {
    const { getByRole, container, rerender } = render(
      <PruneDial enabled startDay="20" closeDay="23" today={9} />,
    );
    const label = getByRole("img").getAttribute("aria-label")!;
    expect(label).toContain("Pruning on");
    expect(label).toContain("editable for 4 days");
    expect(label).toContain("pruning for 27 days");
    expect(container.querySelectorAll("path.art-fade").length).toBe(3);
    rerender(<PruneDial enabled startDay="" closeDay="23" today={9} />);
    expect(container.querySelectorAll("path.art-fade").length).toBe(0);
  });
  it("greys out and says off when pruning is disabled", () => {
    const { container } = render(<PruneDial enabled={false} startDay="20" closeDay="23" today={9} />);
    expect(container.querySelector(".art-settings-dialwrap")!.hasAttribute("data-off")).toBe(true);
    expect(container.textContent).toContain("off");
  });
  it("gives two dials distinct pattern ids", () => {
    const { container } = render(
      <>
        <PruneDial enabled startDay="20" closeDay="23" today={1} />
        <PruneDial enabled startDay="20" closeDay="23" today={1} />
      </>,
    );
    const ids = [...container.querySelectorAll("pattern")].map((p) => p.id);
    expect(new Set(ids).size).toBe(2);
  });
});
