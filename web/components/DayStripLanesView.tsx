"use client";

// The expanded "lanes" view (one row per project) — split out of
// DayStrip.tsx to stay under the file/function-length guard. Label +
// track live in ONE flex row per lane so they can never fall out of
// alignment; the gap/overlap bands are a single absolutely-positioned
// overlay layer on top of the tracks column, not a lane row of their own.

import { CSSProperties } from "react";
import { formatDuration } from "@/lib/format";
import { segKey, TrackSegmentButton, type SegProps } from "./DayStrip";
import type { TrackSegment } from "@/lib/dayStrip";
import type { Lane } from "@/lib/dayStripLanes";
import type { OverlapBand } from "@/lib/dayStripOverlaps";
import { OverlapBands } from "./DayStripOverlapBands";

export function LanesView({
  lanes,
  gapSegments,
  segProps,
  bands,
  day,
  openOverlap,
  onToggleOverlap,
}: {
  lanes: Lane[];
  gapSegments: Extract<TrackSegment, { kind: "gap" }>[];
  segProps: SegProps;
  bands: OverlapBand[];
  day: string;
  openOverlap: string | null;
  onToggleOverlap: (key: string) => void;
}) {
  return (
    <div className="day-strip-lanes">
      {lanes.map((lane) => (
        <LaneRow key={lane.key} lane={lane} segProps={segProps} />
      ))}
      {/* One overlay layer over the tracks column, spanning every lane row's
         height — not a lane row of its own, so it can never misalign the
         label/track pairing above. */}
      <div className="day-strip-lanes-overlay" aria-hidden="true">
        {gapSegments.map((g) => (
          <span
            key={`gapband-${g.startMs}`}
            className="day-strip-gap-band"
            style={{ left: `${g.leftPct}%`, width: `${g.widthPct}%` }}
          />
        ))}
      </div>
      <OverlapBands bands={bands} day={day} openOverlap={openOverlap} onToggleOverlap={onToggleOverlap} />
    </div>
  );
}

function LaneRow({ lane, segProps }: { lane: Lane; segProps: SegProps }) {
  return (
    <div className="day-strip-lane-row" data-testid="lane-row">
      <div className="day-strip-lane-label" title={`${lane.label} ${formatDuration(lane.totalSeconds)}`}>
        <span className="day-strip-lane-name">{lane.label}</span>
        <span className="day-strip-lane-total">
          {formatDuration(lane.totalSeconds)}
          {lane.activitySeconds > 0 && (
            <span className="day-strip-lane-active">
              {" "}
              · active {formatDuration(lane.activitySeconds)}
            </span>
          )}
        </span>
      </div>
      <div className="day-strip-lane-track" data-testid="lane-track">
        {lane.activityBands.map((b) => (
          <span
            key={`activity-${b.leftPct}-${b.widthPct}`}
            className="day-strip-lane-activity"
            style={{ left: `${b.leftPct}%`, width: `${b.widthPct}%`, "--h": lane.hue ?? 0 } as CSSProperties}
            aria-hidden="true"
          />
        ))}
        {lane.segments.map((seg) => (
          <TrackSegmentButton key={segKey(seg)} seg={seg} {...segProps} />
        ))}
      </div>
    </div>
  );
}
