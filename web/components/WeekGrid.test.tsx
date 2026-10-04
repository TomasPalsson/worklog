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

  it("ticketed block keeps the mono key and lists its description", () => {
    const c = grid([blk(1, s, e, { jira_issue: "ABC-1", description: "Fix login" })]);
    expect(c.querySelector(".week-block-ticket")?.textContent).toBe("ABC-1");
    expect(c.querySelector(".week-block-act-text")?.textContent).toBe("Fix login");
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
      blk(1, "2026-09-21T10:00:00", "2026-09-21T12:00:00", { jira_issue: "A-1" }),
      blk(2, "2026-09-21T10:00:00", "2026-09-21T12:00:00", { jira_issue: "A-2" }),
      blk(3, "2026-09-21T10:00:00", "2026-09-21T12:00:00", { jira_issue: "A-3" }),
    ]);
    const bl = [...c.querySelectorAll<HTMLElement>(".week-block")];
    expect(bl.map((b) => b.dataset.lanes)).toEqual(["2", "2", "2"]);
    expect(bl.map((b) => b.style.width)).toEqual(["50%", "50%", "50%"]);
    expect(bl[2].style.left).toContain("calc(");
  });
});

describe("WeekGrid runs", () => {
  it("merges a run of 3 into one element with combined duration and count", () => {
    const c = grid([
      blk(1, "2026-09-21T10:00:00", "2026-09-21T10:20:00", { jira_issue: "ABC-1", description: "a" }),
      blk(2, "2026-09-21T10:25:00", "2026-09-21T10:45:00", { jira_issue: "ABC-1" }),
      blk(3, "2026-09-21T10:50:00", "2026-09-21T11:00:00", { jira_issue: "ABC-1" }),
    ]);
    const bl = c.querySelectorAll<HTMLElement>(".week-block");
    expect(bl.length).toBe(1);
    expect(bl[0].querySelector(".week-block-meta")?.textContent).toBe("50m · 3 blocks");
    expect(bl[0].getAttribute("aria-label")).toContain("across 3 blocks");
    expect(bl[0].title.split("\n").length).toBe(4);
  });

  it("tall run: meta on its own row, activities ordered by duration", () => {
    const c = grid([
      blk(1, "2026-09-21T09:00:00", "2026-09-21T09:20:00", { jira_issue: "ABC-1", description: "short" }),
      blk(2, "2026-09-21T09:20:00", "2026-09-21T10:20:00", { jira_issue: "ABC-1", description: "long" }),
      blk(3, "2026-09-21T10:20:00", "2026-09-21T10:30:00", { jira_issue: "ABC-1", description: "short" }),
    ]);
    const el = c.querySelector(".week-block")!;
    expect(el.querySelector(".week-block-row .week-block-dur")).toBeNull();
    expect(el.querySelector(".week-block-row")?.textContent).toBe("ABC-1");
    expect(el.querySelector(".week-block-meta")?.textContent).toBe("1h 30m · 3 blocks");
    expect([...el.querySelectorAll(".week-block-desc")].map((d) => d.textContent)).toEqual(["1hlong", "30mshort"]);
  });

  it("short run: one row with duration, no count", () => {
    const c = grid([
      blk(1, "2026-09-21T10:00:00", "2026-09-21T10:15:00", { jira_issue: "ABC-1" }),
      blk(2, "2026-09-21T10:15:00", "2026-09-21T10:30:00", { jira_issue: "ABC-1" }),
    ]);
    const el = c.querySelector(".week-block")!;
    expect(el.querySelector(".week-block-row")?.textContent).toBe("ABC-130m");
    expect(el.querySelector(".week-block-meta")).toBeNull();
    expect(el.textContent).not.toContain("blocks");
  });
});
