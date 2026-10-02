// The Time card's own numbers: today vs Tempo, ticket totals, estimate meter, and the Tempo refresh footer.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { mondayOf } from "@/lib/format";
import { localToday } from "@/lib/taskBoard";
import { subscribe, type ToastMsg } from "@/lib/toast";
import type { TodayTotals } from "@/lib/types";
import { TimeCard, todayView } from "./TaskModalTime";
import { payload } from "./workLogTestKit";
import { detail, row } from "./taskModalTestKit";

afterEach(cleanup);

const today = (over: Partial<TodayTotals> = {}): TodayTotals => ({
  day: "2026-10-02",
  worked_seconds: 8100,
  in_tempo_seconds: 5400,
  ticket_worked_seconds: 2700,
  ticket_in_tempo_seconds: 1800,
  ...over,
});

const loaded = (over: Record<string, unknown> = {}) => ({ s: "ok" as const, data: { ...payload([]), today: today(), ...over } });

function card(load: ReturnType<typeof loaded> | { s: "loading" }, props: Record<string, unknown> = {}) {
  render(<TimeCard task={row({ today_seconds: 0 })} detail={null} load={load} onTempo={() => {}} {...props} />);
}
const text = () => (document.querySelector(".task-time") as HTMLElement).textContent;

describe("today", () => {
  it("builds the view", () => {
    expect(todayView(today())).toEqual({
      worked: "about 2h 15m",
      tempo: "1h 30m in Tempo",
      tempoValue: "1h 30m",
      gap: "45m not in Tempo yet",
      level: false,
      ticket: "45m worked · 30m in Tempo",
    });
    expect(todayView(today({ in_tempo_seconds: null }))).toMatchObject({ tempo: "not pulled yet", gap: null, level: false });
    expect(todayView(today({ in_tempo_seconds: 8100 }))).toMatchObject({ gap: null, level: true });
    expect(todayView(today({ in_tempo_seconds: 7500 }))?.gap).toBeNull(); // 10m behind is under the 15m bar
    expect(todayView(today({ worked_seconds: 0, in_tempo_seconds: null }))).toBeNull();
  });

  it("says worked vs in Tempo for the day, the gap in the warn tone, and the ticket on its own line", () => {
    card(loaded());
    const figs = [...document.querySelectorAll(".task-fig")].map((f) => f.textContent);
    expect(figs).toEqual(["about 2h 15mWorked", "1h 30mIn Tempo"]);
    expect(screen.getByText("45m not in Tempo yet").getAttribute("data-tone")).toBe("changed");
    expect(screen.getByText("This ticket: 45m worked · 30m in Tempo")).toBeTruthy();
  });

  it("is in the ok tone when Tempo has caught up, and says not pulled yet for an unpulled day", () => {
    card(loaded({ today: today({ in_tempo_seconds: 9000 }) }));
    expect(screen.getByText("Up to date").getAttribute("data-tone")).toBe("ok");
    cleanup();
    card(loaded({ today: today({ in_tempo_seconds: null }) }));
    expect([...document.querySelectorAll(".task-fig")].map((f) => f.textContent)).toEqual(["about 2h 15mWorked", "Not pulled yetIn Tempo"]);
  });
});

