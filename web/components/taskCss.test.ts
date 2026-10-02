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
  expect(coarse).toContain('.task-day-trigger, .task-log-open, .task-tabs [role="tab"], .task-more { min-height: 40px; }');
});

it("the description clamps to four lines, and a busy status button shows the progress cursor", () => {
  expect(css).toContain(".task-prose:not([data-open]) { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 4; overflow: hidden; }");
  expect(css).not.toContain("mask-image: linear-gradient(to bottom, #000 calc(100% - 48px)"); // no ghost line, no fade
  expect(rule(".task-status:disabled, .task-menu button:disabled, .task-chooser button:disabled, .task-move-menu button:disabled")).toContain("cursor: progress");
});

it("day rows keep label, hours and chip on one line and wrap the actions under them on phones", () => {
  for (const sel of ["task-day-label", "task-day-hours", "task-day-chip"]) expect(css).toMatch(new RegExp(`\\.${sel}[^{]*\{[^}]*white-space: nowrap`));
  const phone = css.slice(css.indexOf("/* day rows: line 1"));
  expect(phone).toContain(".task-day-row { flex-wrap: wrap; justify-content: flex-end;");
  expect(phone).toContain(".task-day-head { flex: 1 0 100%; }");
});

const phoneRows = phone.slice(phone.indexOf("/* day rows: line 1"));

it("phone day row: line 1 never wraps (short chip), line 2 is a full-width action beside the menu", () => {
  expect(phoneRows).toContain(".task-day-toggle { flex-wrap: nowrap; gap: 6px; padding: 0 4px; min-width: 0; }");
  expect(phoneRows).not.toMatch(/\.task-day-chip\s*\{[^}]*text-overflow/);
  expect(phoneRows).toContain(".task-day-trigger { flex: 1 1 0; min-width: 0; margin-left: 24px; }");
  expect(rule(".task-day-head")).toContain("min-width: 0"); // the head can shrink inside the row
  expect(rule(".task-day-menu-wrap")).toContain("flex: none"); // the menu keeps its size; the button takes the rest
});

it("phone filter fills the row instead of a fixed 280px; tabs never wrap", () => {
  expect(phoneRows).toContain(".task-filter { flex: 1 1 100%; min-width: 0; }");
  expect(phoneRows).toContain(".task-filter input { width: 100%; }");
  expect(css).toContain('.task-tabs [role="tab"] { white-space: nowrap; }');
});

it("the day's send action is the primary button, compact: 36px, 40px on touch", () => {
  expect(rule(".task-day-trigger")).toContain("min-height: 36px");
  expect(rule(".task-log-open, .task-day-older")).toContain("min-height: 28px");
});

it("phone block rows stack: time and length, then the clamped description, chevron centred on the right", () => {
  expect(phoneRows).toContain(".task-block-range { grid-column: 1; grid-row: 1; color: var(--fg-muted); }");
  expect(phoneRows).toContain(".task-block-desc { grid-column: 1 / 4; grid-row: 2; }");
  expect(phoneRows).toContain(".task-block-go { grid-column: 4; grid-row: 1 / 3; }");
  expect(phoneRows).toContain('.task-block-dur::before { content: "· "; }');
  expect(rule(".task-block-desc")).toContain("-webkit-line-clamp: 2");
  expect(rule(".task-block-range, .task-block-dur")).toContain("font-size: 12px");
});

it("Show more is a plain 13px muted text button that underlines on hover", () => {
  const more = css.slice(css.indexOf(".task-more {"), css.indexOf(".task-more:focus-visible"));
  expect(more).toContain("font-size: 13px");
  expect(more).toContain("color: var(--fg-muted)");
  expect(more).toContain(".task-more:hover { text-decoration: underline; }");
});

it("tab count pills use full-strength text on the sunk fill (contrast), not muted", () => {
  const r = rule(".task-tab-count");
  expect(r).toContain("background: var(--bg-sunk)");
  expect(r).toContain("color: var(--fg);");
});

it("the change-notice slip is hidden while the ticket dialog is open, and only then", () => {
  expect(css).toContain('body:has(.task-modal[role="dialog"][aria-modal="true"]) .chg-slip { display: none; }');
  expect(css.match(/\.chg-slip[^{]*\{[^}]*display: none/g)?.length).toBe(1); // no other rule hides it
});

it("the phone Jira link is a 40px icon with a right-anchored tooltip", () => {
  expect(css).toContain(".task-modal-jira-icon { position: relative; justify-content: center; width: 40px; height: 40px;");
  expect(css).toContain(".task-modal-jira-icon[data-tip]::after {");
});

it("the Tempo state text may wrap in the sidebar, but '· Show unsent day' never splits", () => {
  expect(rule(".task-tempo-line")).not.toContain("white-space: nowrap");
  expect(rule(".task-tempo-act")).toContain("white-space: nowrap");
});
