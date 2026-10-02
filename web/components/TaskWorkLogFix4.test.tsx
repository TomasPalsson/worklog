// Fix round 4 for the "Work logged" section: focus after Send, Use tracked time, slimmer day meta,
// vocabulary, accessible hours name, newest-5 days, form alignment.

import { afterEach, describe, expect, it, mock, setDefaultTimeout } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskWorkLog } from "./TaskWorkLog";

setDefaultTimeout(20000);
afterEach(cleanup);

const block = (over: Partial<RawBlock> = {}): RawBlock =>
  ({
    id: 1,
    day: "2026-10-01",
    jira_issue: "ABC-1",
    started_at: "2026-10-01T09:00:00Z",
    ended_at: "2026-10-01T10:30:00Z",
    duration_seconds: 5400,
    description: "Fixed the redirect",
    estimated_by: "claude",
    tempo_worklog_id: null,
    dirty: false,
    ...over,
  }) as RawBlock;

const day = (over: Partial<TicketDay> = {}): TicketDay => ({
  day: "2026-10-01",
  line_seconds: 5400,
  line_text: "Worked on login",
  tracked_seconds: 5400,
  hours_set_by_hand: false,
  in_tempo_seconds: null,
  blocks: [block()],
  ...over,
});

const ok = { ok: true as const, data: {} };
const syncOk = () => mock(async () => ({ ok: true as const, data: { synced: 1, errors: [], results: [] as never[] } }));
const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

async function open(days: TicketDay[] = [day()], over: Record<string, unknown> = {}) {
  const a = {
    loadTicketBlocks: mock(async () => ({
      ok: true as const,
      data: { key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days } as TicketBlocks,
    })),
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineHours: mock(async () => ok),
    saveTempoLineText: mock(async () => ok),
    runSync: syncOk(),
    ...over,
  } as unknown as TaskActions;
  render(<TaskWorkLog taskKey="ABC-1" actions={a} onAnnounce={() => {}} />);
  await waitFor(() => expect(document.querySelector(".task-skel")).toBeNull());
  return a;
}

const btn = (name: string | RegExp) => screen.getByRole("button", { name });
const changedDay = (over: Partial<TicketDay> = {}) =>
  day({ in_tempo_seconds: 3600, blocks: [block({ tempo_worklog_id: "w1", dirty: true })], ...over });

describe("focus after Send", () => {
  it("a real send focuses the Sent to Tempo strip", async () => {
    await open();
    fireEvent.click(btn("Preview send Thu 1 Oct to Tempo"));
    fireEvent.click(await screen.findByRole("button", { name: "Send to Tempo" }));
    const strip = await screen.findByText("Sent to Tempo · 1h 30m");
    expect(strip.getAttribute("tabindex")).toBe("-1");
    expect(document.activeElement).toBe(strip);
  });

  it("an update says Tempo updated and focuses it", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Preview update Thu 1 Oct in Tempo"));
    fireEvent.click(await screen.findByRole("button", { name: "Update Tempo" }));
    const strip = await screen.findByText("Tempo updated · 1h 30m");
    expect(document.activeElement).toBe(strip);
  });

  it("nothing sent focuses the message", async () => {
    const runSync = mock(async (_d: string, dry: boolean) => ({
      ok: true as const,
      data: { synced: dry ? 1 : 0, errors: [], results: [] },
    }));
    await open([day()], { runSync });
    fireEvent.click(btn("Preview send Thu 1 Oct to Tempo"));
    fireEvent.click(await screen.findByRole("button", { name: "Send to Tempo" }));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct/);
    expect(document.activeElement).toBe(msg);
  });
});

describe("Use tracked time", () => {
  const byHand = () => day({ hours_set_by_hand: true, line_seconds: 7200, tracked_seconds: 6120 });

  it("clears the override with null and refetches", async () => {
    const a = await open([byHand()]);
    fireEvent.click(btn("2h — edit hours for Thu 1 Oct"));
    fireEvent.click(btn("Use tracked time (1h 30m)")); // 1h 42m rounds to 1h 30m
    await waitFor(() => expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull());
    expect(calls(a.saveTempoLineHours)[0]).toEqual([{ day: "2026-10-01", jira_issue: "ABC-1" }, null]);
    await waitFor(() => expect(calls(a.loadTicketBlocks)).toHaveLength(2));
  });

  it("is absent when the hours are not set by hand", async () => {
    await open();
    fireEvent.click(btn("1h 30m — edit hours for Thu 1 Oct"));
    expect(screen.queryByRole("button", { name: /Use tracked time/ })).toBeNull();
  });
});

describe("day meta", () => {
  it("the trigger sits in the head right after the chip; Edit Tempo text is an icon with a tip", async () => {
    await open();
    const chip = screen.getByText("Not in Tempo");
    expect(chip.nextElementSibling).toBe(btn("Preview send Thu 1 Oct to Tempo"));
    const edit = btn("Edit Tempo text for Thu 1 Oct");
    expect(edit.getAttribute("data-tip")).toBe("Edit Tempo text");
    expect(edit.textContent).toBe("");
  });

  it("states the rounding rule", async () => {
    await open([day({ line_seconds: 1800, tracked_seconds: 6120 })]);
    expect(screen.getByText("Rounded to the nearest half hour from 1h 42m tracked")).toBeTruthy();
  });

  it("the hours name starts with its visible text and has no title", async () => {
    await open();
    expect(btn("1h 30m — edit hours for Thu 1 Oct").hasAttribute("title")).toBe(false);
  });
});

describe("older days", () => {
  const days = (n: number) =>
    Array.from({ length: n }, (_, i) => {
      const d = `2026-10-${String(n - i).padStart(2, "0")}`;
      return day({ day: d, blocks: [block({ id: i + 1, day: d })] });
    });

  it("shows the newest 5 and expands the rest", async () => {
    await open(days(7));
    expect(document.querySelectorAll(".task-day")).toHaveLength(5);
    fireEvent.click(btn("Show 2 older days"));
    expect(document.querySelectorAll(".task-day")).toHaveLength(7);
    expect(screen.queryByRole("button", { name: /older day/ })).toBeNull();
  });

  it("opening the log form scrolls the section head into view", async () => {
    await open();
    const scroll = mock(() => {});
    (document.querySelector(".task-work-head") as HTMLElement).scrollIntoView = scroll;
    fireEvent.click(btn("Log time"));
    expect(scroll).toHaveBeenCalledWith({ block: "nearest" });
  });
});

describe("styles", () => {
  const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
  it("pressed chip is inverted and the buttons have hover states", () => {
    expect(css).toContain('[aria-pressed="true"] { background: var(--fg); border-color: var(--fg); color: var(--bg);');
    expect(css).toContain(".task-btn-primary:hover:not(:disabled)");
    expect(css).toContain(".task-btn-secondary:hover:not(:disabled) { background: var(--bg-sunk); }");
    expect(css).toContain("transition: background-color 120ms cubic-bezier(0.25, 1, 0.5, 1)");
  });
});
