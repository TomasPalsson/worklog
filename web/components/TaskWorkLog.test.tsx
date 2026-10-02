// Work log tab: blocks grouped by day, Tempo chips, log time, edit the day, two-step sync.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { formatRange, todayISO } from "@/lib/format";
import type { RawBlock, TicketBlocks, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { Harness, dryRunData, more, toggle } from "./workLogTestKit";

afterEach(cleanup);

const block = (over: Partial<RawBlock> = {}): RawBlock => ({
  id: 1,
  day: "2026-10-01",
  jira_issue: "ABC-1",
  started_at: "2026-10-01T09:00:00Z",
  ended_at: "2026-10-01T10:00:00Z",
  duration_seconds: 3600,
  description: "Fixed the redirect",
  estimated_by: "claude",
  flagged: false,
  tempo_worklog_id: null,
  is_personal: false,
  dirty: false,
  exported_at: null,
  ignored_at: null,
  ticket_origin: "manual",
  ...over,
});

const day = (over: Partial<TicketDay> = {}): TicketDay => ({
  day: "2026-10-01",
  line_seconds: 3600,
  line_text: "Worked on login",
  tracked_seconds: over.line_seconds ?? 3600,
  hours_set_by_hand: false,
  in_tempo_seconds: null,
  blocks: [block()],
  ...over,
});

const payload = (days: TicketDay[]): TicketBlocks => ({
  key: "ABC-1",
  from: "2026-09-19",
  to: "2026-10-02",
  days,
  in_tempo_total_seconds: null,
  pulled_at: null,
  today: { day: "2026-10-02", worked_seconds: 0, in_tempo_seconds: null, ticket_worked_seconds: 0, ticket_in_tempo_seconds: null },
});
const loads = (days: TicketDay[]) => mock(async () => ({ ok: true as const, data: payload(days) }));

const two = [
  day({ day: "2026-10-01", line_seconds: 5400, blocks: [block({ id: 1, tempo_worklog_id: "w1" })] }),
  day({
    day: "2026-09-30",
    line_seconds: 1800,
    line_text: "",
    blocks: [block({ id: 2, day: "2026-09-30", description: null, estimated_by: "manual" })],
  }),
];

function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}) {
  return {
    loadTicketBlocks: loads(two),
    logTicketTime: mock(async () => ({ ok: true as const, data: block({ id: 9 }) })),
    saveTempoLineHours: mock(async () => ({ ok: true as const, data: {} })),
    saveTempoLineText: mock(async () => ({ ok: true as const, data: {} })),
    runSync: mock(async (_d: string, dry: boolean) => ({
      ok: true as const,
      data: dry ? { day: "", dry_run: true, ...dryRunData } : { day: "", dry_run: false, synced: 1, skipped: 0, errors: [] as string[] },
    })),
    ...over,
  } as unknown as TaskActions;
}

const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

function show(a: TaskActions = actions()) {
  const onAnnounce = mock((_m: string) => {});
  render(<Harness actions={a} onAnnounce={onAnnounce} />);
  return { a, onAnnounce };
}

async function ready(a: TaskActions = actions()) {
  const r = show(a);
  await screen.findByText(/^\d+ days?$/);
  return r;
}

