// Fix round 1 for the "Work logged" section: chip tones, edit cue, preview box, plain outcomes, form Esc/validation.

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
  tracked_seconds: over.line_seconds ?? 3600,
  hours_set_by_hand: false,
  blocks: [block()],
  ...over,
});

const load = (days: TicketDay[]) =>
  mock(async () => ({ ok: true as const, data: { key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days } as TicketBlocks }));

type Sync = { synced: number; results?: { block_id: number; status: string; reason: string | null }[] };
const sync = (real: Sync, dry: Sync = { synced: 1 }) =>
  mock(async (_d: string, isDry: boolean) => ({
    ok: true as const,
    data: { day: "", dry_run: isDry, skipped: 0, errors: [] as string[], ...(isDry ? dry : real) },
  }));

async function open(days: TicketDay[] = [day()], runSync = sync({ synced: 1 })) {
  const a = {
    loadTicketBlocks: load(days),
    runSync,
    logTicketTime: mock(async () => ({ ok: true as const, data: block() })),
  } as unknown as TaskActions;
  const onAnnounce = mock((_m: string) => {});
  render(<TaskWorkLog taskKey="ABC-1" actions={a} onAnnounce={onAnnounce} />);
  await waitFor(() => expect(document.querySelector(".task-skel")).toBeNull());
  return { a, onAnnounce, runSync };
}

describe("day head", () => {
  it("tones the chips: unsynced slate, changed amber, synced sage", async () => {
    await open([
      day({ day: "2026-10-03", blocks: [block({ tempo_worklog_id: "a" })] }),
      day({ day: "2026-10-02", blocks: [block({ tempo_worklog_id: "a", dirty: true })] }),
      day({ day: "2026-10-01" }),
    ]);
    expect(screen.getByText("In Tempo").getAttribute("data-chip")).toBe("ok");
    expect(screen.getByText("Changed since sync").getAttribute("data-chip")).toBe("changed");
    expect(screen.getByText("Not synced").getAttribute("data-chip")).toBe("none");
  });

  it("hours read as editable: labelled button with a pencil", async () => {
    await open();
    const b = screen.getByRole("button", { name: "Edit hours for Thu 1 Oct" });
    expect(b.textContent).toBe("1h 30m");
    expect(b.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByRole("button", { name: "Edit text for Thu 1 Oct" }).textContent).toBe("Edit text");
  });

  it("block rows end in a chevron", async () => {
    await open();
    expect(document.querySelector(".task-block-row .task-block-go")?.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("preview", () => {
  const start = async (runSync = sync({ synced: 1 })) => {
    const r = await open([day()], runSync);
    fireEvent.click(screen.getByRole("button", { name: "Preview sync Thu 1 Oct to Tempo" }));
    await screen.findByText("Preview — nothing sent yet");
    return r;
  };

  it("shows labelled lines, the formatted day, the quote and the note", async () => {
    await start();
    const box = document.querySelector(".task-day-preview") as HTMLElement;
    expect(box.textContent).toContain("Hours1h 30m");
    expect(box.textContent).toContain("DayThu 1 Oct");
    expect(box.textContent).toContain("TicketABC-1");
    expect(box.textContent).not.toContain("2026-10-01");
    expect(screen.getByText("“Worked on login”")).toBeTruthy();
    expect(screen.getByText("Sends only this ticket's line for this day to Tempo.")).toBeTruthy();
  });

  it("focuses Send to Tempo; Esc cancels, is prevented, and refocuses the trigger", async () => {
    const { runSync } = await start();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Send to Tempo" }));
    // fireEvent returns false when a handler called preventDefault.
    expect(fireEvent.keyDown(document.body, { key: "Escape" })).toBe(false);
    await waitFor(() => expect(screen.queryByText("Preview — nothing sent yet")).toBeNull());
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Preview sync Thu 1 Oct to Tempo" }));
    expect(runSync.mock.calls).toHaveLength(1);
  });

  it("confirmation strip persists after refetch and is announced", async () => {
    const { a, onAnnounce } = await start();
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo" }));
    expect(await screen.findByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    await waitFor(() => expect((a.loadTicketBlocks as unknown as ReturnType<typeof mock>).mock.calls).toHaveLength(2));
    expect(screen.getByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    expect(onAnnounce).toHaveBeenCalledWith("Sent 1h 30m to Tempo for ABC-1 on Thu 1 Oct.");
  });

  it("nothing sent: names the first reason in plain words", async () => {
    const results = [{ block_id: 1, status: "skipped", reason: "already in Tempo — logged outside worklog" }];
    await start(sync({ synced: 0, results }));
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo" }));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo/);
    expect(msg.textContent).toBe("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo — logged outside worklog.");
    expect(document.body.textContent).not.toMatch(/daemon|issue mapping/i);
  });

  it("nothing sent without a reason: generic plain copy", async () => {
    await start(sync({ synced: 0 }));
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo" }));
    expect(
      await screen.findByText("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct. It may already be in Tempo, or have no hours."),
    ).toBeTruthy();
  });
});

describe("log form", () => {
  const openForm = async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
  };

  it("Esc closes the form (prevented) and returns focus to Log time", async () => {
    await openForm();
    // fireEvent returns false when a handler called preventDefault.
    expect(fireEvent.keyDown(screen.getByLabelText("What you did"), { key: "Escape" })).toBe(false);
    await waitFor(() => expect(screen.queryByLabelText("What you did")).toBeNull());
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Log time" }));
  });

  it("validates length and description on blur", async () => {
    await openForm();
    fireEvent.blur(screen.getByLabelText("What you did"));
    expect(screen.getByText("Say what you did.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Length"), { target: { value: "721" } });
    fireEvent.blur(screen.getByLabelText("Length"));
    expect(screen.getByText("Length must be 1 to 720 minutes.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "x" } });
    expect(screen.queryByText("Say what you did.")).toBeNull();
  });

  it("shows the chosen day in words", async () => {
    await openForm();
    expect(screen.getByText("Fri 2 Oct")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Day"), { target: { value: "2026-10-01" } });
    expect(screen.getAllByText("Thu 1 Oct").length).toBeGreaterThan(0);
  });
});

describe("empty state", () => {
  it("offers a Log time button that opens the form", async () => {
    await open([]);
    expect(screen.getByText("No work logged on ABC-1 in the last 14 days.")).toBeTruthy();
    const buttons = screen.getAllByRole("button", { name: "Log time" });
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[1]);
    expect(screen.getByLabelText("What you did")).toBeTruthy();
    expect(screen.queryAllByRole("button", { name: "Log time" })).toHaveLength(0);
  });
});
