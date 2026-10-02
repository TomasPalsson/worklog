// "Work logged" section: the Log time form (open, validate, Esc/Cancel/discard, focus, confirmation, empty state).

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { block, btn, btns, calls, day, logTime, open, payload, settle } from "./workLogTestKit";

afterEach(cleanup);

const desc = () => screen.getByLabelText("What you did") as HTMLTextAreaElement;

describe("log form", () => {
  it("Esc closes the form (prevented) and returns focus to Log time", async () => {
    await open();
    fireEvent.click(logTime());
    // fireEvent returns false when a handler called preventDefault.
    expect(fireEvent.keyDown(desc(), { key: "Escape" })).toBe(false);
    await settle();
    expect(screen.queryByLabelText("What you did")).toBeNull();
    expect(document.activeElement).toBe(logTime());
  });

  it("validates length and description on blur", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.blur(desc());
    expect(screen.getByText("Say what you did.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Length"), { target: { value: "721" } });
    fireEvent.blur(screen.getByLabelText("Length"));
    expect(screen.getByText("Length must be 1 to 720 minutes.")).toBeTruthy();
    fireEvent.change(desc(), { target: { value: "x" } });
    expect(screen.queryByText("Say what you did.")).toBeNull();
  });

  it("shows the chosen day in words", async () => {
    await open();
    fireEvent.click(logTime());
    expect(screen.getByText("Fri 2 Oct")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Day"), { target: { value: "2026-10-01" } });
    expect(screen.getAllByText("Thu 1 Oct").length).toBeGreaterThan(0);
  });

  it("opening the log form focuses Day", async () => {
    await open();
    fireEvent.click(logTime());
    expect(document.activeElement).toBe(screen.getByLabelText("Day"));
  });

  it("opening the log form scrolls the section head into view", async () => {
    await open();
    const scroll = mock(() => {});
    (document.querySelector(".task-work-head") as HTMLElement).scrollIntoView = scroll;
    fireEvent.click(logTime());
    expect(scroll).toHaveBeenCalledWith({ block: "nearest" });
  });

  it("the chip matching the length is pressed", async () => {
    await open();
    fireEvent.click(logTime());
    expect(btn("1h").getAttribute("aria-pressed")).toBe("true");
    expect(btn("30m").getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(btn("30m"));
    expect(btn("30m").getAttribute("aria-pressed")).toBe("true");
    expect(btn("1h").getAttribute("aria-pressed")).toBe("false");
    fireEvent.change(screen.getByLabelText("Length"), { target: { value: "45" } });
    expect(document.querySelectorAll('.task-log-chips [aria-pressed="true"]')).toHaveLength(0);
  });
});

describe("Esc and Cancel never destroy a draft", () => {
  it("Esc outside the form (e.g. the composer) leaves the log form open", async () => {
    await open();
    fireEvent.click(logTime());
    // Not prevented: nothing in this form claimed it.
    expect(fireEvent.keyDown(document.body, { key: "Escape" })).toBe(true);
    expect(desc()).toBeTruthy();
  });

  it("Esc with a typed description asks before discarding", async () => {
    await open();
    fireEvent.click(logTime());
    const box = desc();
    fireEvent.change(box, { target: { value: "important notes" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.getByText(/Discard this entry\?/)).toBeTruthy();
    expect(desc()).toBeTruthy();
    fireEvent.click(btn("Keep editing"));
    expect(screen.queryByText(/Discard this entry\?/)).toBeNull();
    expect(desc().value).toBe("important notes");
    fireEvent.keyDown(box, { key: "Escape" });
    fireEvent.click(btn("Discard"));
    expect(screen.queryByLabelText("What you did")).toBeNull();
  });

  it("Cancel with a typed description asks first, like Esc; Keep editing stays", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.change(desc(), { target: { value: "work" } });
    fireEvent.click(btn("Cancel"));
    expect(screen.getByText(/Discard this entry\?/)).toBeTruthy();
    fireEvent.click(btn("Keep editing"));
    expect(screen.queryByText(/Discard this entry\?/)).toBeNull();
    expect(desc().value).toBe("work");
  });
});

describe("focus never drops to body", () => {
  it("Cancel, Esc and Log all return focus to Log time", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(logTime());

    fireEvent.click(logTime());
    fireEvent.keyDown(desc(), { key: "Escape" });
    expect(document.activeElement).toBe(logTime());

    fireEvent.click(logTime());
    fireEvent.change(desc(), { target: { value: "work" } });
    fireEvent.click(btn("Log 1h"));
    await settle();
    expect(btns("Log time")).toHaveLength(1); // form closed, Log time back
    expect(document.activeElement).toBe(logTime());
  });

  it("Discard returns focus to Log time", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.change(desc(), { target: { value: "work" } });
    fireEvent.click(btn("Cancel"));
    fireEvent.click(btn("Discard"));
    expect(document.activeElement).toBe(logTime());
  });
});

describe("log confirmation", () => {
  it("highlights the new row, scrolls it into view and keeps a Logged strip", async () => {
    const scroll = mock(() => {});
    (HTMLElement.prototype as unknown as { scrollIntoView: unknown }).scrollIntoView = scroll;
    const logged = block({ id: 9, description: "New entry" });
    const loadTicketBlocks = mock()
      .mockResolvedValueOnce({ ok: true, data: payload([day()]) })
      .mockResolvedValue({ ok: true, data: payload([day({ blocks: [block(), logged] })]) });
    const { a } = await open([day()], { loadTicketBlocks });
    fireEvent.click(logTime());
    fireEvent.change(desc(), { target: { value: "New entry" } });
    fireEvent.click(btn("Log 1h"));
    await settle();
    expect(document.querySelector("[data-fresh]")).toBeTruthy();
    expect(document.querySelector("[data-fresh]")?.textContent).toContain("New entry");
    expect(scroll).toHaveBeenCalledWith({ block: "nearest" });
    expect(screen.getByText("Logged 1h").className).toBe("task-day-sent");
    expect(calls(a.logTicketTime)).toHaveLength(1);
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
  });
});

describe("empty state", () => {
  it("offers a Log time button that opens the form", async () => {
    await open([]);
    expect(screen.getByText("Nothing tracked on ABC-1 yet. Time from your sessions lands here on its own — or log it by hand.")).toBeTruthy();
    const buttons = btns("Log time");
    expect(buttons).toHaveLength(1); // only the empty row's; the head one is hidden
    fireEvent.click(buttons[0]);
    expect(desc()).toBeTruthy();
    expect(btns("Log time")).toHaveLength(0);
  });
});
