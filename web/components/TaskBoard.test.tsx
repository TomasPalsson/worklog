// My Tasks board: columns, filters, drag-to-move, opening the panel.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { TaskRow, TicketDetail, Transition } from "@/lib/types";
import { TaskBoard } from "./TaskBoard";
import type { TaskActions } from "./TaskCard";

afterEach(cleanup);

const row = (over: Partial<TaskRow>): TaskRow => ({
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
const review: Transition = { id: "12", name: "Send to review", to_status: "In Review", to_category: "indeterminate" };
const done: Transition = { id: "31", name: "Finish", to_status: "Done", to_category: "done" };

const detail: TicketDetail = {
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  issue_type: null,
  priority: null,
  assignee: null,
  updated: null,
  url: "https://x.atlassian.net/browse/ABC-1",
  description: "Long description",
  comments: [],
};

function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: [start, done] })),
    loadTicketDetail: mock(async () => ({ ok: true as const, data: detail })),
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
const cardBtn = (key: string) => card(key).querySelector(".task-card-btn") as HTMLElement;
const live = () => screen.getByRole("status").textContent;

const dataTransfer = { setData() {}, getData: () => "", effectAllowed: "" };

async function drag(key: string, target: string) {
  fireEvent.dragStart(card(key), { dataTransfer });
  fireEvent.dragOver(col(target), { dataTransfer });
  await act(async () => {
    fireEvent.drop(col(target), { dataTransfer });
  });
}

const tasks = [
  row({}),
  row({ key: "ABC-2", summary: "Spike cache", status: "In Progress", status_category: "indeterminate", assigned: false, week_seconds: 1800, today_seconds: 1800 }),
  row({ key: "ABC-3", summary: "Ship docs", status: "Done", status_category: "done", week_seconds: 0 }),
  row({ key: "ABC-4", summary: "Unknown status", status: null, status_category: null, week_seconds: 0 }),
];

describe("TaskBoard columns", () => {
  it("puts each card in its category column (null goes to To do) with counts in the heads", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(within(col("new")).getByText("Fix login")).not.toBeNull();
    expect(within(col("new")).getByText("Unknown status")).not.toBeNull();
    expect(within(col("indeterminate")).getByText("Spike cache")).not.toBeNull();
    expect(within(col("done")).getByText("Ship docs")).not.toBeNull();
    expect(within(col("new")).getByTestId("count-new").textContent).toBe("2");
    expect(within(col("done")).getByTestId("count-done").textContent).toBe("1");
    expect(screen.getByRole("heading", { name: /To do/ })).not.toBeNull();
  });

  it("shows the key, the not-assigned tag for unassigned tickets, and hours (today only when worked)", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(within(card("ABC-1")).getByText("ABC-1")).not.toBeNull();
    expect(within(card("ABC-1")).queryByText("not assigned")).toBeNull();
    const tag = within(card("ABC-2")).getByText("not assigned");
    expect(tag.getAttribute("title")).toContain("Not assigned to you");
    expect(within(card("ABC-1")).getByText("1h 30m this week")).not.toBeNull();
    expect(within(card("ABC-2")).getByText("30m this week · 30m today")).not.toBeNull();
    expect(within(card("ABC-3")).getByText("Not worked this week")).not.toBeNull();
  });

  it("shows an empty state when there are no tickets at all", () => {
    render(<TaskBoard actions={actions()} tasks={[]} />);
    expect(screen.getByText(/No tickets cached yet\./)).not.toBeNull();
  });
});

describe("TaskBoard filters", () => {
  it("narrows by key and by summary, case-insensitively, and says so in empty columns", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    const filter = screen.getByLabelText("Filter");
    fireEvent.change(filter, { target: { value: "abc-2" } });
    expect(screen.queryByText("Fix login")).toBeNull();
    expect(screen.getByText("Spike cache")).not.toBeNull();
    fireEvent.change(filter, { target: { value: "DOCS" } });
    expect(screen.queryByText("Spike cache")).toBeNull();
    expect(screen.getByText("Ship docs")).not.toBeNull();
    expect(within(col("new")).getByText("No tickets match “DOCS”.")).not.toBeNull();
  });

  it("'Only tickets I worked this week' hides cards with no hours", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(screen.getByLabelText("Only tickets I worked this week"));
    expect(screen.queryByText("Ship docs")).toBeNull();
    expect(screen.queryByText("Unknown status")).toBeNull();
    expect(screen.getByText("Fix login")).not.toBeNull();
  });
});