describe("TaskWorkLog read", () => {
  it("summarises the day count and loads for the key", async () => {
    const { a } = show();
    expect(await screen.findByText("2 days")).toBeTruthy();
    expect(calls(a.loadTicketBlocks)[0]).toEqual(["ABC-1"]);
  });

  it("says 1 day in the singular", async () => {
    show(actions({ loadTicketBlocks: loads([day()]) }));
    expect(await screen.findByText("1 day")).toBeTruthy();
  });

  it("groups newest first with plain day labels", async () => {
    await ready();
    const labels = [...document.querySelectorAll(".task-day-label")];
    expect(labels.map((l) => l.textContent)).toEqual(["Thu 1 Oct", "Wed 30 Sep"]);
    // The day label is plain text; the block rows are the links.
    expect(screen.queryAllByRole("link", { name: /^(Thu 1 Oct|Wed 30 Sep)$/ })).toHaveLength(0);
  });

  it("chips: all synced, changed since sync, not synced", async () => {
    await ready(
      actions({
        loadTicketBlocks: loads([
          day({ day: "2026-10-03", blocks: [block({ tempo_worklog_id: "a" })] }),
          day({ day: "2026-10-02", blocks: [block({ tempo_worklog_id: "a", dirty: true })] }),
          day({ day: "2026-10-01", blocks: [block({ tempo_worklog_id: "a" }), block({ id: 2, tempo_worklog_id: "" })] }),
        ]),
      }),
    );
    expect(screen.getByText("Sent")).toBeTruthy();
    expect(screen.getByText("Changed since sent")).toBeTruthy();
    expect(screen.getByText("Not sent")).toBeTruthy();
  });

  it("shows line text and omits it when empty", async () => {
    await ready();
    expect(screen.getAllByText("Worked on login")).toHaveLength(1);
  });

  it("long line text can be expanded", async () => {
    const long = "word ".repeat(80).trim();
    // The toggle follows measured overflow, so simulate a clamped element.
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, get: () => 60 });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 36 });
    try {
      await ready(actions({ loadTicketBlocks: loads([day({ line_text: long })]) }));
      fireEvent.click(screen.getByRole("button", { name: "more" }));
      expect(screen.getByRole("button", { name: "less" })).toBeTruthy();
    } finally {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
    }
  });

  it("block rows link to the block page with range, duration and description", async () => {
    await ready();
    const row = screen.getByRole("link", { name: /Fixed the redirect/ });
    expect(row.getAttribute("href")).toBe("/2026-10-01/block/1");
    expect(within(row).getByText(formatRange("2026-10-01T09:00:00Z", "2026-10-01T10:00:00Z"))).toBeTruthy();
    expect(within(row).getByText("1h")).toBeTruthy();
    expect(within(row).queryByText("Edited")).toBeNull();
  });

  it("tags manual blocks Edited and shows No description", async () => {
    await ready();
    fireEvent.click(toggle("Wed 30 Sep")); // only the newest day starts open
    const row = screen.getByText("No description").closest("a") as HTMLElement;
    expect(within(row).getByText("Edited")).toBeTruthy();
    expect(row.getAttribute("href")).toBe("/2026-09-30/block/2");
  });

  it("empty state", async () => {
    show(actions({ loadTicketBlocks: loads([]) }));
    expect(await screen.findByText("Nothing tracked on ABC-1 yet. Time from your sessions lands here on its own — or log it by hand.")).toBeTruthy();
  });

  it("loading skeleton", () => {
    show(actions({ loadTicketBlocks: mock(() => new Promise(() => {})) }));
    expect(document.querySelectorAll(".task-skel i")).toHaveLength(3);
  });

  it("error with Try again", async () => {
    const load = mock(async () => ({ ok: false as const, error: "daemon down." }));
    show(actions({ loadTicketBlocks: load }));
    expect(await screen.findByText("Couldn't load work for ABC-1: daemon down")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(load.mock.calls.length).toBe(2));
  });
});

