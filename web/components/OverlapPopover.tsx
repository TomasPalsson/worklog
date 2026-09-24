"use client";

// In-page popover opened by clicking an overlap band on the day strip
// (see DayStrip.tsx). Lets the owner rebalance which project a shared
// window's minutes go to: "Give all to X", a two-way slider, or (3+
// projects) a number input per project. Split from its sub-components
// (OverlapPopoverControls) to stay under the function-length guard.

import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";
import { allocateOverlap, resetOverlapAllocation } from "@/app/actions-overlaps";
import { percentsSumToHundred, percentsToShares } from "@/lib/dayStripOverlaps";
import type { Overlap } from "@/lib/types";
import { OverlapPopoverControls } from "./OverlapPopoverControls";

interface Props {
  day: string;
  overlap: Overlap;
  onClose: () => void;
}

export function OverlapPopover({ day, overlap, onClose }: Props) {
  const router = useRouter();
  const [pending, start] = useTransition();
  const projects = overlap.projects.map((p) => p.project);
  const existing = overlap.allocation?.shares ?? null;

  const [pctA, setPctA] = useState(() => Math.round((existing?.[projects[0]] ?? 0.5) * 100));
  const [percents, setPercents] = useState<number[]>(() =>
    projects.map((p) => Math.round((existing?.[p] ?? 1 / projects.length) * 100)),
  );

  function save(shares: Record<string, number>) {
    start(async () => {
      const r = await allocateOverlap(day, overlap.started_at, overlap.ended_at, shares);
      if (r.ok) {
        onClose();
        router.refresh();
      }
    });
  }

  function reset() {
    start(async () => {
      const r = await resetOverlapAllocation(day, overlap.started_at, overlap.ended_at);
      if (r.ok) {
        onClose();
        router.refresh();
      }
    });
  }

  const twoWay = projects.length === 2;
  const shares = twoWay
    ? percentsToShares(projects, [pctA, 100 - pctA])
    : percentsToShares(projects, percents);
  const canSave = twoWay || percentsSumToHundred(percents);

  return (
    <div className="overlap-popover" role="dialog" aria-label="Split this overlap">
      <OverlapPopoverControls
        overlap={overlap}
        projects={projects}
        pending={pending}
        pctA={pctA}
        setPctA={setPctA}
        percents={percents}
        setPercents={setPercents}
        canSave={canSave}
        hasExisting={existing !== null}
        onGiveAll={save}
        onReset={reset}
        onSave={() => save(shares)}
      />
    </div>
  );
}
