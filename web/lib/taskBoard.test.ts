import { describe, expect, it } from "bun:test";
import { columnOf, COLUMNS, formatStamp, movesInto, weekTotal } from "./taskBoard";
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
    expect(COLUMNS.map((c) => c.title)).toEqual(["To do", "In progress", "Done"]);
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

  it("formatStamp renders d MMM, HH:MM and passes garbage through", () => {
    expect(formatStamp("2026-09-05T08:07:00")).toBe("5 Sep, 08:07");
    expect(formatStamp("2026-09-05T08:07:00.000+0000")).toMatch(/^\d{1,2} Sep, \d\d:\d\d$/);
    expect(formatStamp("nope")).toBe("nope");
  });
});
