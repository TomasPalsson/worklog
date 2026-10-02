// My Tasks board: Undo strip, Move menu keyboard, card details.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { TaskRow, Transition } from "@/lib/types";
import { TaskBoard } from "./TaskBoard";
import type { TaskActions } from "./TaskCard";

afterEach(cleanup);

const row = (over: Partial<TaskRow> = {}): TaskRow => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  url: "https://x.atlassian.net/browse/ABC-1",
  assigned: true,
  week_seconds: 5400,
  today_seconds: 0,
  last_worked_day: null,
  ...over,
});

const start: Transition = { id: "11", name: "Start", to_status: "In Progress", to_category: "indeterminate" };
const back: Transition = { id: "21", name: "Reopen", to_status: "To Do", to_category: "new" };
const back2: Transition = { id: "22", name: "Backlog", to_status: "Backlog", to_category: "new" };

/** Transitions offered per call, in order; the last entry repeats. */
function actions(perCall: Transition[][]): TaskActions {
  let n = 0;
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: perCall[Math.min(n++, perCall.length - 1)] })),
    loadTicketDetail: mock(async () => ({ ok: false as const, error: "unused" })),
    transitionTicket: mock(async (key: string, id: string) => {
      const t = [start, back, back2].find((x) => x.id === id) as Transition;
      return { ok: true as const, data: { key, status: t.to_status, status_category: t.to_category } };
    }),
    commentOnTicket: mock(async () => ({ ok: true as const, data: { ok: true as const } })),
    draftTicketUpdate: mock(async () => ({ ok: false as const, error: "unused" })),
  } as unknown as TaskActions;
}

const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;
const card = (key: string) => screen.getByTestId(`card-${key}`);
const col = (id: string) => screen.getByTestId(`column-${id}`);
const dataTransfer = { setData() {}, getData: () => "", effectAllowed: "" };

async function drag(key: string, target: string) {
  fireEvent.dragStart(card(key), { dataTransfer });
  fireEvent.dragOver(col(target), { dataTransfer });
  await act(async () => {
    fireEvent.drop(col(target), { dataTransfer });
  });
}

async function movedWithUndo(perCall: Transition[][], undoMs?: number) {
  const a = actions(perCall);
  render(<TaskBoard actions={a} tasks={[row()]} undoMs={undoMs} />);
  await drag("ABC-1", "indeterminate");
  const undo = await within(card("ABC-1")).findByRole("button", { name: "Undo" });
  return { a, undo };
}

describe("TaskBoard undo", () => {
  it("announces through a task-sr region", () => {
    render(<TaskBoard actions={actions([[start]])} tasks={[row()]} />);
    expect(screen.getByRole("status").className).toBe("task-sr");
  });

  it("shows the strip after a move and one way back applies it", async () => {
    const { a, undo } = await movedWithUndo([[start], [back]]);
    expect(within(card("ABC-1")).getByText("Moved to In Progress")).not.toBeNull();
    await act(async () => {
      fireEvent.click(undo);
    });
    await waitFor(() => expect(within(col("new")).getByText("Fix login")).not.toBeNull());
    expect(calls(a.transitionTicket)[1]).toEqual(["ABC-1", "21"]);
  });

  it("several ways back open the chooser for the previous column", async () => {
    const { a, undo } = await movedWithUndo([[start], [back, back2]]);
    await act(async () => {
      fireEvent.click(undo);
    });
    expect(await screen.findByRole("group", { name: "Move ABC-1 to To Do?" })).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(1);
  });

  it("no way back puts the reason on the card", async () => {
    const { a, undo } = await movedWithUndo([[start], [start]]);
    await act(async () => {
      fireEvent.click(undo);
    });
    expect(
      await within(card("ABC-1")).findByText("Couldn't move to To Do — Jira has no way back to To Do from In Progress."),
    ).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(1);
  });

  it("the strip expires", async () => {
    const { undo } = await movedWithUndo([[start]], 300);
    expect(undo.isConnected).toBe(true);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 350));
    });
    expect(within(card("ABC-1")).queryByRole("button", { name: "Undo" })).toBeNull();
  });

  it("the strip is replaced on the card's next move", async () => {
    await movedWithUndo([[start], [back]]);
    await drag("ABC-1", "new");
    await waitFor(() => expect(within(col("new")).getByText("Fix login")).not.toBeNull());
    expect(within(card("ABC-1")).getByText("Moved to To Do")).not.toBeNull();
    expect(within(card("ABC-1")).getAllByRole("button", { name: "Undo" }).length).toBe(1);
  });
});

describe("TaskBoard card details", () => {
  it("Move trigger has a tooltip and arrow keys cycle the menu with wrap, Home and End", () => {
    render(<TaskBoard actions={actions([[start]])} tasks={[row()]} />);
    const btn = screen.getByRole("button", { name: "Move ABC-1" });
    expect(btn.getAttribute("data-tip")).toBe("Move");
    fireEvent.click(btn);
    const [first, last] = screen.getAllByRole("menuitem");
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "ArrowDown" });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "ArrowDown" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "ArrowUp" });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "Home" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "End" });
    expect(document.activeElement).toBe(last);
  });

  it("hides the not assigned tag in Done, keeps it elsewhere", () => {
    render(
      <TaskBoard
        actions={actions([[start]])}
        tasks={[row({ assigned: false }), row({ key: "ABC-2", assigned: false, status: "Done", status_category: "done" })]}
      />,
    );
    expect(within(card("ABC-1")).queryByText("not assigned")).not.toBeNull();
    expect(within(card("ABC-2")).queryByText("not assigned")).toBeNull();
  });
});
