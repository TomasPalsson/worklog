// "Work logged" section: editing a day's hours and Tempo text, Esc, and where focus lands.

import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { btn, btns, calls, day, hoursBtn, open, settle, textBtn } from "./workLogTestKit";

afterEach(cleanup);

describe("Esc in the text editor", () => {
  it("cancels only that edit and returns focus", async () => {
    const { a } = await open();
    fireEvent.click(textBtn());
    const box = screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "half typed" } });
    expect(fireEvent.keyDown(box, { key: "Escape" })).toBe(false);
    expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(textBtn());
    expect(screen.getByText("Worked on login")).toBeTruthy();
    expect(calls(a.saveTempoLineText)).toHaveLength(0);
  });
});

describe("focus returns to the opener", () => {
  it("hours editor: Save and Cancel return focus to the hours button", async () => {
    await open();
    fireEvent.click(hoursBtn());
    fireEvent.change(screen.getByLabelText("Hours for Thu 1 Oct"), { target: { value: "2" } });
    fireEvent.click(btn("Save hours"));
    await settle();
    expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(hoursBtn());

    fireEvent.click(hoursBtn());
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(hoursBtn());
  });

  it("text editor: Save and Cancel return focus to Edit Tempo text", async () => {
    await open();
    fireEvent.click(textBtn());
    fireEvent.change(screen.getByLabelText("Line text for Thu 1 Oct"), { target: { value: "New" } });
    fireEvent.click(btn("Save text"));
    await settle();
    expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(textBtn());

    fireEvent.click(textBtn());
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(textBtn());
  });
});

describe("text editing layout", () => {
  it("Edit Tempo text sits right after the line text and the text hides while editing", async () => {
    await open();
    const text = document.querySelector(".task-day-text") as HTMLElement;
    expect(text.nextElementSibling).toBe(textBtn());
    fireEvent.click(textBtn());
    expect(document.querySelector(".task-day-text")).toBeNull();
    expect((screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement).value).toBe("Worked on login");
  });
});

describe("Use tracked time", () => {
  const byHand = () => day({ hours_set_by_hand: true, line_seconds: 7200, tracked_seconds: 6120 });

  it("clears the override with null and refetches", async () => {
    const { a } = await open([byHand()]);
    fireEvent.click(btn("2h — edit hours for Thu 1 Oct"));
    fireEvent.click(btn("Use tracked time (1h 30m)")); // 1h 42m rounds to 1h 30m
    await settle();
    expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull();
    expect(calls(a.saveTempoLineHours)[0]).toEqual([{ day: "2026-10-01", jira_issue: "ABC-1" }, null]);
    await settle();
    expect(calls(a.loadTicketBlocks)).toHaveLength(2);
  });

  it("is absent when the hours are not set by hand", async () => {
    await open();
    fireEvent.click(hoursBtn());
    expect(btns(/Use tracked time/)).toHaveLength(0);
  });
});
