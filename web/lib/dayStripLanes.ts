// Pure helpers for the expanded "lanes" view and legend-focus interaction —
// split out of lib/dayStrip.ts so that module stays under the size guard.

import { isSlateKey, type LegendEntry, type TrackSegment, type TrackWindow } from "./dayStrip";
import type { ProjectActivity } from "./types";

/** Percentage-positioned faint fill showing a project's FULL activity
 * (not just the minutes it owns) — see `ProjectActivity` on the day
 * summary. */
export interface ActivityBand {
  leftPct: number;
  widthPct: number;
}

export interface Lane {
  key: string;
  label: string;
  totalSeconds: number;
  hue: number | null;
  slate: boolean;
  segments: TrackSegment[];
  /** Empty when no activity data matched this lane's key (older daemon,
   * or the "Other" lane, which never gets one — personal/unassigned
   * activity isn't tracked server-side). */
  activityBands: ActivityBand[];
  /** Sum of this lane's activity spans, for the "· active Xh Ym" label. */
  activitySeconds: number;
}

/** This project's activity spans, clipped to `window` (fully-inside, same
 * convention `overlapsInWindow` uses) and converted to percentage bands. */
function activityBandsFor(
  key: string,
  activity: ProjectActivity[],
  window: TrackWindow,
): { bands: ActivityBand[]; seconds: number } {
  const totalMs = window.endMs - window.startMs;
  const entry = activity.find((a) => a.project === key);
  if (!entry || totalMs <= 0) return { bands: [], seconds: 0 };
  const pct = (ms: number) => ((ms - window.startMs) / totalMs) * 100;
  const bands: ActivityBand[] = [];
  let seconds = 0;
  for (const span of entry.spans) {
    const startMs = new Date(span.started_at).getTime();
    const endMs = new Date(span.ended_at).getTime();
    if (startMs < window.startMs || endMs > window.endMs) continue;
    bands.push({ leftPct: pct(startMs), widthPct: pct(endMs) - pct(startMs) });
    seconds += (endMs - startMs) / 1000;
  }
  return { bands, seconds };
}

/** One row per project for the expanded "lanes" view, in the FULL
 * (uncompacted) legend's order — every project gets its own lane.
 * personal + unassigned collect into a single "Other" lane appended
 * last. Away isn't a lane — gaps render as a band behind all lanes
 * instead. Lanes with no segments are dropped. `window`/`activity` are
 * optional so existing callers (and their fixtures) that don't care about
 * the activity fill keep working unchanged. */
export function buildLanes(
  segments: TrackSegment[],
  fullLegend: LegendEntry[],
  window?: TrackWindow,
  activity: ProjectActivity[] = [],
): Lane[] {
  const blockSegs = segments.filter(
    (s): s is Extract<TrackSegment, { kind: "block" }> => s.kind === "block",
  );
  const lanes: Lane[] = [];
  for (const e of fullLegend) {
    if (e.away || e.slate) continue;
    const segs = blockSegs.filter((s) => s.key === e.key);
    if (segs.length === 0) continue;
    const { bands, seconds } = window
      ? activityBandsFor(e.key, activity, window)
      : { bands: [], seconds: 0 };
    lanes.push({
      key: e.key,
      label: e.label,
      totalSeconds: e.totalSeconds,
      hue: e.hue,
      slate: false,
      segments: segs,
      activityBands: bands,
      activitySeconds: seconds,
    });
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
        activityBands: [],
        activitySeconds: 0,
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
