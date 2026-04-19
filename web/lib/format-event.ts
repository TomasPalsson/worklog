// Formatting helpers for the per-block events drill-down.
//
// Kept separate from `./format.ts` because those are block-oriented
// (durations, day headers) while these are event-oriented (timestamps,
// source glyphs, details truncation).

import type { SourceKind } from "./types";

/**
 * Render an event's `started_at` as a short local time — `HH:MM`.
 * Drill-downs render a rapid-fire list; seconds and the full date add
 * noise without signal.
 */
export function formatEventTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

/**
 * Cap `details` at `maxChars` for the collapsed preview, preserving a
 * visible marker so readers know there's more to expand. Char-counting
 * (not byte-counting) so UTF-8 emoji / non-ASCII stay intact.
 */
export function previewDetails(details: string | null, maxChars: number = 160): {
  preview: string;
  truncated: boolean;
} {
  if (!details) return { preview: "", truncated: false };
  const total = [...details].length;
  if (total <= maxChars) return { preview: details, truncated: false };
  const preview = [...details].slice(0, maxChars).join("");
  return { preview: `${preview}…`, truncated: true };
}

/**
 * Human-readable source label shown next to each event row. Mirrors
 * the display buckets in `./types.ts::sourceKind`, but adds words — the
 * drill-down has room for "GitHub" and "Claude" where the badge only
 * had room for a glyph.
 */
export function sourceLabel(kind: SourceKind): string {
  switch (kind) {
    case "github":
      return "GitHub";
    case "claude":
      return "Claude";
    case "gcal":
      return "Calendar";
    case "jira":
      return "Jira";
    default:
      return "other";
  }
}
