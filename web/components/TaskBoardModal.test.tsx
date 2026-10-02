// My Tasks board + ticket dialog: the board stays put, goes inert, and `?ticket=` follows the open ticket.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TaskRow } from "@/lib/types";
import { TaskBoard } from "./TaskBoard";
import { actions, row } from "./taskModalTestKit";

// happy-dom starts on about:blank, which has no path or query to write to.
beforeEach(() => (window as unknown as { happyDOM: { setURL(url: string): void } }).happyDOM.setURL("http://localhost/tasks"));
afterEach(cleanup);

const tasks: TaskRow[] = [row({}), row({ key: "ABC-2", summary: "Spike cache" })];
const cardBtn = (key: string) => document.querySelector(`[data-task-key="${key}"] .task-card-btn`) as HTMLElement;
const search = () => window.location.search;
const popTo = (url: string) => {
  window.history.replaceState(null, "", url);
  act(() => void window.dispatchEvent(new PopStateEvent("popstate")));
};

describe("board behind the dialog", () => {
  it("does not shift: no two-pane layout class or rule is left", () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    expect(document.querySelector(".has-panel")).toBeNull();
    const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
    expect(css).not.toContain("has-panel");
    expect(css).not.toContain(".task-panel");
  });

  it("opening a card renders the dialog; the board is inert behind it and live again after", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    const root = document.querySelector(".task-board-root") as HTMLElement;
    expect(root.hasAttribute("inert")).toBe(false);
    fireEvent.click(cardBtn("ABC-1"));
    expect(screen.getByRole("dialog")).not.toBeNull();
    expect(root.hasAttribute("inert")).toBe(true);
    expect(root.contains(screen.getByRole("dialog"))).toBe(false);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(root.hasAttribute("inert")).toBe(false);
    await act(async () => {});
  });

  it("closing returns focus to the card that opened it, from Esc, the close button and the backdrop", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-2"));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(document.activeElement).toBe(cardBtn("ABC-2"));
    fireEvent.click(cardBtn("ABC-1"));
    fireEvent.click(screen.getByRole("button", { name: "Close ABC-1" }));
    expect(document.activeElement).toBe(cardBtn("ABC-1"));
    fireEvent.click(cardBtn("ABC-1"));
    const backdrop = screen.getByRole("dialog").parentElement as HTMLElement;
    fireEvent.mouseDown(backdrop);
    fireEvent.click(backdrop);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(cardBtn("ABC-1"));
    await act(async () => {});
  });
});

describe("?ticket= in the address", () => {
  it("opening a card pushes ?ticket=KEY without leaving the page", async () => {
    const before = window.history.length;
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(search()).toBe("");
    fireEvent.click(cardBtn("ABC-1"));
    expect(search()).toBe("?ticket=ABC-1");
    expect(window.history.length).toBe(before + 1);
    await act(async () => {});
  });

  it("switching to another card replaces the entry instead of stacking another", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    const length = window.history.length;
    fireEvent.click(cardBtn("ABC-2"));
    expect(search()).toBe("?ticket=ABC-2");
    expect(window.history.length).toBe(length);
    await act(async () => {});
  });

  it("closing takes the param out again", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(search()).toBe(""));
  });

  it("keeps other query parameters", async () => {
    window.history.replaceState(null, "", "/tasks?q=1");
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    expect(search()).toBe("?q=1&ticket=ABC-1");
    await act(async () => {});
  });

  it("popstate (Back) closes the dialog and Forward opens it again", async () => {
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    fireEvent.click(cardBtn("ABC-1"));
    expect(screen.getByRole("dialog")).not.toBeNull();
    popTo("/tasks");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(cardBtn("ABC-1"));
    popTo("/tasks?ticket=ABC-1");
    expect(screen.getByRole("dialog")).not.toBeNull();
    await act(async () => {});
  });

  it("loading the page with ?ticket=KEY opens that ticket", async () => {
    window.history.replaceState(null, "", "/tasks?ticket=ABC-2");
    const a = actions();
    render(<TaskBoard actions={a} tasks={tasks} />);
    expect(screen.getByRole("heading", { name: "Spike cache" })).not.toBeNull();
    expect(screen.getByRole("dialog")).not.toBeNull();
    await act(async () => {});
    expect((a.loadTicketDetail as ReturnType<typeof mock>).mock.calls[0]).toEqual(["ABC-2"]);
  });

  it("ignores a ticket that is not on the board", () => {
    window.history.replaceState(null, "", "/tasks?ticket=NOPE-9");
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closing a ticket that was opened from the address only strips the param", async () => {
    window.history.replaceState(null, "", "/tasks?ticket=ABC-1");
    render(<TaskBoard actions={actions()} tasks={tasks} />);
    const length = window.history.length;
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(search()).toBe("");
    expect(window.history.length).toBe(length);
    await act(async () => {});
  });
});
