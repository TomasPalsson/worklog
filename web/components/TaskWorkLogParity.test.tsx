// Work log parity with the day page: Regenerate text with AI, Merge blocks (with a confirm step), Move a block.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent } from "@testing-library/react";
import { subscribe, type ToastMsg } from "@/lib/toast";

const regenerate = mock(async (_k: { day: string; jira_issue: string }) => ({ ok: true as const, data: {} }));
const merge = mock(async (_p: number, _a: number[], _d: string) => ({ ok: true as const, data: undefined }));
const assign = mock(async (_id: number, _key: string | null, _day: string) => ({ ok: true as const, data: undefined }));
mock.module("@/app/actions-tempo-lines", () => ({
  regenerateTempoLineText: regenerate,
  saveTempoLineHours: mock(async () => ({ ok: true as const, data: {} })),
  saveTempoLineText: mock(async () => ({ ok: true as const, data: {} })),
}));
// Bun's mock.module is process-wide: export every name the other test files' trees import from here.
mock.module("@/app/actions", () => ({
  mergeGroup: merge,
  assignTicket: assign,
  assignExternalTicket: mock(async () => ({ ok: true as const, data: undefined })),
  searchJiraTickets: mock(async () => ({ ok: true as const, data: [] })),
  setDuration: mock(async () => ({ ok: true as const, data: undefined })),
  setDescription: mock(async () => ({ ok: true as const, data: undefined })),
  setPersonal: mock(async () => ({ ok: true as const, data: undefined })),
  setIgnored: mock(async () => ({ ok: true as const, data: undefined })),
  deleteBlock: mock(async () => ({ ok: true as const, data: undefined })),
  describeBlock: mock(async () => ({ ok: true as const, data: { minutes: 0, jira_issue: null } })),
  fetchBlockEvents: mock(async () => ({ ok: true as const, data: [] })),
  fetchBlockCommits: mock(async () => ({ ok: true as const, data: [] })),
  createTicket: mock(async () => ({ ok: true as const, data: undefined })),
  fetchAccounts: mock(async () => ({ ok: true as const, data: [] })),
  fetchProjects: mock(async () => ({ ok: true as const, data: [] })),
  saveBillingFolder: mock(async () => ({ ok: true as const, data: undefined })),
}));

let kit: typeof import("./workLogTestKit");
beforeAll(async () => {
  kit = await import("./workLogTestKit");
});

let toasts: ToastMsg[] = [];
subscribe((q) => (toasts = q));
afterEach(() => {
  cleanup();
  for (const m of [regenerate, merge, assign]) m.mockClear();
  merge.mockImplementation(async () => ({ ok: true as const, data: undefined }));
});

const twoBlocks = () => [
  kit.block({ id: 7, started_at: "2026-10-01T13:00:00Z", ended_at: "2026-10-01T14:00:00Z" }),
  kit.block({ id: 5, started_at: "2026-10-01T09:00:00Z", ended_at: "2026-10-01T10:00:00Z" }),
  kit.block({ id: 6, started_at: "2026-10-01T11:00:00Z", ended_at: "2026-10-01T12:00:00Z" }),
];
const texts = () => toasts.map((t) => t.text);

describe("Regenerate text with AI", () => {
  it("writes the day's text again, toasts, and reloads the blocks", async () => {
    const { a } = await kit.open();
    kit.pick("Regenerate text with AI");
    expect(regenerate.mock.calls[0][0]).toEqual({ day: "2026-10-01", jira_issue: "ABC-1" });
    await kit.settle();
    expect(texts()).toContain("New Tempo text written");
    expect(kit.calls(a.loadTicketBlocks).length).toBe(2);
  });

  it("says Writing… while it runs and toasts the daemon's message on failure", async () => {
    let release: (v: { ok: false; error: string }) => void = () => {};
    regenerate.mockImplementationOnce(() => new Promise((r) => (release = r as typeof release)) as never);
    await kit.open();
    kit.pick("Regenerate text with AI");
    await kit.settle();
    expect(document.querySelector(".task-day-plain")?.textContent).toBe("Writing…");
    await act(async () => release({ ok: false, error: "model offline" }));
    expect(document.querySelector(".task-day-plain")).toBeNull();
    expect(texts()).toContain("Couldn't write new text — model offline");
  });

  it("is disabled with a reason when the day has no Tempo line", async () => {
    await kit.open([kit.day({ line_seconds: 0, line_text: "", tracked_seconds: 0 })]);
    fireEvent.click(kit.more());
    const item = kit.btn("Regenerate text with AI");
    expect(item.disabled).toBe(true);
    expect(item.title).toBe("There is no Tempo text for this day yet");
  });
});

describe("a rejected call never leaves the day busy", () => {
  it("regenerate: toasts that the service is unreachable and clears Writing…", async () => {
    regenerate.mockImplementationOnce(async () => {
      throw new Error("fetch failed");
    });
    await kit.open();
    kit.pick("Regenerate text with AI");
    await kit.settle();
    expect(texts()).toContain("Couldn't reach the worklog service");
    expect(document.querySelector(".task-day-plain")).toBeNull();
  });

  it("merge: the same", async () => {
    merge.mockImplementationOnce(async () => {
      throw new Error("fetch failed");
    });
    await kit.open([kit.day({ blocks: twoBlocks() })]);
    kit.pick("Merge 3 blocks into one");
    fireEvent.click(kit.btn("Merge"));
    await kit.settle();
    expect(texts()).toContain("Couldn't reach the worklog service");
    expect(document.querySelector(".task-day-plain")).toBeNull();
  });
});

