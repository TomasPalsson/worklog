// Pure helpers for the billing-export panel.
//
// The daemon owns ALL rendering (text/csv/json come pre-rendered from
// `GET /export/:day` so the Rust renderers stay the single source of
// truth). What lives here is only what the browser needs on top: the
// day total, download filenames/mime types, and the blob-download
// mechanic. Keeping it dependency-free means it unit-tests without a
// DOM or a Next runtime.

import type { BillingRow } from "./types";

/**
 * Sum of a day's billed hours. Each row's `hours` is already
 * half-hour-rounded daemon-side, so this is a plain sum — deliberately
 * NOT a re-round of the summed seconds, which would disagree with the
 * per-row figures the user is reading (and billing from).
 */
export function totalBilledHours(rows: BillingRow[]): number {
  return rows.reduce((acc, r) => acc + r.hours, 0);
}

/**
 * Render hours the way the text/CSV export does — comma decimal, no
 * trailing `,0` on whole hours (`4`, `5,5`). Mirrors the Rust
 * `format_hours` helper in `billing.rs`; kept in step with it so the
 * on-screen table matches what Copy puts on the clipboard.
 */
export function formatExportHours(hours: number): string {
  const halves = Math.round(hours * 2);
  const whole = Math.floor(halves / 2);
  return halves % 2 === 1 ? `${whole},5` : `${whole}`;
}

export type ExportFormat = "text" | "csv" | "json";

/** Download filename for a day's export, e.g. `worklog-2026-07-23.csv`. */
export function exportFilename(day: string, format: ExportFormat): string {
  const ext = format === "text" ? "txt" : format;
  return `worklog-${day}.${ext}`;
}

/** MIME type per export format (charset pinned so Excel opens UTF-8 CSV). */
export function exportMime(format: ExportFormat): string {
  switch (format) {
    case "csv":
      return "text/csv;charset=utf-8";
    case "json":
      return "application/json;charset=utf-8";
    default:
      return "text/plain;charset=utf-8";
  }
}

/**
 * Trigger a client-side file download. Revokes the object URL on the
 * next tick — Safari cancels the download if the URL dies synchronously.
 */
export function downloadText(filename: string, mime: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
