import { describe, expect, it } from "bun:test";
import {
  formatDuration,
  formatRange,
  formatTotalHours,
  shiftDay,
  todayISO,
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
