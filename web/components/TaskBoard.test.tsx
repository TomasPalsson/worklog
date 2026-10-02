// B13: My Tasks board — cards, status menu, comment box, Draft with AI.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TaskRow, TicketDraft, Transition } from "@/lib/types";
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
const done: Transition = { id: "31", name: "Finish", to_status: "Done", to_category: "done" };

function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: [start, done] })),
    transitionTicket: mock(async (key: string) => ({
      ok: true as const,
      data: { key, status: "In Progress", status_category: "indeterminate" as const },
    })),
    commentOnTicket: mock(async () => ({ ok: true as const, data: { ok: true as const } })),
    draftTicketUpdate: mock(async () => ({
      ok: true as const,
      data: {
        comment: "Fixed the login redirect.",
        suggested_transition_id: "31",
        transitions: [start, done],
      } satisfies TicketDraft,
    })),
    ...over,
  } as unknown as TaskActions;
}

const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

describe("TaskBoard", () => {
  it("splits assigned tickets from other worked ones and shows hours", () => {
    render(
      <TaskBoard
        actions={actions()}
        tasks={[
          row({}),
          row({ key: "ABC-2", summary: "Spike cache", assigned: false, week_seconds: 1800, today_seconds: 1800 }),
        ]}
      />,
    );
    expect(screen.getByRole("heading", { name: "Assigned" })).not.toBeNull();
    expect(screen.getByRole("heading", { name: "Worked this week" })).not.toBeNull();
    expect(screen.getByText("Fix login")).not.toBeNull();
    expect(screen.getByRole("link", { name: "ABC-1" }).getAttribute("href")).toBe(
      "https://x.atlassian.net/browse/ABC-1",
    );
    expect(screen.getByText("1h 30m this week")).not.toBeNull();
    expect(screen.getByText("30m today")).not.toBeNull();
  });

  it("shows an empty state when there are no tickets", () => {
    render(<TaskBoard actions={actions()} tasks={[]} />);
    expect(screen.getByText("No tickets yet.")).not.toBeNull();
  });

  it("moves a ticket through the status menu and shows the new status", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={[row({})]} />);
    fireEvent.click(screen.getByRole("button", { name: "Change status" }));
    fireEvent.click(await screen.findByRole("button", { name: "Start → In Progress" }));
    await waitFor(() => expect(screen.getByTestId("status-ABC-1").textContent).toBe("In Progress"));
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
  });

  it("posts a trimmed comment, clears the box, and disables Post when empty", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={[row({})]} />);
    const post = screen.getByRole("button", { name: "Post comment" }) as HTMLButtonElement;
    expect(post.disabled).toBe(true);
    const box = screen.getByRole("textbox", { name: "Comment on ABC-1" }) as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "  shipped  " } });
    fireEvent.click(post);
    await waitFor(() => expect(box.value).toBe(""));
    expect(calls(a.commentOnTicket)[0]).toEqual(["ABC-1", "shipped"]);
  });

  it("Draft with AI fills the comment and flags the suggested transition without applying it", async () => {
    const a = actions();
    render(<TaskBoard actions={a} tasks={[row({})]} />);
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    const box = screen.getByRole("textbox", { name: "Comment on ABC-1" }) as HTMLTextAreaElement;
    await waitFor(() => expect(box.value).toBe("Fixed the login redirect."));
    expect(await screen.findByRole("button", { name: "Finish → Done (suggested)" })).not.toBeNull();
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.commentOnTicket).length).toBe(0);
  });

  it("surfaces a failed draft and leaves the comment untouched", async () => {
    const a = actions({
      draftTicketUpdate: mock(async () => ({ ok: false as const, error: "claude timed out" })),
    });
    render(<TaskBoard actions={a} tasks={[row({})]} />);
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    expect((await screen.findByRole("alert")).textContent).toBe("claude timed out");
    expect((screen.getByRole("textbox", { name: "Comment on ABC-1" }) as HTMLTextAreaElement).value).toBe("");
  });

  async function draftThenEdit(a: TaskActions) {
    render(<TaskBoard actions={a} tasks={[row({})]} />);
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    const box = screen.getByRole("textbox", { name: "Comment on ABC-1" }) as HTMLTextAreaElement;
    await waitFor(() => expect(box.value).toBe("Fixed the login redirect."));
    return box;
  }

  it("Post applies the kept suggested transition before the comment", async () => {
    const order: string[] = [];
    const a = actions({
      transitionTicket: mock(async (key: string) => {
        order.push("transition");
        return { ok: true as const, data: { key, status: "Done", status_category: "done" as const } };
      }),
      commentOnTicket: mock(async () => {
        order.push("comment");
        return { ok: true as const, data: { ok: true as const } };
      }),
    });
    const box = await draftThenEdit(a);
    fireEvent.click(screen.getByRole("button", { name: "Post comment" }));
    await waitFor(() => expect(box.value).toBe(""));
    expect(order).toEqual(["transition", "comment"]);
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "31"]);
    expect(calls(a.commentOnTicket)[0]).toEqual(["ABC-1", "Fixed the login redirect."]);
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("Done");
  });

  it("dropping the suggestion makes Post comment only", async () => {
    const a = actions();
    const box = await draftThenEdit(a);
    fireEvent.click(screen.getByRole("button", { name: "Drop suggestion" }));
    expect(screen.queryByRole("button", { name: "Finish → Done (suggested)" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Post comment" }));
    await waitFor(() => expect(box.value).toBe(""));
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.commentOnTicket).length).toBe(1);
  });

  it("a rejected transition shows Jira's error, keeps the draft, and posts no comment", async () => {
    const a = actions({
      transitionTicket: mock(async () => ({ ok: false as const, error: "Resolution is required" })),
    });
    const box = await draftThenEdit(a);
    fireEvent.click(screen.getByRole("button", { name: "Post comment" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Resolution is required");
    expect(box.value).toBe("Fixed the login redirect.");
    expect(calls(a.commentOnTicket).length).toBe(0);
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("To Do");
  });
});