describe("TaskBoard drag to move", () => {
  it("a drop with one matching transition transitions the ticket and the card lands in the column", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "indeterminate");
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
    expect(within(col("new")).queryByText("Fix login")).toBeNull();
  });

  it("the response's status category decides the final column", async () => {
    const a = actions({
      transitionTicket: mock(async (key: string) => ({
        ok: true as const,
        data: { key, status: "Done", status_category: "done" as const },
      })),
    });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "indeterminate");
    await waitFor(() => expect(within(col("done")).getByText("Fix login")).not.toBeNull());
  });

  it("two matching transitions open a chooser; picking one applies it; Esc cancels", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, review, done] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "indeterminate");
    const chooser = await within(col("indeterminate")).findByRole("group", { name: "Move ABC-1 to In progress?" });
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(within(col("new")).getByText("Fix login")).not.toBeNull();
    expect(within(chooser).getByRole("button", { name: "Start → In Progress" })).not.toBeNull();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("group", { name: /Move ABC-1/ })).toBeNull();

    await drag("ABC-1", "indeterminate");
    fireEvent.click(await screen.findByRole("button", { name: "Send to review → In Review" }));
    await waitFor(() => expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "12"]));
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
  });

  it("no matching transition: no call, the card stays and the error names the move", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "done");
    expect(
      await within(card("ABC-1")).findByText("Couldn't move to Done — Jira has no move from To Do to Done."),
    ).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(within(col("new")).getByText("Fix login")).not.toBeNull();
  });

  it("a failed transition returns the card and shows the error on it", async () => {
    const a = actions({
      transitionTicket: mock(async () => ({ ok: false as const, error: "Resolution is required" })),
    });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "indeterminate");
    expect(await within(card("ABC-1")).findByText("Couldn't move to In progress — Resolution is required")).not.toBeNull();
    expect(within(col("new")).getByText("Fix login")).not.toBeNull();
  });

  it("a failed transitions load returns the card with that error", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: false as const, error: "jira down" })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "done");
    expect(await within(card("ABC-1")).findByText("Couldn't move to Done — jira down")).not.toBeNull();
    expect(within(col("new")).getByText("Fix login")).not.toBeNull();
  });

  it("dropping on the card's own column calls nothing", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "new");
    expect(calls(a.loadTransitions).length).toBe(0);
    expect(calls(a.transitionTicket).length).toBe(0);
  });

  it("an empty column without a filter shows a drop hint; with a filter it says nothing matches", () => {
    render(<TaskBoard actions={actions()} tasks={[row({})]} />);
    expect(within(col("done")).getByText("Drop a ticket here")).not.toBeNull();
    expect(within(col("new")).queryByText("Drop a ticket here")).toBeNull();
    fireEvent.change(screen.getByLabelText("Filter"), { target: { value: "zzz" } });
    expect(within(col("done")).queryByText("Drop a ticket here")).toBeNull();
    expect(within(col("done")).getByText("No tickets match “zzz”.")).not.toBeNull();
  });

  it("the hovered column says release to move", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.dragStart(card("ABC-1"), { dataTransfer });
    fireEvent.dragOver(col("done"), { dataTransfer });
    expect(within(col("done")).getByText(/release to move/)).not.toBeNull();
  });
});

const moveVia = async (key: string, item: string) => {
  fireEvent.click(screen.getByRole("button", { name: `Move ${key}` }));
  await act(async () => {
    fireEvent.click(screen.getByRole("menuitem", { name: item }));
  });
};

