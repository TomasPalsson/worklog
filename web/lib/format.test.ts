import { describe, expect, it } from "bun:test";
import {
  formatDuration,
  formatRange,
  formatTotalHours,
  formatWeekRange,
  mondayOf,
  shiftDay,
  shiftWeek,
  shortMonthDay,
  shortWeekday,
  todayISO,
  weekDays,
} from "./format";

describe("formatDuration", () => {
  it("shows just minutes under an hour", () => {
    expect(formatDuration(1800)).toBe("30m");
    expect(formatDuration(60)).toBe("1m");
  });

  it("shows hours only on whole-hour boundaries", () => {
    expect(formatDuration(3600)).toBe("1h");
    expect(formatDuration(7200)).toBe("2h");
  });

  it("shows h + m for mixed", () => {
    expect(formatDuration(3900)).toBe("1h 5m");
    expect(formatDuration(9900)).toBe("2h 45m");
  });

  it("rounds seconds to nearest minute", () => {
    expect(formatDuration(29)).toBe("0m");
    expect(formatDuration(31)).toBe("1m");
  });
});

describe("formatTotalHours", () => {
  it("shows one decimal", () => {
    expect(formatTotalHours(3600)).toBe("1.0h");
    expect(formatTotalHours(5400)).toBe("1.5h");
    expect(formatTotalHours(9000)).toBe("2.5h");
  });
});

describe("shiftDay", () => {
  it("moves forward and backward", () => {
    expect(shiftDay("2026-04-18", 1)).toBe("2026-04-19");
    expect(shiftDay("2026-04-18", -1)).toBe("2026-04-17");
  });

  it("handles month boundaries", () => {
    expect(shiftDay("2026-04-30", 1)).toBe("2026-05-01");
    expect(shiftDay("2026-05-01", -1)).toBe("2026-04-30");
  });

  it("handles year boundaries", () => {
    expect(shiftDay("2026-12-31", 1)).toBe("2027-01-01");
  });
});

describe("todayISO", () => {
  it("returns a YYYY-MM-DD string", () => {
    expect(todayISO()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("formatRange", () => {
  it("joins local clock times with an en dash", () => {
    const v = formatRange(
      "2026-04-18T09:00:00Z",
      "2026-04-18T09:30:00Z",
    );
    expect(v).toMatch(/^\d{2}:\d{2}–\d{2}:\d{2}$/);
  });
});

describe("mondayOf", () => {
  it("snaps every weekday in a week to the same Monday", () => {
    // 2026-05-11 is a Monday.
    for (let i = 0; i < 7; i++) {
      expect(mondayOf(shiftDay("2026-05-11", i))).toBe("2026-05-11");
    }
  });

  it("crosses month and year boundaries", () => {
    // 2026-01-03 is a Saturday; its Monday is 2025-12-29.
    expect(mondayOf("2026-01-03")).toBe("2025-12-29");
  });
});

describe("shiftWeek", () => {
  it("moves by whole weeks", () => {
    expect(shiftWeek("2026-05-11", 1)).toBe("2026-05-18");
    expect(shiftWeek("2026-05-11", -2)).toBe("2026-04-27");
  });
});

describe("weekDays", () => {
  it("returns 7 consecutive days starting at the given Monday", () => {
    expect(weekDays("2026-05-11")).toEqual([
      "2026-05-11",
      "2026-05-12",
      "2026-05-13",
      "2026-05-14",
      "2026-05-15",
      "2026-05-16",
      "2026-05-17",
    ]);
  });
});

describe("formatWeekRange", () => {
  it("uses the compact same-month form within the current year", () => {
    const cy = new Date().getFullYear();
    // Pick a definitely-same-month Monday in the current year.
    const monday = `${cy}-05-11`;
    expect(formatWeekRange(monday)).toBe("May 11–17");
  });

  it("includes both months when the week straddles a month boundary", () => {
    const cy = new Date().getFullYear();
    // Pick the Monday of a known cross-month week.
    const monday = `${cy}-04-27`; // Mon → Sun 2026-04-27..2026-05-03
    expect(formatWeekRange(monday)).toBe("Apr 27 – May 3");
  });

  it("includes the year when the week is not in the current year", () => {
    const v = formatWeekRange("2024-05-13");
    expect(v).toBe("May 13–19, 2024");
  });

  it("spans years when the week crosses a year boundary", () => {
    // Mon 2025-12-29 → Sun 2026-01-04.
    expect(formatWeekRange("2025-12-29")).toBe(
      "Dec 29, 2025 – Jan 4, 2026",
    );
  });
});

describe("shortWeekday / shortMonthDay", () => {
  it("formats the column headers", () => {
    expect(shortWeekday("2026-05-11")).toBe("Mon");
    expect(shortWeekday("2026-05-17")).toBe("Sun");
    expect(shortMonthDay("2026-05-11")).toBe("May 11");
  });
});
