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

const phone = css.slice(css.indexOf("@media (max-width: 759px)"), css.indexOf("@media (pointer: coarse)"));

it("on phones the body is the one scroller: the columns neither scroll nor have a fixed height", () => {
  expect(phone).toContain(".task-modal-body { display: flex; flex-direction: column; gap: 16px; overflow-x: hidden; overflow-y: auto;");
  expect(phone).toContain(".task-modal-main, .task-modal-side { display: grid; gap: 16px; min-width: 0; height: auto; overflow: visible; }");
  expect(phone).toContain(".task-headline { font-size: 22px; line-height: 1.2; }");
});

it("touch screens get 40px targets for the small icon, day, Log time and tab buttons", () => {
  const coarse = css.slice(css.indexOf("@media (pointer: coarse)"));
  expect(coarse).toContain(".task-icon-btn { min-width: 40px; min-height: 40px; }");
  expect(coarse).toContain('.task-day-trigger, .task-log-open, .task-tabs [role="tab"] { min-height: 40px; }');
});

it("the description clamps to four lines, and a busy status button shows the progress cursor", () => {
  expect(css).toContain(".task-prose:not([data-open]) { max-height: calc(14px * 1.6 * 4); overflow: hidden; }");
  expect(rule(".task-status:disabled, .task-menu button:disabled, .task-chooser button:disabled, .task-move-menu button:disabled")).toContain("cursor: progress");
});

it("day rows keep label, hours and chip on one line and wrap the actions under them on phones", () => {
  for (const sel of ["task-day-label", "task-day-hours", "task-day-chip"]) expect(css).toMatch(new RegExp(`\\.${sel}[^{]*\{[^}]*white-space: nowrap`));
  const phone = css.slice(css.indexOf("/* day rows: line 1"));
  expect(phone).toContain(".task-day-row { flex-wrap: wrap; justify-content: flex-end;");
  expect(phone).toContain(".task-day-head { flex: 1 0 100%; }");
});

it("the phone Jira link is a 40px icon with a right-anchored tooltip", () => {
  expect(css).toContain(".task-modal-jira-icon { position: relative; justify-content: center; width: 40px; height: 40px;");
  expect(css).toContain(".task-modal-jira-icon[data-tip]::after {");
});

it("the Tempo value and its Review button never split across lines", () => {
  expect(rule(".task-tempo-line")).toContain("white-space: nowrap");
});
