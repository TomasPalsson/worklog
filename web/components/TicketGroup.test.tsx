// An assigned ticket group shows its Tempo line text and hours, and lets the
// Owner edit, regenerate or override them. The unassigned group has no controls.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BlockGroup } from "@/app/[day]/page";
import type { TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";
import { dismiss, subscribe } from "@/lib/toast";
import { ToastHost } from "./ToastHost";

const okVoid = () => mock(async () => ({ ok: true as const, data: undefined }));
const mergeGroup = mock(async (..._a: unknown[]): Promise<{ ok: true; data: undefined } | { ok: false; error: string }> => ({
  ok: true,
  data: undefined,
}));
const undoLastChange = mock(async (_day: string) => ({
  ok: true as const,
  data: { outcome: "restored" as const, change: "merge" as const, block_ids: [1] },
}));
const okList = () => mock(async () => ({ ok: true as const, data: [] }));
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
  usePathname: () => "/",
}));
// Process-wide like every mock.module: export the names every component tree
// that shares this specifier imports, so test order must not matter.
mock.module("@/app/actions", () => ({
  mergeGroup,
  undoLastChange,
  saveBillingFolder: okVoid(),
  setDuration: okVoid(),
  setDescription: okVoid(),
  setPersonal: okVoid(),
  deleteBlock: okVoid(),
  setIgnored: okVoid(),
  describeBlock: mock(async () => ({ ok: true as const, data: { minutes: 0, jira_issue: null } })),
  fetchBlockEvents: okList(),
  fetchBlockCommits: okList(),
  assignTicket: okVoid(),
  assignExternalTicket: okVoid(),
  searchJiraTickets: okList(),
  createTicket: okVoid(),
  fetchAccounts: okList(),
  fetchProjects: okList(),
}));

function line(overrides: Partial<TempoLine> = {}): TempoLine {
  return {
    day: "2026-09-25",
    jira_issue: "PROJ-1",
    text: "Wrote the importer",
    text_origin: "generated",
    fallback_text: "Work on PROJ-1",
    union_seconds: 5400,
    hours_override_seconds: null,
    effective_seconds: 5400,
    billing: null,
    ...overrides,
  };
}

function group(overrides: Partial<BlockGroup> = {}): BlockGroup {
  return {
    key: "PROJ-1",
    label: "PROJ-1",
    unassigned: false,
    blocks: [],
    totalSeconds: 5400,
    syncState: "unsynced",
    previewDescription: "joined preview",
    defaultOpen: true,
    ...overrides,
  };
}

type HoursResult = { ok: true; data: TempoLine } | { ok: false; error: string };

const key: TempoLineKey = { day: "2026-09-25", jira_issue: "PROJ-1" };
const saveText = mock(async (_key: TempoLineKey, _text: string) => ({
  ok: true as const,
  data: line(),
}));
const saveHours = mock(
  async (_key: TempoLineKey, _seconds: number | null): Promise<HoursResult> => ({
    ok: true,
    data: line(),
  }),
);
const regenerate = mock(async (_key: TempoLineKey) => ({ ok: true as const, data: line() }));

type Progress = import("@/lib/types").DayProgress;
type ProgressRes = { ok: true; data: Progress } | { ok: false; error: string };
const loadDayProgress = mock(async (_day: string, _refresh?: string): Promise<ProgressRes> => ({
  ok: false,
  error: "unset",
}));
// Mock only the daemon boundary: bun's mock.module is process-wide, so
// replacing @/app/actions-progress would leak into every later test file.
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: async (_method: string, path: string) => {
    const m = path.match(/^\/progress\/([^?]+)(?:\?refresh=(.*))?$/);
    if (!m) throw new Error(`unexpected daemon call ${path}`);
    const r = await loadDayProgress(m[1], m[2] === undefined ? undefined : decodeURIComponent(m[2]));
    if (!r.ok) throw new Error(r.error);
    return r.data;
  },
}));

let DayProgressProvider: (props: { day: string; children: React.ReactNode }) => React.JSX.Element;
let TicketGroup: typeof import("./TicketGroup").TicketGroup;

beforeAll(async () => {
  TicketGroup = (await import("./TicketGroup")).TicketGroup;
  DayProgressProvider = (await import("./DayProgressProvider")).DayProgressProvider;
});

afterEach(() => {
  cleanup();
  // The toast queue is module-global; leaving "Regenerated" in it breaks
  // other files' exact-text lookups.
  subscribe((queued) => queued.forEach((msg) => dismiss(msg.id)))();
  saveText.mockClear();
  saveHours.mockClear();
  regenerate.mockClear();
  mergeGroup.mockClear();
  undoLastChange.mockClear();
});

