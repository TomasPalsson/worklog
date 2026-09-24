// Pure helpers for the day strip's overlap bands + split popover. Split out
// of lib/dayStrip.ts (which stays project/gap layout only) so both modules
// keep their own concern and their own size budget.

import { formatDuration, formatRange } from "./format";
import type { TrackWindow } from "./dayStrip";
import type { Overlap } from "./types";

export interface OverlapBand {
  overlap: Overlap;
  leftPct: number;
  widthPct: number;
  label: string;
  /** Index range `[first, last]` of the lanes (in `laneKeys` order) this
   * overlap's projects touch, or `null` when none of them have a lane
   * (shouldn't happen — every overlap project has activity — but a
   * stale/late-arriving lane list shouldn't crash the render). */
  laneRange: [number, number] | null;
}

/** Overlaps whose window falls fully inside the track — mirrors how gaps
 * are filtered in DayStrip, so a stray overlap outside the rendered span
 * (e.g. from a block since deleted) never draws off the track. */
export function overlapsInWindow(overlaps: Overlap[], window: TrackWindow): Overlap[] {
  return overlaps.filter((o) => {
    const s = new Date(o.started_at).getTime();
    const e = new Date(o.ended_at).getTime();
    return s >= window.startMs && e <= window.endMs;
  });
}

/** Label shown on the band / popover header, e.g. "Overlap 10:45–13:54 · 3h 09m". */
export function overlapLabel(overlap: Overlap): string {
  return `Overlap ${formatRange(overlap.started_at, overlap.ended_at)} · ${formatDuration(overlap.minutes * 60)}`;
}

/** Percentage-positioned bands for the lanes view, one per overlap. */
export function buildOverlapBands(
  overlaps: Overlap[],
  window: TrackWindow,
  laneKeys: string[],
): OverlapBand[] {
  const totalMs = window.endMs - window.startMs;
  if (totalMs <= 0) return [];
  const pct = (ms: number) => ((ms - window.startMs) / totalMs) * 100;
  return overlapsInWindow(overlaps, window).map((overlap) => {
    const startMs = new Date(overlap.started_at).getTime();
    const endMs = new Date(overlap.ended_at).getTime();
    const indices = overlap.projects
      .map((p) => laneKeys.indexOf(p.project))
      .filter((i) => i !== -1);
    const laneRange: [number, number] | null =
      indices.length > 0 ? [Math.min(...indices), Math.max(...indices)] : null;
    return {
      overlap,
      leftPct: pct(startMs),
      widthPct: pct(endMs) - pct(startMs),
      label: overlapLabel(overlap),
      laneRange,
    };
  });
}

/** "Give all to X" — the whole window goes to one project. */
export function giveAllShares(project: string): Record<string, number> {
  return { [project]: 1 };
}

/** Convert whole-number percents (one per project, same order) to
 * fractions. Callers validate `percentsSumToHundred` first. */
export function percentsToShares(projects: string[], percents: number[]): Record<string, number> {
  const shares: Record<string, number> = {};
  projects.forEach((p, i) => {
    shares[p] = (percents[i] ?? 0) / 100;
  });
  return shares;
}

export function percentsSumToHundred(percents: number[]): boolean {
  return percents.reduce((a, b) => a + b, 0) === 100;
}

/** Linked sliders: set project `i` to `value` and spread the rest over
 * the others in proportion to what they had (evenly if they were all 0).
 * Whole numbers, always summing to 100 (largest-remainder rounding). */
export function rebalancePercents(percents: number[], i: number, value: number): number[] {
  const v = Math.max(0, Math.min(100, Math.round(value)));
  const others = percents.map((p, j) => (j === i ? 0 : p));
  const othersSum = others.reduce((a, b) => a + b, 0);
  const n = percents.length - 1;
  const raw = others.map((p, j) =>
    j === i ? 0 : ((othersSum > 0 ? p / othersSum : 1 / n) * (100 - v)),
  );
  const out = raw.map(Math.floor);
  let left = 100 - v - out.reduce((a, b) => a + b, 0);
  const order = raw
    .map((r, j) => ({ j, frac: r - Math.floor(r) }))
    .filter(({ j }) => j !== i)
    .sort((a, b) => b.frac - a.frac);
  for (const { j } of order) {
    if (left <= 0) break;
    out[j] += 1;
    left -= 1;
  }
  out[i] = v;
  return out;
}

/** A drag-selected window, shaped like an `Overlap` so it can reuse
 * <OverlapPopover> unchanged — `human_events`/`background_events` aren't
 * shown for a selection so they're just 0. `existingShares` prefills the
 * split (and shows "Reset to automatic") when the selection exactly
 * matches an already-saved allocation. */
export function selectionOverlap(
  startMs: number,
  endMs: number,
  projects: string[],
  existingShares: Record<string, number> | null,
): Overlap {
  return {
    started_at: new Date(startMs).toISOString(),
    ended_at: new Date(endMs).toISOString(),
    minutes: Math.round((endMs - startMs) / 60_000),
    projects: projects.map((project) => ({ project, human_events: 0, background_events: 0 })),
    allocation: existingShares ? { shares: existingShares } : null,
  };
}
