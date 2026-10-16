import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { dismiss, subscribe, type ToastMsg } from "@/lib/toast";
import { registryHook } from "./billingRegistryTestHook";
import type { Block } from "@/lib/types";

type Res = { ok: true; data: undefined } | { ok: false; error: string };
const okRes = async (): Promise<Res> => ({ ok: true, data: undefined });
const setPersonal = mock(okRes);
const setIgnored = mock(okRes);
const deleteBlock = mock(okRes);
const mergeGroup = mock(okRes);
const describeBlock = mock(async () => ({ ok: true as const, data: { minutes: 20, jira_issue: null } }));
const undoLastChange = mock(async () => ({
  ok: true as const,
  data: { outcome: "restored" as const, change: "deleted" as const, block_ids: [1] },
}));
mock.module("@/app/actions", () => ({
  setDuration: mock(okRes),
  setDescription: mock(okRes),
  setPersonal,
  setIgnored,
  undoLastChange,
  deleteBlock,
  describeBlock,
  fetchBlockEvents: mock(async () => ({ ok: true as const, data: [] })),
  fetchBlockCommits: mock(async () => ({ ok: true as const, data: [] })),
  assignTicket: mock(okRes),
  assignExternalTicket: mock(okRes),
  searchJiraTickets: mock(async () => ({ ok: true as const, data: [] })),
  createTicket: mock(okRes),
  fetchAccounts: mock(async () => ({ ok: true as const, data: [] })),
  fetchProjects: mock(async () => ({ ok: true as const, data: [] })),
  saveBillingFolder: mock(okRes),
  fetchBillingRegistry: () => registryHook.fetch(),
  mergeGroup,
}));
mock.module("@/app/actions-note-block", () => ({
  regenerateNoteAction: mock(async () => ({ ok: true as const, data: { started: true } })),
  noteStatusAction: mock(async () => ({ ok: true as const, data: { state: "running" as const } })),
  addNoteBlock: mock(async () => ({ ok: false as const, error: "unused" })),
}));
const redescribeBlock = mock(async (_id: number, _day: string, _instruction: string) => ({
  ok: true as const,
  data: { block_id: 1, description: "Fixed the login bug" },
}));
type RedescribeReply = Awaited<ReturnType<typeof redescribeBlock>> | { ok: false; error: string };
const splitBlock = mock(async (_id: number, _day: string, _minutes: number) => ({
  ok: true as const,
  data: {},
}));
mock.module("@/app/actions-block-menu", () => ({ redescribeBlock, splitBlock }));
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
  usePathname: () => "/",
}));

let BlockCard: typeof import("./BlockCard").BlockCard;
beforeAll(async () => {
  BlockCard = (await import("./BlockCard")).BlockCard;
});

afterEach(() => {
  cleanup();
  subscribe((queued) => queued.forEach((msg) => dismiss(msg.id)))();
  for (const m of [setPersonal, setIgnored, deleteBlock, mergeGroup, describeBlock, undoLastChange, redescribeBlock, splitBlock]) m.mockClear();
  deleteBlock.mockImplementation(okRes);
});

const DAY = "2026-07-25";
function makeBlock(overrides: Partial<Block> = {}): Block {
  return {
    id: 1,
    day: DAY,
    jira_issue: "PROJ-1",
    started_at: `${DAY}T10:00:00Z`,
    ended_at: `${DAY}T10:30:00Z`,
    duration_seconds: 1800,
    description: "worked on stuff",
    estimated_by: "ai",
    tempo_worklog_id: null,
    is_personal: false,
    ignored_at: null,
    dirty: false,
    event_count: 3,
    sources: [],
    project_path: null,
    confidence: "high",
    ...overrides,
  } as Block;
}

