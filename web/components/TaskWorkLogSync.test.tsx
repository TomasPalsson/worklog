// "Work logged" section: the two-step Tempo sync (preview, send/update), its strips and the nothing-sent messages.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { btn, calls, changedDay, day, open, settle, toggle } from "./workLogTestKit";

afterEach(cleanup);

type Sync = { synced: number; results?: { block_id: number; status: string; reason: string | null }[] };
const sync = (real: Sync, dry: Sync = { synced: 1 }) =>
  mock(async (_d: string, isDry: boolean) => ({
    ok: true as const,
    data: { day: "", dry_run: isDry, skipped: 0, errors: [] as string[], ...(isDry ? dry : real) },
  }));

const PREVIEW = "Send to Tempo, Thu 1 Oct";

describe("preview", () => {
  const start = async (runSync = sync({ synced: 1 })) => {
    const r = await open([day()], { runSync });
    fireEvent.click(btn(PREVIEW));
    await screen.findByText("Preview — nothing sent yet");
    return { ...r, runSync };
  };

  it("shows labelled lines, the formatted day, the text pointer and the note", async () => {
    await start();
    const box = document.querySelector(".task-day-preview") as HTMLElement;
    expect(box.textContent).toContain("Hours1h 30m");
    expect(box.textContent).toContain("DayThu 1 Oct");
    expect(box.textContent).not.toContain("Ticket"); // the dialog is the ticket
    expect(box.textContent).not.toContain("2026-10-01");
    expect(box.textContent).toContain("Textas shown below");
    expect(box.textContent).not.toContain("“");
    expect(screen.getByText("Sends only this ticket's line for this day to Tempo.")).toBeTruthy();
  });

  it("focuses Send to Tempo; Esc cancels, is prevented, and refocuses the trigger", async () => {
    const { runSync } = await start();
    expect(document.activeElement).toBe(btn("Send to Tempo"));
    // fireEvent returns false when a handler called preventDefault.
    expect(fireEvent.keyDown(document.activeElement as Element, { key: "Escape" })).toBe(false);
    await settle();
    expect(screen.queryByText("Preview — nothing sent yet")).toBeNull();
    expect(document.activeElement).toBe(btn(PREVIEW));
    expect(runSync.mock.calls).toHaveLength(1);
  });

  it("Esc outside the preview does not cancel it", async () => {
    await start();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText("Preview — nothing sent yet")).toBeTruthy();
  });

  it("confirmation strip persists after refetch and is announced", async () => {
    const { a, onAnnounce } = await start();
    fireEvent.click(btn("Send to Tempo"));
    expect(await screen.findByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    await settle();
    expect(calls(a.loadTicketBlocks)).toHaveLength(2);
    expect(screen.getByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    expect(onAnnounce).toHaveBeenCalledWith("Sent 1h 30m to Tempo for ABC-1 on Thu 1 Oct.");
  });

  it("nothing sent: names the first reason in plain words", async () => {
    const results = [{ block_id: 1, status: "skipped", reason: "already in Tempo — logged outside worklog" }];
    await start(sync({ synced: 0, results }));
    fireEvent.click(btn("Send to Tempo"));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo/);
    expect(msg.textContent).toBe("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo — logged outside worklog.");
    expect(document.body.textContent).not.toMatch(/daemon|issue mapping/i);
  });

  it("nothing sent without a reason: generic plain copy", async () => {
    await start(sync({ synced: 0 }));
    fireEvent.click(btn("Send to Tempo"));
    expect(
      await screen.findByText("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct. It may already be in Tempo, or have no hours."),
    ).toBeTruthy();
  });
});

describe("strips stay visible when the day is folded", () => {
  it("the Sent to Tempo strip and the open preview survive folding the day", async () => {
    await open();
    fireEvent.click(btn(PREVIEW));
    await screen.findByText("Preview — nothing sent yet");
    fireEvent.click(toggle()); // fold: the day was open
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    expect(screen.getByText("Preview — nothing sent yet")).toBeTruthy();
    fireEvent.click(btn("Send to Tempo"));
    expect(await screen.findByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    expect(document.querySelector(".task-day-body")).toBeNull();
  });

  it("opening the preview on a folded day opens the day, since the preview points at its text", async () => {
    await open([day({ day: "2026-10-02" }), day()]);
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(btn("Send to Tempo, Thu 1 Oct"));
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
  });

  it("logging time on a folded day opens it and shows the Logged strip", async () => {
    await open();
    fireEvent.click(toggle()); // fold
    fireEvent.click(btn("Log time"));
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: "x" } });
    fireEvent.click(btn(/^Log \d/)); // the stubbed log lands on Thu 1 Oct
    await settle();
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Logged 1h")).toBeTruthy();
  });
});

describe("Changed since sent is explained", () => {
  it("the chip title gives In Tempo -> now, and an opened day says the same reason as a muted line", async () => {
    await open([changedDay()]);
    const note = "Edited after it was sent · In Tempo: 1h → now 1h 30m";
    expect(screen.getByText("Changed since sent").closest(".task-day-chip")!.getAttribute("title")).toBe(note);
    expect(document.querySelector(".task-day-body .task-day-note")?.textContent).toBe(note);
  });

  it("omits the arrow part when Tempo's hours are unknown", async () => {
    await open([changedDay({ in_tempo_seconds: null })]);
    expect(screen.getByText("Changed since sent").closest(".task-day-chip")!.getAttribute("title")).toBe("Edited after it was sent to Tempo");
  });

  it("the update preview lists In Tempo and Will be instead of Hours", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Update Tempo, Thu 1 Oct"));
    await screen.findByText("Preview — Tempo will be updated");
    const box = document.querySelector(".task-day-preview") as HTMLElement;
    expect(box.textContent).toContain("In Tempo1h");
    expect(box.textContent).toContain("Will be1h 30m");
    expect(box.textContent).not.toContain("Hours");
  });
});

describe("focus after Send", () => {
  it("a real send focuses the Sent to Tempo strip", async () => {
    await open();
    fireEvent.click(btn(PREVIEW));
    fireEvent.click(await screen.findByText("Send to Tempo", { selector: "button" }));
    const strip = await screen.findByText("Sent to Tempo · 1h 30m");
    expect(strip.getAttribute("tabindex")).toBe("-1");
    expect(document.activeElement).toBe(strip);
  });

  it("an update says Tempo updated and focuses it", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Update Tempo, Thu 1 Oct"));
    fireEvent.click(await screen.findByText("Update Tempo", { selector: "button" }));
    const strip = await screen.findByText("Tempo updated · 1h 30m");
    expect(document.activeElement).toBe(strip);
  });

  it("nothing sent focuses the message", async () => {
    const runSync = mock(async (_d: string, dry: boolean) => ({
      ok: true as const,
      data: { synced: dry ? 1 : 0, errors: [], results: [] },
    }));
    await open([day()], { runSync });
    fireEvent.click(btn(PREVIEW));
    fireEvent.click(await screen.findByText("Send to Tempo", { selector: "button" }));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct/);
    expect(document.activeElement).toBe(msg);
  });
});
