// "Work logged" section: the two-step Tempo sync (preview, send/update), its strips and the nothing-sent messages.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { btn, calls, changedDay, day, open, settle, toggle } from "./workLogTestKit";

afterEach(cleanup);

type Sync = { synced: number; results?: { block_id: number; status: string; reason: string | null }[] };
// The real daemon never counts a dry run as synced; what it would send comes back as `dry-run*` rows.
const DRY = { synced: 0, errors: [] as string[], results: [{ block_id: 1, status: "dry-run", reason: null }] };
const sync = (real: Sync, dry: Sync = DRY) =>
  mock(async (_d: string, isDry: boolean) => ({
    ok: true as const,
    data: { day: "", dry_run: isDry, skipped: 0, errors: [] as string[], ...(isDry ? dry : real) },
  }));

const PREVIEW = "Send to Tempo, Thu 1 Oct";

describe("preview", () => {
  const start = async (runSync = sync({ synced: 1 })) => {
    const r = await open([day()], { runSync });
    fireEvent.click(btn(PREVIEW));
    await screen.findByRole("group", { name: "Confirm sending Thu 1 Oct to Tempo" });
    return { ...r, runSync };
  };

  it("asks one line: Send <hours> to Tempo for <day>?, with no card furniture", async () => {
    await start();
    const box = document.querySelector(".task-day-confirm") as HTMLElement;
    expect(box.getAttribute("role")).toBe("group");
    expect(box.getAttribute("aria-label")).toBe("Confirm sending Thu 1 Oct to Tempo");
    expect(box.querySelector(".task-day-confirm-q")!.textContent).toBe("Send 1h 30m to Tempo for Thu 1 Oct?");
    expect(box.querySelector("h5, dl")).toBeNull();
    expect(box.textContent).not.toContain("2026-10-01");
    expect(btn("Send")).toBeTruthy();
    expect(btn("Cancel")).toBeTruthy();
  });

  it("marks the text to be sent while open and clears it on Cancel", async () => {
    await start();
    expect(document.querySelector(".task-day[data-confirm]")).not.toBeNull();
    fireEvent.click(btn("Cancel"));
    await settle();
    expect(document.querySelector(".task-day[data-confirm]")).toBeNull();
  });

  it("focuses Send; Esc cancels, is prevented, and refocuses the trigger", async () => {
    const { runSync } = await start();
    expect(document.activeElement).toBe(btn("Send"));
    // fireEvent returns false when a handler called preventDefault.
    expect(fireEvent.keyDown(document.activeElement as Element, { key: "Escape" })).toBe(false);
    await settle();
    expect(screen.queryByRole("group", { name: /^Confirm sending/ })).toBeNull();
    expect(document.activeElement).toBe(btn(PREVIEW));
    expect(runSync.mock.calls).toHaveLength(1);
  });

  it("Esc outside the preview does not cancel it", async () => {
    await start();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("group", { name: /^Confirm sending/ })).toBeTruthy();
  });

  it("confirmation strip persists after refetch and is announced", async () => {
    const { a, onAnnounce } = await start();
    fireEvent.click(btn("Send"));
    expect(await screen.findByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    await settle();
    expect(calls(a.loadTicketBlocks)).toHaveLength(2);
    expect(screen.getByText("Sent to Tempo · 1h 30m")).toBeTruthy();
    expect(onAnnounce).toHaveBeenCalledWith("Sent 1h 30m to Tempo for ABC-1 on Thu 1 Oct.");
  });

  it("nothing sent: names the first reason in plain words", async () => {
    const results = [{ block_id: 1, status: "skipped", reason: "already in Tempo — logged outside worklog" }];
    await start(sync({ synced: 0, results }));
    fireEvent.click(btn("Send"));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo/);
    expect(msg.textContent).toBe("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo — logged outside worklog.");
    expect(document.body.textContent).not.toMatch(/daemon|issue mapping/i);
  });

  it("a dry run with nothing to send says why and skips the preview", async () => {
    const results = [{ block_id: 1, status: "skipped", reason: "already in Tempo — logged outside worklog" }];
    const runSync = sync({ synced: 0 }, { synced: 0, results });
    await open([day()], { runSync });
    fireEvent.click(btn(PREVIEW));
    expect(await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct: already in Tempo/)).toBeTruthy();
    expect(screen.queryByRole("group", { name: /^Confirm sending/ })).toBeNull();
    expect(runSync.mock.calls).toHaveLength(1);
  });

  it("nothing sent without a reason: generic plain copy", async () => {
    await start(sync({ synced: 0 }));
    fireEvent.click(btn("Send"));
    expect(
      await screen.findByText("Nothing was sent to Tempo for ABC-1 on Thu 1 Oct. It may already be in Tempo, or have no hours."),
    ).toBeTruthy();
  });
});

describe("strips stay visible when the day is folded", () => {
  it("the Sent to Tempo strip and the open preview survive folding the day", async () => {
    await open();
    fireEvent.click(btn(PREVIEW));
    await screen.findByRole("group", { name: "Confirm sending Thu 1 Oct to Tempo" });
    fireEvent.click(toggle()); // fold: the day was open
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    expect(screen.getByRole("group", { name: /^Confirm sending/ })).toBeTruthy();
    fireEvent.click(btn("Send"));
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

  it("the update confirm shows In Tempo -> now, or just the hours when unknown", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Update Tempo, Thu 1 Oct"));
    await screen.findByRole("group", { name: "Confirm sending Thu 1 Oct to Tempo" });
    expect(document.querySelector(".task-day-confirm-q")!.textContent).toBe("Update Tempo: 1h → 1h 30m?");
    expect(btn("Update")).toBeTruthy();
    cleanup();
    await open([changedDay({ in_tempo_seconds: null })]);
    fireEvent.click(btn("Update Tempo, Thu 1 Oct"));
    await screen.findByRole("group", { name: "Confirm sending Thu 1 Oct to Tempo" });
    expect(document.querySelector(".task-day-confirm-q")!.textContent).toBe("Update 1h 30m in Tempo for Thu 1 Oct?");
  });
});

describe("focus after Send", () => {
  it("a real send focuses the Sent to Tempo strip", async () => {
    await open();
    fireEvent.click(btn(PREVIEW));
    fireEvent.click(await screen.findByRole("button", { name: "Send" }));
    const strip = await screen.findByText("Sent to Tempo · 1h 30m");
    expect(strip.getAttribute("tabindex")).toBe("-1");
    expect(document.activeElement).toBe(strip);
  });

  it("an update says Tempo updated and focuses it", async () => {
    await open([changedDay()]);
    fireEvent.click(btn("Update Tempo, Thu 1 Oct"));
    fireEvent.click(await screen.findByRole("button", { name: "Update" }));
    const strip = await screen.findByText("Tempo updated · 1h 30m");
    expect(document.activeElement).toBe(strip);
  });

  it("nothing sent focuses the message", async () => {
    const runSync = mock(async (_d: string, dry: boolean) => ({
      ok: true as const,
      data: dry ? DRY : { synced: 0, errors: [], results: [] },
    }));
    await open([day()], { runSync });
    fireEvent.click(btn(PREVIEW));
    fireEvent.click(await screen.findByRole("button", { name: "Send" }));
    const msg = await screen.findByText(/^Nothing was sent to Tempo for ABC-1 on Thu 1 Oct/);
    expect(document.activeElement).toBe(msg);
  });
});
