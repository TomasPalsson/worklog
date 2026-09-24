"use client";

// Saved-allocation brackets shown above the lanes tracks — one per saved
// window (from a drag-selection or an overlap band's split), each a thin
// muted bracket reading "you set 90/10" with a reset affordance. Split
// out of DayStripLanesView.tsx to stay under the file/function-length
// guard.

import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { resetOverlapAllocation } from "@/app/actions-overlaps";
import type { AllocationBand } from "@/lib/dayStripAllocations";

export function AllocationBrackets({ bands, day }: { bands: AllocationBand[]; day: string }) {
  if (bands.length === 0) return null;
  return (
    <div className="day-strip-lane-row day-strip-allocations-row">
      <div className="day-strip-lane-label" aria-hidden="true" />
      <div className="day-strip-lane-track day-strip-allocations-track">
        {bands.map((band) => (
          <AllocationBracket key={`${band.allocation.started_at}|${band.allocation.ended_at}`} band={band} day={day} />
        ))}
      </div>
    </div>
  );
}

function AllocationBracket({ band, day }: { band: AllocationBand; day: string }) {
  const router = useRouter();
  const [pending, start] = useTransition();

  function reset() {
    start(async () => {
      const r = await resetOverlapAllocation(day, band.allocation.started_at, band.allocation.ended_at);
      if (r.ok) router.refresh();
    });
  }

  return (
    <div
      className="day-strip-allocation-bracket"
      style={{ left: `${band.leftPct}%`, width: `${band.widthPct}%` }}
      title={band.ratioLabel}
    >
      <span className="day-strip-allocation-label">
        {band.ratioLabel}
        <button type="button" className="day-strip-allocation-reset" disabled={pending} onClick={reset}>
          reset
        </button>
      </span>
    </div>
  );
}
