// Activity: the Work log / Comments tablist, which tab opens first, the quick actions and the comment thread.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { block, day, payload } from "./workLogTestKit";
import { actions, detail, open, quiet, row } from "./taskModalTestKit";

afterEach(cleanup);

const tab = (name: RegExp) => screen.getByRole("tab", { name });
const selected = () => screen.getAllByRole("tab").find((t) => t.getAttribute("aria-selected") === "true")?.textContent ?? "";
const withDays = () =>
  actions({ loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload([day(), day({ day: "2026-09-30", line_seconds: 1800, blocks: [block({ id: 2 })] })]) })) });

describe("tablist", () => {
  it("has two tabs with a roving tabindex, each controlling a labelled panel", async () => {
    open(actions());
    expect(screen.getByRole("tablist", { name: "Activity" })).not.toBeNull();
    const [work, comments] = screen.getAllByRole("tab");
    expect(work.textContent).toMatch(/^Work log/);
    expect(comments.textContent).toMatch(/^Comments/);
    expect(work.getAttribute("tabindex")).toBe("0");
    expect(comments.getAttribute("tabindex")).toBe("-1");
    const panel = screen.getByRole("tabpanel");
    expect(panel.getAttribute("aria-labelledby")).toBe(work.id);
    expect(work.getAttribute("aria-controls")).toBe(panel.id);
    await screen.findByText(/Steps to reproduce/);
  });

  it("shows the work total and the comment count as chips once loaded", async () => {
    open(withDays());
    await waitFor(() => expect(tab(/Work log/).textContent).toBe("Work log2h"));
    await waitFor(() => expect(tab(/Comments/).textContent).toBe("Comments2"));
    expect(tab(/Work log/).querySelector(".task-tab-count")?.textContent).toBe("2h");
  });

  it("clicking a tab switches the panel", async () => {
    open(actions());
    fireEvent.click(tab(/Comments/));
    expect(selected()).toMatch(/^Comments/);
    expect(screen.getByRole("textbox", { name: "Add a comment" })).not.toBeNull();
    expect(screen.queryByText("No work logged on ABC-1 in the last 14 days.")).toBeNull();
    fireEvent.click(tab(/Work log/));
    expect(await screen.findByText("No work logged on ABC-1 in the last 14 days.")).not.toBeNull();
  });

  it("Arrow keys, Home and End move between the tabs and focus follows", async () => {
    open(actions());
    const [work, comments] = screen.getAllByRole("tab");
    fireEvent.keyDown(work, { key: "ArrowRight" });
    expect(selected()).toMatch(/^Comments/);
    expect(document.activeElement).toBe(comments);
    expect(comments.getAttribute("tabindex")).toBe("0");
    fireEvent.keyDown(comments, { key: "ArrowRight" }); // wraps
    expect(selected()).toMatch(/^Work log/);
    expect(document.activeElement).toBe(work);
    fireEvent.keyDown(work, { key: "ArrowLeft" }); // wraps back
    expect(selected()).toMatch(/^Comments/);
    fireEvent.keyDown(comments, { key: "Home" });
    expect(selected()).toMatch(/^Work log/);
    fireEvent.keyDown(work, { key: "End" });
    expect(selected()).toMatch(/^Comments/);
    expect(fireEvent.keyDown(comments, { key: "x" })).toBe(true); // other keys are left alone
    await screen.findByText(/Steps to reproduce/);
  });
});

describe("which tab opens first", () => {
  it("Work log when there are hours this week", async () => {
    open(actions(), row({ week_seconds: 1800 }));
    expect(selected()).toMatch(/^Work log/);
    await screen.findByText(/Steps to reproduce/);
  });

  it("Comments when there is no work at all", async () => {
    open(actions(), quiet());
    expect(selected()).toMatch(/^Comments/);
    await screen.findByText(/Steps to reproduce/);
  });

  it("Work log when no hours this week but a block exists in the last 14 days", async () => {
    open(withDays(), quiet());
    expect(selected()).toMatch(/^Comments/); // until the days arrive
    await waitFor(() => expect(selected()).toMatch(/^Work log/));
  });

  it("an explicit choice is remembered for the next ticket, and wins over the rule", async () => {
    open(actions(), row({ week_seconds: 1800 }));
    fireEvent.click(tab(/Comments/));
    expect(window.localStorage.getItem("worklog.ticketTab")).toBe("comments");
    cleanup();
    open(actions(), row({ week_seconds: 1800 }));
    expect(selected()).toMatch(/^Comments/);
    await screen.findByText(/Steps to reproduce/);
  });

  it("quick actions switch tabs but are not remembered", async () => {
    open(actions(), row({ week_seconds: 1800 }));
    fireEvent.click(screen.getByRole("button", { name: "Comment" }));
    expect(window.localStorage.getItem("worklog.ticketTab")).toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });

  it("works with storage that throws", async () => {
    const real = window.localStorage;
    const broken = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("full"); } };
    Object.defineProperty(window, "localStorage", { configurable: true, value: broken });
    try {
      open(actions(), row({ week_seconds: 1800 }));
      expect(selected()).toMatch(/^Work log/);
      fireEvent.click(tab(/Comments/));
      expect(selected()).toMatch(/^Comments/);
    } finally {
      Object.defineProperty(window, "localStorage", { configurable: true, value: real });
    }
    await screen.findByText(/Steps to reproduce/);
  });
});

describe("quick actions", () => {
  it("Log time switches to the Work log and opens the form", async () => {
    open(actions(), quiet());
    expect(selected()).toMatch(/^Comments/);
    fireEvent.click(screen.getAllByRole("button", { name: "Log time" })[0]);
    expect(selected()).toMatch(/^Work log/);
    expect(screen.getByLabelText("What you did")).not.toBeNull();
    expect(document.activeElement).toBe(screen.getByLabelText("Day"));
    await screen.findByText(/Steps to reproduce/);
  });

  it("Log time on the Work log tab opens the form too, and again after it was closed", async () => {
    open(actions());
    const quick = () => screen.getAllByRole("button", { name: "Log time" })[0];
    fireEvent.click(quick());
    expect(screen.getByLabelText("What you did")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByLabelText("What you did")).toBeNull();
    fireEvent.click(quick());
    expect(screen.getByLabelText("What you did")).not.toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });

  it("Comment switches to Comments and focuses the composer", async () => {
    open(actions());
    expect(selected()).toMatch(/^Work log/);
    fireEvent.click(screen.getByRole("button", { name: "Comment" }));
    expect(selected()).toMatch(/^Comments/);
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Add a comment" }));
    await screen.findByText(/Steps to reproduce/);
  });

  it("are the only two buttons in the quick row", () => {
    open(actions());
    const names = [...document.querySelectorAll(".task-quick button")].map((b) => b.textContent);
    expect(names).toEqual(["Log time", "Comment"]);
  });
});

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
    expect(tab(/Comments/).textContent).toBe("Comments3");
  });

  it("says when there are none yet", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ comments: [] }) })) }), quiet());
    expect(await screen.findByText("No comments yet.")).not.toBeNull();
  });

  it("shows a skeleton while Jira loads and keeps the composer usable if it fails", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: false as const, error: "jira down" })) }), quiet());
    expect(document.querySelector(".task-tabpanel .task-skel")).toBeTruthy();
    await screen.findByText(/Couldn't load ABC-1/);
    expect(document.querySelector(".task-tabpanel .task-skel")).toBeNull();
    expect(screen.getByRole("textbox", { name: "Add a comment" })).not.toBeNull();
  });
});
