// Comment composer inside the dialog: collapsed field, Draft with AI, Post and move.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import type { TaskActions } from "./TaskCard";
import { actions, calls, open as openDialog, quiet, unfold } from "./taskModalTestKit";

afterEach(cleanup);

const box = () => screen.getByRole("textbox", { name: "Add a comment" }) as HTMLTextAreaElement;
const post = () => screen.getByRole("button", { name: /^Post/ }) as HTMLButtonElement;
const draftBtn = () => screen.getByRole("button", { name: "Draft with AI" });

/** The composer lives in the dialog's Jira fold, which these tests open first. */
function open(...args: Parameters<typeof openDialog>) {
  const handlers = openDialog(...args);
  unfold();
  return handlers;
}

function openComposer(a: TaskActions) {
  const handlers = open(a, quiet());
  fireEvent.focus(box());
  return handlers;
}

describe("collapsed composer", () => {
  it("is one quiet labelled field until it is used", () => {
    open(actions(), quiet());
    expect(box().getAttribute("rows")).toBe("1");
    expect(box().getAttribute("placeholder")).toBe("Add a comment…");
    expect(screen.queryByRole("button", { name: /^Post/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Draft with AI" })).toBeNull();
    expect(screen.queryByText("0 / 5000")).toBeNull();
  });

  it("focus opens Draft with AI and Post; leaving it empty folds it back", () => {
    open(actions(), quiet());
    fireEvent.focus(box());
    expect(draftBtn()).not.toBeNull();
    expect(post().disabled).toBe(true);
    fireEvent.blur(box());
    expect(screen.queryByRole("button", { name: /^Post/ })).toBeNull();
  });

  it("text keeps it open after focus leaves", () => {
    open(actions(), quiet());
    fireEvent.focus(box());
    fireEvent.change(box(), { target: { value: "half" } });
    fireEvent.blur(box());
    expect(post().disabled).toBe(false);
  });

  it("shows the counter only once there is text", () => {
    openComposer(actions());
    expect(screen.queryByText("0 / 5000")).toBeNull();
    fireEvent.change(box(), { target: { value: "hi" } });
    expect(screen.getByText("2 / 5000")).not.toBeNull();
  });

  it("keeping the pointer on the buttons does not pull focus off the field", () => {
    openComposer(actions());
    expect(fireEvent.mouseDown(draftBtn())).toBe(false);
  });
});

describe("TaskComposer", () => {
  it("posts a trimmed comment, clears the box, adds it to the thread and disables Post when empty", async () => {
    const a = actions();
    openComposer(a);
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
    openComposer(a);
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
    openComposer(actions());
    fireEvent.change(box(), { target: { value: "x".repeat(5000) } });
    expect(screen.getByText("5000 / 5000")).not.toBeNull();
    expect(post().disabled).toBe(false);
    fireEvent.change(box(), { target: { value: "x".repeat(5001) } });
    expect(screen.getByText("5001 / 5000")).not.toBeNull();
    expect(post().disabled).toBe(true);
  });

  it("Draft with AI fills the comment and flags the suggested transition without applying it", async () => {
    const a = actions();
    openComposer(a);
    fireEvent.click(draftBtn());
    await waitFor(() => expect(box().value).toBe("Fixed the login redirect."));
    expect(await screen.findByText("Also moves to Done")).not.toBeNull();
    expect(post().textContent).toBe("Post and move to Done");
    expect(calls(a.transitionTicket).length).toBe(0);
    expect(calls(a.commentOnTicket).length).toBe(0);
  });

  it("surfaces a failed draft and leaves the comment untouched", async () => {
    openComposer(actions({ draftTicketUpdate: mock(async () => ({ ok: false as const, error: "claude timed out" })) }));
    fireEvent.click(draftBtn());
    expect((await screen.findByRole("alert")).textContent).toBe("claude timed out");
    expect(box().value).toBe("");
  });

  async function draft(a: TaskActions) {
    const handlers = openComposer(a);
    fireEvent.click(draftBtn());
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
    // the new status shows in the sidebar too
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("Done");
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

  it("announces a posted comment in a task-sr live region inside the dialog", async () => {
    openComposer(actions());
    fireEvent.change(box(), { target: { value: "shipped" } });
    fireEvent.click(post());
    const region = await screen.findByText("Comment posted to ABC-1.");
    expect(region.className).toBe("task-sr");
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(screen.getByRole("dialog").contains(region)).toBe(true);
  });

  it("reads Drafting… while a draft runs and Posting… while posting, both disabled", async () => {
    let finishDraft!: (v: unknown) => void;
    const drafting = new Promise((r) => (finishDraft = r));
    let finishPost!: (v: unknown) => void;
    const posting = new Promise((r) => (finishPost = r));
    openComposer(actions({ draftTicketUpdate: mock(() => drafting), commentOnTicket: mock(() => posting) }));
    fireEvent.click(draftBtn());
    const busy = await screen.findByRole("button", { name: "Drafting…" });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    finishDraft({ ok: false, error: "nope" });
    await screen.findByRole("alert");
    fireEvent.change(box(), { target: { value: "shipped" } });
    fireEvent.click(post());
    const busyPost = await screen.findByRole("button", { name: "Posting…" });
    expect((busyPost as HTMLButtonElement).disabled).toBe(true);
    finishPost({ ok: true, data: { ok: true } });
    await waitFor(() => expect(box().value).toBe(""));
  });

  it("Draft appends after a blank line to typed text", async () => {
    openComposer(actions());
    fireEvent.change(box(), { target: { value: "my own note" } });
    fireEvent.click(draftBtn());
    await waitFor(() => expect(box().value).toBe("my own note\n\nFixed the login redirect."));
  });
});
