// Work log tab: day rows (chips, disclosure, hours note), clamp toggle, older days, copy and styles.

import { afterEach, describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { block, btn, btns, day, logTime, more, open, settle, toggle } from "./workLogTestKit";

afterEach(cleanup);

describe("day row", () => {
  it("tones the chips: unsynced slate, changed amber, synced sage", async () => {
    await open([
      day({ day: "2026-10-03", blocks: [block({ tempo_worklog_id: "a" })] }),
      day({ day: "2026-10-02", blocks: [block({ tempo_worklog_id: "a", dirty: true })] }),
      day({ day: "2026-10-01" }),
    ]);
    expect(screen.getByText("In Tempo").getAttribute("data-chip")).toBe("ok");
    expect(screen.getByText("Changed since sent").getAttribute("data-chip")).toBe("changed");
    expect(screen.getByText("Not in Tempo").getAttribute("data-chip")).toBe("none");
  });

  it("is one line: chevron, label, plain hours and chip in the disclosure button, then the actions", async () => {
    await open();
    const row = document.querySelector(".task-day-row") as HTMLElement;
    const t = toggle();
    expect(t.parentElement?.tagName).toBe("H4");
    expect(t.querySelector(".task-day-chev")?.getAttribute("aria-hidden")).toBe("true");
    expect([...t.children].map((c) => c.textContent)).toEqual(["", "Thu 1 Oct", "1h 30m", "Not in Tempo"]);
    // hours are text now; editing them lives in the menu
    expect(row.querySelector(".task-day-hours")?.tagName).toBe("SPAN");
    expect(row.querySelector(".task-day-hours")?.hasAttribute("title")).toBe(false);
    expect(more().textContent).toBe("");
  });

  it("block rows end in a chevron", async () => {
    await open();
    expect(document.querySelector(".task-block-row .task-block-go")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("the day label sits in an h4 and the preview heading is an h5", async () => {
    await open();
    expect(document.querySelector("h4 .task-day-label")?.textContent).toBe("Thu 1 Oct");
    fireEvent.click(btn("Send to Tempo, Thu 1 Oct"));
    expect((await screen.findByText("Preview — nothing sent yet")).tagName).toBe("H5");
  });

  it("the summary sits in the head line, even with the form open", async () => {
    await open();
    const head = () => (document.querySelector(".task-work-head") as HTMLElement).textContent;
    expect(head()).toContain("1h 30m over 1 day · last 14 days");
    fireEvent.click(logTime());
    expect(head()).toContain("1h 30m over 1 day · last 14 days");
  });

  it("the next action sits right after the day, named Send; Update once it changed", async () => {
    await open([
      day({ day: "2026-10-02", blocks: [block({ tempo_worklog_id: "a", dirty: true })] }),
      day({ day: "2026-10-01" }),
    ]);
    const send = btn("Send to Tempo, Thu 1 Oct");
    expect(send.textContent).toBe("Send to Tempo");
    expect(screen.getByText("Not in Tempo").closest("h4")?.nextElementSibling).toBe(send);
    expect(btn("Update Tempo, Fri 2 Oct").textContent).toBe("Update Tempo");
  });

  it("an In Tempo day has no send action", async () => {
    await open([day({ blocks: [block({ tempo_worklog_id: "a" })] })]);
    expect(btns(/^Preview /)).toHaveLength(0);
    expect(more()).toBeTruthy();
  });
});

describe("disclosure", () => {
  const two = [day({ day: "2026-10-02", blocks: [block({ id: 1, day: "2026-10-02" })] }), day({ day: "2026-10-01", blocks: [block({ id: 2 })] })];

  it("opens the newest day and folds the rest", async () => {
    await open(two);
    expect(toggle("Fri 2 Oct").getAttribute("aria-expanded")).toBe("true");
    expect(toggle("Thu 1 Oct").getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelectorAll(".task-day-body")).toHaveLength(1);
    expect(document.querySelectorAll(".task-block-row")).toHaveLength(1);
  });

  it("clicking a row opens or folds it, and its neighbours stay as they were", async () => {
    await open(two);
    fireEvent.click(toggle("Thu 1 Oct"));
    expect(toggle("Thu 1 Oct").getAttribute("aria-expanded")).toBe("true");
    expect(document.querySelectorAll(".task-block-row")).toHaveLength(2);
    fireEvent.click(toggle("Fri 2 Oct"));
    expect(toggle("Fri 2 Oct").getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelectorAll(".task-block-row")).toHaveLength(1);
  });
});

describe("hours note", () => {
  const note = (text: string) => screen.queryByText(text);

  it("hand-set hours", async () => {
    await open([day({ hours_set_by_hand: true, line_seconds: 5400, tracked_seconds: 2400 })]);
    expect(note("Set by hand · 40m tracked")).toBeTruthy();
  });

  it("rounded hours", async () => {
    await open([day({ line_seconds: 1800, tracked_seconds: 2400 })]);
    expect(note("Rounded to the nearest half hour from 40m tracked")).toBeTruthy();
  });

  it("no note when hours equal the tracked time", async () => {
    await open([day()]);
    expect(document.querySelector(".task-day-note")).toBeNull();
  });

  it("states the rounding rule", async () => {
    await open([day({ line_seconds: 1800, tracked_seconds: 6120 })]);
    expect(screen.getByText("Rounded to the nearest half hour from 1h 42m tracked")).toBeTruthy();
  });

  it("is part of the open day, not the row", async () => {
    await open([day({ line_seconds: 1800, tracked_seconds: 2400 })]);
    fireEvent.click(toggle());
    expect(note("Rounded to the nearest half hour from 40m tracked")).toBeNull();
  });
});

describe("clamp toggle", () => {
  const measure = (scroll: number, client: number) => {
    const proto = HTMLElement.prototype;
    Object.defineProperty(proto, "scrollHeight", { configurable: true, get: () => scroll });
    Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => client });
  };
  afterEach(() => {
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
  });

  it("shows more only when the text overflows, regardless of length", async () => {
    measure(60, 36);
    await open([day({ line_text: "short" })]);
    await settle();
    expect(btns("more")).toHaveLength(1); // measured in an effect, flushed async
    cleanup();
    measure(36, 36);
    await open([day({ line_text: "x".repeat(400) })]);
    await new Promise((r) => setTimeout(r, 20)); // let the same effect run before asserting absence
    expect(btns("more")).toHaveLength(0);
  });
});

describe("older days", () => {
  const days = (n: number) =>
    Array.from({ length: n }, (_, i) => {
      const d = `2026-10-${String(n - i).padStart(2, "0")}`;
      return day({ day: d, blocks: [block({ id: i + 1, day: d })] });
    });

  it("shows the newest 5 and expands the rest", async () => {
    await open(days(7));
    expect(document.querySelectorAll(".task-day")).toHaveLength(5);
    fireEvent.click(btn("Show 2 older days"));
    expect(document.querySelectorAll(".task-day")).toHaveLength(7);
    expect(btns(/older day/)).toHaveLength(0);
  });
});

describe("styles", () => {
  const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");

  it("light --fg-subtle is 0.54 in the base :root and nothing hard-codes the old value", () => {
    expect(css).toContain("--fg-subtle:  oklch(0.54 0.01 260);");
    expect(css).not.toContain("oklch(0.58 0.01 260)");
  });

  it("pressed chip is inverted and the buttons have hover states", () => {
    expect(css).toContain('[aria-pressed="true"] { background: var(--fg); border-color: var(--fg); color: var(--bg);');
    expect(css).toContain(".task-btn-primary:hover:not(:disabled)");
    expect(css).toContain(".task-btn-secondary:hover:not(:disabled) { background: var(--bg-sunk); }");
    expect(css).toContain("transition: background-color 120ms cubic-bezier(0.25, 1, 0.5, 1)");
  });
});
