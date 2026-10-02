// Reading panel + composer: detail load, status menu, Draft with AI, Post.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TaskRow, TicketDetail, TicketDraft, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskPanel } from "./TaskPanel";

afterEach(cleanup);

const row = (over: Partial<TaskRow> = {}): TaskRow => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  url: "https://x.atlassian.net/browse/ABC-1",
  assigned: true,
  week_seconds: 5400,
  today_seconds: 1800,
  last_worked_day: null,
  ...over,
});

const detail = (over: Partial<TicketDetail> = {}): TicketDetail => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  issue_type: "Bug",
  priority: "High",
  assignee: "Ada",
  updated: "2026-09-05T08:07:00",
  url: "https://jira.example/browse/ABC-1",
  description: "Steps to reproduce\nclick login",
  comments: [
    { id: "1", author: "Grace", created: "2026-09-04T09:00:00", body: "first one" },
    { id: "2", author: "Linus", created: "2026-09-05T09:30:00", body: "second one" },
  ],
  ...over,
});

const start: Transition = { id: "11", name: "Start", to_status: "In Progress", to_category: "indeterminate" };
const done: Transition = { id: "31", name: "Finish", to_status: "Done", to_category: "done" };

function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: [start, done] })),
    loadTicketDetail: mock(async () => ({ ok: true as const, data: detail() })),
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

function open(a: TaskActions, task = row()) {
  const onClose = mock(() => {});
  const onStatus = mock((_s: unknown) => {});
  render(<TaskPanel task={task} actions={a} onClose={onClose} onStatus={onStatus} />);
  return { onClose, onStatus };
}

const box = () => screen.getByRole("textbox", { name: "Add a comment" }) as HTMLTextAreaElement;
const post = () => screen.getByRole("button", { name: /^Post/ }) as HTMLButtonElement;

describe("TaskPanel reading", () => {
  it("is a non-modal dialog labelled by the headline, shows the cached row at once, then description and comments", async () => {
    const a = actions();
    open(a);
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("false");
    expect(screen.getByRole("heading", { name: "Fix login" })).not.toBeNull();
    expect(screen.getByText("1h 30m this week · 30m today")).not.toBeNull();
    expect(await screen.findByText(/Steps to reproduce/)).not.toBeNull();
    expect(calls(a.loadTicketDetail)[0]).toEqual(["ABC-1"]);
    expect(screen.getByRole("heading", { name: "Comments · 2" })).not.toBeNull();
    const bodies = screen.getAllByText(/one$/).map((n) => n.textContent);
    expect(bodies).toEqual(["first one", "second one"]);
    expect(screen.getByText("Bug")).not.toBeNull();
    expect(screen.getByText("Ada")).not.toBeNull();
    expect(screen.getByRole("link", { name: /Open in Jira/ }).getAttribute("href")).toBe(
      "https://jira.example/browse/ABC-1",
    );
  });

  it("shows empty-description and empty-comments copy", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ description: "", comments: [] }) })) }));
    expect(await screen.findByText("No description in Jira.")).not.toBeNull();
    expect(screen.getByText("No comments yet.")).not.toBeNull();
  });

  it("shows a load error and Try again re-calls the loader", async () => {
    let n = 0;
    const loader = mock(async () =>
      ++n === 1 ? { ok: false as const, error: "jira down" } : { ok: true as const, data: detail() },
    );
    open(actions({ loadTicketDetail: loader }));
    expect((await screen.findByText("Couldn't load ABC-1 from Jira: jira down")).textContent).toBe("Couldn't load ABC-1 from Jira: jira down");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/Steps to reproduce/)).not.toBeNull();
    expect(loader.mock.calls.length).toBe(2);
  });

  it("moves focus to the dialog on open and Esc or the close button calls onClose", async () => {
    const { onClose } = open(actions());
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    expect(screen.getByRole("heading", { name: "Fix login" }).hasAttribute("tabindex")).toBe(false);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose.mock.calls.length).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Close ABC-1" }));
    expect(onClose.mock.calls.length).toBe(2);
    await screen.findByText(/Steps to reproduce/);
  });

  it("strips a trailing period from a load error reason", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: false as const, error: "jira down." })) }));
    expect((await screen.findByText(/Couldn't load ABC-1/)).textContent).toBe("Couldn't load ABC-1 from Jira: jira down");
  });

  it("shows the live Jira status once detail has loaded", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ status: "In Review", status_category: "indeterminate" }) })) }));
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("To Do");
    await waitFor(() => expect(screen.getByTestId("status-ABC-1").textContent).toBe("In Review"));
    expect(screen.getByTestId("status-ABC-1").getAttribute("data-category")).toBe("indeterminate");
  });

  it("labels a menu move by name only when the name equals the target status", async () => {
    const same: Transition = { id: "41", name: "Done", to_status: "done", to_category: "done" };
    open(actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, same] })) }));
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    expect(await screen.findByRole("button", { name: "Start → In Progress" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Done" })).not.toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });

  it("changes status through the chip menu and reports the new status", async () => {
    const a = actions();
    const { onStatus } = open(a);
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("button", { name: "Start → In Progress" }));
    await waitFor(() => expect(onStatus.mock.calls.length).toBe(1));
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
    expect(onStatus.mock.calls[0][0]).toEqual({ status: "In Progress", status_category: "indeterminate" });
    await screen.findByText(/Steps to reproduce/);
  });
});

