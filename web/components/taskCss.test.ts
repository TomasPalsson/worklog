import { expect, it } from "bun:test";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
const rule = (selector: string) => css.split("\n").find((l) => l.startsWith(`${selector} {`)) ?? "";

it("selected-card stripes never compose with --elevation-1 (it is `none` in dark, which voids the whole shadow)", () => {
  for (const sel of [
    ".task-card[data-selected]",
    '.task-card[data-selected][data-column="indeterminate"]',
    '.task-card[data-selected][data-column="done"]',
  ]) {
    const r = rule(sel);
    expect(r).toContain("inset 2px 0 0");
    expect(r).not.toContain("--elevation-1");
  }
});

it("the focus ring rule still comes after the selected rules so it wins while focused", () => {
  expect(css.indexOf(".task-card:has(> .task-card-btn:focus-visible)")).toBeGreaterThan(
    css.indexOf('.task-card[data-selected][data-column="done"]'),
  );
});

it("the dialog scrim is defined in the base :root and in both dark blocks", () => {
  expect(css.match(/--scrim:/g)?.length).toBe(3);
  expect(css).toContain("--scrim: oklch(0.15 0.01 85 / 0.45);");
  expect(css.match(/--scrim: oklch\(0\.05 0 0 \/ 0\.6\);/g)?.length).toBe(2);
  expect(css.match(/--elevation-modal: none;/g)?.length).toBe(2);
});

it("the dialog is a centred, capped box that goes full-screen on small screens", () => {
  expect(css).toContain("width: min(1080px, calc(100vw - 48px));");
  expect(css).toContain("height: min(860px, calc(100vh - 48px));");
  expect(css).toContain("grid-template-columns: minmax(0, 1fr) 320px;");
  expect(css).toContain("@media (max-width: 759px)");
  expect(css).toContain("height: 100dvh;");
});

it("dialog motion is switched off for reduced motion", () => {
  expect(css).toContain(".task-card, .task-modal, .task-modal-backdrop { transition: none; animation: none; }");
});
