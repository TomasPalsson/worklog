// Fix round 3 for the "Work logged" section: focus never drops to body, "Changed since sent" explained,
// summary in the head, chip pressed state, Cancel discard prompt, headings, scoped preview Esc, contrast.

import { afterEach, describe, expect, it, mock, setDefaultTimeout } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskWorkLog } from "./TaskWorkLog";

// getByRole walks the whole accessibility tree in jsdom (~0.5s a call); flows with many lookups need room.
setDefaultTimeout(20000);

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

const changedDay = (over: Partial<TicketDay> = {}) =>
  day({ in_tempo_seconds: 3600, blocks: [block({ tempo_worklog_id: "w1", dirty: true })], ...over });

const payload = (days: TicketDay[]): TicketBlocks => ({ key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days });
const ok = { ok: true as const, data: {} };

async function open(days: TicketDay[] = [day()]) {
  const a = {
    loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload(days) })),
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineHours: mock(async () => ok),
    saveTempoLineText: mock(async () => ok),
    runSync: mock(async () => ({ ok: true as const, data: { synced: 1, errors: [], results: [] } })),
  } as unknown as TaskActions;
  render(<TaskWorkLog taskKey="ABC-1" actions={a} onAnnounce={() => {}} />);
  await waitFor(() => expect(document.querySelector(".task-skel")).toBeNull());
  return a;
}

const btn = (name: string) => screen.getByRole("button", { name });
const logTime = () => screen.getByRole("button", { name: "Log time" });
const hoursBtn = () => btn("1h 30m — edit hours for Thu 1 Oct");
const textBtn = () => btn("Edit Tempo text for Thu 1 Oct");

describe("focus never drops to body", () => {
  it("opening the log form focuses Day", async () => {
    await open();
    fireEvent.click(logTime());
    expect(document.activeElement).toBe(screen.getByLabelText("Day"));
  });

  it("Cancel, Esc and Log all return focus to Log time", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(logTime());

    fireEvent.click(logTime());
    fireEvent.keyDown(screen.getByLabelText("What you did"), { key: "Escape" });
    expect(document.activeElement).toBe(logTime());

    fireEvent.click(logTime());
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "work" } });
    fireEvent.click(btn("Log 1h"));
    await waitFor(() => expect(screen.queryByLabelText("What you did")).toBeNull());
    expect(document.activeElement).toBe(logTime());
  });

  it("Discard returns focus to Log time", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "work" } });
    fireEvent.click(btn("Cancel"));
    fireEvent.click(btn("Discard"));
    expect(document.activeElement).toBe(logTime());
  });

  it("hours editor: Save and Cancel return focus to the hours button", async () => {
    await open();
    fireEvent.click(hoursBtn());
    fireEvent.change(screen.getByLabelText("Hours for Thu 1 Oct"), { target: { value: "2" } });
    fireEvent.click(btn("Save hours"));
    await waitFor(() => expect(screen.queryByLabelText("Hours for Thu 1 Oct")).toBeNull());
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
    await waitFor(() => expect(screen.queryByLabelText("Line text for Thu 1 Oct")).toBeNull());
    expect(document.activeElement).toBe(textBtn());

    fireEvent.click(textBtn());
    fireEvent.click(btn("Cancel"));
    expect(document.activeElement).toBe(textBtn());
  });
});

describe("Changed since sent is explained", () => {
  it("chip title and a day note give In Tempo -> now", async () => {
    await open([changedDay()]);
    const note = "Edited after it was sent · In Tempo: 1h → now 1h 30m";
    expect(screen.getByText("Changed since sent").getAttribute("title")).toBe(note);
    expect(screen.getByText(note).tagName).toBe("P");
  });

  it("omits the arrow part when Tempo's hours are unknown", async () => {
    await open([changedDay({ in_tempo_seconds: null })]);
    expect(screen.getByText("Edited after it was sent to Tempo")).toBeTruthy();
  });

  it("the update preview lists In Tempo and Will be instead of Hours", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Preview update Thu 1 Oct in Tempo"));
    await screen.findByText("Preview — Tempo will be updated");
    const box = document.querySelector(".task-day-preview") as HTMLElement;
    expect(box.textContent).toContain("In Tempo1h");
    expect(box.textContent).toContain("Will be1h 30m");
    expect(box.textContent).not.toContain("Hours");
  });
});

describe("day head and text editing", () => {
  it("day label is an h4 and the preview heading an h5", async () => {
    await open();
    expect(screen.getByRole("heading", { level: 4, name: "Thu 1 Oct" })).toBeTruthy();
    fireEvent.click(btn("Preview send Thu 1 Oct to Tempo"));
    expect(await screen.findByRole("heading", { level: 5, name: "Preview — nothing sent yet" })).toBeTruthy();
  });

  it("Edit Tempo text sits right after the line text and the text hides while editing", async () => {
    await open();
    const text = document.querySelector(".task-day-text") as HTMLElement;
    expect(text.nextElementSibling).toBe(textBtn());
    fireEvent.click(textBtn());
    expect(document.querySelector(".task-day-text")).toBeNull();
    expect((screen.getByLabelText("Line text for Thu 1 Oct") as HTMLTextAreaElement).value).toBe("Worked on login");
  });

  it("the summary sits in the head row, after the heading, even with the form open", async () => {
    await open();
    const head = () => (document.querySelector(".task-work-head") as HTMLElement).textContent;
    expect(head()).toContain("1h 30m over 1 day");
    fireEvent.click(logTime());
    expect(head()).toContain("1h 30m over 1 day");
  });

  it("Esc outside the preview does not cancel it", async () => {
    await open();
    fireEvent.click(btn("Preview send Thu 1 Oct to Tempo"));
    await screen.findByText("Preview — nothing sent yet");
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText("Preview — nothing sent yet")).toBeTruthy();
  });
});

describe("log form chips and Cancel", () => {
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

  it("Cancel with a typed description asks first, like Esc; Keep editing stays", async () => {
    await open();
    fireEvent.click(logTime());
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "work" } });
    fireEvent.click(btn("Cancel"));
    expect(screen.getByText(/Discard this entry\?/)).toBeTruthy();
    fireEvent.click(btn("Keep editing"));
    expect(screen.queryByText(/Discard this entry\?/)).toBeNull();
    expect((screen.getByLabelText("What you did") as HTMLTextAreaElement).value).toBe("work");
  });
});

describe("contrast", () => {
  it("light --fg-subtle is 0.54 in the base :root and nothing hard-codes the old value", () => {
    const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
    expect(css).toContain("--fg-subtle:  oklch(0.54 0.01 260);");
    expect(css).not.toContain("oklch(0.58 0.01 260)");
  });
});
