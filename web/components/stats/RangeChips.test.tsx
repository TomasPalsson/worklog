import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { RangeChips, parseRange, rangeDates } from "./RangeChips";

afterEach(cleanup);

describe("rangeDates", () => {
  it("all sends no bounds", () => expect(rangeDates("all", "2026-10-09")).toEqual({}));
  it("7d is today and the six days before", () =>
    expect(rangeDates("7d", "2026-10-09")).toEqual({ from: "2026-10-03", to: "2026-10-09" }));
  it("crosses month and year ends", () => {
    expect(rangeDates("30d", "2026-01-10")).toEqual({ from: "2025-12-12", to: "2026-01-10" });
    expect(rangeDates("90d", "2026-03-01").from).toBe("2025-12-02");
  });
});

describe("parseRange", () => {
  it("falls back to all", () => {
    expect(parseRange(undefined)).toBe("all");
    expect(parseRange("1y")).toBe("all");
    expect(parseRange("30d")).toBe("30d");
  });
});

describe("RangeChips", () => {
  it("links each range and marks only the current one", () => {
    const { container } = render(<RangeChips current="30d" />);
    expect(screen.getByRole("link", { name: "7 days" }).getAttribute("href")).toBe("/stats?range=7d");
    expect(screen.getByRole("link", { name: "All time" }).getAttribute("href")).toBe("/stats");
    const cur = [...container.querySelectorAll('[aria-current="page"]')].map((e) => e.textContent);
    expect(cur).toEqual(["30 days"]);
  });
});
