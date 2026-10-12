import { describe, expect, it } from "bun:test";
import type { LooseEnd, LooseEnds } from "./time_leaks_contract";
import { summarise } from "./unbilled";

const item = (over: Partial<LooseEnd>): LooseEnd => ({
  block_id: 1,
  day: "2026-10-05",
  jira_issue: "ABC-1",
  started_at: "2026-10-05T09:00:00Z",
  ended_at: "2026-10-05T10:00:00Z",
  seconds: 3600,
  description: null,
  missing: ["export"],
  ...over,
});
const data = (items: LooseEnd[]): LooseEnds => ({ cycle_start: "2026-09-20", cycle_end: "2026-10-19", items });

describe("summarise", () => {
  it("groups blocks into one entry per day, oldest first", () => {
    // catches: listing blocks instead of days, or newest first
    const s = summarise(data([item({ day: "2026-10-07" }), item({ day: "2026-10-05" }), item({ day: "2026-10-05", block_id: 2 })]), "2026-10-12", 9);
    expect(s.days.map((d) => d.day)).toEqual(["2026-10-05", "2026-10-07"]);
    expect(s.days[0].seconds).toBe(7200);
  });

  it("lists each Tempo-missing ticket once with its hours, biggest first", () => {
    // catches: one row per block, or losing a ticket's second block
    const s = summarise(
      data([
        item({ missing: ["tempo"], jira_issue: "A-1", seconds: 600 }),
        item({ missing: ["tempo", "export"], jira_issue: "B-2", seconds: 1800, block_id: 2 }),
        item({ missing: ["tempo"], jira_issue: "A-1", seconds: 1800, block_id: 3 }),
        item({ missing: ["export"], jira_issue: "C-3", seconds: 900, block_id: 4 }),
      ]),
      "2026-10-12",
      9,
    );
    const day = s.days[0];
    expect(day.tempo.state).toBe("needs");
    expect(day.tempo.tickets).toEqual([
      { key: "A-1", seconds: 2400 },
      { key: "B-2", seconds: 1800 },
    ]);
  });

  it("marks a day billed-pending with the hours not yet marked", () => {
    // catches: counting Tempo-only blocks as unbilled
    const s = summarise(data([item({ missing: ["export"], seconds: 900 }), item({ missing: ["tempo"], seconds: 600, block_id: 2 })]), "2026-10-12", 9);
    expect(s.days[0].bill).toEqual({ state: "needs", seconds: 900 });
  });

  it("a day with nothing left on one step shows that step done", () => {
    // catches: showing 'needs' for a step with no missing block
    const s = summarise(data([item({ missing: ["export"] })]), "2026-10-12", 9);
    expect(s.days[0].tempo.state).toBe("done");
    expect(s.days[0].tempo.tickets).toEqual([]);
  });

  it("today's Tempo lines wait for the 17:00 send instead of needing action", () => {
    // catches: flagging today's work as a problem before the send ran
    const today = "2026-10-12";
    expect(summarise(data([item({ day: today, missing: ["tempo"] })]), today, 16).days[0].tempo.state).toBe("waiting");
    expect(summarise(data([item({ day: today, missing: ["tempo"] })]), today, 17).days[0].tempo.state).toBe("needs");
  });

  it("totals hours and days per step", () => {
    // catches: double-counting a block missing both steps in one total
    const s = summarise(
      data([
        item({ day: "2026-10-05", missing: ["tempo", "export"], seconds: 3600 }),
        item({ day: "2026-10-06", missing: ["export"], seconds: 1800, block_id: 2 }),
      ]),
      "2026-10-12",
      9,
    );
    expect(s.tempo).toEqual({ seconds: 3600, days: 1 });
    expect(s.bill).toEqual({ seconds: 5400, days: 2 });
  });

  it("gives every cycle day a state for the strip, future days marked future", () => {
    // catches: a strip that skips days with nothing open, or paints the future
    const s = summarise(data([item({ day: "2026-10-05", missing: ["tempo"] }), item({ day: "2026-10-06", block_id: 2 })]), "2026-10-12", 9);
    expect(s.cycle).toHaveLength(30);
    expect(s.cycle[0]).toEqual({ day: "2026-09-20", state: "clear" });
    expect(s.cycle.find((c) => c.day === "2026-10-05")?.state).toBe("tempo");
    expect(s.cycle.find((c) => c.day === "2026-10-06")?.state).toBe("bill");
    expect(s.cycle.find((c) => c.day === "2026-10-12")?.state).toBe("clear");
    expect(s.cycle.find((c) => c.day === "2026-10-13")?.state).toBe("future");
  });
});
