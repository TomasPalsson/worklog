// My Tasks board: the Move menu asks Jira once on open and disables columns Jira cannot reach.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { TaskRow, Transition } from "@/lib/types";
import { TaskBoard } from "./TaskBoard";
import type { TaskActions } from "./TaskCard";

afterEach(cleanup);

const task: TaskRow = {
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  url: null,
  assigned: true,
  week_seconds: 5400,
  today_seconds: 0,
  last_worked_day: null,
  issue_type: null,
  priority: null,
  due_date: null,
  labels: [],
  parent_summary: null,
  updated: null,
  day_seconds: [0, 0, 0, 0, 0, 0, 0],
};
const tasks = [task];
const start: Transition = { id: "11", name: "Start", to_status: "In Progress", to_category: "indeterminate" };
const review: Transition = { id: "12", name: "Send to review", to_status: "In Review", to_category: "indeterminate" };
const done: Transition = { id: "31", name: "Finish", to_status: "Done", to_category: "done" };

function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: [start, done] })),
    loadTicketDetail: mock(async () => ({ ok: false as const, error: "unused" })),
    loadTicketBlocks: mock(async () => ({ ok: false as const, error: "unused" })),
    transitionTicket: mock(async (key: string) => ({
      ok: true as const,
      data: { key, status: "In Progress", status_category: "indeterminate" as const },
    })),
    commentOnTicket: mock(async () => ({ ok: true as const, data: { ok: true as const } })),
    draftTicketUpdate: mock(async () => ({ ok: false as const, error: "unused" })),
    ...over,
  } as unknown as TaskActions;
}

const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;
const col = (id: string) => screen.getByTestId(`column-${id}`);
const card = (key: string) => screen.getByTestId(`card-${key}`);

const moveVia = async (key: string, item: string) => {
  fireEvent.click(screen.getByRole("button", { name: `Move ${key}` }));
  const entry = await screen.findByRole("menuitem", { name: item });
  await act(async () => {
    fireEvent.click(entry);
  });
};

describe("Move menu transitions lookup", () => {
  it("opening the menu asks Jira once, shows Checking Jira… until it answers, and the pick reuses the answer", async () => {
    let resolve: (v: unknown) => void = () => {};
    const a = actions({ loadTransitions: mock(() => new Promise((res) => (resolve = res))) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    fireEvent.click(screen.getByRole("button", { name: "Move ABC-1" }));
    const waiting = screen.getAllByRole("menuitem");
    expect(waiting.map((n) => n.textContent)).toEqual(["Blocked — Checking Jira…", "In Progress — Checking Jira…", "Verification — Checking Jira…", "Done — Checking Jira…"]);
    expect(waiting.every((n) => n.getAttribute("aria-disabled") === "true")).toBe(true);
    await act(async () => resolve({ ok: true, data: [start, done] }));
    await act(async () => {
      fireEvent.click(await screen.findByRole("menuitem", { name: "Move to In Progress" }));
    });
    await waitFor(() => expect(calls(a.transitionTicket).length).toBe(1));
    expect(calls(a.loadTransitions).length).toBe(1);
  });

  it("a column Jira cannot reach is disabled and reads no Jira move; clicking it does nothing", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    fireEvent.click(screen.getByRole("button", { name: "Move ABC-1" }));
    const none = await screen.findByRole("menuitem", { name: "Done — no Jira move" });
    expect(none.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(none);
    expect(screen.queryByRole("menu")).not.toBeNull();
    expect(card("ABC-1").textContent).not.toContain("Couldn't move");
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.loadTransitions).length).toBe(1);
  });

  it("several matches open the chooser from the menu without a second lookup", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, review, done] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to In Progress");
    expect(await screen.findByRole("group", { name: "Move ABC-1 to In Progress?" })).not.toBeNull();
    expect(calls(a.loadTransitions).length).toBe(1);
  });

  it("if the lookup fails both items stay enabled and a pick falls back to the error flow", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: false as const, error: "jira down" })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to Done");
    expect(await within(card("ABC-1")).findByText(/jira down/)).not.toBeNull();
    expect(calls(a.loadTransitions).length).toBe(2); // menu open + the fallback re-check
  });

  it("one matching transition runs exactly like a drop", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to In Progress");
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
  });

  it("several matching transitions open the chooser", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, review, done] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to In Progress");
    expect(await screen.findByRole("group", { name: "Move ABC-1 to In Progress?" })).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(0);
  });
});
