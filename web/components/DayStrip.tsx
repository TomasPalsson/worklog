"use client";

// A single horizontal timeline of the day: blocks coloured by project,
// gaps hatched, hour ticks underneath, a legend of totals below that.
// Replaces the old stacked "day-gaps" bullet list — see app/[day]/page.tsx.

import { CSSProperties } from "react";
import { formatDuration } from "@/lib/format";
import {
  buildLegend,
  buildSegments,
  computeTrackWindow,
  hourTicks,
  type StripBlock,
  type StripGap,
  type TrackSegment,
} from "@/lib/dayStrip";

interface Props {
  blocks: StripBlock[];
  gaps: StripGap[];
}

/** Clicking a block segment scrolls its BlockCard into view and flashes it
 * for a moment so the owner can find it in the list below. */
function scrollToBlock(blockId: number) {
  const el = document.getElementById(`block-${blockId}`);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.add("block-flash");
  window.setTimeout(() => el.classList.remove("block-flash"), 1200);
}

export function DayStrip({ blocks, gaps }: Props) {
  const window_ = computeTrackWindow(blocks);
  if (!window_) return null;

  const segments = buildSegments(blocks, gaps, window_);
  const ticks = hourTicks(window_);
  const legend = buildLegend(blocks, gaps);

  return (
    <section className="day-strip" aria-label="Day timeline">
      <div className="day-strip-track">
        {segments.map((seg) => (
          <TrackSegmentButton key={segKey(seg)} seg={seg} />
        ))}
      </div>

      <div className="day-strip-ticks">
        {ticks.map((t) => (
          <span key={t.ms} className="day-strip-tick" style={{ left: `${t.pct}%` }}>
            {t.label}
          </span>
        ))}
      </div>

      <ul className="day-strip-legend" role="list">
        {legend.map((e) => (
          <li key={e.key} className="day-strip-legend-item">
            <span
              className={`day-strip-swatch ${e.away ? "day-strip-swatch-away" : ""}`}
              data-slate={e.slate || undefined}
              style={e.hue !== null ? ({ "--h": e.hue } as CSSProperties) : undefined}
              aria-hidden="true"
            />
            {e.label} {formatDuration(e.totalSeconds)}
          </li>
        ))}
      </ul>
    </section>
  );
}

function segKey(seg: TrackSegment): string {
  return seg.kind === "block" ? `block-${seg.blockId}` : `gap-${seg.startMs}`;
}

function TrackSegmentButton({ seg }: { seg: TrackSegment }) {
  if (seg.kind === "block") {
    return (
      <button
        type="button"
        className="day-strip-seg day-strip-block"
        data-slate={seg.slate || undefined}
        style={
          {
            left: `${seg.leftPct}%`,
            width: `${seg.widthPct}%`,
            opacity: seg.opacity,
            "--h": seg.hue,
          } as CSSProperties
        }
        aria-label={seg.ariaLabel}
        title={seg.ariaLabel}
        onClick={() => scrollToBlock(seg.blockId)}
      />
    );
  }
  return (
    <button
      type="button"
      className="day-strip-seg day-strip-gap"
      style={{ left: `${seg.leftPct}%`, width: `${seg.widthPct}%` }}
      aria-label={seg.ariaLabel}
      title={seg.ariaLabel}
    >
      {seg.showLabel && (
        <span className="day-strip-gap-label">
          {seg.label} · {formatDuration(seg.minutes * 60)}
        </span>
      )}
    </button>
  );
}