const card = () => screen.getByRole("article");
const openMenu = (block: Block = makeBlock(), dayBlocks: Block[] = []) => {
  render(<BlockCard block={block} tickets={[]} day={DAY} dayBlocks={dayBlocks} />);
  fireEvent.contextMenu(card(), { clientX: 40, clientY: 60 });
};
const item = (name: string) => screen.getByRole("menuitem", { name }) as HTMLButtonElement;
const toasts = () => {
  let q: ToastMsg[] = [];
  subscribe((m) => (q = m))();
  return q;
};

describe("BlockContextMenu opening (FR-08, FR-09)", () => {
  it("right-click opens a menu synchronously and cancels the browser menu (catches a missing preventDefault)", () => {
    render(<BlockCard block={makeBlock()} tickets={[]} day={DAY} />);
    const notCancelled = fireEvent.contextMenu(card());
    expect(notCancelled).toBe(false);
    expect(screen.getByRole("menu")).toBeTruthy();
  });

  it("opens at the pointer (catches a fixed position)", () => {
    openMenu();
    const style = (screen.getByRole("menu") as HTMLElement).style;
    expect([style.left, style.top]).toEqual(["40px", "60px"]);
  });

  it("is closed until asked (catches always-open)", () => {
    render(<BlockCard block={makeBlock()} tickets={[]} day={DAY} />);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("ContextMenu key on the card opens it", () => {
    render(<BlockCard block={makeBlock()} tickets={[]} day={DAY} />);
    fireEvent.keyDown(card(), { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toBeTruthy();
  });

  it("Shift+F10 opens it; plain F10 does not (catches ignoring the modifier)", () => {
    render(<BlockCard block={makeBlock()} tickets={[]} day={DAY} />);
    fireEvent.keyDown(card(), { key: "F10" });
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.keyDown(card(), { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeTruthy();
  });

  it("focus lands on the first enabled item (catches focus left on the card)", () => {
    openMenu();
    expect(document.activeElement).toBe(screen.getAllByRole("menuitem")[0]);
  });

  it("focus skips a disabled first item (catches focusing a disabled button)", () => {
    openMenu(makeBlock({ tempo_worklog_id: "TW-1" }));
    const first = screen.getAllByRole("menuitem").find((b) => !(b as HTMLButtonElement).disabled);
    expect(document.activeElement).toBe(first ?? null);
  });

  it("arrow keys move focus between items", () => {
    openMenu();
    const first = document.activeElement;
    fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
    expect(document.activeElement).not.toBe(first);
    expect(screen.getByRole("menu").contains(document.activeElement)).toBe(true);
  });

  it("Esc closes it and focus returns to the card (catches focus lost to body)", async () => {
    openMenu();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(document.activeElement).toBe(card());
  });

  it("an outside click closes it; a click inside the menu does not close it by itself", () => {
    openMenu();
    fireEvent.mouseDown(screen.getByRole("menu"));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("every item is a role=menuitem button (catches div items)", () => {
    openMenu();
    const items = screen.getAllByRole("menuitem");
    expect(items.map((i) => i.tagName)).toEqual(items.map(() => "BUTTON"));
    expect(items.length).toBe(10);
  });
});

describe("BlockContextMenu items mirror the card buttons (FR-10)", () => {
  it("Describe calls describeBlock and closes", async () => {
    openMenu();
    fireEvent.click(item("Describe with Claude"));
    await waitFor(() => expect(describeBlock).toHaveBeenCalledWith(1, DAY));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("Personal flips to true on a work block", async () => {
    openMenu();
    fireEvent.click(item("Mark personal"));
    await waitFor(() => expect(setPersonal).toHaveBeenCalledWith(1, true, DAY));
  });

  it("Work flips to false on a personal block (catches a one-way toggle)", async () => {
    openMenu(makeBlock({ is_personal: true }));
    fireEvent.click(item("Mark as work"));
    await waitFor(() => expect(setPersonal).toHaveBeenCalledWith(1, false, DAY));
  });

  it("Noise ignores with reason noise", async () => {
    openMenu();
    fireEvent.click(item("Noise"));
    await waitFor(() => expect(setIgnored).toHaveBeenCalledWith(1, true, DAY, "noise"));
  });

  it("Duplicate merges into the same-ticket neighbour, not ignoring", async () => {
    const twin = makeBlock({ id: 2, started_at: `${DAY}T11:00:00Z`, ended_at: `${DAY}T11:30:00Z` });
    openMenu(makeBlock(), [makeBlock(), twin]);
    fireEvent.click(item("Duplicate"));
    await waitFor(() => expect(mergeGroup).toHaveBeenCalledWith(2, [1], DAY));
    expect(setIgnored).not.toHaveBeenCalled();
  });

  it("Internal closes the menu and opens the ticket picker, ignoring nothing", async () => {
    openMenu();
    fireEvent.click(item("Internal"));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(setIgnored).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole("listbox")).toBeTruthy());
  });
});

describe("BlockContextMenu delete (FR-11)", () => {
  it("Delete calls deleteBlock and the toast offers Undo", async () => {
    openMenu();
    fireEvent.click(item("Delete"));
    await waitFor(() => expect(deleteBlock).toHaveBeenCalledWith(1, DAY));
    await waitFor(() => expect(toasts().some((t) => t.action?.label === "Undo")).toBe(true));
  });

  it("Undo on the toast runs the day's undo", async () => {
    openMenu();
    fireEvent.click(item("Delete"));
    await waitFor(() => expect(toasts().some((t) => t.action?.label === "Undo")).toBe(true));
    toasts().find((t) => t.action?.label === "Undo")!.action!.onClick();
    await waitFor(() => expect(undoLastChange).toHaveBeenCalledWith(DAY));
  });

  it("a refused delete shows an error toast and no Undo (catches a swallowed error)", async () => {
    deleteBlock.mockImplementation(async () => ({ ok: false, error: "nope" }));
    openMenu();
    fireEvent.click(item("Delete"));
    await waitFor(() => expect(toasts().some((t) => t.text.includes("nope"))).toBe(true));
    expect(toasts().some((t) => t.action?.label === "Undo")).toBe(false);
  });

  it.each([
    ["synced", { tempo_worklog_id: "TW-1" }],
    ["exported", { exported_at: "2026-07-26T10:00:00Z" }],
  ])("Delete is disabled with its reason on a %s block", (_n, o) => {
    openMenu(makeBlock(o));
    expect(item("Delete").disabled).toBe(true);
    expect(item("Delete").title).toBe("Synced or exported — can't delete");
  });

  it("Delete is enabled on an unsynced, unexported block (catches always-disabled)", () => {
    openMenu();
    expect(item("Delete").disabled).toBe(false);
  });
});

describe("BlockContextMenu synced guard (FR-14)", () => {
  it.each([
    ["synced", { tempo_worklog_id: "TW-1" }],
    ["exported", { exported_at: "2026-07-26T10:00:00Z" }],
  ])("disables all three Ignore items on a %s block but keeps Describe and Personal", (_n, o) => {
    openMenu(makeBlock(o));
    for (const name of ["Noise", "Duplicate", "Internal"]) {
      expect(item(name).disabled).toBe(true);
      expect(item(name).title).not.toBe("");
    }
    expect(item("Describe with Claude").disabled).toBe(false);
    expect(item("Mark personal").disabled).toBe(false);
  });

  it("a disabled Noise does nothing when clicked", () => {
    openMenu(makeBlock({ tempo_worklog_id: "TW-1" }));
    fireEvent.click(item("Noise"));
    expect(setIgnored).not.toHaveBeenCalled();
  });
});

describe("BlockContextMenu regenerate with prompt (FR-05, FR-07b)", () => {
  const REGENERATE = "Regenerate with prompt…";
  const openBox = (block: Block = makeBlock()) => {
    openMenu(block);
    fireEvent.click(item(REGENERATE));
  };
  const box = () => screen.getByLabelText("Instruction") as HTMLInputElement;
  const submit = () => screen.getByRole("button", { name: /^(Regenerate|Writing…)$/ }) as HTMLButtonElement;
  const type = (text: string) => fireEvent.change(box(), { target: { value: text } });
  let finish: (r: RedescribeReply) => void;
  const hold = () =>
    redescribeBlock.mockImplementationOnce(
      () => new Promise((resolve) => (finish = resolve as (r: RedescribeReply) => void)) as never,
    );

  it("picking the item swaps the list for an instruction box, menu still open (catches closing the menu)", () => {
    openBox();
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.queryAllByRole("menuitem").length).toBe(0);
    expect(document.activeElement).toBe(box());
  });

  it("the box is capped at 500 (catches a missing or wrong maxLength)", () => {
    openBox();
    expect(box().maxLength).toBe(500);
  });

  it("submit is disabled for blank and whitespace-only text (catches skipping the trim)", () => {
    openBox();
    expect(submit().disabled).toBe(true);
    type("   ");
    expect(submit().disabled).toBe(true);
    type("x");
    expect(submit().disabled).toBe(false);
  });

  it("500 Icelandic chars (1000 bytes) are accepted, 501 refused (catches byte counting and > for >=)", () => {
    openBox();
    type("þ".repeat(500));
    expect(submit().disabled).toBe(false);
    type("ðæ".repeat(250) + "þ");
    expect(submit().disabled).toBe(true);
  });

  it("posts the trimmed instruction for this block and day", async () => {
    openBox();
    type("  þetta var innskráningarvillan  ");
    fireEvent.click(submit());
    await waitFor(() => expect(redescribeBlock).toHaveBeenCalledWith(1, DAY, "þetta var innskráningarvillan"));
  });

  it("while writing: button reads Writing… and is disabled, the box stays (catches closing on submit)", async () => {
    hold();
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(submit().textContent).toBe("Writing…"));
    expect(submit().disabled).toBe(true);
    expect(screen.getByRole("menu")).toBeTruthy();
    finish({ ok: true, data: { block_id: 1, description: "x" } });
  });

  it("while writing, an outside click does not close the menu (catches the dismiss hook staying armed)", async () => {
    hold();
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(submit().textContent).toBe("Writing…"));
    fireEvent.mouseDown(document.body);
    expect(screen.getByRole("menu")).toBeTruthy();
    finish({ ok: true, data: { block_id: 1, description: "x" } });
  });

  it("while writing, Esc still closes it (catches ignoring Esc as well)", async () => {
    hold();
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(submit().textContent).toBe("Writing…"));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    finish({ ok: true, data: { block_id: 1, description: "x" } });
  });

  it("before writing, an outside click closes it as usual", () => {
    openBox();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("success closes the menu and toasts 'Description rewritten'", async () => {
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(toasts().some((t) => t.text === "Description rewritten")).toBe(true);
  });

  it("failure toasts the reason, keeps the box open with the text kept and re-enabled (catches a swallowed error)", async () => {
    redescribeBlock.mockImplementationOnce(async () => ({ ok: false, error: "model timed out" }) as never);
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(toasts().some((t) => t.text === "Rewrite failed — model timed out")).toBe(true));
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(box().value).toBe("login bug");
    expect(submit().disabled).toBe(false);
  });

  it.each([
    ["personal", { is_personal: true }, "personal blocks are not described"],
    ["note", { rough_note: "fixed it" }, "note blocks are rewritten from their note"],
    ["undescribed", { description: null }, "block has no description yet — describe it first"],
    ["blank-description", { description: "" }, "block has no description yet — describe it first"],
  ])("is disabled on a %s block with its reason as the tooltip", (_n, o, reason) => {
    openMenu(makeBlock(o as Partial<Block>));
    expect(item(REGENERATE).disabled).toBe(true);
    expect(item(REGENERATE).title).toBe(reason);
  });

  it("stays enabled on a synced or exported block (FR-14)", () => {
    openMenu(makeBlock({ tempo_worklog_id: "TW-1", exported_at: "2026-07-26T10:00:00Z" }));
    expect(item(REGENERATE).disabled).toBe(false);
  });
});

describe("BlockContextMenu split (FR-12)", () => {
  // 30-minute block: valid first parts are 1..29.
  const openSplit = (block: Block = makeBlock()) => {
    openMenu(block);
    fireEvent.click(item("Split…"));
  };
  const minutes = () => screen.getByLabelText("Minutes") as HTMLInputElement;
  const confirm = () => screen.getByRole("button", { name: "Confirm" }) as HTMLButtonElement;
  const type = (v: string) => fireEvent.change(minutes(), { target: { value: v } });

  it("opens a blank minutes box, menu still open, Confirm disabled (catches a prefilled value)", () => {
    openSplit();
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(minutes().value).toBe("");
    expect(confirm().disabled).toBe(true);
  });

  it.each([
    ["0 (below range)", "0", true],
    ["1 (lower bound)", "1", false],
    ["29 (length - 1)", "29", false],
    ["30 (== length, > for >=)", "30", true],
    ["31", "31", true],
    ["-5", "-5", true],
    ["2.5 (fraction)", "2.5", true],
    ["blank", "", true],
  ])("Confirm for %s is disabled=%s", (_n, v, disabled) => {
    openSplit();
    type(v);
    expect(confirm().disabled).toBe(disabled);
  });

  it("a 30m30s block allows 30 but not 31 (catches flooring the length)", () => {
    openSplit(makeBlock({ duration_seconds: 1830 }));
    type("30");
    expect(confirm().disabled).toBe(false);
    type("31");
    expect(confirm().disabled).toBe(true);
  });

  it("confirm splits this block on this day at the typed minutes and closes", async () => {
    openSplit(makeBlock({ id: 9 }));
    type("12");
    fireEvent.click(confirm());
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(splitBlock).toHaveBeenCalledWith(9, DAY, 12);
  });

  it("a daemon refusal shows its reason inside the menu, keeps the box and value (swallowed error)", async () => {
    splitBlock.mockImplementationOnce(async () => ({ ok: false, error: "split point 5m is not allowed" }) as never);
    openSplit();
    type("5");
    fireEvent.click(confirm());
    await waitFor(() => expect(screen.getByRole("menu").textContent).toContain("split point 5m is not allowed"));
    expect(minutes().value).toBe("5");
    expect(confirm().disabled).toBe(false);
  });

  it.each([
    ["synced", { tempo_worklog_id: "TW-1" }],
    ["exported", { exported_at: "2026-07-26T10:00:00Z" }],
  ])("stays enabled on a %s block (FR-14)", (_n, o) => {
    openMenu(makeBlock(o));
    expect(item("Split…").disabled).toBe(false);
  });
});

describe("BlockContextMenu merge with neighbour (FR-13, FR-14)", () => {
  const at = (id: number, from: string, to: string, o: Partial<Block> = {}) =>
    makeBlock({ id, started_at: `${DAY}T${from}:00Z`, ended_at: `${DAY}T${to}:00Z`, ...o });
  const me = at(2, "10:00", "10:30", { jira_issue: "PROJ-1" });
  const MERGE_PREV = "Merge with previous";
  const MERGE_NEXT = "Merge with next";

  it("merges into the previous block with the clicked block as primary, whatever ticket or gap", async () => {
    const prev = at(1, "08:00", "08:30", { jira_issue: "OTHER-9" });
    openMenu(me, [prev, me]);
    fireEvent.click(item(MERGE_PREV));
    await waitFor(() => expect(mergeGroup).toHaveBeenCalledWith(2, [1], DAY));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("merges the next block, primary still the clicked one (catches swapped primary)", async () => {
    openMenu(me, [me, at(3, "12:00", "12:30")]);
    fireEvent.click(item(MERGE_NEXT));
    await waitFor(() => expect(mergeGroup).toHaveBeenCalledWith(2, [3], DAY));
  });

  it("picks the adjacent neighbour from an unsorted list, not the first or last (catches array order)", async () => {
    const day = [at(5, "15:00", "15:30"), at(4, "06:00", "06:30"), me, at(3, "11:00", "11:30"), at(1, "09:00", "09:30")];
    openMenu(me, day);
    fireEvent.click(item(MERGE_PREV));
    await waitFor(() => expect(mergeGroup).toHaveBeenCalledWith(2, [1], DAY));
    cleanup();
    mergeGroup.mockClear();
    openMenu(me, day);
    fireEvent.click(item(MERGE_NEXT));
    await waitFor(() => expect(mergeGroup).toHaveBeenCalledWith(2, [3], DAY));
  });

  it("a refused merge toasts the reason (swallowed error)", async () => {
    mergeGroup.mockImplementationOnce(async () => ({ ok: false, error: "would orphan a synced block" }) as never);
    openMenu(me, [at(1, "08:00", "08:30"), me]);
    fireEvent.click(item(MERGE_PREV));
    await waitFor(() => expect(toasts().some((t) => t.text.includes("would orphan a synced block"))).toBe(true));
  });

  it("first block of the day: previous disabled, next enabled; last block: the reverse", () => {
    openMenu(me, [me, at(3, "12:00", "12:30")]);
    expect(item(MERGE_PREV).disabled).toBe(true);
    expect(item(MERGE_PREV).title).not.toBe("");
    expect(item(MERGE_NEXT).disabled).toBe(false);
    cleanup();
    openMenu(me, [at(1, "08:00", "08:30"), me]);
    expect(item(MERGE_PREV).disabled).toBe(false);
    expect(item(MERGE_NEXT).disabled).toBe(true);
  });

  it("a lone block disables both", () => {
    openMenu(me, [me]);
    expect(item(MERGE_PREV).disabled).toBe(true);
    expect(item(MERGE_NEXT).disabled).toBe(true);
  });

  it.each([
    ["synced", { tempo_worklog_id: "TW-1" }],
    ["exported", { exported_at: "2026-07-26T10:00:00Z" }],
  ])("a %s neighbour disables only that side, with a reason", (_n, o) => {
    openMenu(me, [at(1, "08:00", "08:30", o), me, at(3, "12:00", "12:30")]);
    expect(item(MERGE_PREV).disabled).toBe(true);
    expect(item(MERGE_PREV).title).not.toBe("");
    expect(item(MERGE_NEXT).disabled).toBe(false);
  });

  it("a synced block further back does not matter (adjacent only)", () => {
    openMenu(me, [at(0, "06:00", "06:30", { tempo_worklog_id: "TW-1" }), at(1, "08:00", "08:30"), me]);
    expect(item(MERGE_PREV).disabled).toBe(false);
  });

  it("touching blocks merge; overlap by one minute does not (catches <= for <)", () => {
    openMenu(me, [at(1, "09:30", "10:00"), me, at(3, "10:29", "11:00")]);
    expect(item(MERGE_PREV).disabled).toBe(false);
    expect(item(MERGE_NEXT).disabled).toBe(true);
  });

  it.each([
    ["synced", { tempo_worklog_id: "TW-1" }],
    ["exported", { exported_at: "2026-07-26T10:00:00Z" }],
  ])("both Merge items are disabled when the clicked block is %s (FR-14)", (_n, o) => {
    const mine = { ...me, ...o } as Block;
    openMenu(mine, [at(1, "08:00", "08:30"), mine, at(3, "12:00", "12:30")]);
    for (const name of [MERGE_PREV, MERGE_NEXT]) {
      expect(item(name).disabled).toBe(true);
      expect(item(name).title).not.toBe("");
    }
  });

  it("ignored blocks and other days are not neighbours", () => {
    openMenu(me, [at(1, "08:00", "08:30", { ignored_at: "2026-07-25T11:00:00Z" }), me, at(3, "12:00", "12:30", { day: "2026-07-26" })]);
    expect(item(MERGE_PREV).disabled).toBe(true);
    expect(item(MERGE_NEXT).disabled).toBe(true);
  });
});