describe("TaskComposer", () => {
  it("posts a trimmed comment, clears the box, appends it to the thread and disables Post when empty", async () => {
    const a = actions();
    open(a);
    expect(post().disabled).toBe(true);
    fireEvent.change(box(), { target: { value: "  shipped  " } });
    fireEvent.click(post());
    await waitFor(() => expect(box().value).toBe(""));
    expect(calls(a.commentOnTicket)[0]).toEqual(["ABC-1", "shipped"]);
    expect(await screen.findByText("You")).not.toBeNull();
    expect(screen.getByText("shipped")).not.toBeNull();
  });

  it("Post is disabled while a post is in flight", async () => {
    let resolve!: (v: { ok: true; data: { ok: true } }) => void;
    const pending = new Promise<{ ok: true; data: { ok: true } }>((r) => (resolve = r));
    const a = actions({ commentOnTicket: mock(() => pending) });
    open(a);
    fireEvent.change(box(), { target: { value: "shipped" } });
    expect(post().disabled).toBe(false);
    fireEvent.click(post());
    await waitFor(() => expect(calls(a.commentOnTicket).length).toBe(1));
    expect(box().value).toBe("shipped");
    expect(post().disabled).toBe(true);
    resolve({ ok: true, data: { ok: true } });
    await waitFor(() => expect(box().value).toBe(""));
    fireEvent.change(box(), { target: { value: "again" } });
    expect(post().disabled).toBe(false);
  });

  it("turns Post off and shows the count above 5000 characters", () => {
    open(actions());
    fireEvent.change(box(), { target: { value: "x".repeat(5000) } });
    expect(screen.getByText("5000 / 5000")).not.toBeNull();
    expect(post().disabled).toBe(false);
    fireEvent.change(box(), { target: { value: "x".repeat(5001) } });
    expect(screen.getByText("5001 / 5000")).not.toBeNull();
    expect(post().disabled).toBe(true);
  });

  it("Draft with AI fills the comment and flags the suggested transition without applying it", async () => {
    const a = actions();
    open(a);
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    await waitFor(() => expect(box().value).toBe("Fixed the login redirect."));
    expect(await screen.findByText("Also moves to Done")).not.toBeNull();
    expect(post().textContent).toBe("Post and move to Done");
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.commentOnTicket).length).toBe(0);
  });

  it("surfaces a failed draft and leaves the comment untouched", async () => {
    open(actions({ draftTicketUpdate: mock(async () => ({ ok: false as const, error: "claude timed out" })) }));
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    expect((await screen.findByRole("alert")).textContent).toBe("claude timed out");
    expect(box().value).toBe("");
  });

  async function draft(a: TaskActions) {
    const handlers = open(a);
    fireEvent.click(screen.getByRole("button", { name: "Draft with AI" }));
    await waitFor(() => expect(box().value).toBe("Fixed the login redirect."));
    return handlers;
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
    const { onStatus } = await draft(a);
    fireEvent.click(post());
    await waitFor(() => expect(box().value).toBe(""));
    expect(order).toEqual(["transition", "comment"]);
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "31"]);
    expect(calls(a.commentOnTicket)[0]).toEqual(["ABC-1", "Fixed the login redirect."]);
    expect(onStatus.mock.calls[0][0]).toEqual({ status: "Done", status_category: "done" });
    expect(screen.queryByText("Also moves to Done")).toBeNull();
  });

  it("dropping the suggestion makes Post a comment only", async () => {
    const a = actions();
    await draft(a);
    fireEvent.click(screen.getByRole("button", { name: "Don't change the status" }));
    expect(screen.queryByText("Also moves to Done")).toBeNull();
    expect(post().textContent).toBe("Post comment");
    fireEvent.click(post());
    await waitFor(() => expect(box().value).toBe(""));
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.commentOnTicket).length).toBe(1);
  });

  it("a rejected transition shows Jira's error, keeps the draft, and posts no comment", async () => {
    const a = actions({
      transitionTicket: mock(async () => ({ ok: false as const, error: "Resolution is required" })),
    });
    const { onStatus } = await draft(a);
    fireEvent.click(post());
    expect((await screen.findByRole("alert")).textContent).toBe("Resolution is required");
    expect(box().value).toBe("Fixed the login redirect.");
    expect(calls(a.commentOnTicket).length).toBe(0);
    expect(onStatus.mock.calls.length).toBe(0);
  });
});
