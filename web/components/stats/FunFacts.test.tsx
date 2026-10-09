import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { StatsRecords } from "@/lib/stats_contract";
import { FunFacts, factsOf } from "./FunFacts";

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