describe("log time", () => {
  async function openForm(a = actions()) {
    await ready(a);
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
    return a;
  }
  const fill = (text: string) =>
    fireEvent.change(screen.getByLabelText("What you did"), { target: { value: text } });

  it("defaults: the daemon's today (not the browser's), max that day, a 15-minute start", async () => {
    await openForm();
    const d = screen.getByLabelText("Day") as HTMLInputElement;
    // payload().to is the daemon's local date; it differs from the browser's on purpose.
    expect(d.value).toBe("2026-10-02");
    expect(d.max).toBe("2026-10-02");
    expect((screen.getByLabelText("Start") as HTMLInputElement).value).toMatch(/^\d\d:(00|15|30|45)$/);
  });

  it("requires a description and a valid length", async () => {
    const a = await openForm();
    fireEvent.click(screen.getByRole("button", { name: /^Log \d/ }));
    expect(screen.getByText("Say what you did.")).toBeTruthy();
    fill("x");
    fireEvent.change(screen.getByLabelText("Length"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: /^Log / }));
    expect(screen.getByText("Length must be 1 to 720 minutes.")).toBeTruthy();
    expect(calls(a.logTicketTime)).toHaveLength(0);
  });

  it("chips set the length and the button names it", async () => {
    await openForm();
    fireEvent.click(screen.getByRole("button", { name: "2h" }));
    expect((screen.getByLabelText("Length") as HTMLInputElement).value).toBe("120");
    expect(screen.getByRole("button", { name: "Log 2h" })).toBeTruthy();
  });

  it("submits, refetches and closes", async () => {
    const a = await openForm();
    fill("  Did a thing ");
    fireEvent.click(screen.getByRole("button", { name: "30m" }));
    fireEvent.change(screen.getByLabelText("Start"), { target: { value: "09:15" } });
    fireEvent.click(screen.getByRole("button", { name: "Log 30m" }));
    await waitFor(() => expect(calls(a.logTicketTime)).toHaveLength(1));
    expect(calls(a.logTicketTime)[0]).toEqual([
      "ABC-1",
      { day: todayISO(), start: "09:15", minutes: 30, description: "Did a thing" },
    ]);
    await waitFor(() => expect(calls(a.loadTicketBlocks)).toHaveLength(2));
    await waitFor(() => expect(screen.queryByLabelText("What you did")).toBeNull());
  });

  it("announces the log", async () => {
    const { onAnnounce } = await ready();
    fireEvent.click(screen.getByRole("button", { name: "Log time" }));
    fill("x");
    fireEvent.click(screen.getByRole("button", { name: /^Log \d/ }));
    await waitFor(() => expect(onAnnounce).toHaveBeenCalledWith("Logged 1h on ABC-1."));
  });

  it("keeps the values and shows the error on failure", async () => {
    const a = await openForm(actions({ logTicketTime: mock(async () => ({ ok: false, error: "overlaps lunch" })) }));
    fill("Did a thing");
    fireEvent.click(screen.getByRole("button", { name: /^Log \d/ }));
    expect(await screen.findByText("overlaps lunch")).toBeTruthy();
    expect((screen.getByLabelText("What you did") as HTMLTextAreaElement).value).toBe("Did a thing");
    expect(calls(a.loadTicketBlocks)).toHaveLength(1);
  });
});

