import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { cityAria, cityHeadline, craneTip, flagTip, floorsFor, plotTip, gardenShare, layoutCity, plotTitle, towerHeight, windowCount, windowSlot, WorkdayCity } from "./WorkdayCity";
import { day } from "./time-fixtures";

afterEach(cleanup);

// Mon 21 Sep .. Sun 27 Sep 2026, then Mon 28
const daily = [
  day({ day: "2026-09-21", work_seconds: 3600, prompts: 30 }),
  day({ day: "2026-09-22", work_seconds: 40800, prompts: 198, personal_seconds: 3600 }),
  day({ day: "2026-09-23" }),
  day({ day: "2026-09-24" }),
  day({ day: "2026-09-25" }),
  day({ day: "2026-09-26" }),
  day({ day: "2026-09-27", work_seconds: 600 }),
  day({ day: "2026-09-28", work_seconds: 7200 }),
];

describe("helpers", () => {
  it("height scales to 9 with a visible minimum", () => {
    expect(towerHeight(100, 100)).toBe(9);
    expect(towerHeight(1, 1e9)).toBe(0.3);
    expect(towerHeight(0, 100)).toBe(0);
    expect(towerHeight(5, 0)).toBe(0);
  });
  it("windows are capped to what fits", () => {
    expect(windowCount(198, 9)).toBe(20);
    expect(windowCount(100000, 9)).toBe(4 * floorsFor(9));
    expect(windowCount(100, 0.3)).toBe(0);
    expect(windowCount(0, 9)).toBe(0);
  });
  it("window slots alternate faces and climb", () => {
    expect(windowSlot(0).face).toBe("left");
    expect(windowSlot(1).face).toBe("right");
    expect(windowSlot(4).z).toBeGreaterThan(windowSlot(0).z);
  });
  it("garden share", () => {
    expect(gardenShare(3600, 3600)).toBe(0.5);
    expect(gardenShare(0, 0)).toBe(0);
  });
});

describe("layoutCity", () => {
  const plots = layoutCity(daily, "2026-09-28");
  it("grids by weekday and week", () => {
    expect(plots[1]).toMatchObject({ col: 1, row: 0, busiest: true });
    expect(plots[7]).toMatchObject({ col: 0, row: 1, today: true });
  });
  it("classifies weekend park vs weekday lot", () => {
    expect(plots[2].kind).toBe("lot");
    expect(plots[5].kind).toBe("park");
    expect(plots[6].kind).toBe("tower");
  });
  it("keeps only the latest 13 weeks", () => {
    const long = Array.from({ length: 140 }, (_, i) =>
      day({ day: new Date(Date.UTC(2026, 0, 5 + i)).toISOString().slice(0, 10), work_seconds: 60 }),
    );
    expect(Math.max(...layoutCity(long, "x").map((p) => p.row))).toBe(12);
  });
  it("headline and title use exact numbers", () => {
    expect(cityHeadline(plots)).toContain("Tuesday 22 Sep is your skyscraper: 11h 20m");
    expect(plotTitle(plots[1])).toBe("Tue 22 Sep · 11h 20m work · 198 prompts · 1h personal");
  });
});

describe("WorkdayCity", () => {
  it("renders aria and no NaN", () => {
    const { container } = render(<WorkdayCity daily={daily} today="2026-09-28" />);
    const label = container.querySelector("svg")!.getAttribute("aria-label")!;
    expect(label).toContain("Tallest is Tue 22 Sep at 11h 20m");
    expect(label).toBe(cityAria(layoutCity(daily, "2026-09-28")));
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.textContent).toContain("trees = days off");
  });
  it("empty state", () => {
    const { container } = render(<WorkdayCity daily={[]} today="2026-09-28" />);
    expect(container.textContent).toContain("nothing built yet");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("no-work days are an empty city", () => {
    const { container } = render(<WorkdayCity daily={[day({ day: "2026-09-26" })]} today="2026-09-26" />);
    expect(container.textContent).toContain("nothing built yet");
  });
  it("single huge day", () => {
    const { container } = render(<WorkdayCity daily={[day({ day: "2026-09-22", work_seconds: 1e9, prompts: 1e7 })]} today="2026-09-22" />);
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.textContent).toContain("skyscraper");
  });
});

