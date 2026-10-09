import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { CoverageRing, railSegments } from "./CoverageRing";

afterEach(cleanup);

const g = (a: string, b: string, minutes = 30) => ({
  started_at: `2026-10-06T${a}:00Z`,
  ended_at: `2026-10-06T${b}:00Z`,
  minutes,
});

describe("railSegments", () => {
  it("positions gaps within earliest..latest", () => {
    const s = railSegments([g("10:00", "10:30"), g("11:30", "12:00")]);
    expect(s[0]).toEqual({ left: 0, width: 25 });
    expect(s[1]).toEqual({ left: 75, width: 25 });
  });
  it("is empty with no usable gaps", () => {
    expect(railSegments([])).toEqual([]);
    expect(railSegments([g("10:00", "10:00")])).toEqual([]);
  });
});

describe("CoverageRing", () => {
  it("labels partial coverage with gaps and held back", () => {
    const { container } = render(<CoverageRing percent={82} gaps={[g("10:00", "10:30")]} heldBack={1} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("82% covered, 1 gap, 1 held back");
    expect(container.querySelector(".art-ring-check")).toBeNull();
    expect(container.querySelectorAll(".art-ring-gap").length).toBe(1);
  });
  it("closes with a check and rays when fully covered", () => {
    const { container } = render(<CoverageRing percent={100} gaps={[]} heldBack={0} />);
    expect(container.querySelector(".art-ring-check")).not.toBeNull();
    expect(container.querySelectorAll(".art-ring-ray").length).toBe(6);
  });
  it("does not celebrate when held back, and clamps over 100", () => {
    const { container } = render(<CoverageRing percent={140} gaps={[]} heldBack={2} />);
    expect(container.querySelector(".art-ring-check")).toBeNull();
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("100% covered, 0 gaps, 2 held back");
  });
  it("draws no arc at 0%", () => {
    const { container } = render(<CoverageRing percent={0} gaps={[]} heldBack={0} />);
    expect(container.querySelector(".art-ring-arc")).toBeNull();
  });
});
