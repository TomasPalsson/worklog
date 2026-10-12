import { describe, expect, it } from "bun:test";
import type { TempoGap, UnbilledDay, UnbilledDays } from "./time_leaks_contract";
import { summarise } from "./unbilled";

const gap = (over: Partial<TempoGap>): TempoGap => ({
  jira_issue: "ABC-1",
  seconds: 3600,
  in_tempo_seconds: 0,
  reason: "not_sent",
  ...over,
});
const day = (over: Partial<UnbilledDay>): UnbilledDay => ({
  day: "2026-10-05",
  dismissed: null,
  tempo: [],
  ticketless_seconds: 0,
  export: null,
  ...over,
});
const data = (days: UnbilledDay[]): UnbilledDays => ({
  cycle_start: "2026-09-20",
  cycle_end: "2026-10-19",
  tempo_pulled_at: "2026-10-12T08:00:00Z",
  days,
});

describe("summarise", () => {
  it("keeps the daemon's days, oldest first", () => {
    // catches: re-sorting newest first, or dropping a day that only has export left
    const s = summarise(data([day({ day: "2026-10-07", export: { seconds: 60 } }), day({ day: "2026-10-05", ticketless_seconds: 60 })]), "2026-10-12", 9);
    expect(s.days.map((d) => d.day)).toEqual(["2026-10-05", "2026-10-07"]);
  });

  it("a ticket's missing hours are the whole line unless Tempo is short", () => {
    // catches: subtracting Tempo's hours for not_sent/deleted, or not subtracting them for short
    const s = summarise(
      data([
        day({
          tempo: [
            gap({ jira_issue: "A-1", seconds: 3600, in_tempo_seconds: 1800, reason: "short" }),
            gap({ jira_issue: "B-2", seconds: 4920, in_tempo_seconds: 3600, reason: "deleted" }),
            gap({ jira_issue: "C-3", seconds: 600, reason: "not_sent" }),
          ],
        }),
      ]),
      "2026-10-12",
      9,
    );
    const d = s.days[0];
    expect(d.tempo.state).toBe("needs");
    expect(d.tempo.tickets.map((t) => [t.key, t.missing, t.reason])).toEqual([
      ["B-2", 4920, "deleted"],
      ["A-1", 1800, "short"],
      ["C-3", 600, "not_sent"],
    ]);
    expect(d.tempo.seconds).toBe(7320);
  });

  it("a day with no Tempo gap shows that step done", () => {
    // catches: showing 'needs' for a day whose only open item is ticketless work
    const s = summarise(data([day({ ticketless_seconds: 900 })]), "2026-10-12", 9);
    expect(s.days[0].tempo.state).toBe("done");
    expect(s.days[0].tempo.tickets).toEqual([]);
    expect(s.days[0].ticketless).toBe(900);
  });

  it("today's Tempo lines wait for the 17:00 send instead of needing action", () => {
    // catches: flagging today's work as a problem before the send ran
    const today = "2026-10-12";
    const d = data([day({ day: today, tempo: [gap({})] })]);
    expect(summarise(d, today, 16).days[0].tempo.state).toBe("waiting");
    expect(summarise(d, today, 17).days[0].tempo.state).toBe("needs");
  });

  it("totals missing Tempo hours, ticketless hours and export hours with their day counts", () => {
    // catches: adding export hours into the Tempo total, or counting a day without that item
    const s = summarise(
      data([
        day({ day: "2026-10-05", tempo: [gap({ seconds: 3600, in_tempo_seconds: 1200, reason: "short" })], export: { seconds: 3600 } }),
        day({ day: "2026-10-06", ticketless_seconds: 1800, export: { seconds: 900 } }),
        day({ day: "2026-10-07", ticketless_seconds: 600 }),
      ]),
      "2026-10-12",
      9,
    );
    expect(s.tempo).toEqual({ seconds: 2400, days: 1 });
    expect(s.ticket).toEqual({ seconds: 2400, days: 2 });
    expect(s.bill).toEqual({ seconds: 4500, days: 2 });
  });

  it("strip state: tempo beats ticket beats bill beats waiting; dismissed days with nothing open are clear", () => {
    // catches: the old tempo > waiting > bill order, a missing ticket state, or painting a dismissed day
    const today = "2026-10-12";
    const s = summarise(
      data([
        day({ day: "2026-10-01", tempo: [gap({})], ticketless_seconds: 60, export: { seconds: 60 } }),
        day({ day: "2026-10-02", ticketless_seconds: 60, export: { seconds: 60 } }),
        day({ day: "2026-10-03", export: { seconds: 60 } }),
        day({ day: "2026-10-04", dismissed: "Left earlier" }),
        day({ day: "2026-10-05", dismissed: "Klipping", ticketless_seconds: 60 }),
        day({ day: today, tempo: [gap({})], export: { seconds: 60 } }),
      ]),
      today,
      9,
    );
    const at = (d: string) => s.cycle.find((c) => c.day === d)?.state;
    expect(at("2026-10-01")).toBe("tempo");
    expect(at("2026-10-02")).toBe("ticket");
    expect(at("2026-10-03")).toBe("bill");
    expect(at("2026-10-04")).toBe("clear");
    expect(at("2026-10-05")).toBe("ticket");
    expect(at(today)).toBe("bill");
    expect(summarise(data([day({ day: today, tempo: [gap({})] })]), today, 9).cycle.find((c) => c.day === today)?.state).toBe("waiting");
  });

  it("gives every cycle day a state for the strip, future days marked future", () => {
    // catches: a strip that skips days with nothing open, or paints the future
    const s = summarise(data([day({ day: "2026-10-05", tempo: [gap({})] })]), "2026-10-12", 9);
    expect(s.cycle).toHaveLength(30);
    expect(s.cycle[0]).toEqual({ day: "2026-09-20", state: "clear" });
    expect(s.cycle.find((c) => c.day === "2026-10-12")?.state).toBe("clear");
    expect(s.cycle.find((c) => c.day === "2026-10-13")?.state).toBe("future");
  });
});
