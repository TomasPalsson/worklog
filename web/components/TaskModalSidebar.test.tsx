// Ticket dialog sidebar: status button and menu, Time card and the Tempo summary.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { day, payload } from "./workLogTestKit";
import { actions, calls, open, row, start } from "./taskModalTestKit";
import type { Transition } from "@/lib/types";

afterEach(cleanup);

const card = (cls: string) => document.querySelector(cls) as HTMLElement;
/** The value cell for a label in a sidebar card. */
const value = (cls: string, term: string) => {
  const dt = [...card(cls).querySelectorAll("dt")].find((n) => n.textContent === term) as HTMLElement;
  return dt.nextElementSibling as HTMLElement;
};
const terms = (cls: string) => [...card(cls).querySelectorAll("dt")].map((n) => n.textContent);
const withDays = (days: ReturnType<typeof day>[]) =>
  actions({ loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload(days) })) });

describe("status button", () => {
  it("is full width with the ticket's status, coloured by category", () => {
    open(actions());
    const b = screen.getByTestId("status-ABC-1");
    expect(b.textContent).toBe("To Do");
    expect(b.className).toBe("task-status");
    expect(b.getAttribute("aria-haspopup")).toBe("true");
    expect(b.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("labels a menu move by name only when the name equals the target status", async () => {
    const same: Transition = { id: "41", name: "Done", to_status: "done", to_category: "done" };
    open(actions({ loadTransitions: mock(async () => ({ ok: true as const, data: [start, same] })) }));
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    expect(await screen.findByRole("menuitem", { name: "Start → In Progress" })).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Done" })).not.toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });

  it("changes status through the menu and reports the new status", async () => {
    const a = actions();
    const { onStatus } = open(a);
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Start → In Progress" }));
    await waitFor(() => expect(onStatus.mock.calls.length).toBe(1));
    expect(calls(a.transitionTicket)[0]).toEqual(["ABC-1", "11"]);
    expect(onStatus.mock.calls[0][0]).toEqual({ status: "In Progress", status_category: "indeterminate" });
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("In Progress");
    await screen.findByText(/Steps to reproduce/);
  });

  it("closes on Esc without closing the dialog, and on an outside click", async () => {
    const { onClose } = open(actions());
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    await screen.findByRole("menuitem", { name: "Start → In Progress" });
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menuitem", { name: "Start → In Progress" })).toBeNull();
    expect(onClose.mock.calls.length).toBe(0);
    expect(document.activeElement).toBe(screen.getByTestId("status-ABC-1"));
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    await screen.findByRole("menuitem", { name: "Start → In Progress" });
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menuitem", { name: "Start → In Progress" })).toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });

  it("is a menu of menuitems, focuses the first, and ArrowUp/Down wrap with Home/End", async () => {
    const { onClose } = open(actions());
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    const [first, last] = await screen.findAllByRole("menuitem");
    expect(screen.getByRole("menu")).not.toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(first));
    fireEvent.keyDown(first, { key: "ArrowUp" });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "ArrowDown" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "End" });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "Home" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(onClose.mock.calls.length).toBe(0);
  });

  it("while a status change runs it reads Moving… and is disabled", async () => {
    let finish: (v: unknown) => void = () => {};
    open(actions({ transitionTicket: mock(() => new Promise((res) => (finish = res))) }));
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Start → In Progress" }));
    const chip = screen.getByTestId("status-ABC-1") as HTMLButtonElement;
    await waitFor(() => expect(chip.textContent).toContain("Moving…"));
    expect(chip.disabled).toBe(true);
    finish({ ok: true, data: { key: "ABC-1", status: "In Progress", status_category: "indeterminate" } });
    await waitFor(() => expect(chip.textContent).toContain("In Progress"));
    expect(chip.disabled).toBe(false);
  });

  it("hands focus back to the status button after a successful move", async () => {
    open(actions());
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Start → In Progress" }));
    await waitFor(() => expect(screen.getByTestId("status-ABC-1").textContent).toBe("In Progress"));
    expect(document.activeElement).toBe(screen.getByTestId("status-ABC-1"));
  });

  it("shows Jira's refusal under the button", async () => {
    open(actions({ transitionTicket: mock(async () => ({ ok: false as const, error: "Resolution is required" })) }));
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Finish → Done" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Resolution is required");
  });
});

describe("Time card", () => {
  it("is headed Tempo and shows today (left out at 0); this week lives in the ledger", async () => {
    open(withDays([day({ line_seconds: 5400 }), day({ day: "2026-09-30", line_seconds: 1800 })]));
    const t = ".task-time";
    expect(card(t).querySelector(".task-label")?.textContent).toBe("Tempo");
    expect(value(t, "Today").textContent).toBe("30m");
    expect(terms(t)).toEqual(["Today"]);
    expect(screen.queryByText("Show unsent day")).toBeNull(); // the ledger owns the unsent state
    cleanup();
    open(actions(), row({ today_seconds: 0 }));
    expect(terms(t)).toEqual([]);
    await screen.findByText(/Steps to reproduce/);
  });
});

describe("sidebar footer", () => {
  it("says when Jira last answered, once it has", async () => {
    open(actions());
    expect(document.querySelector(".task-side-foot")).toBeNull();
    await waitFor(() => expect(document.querySelector(".task-side-foot")?.textContent).toMatch(/^Jira synced at \d\d:\d\d$/));
  });
});
