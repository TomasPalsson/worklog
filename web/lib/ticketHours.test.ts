import { describe, expect, it } from "bun:test";

import { relativeDay, ticketHours } from "./ticketHours";
import type { RawBlock, TicketBlocks, TicketDay } from "./types";

const blk = (over: Partial<RawBlock> = {}): RawBlock => ({ tempo_worklog_id: "w", dirty: false, exported_at: "2026-10-01T00:00:00Z", ...over }) as RawBlock;
const d = (day: string, line_seconds: number, blocks: RawBlock[] = [blk()]): TicketDay => ({ day, line_seconds, blocks }) as TicketDay;
const data = (days: TicketDay[]): TicketBlocks => ({ key: "A-1", from: "", to: "", days, in_tempo_total_seconds: null, pulled_at: null }) as unknown as TicketBlocks;

const TODAY = "2026-10-02"; // Friday; Monday is 2026-09-28

describe("ticketHours", () => {
  it("is all zero for no days", () => {
    expect(ticketHours(data([]), TODAY)).toEqual({ total: 0, week: 0, month: 0, days: 0, first: null, last: null, unsent: 0, uninvoiced: 0, tracked: 0, unsentDays: 0 });
  });

  it("splits week and month on their boundaries", () => {
    const h = ticketHours(data([d("2026-10-02", 3600), d("2026-09-30", 1800), d("2026-09-28", 1800), d("2026-09-27", 7200), d("2026-10-03", 900)]), TODAY);
    expect(h.total).toBe(3600 + 1800 + 1800 + 7200 + 900);
    // Fri, Wed and Mon count for the week; Sunday and tomorrow do not.
    expect(h.week).toBe(3600 + 1800 + 1800);
    // Only October days count for the month (tomorrow included).
    expect(h.month).toBe(3600 + 900);
    expect(h.days).toBe(5);
    expect(h.first).toBe("2026-09-27");
    expect(h.last).toBe("2026-10-03");
  });

  it("finds first and last whatever the order", () => {
    const h = ticketHours(data([d("2026-09-01", 1), d("2026-10-01", 1), d("2026-08-01", 1)]), TODAY);
    expect([h.first, h.last]).toEqual(["2026-08-01", "2026-10-01"]);
  });

  it("counts Not sent and Changed since sent days as unsent", () => {
    const h = ticketHours(
      data([
        d("2026-10-02", 1000, [blk()]),
        d("2026-10-01", 2000, [blk({ tempo_worklog_id: null })]),
        d("2026-09-30", 4000, [blk({ dirty: true })]),
      ]),
      TODAY,
    );
    expect(h.unsent).toBe(6000);
  });

  it("counts days with any unexported block as uninvoiced", () => {
    const h = ticketHours(data([d("2026-10-02", 1000, [blk(), blk({ exported_at: null })]), d("2026-10-01", 2000, [blk()])]), TODAY);
    expect(h.uninvoiced).toBe(1000);
  });
});

describe("relativeDay", () => {
  it("words the distance", () => {
    expect(relativeDay("2026-10-02", TODAY)).toBe("today");
    expect(relativeDay("2026-10-01", TODAY)).toBe("yesterday");
    expect(relativeDay("2026-09-27", TODAY)).toBe("5 days ago");
  });
});
