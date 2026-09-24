"use client";

// Saved splits in the lanes view. Two parts, so labels never pile up:
//  * a thin marker strip above the tracks — one bar per saved range, in
//    the colour of the project that got the most of it;
//  * a chip per saved range under the lanes, spelling it out:
//    "12:01–12:26 · vitinn-infra 100%" with a reset.

import type { CSSProperties } from "react";
import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { resetOverlapAllocation } from "@/app/actions-overlaps";
import { formatRange } from "@/lib/format";
import { sharesByLargest, type AllocationBand } from "@/lib/dayStripAllocations";

type Hues = Record<string, number>;

const hueStyle = (hues: Hues, project: string | undefined) =>
  ({ "--h": (project && hues[project]) ?? 0 }) as CSSProperties;

export function AllocationBrackets({ bands, hues }: { bands: AllocationBand[]; hues: Hues }) {
  if (bands.length === 0) return null;
  return (
    <div className="day-strip-lane-row day-strip-allocations-row" aria-hidden="true">
      <div className="day-strip-lane-label" />
      <div className="day-strip-lane-track day-strip-allocations-track">
        {bands.map((band) => (
          <span
            key={`${band.allocation.started_at}|${band.allocation.ended_at}`}
            className="day-strip-allocation-bracket"
            style={{
              left: `${band.leftPct}%`,
              width: `${band.widthPct}%`,
              ...hueStyle(hues, sharesByLargest(band.allocation.shares)[0]?.[0]),
            }}
          />
        ))}
      </div>
    </div>
  );
}

export function AllocationChips({ bands, day, hues }: { bands: AllocationBand[]; day: string; hues: Hues }) {
  if (bands.length === 0) return null;
  return (
    <ul className="day-strip-allocation-chips" aria-label="Your saved splits">
      {bands.map((band) => (
        <AllocationChip key={`${band.allocation.started_at}|${band.allocation.ended_at}`} band={band} day={day} hues={hues} />
      ))}
    </ul>
  );
}

function AllocationChip({ band, day, hues }: { band: AllocationBand; day: string; hues: Hues }) {
  const router = useRouter();
  const [pending, start] = useTransition();
  const { started_at, ended_at, shares } = band.allocation;

  function reset() {
    start(async () => {
      const r = await resetOverlapAllocation(day, started_at, ended_at);
      if (r.ok) router.refresh();
    });
  }

  return (
    <li className="day-strip-allocation-chip">
      <span className="day-strip-allocation-when">{formatRange(started_at, ended_at)}</span>
      {sharesByLargest(shares).map(([project, pct]) => (
        <span key={project} className="day-strip-allocation-share" style={hueStyle(hues, project)}>
          <span className="overlap-popover-dot" aria-hidden="true" />
          {project} {pct}%
        </span>
      ))}
      <button type="button" className="day-strip-allocation-reset" disabled={pending} onClick={reset}>
        reset
      </button>
    </li>
  );
}
