import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import {
  columnOf,
  COLUMNS,
  dueState,
  formatStamp,
  movesInto,
  initials,
  relativeAge,
  relativeWords,
  shortDate,
  sparkHeights,
  ticketCount,
  transitionLabel,
  weekMax,
  weekTotal,
} from "./taskBoard";
import type { TaskRow, Transition } from "./types";

const t = (id: string, to_category: Transition["to_category"]): Transition => ({
  id,
  name: id,
  to_status: id,
  to_category,
});

describe("taskBoard", () => {
  it("columnOf maps a category to its column and null to To do", () => {
    expect(columnOf("done")).toBe("done");
    expect(columnOf("indeterminate")).toBe("indeterminate");
    expect(columnOf("new")).toBe("new");
    expect(columnOf(null)).toBe("new");
    expect(COLUMNS.map((c) => c.title)).toEqual(["To Do", "In Progress", "Done"]);
  });

  it("movesInto keeps matching transitions and never matches a null category", () => {
    const all = [t("a", "done"), t("b", null), t("c", "done"), t("d", "new")];
    expect(movesInto(all, "done").map((x) => x.id)).toEqual(["a", "c"]);
    expect(movesInto([t("b", null)], "new")).toEqual([]);
  });

  it("weekTotal sums week_seconds", () => {
    const rows = [{ week_seconds: 60 }, { week_seconds: 90 }] as TaskRow[];
    expect(weekTotal(rows)).toBe(150);
    expect(weekTotal([])).toBe(0);
  });

  it("transitionLabel shows only the name when it equals the target status", () => {
    const mk = (name: string, to_status: string): Transition => ({ id: "1", name, to_status, to_category: "done" });
    expect(transitionLabel(mk("Done", "Done"))).toBe("Done");
    expect(transitionLabel(mk("done", "Done"))).toBe("done");
    expect(transitionLabel(mk("Finish", "Done"))).toBe("Finish → Done");
  });

  it("ticketCount pluralises", () => {
    expect(ticketCount(1)).toBe("1 ticket");
    expect(ticketCount(0)).toBe("0 tickets");
    expect(ticketCount(3)).toBe("3 tickets");
  });

  it("the tasks page does not nest a <main> inside the layout's <main>", () => {
    const root = import.meta.dir + "/..";
    expect(readFileSync(`${root}/app/layout.tsx`, "utf8")).toContain("<main");
    expect(readFileSync(`${root}/app/tasks/page.tsx`, "utf8")).not.toContain("<main");
  });

  it("formatStamp renders d MMM, HH:MM and passes garbage through", () => {
    expect(formatStamp("2026-09-05T08:07:00")).toBe("5 Sep, 08:07");
    expect(formatStamp("2026-09-05T08:07:00.000+0000")).toMatch(/^\d{1,2} Sep, \d\d:\d\d$/);
    expect(formatStamp("nope")).toBe("nope");
  });
});

describe("card helpers", () => {
  it("dueState classifies against today", () => {
    expect(dueState("2026-10-01", "2026-10-03")).toEqual({ state: "overdue", days: -2 });
    expect(dueState("2026-10-03", "2026-10-03")).toEqual({ state: "today", days: 0 });
    expect(dueState("2026-10-05", "2026-10-03").state).toBe("soon");
    expect(dueState("2026-10-06", "2026-10-03").state).toBe("later");
    expect(dueState("2026-11-01", "2026-10-30")).toEqual({ state: "soon", days: 2 });
  });

  it("shortDate renders d MMM", () => {
    expect(shortDate("2026-10-04")).toBe("4 Oct");
  });

  it("relativeAge buckets and tolerates Jira offsets", () => {
    const now = new Date("2026-10-02T12:00:00Z");
    expect(relativeAge("2026-10-02T11:59:30Z", now)).toBe("just now");
    expect(relativeAge("2026-10-02T11:15:00Z", now)).toBe("45m ago");
    expect(relativeAge("2026-10-02T09:00:00Z", now)).toBe("3h ago");
    expect(relativeAge("2026-09-30T12:00:00.000+0000", now)).toBe("2d ago");
    expect(relativeAge("nope", now)).toBe("");
  });

  it("relativeWords spells the age out and tolerates Jira offsets", () => {
    const now = new Date("2026-10-02T12:00:00Z");
    expect(relativeWords("2026-10-02T11:59:30Z", now)).toBe("just now");
    expect(relativeWords("2026-10-02T11:59:00Z", now)).toBe("1 minute ago");
    expect(relativeWords("2026-10-02T11:15:00Z", now)).toBe("45 minutes ago");
    expect(relativeWords("2026-10-02T09:00:00Z", now)).toBe("3 hours ago");
    expect(relativeWords("2026-09-30T12:00:00.000+0000", now)).toBe("2 days ago");
    expect(relativeWords("nope", now)).toBe("");
  });

  it("initials takes up to two words and never comes back empty", () => {
    expect(initials("Grace Hopper")).toBe("GH");
    expect(initials("Linus")).toBe("L");
    expect(initials("ada king lovelace")).toBe("AK");
    expect(initials("  ")).toBe("?");
  });

  it("sparkHeights scales to the max, floors at 2px", () => {
    expect(sparkHeights([0, 3600, 7200, 1], 7200)).toEqual([2, 8, 16, 2]);
    expect(sparkHeights([0, 0], 1)).toEqual([2, 2]);
    expect(sparkHeights([100], 100, 20)).toEqual([20]);
  });

  it("weekMax is the biggest day, at least 1", () => {
    const rows = [{ day_seconds: [0, 5, 0, 0, 0, 0, 0] }, { day_seconds: [9, 0, 0, 0, 0, 0, 0] }] as TaskRow[];
    expect(weekMax(rows)).toBe(9);
    expect(weekMax([])).toBe(1);
    expect(weekMax([{ day_seconds: [0, 0, 0, 0, 0, 0, 0] }] as TaskRow[])).toBe(1);
  });
});
