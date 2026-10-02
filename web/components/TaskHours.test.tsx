// The time ledger: loading, error, empty and the figures.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TaskHours } from "./TaskHours";
import type { BlocksLoad } from "./useWorkLog";
import { block, day, payload } from "./workLogTestKit";

afterEach(cleanup);

const TODAY = "2026-10-02";
const sent = (over = {}) => block({ tempo_worklog_id: "w", exported_at: "2026-10-01T00:00:00Z", ...over });
const ok = (days: ReturnType<typeof day>[]): BlocksLoad => ({ s: "ok", data: payload(days) });
const show = (load: BlocksLoad, onTempo = () => {}, onRetry = () => {}) =>
  render(<TaskHours taskKey="ABC-1" load={load} today={TODAY} onTempo={onTempo} onRetry={onRetry} />);
const figs = () => [...document.querySelectorAll(".task-hours-figs .task-fig")].map((f) => f.textContent);

describe("TaskHours", () => {
  it("shows a skeleton while loading", () => {
    show({ s: "loading" });
    expect(document.querySelector(".task-skel")).toBeTruthy();
  });

  it("says it could not load and retries", () => {
    const retry = mock(() => {});
    show({ s: "error", error: "boom" }, () => {}, retry);
    expect(screen.getByRole("alert").textContent).toContain("Couldn't load your time on ABC-1");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry.mock.calls.length).toBe(1);
  });

  it("explains an empty ticket", () => {
    show(ok([]));
    expect(screen.getByText("No time on ABC-1 yet. It shows up here as soon as you work on it.")).toBeTruthy();
  });

  it("shows the total, span, last worked and the four figures", () => {
    show(ok([day({ day: "2026-10-01", line_seconds: 5400, blocks: [sent()] }), day({ day: "2026-09-20", line_seconds: 3600, blocks: [sent({ exported_at: null })] })]));
    expect(screen.getByRole("heading", { name: "Your time" })).toBeTruthy();
    expect(document.querySelector(".task-hours-total")?.textContent).toBe("2h 30m");
    expect(screen.getByText("2 days · 20 Sep – 1 Oct · Last worked yesterday")).toBeTruthy();
    expect(figs()).toEqual(["1h 30mThis week", "1h 30mThis month", "All sentNot in Tempo", "1hNot invoiced"]);
  });

  it("uses a single date when first and last are the same day", () => {
    show(ok([day({ day: "2026-10-02", blocks: [sent()] })]));
    expect(screen.getByText("1 day · 2 Oct · Last worked today")).toBeTruthy();
  });

  it("offers Show unsent day when hours are not in Tempo", () => {
    const onTempo = mock(() => {});
    show(ok([day({ day: "2026-10-02", line_seconds: 1800, blocks: [block({ tempo_worklog_id: null })] })]), onTempo);
    expect(figs()[2]).toBe("30mNot in TempoShow unsent day");
    fireEvent.click(screen.getByRole("button", { name: "Show unsent day" }));
    expect(onTempo.mock.calls.length).toBe(1);
  });

  it("says All invoiced and All sent with nothing outstanding", () => {
    show(ok([day({ blocks: [sent()] })]));
    expect(screen.getByText("All invoiced")).toBeTruthy();
    expect(screen.getByText("All sent")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show unsent day" })).toBeNull();
  });
});