function renderGroup(blockGroup: BlockGroup, tempoLine?: TempoLine) {
  return render(
    <>
      <TicketGroup
        group={blockGroup}
        day="2026-09-25"
        line={tempoLine}
        saveText={saveText}
        saveHours={saveHours}
        regenerate={regenerate}
      >
        {null}
      </TicketGroup>
      <ToastHost />
    </>,
  );
}

describe("TicketGroup line text", () => {
  it("shows the stored text", () => {
    renderGroup(group(), line());
    expect(screen.getByText("Wrote the importer")).toBeTruthy();
  });

  it("shows the fallback when nothing is generated", () => {
    renderGroup(group(), line({ text: null, text_origin: null }));
    expect(screen.getByText("Work on PROJ-1")).toBeTruthy();
    expect(screen.getByText("not generated")).toBeTruthy();
  });

  it("saves edited text on Cmd-Enter", async () => {
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Edit/ }));
    const box = screen.getByLabelText("Edit line text for PROJ-1");
    fireEvent.change(box, { target: { value: "New words" } });
    fireEvent.keyDown(box, { key: "Enter", metaKey: true });
    await waitFor(() => expect(saveText).toHaveBeenCalledTimes(1));
    expect(saveText.mock.calls[0]).toEqual([key, "New words"]);
  });

  it("cancels the edit on Escape without saving", () => {
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.keyDown(screen.getByLabelText("Edit line text for PROJ-1"), { key: "Escape" });
    expect(screen.queryByLabelText("Edit line text for PROJ-1")).toBeNull();
    expect(saveText).not.toHaveBeenCalled();
  });

  it("sends empty text to reset to generated", async () => {
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Edit/ }));
    const box = screen.getByLabelText("Edit line text for PROJ-1");
    fireEvent.change(box, { target: { value: "" } });
    fireEvent.keyDown(box, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(saveText).toHaveBeenCalledTimes(1));
    expect(saveText.mock.calls[0]).toEqual([key, ""]);
  });

  it("regenerates the line", async () => {
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Regenerate/ }));
    await waitFor(() => expect(regenerate).toHaveBeenCalledTimes(1));
    expect(regenerate.mock.calls[0][0]).toEqual(key);
  });

  it("keeps a failed regenerate's reason on the card, and Try again retries", async () => {
    regenerate.mockImplementationOnce(async () => ({ ok: false, error: "model unreachable" }) as never);
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Regenerate/ }));
    const alert = await screen.findByText(/Couldn't write a new text — model unreachable/);
    expect(alert.closest('[role="alert"]')).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(regenerate).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText(/Couldn't write a new text/)).toBeNull());
  });

  it("returns focus to Edit text after the editor closes", () => {
    renderGroup(group(), line());
    fireEvent.click(screen.getByRole("button", { name: /Edit text/ }));
    fireEvent.keyDown(screen.getByLabelText("Edit line text for PROJ-1"), { key: "Escape" });
    expect(document.activeElement?.textContent).toMatch(/Edit text/);
  });

  it("drops the replace confirm when you start editing instead", () => {
    renderGroup(group(), line({ text_origin: "manual" }));
    fireEvent.click(screen.getByRole("button", { name: /Regenerate/ }));
    fireEvent.click(screen.getByRole("button", { name: /Edit text/ }));
    fireEvent.keyDown(screen.getByLabelText("Edit line text for PROJ-1"), { key: "Escape" });
    expect(screen.queryByRole("button", { name: /Replace your text/ })).toBeNull();
  });

  it("asks once more before regenerating over your own text", async () => {
    renderGroup(group(), line({ text_origin: "manual" }));
    fireEvent.click(screen.getByRole("button", { name: /Regenerate/ }));
    expect(regenerate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Replace your text/ }));
    await waitFor(() => expect(regenerate).toHaveBeenCalledTimes(1));
  });
});

