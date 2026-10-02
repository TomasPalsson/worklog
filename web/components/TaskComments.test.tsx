// Comments inside the dialog's Jira fold: newest first, local posts, empty and loading states.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { actions, detail, open, quiet, unfold } from "./taskModalTestKit";

afterEach(cleanup);

describe("comments", () => {
  it("lists the newest first with an avatar, the author and a relative time", async () => {
    const day2 = new Date(Date.now() - 2 * 86_400_000).toISOString();
    const hour = new Date(Date.now() - 3 * 3_600_000).toISOString();
    open(
      actions({
        loadTicketDetail: mock(async () => ({
          ok: true as const,
          data: detail({
            comments: [
              { id: "1", author: "Grace Hopper", created: day2, body: "first one" },
              { id: "2", author: "Linus", created: hour, body: "second one" },
            ],
          }),
        })),
      }),
      quiet(),
    );
    unfold();
    await screen.findByText("first one");
    const items = [...document.querySelectorAll(".task-comments li")];
    expect(items.map((li) => li.querySelector("p")?.textContent)).toEqual(["second one", "first one"]);
    expect(items[0].querySelector(".task-avatar")?.textContent).toBe("L");
    expect(items[1].querySelector(".task-avatar")?.textContent).toBe("GH");
    expect(items[1].querySelector("strong")?.textContent).toBe("Grace Hopper");
    expect(items[0].querySelector("time")?.textContent).toBe("3 hours ago");
    expect(items[1].querySelector("time")?.textContent).toBe("2 days ago");
    expect(items[1].querySelector("time")?.getAttribute("title")).toMatch(/^\d{1,2} \w{3}, \d\d:\d\d$/);
  });

  it("shows the composer first, then the thread, and a comment you post goes to the top", async () => {
    open(actions(), quiet());
    unfold();
    await screen.findByText("first one");
    const form = document.querySelector(".task-composer") as HTMLElement;
    expect(form.nextElementSibling?.classList.contains("task-comments")).toBe(true);
    const box = screen.getByRole("textbox", { name: "Add a comment" });
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "shipped" } });
    fireEvent.click(screen.getByRole("button", { name: /^Post/ }));
    await screen.findByText("You");
    const first = document.querySelector(".task-comments li p") as HTMLElement;
    expect(first.textContent).toBe("shipped");
    expect(document.querySelector(".task-comments li .task-avatar")?.textContent).toBe("Y");
    expect(document.querySelectorAll(".task-comments li").length).toBe(3);
  });

  it("says when there are none yet", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ comments: [] }) })) }), quiet());
    unfold();
    expect(await screen.findByText("No comments yet.")).not.toBeNull();
  });

  it("shows a skeleton while Jira loads and keeps the composer usable if it fails", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: false as const, error: "jira down" })) }), quiet());
    unfold();
    expect(document.querySelector(".task-comments-section .task-skel")).toBeTruthy();
    await screen.findByText(/Couldn't load ABC-1/);
    expect(document.querySelector(".task-comments-section .task-skel")).toBeNull();
    expect(screen.getByRole("textbox", { name: "Add a comment" })).not.toBeNull();
  });
});
