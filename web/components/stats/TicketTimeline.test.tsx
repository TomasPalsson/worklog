import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { TicketStat } from "@/lib/stats_contract";
import { parseTip } from "./tip";
import { TicketTimeline } from "./TicketTimeline";
import {
  buildCols,
  cellTip,
  fmtHours,
  headline,
  rangeDays,
  rowTip,
  shadeBucket,
  shadeRanges,
  sortTickets,
  ticketDays,
} from "./timelineLib";

afterEach(cleanup);

const tk = (key: string, days: string[], secs: number[], extra: Partial<TicketStat> = {}): TicketStat => ({
  key,
  summary: `${key} summary`,
  seconds: secs.reduce((a, b) => a + b, 0),
  blocks: days.length,
  first_day: days[0],
  last_day: days[days.length - 1],
  days_active: days.length,
  days,
  day_seconds: secs,
  status: "In Progress",
  status_category: "indeterminate",
  ...extra,
});

describe("helpers", () => {
  it("formats hours", () => {
    expect(fmtHours(9000)).toBe("2.5");
    expect(fmtHours(3600)).toBe("1");
    expect(fmtHours(1000)).toBe("0.3");
    expect(fmtHours(37800)).toBe("11"); // 10.5h: whole hours from 10h so it fits a 20px square
  });
  it("legend switches to minutes when the busiest ticket-day is under an hour", () => {
    expect(shadeRanges(300)).toEqual(["0–1m", "1–3m", "3–4m", "4–5m"]);
    expect(shadeRanges(4 * 3600)).toEqual(["0–1h", "1–2h", "2–3h", "3–4h"]);
  });
  it("buckets shades by the max", () => {
    expect(shadeBucket(0, 100)).toBe(0);
    expect(shadeBucket(10, 100)).toBe(1);
    expect(shadeBucket(50, 100)).toBe(2);
    expect(shadeBucket(75, 100)).toBe(3);
    expect(shadeBucket(100, 100)).toBe(4);
  });
  it("falls back for old daemons", () => {
    const t = tk("A-1", ["2026-10-05", "2026-10-07"], [0, 0], { seconds: 7200, day_seconds: undefined as never });
    expect(ticketDays(t).map((d) => d.seconds)).toEqual([3600, 3600]);
    const u = { ...t, days: undefined as never };
    expect(ticketDays(u).map((d) => d.day)).toEqual(["2026-10-05", "2026-10-07"]);
  });
  it("sorts by hours and by start", () => {
    const a = tk("A-1", ["2026-10-06"], [3600]);
    const b = tk("B-1", ["2026-10-05"], [1800]);
    expect(sortTickets([b, a], "hours").map((t) => t.key)).toEqual(["A-1", "B-1"]);
    expect(sortTickets([a, b], "start").map((t) => t.key)).toEqual(["B-1", "A-1"]);
  });
  it("builds columns with weekends, mondays, months and today", () => {
    expect(rangeDays("2026-09-29", "2026-10-11")).toBe(13);
    const c = buildCols("2026-09-29", "2026-10-11", "2026-10-10");
    expect(c).toHaveLength(13);
    expect(c[0].month).toBe("Sep");
    expect(c[2].month).toBe("Oct"); // 1 Oct
    expect(c[1].month).toBeNull();
    expect(c.filter((x) => x.monday).map((x) => x.day)).toEqual(["2026-10-05"]);
    expect(c.filter((x) => x.weekend).map((x) => x.dom)).toEqual([3, 4, 10, 11]);
    expect(c.filter((x) => x.today).map((x) => x.day)).toEqual(["2026-10-10"]);
  });
  it("writes tips and headline", () => {
    const t = tk("GENAI-1", ["2026-10-05", "2026-10-07"], [9000, 3600]);
    expect(headline([t])).toContain("GENAI-1 took the most: 3h 30m over 2 days");
    const ct = cellTip(t, "2026-10-07", 3600);
    expect(ct.title).toBe("GENAI-1 · Wed 7 Oct");
    expect(ct.rows).toContainEqual(["Day", "2 of 2 on this ticket"]);
    expect(ct.rows).toContainEqual(["Since previous", "2 days earlier"]);
    expect(rowTip(t, 42000).rows).toContainEqual(["Busiest day", "Mon 5 Oct · 2h 30m"]);
  });
});

describe("TicketTimeline", () => {
  const many = Array.from({ length: 15 }, (_, i) => tk(`T-${i + 1}`, [`2026-10-0${(i % 5) + 1}`], [(i + 1) * 600]));
  const props = { from: "2026-10-01", to: "2026-10-10", today: "2026-10-10", totalWork: 100000 };

  it("shows 12 rows then reveals all", () => {
    const { container } = render(<TicketTimeline tickets={many} {...props} />);
    expect(container.querySelectorAll("[data-ticket]")).toHaveLength(12);
    fireEvent.click(screen.getByText("Show all 15 tickets"));
    expect(container.querySelectorAll("[data-ticket]")).toHaveLength(15);
  });

  it("cells carry tips with the right numbers", () => {
    const t = tk("GENAI-9", ["2026-10-05"], [9000]);
    const { container } = render(<TicketTimeline tickets={[t]} {...props} />);
    const cell = container.querySelector(".tt-fill")!;
    expect(cell.textContent).toBe("2.5");
    const tip = parseTip(cell.getAttribute("data-stip"))!;
    expect(tip.title).toBe("GENAI-9 · Mon 5 Oct");
    expect(tip.rows).toContainEqual(["Hours", "2h 30m"]);
    expect(cell.getAttribute("tabindex")).toBe("0");
  });

  it("toggles sort order", () => {
    const a = tk("A-1", ["2026-10-06"], [3600]);
    const b = tk("B-1", ["2026-10-02"], [1800]);
    const { container } = render(<TicketTimeline tickets={[b, a]} {...props} />);
    const keys = () => [...container.querySelectorAll("[data-ticket]")].map((e) => e.getAttribute("data-ticket"));
    expect(keys()).toEqual(["A-1", "B-1"]);
    fireEvent.click(screen.getByText("by start date"));
    expect(keys()).toEqual(["B-1", "A-1"]);
  });

  it("shows a calm empty state", () => {
    render(<TicketTimeline tickets={[]} {...props} />);
    expect(screen.getByText("No ticket work in this range yet.")).toBeTruthy();
  });
});
