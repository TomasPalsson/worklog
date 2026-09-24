"use client";

// Click-and-drag time-range selection for the lanes view — the owner
// picks an arbitrary window (not just a detected overlap) to rebalance.
// Pure math (snapping, fraction->selection, which projects are active)
// lives here so it's unit-testable without a DOM; `useDragSelection`
// wires it to pointer events on the lanes container.

import { useCallback, useRef, useState } from "react";
import { clock, type TrackWindow } from "./dayStrip";
import { formatDuration } from "./format";
import type { ProjectActivity, SavedAllocation } from "./types";

const MS_PER_MIN = 60_000;

export interface Selection {
  startMs: number;
  endMs: number;
  leftPct: number;
  widthPct: number;
  label: string;
}

export function snapToMinute(ms: number): number {
  return Math.round(ms / MS_PER_MIN) * MS_PER_MIN;
}

/** A drag's selection from two arbitrary fractional positions (0..1)
 * along the tracks — order-independent, snapped to whole minutes, and
 * clamped inside the window. `null` once snapped down to under a minute
 * (a click, not a drag). */
export function selectionFromFractions(
  fracA: number,
  fracB: number,
  window: TrackWindow,
): Selection | null {
  const totalMs = window.endMs - window.startMs;
  if (totalMs <= 0) return null;
  const toMs = (f: number) => window.startMs + Math.min(1, Math.max(0, f)) * totalMs;
  const startMs = Math.max(window.startMs, snapToMinute(toMs(Math.min(fracA, fracB))));
  const endMs = Math.min(window.endMs, snapToMinute(toMs(Math.max(fracA, fracB))));
  if (endMs - startMs < MS_PER_MIN) return null;
  const pct = (ms: number) => ((ms - window.startMs) / totalMs) * 100;
  const minutes = Math.round((endMs - startMs) / MS_PER_MIN);
  return {
    startMs,
    endMs,
    leftPct: pct(startMs),
    widthPct: pct(endMs) - pct(startMs),
    label: `${clock(startMs)}–${clock(endMs)} · ${formatDuration(minutes * 60)}`,
  };
}

/** Work projects whose activity intersects `[startMs, endMs)`. */
export function projectsActiveInRange(
  activity: ProjectActivity[],
  startMs: number,
  endMs: number,
): string[] {
  return activity
    .filter((a) =>
      a.spans.some(
        (s) => new Date(s.started_at).getTime() < endMs && new Date(s.ended_at).getTime() > startMs,
      ),
    )
    .map((a) => a.project);
}

type LaneLike = { key: string; segments: { kind: string; startMs: number; endMs: number }[] };

/** Ms each project's blocks own inside the range, in `projects` order. */
function ownedMs(lanes: LaneLike[], projects: string[], startMs: number, endMs: number): number[] {
  return projects.map((p) =>
    (lanes.find((l) => l.key === p)?.segments ?? [])
      .filter((s) => s.kind === "block")
      .reduce((sum, s) => sum + Math.max(0, Math.min(endMs, s.endMs) - Math.max(startMs, s.startMs)), 0),
  );
}

/** Worked (owned) minutes in the range — what a saved split divides up;
 * idle minutes stay idle. */
export function workedMinutes(lanes: LaneLike[], projects: string[], startMs: number, endMs: number): number {
  return Math.round(ownedMs(lanes, projects, startMs, endMs).reduce((a, b) => a + b, 0) / MS_PER_MIN);
}

/** How the range is split right now: each project's share of the minutes
 * its lane's blocks own inside `[startMs, endMs)`. Seeds the sliders so the
 * owner starts from today's split, not an even one. `null` if no one owns
 * a minute there. */
export function currentShares(
  lanes: LaneLike[],
  projects: string[],
  startMs: number,
  endMs: number,
): Record<string, number> | null {
  const owned = ownedMs(lanes, projects, startMs, endMs);
  const total = owned.reduce((a, b) => a + b, 0);
  if (total <= 0) return null;
  return Object.fromEntries(projects.map((p, i) => [p, owned[i] / total]));
}

