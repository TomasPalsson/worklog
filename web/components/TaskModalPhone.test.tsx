// The dialog at phone width: status and hours under the title, a sidebar that only holds the Tempo card, one scroller.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { block, changedDay, day, payload } from "./workLogTestKit";
import { actions, open, quiet, restoreViewport, row, viewport } from "./taskModalTestKit";

afterEach(() => {
  cleanup();
  restoreViewport();
});

const summary = () => document.querySelector(".task-summary") as HTMLElement;
const line = () => (document.querySelector(".task-summary-line") as HTMLElement).textContent;
const withDays = (days: ReturnType<typeof day>[]) =>
  actions({ loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload(days) })) });

describe("summary under the title", () => {
  it("sits right after the title on phones, with the status button and one line of hours", async () => {
    viewport(false);
    open(withDays([day()]));
    const title = screen.getByRole("heading", { name: "Fix login" });
    expect(title.nextElementSibling).toBe(summary());
    expect(summary().querySelector('[data-testid="status-ABC-1"]')).toBeTruthy();
    await waitFor(() => expect(line()).toBe("Today 30m · 1 day not sent"));
  });

  it("leaves Today out at 0 and says All sent as plain text", async () => {
    viewport(false);
    open(withDays([day({ blocks: [block({ tempo_worklog_id: "w" })] })]), row({ today_seconds: 0 }));
    await waitFor(() => expect(line()).toBe("All sent"));
    expect(summary().querySelector(".task-summary-line button")).toBeNull();
  });

  it("is absent on desktop, where the sidebar has the status and the Time card", () => {
    viewport(true);
    open(actions());
    expect(summary()).toBeNull();
    expect(document.querySelector(".task-modal-side [data-testid='status-ABC-1']")).toBeTruthy();
    expect(document.querySelector(".task-time")).toBeTruthy();
  });

  it("moves the sidebar's status out on phones: one status button; the sidebar holds only the Tempo card", () => {
    viewport(false);
    open(actions());
    expect(screen.getAllByTestId("status-ABC-1")).toHaveLength(1);
    expect(document.querySelector(".task-modal-side .task-status")).toBeNull();
    expect([...document.querySelectorAll(".task-modal-side .task-label")].map((n) => n.textContent)).toEqual(["Tempo"]);
  });

  it("tab order matches the page: title, status, then the ledger, work log and description", () => {
    viewport(false);
    open(actions(), quiet());
    const order = [...document.querySelectorAll(".task-modal-body button, .task-modal-body summary")];
    const status = order.indexOf(screen.getByTestId("status-ABC-1"));
    expect(status).toBe(0);
    expect(order.indexOf(screen.getByText("Description", { selector: "summary" }))).toBeGreaterThan(status);
  });

  it("its Tempo part scrolls to the first day still to send", async () => {
    const scroll = mock((_o: unknown) => {});
    (HTMLElement.prototype as unknown as { scrollIntoView: unknown }).scrollIntoView = scroll;
    try {
      viewport(false);
      open(withDays([day({ day: "2026-10-03", blocks: [block({ tempo_worklog_id: "w" })] }), changedDay({ day: "2026-10-02" })]), quiet());
      const review = await waitFor(() => {
        const b = document.querySelector(".task-summary-line button") as HTMLElement;
        if (!b) throw new Error("not yet");
        return b;
      });
      expect(review.textContent).toBe("1 day changed since sent");
      fireEvent.click(review);
      await waitFor(() => expect(scroll.mock.calls.length).toBeGreaterThan(0));
      expect((scroll.mock.contexts.at(-1) as HTMLElement).getAttribute("data-day")).toBe("2026-10-02");
    } finally {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
    }
  });

  it("follows the viewport while the dialog is open", () => {
    const v = viewport(true);
    open(actions());
    expect(summary()).toBeNull();
    v.set(false);
    expect(summary()).toBeTruthy();
    expect(screen.getAllByTestId("status-ABC-1")).toHaveLength(1);
    v.set(true);
    expect(summary()).toBeNull();
  });
});

describe("at phone width", () => {
  it("still offers Open in Jira", () => {
    viewport(false);
    open(actions());
    expect(screen.getByRole("link", { name: "Open ABC-1 in Jira" }).getAttribute("href")).toBe("https://x.atlassian.net/browse/ABC-1");
  });
});

describe("Jira link in the header", () => {
  it("is icon-only on phones, named by its aria-label", () => {
    viewport(false);
    open(withDays([day()]));
    const link = screen.getByRole("link", { name: "Open ABC-1 in Jira" });
    expect(link.getAttribute("data-tip")).toBe("Open in Jira");
    expect(link.textContent).toBe("");
  });
});
