// Pure helpers for the lanes view's saved splits — where each saved range
// sits on the track and who got what, with the same fully-inside-the-window
// clipping convention `overlapsInWindow` uses.

import type { TrackWindow } from "./dayStrip";
import type { SavedAllocation } from "./types";

export interface AllocationBand {
  allocation: SavedAllocation;
  leftPct: number;
  widthPct: number;
}

/** `[project, whole percent]` pairs, the biggest share first. */
export function sharesByLargest(shares: Record<string, number>): [string, number][] {
  return Object.entries(shares)
    .map(([p, f]): [string, number] => [p, Math.round(f * 100)])
    .sort((a, b) => b[1] - a[1]);
}

export function buildAllocationBands(
  allocations: SavedAllocation[],
  window: TrackWindow,
): AllocationBand[] {
  const totalMs = window.endMs - window.startMs;
  if (totalMs <= 0) return [];
  const pct = (ms: number) => ((ms - window.startMs) / totalMs) * 100;
  return allocations
    .filter((a) => {
      const s = new Date(a.started_at).getTime();
      const e = new Date(a.ended_at).getTime();
      return s >= window.startMs && e <= window.endMs;
    })
    .map((allocation) => {
      const startMs = new Date(allocation.started_at).getTime();
      const endMs = new Date(allocation.ended_at).getTime();
      return {
        allocation,
        leftPct: pct(startMs),
        widthPct: pct(endMs) - pct(startMs),
      };
    });
}