describe("edit the day", () => {
  const editHours = () => {
    fireEvent.click(more());
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit hours" }));
  };

  it("saves hours and refetches", async () => {
    const { a } = await ready();
    editHours();
    const input = screen.getByLabelText("Hours for Thu 1 Oct") as HTMLInputElement;
    expect(input.value).toBe("1.5");
    fireEvent.change(input, { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save hours" }));
    await waitFor(() => expect(calls(a.saveTempoLineHours)).toHaveLength(1));
    expect(calls(a.saveTempoLineHours)[0]).toEqual([{ day: "2026-10-01", jira_issue: "ABC-1" }, 7200]);
    await waitFor(() => expect(calls(a.loadTicketBlocks)).toHaveLength(2));
  });

  it("rejects hours that are not a half-hour step", async () => {
    const { a } = await ready();
    editHours();
    fireEvent.change(screen.getByLabelText("Hours for Thu 1 Oct"), { target: { value: "1.3" } });
    fireEvent.click(screen.getByRole("button", { name: "Save hours" }));
    expect(screen.getByText(/half-hour step/)).toBeTruthy();
    expect(calls(a.saveTempoLineHours)).toHaveLength(0);
  });

  it("shows an hours save error inline", async () => {
    await ready(actions({ saveTempoLineHours: mock(async () => ({ ok: false, error: "nope" })) }));
    editHours();
    fireEvent.click(screen.getByRole("button", { name: "Save hours" }));
    expect(await screen.findByText("nope")).toBeTruthy();
  });

  it("edits the line text and refetches", async () => {
    const { a } = await ready();
    fireEvent.click(more());
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit Tempo text" }));
    fireEvent.change(screen.getByLabelText("Line text for Thu 1 Oct"), { target: { value: "New text" } });
    fireEvent.click(screen.getByRole("button", { name: "Save text" }));
    await waitFor(() => expect(calls(a.saveTempoLineText)).toHaveLength(1));
    expect(calls(a.saveTempoLineText)[0]).toEqual([{ day: "2026-10-01", jira_issue: "ABC-1" }, "New text"]);
    await waitFor(() => expect(calls(a.loadTicketBlocks)).toHaveLength(2));
  });
});

describe("sync to Tempo", () => {
  it("only offers sync when the day is not in Tempo", async () => {
    await ready();
    // 1 Oct is fully synced, 30 Sep is not.
    expect(screen.getAllByRole("button", { name: /^Send to Tempo, / })).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Send to Tempo, Wed 30 Sep" })).toBeTruthy();
  });

  it("dry-runs first, previews, and sends only after confirm", async () => {
    const { a } = await ready();
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo, Wed 30 Sep" }));
    expect(await screen.findByText("Preview — nothing sent yet")).toBeTruthy();
    expect(calls(a.runSync)).toEqual([["2026-09-30", true, "ABC-1"]]);
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo" }));
    expect(await screen.findByText("Sent to Tempo · 30m")).toBeTruthy();
    expect(calls(a.runSync)).toEqual([
      ["2026-09-30", true, "ABC-1"],
      ["2026-09-30", false, "ABC-1"],
    ]);
    await waitFor(() => expect(calls(a.loadTicketBlocks)).toHaveLength(2));
  });

  it("does not repeat the line text in the preview; it points at the text below", async () => {
    await ready(actions({ loadTicketBlocks: loads([day({ line_text: "Worked on login" })]) }));
    fireEvent.click(screen.getByRole("button", { name: /^Send to Tempo, / }));
    expect(await screen.findByText("as shown below")).toBeTruthy();
    expect(screen.getAllByText("Worked on login")).toHaveLength(1);
  });

  it("Cancel sends nothing", async () => {
    const { a } = await ready();
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo, Wed 30 Sep" }));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(calls(a.runSync)).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Send to Tempo" })).toBeNull();
  });

  it("shows errors from the real run", async () => {
    const run = mock(async (_d: string, dry: boolean) => ({
      ok: true as const,
      data: dry ? { day: "", dry_run: dry, ...dryRunData } : { day: "", dry_run: dry, synced: 0, skipped: 0, errors: ["Tempo said 400"] },
    }));
    await ready(actions({ runSync: run }));
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo, Wed 30 Sep" }));
    fireEvent.click(await screen.findByRole("button", { name: "Send to Tempo" }));
    expect(await screen.findByText("Tempo said 400")).toBeTruthy();
  });

  it("keeps the Sent to Tempo strip visible after the refetch flips the day to In Tempo", async () => {
    let sent = false;
    const load = mock(async () => ({
      ok: true as const,
      data: payload([
        day({ day: "2026-09-30", blocks: [block({ day: "2026-09-30", tempo_worklog_id: sent ? "w9" : null })] }),
      ]),
    }));
    const run = mock(async (_d: string, dry: boolean) => {
      if (!dry) sent = true;
      return { ok: true as const, data: dry ? { day: "", dry_run: dry, ...dryRunData } : { day: "", dry_run: dry, synced: 1, skipped: 0, errors: [] as string[] } };
    });
    await ready(actions({ loadTicketBlocks: load, runSync: run }));
    fireEvent.click(screen.getByRole("button", { name: /^Send to Tempo, / }));
    fireEvent.click(await screen.findByRole("button", { name: "Send to Tempo" }));
    expect(await screen.findByText("Sent")).toBeTruthy();
    expect(screen.getByText("Sent to Tempo · 1h")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Send to Tempo, / })).toBeNull();
  });

  it("a dry run with nothing sendable says why and offers no Send", async () => {
    const run = mock(async () => ({
      ok: true as const,
      data: { day: "", dry_run: true, synced: 0, skipped: 1, errors: [] as string[] },
    }));
    await ready(actions({ loadTicketBlocks: loads([day({ day: "2026-09-30", line_seconds: 0 })]), runSync: run }));
    fireEvent.click(screen.getByRole("button", { name: /^Send to Tempo, / }));
    expect(await screen.findByText("Nothing was sent to Tempo for ABC-1 on Wed 30 Sep. It may already be in Tempo, or have no hours.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Send to Tempo" })).toBeNull();
  });

  it("shows a failed dry run", async () => {
    await ready(actions({ runSync: mock(async () => ({ ok: false, error: "no token" })) }));
    fireEvent.click(screen.getByRole("button", { name: "Send to Tempo, Wed 30 Sep" }));
    expect(await screen.findByText("no token")).toBeTruthy();
  });
});
