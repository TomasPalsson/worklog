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
