import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { Block } from "@/lib/types";
import { WeekGrid } from "./WeekGrid";

afterEach(cleanup);

const blk = (id: number, start: string, end: string, over: Partial<Block> = {}): Block =>
  ({
    id,
    day: "2026-09-21",
    jira_issue: null,
    started_at: start,
    ended_at: end,
    duration_seconds: (Date.parse(end) - Date.parse(start)) / 1000,
    description: null,
    tempo_worklog_id: null,
    is_personal: false,
    dirty: false,
    ...over,
  }) as Block;

const grid = (blocks: Block[]) =>
  render(<WeekGrid days={[{ day: "2026-09-21", blocks, totalSeconds: 0 }]} />).container;

describe("WeekGrid block labels", () => {
  const s = "2026-09-21T10:00:00";
  const e = "2026-09-21T11:00:00";

  it("no-ticket block shows its description, not 'No ticket'", () => {
    const c = grid([blk(1, s, e, { description: "Fix login" })]);
    const el = c.querySelector(".week-block-flagged");
    expect(el?.textContent).toBe("Fix login");
    expect(el?.querySelector("svg")).not.toBeNull();
  });

  it("falls back to 'No ticket' without a description", () => {
    expect(grid([blk(1, s, e)]).querySelector(".week-block-flagged")?.textContent).toBe("No ticket");
  });

  it("ticketed block keeps the mono key and the description row", () => {
    const c = grid([blk(1, s, e, { jira_issue: "ABC-1", description: "Fix login" })]);
    expect(c.querySelector(".week-block-ticket")?.textContent).toBe("ABC-1");
    expect(c.querySelector(".week-block-desc")?.textContent).toBe("Fix login");
    expect(c.querySelector(".week-block-flagged")).toBeNull();
  });

  it("tiny block renders no label", () => {
    const c = grid([blk(1, s, "2026-09-21T10:05:00")]);
    const el = c.querySelector(".week-block[data-tiny]");
    expect(el).not.toBeNull();
    expect(el?.textContent).toBe("");
  });
});

describe("WeekGrid overlap lanes", () => {
  it("caps at two lanes and stacks a third overlapping block", () => {
    const c = grid([
      blk(1, "2026-09-21T10:00:00", "2026-09-21T12:00:00"),
      blk(2, "2026-09-21T10:00:00", "2026-09-21T12:00:00"),
      blk(3, "2026-09-21T10:00:00", "2026-09-21T12:00:00"),
    ]);
    const bl = [...c.querySelectorAll<HTMLElement>(".week-block")];
    expect(bl.map((b) => b.dataset.lanes)).toEqual(["2", "2", "2"]);
    expect(bl.map((b) => b.style.width)).toEqual(["50%", "50%", "50%"]);
    expect(bl[2].style.left).toContain("calc(");
  });
});
