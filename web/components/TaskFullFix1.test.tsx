// ticket-full fix round 1: board tickets reach the Move picker, one Today on phones, labels, Jira context path, polish CSS.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TaskRow } from "@/lib/types";
import { TaskBoard } from "./TaskBoard";
import { TaskRelated } from "./TaskRelated";
import { block, day, payload } from "./workLogTestKit";
import { actions, detail, open, restoreViewport, row, viewport } from "./taskModalTestKit";

beforeEach(() => (window as unknown as { happyDOM: { setURL(url: string): void } }).happyDOM.setURL("http://localhost/tasks"));
afterEach(() => {
  cleanup();
  restoreViewport();
});

const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
const withBlocks = (today: Record<string, unknown>, over: Record<string, unknown> = {}) =>
  actions({
    loadTicketBlocks: mock(async () => ({ ok: true as const, data: { ...payload([day({ blocks: [block({ tempo_worklog_id: "w" })] })]), today } })),
    ...over,
  });
const totals = (over: Record<string, unknown> = {}) => ({
  day: "2026-10-02",
  worked_seconds: 8100,
  in_tempo_seconds: 5400,
  ticket_worked_seconds: 0,
  ticket_in_tempo_seconds: null,
  ...over,
});
const lines = () => [...document.querySelectorAll(".task-summary-line")].map((l) => l.textContent);

describe("Move gets the board's tickets", () => {
  it("lists another board ticket when Move opens inside the modal", async () => {
    const tasks: TaskRow[] = [row({}), row({ key: "ABC-2", summary: "Spike cache" })];
    const a = actions({ loadTicketBlocks: mock(async () => ({ ok: true as const, data: payload([day()]) })) });
    render(<TaskBoard actions={a} tasks={tasks} />);
    fireEvent.click(document.querySelector('[data-task-key="ABC-1"] .task-card-btn') as HTMLElement);
    await act(async () => {});
    fireEvent.click(await screen.findByRole("button", { name: "Move this block to another ticket" }));
    const options = [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent);
    expect(options.some((t) => t?.includes("ABC-2") && t.includes("Spike cache"))).toBe(true);
  });
});

describe("phone summary has one Today", () => {
  it("live: `Today about … worked · … in Tempo` with the gap tone, and no cached Today", async () => {
    viewport(false);
    open(withBlocks(totals()));
    await waitFor(() => expect(lines()[1]).toBe("Today about 2h 15m worked · 1h 30m in Tempo45m not in Tempo yet"));
    expect(lines()[0]).not.toContain("Today");
    expect(document.querySelectorAll(".task-summary-line .task-tempo[data-tone='changed']").length).toBe(1);
  });

  it("level with Tempo reads Up to date in the ok tone", async () => {
    viewport(false);
    open(withBlocks(totals({ in_tempo_seconds: 9000 })));
    await waitFor(() => expect(lines()[1]).toBe("Today about 2h 15m worked · 2h 30m in TempoUp to date"));
  });

  it("before the live numbers arrive, only the cached Today shows", () => {
    viewport(false);
    open(actions({ loadTicketBlocks: mock(() => new Promise(() => {})) }));
    expect(lines()).toEqual(["This week 1h 30m · Today 30m"]);
  });
});

describe("Details labels", () => {
  it("prefer the live detail even when it has none", async () => {
    viewport(true);
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ labels: [] }) })) }), row({ labels: ["stale"] }));
    expect(document.querySelector(".task-details")?.textContent).toContain("stale"); // cached until the detail loads
    await waitFor(() => expect(document.querySelector(".task-details")?.textContent).not.toContain("stale"));
  });
});

describe("Related", () => {
  const ref = { key: "ABC-9", summary: "Other", status: "To Do", status_category: "new" as const, issue_type: "Task" };

  it("keeps a Jira context path in the browse link", () => {
    render(<TaskRelated detail={detail({ url: "https://host/jira/browse/A-1", subtasks: [ref] })} />);
    expect(screen.getByRole("link", { name: /ABC-9/ }).getAttribute("href")).toBe("https://host/jira/browse/ABC-9");
  });

  it("says Show all N with what it shows", () => {
    const many = (n: number) => Array.from({ length: n }, (_, i) => ({ ...ref, key: `ABC-${i + 10}` }));
    render(
      <TaskRelated
        detail={detail({
          subtasks: many(6),
          links: many(7).map((issue) => ({ relation: "blocks", issue })),
          attachments: many(8).map((r) => ({ filename: r.key, size_bytes: 10, url: "https://x/y", created: null, author: null })),
        })}
      />,
    );
    for (const name of ["Show all 6 subtasks", "Show all 7 linked issues", "Show all 8 attachments"]) {
      expect(screen.getByRole("button", { name })).toBeTruthy();
    }
  });
});

describe("polish CSS", () => {
  const rule = (sel: string) => css.split("\n").find((l) => l.startsWith(`${sel} {`)) ?? "";

  it("phone attachments wrap their meta to a second line instead of hiding it", () => {
    expect(css).not.toMatch(/\.task-rel-meta \{ display: none; \}/);
    expect(css).toContain(".task-rel-file { flex-wrap: wrap; }");
  });

  it("the meter fill animates a transform with the shared easing, and motion is off for reduced motion", () => {
    expect(rule(".task-meter > span")).toContain("transform-origin: left");
    expect(rule(".task-meter > span")).toContain("transition: transform 150ms cubic-bezier(0.25, 1, 0.5, 1)");
    const fix1 = css.slice(css.indexOf("ticket-full: fix1"));
    expect(fix1.slice(fix1.indexOf("prefers-reduced-motion: reduce"))).toContain(".task-meter > span { transition: none; }");
  });

  it("the new buttons and rows have a pressed state", () => {
    expect(css).toContain(".task-rel-row:active");
    expect(css).toContain(".task-block-move:active");
  });

  it("12px footers use the muted ink, and the inline picker popover fits the modal", () => {
    expect(rule(".task-time-foot")).toContain("var(--fg-muted)");
    expect(rule(".task-side-foot")).toContain("var(--fg-muted)");
    expect(css).toContain(".task-block-picker .combobox-popover { left: 0; right: 0; width: auto; max-width: 100%; }");
  });
});
