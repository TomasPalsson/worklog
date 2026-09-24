"use client";

// The amber overlap bands drawn over the lanes view (DayStrip.tsx) —
// split out to stay under the file/function-length guard. All bands share
// ONE absolutely-positioned overlay layer over the tracks column (so they
// can never turn into a lane row of their own); each spans the full lanes
// height — simpler than measuring the exact lane rows it touches, and the
// label + popover already say which projects are involved. Opens an
// OverlapPopover on click.

const OVERLAP_LABEL_MIN_WIDTH_PCT = 4;

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
    <div className="day-strip-overlap-layer" aria-hidden={bands.length === 0 || undefined}>
      {bands.map((band) => (
        <OverlapBandButton
          key={overlapKey(band.overlap)}
          band={band}
          day={day}
          open={openOverlap === overlapKey(band.overlap)}
          onToggle={onToggleOverlap}
        />
      ))}
    </div>
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
  const showLabel = band.widthPct >= OVERLAP_LABEL_MIN_WIDTH_PCT;
  return (
    <div className="day-strip-overlap-band" style={{ left: `${band.leftPct}%`, width: `${band.widthPct}%` }}>
      <button
        type="button"
        className="day-strip-overlap-inner"
        aria-label={band.label}
        title={band.label}
        onClick={() => onToggle(key)}
      >
        {showLabel && <span className="day-strip-overlap-label">{band.label}</span>}
      </button>
      {open && (
        <div className="day-strip-overlap-popover-anchor">
          <OverlapPopover day={day} overlap={band.overlap} onClose={() => onToggle(key)} />
        </div>
      )}
    </div>
  );
}
