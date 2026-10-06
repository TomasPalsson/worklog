// Read-back row after a hand send (spec 018, FR-14, B5).

import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { PreflightRow } from "@/lib/daily_helpers_contract";
import { ReadBackRow } from "./ReadBackRow";

afterEach(cleanup);

const match: PreflightRow = { check: "read_back", ok: true, detail: "2026-10-01: Tempo matches (3600s)", target: null };
const mismatch: PreflightRow = { check: "read_back", ok: false, detail: "2026-10-01: sent 3600s, Tempo has 1800s", target: "2026-10-01" };

describe("ReadBackRow", () => {
  it("shows a match as green", () => {
    const { container } = render(<ReadBackRow row={match} />);
    // catches: matching read-back drawn red, or detail dropped
    expect(screen.getByText(match.detail)).toBeTruthy();
    expect(container.querySelector(".task-check-ok")).toBeTruthy();
    expect(container.querySelector(".task-check-red")).toBeNull();
  });

  it("shows a mismatch as red with the day and both totals", () => {
    const { container } = render(<ReadBackRow row={mismatch} />);
    // catches: always-green row, or a generic message without day and totals
    expect(container.querySelector(".task-check-red")).toBeTruthy();
    expect(container.querySelector(".task-check-ok")).toBeNull();
    const text = screen.getByText(mismatch.detail).textContent ?? "";
    expect(text).toContain("2026-10-01");
    expect(text).toContain("3600s");
    expect(text).toContain("1800s");
  });
});
