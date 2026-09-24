"use client";

// Render-only body of OverlapPopover, split out to stay under the
// function-length guard. All state/daemon calls live in the parent.

import { formatDuration } from "@/lib/format";
import { giveAllShares, percentsToShares } from "@/lib/dayStripOverlaps";
import type { Overlap } from "@/lib/types";

interface Props {
  overlap: Overlap;
  projects: string[];
  pending: boolean;
  pctA: number;
  setPctA: (v: number) => void;
  percents: number[];
  setPercents: (v: number[]) => void;
  canSave: boolean;
  hasExisting: boolean;
  onGiveAll: (shares: Record<string, number>) => void;
  onReset: () => void;
  onSave: () => void;
}

export function OverlapPopoverControls({
  overlap,
  projects,
  pending,
  pctA,
  setPctA,
  percents,
  setPercents,
  canSave,
  hasExisting,
  onGiveAll,
  onReset,
  onSave,
}: Props) {
  const twoWay = projects.length === 2;
  return (
    <>
      <div className="overlap-popover-give-all">
        {overlap.projects.map((p) => (
          <button
            key={p.project}
            type="button"
            disabled={pending}
            onClick={() => onGiveAll(giveAllShares(p.project))}
          >
            Give all to {p.project}
          </button>
        ))}
      </div>

      {twoWay ? (
        <TwoWaySlider overlap={overlap} projects={projects} pctA={pctA} setPctA={setPctA} />
      ) : (
        <MultiWayInputs projects={projects} percents={percents} setPercents={setPercents} />
      )}

      {hasExisting && (
        <button type="button" className="overlap-popover-reset" disabled={pending} onClick={onReset}>
          Reset to automatic
        </button>
      )}

      <button type="button" disabled={pending || !canSave} onClick={onSave}>
        Save
      </button>
    </>
  );
}

function TwoWaySlider({
  overlap,
  projects,
  pctA,
  setPctA,
}: {
  overlap: Overlap;
  projects: string[];
  pctA: number;
  setPctA: (v: number) => void;
}) {
  const shares = percentsToShares(projects, [pctA, 100 - pctA]);
  return (
    <label className="overlap-popover-slider">
      <input
        type="range"
        min={0}
        max={100}
        step={5}
        value={pctA}
        onChange={(e) => setPctA(Number(e.target.value))}
      />
      <span>
        {projects[0]} {pctA}% · {formatDuration(overlap.minutes * 60 * (shares[projects[0]] ?? 0))}
        {" / "}
        {projects[1]} {100 - pctA}% · {formatDuration(overlap.minutes * 60 * (shares[projects[1]] ?? 0))}
      </span>
    </label>
  );
}

function MultiWayInputs({
  projects,
  percents,
  setPercents,
}: {
  projects: string[];
  percents: number[];
  setPercents: (v: number[]) => void;
}) {
  return (
    <div className="overlap-popover-inputs">
      {projects.map((p, i) => (
        <label key={p}>
          {p}
          <input
            type="number"
            min={0}
            max={100}
            value={percents[i]}
            onChange={(e) => {
              const next = [...percents];
              next[i] = Number(e.target.value);
              setPercents(next);
            }}
          />
          %
        </label>
      ))}
    </div>
  );
}
