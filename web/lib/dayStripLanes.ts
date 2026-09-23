// Pure helpers for the expanded "lanes" view and legend-focus interaction —
// split out of lib/dayStrip.ts so that module stays under the size guard.

import { isSlateKey, type LegendEntry, type TrackSegment } from "./dayStrip";

export interface Lane {
  key: string;
  label: string;
  totalSeconds: number;
  hue: number | null;
  slate: boolean;
  segments: TrackSegment[];
}

/** One row per project for the expanded "lanes" view, in the FULL
 * (uncompacted) legend's order — every project gets its own lane.
 * personal + unassigned collect into a single "Other" lane appended
 * last. Away isn't a lane — gaps render as a band behind all lanes
 * instead. Lanes with no segments are dropped. */
export function buildLanes(segments: TrackSegment[], fullLegend: LegendEntry[]): Lane[] {
  const blockSegs = segments.filter(
    (s): s is Extract<TrackSegment, { kind: "block" }> => s.kind === "block",
  );
  const lanes: Lane[] = [];
  for (const e of fullLegend) {
    if (e.away || e.slate) continue;
    const segs = blockSegs.filter((s) => s.key === e.key);
    if (segs.length === 0) continue;
    lanes.push({ key: e.key, label: e.label, totalSeconds: e.totalSeconds, hue: e.hue, slate: false, segments: segs });
  }
  const other = fullLegend.filter((e) => e.slate && !e.away);
  if (other.length > 0) {
    const keys = new Set(other.map((e) => e.key));
    const segs = blockSegs.filter((s) => keys.has(s.key));
    if (segs.length > 0) {
      lanes.push({
        key: "other",
        label: "Other",
        totalSeconds: other.reduce((a, e) => a + e.totalSeconds, 0),
        hue: null,
        slate: true,
        segments: segs,
      });
    }
  }
  return lanes;
}

/** Whether a block segment (by its project key) should read as focused
 * while `focusKey` (a legend row's key) is hovered/keyboard-focused.
 * `null` focusKey means nothing is focused — everything reads normal.
 * "+N more" focuses every folded project (by label, which for real
 * projects is the same string as the key); "Other" focuses personal
 * and unassigned. */
export function isFocused(segKey: string, focusKey: string | null, legend: LegendEntry[]): boolean {
  if (!focusKey) return true;
  if (segKey === focusKey) return true;
  const entry = legend.find((e) => e.key === focusKey);
  if (!entry) return false;
  if (entry.key === "more") return (entry.title ?? "").split(", ").includes(segKey);
  if (entry.key === "other") return isSlateKey(segKey);
  return false;
}
