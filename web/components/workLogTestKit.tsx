// Shared fixtures for the TaskWorkLog* / TaskLogTime tests. Buttons are found by their accessible name
// (aria-label, else text) with a plain DOM scan: getByRole walks the whole tree and costs ~0.5s a call here.

import { expect, mock } from "bun:test";
import { act, fireEvent, render } from "@testing-library/react";
import type { JiraTicket, RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskWorkLog } from "./TaskWorkLog";
import { useWorkLog } from "./useWorkLog";

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

export const payload = (days: TicketDay[]): TicketBlocks => ({
  key: "ABC-1",
  from: "2026-09-19",
  to: "2026-10-02",
  days,
  in_tempo_total_seconds: null,
  pulled_at: null,
  today: { day: "2026-10-02", worked_seconds: 0, in_tempo_seconds: null, ticket_worked_seconds: 0, ticket_in_tempo_seconds: null },
});

const ok = { ok: true as const, data: {} };
/** A real daemon dry run: never counted as synced, the line to send comes back as a `dry-run` row. */
export const dryRunData = { synced: 0, skipped: 0, errors: [] as string[], results: [{ block_id: 1, status: "dry-run", reason: null }] };
export const syncOk = () =>
  mock(async (_d: string, dry: boolean) => ({ ok: true as const, data: dry ? dryRunData : { synced: 1, errors: [], results: [] as never[] } }));
export const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

/** What the modal does around the tab: owns the loaded days, so the tests exercise the tab on its own. */
export function Harness({ actions, onAnnounce, tickets }: { actions: TaskActions; onAnnounce: (m: string) => void; tickets?: JiraTicket[] }) {
  const work = useWorkLog("ABC-1", actions);
  return <TaskWorkLog taskKey="ABC-1" actions={actions} work={work} onAnnounce={onAnnounce} tickets={tickets} />;
}

export async function open(days: TicketDay[] = [day()], over: Record<string, unknown> = {}, tickets?: JiraTicket[]) {
  const a = {
    loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload(days) })),
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineHours: mock(async () => ok),
    saveTempoLineText: mock(async () => ok),
    runSync: syncOk(),
    loadPreflight: mock(async () => ({ ok: true as const, data: [] })),
    loadReadBack: mock(async () => ({ ok: true as const, data: { check: "read_back", ok: true, target: null, detail: "Tempo matches" } })),
    ...over,
  } as unknown as TaskActions;
  const onAnnounce = mock((_m: string) => {});
  // act flushes the resolved load directly; waitFor's polling cost a flat 1s per call here.
  await act(async () => {
    render(<Harness actions={a} onAnnounce={onAnnounce} tickets={tickets} />);
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

/** The "⋯" button on a day row. */
export const more = (label = "Thu 1 Oct") => btn(`More for ${label}`);

/** Opens a day's "⋯" menu and picks an item from it. */
export function pick(item: string, label = "Thu 1 Oct") {
  fireEvent.click(more(label));
  fireEvent.click(btn(item));
}

/** A day row's disclosure button. */
export const toggle = (label = "Thu 1 Oct") =>
  [...document.querySelectorAll<HTMLButtonElement>(".task-day-toggle")].find((b) => b.textContent?.includes(label)) as HTMLButtonElement;

/** Flush pending promises and the renders they cause (RTL's waitFor polls with a ~1s floor in this setup). */
export const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
