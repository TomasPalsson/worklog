import { expect, it } from "bun:test";

import { timeoutMs } from "./daemonTimeout";

it("a ticket line's Generate/Regenerate waits long enough for one thinking AI call", () => {
  expect(timeoutMs("/tempo/lines/regenerate")).toBe(120_000);
  expect(timeoutMs("/tempo/lines/text")).toBe(10_000); // plain writes stay quick
});

it("keeps the existing per-route limits", () => {
  expect(timeoutMs("/blocks/3/estimate")).toBe(600_000);
  expect(timeoutMs("/tickets/A-1/draft")).toBe(90_000);
  expect(timeoutMs("/days/2026-10-02")).toBe(10_000);
});