describe("TaskBoard Move menu", () => {
  it("lists only the other columns and has menu semantics", () => {
    render(<TaskBoard actions={actions()} tasks={[row({})]} />);
    const btn = screen.getByRole("button", { name: "Move ABC-1" });
    expect(btn.getAttribute("aria-haspopup")).toBe("menu");
    expect(btn.parentElement?.parentElement).toBe(card("ABC-1"));
    fireEvent.click(btn);
    const items = screen.getAllByRole("menuitem").map((n) => n.textContent);
    expect(items).toEqual(["Move to In progress", "Move to Done"]);
  });

  it("one matching transition runs exactly like a drop", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to In progress");
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
  });

  it("several matching transitions open the chooser", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, review, done] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to In progress");
    expect(await screen.findByRole("group", { name: "Move ABC-1 to In progress?" })).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(0);
  });

  it("no matching transition shows the error and calls nothing", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await moveVia("ABC-1", "Move to Done");
    expect(await within(card("ABC-1")).findByText(/Couldn't move to Done — Jira has no move/)).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(0);
  });

  it("Esc closes the menu, returns focus to the Move button and keeps the panel open", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    await screen.findByRole("dialog");
    const btn = screen.getByRole("button", { name: "Move ABC-1" });
    fireEvent.click(btn);
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Move to Done" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(btn);
    expect(screen.queryByRole("dialog")).not.toBeNull();
  });
});

describe("TaskBoard outcomes", () => {
  it("announces a successful move politely", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
    await drag("ABC-1", "indeterminate");
    await waitFor(() => expect(live()).toBe("Moved ABC-1 to In progress."));
    expect(card("ABC-1").getAttribute("data-landed")).toBe("true");
  });

  it("announces a failed move, and Dismiss error restores the hours line", async () => {
    const a = actions({
      transitionTicket: mock(async () => ({ ok: false as const, error: "Resolution is required" })),
    });
    render(<TaskBoard actions={a} tasks={tasks} />);
    await drag("ABC-1", "indeterminate");
    await within(card("ABC-1")).findByText(/Resolution is required/);
    expect(live()).toBe("Couldn't move ABC-1 to In progress.");
    expect(within(card("ABC-1")).queryByText("1h 30m this week")).toBeNull();
    fireEvent.click(within(card("ABC-1")).getByRole("button", { name: "Dismiss error" }));
    expect(within(card("ABC-1")).getByText("1h 30m this week")).not.toBeNull();
    expect(within(card("ABC-1")).queryByText(/Resolution is required/)).toBeNull();
  });
});

describe("TaskBoard toolbar and layout", () => {
  it("shows today's hours only when there are some", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(within(card("ABC-2")).getByText("30m this week · 30m today")).not.toBeNull();
    expect(within(card("ABC-1")).queryByText(/today/)).toBeNull();
  });

  it("clear filter empties the input and the button goes away", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    const input = screen.getByLabelText("Filter") as HTMLInputElement;
    expect(screen.queryByRole("button", { name: "Clear filter" })).toBeNull();
    fireEvent.change(input, { target: { value: "docs" } });
    fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    expect(input.value).toBe("");
    expect(screen.queryByRole("button", { name: "Clear filter" })).toBeNull();
    expect(screen.getByText("Fix login")).not.toBeNull();
  });
});

describe("TaskBoard panel", () => {
  it("opens on card click, and Esc closes it and returns focus to the card button", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    expect(screen.queryByRole("dialog")).toBeNull();
    const button = cardBtn("ABC-1");
    fireEvent.click(button);
    expect(await screen.findByText("Long description")).not.toBeNull();
    expect(calls(a.loadTicketDetail)[0]).toEqual(["ABC-1"]);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("Esc with the chooser open cancels only the chooser; the panel stays", async () => {
    const a = actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, review, done] })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    await screen.findByText("Long description");
    await drag("ABC-1", "indeterminate");
    await screen.findByRole("group", { name: /Move ABC-1/ });
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("group", { name: /Move ABC-1/ })).toBeNull();
    expect(screen.queryByRole("dialog")).not.toBeNull();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("an unsent comment survives closing the panel and switching cards", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    fireEvent.change(await screen.findByLabelText("Add a comment"), { target: { value: "half written" } });
    fireEvent.click(cardBtn("ABC-2"));
    expect((await screen.findByLabelText("Add a comment") as HTMLTextAreaElement).value).toBe("");
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(cardBtn("ABC-1"));
    expect((await screen.findByLabelText("Add a comment") as HTMLTextAreaElement).value).toBe("half written");
  });

  it("a status change in the panel moves the card to its new column", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    fireEvent.click(await screen.findByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("button", { name: "Start → In Progress" }));
    await waitFor(() => expect(within(col("indeterminate")).getByText("Fix login")).not.toBeNull());
    expect(screen.getByTestId("status-ABC-1").textContent).toContain("In Progress");
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
  });
});