describe("TicketGroup hours", () => {
  // The billed figure itself is the control: click it, type, Enter.
  function openHours() {
    fireEvent.click(screen.getByRole("button", { name: /Change billed hours for PROJ-1/ }));
    return screen.getByLabelText("Hours for PROJ-1") as HTMLInputElement;
  }

  it("shows the effective hours on the hours button", () => {
    renderGroup(group(), line({ effective_seconds: 7200, hours_override_seconds: 7200 }));
    expect(screen.getByRole("button", { name: /2\.0h billed/ })).toBeTruthy();
  });

  it("has no hours input until the figure is clicked", () => {
    renderGroup(group(), line());
    expect(screen.queryByLabelText("Hours for PROJ-1")).toBeNull();
  });

  it("opens prefilled with the billed hours", () => {
    renderGroup(group(), line());
    expect(openHours().value).toBe("1.5");
  });

  it("saves a half-hour override in seconds", async () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "2.5" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, 9000]);
  });

  it("does not save when the value is unchanged", () => {
    renderGroup(group(), line());
    fireEvent.keyDown(openHours(), { key: "Enter" });
    expect(saveHours).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Hours for PROJ-1")).toBeNull();
  });

  it("cancels on Escape without saving", () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "3" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(saveHours).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Hours for PROJ-1")).toBeNull();
  });

  it("clears the override when the box is emptied", async () => {
    renderGroup(group(), line({ hours_override_seconds: 3600, effective_seconds: 3600 }));
    const box = openHours();
    fireEvent.change(box, { target: { value: "" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, null]);
  });

  it("marks an override and names the tracked hours", () => {
    renderGroup(group(), line({ hours_override_seconds: 7200, effective_seconds: 7200 }));
    expect(screen.getByText("hours changed · 1.5h tracked")).toBeTruthy();
  });

  it("rejects a non-half-hour value inline without calling the action", () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "1.3" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const alert = screen.getByText("1.3h isn't a half-hour step — try 1.5").closest('[role="alert"]');
    expect(alert).toBeTruthy();
    expect(box.getAttribute("aria-invalid")).toBe("true");
    expect(saveHours).not.toHaveBeenCalled();
  });

  it("accepts a decimal comma", async () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "2,5" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, 9000]);
  });

  it("accepts a trailing h", async () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "2h" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, 7200]);
  });

  it("rejects non-numbers and more than a day", () => {
    renderGroup(group(), line());
    const box = openHours();
    for (const value of ["1e1", "0x10", "25"]) {
      fireEvent.change(box, { target: { value } });
      fireEvent.keyDown(box, { key: "Enter" });
      expect(box.getAttribute("aria-invalid")).toBe("true");
    }
    expect(saveHours).not.toHaveBeenCalled();
  });

  it("steps by half an hour with the arrow keys", () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.keyDown(box, { key: "ArrowUp" });
    expect(box.value).toBe("2");
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(box.value).toBe("1");
  });

  it("drops an invalid value on blur and says so", async () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "1.3" } });
    fireEvent.blur(box);
    expect(saveHours).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Hours for PROJ-1")).toBeNull();
    await waitFor(() => expect(screen.getByText(/Not saved — 1\.3h isn't a half-hour step/)).toBeTruthy());
  });

  it("steps from a typed value to the next half-hour boundary", () => {
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "1.3" } });
    fireEvent.keyDown(box, { key: "ArrowUp" });
    expect(box.value).toBe("1.5");
    fireEvent.change(box, { target: { value: "1.3" } });
    fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(box.value).toBe("1");
  });

  it("keeps the box focused after a rejected save so Esc still works", async () => {
    saveHours.mockImplementationOnce(async () => ({ ok: false, error: "hours must be positive" }));
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "2" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(screen.getByText("hours must be positive")).toBeTruthy());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Hours for PROJ-1")));
  });

  it("returns focus to the hours after Escape", () => {
    renderGroup(group(), line());
    fireEvent.keyDown(openHours(), { key: "Escape" });
    expect(document.activeElement?.getAttribute("aria-label")).toMatch(/Change billed hours for PROJ-1/);
  });

  it("shows a daemon rejection inline", async () => {
    saveHours.mockImplementationOnce(async () => ({ ok: false, error: "hours must be positive" }));
    renderGroup(group(), line());
    const box = openHours();
    fireEvent.change(box, { target: { value: "1" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(screen.getByText("hours must be positive")).toBeTruthy());
  });
});

describe("TicketGroup without a line", () => {
  it("renders no controls for the unassigned group", () => {
    renderGroup(group({ key: "__unassigned__", label: "Unassigned", unassigned: true }));
    expect(screen.queryByRole("button", { name: /Edit/ })).toBeNull();
    expect(screen.queryByLabelText(/Hours for/)).toBeNull();
  });

  it("falls back to the preview description when no line is loaded", () => {
    renderGroup(group());
    expect(screen.getByText("joined preview")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Regenerate/ })).toBeNull();
  });
});

