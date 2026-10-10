import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import type { DailyStat, StatsReport } from "@/lib/stats_contract";
import { StatsHero, bucket, groupsOf, heroTiles, sparkPoints, sparkTips, tileTip } from "./StatsHero";
import { parseTip } from "./tip";

afterEach(cleanup);

const day = (o: Partial<DailyStat> = {}): DailyStat => ({
  day: "2026-10-01", work_seconds: 0, personal_seconds: 0, ignored_seconds: 0, prompts: 0, tool_calls: 0,
  shell: 0, slack: 0, browser_minutes: 0, claude_busy_minutes: 0, commits: 0, meeting_seconds: 0,
  first_at: null, last_at: null, folders: 0, tickets: 0, ...o,
});
const report = (daily: DailyStat[], t: Partial<StatsReport["totals"]> = {}): StatsReport =>
  ({
    totals: {
      work_seconds: 0, days_worked: 0, prompts: 0, tool_calls: 0, helpers: 0, shell_commands: 0, commits: 0,
      prs: 0, slack_messages: 0, browser_minutes: 0, meetings: 0, meeting_seconds: 0, ...t,
    },
    daily,
  }) as unknown as StatsReport;

describe("sparkPoints / bucket", () => {
  it("all zero is a flat line, not NaN", () => {
    const p = sparkPoints([0, 0, 0]);
    expect(p).not.toContain("NaN");
    expect(new Set(p.split(" ").map((s) => s.split(",")[1])).size).toBe(1);
  });
  it("empty is empty; single value is finite", () => {
    expect(sparkPoints([])).toBe("");
    expect(sparkPoints([5])).not.toContain("NaN");
  });
  it("peak sits at the top", () => {
    expect(sparkPoints([0, 10]).split(" ")[1]).toBe("100.0,2.0");
  });
  it("buckets long series by summing and conserves the total", () => {
    const v = Array.from({ length: 365 }, (_, i) => i);
    const b = bucket(v, 60);
    expect(b.length).toBe(60);
    expect(b.reduce((a, x) => a + x, 0)).toBe(v.reduce((a, x) => a + x, 0));
    expect(bucket([1, 2], 60)).toEqual([1, 2]);
  });
});

describe("StatsHero", () => {
  it("shows big numbers with thousands separators and per-day captions", () => {
    const r = report([day(), day({ day: "2026-10-02" })], {
      work_seconds: 7.5 * 3600, days_worked: 2, prompts: 4213, tool_calls: 1_000_000, commits: 3, prs: 1,
    });
    const tiles = heroTiles(r);
    expect(tiles).toHaveLength(9);
    expect(tiles[0].value).toBe("7.5");
    expect(tiles[0].caption).toContain("3.8h");
    expect(tiles.find((t) => t.key === "prompts")!.value).toBe("4,213");
    expect(tiles.find((t) => t.key === "tools")!.value).toBe("1,000,000");
    expect(tiles.find((t) => t.key === "shipped")!.value).toBe("4");
    const { container } = render(<StatsHero report={r} />);
    expect(container.querySelectorAll(".stats-tile").length).toBe(9);
    expect(container.textContent).not.toContain("NaN");
  });
  it("zero everything stays calm", () => {
    const { container } = render(<StatsHero report={report([])} />);
    expect(container.textContent).not.toMatch(/NaN|Infinity/);
    expect(container.textContent).toContain("No hours logged yet.");
  });
  it("single day draws a dot, not a line", () => {
    const { container } = render(<StatsHero report={report([day({ prompts: 3 })], { prompts: 3, days_worked: 1 })} />);
    expect(container.querySelector("circle")).not.toBeNull();
  });
});

describe("hero tips", () => {
  const days = ["2026-10-01", "2026-10-02", "2026-10-03"];
  const fmt = (n: number) => `${n} p`;
  it("sparkTips: exact strings for a fixture", () => {
    const t = sparkTips([10, 0, 30], days, fmt, "Prompts");
    expect(t).toHaveLength(3);
    expect(t[2].title).toBe("Sat Oct 3");
    expect(t[2].rows).toEqual([["Prompts", "30 p"], ["Average active day", "20 p"], ["Peak day", "30 p"]]);
    expect(t[2].note).toBe("Your peak of the range.");
    expect(t[0].note).toBe("50% of your average active day.");
    expect(t[1].note).toBe("A quiet one.");
  });
  it("sparkTips: all zero and single day do not NaN", () => {
    const z = sparkTips([0, 0], days, fmt, "P");
    expect(JSON.stringify(z)).not.toMatch(/NaN|Infinity/);
    expect(sparkTips([5], ["2026-10-01"], fmt, "P")[0].note).toBe("Your peak of the range.");
    expect(sparkTips([], [], fmt, "P")).toEqual([]);
  });
  it("sparkTips: long series are bucketed and cover every day", () => {
    const g = groupsOf(365);
    expect(g).toHaveLength(60);
    expect(g.flat()).toHaveLength(365);
    const many = Array.from({ length: 365 }, (_, i) => `2026-01-01`.replace("01", String((i % 28) + 1).padStart(2, "0")));
    const t = sparkTips(new Array(365).fill(1), many, fmt, "P");
    expect(t[0].sub).toContain("days, summed");
  });
  it("tileTip: total, per worked day, best and quietest day", () => {
    const r = report(
      [day({ day: days[0], prompts: 10 }), day({ day: days[1] }), day({ day: days[2], prompts: 30 })],
      { prompts: 40, days_worked: 2 },
    );
    const tile = heroTiles(r).find((x) => x.key === "prompts")!;
    const t = tileTip(tile, days, 2);
    expect(t.rows).toEqual([
      ["Total", "40"], ["Per worked day", "20 prompts"], ["Best day", "Sat Oct 3 \u00b7 30 prompts"],
      ["Quietest active day", "Thu Oct 1 \u00b7 10 prompts"], ["Days worked", "2"],
    ]);
    expect(t.note).toBe("Your best day was 1.5\u00d7 the average.");
  });
  it("tileTip: no series and zero days stay calm", () => {
    const tile = heroTiles(report([], { helpers: 3 })).find((x) => x.key === "helpers")!;
    const t = tileTip(tile, [], 0);
    expect(t.rows).toEqual([["Total", "3"], ["Days worked", "0"]]);
    expect(t.note).toBeUndefined();
  });
  it("renders data-stip on numbers and sparkline hit areas, no <title>", () => {
    const r = report([day({ prompts: 3 }), day({ day: "2026-10-02", prompts: 6 })], { prompts: 9, days_worked: 2 });
    const { container } = render(<StatsHero report={r} />);
    const nums = container.querySelectorAll(".stats-tile-num[data-stip]");
    expect(nums.length).toBe(9);
    expect(nums[0].getAttribute("tabindex")).toBe("0");
    expect(parseTip(nums[1].getAttribute("data-stip"))?.title).toBe("Claude prompts");
    expect(container.querySelectorAll(".stats-spark-hit[data-stip]").length).toBe(2 * 8);
    expect(container.querySelector("title")).toBeNull();
  });
});
