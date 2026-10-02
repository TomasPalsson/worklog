// Ticket dialog sidebar: status button and menu, Details rows, Time card and the Tempo summary.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { block, changedDay, day, payload } from "./workLogTestKit";
import { actions, calls, detail, open, quiet, row, start } from "./taskModalTestKit";
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

describe("Details", () => {
  it("lists assignee, priority, type, due, labels and updated from the live detail", async () => {
    open(actions(), row({ due_date: "2026-10-30", labels: ["backend", "auth"] }));
    await screen.findByText(/Steps to reproduce/);
    const d = ".task-details";
    expect(value(d, "Assignee").textContent).toBe("Ada");
    expect(value(d, "Priority").textContent).toBe("Priority HighHigh"); // glyph text + name
    expect(value(d, "Priority").querySelector("svg")).toBeTruthy();
    expect(within(value(d, "Type")).getByText("Bug", { selector: "[aria-hidden]" })).toBeTruthy();
    expect(value(d, "Due").textContent).toMatch(/^(Due|Overdue)/);
    expect(value(d, "Labels").textContent).toBe("backendauth");
    expect(value(d, "Updated").textContent).toMatch(/^5 Sep, \d\d:\d\d$/);
  });

  it("shows the cached row until the detail loads, then Jira's values", async () => {
    let finish!: (v: unknown) => void;
    open(actions({ loadTicketDetail: mock(() => new Promise((r) => (finish = r))) }), row({ priority: "Medium", issue_type: "Task" }));
    const d = ".task-details";
    expect(value(d, "Priority").textContent).toContain("Medium");
    expect(within(value(d, "Type")).getByText("Task", { selector: "[aria-hidden]" })).toBeTruthy();
    expect(value(d, "Assignee").textContent).toBe("You"); // the row only knows it is assigned to the Owner
    finish({ ok: true, data: detail() });
    await waitFor(() => expect(value(d, "Assignee").textContent).toBe("Ada"));
    expect(value(d, "Priority").textContent).toContain("High");
    expect(within(value(d, "Type")).getByText("Bug", { selector: "[aria-hidden]" })).toBeTruthy();
  });

  it("leaves out the rows Jira has no value for", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ assignee: null, priority: null, issue_type: null, updated: null }) })) }));
    await waitFor(() => expect(terms(".task-details")).toEqual([]));
    expect(card(".task-details").textContent).not.toContain("None");
  });

  it("lists only the rows that have a value", async () => {
    open(actions(), row({ due_date: "2026-10-30" }));
    await screen.findByText(/Steps to reproduce/);
    expect(terms(".task-details")).toEqual(["Assignee", "Priority", "Type", "Due", "Updated"]); // no Labels
  });

  it("shows an unknown priority as text only", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ priority: "Blocker" }) })) }));
    await screen.findByText(/Steps to reproduce/);
    expect(value(".task-details", "Priority").textContent).toBe("Blocker");
    expect(value(".task-details", "Priority").querySelector("svg")).toBeNull();
  });

  it("is open beside the main column", () => {
    open(actions());
    expect(card(".task-details").hasAttribute("open")).toBe(true);
  });

  it("an unassigned ticket has no Assignee row", () => {
    open(actions(), row({ assigned: false }));
    expect(terms(".task-details")).not.toContain("Assignee");
  });
});

describe("Time card", () => {
  it("shows this week and today (left out at 0), and no second total: the tab count already says the last 14 days", async () => {
    open(withDays([day({ line_seconds: 5400 }), day({ day: "2026-09-30", line_seconds: 1800 })]));
    const t = ".task-time";
    expect(value(t, "This week").textContent).toBe("1h 30m");
    expect(value(t, "Today").textContent).toBe("30m");
    expect(terms(t)).toEqual(["Today", "This week", "Tempo"]);
    await waitFor(() => expect(value(t, "Tempo").textContent).toContain("not sent"));
    cleanup();
    open(actions(), row({ today_seconds: 0 }));
    expect(terms(t)).toEqual(["This week", "Tempo"]);
    await screen.findByText(/Steps to reproduce/);
  });

  it("says All sent when every day is in Tempo", async () => {
    open(withDays([day({ blocks: [block({ tempo_worklog_id: "w" })] })]));
    await waitFor(() => expect(value(".task-time", "Tempo").textContent).toBe("All sent"));
    expect(value(".task-time", "Tempo").querySelector("button")).toBeNull();
    expect(value(".task-time", "Tempo").querySelector(".task-tempo")?.getAttribute("data-tone")).toBe("ok");
  });

  it("says how many days are not sent, and how many changed since sent", async () => {
    open(
      withDays([
        day({ day: "2026-10-03" }),
        day({ day: "2026-10-02" }),
        changedDay({ day: "2026-10-01" }),
      ]),
    );
    await waitFor(() => expect(value(".task-time", "Tempo").textContent).toBe("2 days not sent, 1 day changed since sent · Show unsent day"));
    expect(value(".task-time", "Tempo").querySelectorAll("button")).toHaveLength(1); // said once, with one Show unsent day
  });

  it("says 1 day in the singular and shows a dash with nothing logged", async () => {
    open(withDays([day()]));
    await waitFor(() => expect(value(".task-time", "Tempo").textContent).toBe("1 day not sent · Show unsent day"));
    cleanup();
    open(actions());
    await screen.findByText(/Steps to reproduce/);
    expect(value(".task-time", "Tempo").textContent).toBe("—");
  });

  it("Show unsent day is a button labelled for the first unsent day, the state beside it is plain text", async () => {
    open(withDays([changedDay()]));
    await waitFor(() => expect(value(".task-time", "Tempo").textContent).toBe("1 day changed since sent · Show unsent day"));
    const text = value(".task-time", "Tempo").querySelector("span.task-tempo") as HTMLElement;
    expect(text.getAttribute("data-tone")).toBe("changed");
    expect(text.tagName).toBe("SPAN");
    const b = value(".task-time", "Tempo").querySelector("button") as HTMLElement;
    expect(b.className).toBe("task-review");
    expect(b.getAttribute("aria-label")).toBe("Show the first unsent day");
  });

  it("clicking Show unsent day switches to the Work log and scrolls to the first such day", async () => {
    window.localStorage.setItem("worklog.ticketTab", "comments");
    const scroll = mock((_o: unknown) => {});
    (HTMLElement.prototype as unknown as { scrollIntoView: unknown }).scrollIntoView = scroll;
    try {
      open(withDays([day({ day: "2026-10-03", blocks: [block({ tempo_worklog_id: "w" })] }), day({ day: "2026-10-02" })]), quiet());
      expect(screen.getByRole("tab", { name: /Comments/ }).getAttribute("aria-selected")).toBe("true");
      await waitFor(() => expect(value(".task-time", "Tempo").querySelector("button")).toBeTruthy());
      fireEvent.click(value(".task-time", "Tempo").querySelector("button") as HTMLElement);
      expect(screen.getByRole("tab", { name: /Work log/ }).getAttribute("aria-selected")).toBe("true");
      await waitFor(() => expect(scroll.mock.calls.length).toBeGreaterThan(0));
      const target = scroll.mock.contexts.at(-1) as HTMLElement;
      expect(target.getAttribute("data-day")).toBe("2026-10-02");
    } finally {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
    }
  });
});

describe("sidebar footer", () => {
  it("says when Jira last answered, once it has", async () => {
    open(actions());
    expect(document.querySelector(".task-side-foot")).toBeNull();
    await waitFor(() => expect(document.querySelector(".task-side-foot")?.textContent).toMatch(/^Jira synced at \d\d:\d\d$/));
  });
});