describe("TicketGroup merge confirmation offers Undo (FR-29)", () => {
  const twoBlocks = () =>
    group({
      blocks: [
        { id: 2, started_at: "2026-09-25T10:00:00Z" },
        { id: 1, started_at: "2026-09-25T09:00:00Z" },
      ] as BlockGroup["blocks"],
    });
  const queued = () => {
    let q: import("@/lib/toast").ToastMsg[] = [];
    subscribe((m) => (q = m))();
    return q;
  };

  it("Merged toast carries Undo, which calls undoLastChange(day)", async () => {
    renderGroup(twoBlocks(), line());
    fireEvent.click(screen.getByRole("button", { name: "merge all blocks on PROJ-1" }));
    await waitFor(() => expect(queued().some((t) => t.action?.label === "Undo")).toBe(true));
    queued().find((t) => t.action?.label === "Undo")!.action!.onClick();
    await waitFor(() => expect(undoLastChange).toHaveBeenCalledWith("2026-09-25"));
  });

  it("a failed merge shows an error and no Undo", async () => {
    mergeGroup.mockImplementationOnce(async () => ({ ok: false, error: "refused" }));
    renderGroup(twoBlocks(), line());
    fireEvent.click(screen.getByRole("button", { name: "merge all blocks on PROJ-1" }));
    await waitFor(() => expect(queued().some((t) => t.tone === "error")).toBe(true));
    expect(queued().some((t) => t.action?.label === "Undo")).toBe(false);
  });
});

describe("needs-a-look badge", () => {
  const flagged = (
    origin: TempoLine["text_origin"],
    check: "passed" | "needs_look" | null | undefined,
  ) => ({ ...line({ text_origin: origin }), check_status: check }) as TempoLine;

  it("shows for needs_look on generated text", () => {
    renderGroup(group(), flagged("generated", "needs_look")); // catches: badge condition dropped
    expect(screen.getByText("needs a look")).toBeTruthy();
  });

  it("is hidden for passed", () => {
    renderGroup(group(), flagged("generated", "passed")); // catches: showing on any check_status
    expect(screen.queryByText("needs a look")).toBeNull();
  });

  it("is hidden for null and missing check_status", () => {
    renderGroup(group(), flagged("generated", null)); // catches: inverted condition
    expect(screen.queryByText("needs a look")).toBeNull();
    cleanup();
    renderGroup(group(), flagged("generated", undefined));
    expect(screen.queryByText("needs a look")).toBeNull();
  });

  it("is hidden when the text is the Owner's own", () => {
    renderGroup(group(), flagged("manual", "needs_look")); // catches: generated guard removed
    expect(screen.queryByText("needs a look")).toBeNull();
  });
});

describe("needs-a-look reason", () => {
  it("shows the reason as visible text the badge is described by", () => {
    renderGroup(group(), { ...line({ text_origin: "generated" }), check_status: "needs_look" } as TempoLine);
    const hint = screen.getByText("Vague after one rewrite — edit before it is sent");
    expect(hint.className).toContain("billing-text-hint"); // catches title-only
    const badge = screen.getByText("needs a look");
    expect(badge.getAttribute("aria-describedby")).toBe(hint.id);
    expect(hint.id).not.toBe(""); // catches describedby pointing at nothing
  });

  it("shows no reason when the badge is hidden", () => {
    renderGroup(group(), { ...line({ text_origin: "generated" }), check_status: "passed" } as TempoLine);
    expect(screen.queryByText("Vague after one rewrite — edit before it is sent")).toBeNull();
  });
});

