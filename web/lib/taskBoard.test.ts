import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { columnOf, COLUMNS, formatStamp, movesInto, ticketCount, transitionLabel, weekTotal } from "./taskBoard";
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