/** The saved allocation whose window exactly matches this selection, if
 * any — prefills the popover's split and shows "Reset to automatic"
 * when re-dragging over an already-set window. */
export function findExactAllocation(
  allocations: SavedAllocation[],
  startMs: number,
  endMs: number,
): Record<string, number> | null {
  const match = allocations.find(
    (a) => new Date(a.started_at).getTime() === startMs && new Date(a.ended_at).getTime() === endMs,
  );
  return match?.shares ?? null;
}

/** Elements whose own click/hover behaviour a drag-select must not
 * hijack — block/gap segments and the overlap band button. */
const INTERACTIVE_SELECTOR = ".day-strip-seg, .day-strip-overlap-inner";

/** The four pointer/keyboard handlers `useDragSelection` wires to the
 * lanes container — split out so that hook stays under the size guard.
 * `dragRef` mirrors `dragFracs` synchronously so `onPointerUp` never
 * risks acting on a stale closure. */
function useDragHandlers(
  containerRef: React.RefObject<HTMLDivElement | null>,
  dragRef: React.RefObject<[number, number] | null>,
  window: TrackWindow | null,
  setDragFracs: (v: [number, number] | null | ((cur: [number, number] | null) => [number, number] | null)) => void,
  setCommitted: (v: Selection | null) => void,
) {
  const fracFromClientX = useCallback(
    (clientX: number) => {
      // Measure a track, not the container: the container also holds the
      // lane-name column, which would shift every picked time right.
      const rect = containerRef.current?.querySelector(".day-strip-lane-track")?.getBoundingClientRect();
      if (!rect || rect.width === 0) return 0;
      return (clientX - rect.left) / rect.width;
    },
    [containerRef],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest(INTERACTIVE_SELECTOR)) return;
      const frac = fracFromClientX(e.clientX);
      dragRef.current = [frac, frac];
      setDragFracs([frac, frac]);
      setCommitted(null);
      // Not implemented in every test/DOM environment — pointer capture is
      // an enhancement (keeps the drag tracking outside the element's
      // bounds), not required for the selection logic itself.
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    [dragRef, fracFromClientX, setCommitted, setDragFracs],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      setDragFracs((cur) => {
        if (!cur) return cur;
        const next: [number, number] = [cur[0], fracFromClientX(e.clientX)];
        dragRef.current = next;
        return next;
      });
    },
    [dragRef, fracFromClientX, setDragFracs],
  );

  const onPointerUp = useCallback(() => {
    const cur = dragRef.current;
    dragRef.current = null;
    if (cur && window) {
      const sel = selectionFromFractions(cur[0], cur[1], window);
      if (sel) setCommitted(sel);
    }
    setDragFracs(null);
  }, [dragRef, setCommitted, setDragFracs, window]);

  const onKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      dragRef.current = null;
      setDragFracs(null);
    }
  }, [dragRef, setDragFracs]);

  return { onPointerDown, onPointerMove, onPointerUp, onKeyDown };
}

/** Click-and-drag horizontally across the lanes tracks to pick an
 * arbitrary time range. The pointer handlers go straight on the lanes
 * container — no extra hit-testing overlay needed — and only start a
 * drag when the pointer goes down on empty track background, so
 * existing segment/band clicks keep working unchanged. */
export function useDragSelection(window: TrackWindow | null) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<[number, number] | null>(null);
  const [dragFracs, setDragFracs] = useState<[number, number] | null>(null);
  const [committed, setCommitted] = useState<Selection | null>(null);

  const handlers = useDragHandlers(containerRef, dragRef, window, setDragFracs, setCommitted);
  const live = dragFracs && window ? selectionFromFractions(dragFracs[0], dragFracs[1], window) : null;

  return {
    containerRef,
    handlers,
    live,
    committed,
    closeCommitted: () => setCommitted(null),
  };
}
