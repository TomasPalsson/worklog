"use client";

// Render-only body of OverlapPopover, split out to stay under the
// function-length guard. All state/daemon calls live in the parent.

import type { CSSProperties } from "react";
import { X } from "lucide-react";
import { formatDuration, formatRange } from "@/lib/format";
import { giveAllShares, rebalancePercents } from "@/lib/dayStripOverlaps";
import type { Overlap } from "@/lib/types";

interface Props {
  overlap: Overlap;
  projects: string[];
  hues: Record<string, number>;
  pending: boolean;
  percents: number[];
  setPercents: (v: number[]) => void;
  hasExisting: boolean;
  onGiveAll: (shares: Record<string, number>) => void;
  onReset: () => void;
  onSave: () => void;
  onCancel: () => void;
}

export function OverlapPopoverControls({
  overlap,
  projects,
  hues,
  pending,
  percents,
  setPercents,
  hasExisting,
  onGiveAll,
  onReset,
  onSave,
  onCancel,
}: Props) {
  return (
    <>
      <header className="overlap-popover-head">
        <span>
          {formatRange(overlap.started_at, overlap.ended_at)}
          <span className="overlap-popover-worked"> · {formatDuration(overlap.minutes * 60)} worked</span>
        </span>
        <button type="button" className="overlap-popover-x" aria-label="Close" onClick={onCancel}>
          <X />
        </button>
      </header>

      <ul className="overlap-popover-rows">
        {projects.map((p, i) => (
          <li key={p} className="overlap-popover-row" style={{ "--h": hues[p] ?? 0 } as CSSProperties}>
            <span className="overlap-popover-name">
              <span className="overlap-popover-dot" aria-hidden="true" />
              {p}
            </span>
            <span className="overlap-popover-amount">
              {percents[i]}% · {formatDuration(overlap.minutes * 60 * (percents[i] / 100))}
            </span>
            <button
              type="button"
              className="overlap-popover-all"
              aria-label={`Give all to ${p}`}
              title={`Give all to ${p}`}
              disabled={pending}
              onClick={() => onGiveAll(giveAllShares(p))}
            >
              all
            </button>
            {/* Linked: moving one slider rebalances the others, so the
               total is always 100 and Save can never be invalid. */}
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={percents[i]}
              aria-label={`${p} share`}
              onChange={(e) => setPercents(rebalancePercents(percents, i, Number(e.target.value)))}
            />
          </li>
        ))}
      </ul>

      <footer className="overlap-popover-actions">
        {hasExisting && (
          <button type="button" className="overlap-popover-reset" disabled={pending} onClick={onReset}>
            Reset to automatic
          </button>
        )}
        <button type="button" className="overlap-popover-cancel" disabled={pending} onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="overlap-popover-save" disabled={pending} onClick={onSave}>
          Save
        </button>
      </footer>
    </>
  );
}
