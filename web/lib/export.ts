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

/** Constants the invoicing form always gets, mirroring `billing.rs`. */
export const TEGUND_SKRANINGAR = "Almenn skráning";
export const TAXTI = "Dagvinna";
export const REIKNINGSHAEFT = "Reikningshæft";
export const OREIKNINGSHAEFT = "Óreikningshæft";
/** Shown where a value could not be resolved and the user must pick it. */
export const BLANK = "—";

export function reikningshaefi(billable: boolean): string {
  return billable ? REIKNINGSHAEFT : OREIKNINGSHAEFT;
}

/** `Dagsetning` as the form wants it: dd.mm.yyyy from an ISO day. */
export function formatFormDate(day: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  // Parsed by regex rather than `new Date` so a UTC-vs-local shift can
  // never move the billed date by a day.
  return m ? `${m[3]}.${m[2]}.${m[1]}` : day;
}

/** One row of the invoicing form, as the wizard renders it. */
export interface FormField {
  label: string;
  value: string;
  /** True when the user must pick this in the form (nothing to read off). */
  missing: boolean;
  /** True for typed fields — the only ones worth a copy button. */
  copyable: boolean;
}

/**
 * A billing row as the invoicing form's fields, **in the form's own
 * order** so the user reads straight down while filling it in.
 *
 * `Tengiliður`, `Rukka fyrir akstur`, `Útkall á bakvakt` and `External
 * Id` are deliberately absent — this user never fills them.
 */
export function formFields(row: BillingRow): FormField[] {
  return [
    {
      label: "Dagsetning",
      value: formatFormDate(row.day),
      missing: false,
      copyable: false,
    },
    {
      label: "Viðskiptamaður",
      value: row.customer ?? BLANK,
      missing: row.customer === null,
      copyable: false,
    },
    {
      label: "Verkefni (deild)",
      value: row.verkefni ?? BLANK,
      missing: row.verkefni === null,
      copyable: false,
    },
    {
      label: "Tegund skráningar",
      value: TEGUND_SKRANINGAR,
      missing: false,
      copyable: false,
    },
    { label: "Taxti", value: TAXTI, missing: false, copyable: false },
    {
      label: "Tímar",
      value: formatExportHours(row.hours),
      missing: false,
      copyable: true,
    },
    {
      label: "Reikningshæfi",
      value: reikningshaefi(row.billable),
      missing: false,
      copyable: false,
    },
    {
      label: "Texti á reikning",
      value: row.invoice_text,
      missing: false,
      copyable: true,
    },
  ];
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
