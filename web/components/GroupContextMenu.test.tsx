import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RedescribeTempoLineBody, TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";
import { dismiss, subscribe, type ToastMsg } from "@/lib/toast";

const key: TempoLineKey = { day: "2026-09-25", jira_issue: "PROJ-1" };
const returned: TempoLine = {
  ...key,
  text: "Rewrote",
  text_origin: "manual",
  fallback_text: "Work on PROJ-1",
  union_seconds: 3600,
  hours_override_seconds: null,
  effective_seconds: 3600,
  billing: null,
};
const redescribe = mock(async (_body: RedescribeTempoLineBody): Promise<TempoLine> => returned);

// Mock only the daemon boundary; bun's mock.module is process-wide.
const realDaemonLines = { ...(await import("@/lib/daemonTempoLines")) };
mock.module("@/lib/daemonTempoLines", () => ({ ...realDaemonLines, redescribeTempoLine: redescribe }));
mock.module("next/cache", () => ({ revalidatePath: () => {} }));

let GroupContextMenu: typeof import("./GroupContextMenu").GroupContextMenu;
beforeAll(async () => {
  GroupContextMenu = (await import("./GroupContextMenu")).GroupContextMenu;
});

const toasts = () => {
  let q: ToastMsg[] = [];
  subscribe((m) => (q = m))();
  return q;
};
afterEach(() => {
  cleanup();
  subscribe((queued) => queued.forEach((msg) => dismiss(msg.id)))();
  redescribe.mockClear();
});

const close = mock(() => {});
const open = (lineKey: TempoLineKey | null = key) => {
  close.mockClear();
  render(<GroupContextMenu lineKey={lineKey ?? undefined}x={10} y={10} close={close} />);
};
const item = () => screen.getByRole("menuitem", { name: "Regenerate with prompt…" }) as HTMLButtonElement;
const openBox = () => {
  open();
  fireEvent.click(item());
};
const box = () => screen.getByLabelText("Instruction") as HTMLInputElement;
const submit = () => screen.getByRole("button", { name: /^(Regenerate|Writing…)$/ }) as HTMLButtonElement;
const type = (text: string) => fireEvent.change(box(), { target: { value: text } });

describe("GroupContextMenu", () => {
  it("has exactly one item (catches extra items)", () => {
    open();
    expect(screen.getAllByRole("menuitem").length).toBe(1);
  });

  it("is focused on open (catches no initial focus)", () => {
    open();
    expect(document.activeElement).toBe(item());
  });

  it("is disabled with 'Pick a ticket first' without a line key (catches an enabled item on Unassigned)", () => {
    open(null);
    expect(item().disabled).toBe(true);
    expect(item().title).toBe("Pick a ticket first");
    fireEvent.click(item());
    expect(screen.queryByLabelText("Instruction")).toBeNull();
  });

  it("picking the item swaps in an instruction box, menu still open (catches closing the menu)", () => {
    openBox();
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.queryAllByRole("menuitem").length).toBe(0);
    expect(document.activeElement).toBe(box());
    expect(close).not.toHaveBeenCalled();
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
    expect(box().maxLength).toBe(500);
    type("þ".repeat(500));
    expect(submit().disabled).toBe(false);
    type("ðæ".repeat(250) + "þ");
    expect(submit().disabled).toBe(true);
  });

  it("posts the trimmed instruction with this line's day and ticket", async () => {
    openBox();
    type("  þetta var innskráningarvillan  ");
    fireEvent.click(submit());
    await waitFor(() =>
      expect(redescribe).toHaveBeenCalledWith({ ...key, instruction: "þetta var innskráningarvillan" }),
    );
  });

  it("success closes the menu and toasts 'Text rewritten'", async () => {
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(close).toHaveBeenCalledTimes(1));
    expect(toasts().some((t) => t.text === "Text rewritten")).toBe(true);
  });

  it("failure toasts the reason, keeps the box open with the text kept (catches a swallowed error)", async () => {
    redescribe.mockImplementationOnce(async () => {
      throw new Error("model timed out");
    });
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(toasts().some((t) => t.text === "Rewrite failed — model timed out")).toBe(true));
    expect(close).not.toHaveBeenCalled();
    expect(box().value).toBe("login bug");
    expect(submit().disabled).toBe(false);
  });

  it("Esc closes the menu (catches missing dismiss)", () => {
    open();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("an outside click closes the menu when idle", () => {
    open();
    fireEvent.mouseDown(document.body);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("an outside click does not close it while writing (catches losing the request)", async () => {
    redescribe.mockImplementationOnce(() => new Promise<TempoLine>(() => {}));
    openBox();
    type("login bug");
    fireEvent.click(submit());
    await waitFor(() => expect(submit().textContent).toBe("Writing…"));
    fireEvent.mouseDown(document.body);
    expect(close).not.toHaveBeenCalled();
  });
});
