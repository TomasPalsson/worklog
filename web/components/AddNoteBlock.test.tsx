// "+" form on the day page: validation, defaults, submit, job hand-off.
// Mocks only the server-action boundary and the router.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { JiraTicket, RawBlock } from "@/lib/types";
import { subscribe } from "@/lib/toast";

mock.module("next/navigation", () => ({ useRouter: () => ({ refresh: () => {} }), usePathname: () => "/" }));

type Add = { ok: true; data: RawBlock } | { ok: false; error: string };
const added = { ok: true, data: { id: 42 } } as Add;
const addNoteBlock = mock(async (_body: unknown): Promise<Add> => added);
const noteStatusAction = mock(async (_id: number) => ({ ok: true as const, data: { state: "running" as const } }));
mock.module("@/app/actions-note-block", () => ({ addNoteBlock, noteStatusAction }));
const { AddNoteBlock } = await import("./AddNoteBlock");

const tickets: JiraTicket[] = [
  { key: "ABC-1", summary: "Login", status: null, updated: null },
  { key: "XY_2-30", summary: null, status: null, updated: null },
];
const DAY = "2026-10-08";

beforeEach(() => {
  addNoteBlock.mockClear();
  noteStatusAction.mockClear();
});
afterEach(cleanup);

const mount = (lastEnd: string | null = null) =>
  render(<AddNoteBlock day={DAY} tickets={tickets} lastEnd={lastEnd} />);
const open = () => fireEvent.click(screen.getByRole("button", { name: "Add note block" }));
const field = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const set = (label: string, value: string) => fireEvent.change(field(label), { target: { value } });
const submit = () => void fireEvent.submit(document.querySelector("form") as HTMLFormElement);
async function fill(over: Partial<Record<"Start" | "Minutes" | "Ticket" | "Note", string>> = {}) {
  const v = { Start: "10:00", Minutes: "30", Ticket: "ABC-1", Note: "fixed login bug", ...over };
  for (const [label, value] of Object.entries(v)) set(label, value);
  await act(async () => submit());
}

describe("AddNoteBlock", () => {
  it("shows + but no form until it is clicked", () => {
    mount();
    expect(screen.getByRole("button", { name: "Add note block" })).toBeTruthy();
    expect(document.querySelector("form")).toBeNull();
    open();
    expect(document.querySelector("form")).not.toBeNull();
  });

  it("defaults start to 09:00 with no blocks", () => {
    mount(null);
    open();
    expect(field("Start").value).toBe("09:00");
  });

  it("defaults start to the end of the day's last block", () => {
    mount(new Date(2026, 9, 8, 14, 5).toISOString());
    open();
    expect(field("Start").value).toBe("14:05");
  });

  it("offers every cached ticket in the ticket datalist", () => {
    mount();
    open();
    const list = document.getElementById(field("Ticket").getAttribute("list") as string);
    expect([...(list?.querySelectorAll("option") ?? [])].map((o) => o.getAttribute("value"))).toEqual([
      "ABC-1",
      "XY_2-30",
    ]);
  });

  it("submits the trimmed values for the viewed day", async () => {
    mount();
    open();
    await fill({ Note: "  fixed login bug  ", Ticket: " ABC-1 " });
    expect(addNoteBlock).toHaveBeenCalledTimes(1);
    expect(addNoteBlock.mock.calls[0][0]).toEqual({
      jira_issue: "ABC-1",
      day: DAY,
      start: "10:00",
      minutes: 30,
      note: "fixed login bug",
    });
  });

  it("closes on success and shows Writing… while the job runs", async () => {
    mount();
    open();
    await fill();
    expect(document.querySelector("form")).toBeNull();
    expect(screen.getByText("Writing…")).toBeTruthy();
  });

  const rejected: [string, Parameters<typeof fill>[0], string][] = [
    ["lowercase key", { Ticket: "abc-1" }, "Invalid ticket key"],
    ["key without dash", { Ticket: "ABC1" }, "Invalid ticket key"],
    ["empty key", { Ticket: "" }, "Invalid ticket key"],
    ["key with trailing text (unanchored regex)", { Ticket: "ABC-1 x" }, "Invalid ticket key"],
    ["blank note", { Note: "   " }, "Write a note"],
    ["note of 501 chars (> vs >=)", { Note: "x".repeat(501) }, "Keep the note to 500 characters."],
    ["0 minutes", { Minutes: "0" }, "Minutes must be 1 to 720."],
    ["721 minutes (> vs >=)", { Minutes: "721" }, "Minutes must be 1 to 720."],
    ["fractional minutes", { Minutes: "1.5" }, "Minutes must be 1 to 720."],
    ["empty minutes (Number('') is 0)", { Minutes: "" }, "Minutes must be 1 to 720."],
    ["no start", { Start: "" }, "Pick a start time."],
  ];
  it.each(rejected)("refuses %s with a message and no call", async (_n, over, message) => {
    mount();
    open();
    await fill(over);
    expect(screen.getByText(message)).toBeTruthy();
    expect(addNoteBlock).not.toHaveBeenCalled();
    expect(document.querySelector("form")).not.toBeNull();
  });

  const accepted: [string, Parameters<typeof fill>[0]][] = [
    ["1 minute", { Minutes: "1" }],
    ["720 minutes", { Minutes: "720" }],
    ["a 500-char note", { Note: "x".repeat(500) }],
    ["a 1-char note", { Note: "x" }],
    ["underscore and digits in the key", { Ticket: "XY_2-30" }],
  ];
  it.each(accepted)("accepts %s", async (_n, over) => {
    mount();
    open();
    await fill(over);
    expect(addNoteBlock).toHaveBeenCalledTimes(1);
  });

  it("keeps the form and typed values and shows the daemon's error", async () => {
    addNoteBlock.mockImplementationOnce(async () => ({ ok: false, error: "day is in the future" }));
    mount();
    open();
    await fill();
    expect(screen.getByText("day is in the future")).toBeTruthy();
    expect(field("Note").value).toBe("fixed login bug");
    expect(field("Ticket").value).toBe("ABC-1");
    expect(screen.queryByText("Writing…")).toBeNull();
  });

  it("is usable again after a daemon error (busy is cleared)", async () => {
    addNoteBlock.mockImplementationOnce(async () => ({ ok: false, error: "daemon down" }));
    mount();
    open();
    await fill();
    expect(field("Note").disabled).toBe(false);
    expect((screen.getByRole("button", { name: "Add" }) as HTMLButtonElement).disabled).toBe(false);
  });
});

