// Component-level behaviour that lib/dayStrip*.test.ts can't cover: the
// expand/collapse toggle swapping bar <-> lanes, legend hover dimming
// other projects' segments, and the rich tooltip's show/hide lifecycle.

import { afterEach, beforeAll, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { StripBlock, StripGap } from "@/lib/dayStrip";

let DayStrip: (props: { blocks: StripBlock[]; gaps: StripGap[] }) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./DayStrip");
  DayStrip = mod.DayStrip;
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

function makeBlock(overrides: Partial<StripBlock> & { id: number }): StripBlock {
  return {
    started_at: at(9, 0),
    ended_at: at(10, 0),
    project_path: "/x/alpha",
    is_personal: false,
    confidence: "high",
    description: null,
    ...overrides,
  };
}

describe("DayStrip expand toggle", () => {
  it("shows a single bar by default, swaps to one lane per project on Expand, and back on Collapse", () => {
    const blocks: StripBlock[] = [
      makeBlock({ id: 1, project_path: "/x/alpha" }),
      makeBlock({ id: 2, started_at: at(10, 30), ended_at: at(11, 0), project_path: "/x/beta" }),
    ];
    render(<DayStrip blocks={blocks} gaps={[]} />);

    expect(document.querySelector(".day-strip-track")).toBeTruthy();
    expect(document.querySelector(".day-strip-lanes")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /expand/i }));
    expect(document.querySelector(".day-strip-track")).toBeNull();
    expect(document.querySelectorAll(".day-strip-lane-track").length).toBe(2);

    fireEvent.click(screen.getByRole("button", { name: /collapse/i }));
    expect(document.querySelector(".day-strip-lanes")).toBeNull();
    expect(document.querySelector(".day-strip-track")).toBeTruthy();
  });
});

describe("DayStrip legend focus", () => {
  it("dims every other project's segments while a legend item is hovered", () => {
    const blocks: StripBlock[] = [
      makeBlock({ id: 1, project_path: "/x/alpha" }), // 1h — busiest, legend row 0
      makeBlock({ id: 2, started_at: at(10, 30), ended_at: at(11, 0), project_path: "/x/beta" }), // 30m
    ];
    render(<DayStrip blocks={blocks} gaps={[]} />);

    const legendItems = document.querySelectorAll(".day-strip-legend-item");
    expect(legendItems.length).toBe(2);

    fireEvent.mouseEnter(legendItems[0]);
    const opacities = Array.from(document.querySelectorAll(".day-strip-block")).map(
      (s) => (s as HTMLElement).style.opacity,
    );
    expect(opacities).toContain("0.25");
    expect(opacities.some((o) => o !== "0.25")).toBe(true);

    fireEvent.mouseLeave(legendItems[0]);
    const after = Array.from(document.querySelectorAll(".day-strip-block")).map(
      (s) => (s as HTMLElement).style.opacity,
    );
    expect(after.every((o) => o !== "0.25")).toBe(true);
  });
});

describe("DayStrip tooltip", () => {
  it("shows the description on hover and on focus, hides on leave/blur/Escape", () => {
    const blocks: StripBlock[] = [
      makeBlock({ id: 1, project_path: "/x/alpha", description: "Review PR #12" }),
    ];
    render(<DayStrip blocks={blocks} gaps={[]} />);
    const seg = document.querySelector(".day-strip-block") as HTMLElement;

    expect(document.querySelector(".day-strip-tooltip")).toBeNull();

    fireEvent.mouseEnter(seg);
    expect(screen.getByText("Review PR #12")).toBeTruthy();
    fireEvent.mouseLeave(seg);
    expect(document.querySelector(".day-strip-tooltip")).toBeNull();

    fireEvent.focus(seg);
    expect(screen.getByText("Review PR #12")).toBeTruthy();
    fireEvent.keyDown(seg, { key: "Escape" });
    expect(document.querySelector(".day-strip-tooltip")).toBeNull();

    fireEvent.focus(seg);
    expect(screen.getByText("Review PR #12")).toBeTruthy();
    fireEvent.blur(seg);
    expect(document.querySelector(".day-strip-tooltip")).toBeNull();
  });
});
