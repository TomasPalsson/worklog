// My Tasks board: focus after a move, chooser focus, Undo pause, Try again, selected card.

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
  issue_type: null,
  priority: null,
  due_date: null,
  labels: [],
  parent_summary: null,
  updated: null,
  day_seconds: [0, 0, 0, 0, 0, 0, 0],
  ...over,
});

const start: Transition = { id: "11", name: "Start", to_status: "In Progress", to_category: "indeterminate" };
const review: Transition = { id: "12", name: "Send to review", to_status: "In Review", to_category: "indeterminate" };
const back: Transition = { id: "21", name: "Reopen", to_status: "To Do", to_category: "new" };

function actions(transitions: Transition[], over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: transitions })),
    loadTicketDetail: mock(async () => ({ ok: false as const, error: "unused" })),
    transitionTicket: mock(async (key: string, id: string) => {
      const t = [start, review, back].find((x) => x.id === id) as Transition;
      return { ok: true as const, data: { key, status: t.to_status, status_category: t.to_category } };
    }),
    commentOnTicket: mock(async () => ({ ok: true as const, data: { ok: true as const } })),
    draftTicketUpdate: mock(async () => ({ ok: false as const, error: "unused" })),
    ...over,
  } as unknown as TaskActions;
}

const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;
const col = (id: string) => screen.getByTestId(`column-${id}`);
const card = (key: string) => screen.getByTestId(`card-${key}`);
const cardBtn = (key: string) => card(key).querySelector(".task-card-btn") as HTMLElement;
const live = () => screen.getByRole("status").textContent;
const dataTransfer = { setData() {}, getData: () => "", effectAllowed: "" };
const wait = (ms: number) =>
  act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });

async function moveVia(key: string, item: string) {
  fireEvent.click(screen.getByRole("button", { name: `Move ${key}` }));
  const entry = await screen.findByRole("menuitem", { name: item });
  await act(async () => {
    fireEvent.click(entry);
  });
}

describe("focus after a move", () => {
  it("lands on the card's main button in the target column after a Move-menu move", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} />);
    await moveVia("ABC-1", "Move to In Progress");
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(cardBtn("ABC-1")));
    expect(col("indeterminate").contains(document.activeElement)).toBe(true);
  });

  it("Undo and a chooser pick also hand focus back to the card", async () => {
    render(<TaskBoard actions={actions([start, review, back])} tasks={[row()]} />);
    await moveVia("ABC-1", "Move to In Progress");
    await act(async () => {
      fireEvent.click(await screen.findByRole("button", { name: "Send to review → In Review" }));
    });
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(cardBtn("ABC-1")));
    await act(async () => {
      fireEvent.click(within(card("ABC-1")).getByRole("button", { name: "Undo" }));
    });
    // Undo into To Do has exactly one way back, so it applies at once.
    await waitFor(() => expect(within(col("new")).getByText("Fix login")).not.toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(cardBtn("ABC-1")));
  });

  it("a drag move does not steal focus", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} />);
    (document.activeElement as HTMLElement | null)?.blur();
    fireEvent.dragStart(card("ABC-1"), { dataTransfer });
    fireEvent.dragOver(col("indeterminate"), { dataTransfer });
    await act(async () => {
      fireEvent.drop(col("indeterminate"), { dataTransfer });
    });
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(card("ABC-1").contains(document.activeElement)).toBe(false);
  });
});

describe("chooser", () => {
  it("focuses its first option and announces itself; Cancel and Esc return focus to the card", async () => {
    render(<TaskBoard actions={actions([start, review])} tasks={[row()]} />);
    await moveVia("ABC-1", "Move to In Progress");
    const chooser = await screen.findByRole("group", { name: "Move ABC-1 to In Progress?" });
    expect(document.activeElement).toBe(within(chooser).getAllByRole("button")[0]);
    expect(live()).toBe("Choose how to move ABC-1 to In Progress.");
    fireEvent.click(within(chooser).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("group")).toBeNull();
    expect(document.activeElement).toBe(cardBtn("ABC-1"));

    await moveVia("ABC-1", "Move to In Progress");
    await screen.findByRole("group");
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(screen.queryByRole("group")).toBeNull();
    expect(document.activeElement).toBe(cardBtn("ABC-1"));
  });
});

describe("Undo countdown", () => {
  const undoBtn = () => within(card("ABC-1")).queryByRole("button", { name: "Undo" });

  it("announces Undo and renders the strip without a dangling dot", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} />);
    await moveVia("ABC-1", "Move to In Progress");
    await waitFor(() => expect(live()).toBe("Moved ABC-1 to In Progress. Undo available."));
    expect(within(card("ABC-1")).getByText("Moved to In Progress")).not.toBeNull();
  });

  it("pauses while hovered and resumes with the time left", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} undoMs={300} />);
    await moveVia("ABC-1", "Move to In Progress");
    await waitFor(() => expect(undoBtn()).not.toBeNull());
    // The move left focus on the card; drop it so only the hover is holding the timer.
    fireEvent.blur(cardBtn("ABC-1"), { relatedTarget: document.body });
    fireEvent.mouseEnter(card("ABC-1"));
    await wait(450);
    expect(undoBtn()).not.toBeNull();
    fireEvent.mouseLeave(card("ABC-1"));
    await wait(450);
    expect(undoBtn()).toBeNull();
  });

  it("pauses while focus is inside the card", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} undoMs={300} />);
    await moveVia("ABC-1", "Move to In Progress");
    await waitFor(() => expect(document.activeElement).toBe(cardBtn("ABC-1")));
    await wait(450);
    expect(undoBtn()).not.toBeNull();
    fireEvent.blur(cardBtn("ABC-1"), { relatedTarget: document.body });
    await wait(450);
    expect(undoBtn()).toBeNull();
  });
});

describe("card error and selection", () => {
  it("Try again re-runs the same move into the same column", async () => {
    let n = 0;
    const a = actions([start], {
      transitionTicket: mock(async (key: string) =>
        n++ === 0
          ? { ok: false as const, error: "Resolution is required" }
          : { ok: true as const, data: { key, status: "In Progress", status_category: "indeterminate" as const } },
      ),
    });
    render(<TaskBoard actions={a} tasks={[row()]} />);
    await moveVia("ABC-1", "Move to In Progress");
    await within(card("ABC-1")).findByText(/Resolution is required/);
    await act(async () => {
      fireEvent.click(within(card("ABC-1")).getByRole("button", { name: "Try again" }));
    });
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(calls(a.transitionTicket)).toEqual([
      ["ABC-1", "11"],
      ["ABC-1", "11"],
    ]);
  });

  it("marks the card whose panel is open", async () => {
    render(<TaskBoard actions={actions([start])} tasks={[row()]} />);
    expect(card("ABC-1").getAttribute("data-selected")).toBeNull();
    fireEvent.click(cardBtn("ABC-1"));
    await screen.findByRole("dialog");
    expect(card("ABC-1").getAttribute("data-selected")).toBe("true");
  });

  it("the panel status menu says plainly when Jira offers nothing", async () => {
    render(<TaskBoard actions={actions([])} tasks={[row()]} />);
    fireEvent.click(cardBtn("ABC-1"));
    await screen.findByRole("dialog");
    await act(async () => {
      fireEvent.click(screen.getByTestId("status-ABC-1"));
    });
    expect(await screen.findByText("Jira offers no status changes for this ticket right now.")).not.toBeNull();
  });
});
