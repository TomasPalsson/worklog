// Final fix round: phone chip labels, the block page cue, the blank-line-proof description clamp and the narrow-phone CSS.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, screen } from "@testing-library/react";
import { block, changedDay, day, open as openLog } from "./workLogTestKit";
import { actions, detail, open } from "./taskModalTestKit";

afterEach(cleanup);

const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
const rule = (selector: string) => css.split("\n").find((l) => l.startsWith(`${selector} {`)) ?? "";
const small = css.slice(css.indexOf("@media (max-width: 359px)"));
const phone = css.slice(css.indexOf("/* ticket dialog on small screens"), css.indexOf("/* fingers:"));

describe("day chip", () => {
  it("Changed keeps the full state as the chip's text and adds the short phone label", async () => {
    await openLog([changedDay()]);
    const chip = document.querySelector(".task-day-chip") as HTMLElement;
    expect(chip.querySelector(".task-chip-long")?.textContent).toBe("Changed since sent");
    expect(chip.querySelector(".task-chip-short")?.textContent).toBe("Changed");
    expect(chip.querySelector(".task-chip-short")?.getAttribute("aria-hidden")).toBe("true");
    expect(chip.title).toContain("Edited after it was sent");
  });

  it("Sent and Not sent need no short label", async () => {
    await openLog([day({ blocks: [block({ tempo_worklog_id: "w" })] }), day({ day: "2026-09-30" })]);
    expect(document.querySelectorAll(".task-chip-short")).toHaveLength(0);
  });

  it("phones show the short label, keep the full one for screen readers, and never wrap line 1", () => {
    expect(rule(".task-chip-short")).toContain("display: none");
    expect(phone).toContain(".task-chip-short { display: inline; }");
    expect(phone).toContain(".task-chip-long:has(+ .task-chip-short) { position: absolute; width: 1px; height: 1px;");
    expect(phone).toContain(".task-day-toggle { flex-wrap: nowrap;");
    expect(css).toContain(".task-day-label, .task-day-hours, .task-day-chip { white-space: nowrap; }");
  });

  it("line 2 starts on the day label's left edge (24px = padding + chevron + gap) and the body matches", () => {
    expect(phone).toContain(".task-day-trigger { flex: 1 1 0; min-width: 0; margin-left: 24px; }");
    expect(phone).toContain(".task-day-body { padding-left: 24px; }");
  });

  it("the changed-since-sent reason is a muted note in the opened day (the chip title keeps it too)", async () => {
    await openLog([changedDay()]);
    expect(document.querySelector(".task-day-body .task-day-note")?.textContent).toContain("Edited after it was sent");
  });
});

describe("block rows", () => {
  it("end in an ArrowUpRight cue that says it opens the block page", async () => {
    await openLog([day()]);
    const go = document.querySelector(".task-block-row .task-block-go") as HTMLElement;
    expect(go.title).toBe("Opens the block page");
    expect(go.querySelector(".task-sr")?.textContent).toBe("Opens the block page");
    expect(go.querySelector("svg.lucide-arrow-up-right")).toBeTruthy();
    expect(document.querySelector(".task-block-row .lucide-chevron-right")).toBeNull();
    expect(rule(".task-block-go")).toContain("position: relative");
  });
});

describe("description clamp", () => {
  const withText = (description: string) => actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ description }) })) });
  const prose = () => (document.querySelector(".task-prose") as HTMLElement).textContent;

  it("collapses runs of blank lines and trims the ends in the preview", async () => {
    open(withText("\n\nFirst\n\n\n \n\nSecond\n\n\n"));
    await screen.findByText(/First/);
    expect(prose()).toBe("First\n\nSecond");
  });

  it("keeps a single paragraph break as it was", async () => {
    open(withText("First\n\nSecond"));
    await screen.findByText(/First/);
    expect(prose()).toBe("First\n\nSecond");
  });

  it("clamps to 4 lines on desktop and 2 on phones", () => {
    expect(rule(".task-prose:not([data-open])")).toContain("-webkit-line-clamp: 4");
    expect(phone).toContain(".task-prose:not([data-open]) { -webkit-line-clamp: 2; }");
  });
});

describe("narrow phones and targets", () => {
  it("below 360px: tab gap 12px, tab padding 4px, no Work log hours pill", () => {
    expect(small).toContain(".task-tabs { gap: 12px; }");
    expect(small).toContain('.task-tabs [role="tab"] { padding: 0 4px; }');
    expect(small).toContain('.task-tab-count[data-tab="work"] { display: none; }');
  });

  it("the tablist can never be wider than its container", () => {
    const r = rule(".task-tabs");
    for (const s of ["flex-wrap: nowrap", "min-width: 0", "max-width: 100%", "overflow-x: clip"]) expect(r).toContain(s);
  });

  it("the tab pills carry data-tab so the narrow rule hits only the Work log one", async () => {
    const days = { key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days: [day()] };
    open(actions({ loadTicketBlocks: mock(async () => ({ ok: true as const, data: days })) }));
    await screen.findByText("1h 30m", { selector: '.task-tab-count[data-tab="work"]' });
    expect(document.querySelector('.task-tab-count[data-tab="comments"]')).toBeTruthy();
  });

  it("desktop targets: day trigger 36px, more and copy-key 36x36 (coarse stays 40)", () => {
    expect(rule(".task-day-trigger")).toContain("min-height: 36px");
    expect(css).toContain("justify-content: center; width: 36px; height: 36px;");
    expect(css).toContain(".task-icon-btn { min-width: 40px; min-height: 40px; }");
  });
});
