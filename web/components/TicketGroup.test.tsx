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
