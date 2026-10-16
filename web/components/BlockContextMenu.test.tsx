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
  for (const m of [setPersonal, setIgnored, deleteBlock, mergeGroup, describeBlock, undoLastChange]) m.mockClear();
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
    expect(items.length).toBe(6);
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
