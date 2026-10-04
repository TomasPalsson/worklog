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

  it("renders nothing for an empty ticket (the work log says it)", () => {
    show(ok([]));
    expect(document.querySelector(".task-hours")).toBeNull();
  });

  it("shows the total, span, last worked and the three figures", () => {
    show(ok([day({ day: "2026-10-01", line_seconds: 5400, blocks: [sent()] }), day({ day: "2026-09-20", line_seconds: 3600, blocks: [sent({ exported_at: null })] })]));
    expect(screen.getByRole("heading", { name: "Your time" })).toBeTruthy();
    expect(document.querySelector(".task-hours-total")?.textContent).toBe("2h 30m");
    expect(screen.getByText("2 days · 20 Sep – 1 Oct · Last worked yesterday · rounded up per day from 3h tracked")).toBeTruthy();
    expect(figs()).toEqual(["1h 30mThis week", "1h 30mThis month", "All sentTo send to Tempo"]);
  });

  it("shows the rounding only when tracked differs from billed", () => {
    show(ok([day({ day: "2026-10-01", line_seconds: 1800, tracked_seconds: 600, blocks: [sent()] })]));
    expect(screen.getByText("1 day · 1 Oct · Last worked yesterday · rounded up per day from 10m tracked")).toBeTruthy();
    expect(document.querySelector(".task-hours-total")?.getAttribute("title")).toBeNull();
  });

  it("uses a single date when first and last are the same earlier day", () => {
    show(ok([day({ day: "2026-10-01", blocks: [sent()] })]));
    expect(screen.getByText("1 day · 1 Oct · Last worked yesterday")).toBeTruthy();
  });

  it("drops the date from the sub-line when the only day is today", () => {
    show(ok([day({ day: "2026-10-02", blocks: [sent()] })]));
    expect(screen.getByText("1 day · Last worked today")).toBeTruthy();
  });

  it("shows no invoicing figure and no billing link, even with uninvoiced hours", () => {
    show(ok([day({ blocks: [sent({ exported_at: null })] })]));
    expect(screen.queryByText("Not invoiced")).toBeNull();
    expect(screen.queryByText("All invoiced")).toBeNull();
    expect(document.querySelector('a[href="/billing"]')).toBeNull();
  });

  it("offers Show unsent day when hours are not in Tempo", () => {
    const onTempo = mock(() => {});
    show(ok([day({ day: "2026-10-02", line_seconds: 1800, blocks: [block({ tempo_worklog_id: null })] })]), onTempo);
    expect(figs()[2]).toBe("30mTo send to TempoShow unsent day");
    fireEvent.click(screen.getByRole("button", { name: "Show unsent day" }));
    expect(onTempo.mock.calls.length).toBe(1);
  });

  it("labels several unsent days by count", () => {
    show(ok([day({ day: "2026-10-02", blocks: [block({ tempo_worklog_id: null })] }), day({ day: "2026-10-01", blocks: [block({ tempo_worklog_id: null })] })]));
    expect(screen.getByRole("button", { name: "Show 2 unsent days" })).toBeTruthy();
  });

  it("says All invoiced and All sent with nothing outstanding", () => {
    show(ok([day({ blocks: [sent()] })]));
    expect(screen.getByText("All sent")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show unsent day" })).toBeNull();
  });
});
