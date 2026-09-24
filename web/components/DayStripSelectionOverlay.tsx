"use client";

// Visual layer for the lanes view's click-and-drag time-range selection —
// the translucent rectangle shown while dragging, and the OverlapPopover
// opened (anchored to the selection) once the drag ends. Split out of
// DayStripLanesView.tsx to stay under the file/function-length guard.

import { findExactAllocation, projectsActiveInRange, type Selection } from "@/lib/dayStripSelection";
import { selectionOverlap } from "@/lib/dayStripOverlaps";
import type { ProjectActivity, SavedAllocation } from "@/lib/types";
import { OverlapPopover } from "./OverlapPopover";

export function SelectionOverlay({
  day,
  live,
  committed,
  activity,
  allocations,
  onCloseCommitted,
}: {
  day: string;
  live: Selection | null;
  committed: Selection | null;
  activity: ProjectActivity[];
  allocations: SavedAllocation[];
  onCloseCommitted: () => void;
}) {
  const committedProjects = committed
    ? projectsActiveInRange(activity, committed.startMs, committed.endMs)
    : [];

  return (
    // Not aria-hidden: unlike the purely decorative gap-band overlay, this
    // layer hosts the real, interactive split popover once a drag commits.
    <div className="day-strip-selection-layer">
      {live && (
        <>
          {/* Separate elements, not parent/child: the rectangle's opacity
             must never fade the label text over it. */}
          <div className="day-strip-selection" style={{ left: `${live.leftPct}%`, width: `${live.widthPct}%` }} />
          {live.widthPct >= 4 && (
            <span
              className="day-strip-selection-label"
              style={{ left: `${live.leftPct}%`, width: `${live.widthPct}%` }}
            >
              {live.label}
            </span>
          )}
        </>
      )}
      {committed && committedProjects.length > 0 && (
        <div className="day-strip-selection-popover-anchor" style={{ left: `${committed.leftPct}%` }}>
          <OverlapPopover
            day={day}
            overlap={selectionOverlap(
              committed.startMs,
              committed.endMs,
              committedProjects,
              findExactAllocation(allocations, committed.startMs, committed.endMs),
            )}
            onClose={onCloseCommitted}
          />
        </div>
      )}
    </div>
  );
}
