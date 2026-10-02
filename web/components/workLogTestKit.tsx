// Shared fixtures for the TaskWorkLog* / TaskLogTime tests. Buttons are found by their accessible name
// (aria-label, else text) with a plain DOM scan: getByRole walks the whole tree and costs ~0.5s a call here.

import { expect, mock } from "bun:test";
import { act, render } from "@testing-library/react";
import type { RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskWorkLog } from "./TaskWorkLog";

export const block = (over: Partial<RawBlock> = {}): RawBlock =>
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

export const day = (over: Partial<TicketDay> = {}): TicketDay => ({
  day: "2026-10-01",
  line_seconds: 5400,
  line_text: "Worked on login",
  tracked_seconds: 5400,
  hours_set_by_hand: false,
  in_tempo_seconds: null,
  blocks: [block()],
  ...over,
});

export const changedDay = (over: Partial<TicketDay> = {}) =>
  day({ in_tempo_seconds: 3600, blocks: [block({ tempo_worklog_id: "w1", dirty: true })], ...over });

export const payload = (days: TicketDay[]): TicketBlocks => ({ key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days });

const ok = { ok: true as const, data: {} };
export const syncOk = () => mock(async () => ({ ok: true as const, data: { synced: 1, errors: [], results: [] as never[] } }));
export const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

export async function open(days: TicketDay[] = [day()], over: Record<string, unknown> = {}) {
  const a = {
    loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload(days) })),
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineHours: mock(async () => ok),
    saveTempoLineText: mock(async () => ok),
    runSync: syncOk(),
    ...over,
  } as unknown as TaskActions;
  const onAnnounce = mock((_m: string) => {});
  // act flushes the resolved load directly; waitFor's polling cost a flat 1s per call here.
  await act(async () => {
    render(<TaskWorkLog taskKey="ABC-1" actions={a} onAnnounce={onAnnounce} />);
  });
  expect(document.querySelector(".task-skel")).toBeNull();
  return { a, onAnnounce };
}

const nameOf = (b: Element) => b.getAttribute("aria-label") ?? (b.textContent ?? "").trim();
const matches = (n: string, name: string | RegExp) => (typeof name === "string" ? n === name : name.test(n));

/** All buttons whose accessible name matches. */
export const btns = (name: string | RegExp): HTMLButtonElement[] =>
  [...document.querySelectorAll("button")].filter((b) => matches(nameOf(b), name)) as HTMLButtonElement[];

/** The one button with this name (throws on none or several, like getByRole). */
export const btn = (name: string | RegExp): HTMLButtonElement => {
  const found = btns(name);
  if (found.length !== 1) throw new Error(`expected 1 button named ${String(name)}, found ${found.length}`);
  return found[0];
};

export const logTime = () => btn("Log time");
export const hoursBtn = () => btn("1h 30m — edit hours for Thu 1 Oct");
export const textBtn = () => btn("Edit Tempo text for Thu 1 Oct");

/** Flush pending promises and the renders they cause (RTL's waitFor polls with a ~1s floor in this setup). */
export const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
