"use client";

// The amber overlap bands drawn over the lanes view (DayStrip.tsx) —
// split out to stay under the file/function-length guard. Each band spans
// the lane rows its projects touch (CSS grid-row) and opens an
// OverlapPopover on click.

import type { OverlapBand } from "@/lib/dayStripOverlaps";
import type { Overlap } from "@/lib/types";
import { OverlapPopover } from "./OverlapPopover";

export function overlapKey(o: Overlap): string {
  return `${o.started_at}|${o.ended_at}`;
}

export function OverlapBands({
  bands,
  day,
  openOverlap,
  onToggleOverlap,
}: {
  bands: OverlapBand[];
  day: string;
  openOverlap: string | null;
  onToggleOverlap: (key: string) => void;
}) {
  return (
    <>
      {bands.map((band) => (
        <OverlapBandButton
          key={overlapKey(band.overlap)}
          band={band}
          day={day}
          open={openOverlap === overlapKey(band.overlap)}
          onToggle={onToggleOverlap}
        />
      ))}
    </>
  );
}

function OverlapBandButton({
  band,
  day,
  open,
  onToggle,
}: {
  band: OverlapBand;
  day: string;
  open: boolean;
  onToggle: (key: string) => void;
}) {
  if (!band.laneRange) return null;
  const key = overlapKey(band.overlap);
  return (
    <div
      className="day-strip-overlap-band"
      style={{ gridRow: `${band.laneRange[0] + 1} / ${band.laneRange[1] + 2}` }}
    >
      <button
        type="button"
        className="day-strip-overlap-inner"
        style={{ left: `${band.leftPct}%`, width: `${band.widthPct}%` }}
        aria-label={band.label}
        onClick={() => onToggle(key)}
      >
        <span className="day-strip-overlap-label">{band.label}</span>
      </button>
      {open && (
        <div className="day-strip-overlap-popover-anchor" style={{ left: `${band.leftPct}%` }}>
          <OverlapPopover day={day} overlap={band.overlap} onClose={() => onToggle(key)} />
        </div>
      )}
    </div>
  );
}