describe("AddNoteBlock form chrome", () => {
  it("+ reports open/closed (wrong version: aria-expanded missing or stuck)", () => {
    mount();
    const plus = screen.getByRole("button", { name: "Add note block" });
    expect(plus.getAttribute("aria-expanded")).toBe("false");
    open();
    expect(plus.getAttribute("aria-expanded")).toBe("true");
  });

  it("Cancel closes the form without saving", () => {
    mount();
    open();
    set("Note", "half typed");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(document.querySelector("form")).toBeNull();
    expect(addNoteBlock).not.toHaveBeenCalled();
  });

  it("Escape closes the form without saving", () => {
    mount();
    open();
    fireEvent.keyDown(field("Ticket"), { key: "Escape" });
    expect(document.querySelector("form")).toBeNull();
    expect(addNoteBlock).not.toHaveBeenCalled();
  });

  it("marks the field that is wrong and links it to the message (wrong version: message only in the footer)", async () => {
    mount();
    open();
    await fill({ Ticket: "abc-1" });
    const alert = screen.getByRole("alert");
    expect(field("Ticket").getAttribute("aria-invalid")).toBe("true");
    expect(field("Ticket").getAttribute("aria-describedby")).toBe(alert.id);
    expect(field("Note").getAttribute("aria-invalid")).toBeNull();
    // fixing it clears the mark on the next try
    await fill({ Ticket: "ABC-1", Minutes: "0" });
    expect(field("Ticket").getAttribute("aria-invalid")).toBeNull();
    expect(field("Minutes").getAttribute("aria-invalid")).toBe("true");
  });

  it("moves the cursor to the field that is wrong (wrong version: focus stays on Add)", async () => {
    mount();
    open();
    await fill({ Minutes: "0" });
    expect(document.activeElement).toBe(field("Minutes"));
  });

  it("Escape keeps the form when a note or ticket is typed (wrong version: silent loss)", () => {
    mount();
    open();
    set("Note", "half typed");
    fireEvent.keyDown(field("Note"), { key: "Escape" });
    expect(document.querySelector("form")).not.toBeNull();
    expect(field("Note").value).toBe("half typed");
  });

  it("the cursor starts in Ticket — start and minutes come prefilled", () => {
    mount();
    open();
    expect(document.activeElement).toBe(field("Ticket"));
  });
});

describe("AddNoteBlock feedback", () => {
  it("clears the error as soon as the marked field is edited, not other fields", async () => {
    mount();
    open();
    await fill({ Ticket: "abc-1" });
    set("Note", "other edit");
    expect(field("Ticket").getAttribute("aria-invalid")).toBe("true");
    expect(screen.queryByRole("alert")).not.toBeNull();
    set("Ticket", "ABC-1");
    expect(field("Ticket").getAttribute("aria-invalid")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  const ends: [string, string, string, string | null][] = [
    ["start + minutes", "10:00", "30", "ends 10:30"],
    ["past midnight", "23:30", "60", "ends after midnight"],
    ["exactly midnight", "23:00", "60", "ends at midnight"],
    ["empty minutes", "10:00", "", null],
    ["empty start", "", "30", null],
    ["out-of-range minutes", "10:00", "721", null],
  ];
  it.each(ends)("end line: %s", (_n, start, minutes, text) => {
    mount();
    open();
    set("Start", start);
    set("Minutes", minutes);
    const el = document.querySelector(".note-form-end");
    if (text === null) expect(el).toBeNull();
    else {
      expect(el?.textContent).toBe(text);
      expect(el?.closest("label")).toBeNull();
    }
  });

  it("toasts the save, and not a daemon error", async () => {
    let latest: { text: string }[] = [];
    const unsub = subscribe((m) => (latest = m));
    const before = latest.length;
    addNoteBlock.mockImplementationOnce(async () => ({ ok: false, error: "daemon down" }));
    mount();
    open();
    await fill();
    expect(latest.length).toBe(before);
    cleanup();
    mount();
    open();
    await fill();
    expect(latest.slice(before).map((m) => m.text)).toEqual(["Added 30 min on ABC-1 — writing the description…"]);
    unsub();
  });
});
