// Work log tab: editing a day's hours and Tempo text from the "⋯" menu, Esc, and where focus lands.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { btn, btns, calls, day, more, open, pick, settle } from "./workLogTestKit";

afterEach(cleanup);

const items = () => [...document.querySelectorAll('[role="menuitem"]')].map((n) => n.textContent);

describe("the day menu", () => {
  it("lists Edit hours and Edit Tempo text, and Use tracked time only for hand-set hours", async () => {
    await open();
    fireEvent.click(more());
    expect(more().getAttribute("aria-haspopup")).toBe("menu");
    expect(more().getAttribute("aria-expanded")).toBe("true");
    expect(items()).toEqual(["Edit hours", "Edit Tempo text"]);
    cleanup();
    await open([day({ hours_set_by_hand: true, line_seconds: 7200, tracked_seconds: 6120 })]);
    fireEvent.click(more());
    expect(items()).toEqual(["Edit hours", "Use tracked time", "Edit Tempo text"]);
  });

  it("focuses the first item, moves with the arrow keys, and Esc closes only the menu and refocuses the button", async () => {
    await open();
    fireEvent.click(more());
    const [first, last] = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "ArrowUp" });
    expect(document.activeElement).toBe(last);
    // fireEvent returns false when a handler called preventDefault, which is what keeps the dialog open.
    expect(fireEvent.keyDown(last, { key: "Escape" })).toBe(false);
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(more());
  });

  it("closes on an outside click", async () => {
    await open();
    fireEvent.click(more());
    fireEvent.mouseDown(document.body);
    expect(document.querySelector('[role="menu"]')).toBeNull();
  });
});

describe("Esc in the text editor", () => {
  it("cancels only that edit and returns focus", async () => {
    const { a } = await open();
    pick("Edit Tempo text");
    const box = screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "half typed" } });
    expect(fireEvent.keyDown(box, { key: "Escape" })).toBe(false);
    expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(more());
    expect(screen.getByText("Worked on login")).toBeTruthy();
    expect(calls(a.saveTempoLineText)).toHaveLength(0);
  });
});

describe("focus returns to the day's menu button", () => {
  it("hours editor: Save and Cancel", async () => {
    await open();
    pick("Edit hours");
    fireEvent.change(screen.getByLabelText("Hours for Thu 1 Oct"), { target: { value: "2" } });
    fireEvent.click(btn("Save hours"));
    await settle();
    expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(more());

    pick("Edit hours");
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(more());
  });

  it("hours editor: Esc cancels (prevented) and refocuses", async () => {
    await open();
    pick("Edit hours");
    expect(fireEvent.keyDown(screen.getByLabelText("Hours for Thu 1 Oct"), { key: "Escape" })).toBe(false);
    expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(more());
  });

  it("text editor: Save and Cancel", async () => {
    await open();
    pick("Edit Tempo text");
    fireEvent.change(screen.getByLabelText("Line text for Thu 1 Oct"), { target: { value: "New" } });
    fireEvent.click(btn("Save text"));
    await settle();
    expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(more());

    pick("Edit Tempo text");
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(more());
  });
});

describe("text editing layout", () => {
  it("the editor opens under the day row, the line text hides while editing and the editor starts from it", async () => {
    await open();
    expect(document.querySelector(".task-day-text")).toBeTruthy();
    pick("Edit Tempo text");
    expect(document.querySelector(".task-day-text")).toBeNull();
    const row = document.querySelector(".task-day-row") as HTMLElement;
    expect(row.nextElementSibling?.classList.contains("task-day-text-edit")).toBe(true);
    expect((screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement).value).toBe("Worked on login");
  });

  it("editing works on a collapsed day too", async () => {
    await open([day(), day({ day: "2026-09-30", blocks: [] })]);
    expect(document.querySelectorAll(".task-day-body")).toHaveLength(1); // only the newest is open
    pick("Edit Tempo text", "Wed 30 Sep");
    expect(screen.getByLabelText("Line text for Wed 30 Sep")).toBeTruthy();
  });
});

describe("Use tracked time", () => {
  const byHand = () => day({ hours_set_by_hand: true, line_seconds: 7200, tracked_seconds: 6120 });

  it("clears the override with null and refetches", async () => {
    const { a } = await open([byHand()]);
    pick("Use tracked time");
    await settle();
    expect(calls(a.saveTempoLineHours)[0]).toEqual([{ day: "2026-10-01", jira_issue: "ABC-1" }, null]);
    await settle();
    expect(calls(a.loadTicketBlocks)).toHaveLength(2);
  });

  it("shows Jira's refusal inline", async () => {
    await open([byHand()], { saveTempoLineHours: mock(async () => ({ ok: false, error: "nope" })) });
    pick("Use tracked time");
    await settle();
    expect(screen.getByRole("alert").textContent).toBe("nope");
  });

  it("is absent when the hours are not set by hand", async () => {
    await open();
    fireEvent.click(more());
    expect(btns(/Use tracked time/)).toHaveLength(0);
  });
});
