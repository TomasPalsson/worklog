import { describe, expect, it } from "bun:test";

import {
  exportFilename,
  exportMime,
  formatExportHours,
  totalBilledHours,
} from "./export";
import type { BillingRow } from "./types";

const row = (over: Partial<BillingRow> = {}): BillingRow => ({
  repo: "genai-infra",
  description: "Create MCP server",
  kind: "Work",
  seconds: 14400,
  hours: 4,
  ...over,
});

describe("totalBilledHours", () => {
  it("sums the per-row billed hours", () => {
    const rows = [row({ hours: 4 }), row({ hours: 5.5 }), row({ hours: 2 })];
    expect(totalBilledHours(rows)).toBe(11.5);
  });

  it("is 0 for an empty day", () => {
    expect(totalBilledHours([])).toBe(0);
  });
});

describe("formatExportHours", () => {
  it("renders whole hours without a decimal", () => {
    expect(formatExportHours(4)).toBe("4");
  });

  it("renders half hours with a comma decimal (matches the Rust renderer)", () => {
    expect(formatExportHours(5.5)).toBe("5,5");
  });

  it("renders zero as 0", () => {
    expect(formatExportHours(0)).toBe("0");
  });
});

describe("exportFilename", () => {
  it("uses .txt for the copy-paste text format", () => {
    expect(exportFilename("2026-07-23", "text")).toBe("worklog-2026-07-23.txt");
  });

  it("uses the format as the extension for csv/json", () => {
    expect(exportFilename("2026-07-23", "csv")).toBe("worklog-2026-07-23.csv");
    expect(exportFilename("2026-07-23", "json")).toBe("worklog-2026-07-23.json");
  });
});

describe("exportMime", () => {
  it("pins utf-8 so spreadsheets open the CSV correctly", () => {
    expect(exportMime("csv")).toBe("text/csv;charset=utf-8");
  });

  it("maps json and text", () => {
    expect(exportMime("json")).toBe("application/json;charset=utf-8");
    expect(exportMime("text")).toBe("text/plain;charset=utf-8");
  });
});
