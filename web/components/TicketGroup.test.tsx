// An assigned ticket group shows its Tempo line text and hours, and lets the
// Owner edit, regenerate or override them. The unassigned group has no controls.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BlockGroup } from "@/app/[day]/page";
import type { TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";
import { dismiss, subscribe } from "@/lib/toast";
import { ToastHost } from "./ToastHost";

const okVoid = () => mock(async () => ({ ok: true as const, data: undefined }));
const okList = () => mock(async () => ({ ok: true as const, data: [] }));
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
}));
// Process-wide like every mock.module: export the names every component tree
// that shares this specifier imports, so test order must not matter.
mock.module("@/app/actions", () => ({
  mergeGroup: okVoid(),
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

let TicketGroup: typeof import("./TicketGroup").TicketGroup;

beforeAll(async () => {
  TicketGroup = (await import("./TicketGroup")).TicketGroup;
});

afterEach(() => {
  cleanup();
  // The toast queue is module-global; leaving "Regenerated" in it breaks
  // other files' exact-text lookups.
  subscribe((queued) => queued.forEach((msg) => dismiss(msg.id)))();
  saveText.mockClear();
  saveHours.mockClear();
  regenerate.mockClear();
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
});

describe("TicketGroup hours", () => {
  it("shows the effective hours", () => {
    renderGroup(group(), line({ effective_seconds: 7200, hours_override_seconds: 7200 }));
    expect(screen.getByText("2.0h billed")).toBeTruthy();
  });

  it("saves a half-hour override in seconds", async () => {
    renderGroup(group(), line());
    const box = screen.getByLabelText("Hours for PROJ-1");
    fireEvent.change(box, { target: { value: "2.5" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, 9000]);
  });

  it("clears the override when the box is emptied", async () => {
    renderGroup(group(), line({ hours_override_seconds: 3600, effective_seconds: 3600 }));
    const box = screen.getByLabelText("Hours for PROJ-1");
    fireEvent.change(box, { target: { value: "" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(saveHours).toHaveBeenCalledTimes(1));
    expect(saveHours.mock.calls[0]).toEqual([key, null]);
  });

  it("rejects a non-half-hour value inline without calling the action", () => {
    renderGroup(group(), line());
    const box = screen.getByLabelText("Hours for PROJ-1");
    fireEvent.change(box, { target: { value: "1.3" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(screen.getByText(/multiple of half an hour/)).toBeTruthy();
    expect(saveHours).not.toHaveBeenCalled();
  });

  it("shows a daemon rejection inline", async () => {
    saveHours.mockImplementationOnce(async () => ({ ok: false, error: "hours must be positive" }));
    renderGroup(group(), line());
    const box = screen.getByLabelText("Hours for PROJ-1");
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