describe("ticket totals and estimate", () => {
  it("shows everyone's and your hours on the ticket", () => {
    card(loaded({ in_tempo_total_seconds: 28800 }), { detail: detail({ time_spent_seconds: 45000 }) });
    expect(text()).toContain("12h 30m by everyone");
    expect(text()).toContain("8h by you");
  });

  it("leaves both out when unknown", () => {
    card(loaded(), { detail: detail() });
    expect(text()).not.toContain("Logged on ticket");
    expect(text()).not.toContain("Estimate");
  });

  it("draws the estimate as a labelled meter with the time left", () => {
    card(loaded(), { detail: detail({ time_spent_seconds: 45000, original_estimate_seconds: 57600, remaining_estimate_seconds: 12600 }) });
    const m = screen.getByRole("meter");
    expect(m.getAttribute("aria-valuenow")).toBe("45000");
    expect(m.getAttribute("aria-valuemax")).toBe("57600");
    expect(m.getAttribute("aria-valuemin")).toBe("0");
    expect(m.getAttribute("aria-label")).toBe("Time logged against the estimate of 16h");
    expect(text()).toContain("12h 30m of 16h");
    expect(text()).toContain("3h 30m left");
  });

  it("works the time left out itself, and flags going over", () => {
    card(loaded(), { detail: detail({ time_spent_seconds: 3600, original_estimate_seconds: 7200 }) });
    expect(text()).toContain("1h left");
    cleanup();
    card(loaded(), { detail: detail({ time_spent_seconds: 9000, original_estimate_seconds: 7200 }) });
    expect(screen.getByText("30m over").getAttribute("data-tone")).toBe("changed");
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe("7200");
  });
});

describe("refresh", () => {
  it("says when the numbers are from and pulls this week, then reloads the work log", async () => {
    const pull = mock(async (_monday: string) => ({ ok: true as const, data: {} }));
    const onPulled = mock(() => {});
    const onAnnounce = mock((_m: string) => {});
    card(loaded({ pulled_at: "2026-10-02T14:05:00+00:00" }), { pull, onPulled, onAnnounce });
    expect(document.querySelector(".task-time-foot")?.textContent).toMatch(/^Tempo numbers from \d\d:\d\d · Refresh this week from Tempo$/);
    await act(async () => void fireEvent.click(screen.getByRole("button", { name: "Refresh this week from Tempo" })));
    expect(pull.mock.calls[0]).toEqual([mondayOf(localToday())]);
    expect(onPulled.mock.calls.length).toBe(1);
    expect(onAnnounce.mock.calls[0]).toEqual(["Tempo numbers updated"]);
  });

  it("is busy while pulling", async () => {
    let finish!: (v: unknown) => void;
    const pull = mock(() => new Promise((r) => (finish = r)));
    card(loaded({ pulled_at: "2026-10-02T14:05:00Z" }), { pull });
    await act(async () => void fireEvent.click(screen.getByRole("button", { name: "Refresh this week from Tempo" })));
    expect((screen.getByRole("button", { name: "Refreshing…" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => finish({ ok: true, data: {} }));
    expect(screen.getByRole("button", { name: "Refresh this week from Tempo" })).toBeTruthy();
  });

  it("toasts the daemon's message when the pull fails", async () => {
    let seen: ToastMsg[] = [];
    const off = subscribe((m) => (seen = [...m]));
    const pull = mock(async () => ({ ok: false as const, error: "tempo is down" }));
    const onPulled = mock(() => {});
    card(loaded({ pulled_at: "2026-10-02T14:05:00Z" }), { pull, onPulled });
    await act(async () => void fireEvent.click(screen.getByRole("button", { name: "Refresh this week from Tempo" })));
    off();
    expect(seen.map((m) => m.text)).toContain("tempo is down");
    expect(onPulled.mock.calls.length).toBe(0);
  });

  it("offers Pull this week from Tempo when never pulled", () => {
    card(loaded({ pulled_at: null }));
    expect(document.querySelector(".task-time-foot")?.textContent).toBe("Tempo not pulled yet · Pull this week from Tempo");
  });

  it("holds one line, Loading Tempo numbers…, while the work log loads", () => {
    card({ s: "loading" });
    expect(document.querySelectorAll(".task-time-foot").length).toBe(1);
    expect(document.querySelector(".task-time-foot")?.textContent).toBe("Loading Tempo numbers…");
  });

  it("says it could not load and Try again calls the retry", () => {
    const onRetry = mock(() => {});
    card({ s: "error", error: "down" } as never, { onRetry });
    expect(document.querySelector(".task-time-foot")?.textContent).toBe("Couldn't load Tempo numbers · Try again");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry.mock.calls.length).toBe(1);
  });
});
