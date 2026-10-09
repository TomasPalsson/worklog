import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { DailyStat, StatsRecords } from "@/lib/stats_contract";
import { FunFacts, factsOf, streakRuns, tipsOf } from "./FunFacts";
import { parseTip } from "./tip";

afterEach(cleanup);

const REC: StatsRecords = {
  busiest_day: { day: "2025-03-04", seconds: 11 * 3600 + 1800 },
  longest_block: { day: "2025-03-05", seconds: 5400, ticket: "GEN-1" },
  earliest_start: { day: "2025-03-06", time: "05:41" },
  latest_finish: { day: "2025-03-07", time: "23:12" },
  most_prompts: { day: "2025-03-08", n: 1234 },
  most_tools: { day: "2025-03-09", n: 9876 },
  longest_streak: 12,
  current_streak: 3,
};
const NONE: StatsRecords = {
  busiest_day: null, longest_block: null, earliest_start: null, latest_finish: null,
  most_prompts: null, most_tools: null, longest_streak: 0, current_streak: 0,
};

describe("FunFacts", () => {
  it("formats values and links days to the day page", () => {
    const { container } = render(<FunFacts records={REC} />);
    expect(screen.getByText("11h 30m")).toBeTruthy();
    expect(screen.getByText("1,234")).toBeTruthy();
    expect(screen.getByText("05:41")).toBeTruthy();
    const hrefs = [...container.querySelectorAll("a")].map((a) => a.getAttribute("href"));
    expect(hrefs).toEqual(["/2025-03-04", "/2025-03-05", "/2025-03-06", "/2025-03-07", "/2025-03-08", "/2025-03-09"]);
  });
  it("flames only for streaks of 3 or more", () => {
    expect(factsOf(REC).filter((f) => f.flame).map((f) => f.key)).toEqual(["longest", "current"]);
    expect(factsOf({ ...REC, current_streak: 2 }).filter((f) => f.flame).map((f) => f.key)).toEqual(["longest"]);
    const { container } = render(<FunFacts records={{ ...REC, longest_streak: 2, current_streak: 1 }} />);
    expect(container.querySelector(".stats-flame")).toBeNull();
  });
  it("no data: dashes, no links, no NaN", () => {
    const { container } = render(<FunFacts records={NONE} />);
    expect(container.querySelectorAll("a").length).toBe(0);
    expect(container.textContent).not.toContain("NaN");
    expect(screen.getAllByText("—").length).toBe(6);
    expect(screen.getAllByText("0 days").length).toBe(2);
  });
  it("singular day", () => {
    expect(factsOf({ ...NONE, current_streak: 1 }).find((f) => f.key === "current")!.value).toBe("1 day");
  });
});

const dd = (day: string, o: Partial<DailyStat> = {}): DailyStat => ({
  day, work_seconds: 0, personal_seconds: 0, ignored_seconds: 0, prompts: 0, tool_calls: 0, shell: 0, slack: 0,
  browser_minutes: 0, claude_busy_minutes: 0, commits: 0, meeting_seconds: 0, first_at: null, last_at: null,
  folders: 0, tickets: 0, ...o,
});
const DAILY = [
  dd("2025-03-03", { work_seconds: 3600, prompts: 10, first_at: "09:00", last_at: "17:00" }),
  dd("2025-03-04", { work_seconds: 7200, prompts: 30, first_at: "08:00", last_at: "18:00", tickets: 2, folders: 1, meeting_seconds: 1800 }),
  dd("2025-03-05", { work_seconds: 3600, prompts: 20, first_at: "10:00", last_at: "16:00" }),
  dd("2025-03-06"),
  dd("2025-03-07", { work_seconds: 600 }),
];

describe("FunFacts tips", () => {
  const rec: StatsRecords = {
    ...NONE,
    busiest_day: { day: "2025-03-04", seconds: 7200 },
    most_prompts: { day: "2025-03-04", n: 30 },
    longest_streak: 3,
    current_streak: 1,
  };
  it("streakRuns finds consecutive worked days", () => {
    expect(streakRuns(DAILY)).toEqual([
      { start: "2025-03-03", end: "2025-03-05", n: 3 },
      { start: "2025-03-07", end: "2025-03-07", n: 1 },
    ]);
    expect(streakRuns([])).toEqual([]);
  });
  it("busiest day: breakdown and multiple of the average (exact strings)", () => {
    const t = tipsOf(rec, DAILY).busiest;
    expect(t.rows).toContainEqual(["Worked", "2h"]);
    expect(t.rows).toContainEqual(["Meetings", "30m"]);
    expect(t.rows).toContainEqual(["Active", "08:00 \u2013 18:00"]);
    expect(t.rows).toContainEqual(["Average active day", "1h 3m"]);
    expect(t.note).toBe("That's 1.9\u00d7 your average active day.");
  });
  it("prompts: share and average", () => {
    const t = tipsOf(rec, DAILY).prompts;
    expect(t.rows).toContainEqual(["Share of the range", "50%"]);
    expect(t.note).toBe("That's 1.5\u00d7 your average active day.");
  });
  it("earliest start compares only worked days (a 06:00 Slack check on a day off doesn't count)", () => {
    const daily = [...DAILY, dd("2025-03-08", { first_at: "06:00", slack: 1 })];
    const t = tipsOf({ ...rec, earliest_start: { day: "2025-03-04", time: "08:00" } }, daily).early;
    expect(t.note).toBe("Typically 09:00; this one is #1 of 3 days.");
  });
  it("streak tips carry start/end dates", () => {
    const t = tipsOf(rec, DAILY);
    expect(t.longest.sub).toBe("Mon Mar 3 \u2192 Wed Mar 5");
    expect(t.longest.note).toBe("4 more to a full week.");
    expect(t.current.rows).toContainEqual(["From", "Fri Mar 7"]);
  });
  it("works without daily, and with no data", () => {
    const t = tipsOf(rec);
    expect(t.longest.sub).toBeUndefined();
    expect(t.busiest.rows).toEqual([["Worked", "2h"]]);
    const none = tipsOf(NONE);
    expect(JSON.stringify(none)).not.toMatch(/NaN|Infinity/);
    expect(none.busiest.note).toBe("No work blocks yet.");
  });
  it("renders a data-stip on every card, tab-reachable, no <title>", () => {
    const { container } = render(<FunFacts records={rec} daily={DAILY} />);
    const cards = container.querySelectorAll(".stats-fact-link[data-stip]");
    expect(cards.length).toBe(8);
    expect(parseTip(cards[0].getAttribute("data-stip"))?.title).toBe("Busiest day");
    expect(container.querySelector("title")).toBeNull();
    expect(container.querySelector("div.stats-fact-link")?.getAttribute("tabindex")).toBe("0");
  });
});
