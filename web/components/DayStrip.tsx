"use client";

// A single horizontal timeline of the day: blocks coloured by project,
// gaps hatched, hour ticks underneath, a legend of totals below that.
// Replaces the old stacked "day-gaps" bullet list — see app/[day]/page.tsx.
//
// "Expand" swaps the single bar for one lane per project (buildLanes),
// hovering/focusing a legend row dims every other project's segments
// (isFocused), and hovering/focusing a block segment shows a small
// tooltip card with its time range and description.

import { CSSProperties, useEffect, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { formatDuration } from "@/lib/format";
import {
  buildLegend,
  buildSegments,
  clock,
  withLegendHues,
  mergeAdjacentBlocks,
  compactLegend,
  computeTrackWindow,
  hourTicks,
  type HourTick,
  type LegendEntry,
  type StripBlock,
  type StripGap,
  type TrackSegment,
  type TrackWindow,
} from "@/lib/dayStrip";
import { buildLanes, isFocused } from "@/lib/dayStripLanes";
import { buildOverlapBands, overlapsInWindow } from "@/lib/dayStripOverlaps";
import { LanesView } from "./DayStripLanesView";
import type { Overlap, ProjectActivity } from "@/lib/types";

interface Props {
  day: string;
  blocks: StripBlock[];
  gaps: StripGap[];
  overlaps?: Overlap[];
  activity?: ProjectActivity[];
}

const EXPANDED_STORAGE_KEY = "worklog.dayStrip.expanded";

/** Clicking a block segment scrolls its BlockCard into view and flashes it
 * for a moment so the owner can find it in the list below. */
function scrollToBlock(blockId: number) {
  const el = document.getElementById(`block-${blockId}`);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.add("block-flash");
  window.setTimeout(() => el.classList.remove("block-flash"), 1200);
}

function readStoredExpanded(): boolean {
  try {
    return localStorage.getItem(EXPANDED_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function writeStoredExpanded(next: boolean) {
  try {
    localStorage.setItem(EXPANDED_STORAGE_KEY, next ? "1" : "0");
  } catch {
    // Private browsing / no storage — the toggle still works this visit.
  }
}

/** Expanded/collapsed state, backed by localStorage (best-effort). Reads
 * happen after mount since localStorage isn't available during SSR. */
function useExpanded(): [boolean, () => void] {
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    setExpanded(readStoredExpanded());
  }, []);
  function toggle() {
    setExpanded((prev) => {
      const next = !prev;
      writeStoredExpanded(next);
      return next;
    });
  }
  return [expanded, toggle];
}

function buildStripData(
  blocks: StripBlock[],
  gaps: StripGap[],
  overlaps: Overlap[],
  activity: ProjectActivity[],
  window_: TrackWindow,
) {
  const hued = withLegendHues(buildSegments(blocks, gaps, window_), buildLegend(blocks, gaps));
  const segments = mergeAdjacentBlocks(hued.segments);
  const gapSegments = segments.filter(
    (s): s is Extract<TrackSegment, { kind: "gap" }> => s.kind === "gap",
  );
  const lanes = buildLanes(segments, hued.legend, window_, activity);
  return {
    segments,
    fullLegend: hued.legend,
    legend: compactLegend(hued.legend),
    ticks: hourTicks(window_),
    gapSegments,
    lanes,
    bands: buildOverlapBands(overlaps, window_, lanes.map((l) => l.key)),
  };
}

export function DayStrip({
  day,
  blocks: allBlocks,
  gaps: allGaps,
  overlaps: allOverlaps = [],
  activity = [],
}: Props) {
  // Work-only: personal time never shows — not as a segment, a legend row or
  // by stretching the time axis — and only gaps inside the work day remain.
  const blocks = allBlocks.filter((b) => !b.is_personal);
  const window_ = computeTrackWindow(blocks);
  const gaps = window_
    ? allGaps.filter(
        (g) =>
          new Date(g.started_at).getTime() >= window_.startMs &&
          new Date(g.ended_at).getTime() <= window_.endMs,
      )
    : [];
  const overlaps = window_ ? overlapsInWindow(allOverlaps, window_) : [];
  const [expanded, toggleExpanded] = useExpanded();
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const [openTip, setOpenTip] = useState<string | null>(null);
  const [openOverlap, setOpenOverlap] = useState<string | null>(null);

  if (!window_) return null;

  const data = buildStripData(blocks, gaps, overlaps, activity, window_);
  const segProps: SegProps = {
    legend: data.legend,
    focusKey,
    openTip,
    onOpenTip: setOpenTip,
    onCloseTip: (key) => setOpenTip((cur) => (cur === key ? null : cur)),
  };

  return (
    <section className="day-strip" aria-label="Day timeline">
      <DayStripToolbar
        expanded={expanded}
        onToggle={toggleExpanded}
        onExpand={() => !expanded && toggleExpanded()}
        overlapCount={overlaps.length}
      />

      {expanded ? (
        <LanesView
          lanes={data.lanes}
          gapSegments={data.gapSegments}
          segProps={segProps}
          bands={data.bands}
          day={day}
          openOverlap={openOverlap}
          onToggleOverlap={(key) => setOpenOverlap((cur) => (cur === key ? null : key))}
        />
      ) : (
        <div className="day-strip-track">
          {data.segments.map((seg) => (
            <TrackSegmentButton key={segKey(seg)} seg={seg} {...segProps} />
          ))}
        </div>
      )}

      <Ticks ticks={data.ticks} expanded={expanded} />
      <LegendList legend={data.legend} onFocus={setFocusKey} />
    </section>
  );
}

function DayStripToolbar({
  expanded,
  onToggle,
  onExpand,
  overlapCount,
}: {
  expanded: boolean;
  onToggle: () => void;
  onExpand: () => void;
  overlapCount: number;
}) {
  return (
    <div className="day-strip-toolbar">
      {overlapCount > 0 && (
        <button type="button" className="day-strip-overlap-chip" onClick={onExpand}>
          {overlapCount} overlap{overlapCount === 1 ? "" : "s"}
        </button>
      )}
      <button
        type="button"
        className="day-strip-expand-btn"
        aria-expanded={expanded}
        onClick={onToggle}
      >
        {expanded ? <ChevronUp size={14} strokeWidth={1.75} /> : <ChevronDown size={14} strokeWidth={1.75} />}
        {expanded ? "Collapse" : "Expand"}
      </button>
    </div>
  );
}

function Ticks({ ticks, expanded }: { ticks: HourTick[]; expanded: boolean }) {
  return (
    <div className={`day-strip-ticks${expanded ? " day-strip-ticks-lanes" : ""}`}>
      {ticks.map((t) => (
        <span key={t.ms} className="day-strip-tick" style={{ left: `${t.pct}%` }}>
          {t.label}
        </span>
      ))}
    </div>
  );
}

export interface SegProps {
  legend: LegendEntry[];
  focusKey: string | null;
  openTip: string | null;
  onOpenTip: (key: string) => void;
  onCloseTip: (key: string) => void;
}

function LegendList({
  legend,
  onFocus,
}: {
  legend: LegendEntry[];
  onFocus: (key: string | null) => void;
}) {
  return (
    <ul className="day-strip-legend" role="list">
      {legend.map((e) => (
        <li
          key={e.key}
          className="day-strip-legend-item"
          title={e.title}
          tabIndex={0}
          onMouseEnter={() => onFocus(e.key)}
          onMouseLeave={() => onFocus(null)}
          onFocus={() => onFocus(e.key)}
          onBlur={() => onFocus(null)}
        >
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
  );
}

export function segKey(seg: TrackSegment): string {
  return seg.kind === "block" ? `block-${seg.blockId}` : `gap-${seg.startMs}`;
}

export function TrackSegmentButton({
  seg,
  legend,
  focusKey,
  openTip,
  onOpenTip,
  onCloseTip,
}: { seg: TrackSegment } & SegProps) {
  if (seg.kind === "gap") return <GapSegmentButton seg={seg} />;

  const key = segKey(seg);
  const dimmed = focusKey !== null && !isFocused(seg.key, focusKey, legend);
  const showTip = openTip === key && !!seg.description;
  return (
    <button
      type="button"
      className="day-strip-seg day-strip-block"
      data-slate={seg.slate || undefined}
      data-legend-focused={focusKey !== null && !dimmed ? "true" : undefined}
      style={
        {
          left: `${seg.leftPct}%`,
          width: `${seg.widthPct}%`,
          opacity: dimmed ? 0.25 : seg.opacity,
          "--h": seg.hue,
        } as CSSProperties
      }
      aria-label={seg.ariaLabel}
      onClick={() => scrollToBlock(seg.blockId)}
      onMouseEnter={() => onOpenTip(key)}
      onMouseLeave={() => onCloseTip(key)}
      onFocus={() => onOpenTip(key)}
      onBlur={() => onCloseTip(key)}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCloseTip(key);
      }}
    >
      {showTip && <SegmentTooltip seg={seg} />}
    </button>
  );
}

function GapSegmentButton({ seg }: { seg: Extract<TrackSegment, { kind: "gap" }> }) {
  return (
    <button
      type="button"
      className="day-strip-seg day-strip-gap"
      style={{ left: `${seg.leftPct}%`, width: `${seg.widthPct}%` }}
      aria-label={seg.ariaLabel}
      title={seg.ariaLabel}
    >
      {seg.showShortLabel && (
        <span className="day-strip-gap-label">
          {seg.showLabel ? `${seg.label} · ${formatDuration(seg.minutes * 60)}` : seg.label}
        </span>
      )}
    </button>
  );
}

function SegmentTooltip({ seg }: { seg: Extract<TrackSegment, { kind: "block" }> }) {
  const seconds = (seg.endMs - seg.startMs) / 1000;
  return (
    <div className="day-strip-tooltip" role="tooltip">
      <div className="day-strip-tooltip-title">{seg.label}</div>
      <div className="day-strip-tooltip-range">
        {clock(seg.startMs)}–{clock(seg.endMs)} · {formatDuration(seconds)}
      </div>
      {seg.description && (
        <div className="day-strip-tooltip-desc">
          {seg.description}
          {seg.mergedCount > 1 ? ` and ${seg.mergedCount - 1} more blocks` : ""}
        </div>
      )}
    </div>
  );
}