describe("plotTip", () => {
  const rich = [
    day({ day: "2026-09-21", work_seconds: 3600, prompts: 30 }),
    day({
      day: "2026-09-22", work_seconds: 40800, prompts: 198, personal_seconds: 3600, tool_calls: 1234, shell: 5,
      slack: 2, commits: 3, meeting_seconds: 1800, first_at: "06:30", last_at: "20:10", folders: 2, tickets: 4,
    }),
    day({ day: "2026-09-23" }),
    day({ day: "2026-09-24" }),
    day({ day: "2026-09-25" }),
    day({ day: "2026-09-26", prompts: 12, shell: 4 }),
    day({ day: "2026-09-27" }),
    day({ day: "2026-09-28", work_seconds: 7200 }),
  ];
  const plots = layoutCity(rich, "2026-09-28");

  it("busiest tower: exact rows, rank, bar, note", () => {
    const t = plotTip(plots[1], plots);
    expect(t.title).toBe("Tue 22 Sep");
    expect(t.sub).toBe("busiest day · #1 of 3 worked days");
    expect(t.rows).toEqual([
      ["Work", "11h 20m"], ["Personal", "1h"], ["Claude prompts", "198"], ["Tool calls", "1,234"],
      ["Shell commands", "5"], ["Slack messages", "2"], ["Commits", "3"], ["Meetings", "30m"],
      ["Started → finished", "06:30 – 20:10"], ["Projects", "2"], ["Tickets", "4"],
    ]);
    expect(t.bar).toEqual({ value: 40800, max: 40800, label: "100% of the busiest day" });
    expect(t.note).toBe("1.8× your average worked day. Click to open the day.");
  });
  it("other towers: rank, prompts and early-bird notes, today", () => {
    expect(plotTip(plots[0], plots).sub).toBe("#3 of 3 worked days");
    const lit = layoutCity([day({ day: "2026-09-22", work_seconds: 3600, prompts: 150 }), day({ day: "2026-09-23", work_seconds: 3600 })], "x");
    expect(plotTip(lit[0], lit).note).toBe("Lights on: 150 prompts. Click to open the day.");
    const early = layoutCity([day({ day: "2026-09-22", work_seconds: 3600, first_at: "05:45" }), day({ day: "2026-09-23", work_seconds: 3600 })], "x");
    expect(plotTip(early[0], early).note).toBe("Built before 07:00 (first activity 05:45). Click to open the day.");
    expect(plotTip(plots[7], plots).note).toBe("Under construction: today so far. Click to open the day.");
  });
  it("single tower and huge values stay sane", () => {
    const one = layoutCity([day({ day: "2026-09-22", work_seconds: 1e9, prompts: 1e7 })], "x");
    const t = plotTip(one[0], one);
    expect(t.sub).toBe("busiest day · #1 of 1 worked day");
    expect(t.rows).toContainEqual(["Claude prompts", "10,000,000"]);
    expect(t.bar!.label).toBe("100% of the busiest day");
  });
  it("park with a little activity, empty park, empty lot", () => {
    const t = plotTip(plots[5], plots);
    expect(t.sub).toBe("day off · a park");
    expect(t.note).toBe("Even on a day off: 12 prompts, 4 shell commands.");
    const quiet = plotTip(plots[6], plots);
    expect(quiet.rows).toEqual([["Activity", "none"]]);
    expect(quiet.note).toBe("Trees only: a proper day off.");
    const lot = plotTip(plots[2], plots);
    expect(lot.sub).toBe("weekday · empty lot");
    expect(lot.note).toBe("Nothing built, nothing tracked.");
  });
  it("notes agree with rows for personal-only, slack-only and tool-call-only days", () => {
    const f = layoutCity([
      day({ day: "2026-09-21", personal_seconds: 7200 }),
      day({ day: "2026-09-22", slack: 5 }),
      day({ day: "2026-09-23" }), day({ day: "2026-09-24" }), day({ day: "2026-09-25" }),
      day({ day: "2026-09-26", tool_calls: 40 }),
    ], "x");
    expect(plotTip(f[0], f).note).toBe("No work tracked, but 2h personal.");
    expect(plotTip(f[1], f).note).toBe("No work tracked, but 5 Slack messages.");
    expect(plotTip(f[5], f).note).toBe("Even on a day off: 40 tool calls.");
  });
  it("flag and crane", () => {
    expect(flagTip(plots[1]).rows).toEqual([["Work", "11h 20m"], ["Claude prompts", "198"]]);
    expect(craneTip(plots[7]).sub).toBe("under construction");
    expect(craneTip(plots[7]).note).toContain("today so far");
  });
});

describe("tips in the DOM", () => {
  it("towers link to the day, carry data-stip, and no <title> remains", () => {
    const { container } = render(<WorkdayCity daily={daily} today="2026-09-28" />);
    const link = container.querySelector('a[href="/2026-09-22"]')!;
    expect(link.getAttribute("tabindex")).toBe("0");
    const tip = JSON.parse(link.getAttribute("data-stip")!);
    expect(tip.title).toBe("Tue 22 Sep");
    expect(container.querySelectorAll("title").length).toBe(0);
    expect(container.querySelectorAll("[data-stip]").length).toBeGreaterThan(8);
    expect(container.querySelector("svg")!.getAttribute("role")).toBe("group");
  });
});
