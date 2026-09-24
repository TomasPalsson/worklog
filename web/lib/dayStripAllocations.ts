// Pure helpers for the lanes view's saved-allocation brackets — a thin
// strip above the tracks showing "you set 90/10" for any window the owner
// has manually split (drag-selected or from an overlap band), with the
// same fully-inside-the-window clipping convention `overlapsInWindow` uses.

import type { TrackWindow } from "./dayStrip";
import type { SavedAllocation } from "./types";

export interface AllocationBand {
  allocation: SavedAllocation;
  leftPct: number;
  widthPct: number;
  ratioLabel: string;
}

/** "you set 90/10" — shares' percentages, largest first. */
export function formatSharesRatio(shares: Record<string, number>): string {
  const pcts = Object.values(shares)
    .map((f) => Math.round(f * 100))
    .sort((a, b) => b - a);
  return `you set ${pcts.join("/")}`;
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
        ratioLabel: formatSharesRatio(allocation.shares),
      };
    });
}