describe("TicketGroup estimate bar wiring", () => {
  const DAY = "2026-09-25";
  const H = 3600;
  type B = import("@/lib/types").Block;
  const ticket = (o: Partial<import("@/lib/types").TicketProgress> = {}) => ({
    key: "PROJ-1",
    estimate_seconds: 4 * H,
    people: [{ account_id: "me", name: "Tomas P", is_you: true, seconds: 2 * H, by_day: [[DAY, 2 * H]] as Array<[string, number]> }],
    logged_seconds: 2 * H,
    pulled_at: "2026-07-25T09:42:00Z",
    error: null,
    ...o,
  });
  const loaded = (...tickets: ReturnType<typeof ticket>[]) =>
    loadDayProgress.mockImplementation(async () => ({ ok: true, data: { day: DAY, tickets } }));
  const blk = (o: Partial<B> = {}) =>
    ({ id: 1, duration_seconds: 1800, tempo_worklog_id: null, is_personal: false, jira_issue: "PROJ-1", ...o }) as B;
  const withBar = (blocks: B[], g: Partial<BlockGroup> = {}) =>
    render(
      <DayProgressProvider day={DAY}>
        <TicketGroup group={group({ blocks, ...g })} day={DAY}>
          {null}
        </TicketGroup>
      </DayProgressProvider>,
    );

  afterEach(() => loadDayProgress.mockClear());

  it("B8: shows 'Loading hours from Jira…' while the call is unresolved (catches awaiting before render)", () => {
    loadDayProgress.mockImplementation(() => new Promise(() => {}));
    withBar([blk()]);
    expect(screen.getByText("Loading hours from Jira…")).toBeTruthy();
  });

  it("asks the daemon for the day with no forced refresh, once (catches a fetch per group)", async () => {
    loaded(ticket());
    withBar([blk()]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(loadDayProgress).toHaveBeenCalledTimes(1);
    expect(loadDayProgress).toHaveBeenCalledWith(DAY, undefined);
  });

  it("an unsynced block adds its seconds on top of the logged ones (catches pending = 0)", async () => {
    loaded(ticket());
    withBar([blk()]);
    await waitFor(() => expect(screen.getByText("once synced")).toBeTruthy());
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(2.5 * H));
  });

  it("a synced block adds nothing: its time is already in Tempo (catches pending = duration always)", async () => {
    loaded(ticket());
    withBar([blk({ tempo_worklog_id: "77" })]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(2 * H));
    expect(screen.queryByText("once synced")).toBeNull();
  });

  it("FR-15a: a 1970-01-01 pulled_at reads as unknown, never as a clock time", async () => {
    loaded(ticket({ pulled_at: "1970-01-01T00:00:00+00:00" }));
    withBar([blk()]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(screen.queryByText(/Jira numbers from/)).toBeNull();
  });

  it("a real pulled_at still shows its time (catches hiding the line for every ticket)", async () => {
    loaded(ticket());
    withBar([blk()]);
    await waitFor(() => expect(screen.getByText(/Jira numbers from \d\d:\d\d/)).toBeTruthy());
  });

  it("a failed call shows the error with Try again, and Try again asks again for the whole day (catches swallowing the error)", async () => {
    loadDayProgress.mockImplementation(async () => ({ ok: false, error: "boom" }));
    withBar([blk()]);
    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
    expect(screen.getByRole("alert").textContent).toContain("PROJ-1");
    loaded(ticket());
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(loadDayProgress).toHaveBeenLastCalledWith(DAY, undefined);
  });

  it("a per-ticket retry forces that ticket and keeps the others", async () => {
    loaded(ticket({ error: "jira_unavailable" }), ticket({ key: "PROJ-2" }));
    withBar([blk()]);
    await waitFor(() => expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy());
    loaded(ticket());
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(loadDayProgress).toHaveBeenLastCalledWith(DAY, "PROJ-1"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Try again" })).toBeNull());
  });

  it("renders no bar for the unassigned group (catches showing it for every group)", () => {
    loadDayProgress.mockImplementation(() => new Promise(() => {}));
    withBar([blk({ jira_issue: null })], { key: "__unassigned__", label: "Unassigned", unassigned: true });
    expect(screen.queryByText("Loading hours from Jira…")).toBeNull();
  });

  it("renders no bar outside the day provider (catches a hard dependency on the context)", () => {
    render(
      <TicketGroup group={group({ blocks: [blk()] })} day={DAY}>
        {null}
      </TicketGroup>,
    );
    expect(screen.queryByText("Loading hours from Jira…")).toBeNull();
  });

  it("renders nothing once loaded when the ticket is not in the answer", async () => {
    loaded(ticket({ key: "OTHER-9" }));
    withBar([blk()]);
    await waitFor(() => expect(loadDayProgress).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByText("Loading hours from Jira…")).toBeNull());
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("sums unsynced blocks: 30m + 45m shows +1h 15m once and adds 4500s (catches counting only the first block)", async () => {
    loaded(ticket());
    withBar([blk({ id: 1, duration_seconds: 1800 }), blk({ id: 2, duration_seconds: 2700 })]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(screen.getAllByText("+1h 15m")).toHaveLength(1);
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(2 * H + 4500));
  });

  const withLine = (blocks: B[], g: Partial<BlockGroup>, l: Partial<TempoLine>) =>
    render(
      <DayProgressProvider day={DAY}>
        <TicketGroup group={group({ blocks, ...g })} day={DAY} line={line({ day: DAY, ...l })}>
          {null}
        </TicketGroup>
      </DayProgressProvider>,
    );

  it("unsynced with a line counts the line's billed hours, not the raw blocks", async () => {
    loaded(ticket({ estimate_seconds: H, people: [], logged_seconds: 0 }));
    withLine([blk({ id: 1, duration_seconds: 2100 }), blk({ id: 2, duration_seconds: 900 })], {}, { effective_seconds: H });
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    // catches summing raw blocks (50m instead of the 1h that gets sent)
    expect(screen.getByText("+1h")).toBeTruthy();
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(H));
    expect(screen.getByText("once synced").parentElement?.textContent).toBe("1h of 1h once synced");
  });

  it("a synced group with a line adds nothing", async () => {
    loaded(ticket({ estimate_seconds: H, people: [], logged_seconds: 0 }));
    withLine([blk({ tempo_worklog_id: "77" })], { syncState: "synced" }, { effective_seconds: H });
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    // catches adding the line hours again though they are already in Tempo
    expect(screen.queryByText(/once synced/)).toBeNull();
    expect(screen.queryByText(/Not in Tempo yet/)).toBeNull();
  });

  it("a mixed group counts only the line hours not yet in Tempo today", async () => {
    loaded(
      ticket({
        estimate_seconds: 8 * H,
        people: [{ account_id: "me", name: "Tomas P", is_you: true, seconds: H, by_day: [[DAY, H]] }],
        logged_seconds: H,
      }),
    );
    withLine([blk()], { syncState: "mixed" }, { effective_seconds: 2 * H });
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    // catches using the full line (+2h) instead of line minus what is already in Tempo
    expect(screen.getByText("+1h")).toBeTruthy();
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(2 * H));
  });

  it("three blocks render exactly one meter (catches a bar per block)", async () => {
    loaded(ticket());
    withBar([blk({ id: 1 }), blk({ id: 2 }), blk({ id: 3 })]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(screen.getAllByRole("meter")).toHaveLength(1);
  });

  it("personal blocks add no pending time (catches summing every block)", async () => {
    loaded(ticket());
    withBar([blk({ id: 1 }), blk({ id: 2, duration_seconds: 2700, is_personal: true })]);
    await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
    expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe(String(2.5 * H));
  });

  describe("collapsed-row badge", () => {
    const badge = () => document.querySelector(".ep-badge");
    const at = (seconds: number) => ticket({
      logged_seconds: seconds,
      people: [{ account_id: "me", name: "Tomas P", is_you: true, seconds, by_day: [[DAY, seconds]] as Array<[string, number]> }],
    });

    it("on track shows no badge (catches a badge on every group)", async () => {
      loaded(ticket()); // 2h logged + 30m pending of 4h = 62%
      withBar([blk()]);
      await waitFor(() => expect(screen.getByRole("meter")).toBeTruthy());
      expect(badge()).toBeNull();
    });

    it("running low says what is left, counting unsynced time (catches using logged only)", async () => {
      loaded(at(3 * H)); // 3h + 30m pending = 3h 30m of 4h = 87.5%
      withBar([blk()]);
      await waitFor(() => expect(badge()).not.toBeNull());
      expect(badge()!.getAttribute("data-tone")).toBe("low");
      expect(badge()!.textContent).toBe("30m left"); // catches "1h left" from logged-only maths
    });

    it("over says by how much and the ring carries the over dot (catches clamping at the estimate)", async () => {
      loaded(at(4 * H)); // 4h + 1h 30m pending of 4h
      withBar([blk({ duration_seconds: 5400 })]);
      await waitFor(() => expect(badge()).not.toBeNull());
      expect(badge()!.getAttribute("data-tone")).toBe("over");
      expect(badge()!.textContent).toBe("1h 30m over");
      expect(badge()!.querySelectorAll("circle")).toHaveLength(3); // track + arc + dot; 2 means no over mark
    });

    it("shows no badge while loading or when the numbers failed (catches a badge from stale or missing data)", async () => {
      loadDayProgress.mockImplementation(() => new Promise(() => {}));
      const { unmount } = withBar([blk()]);
      expect(badge()).toBeNull();
      unmount();
      loaded({ ...at(4 * H), error: "jira_unavailable" });
      withBar([blk({ duration_seconds: 5400 })]);
      await waitFor(() => expect(screen.getByText(/couldn't refresh/i)).toBeTruthy());
      expect(badge()).toBeNull();
    });
  });
});
