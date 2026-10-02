// Fix round 2 for the "Work logged" section: Esc never destroys a draft, hours note, measured clamp, log confirmation.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskWorkLog } from "./TaskWorkLog";

afterEach(cleanup);

const block = (over: Partial<RawBlock> = {}): RawBlock =>
  ({
    id: 1,
    day: "2026-10-01",
    jira_issue: "ABC-1",
    started_at: "2026-10-01T09:00:00Z",
    ended_at: "2026-10-01T10:30:00Z",
    duration_seconds: 5400,
    description: "Fixed the redirect",
    estimated_by: "claude",
    tempo_worklog_id: null,
    dirty: false,
    ...over,
  }) as RawBlock;

const day = (over: Partial<TicketDay> = {}): TicketDay => ({
  day: "2026-10-01",
  line_seconds: 5400,
  line_text: "Worked on login",
  tracked_seconds: 5400,
  hours_set_by_hand: false,
  in_tempo_seconds: null,
  blocks: [block()],
  ...over,
});

const payload = (days: TicketDay[]): TicketBlocks => ({ key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days });

async function open(days: TicketDay[] = [day()], over: Record<string, unknown> = {}) {
  const loadTicketBlocks = mock(async () => ({ ok: true as const, data: payload(days) }));
  const a = {
    loadTicketBlocks,
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineText: mock(async () => ({ ok: true as const, data: {} })),
    ...over,
  } as unknown as TaskActions;
  render(<TaskWorkLog taskKey="ABC-1" actions={a} onAnnounce={() => {}} />);
  await waitFor(() => expect(document.querySelector(".task-skel")).toBeNull());
  return { a, loadTicketBlocks };
}

describe("Esc never destroys a draft", () => {
  it("Esc in the text editor cancels only that edit and returns focus", async () => {
    const { a } = await open();
    fireEvent.click(screen.getByRole("button", { name: "Edit Tempo text for Thu 1 Oct" }));
    const box = screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "half typed" } });
    expect(fireEvent.keyDown(box, { key: "Escape" })).toBe(false);
    expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Edit Tempo text for Thu 1 Oct" }));
    expect(screen.getByText("Worked on login")).toBeTruthy();
    expect((a.saveTempoLineText as unknown as ReturnType<typeof mock>).mock.calls).toHaveLength(0);
  });

  it("Esc outside the form (e.g. the composer) leaves the log form open", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
    // Not prevented: nothing in this form claimed it.
    expect(fireEvent.keyDown(document.body, { key: "Escape" })).toBe(true);
    expect(screen.getByLabelText("What you did")).toBeTruthy();
  });

  it("Esc with a typed description asks before discarding", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
    const desc = screen.getByLabelText("What you did");
    fireEvent.change(desc, { target: { value: "important notes" } });
    fireEvent.keyDown(desc, { key: "Escape" });
    expect(screen.getByText(/Discard this entry\?/)).toBeTruthy();
    expect(screen.getByLabelText("What you did")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.queryByText(/Discard this entry\?/)).toBeNull();
    expect((screen.getByLabelText("What you did") as HTMLTextAreaElement).value).toBe("important notes");
    fireEvent.keyDown(desc, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(screen.queryByLabelText("What you did")).toBeNull();
  });
});

describe("hours note", () => {
  const note = (text: string) => screen.queryByText(text);

  it("hand-set hours", async () => {
    await open([day({ hours_set_by_hand: true, line_seconds: 5400, tracked_seconds: 2400 })]);
    expect(note("Set by hand · 40m tracked")).toBeTruthy();
    expect(screen.getByRole("button", { name: "1h 30m — edit hours for Thu 1 Oct" }).hasAttribute("title")).toBe(false);
  });

  it("rounded hours", async () => {
    await open([day({ line_seconds: 1800, tracked_seconds: 2400 })]);
    expect(note("Rounded to the nearest half hour from 40m tracked")).toBeTruthy();
    expect(screen.getByRole("button", { name: "30m — edit hours for Thu 1 Oct" }).hasAttribute("title")).toBe(false);
  });

  it("no note when hours equal the tracked time", async () => {
    await open([day()]);
    expect(document.querySelector(".task-day-note")).toBeNull();
  });
});

describe("clamp toggle", () => {
  const measure = (scroll: number, client: number) => {
    const proto = HTMLElement.prototype;
    Object.defineProperty(proto, "scrollHeight", { configurable: true, get: () => scroll });
    Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => client });
  };
  afterEach(() => {
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
  });

  it("shows more only when the text overflows, regardless of length", async () => {
    measure(60, 36);
    await open([day({ line_text: "short" })]);
    expect(screen.getByRole("button", { name: "more" })).toBeTruthy();
    cleanup();
    measure(36, 36);
    await open([day({ line_text: "x".repeat(400) })]);
    expect(screen.queryByRole("button", { name: "more" })).toBeNull();
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
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "New entry" } });
    fireEvent.click(screen.getByRole("button", { name: "Log 1h" }));
    await waitFor(() => expect(document.querySelector("[data-fresh]")).toBeTruthy());
    expect(document.querySelector("[data-fresh]")?.textContent).toContain("New entry");
    expect(scroll).toHaveBeenCalledWith({ block: "nearest" });
    expect(screen.getByText("Logged 1h").className).toBe("task-day-sent");
    expect((a.logTicketTime as unknown as ReturnType<typeof mock>).mock.calls).toHaveLength(1);
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
  });
});
