// Ticket dialog: Tab stays inside it.

import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { actions, dialog, open } from "./taskModalTestKit";

afterEach(cleanup);

/** Everything Tab can reach inside the dialog, in order (the roving tabs count only when selected). */
const tabbable = () =>
  [
    ...dialog().querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((el) => !el.closest("details:not([open])") || el.tagName === "SUMMARY");

describe("focus trap", () => {
  it("Tab on the last control wraps to the first", async () => {
    open(actions());
    const items = tabbable();
    const last = items[items.length - 1];
    last.focus();
    expect(fireEvent.keyDown(last, { key: "Tab" })).toBe(false);
    expect(document.activeElement).toBe(items[0]);
    await screen.findByText(/Steps to reproduce/);
  });

  it("Shift+Tab on the first control wraps to the last", async () => {
    open(actions());
    const items = tabbable();
    items[0].focus();
    expect(fireEvent.keyDown(items[0], { key: "Tab", shiftKey: true })).toBe(false);
    expect(document.activeElement).toBe(items[items.length - 1]);
    await screen.findByText(/Steps to reproduce/);
  });

  it("Shift+Tab from the dialog itself (where focus starts) goes to the last control", async () => {
    open(actions());
    expect(document.activeElement).toBe(dialog());
    fireEvent.keyDown(dialog(), { key: "Tab", shiftKey: true });
    const items = tabbable();
    expect(document.activeElement).toBe(items[items.length - 1]);
    await screen.findByText(/Steps to reproduce/);
  });

  it("leaves Tab alone in the middle, for the browser to move", async () => {
    open(actions());
    const items = tabbable();
    items[1].focus();
    expect(fireEvent.keyDown(items[1], { key: "Tab" })).toBe(true);
    expect(fireEvent.keyDown(items[1], { key: "Tab", shiftKey: true })).toBe(true);
    await screen.findByText(/Steps to reproduce/);
  });

  it("ignores controls folded away inside a closed details", async () => {
    open(actions());
    const details = document.querySelector("details") as HTMLDetailsElement;
    details.removeAttribute("open");
    const items = tabbable();
    items[items.length - 1].focus();
    fireEvent.keyDown(items[items.length - 1], { key: "Tab" });
    expect(document.activeElement).toBe(items[0]);
    expect(items.every((el) => !details.contains(el) || el.tagName === "SUMMARY")).toBe(true);
    await screen.findByText(/Steps to reproduce/);
  });
});
