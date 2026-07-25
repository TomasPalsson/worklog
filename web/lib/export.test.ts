import { describe, expect, it } from "bun:test";

import {
  BLANK,
  exportFilename,
  exportMime,
  formatExportHours,
  formatFormDate,
  formFields,
  OREIKNINGSHAEFT,
  TAXTI,
  TEGUND_SKRANINGAR,
  totalBilledHours,
} from "./export";
import type { BillingRow } from "./types";

const row = (over: Partial<BillingRow> = {}): BillingRow => ({
  day: "2026-07-23",
  folder: "genai-infra",
  customer: "Sjúkra",
  verkefni: null,
  ticket: "GENAI-1219",
  seconds: 14400,
  hours: 4,
  billable: true,
  invoice_text: "Create MCP server",
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

describe("formatFormDate", () => {
  it("renders dd.mm.yyyy for the form", () => {
    expect(formatFormDate("2026-07-23")).toBe("23.07.2026");
  });

  it("passes an unexpected value through untouched", () => {
    expect(formatFormDate("not-a-day")).toBe("not-a-day");
  });
});

describe("formFields", () => {
  it("lists the form's fields in the form's own order", () => {
    const labels = formFields(row()).map((f) => f.label);
    expect(labels).toEqual([
      "Dagsetning",
      "Viðskiptamaður",
      "Verkefni (deild)",
      "Tegund skráningar",
      "Taxti",
      "Tímar",
      "Reikningshæfi",
      "Texti á reikning",
    ]);
  });

  it("omits the fields this user never fills", () => {
    const labels = formFields(row()).map((f) => f.label);
    for (const skipped of [
      "Tengiliður",
      "Rukka fyrir akstur",
      "Útkall á bakvakt",
      "External Id",
    ]) {
      expect(labels).not.toContain(skipped);
    }
  });

  it("flags unresolved fields as missing and shows a dash", () => {
    const fields = formFields(row({ customer: null, verkefni: null }));
    const customer = fields.find((f) => f.label === "Viðskiptamaður")!;
    const verkefni = fields.find((f) => f.label === "Verkefni (deild)")!;
    expect(customer.missing).toBe(true);
    expect(customer.value).toBe(BLANK);
    expect(verkefni.missing).toBe(true);
  });

  it("marks only the typed fields copyable", () => {
    const copyable = formFields(row())
      .filter((f) => f.copyable)
      .map((f) => f.label);
    // Dropdowns are picked, not pasted — a copy button there is noise.
    expect(copyable).toEqual(["Tímar", "Texti á reikning"]);
  });

  it("fills the constants and the billability label", () => {
    const fields = formFields(row({ billable: false }));
    expect(fields.find((f) => f.label === "Tegund skráningar")!.value).toBe(
      TEGUND_SKRANINGAR,
    );
    expect(fields.find((f) => f.label === "Taxti")!.value).toBe(TAXTI);
    expect(fields.find((f) => f.label === "Reikningshæfi")!.value).toBe(
      OREIKNINGSHAEFT,
    );
  });

  it("renders Tímar with a comma decimal", () => {
    const fields = formFields(row({ hours: 5.5 }));
    expect(fields.find((f) => f.label === "Tímar")!.value).toBe("5,5");
  });
});