describe("Merge blocks into one", () => {
  it("is offered only with two or more blocks", async () => {
    await kit.open();
    fireEvent.click(kit.more());
    expect(kit.btns(/^Merge/).length).toBe(0);
    cleanup();
    await kit.open([kit.day({ blocks: twoBlocks() })]);
    fireEvent.click(kit.more());
    expect(kit.btns("Merge 3 blocks into one").length).toBe(1);
  });

  it("asks first, then merges into the earliest block", async () => {
    const { a } = await kit.open([kit.day({ blocks: twoBlocks() })]);
    kit.pick("Merge 3 blocks into one");
    expect(merge.mock.calls.length).toBe(0);
    expect(document.querySelector('[role="group"]')?.textContent).toContain("Merge 3 blocks?");
    fireEvent.click(kit.btn("Merge"));
    expect(merge.mock.calls[0]).toEqual([5, [6, 7], "2026-10-01"]);
    await kit.settle();
    expect(texts()).toContain("Merged 3 blocks");
    expect(kit.calls(a.loadTicketBlocks).length).toBe(2);
  });

  it("Cancel backs out without merging", async () => {
    await kit.open([kit.day({ blocks: twoBlocks() })]);
    kit.pick("Merge 3 blocks into one");
    fireEvent.click(kit.btn("Cancel"));
    expect(merge.mock.calls.length).toBe(0);
    expect(kit.btns("Merge 3 blocks into one").length).toBe(1);
  });

  it("Cancel puts focus back on the Merge item", async () => {
    await kit.open([kit.day({ blocks: twoBlocks() })]);
    kit.pick("Merge 3 blocks into one");
    fireEvent.click(kit.btn("Cancel"));
    expect(document.activeElement).toBe(kit.btn("Merge 3 blocks into one"));
  });

  it("toasts the daemon's message when the merge fails", async () => {
    merge.mockImplementationOnce(async () => ({ ok: false as const, error: "block is synced" }) as never);
    await kit.open([kit.day({ blocks: twoBlocks() })]);
    kit.pick("Merge 3 blocks into one");
    fireEvent.click(kit.btn("Merge"));
    await kit.settle();
    expect(texts()).toContain("Merge failed — block is synced");
  });
});

describe("Move a block", () => {
  const MOVE = "Move this block to another ticket";

  it("opens the picker inline with the block's ticket, and Esc closes only the picker", async () => {
    await kit.open();
    fireEvent.click(kit.btn(MOVE));
    expect(document.querySelector(".task-block-picker .combobox")).not.toBeNull();
    expect(document.querySelector(".combobox-popover")).not.toBeNull();
    const input = document.querySelector<HTMLInputElement>(".combobox-search input")!;
    // The combobox claims the first Esc (its list closes) ...
    expect(fireEvent.keyDown(input, { key: "Escape" })).toBe(false);
    expect(document.querySelector(".task-block-picker")).not.toBeNull();
    // ... and the next one closes the picker; neither reaches the dialog.
    const chip = document.querySelector<HTMLElement>(".ticket-chip")!;
    expect(fireEvent.keyDown(chip, { key: "Escape" })).toBe(false);
    expect(document.querySelector(".task-block-picker")).toBeNull();
    expect(document.activeElement).toBe(kit.btn(MOVE));
  });

  it("reloads the blocks once the assign finishes", async () => {
    const { a } = await kit.open();
    fireEvent.click(kit.btn(MOVE));
    await kit.settle();
    const before = kit.calls(a.loadTicketBlocks).length;
    await act(async () => {
      fireEvent.click(document.querySelector(".combobox-clear")!);
    });
    await kit.settle();
    expect(assign.mock.calls[0]).toEqual([1, null, "2026-10-01"]);
    expect(kit.calls(a.loadTicketBlocks).length).toBe(before + 1);
  });

  const other = { key: "ABC-2", summary: "Spike cache", status: "To Do", updated: null };
  const option = () => [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((o) => o.textContent?.includes("ABC-2"))!;

  it("opens straight onto the list of the board's tickets", async () => {
    await kit.open([kit.day()], {}, [other]);
    fireEvent.click(kit.btn(MOVE));
    expect(option()).toBeTruthy();
  });

  it("on success reloads, says Moved to KEY (live region and toast) and focuses the day's toggle", async () => {
    const { a, onAnnounce } = await kit.open([kit.day()], {}, [other]);
    fireEvent.click(kit.btn(MOVE));
    const before = kit.calls(a.loadTicketBlocks).length;
    await act(async () => void fireEvent.click(option()));
    await kit.settle();
    expect(assign.mock.calls.at(-1)).toEqual([1, "ABC-2", "2026-10-01"]);
    expect(kit.calls(a.loadTicketBlocks).length).toBe(before + 1);
    expect(onAnnounce.mock.calls.at(-1)).toEqual(["Moved to ABC-2"]);
    expect(texts()).toContain("Moved to ABC-2");
    expect(document.querySelector(".task-block-picker")).toBeNull();
    expect(document.activeElement).toBe(kit.toggle());
  });

  it("on failure keeps the picker open, and neither reloads nor announces", async () => {
    assign.mockImplementationOnce(async () => ({ ok: false as const, error: "nope" }) as never);
    const { a, onAnnounce } = await kit.open([kit.day()], {}, [other]);
    fireEvent.click(kit.btn(MOVE));
    const before = kit.calls(a.loadTicketBlocks).length;
    await act(async () => void fireEvent.click(option()));
    await kit.settle();
    expect(texts()).toContain("Assign ticket failed — nope");
    expect(document.querySelector(".task-block-picker")).not.toBeNull();
    expect(kit.calls(a.loadTicketBlocks).length).toBe(before);
    expect(onAnnounce.mock.calls.length).toBe(0);
  });

  it("keeps the link to the block page", async () => {
    await kit.open();
    expect(document.querySelector('a.task-block-row')?.getAttribute("href")).toBe("/2026-10-01/block/1");
  });
});

