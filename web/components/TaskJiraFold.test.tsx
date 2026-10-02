// The dialog's collapsed Jira section: what is inside, what stays outside, and the Details rows.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { actions, detail, open, row, unfold } from "./taskModalTestKit";
import { block, day, payload } from "./workLogTestKit";

afterEach(cleanup);

const fold = () => document.querySelector(".task-jira-fold") as HTMLDetailsElement;
const card = () => document.querySelector(".task-details") as HTMLElement;
const value = (term: string) => {
  const dt = [...card().querySelectorAll("dt")].find((n) => n.textContent === term) as HTMLElement;
  return dt.nextElementSibling as HTMLElement;
};
const terms = () => [...card().querySelectorAll("dt")].map((n) => n.textContent);
const live = (over: Parameters<typeof detail>[0]) => actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail(over) })) });

describe("Jira fold", () => {
  it("is closed by default; open it and Description, Details, Related and a comment composer are inside, Your time and the work log are not", async () => {
    const parent = { key: "ABC-9", summary: "Epic", status: "To Do", status_category: "new" as const, issue_type: "Epic" };
    open(
      actions({
        loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ reporter: "Grace", parent }) })),
        loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload([day({ blocks: [block()] })]) })),
      }),
    );
    expect(fold().open).toBe(false);
    expect(fold().querySelector("summary")?.textContent).toBe("Jira — description, comments, details");
    unfold();
    expect(fold().open).toBe(true);
    await screen.findByText(/Steps to reproduce/);
    const inside = within(fold());
    expect(inside.getByRole("heading", { name: "Description" })).not.toBeNull();
    expect(inside.getByRole("heading", { name: "Details" })).not.toBeNull();
    await waitFor(() => expect(value("Reporter").textContent).toBe("Grace"));
    expect(inside.getByRole("heading", { name: "Related" })).not.toBeNull();
    expect(inside.getByRole("textbox", { name: "Add a comment" })).not.toBeNull();
    await waitFor(() => expect(document.querySelector(".task-hours")).toBeTruthy());
    expect(fold().querySelector(".task-hours")).toBeNull();
    expect(fold().querySelector(".task-work")).toBeNull();
    expect(fold().parentElement?.querySelector(".task-work")).toBeTruthy();
  });
});

describe("Details", () => {
  it("lists assignee, priority, type, due, labels and updated from the live detail", async () => {
    open(live({ labels: ["backend", "auth"] }), row({ due_date: "2026-10-30" }));
    unfold();
    await screen.findByText(/Steps to reproduce/);
    expect(value("Assignee").textContent).toBe("Ada");
    expect(value("Priority").textContent).toBe("Priority HighHigh"); // glyph text + name
    expect(value("Priority").querySelector("svg")).toBeTruthy();
    expect(within(value("Type")).getByText("Bug", { selector: "[aria-hidden]" })).toBeTruthy();
    expect(value("Due").textContent).toMatch(/^(Due|Overdue)/);
    expect(value("Labels").textContent).toBe("backendauth");
    expect(value("Updated").textContent).toMatch(/^5 Sep, \d\d:\d\d$/);
  });

  it("shows the cached row until the detail loads, then Jira's values", async () => {
    let finish!: (v: unknown) => void;
    open(actions({ loadTicketDetail: mock(() => new Promise((r) => (finish = r))) }), row({ priority: "Medium", issue_type: "Task" }));
    unfold();
    expect(value("Priority").textContent).toContain("Medium");
    expect(within(value("Type")).getByText("Task", { selector: "[aria-hidden]" })).toBeTruthy();
    expect(value("Assignee").textContent).toBe("You"); // the row only knows it is assigned to the Owner
    finish({ ok: true, data: detail() });
    await waitFor(() => expect(value("Assignee").textContent).toBe("Ada"));
    expect(value("Priority").textContent).toContain("High");
    expect(within(value("Type")).getByText("Bug", { selector: "[aria-hidden]" })).toBeTruthy();
  });

  it("leaves out the rows Jira has no value for", async () => {
    open(live({ assignee: null, priority: null, issue_type: null, updated: null }));
    unfold();
    await waitFor(() => expect(terms()).toEqual([]));
    expect(card().textContent).not.toContain("None");
  });

  it("lists only the rows that have a value", async () => {
    open(actions(), row({ due_date: "2026-10-30" }));
    unfold();
    await screen.findByText(/Steps to reproduce/);
    expect(terms()).toEqual(["Assignee", "Priority", "Type", "Due", "Updated"]); // no Labels
  });

  it("shows an unknown priority as text only", async () => {
    open(live({ priority: "Blocker" }));
    unfold();
    await screen.findByText(/Steps to reproduce/);
    expect(value("Priority").textContent).toBe("Blocker");
    expect(value("Priority").querySelector("svg")).toBeNull();
  });

  it("an unassigned ticket has no Assignee row", () => {
    open(actions(), row({ assigned: false }));
    unfold();
    expect(terms()).not.toContain("Assignee");
  });

  it("labels prefer the live detail even when it has none", async () => {
    open(live({ labels: [] }), row({ labels: ["stale"] }));
    unfold();
    expect(card().textContent).toContain("stale"); // cached until the detail loads
    await waitFor(() => expect(card().textContent).not.toContain("stale"));
  });
});
