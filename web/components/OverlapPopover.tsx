"use client";

// In-page popover opened by clicking an overlap band on the day strip
// (see DayStrip.tsx). Lets the owner rebalance which project a shared
// window's minutes go to: "Give all to X", or one linked slider per
// project (moving one rebalances the rest so the total stays 100). Split from its sub-components
// (OverlapPopoverControls) to stay under the function-length guard.

import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";
import { allocateOverlap, resetOverlapAllocation } from "@/app/actions-overlaps";
import { percentsToShares } from "@/lib/dayStripOverlaps";
import { toast } from "@/lib/toast";
import type { Overlap } from "@/lib/types";
import { OverlapPopoverControls } from "./OverlapPopoverControls";

interface Props {
  day: string;
  overlap: Overlap;
  onClose: () => void;
  /** Starting split when nothing is saved yet (today's automatic split);
   * falls back to an even split. */
  initialShares?: Record<string, number> | null;
  /** Lane hue per project, so each slider wears its project's colour. */
  hues?: Record<string, number>;
}

export function OverlapPopover({ day, overlap, onClose, initialShares = null, hues = {} }: Props) {
  const router = useRouter();
  const [pending, start] = useTransition();
  const existing = overlap.allocation?.shares ?? null;
  const start0 = existing ?? initialShares;
  // Biggest share first; fixed once so slider indexes stay stable.
  const [projects] = useState(() => {
    const names = overlap.projects.map((p) => p.project);
    return start0 ? [...names].sort((a, b) => (start0[b] ?? 0) - (start0[a] ?? 0)) : names;
  });

  const [percents, setPercents] = useState<number[]>(() => {
    const init = projects.map((p) => Math.round((start0 ? (start0[p] ?? 0) : 1 / projects.length) * 100));
    // Rounding 1/3 etc. can leave 99 or 101; the biggest share absorbs it.
    init[0] += 100 - init.reduce((a, b) => a + b, 0);
    return init;
  });

  function save(shares: Record<string, number>) {
    // A 0% project simply gets nothing; the daemon only takes shares > 0.
    const nonZero = Object.fromEntries(Object.entries(shares).filter(([, f]) => f > 0));
    start(async () => {
      const r = await allocateOverlap(day, overlap.started_at, overlap.ended_at, nonZero);
      if (r.ok) {
        onClose();
        router.refresh();
      } else {
        toast.error(`Save split failed — ${r.error}`);
      }
    });
  }

  function reset() {
    start(async () => {
      const r = await resetOverlapAllocation(day, overlap.started_at, overlap.ended_at);
      if (r.ok) {
        onClose();
        router.refresh();
      } else {
        toast.error(`Reset failed — ${r.error}`);
      }
    });
  }

  return (
    <div className="overlap-popover" role="dialog" aria-label="Split this overlap">
      <OverlapPopoverControls
        overlap={overlap}
        projects={projects}
        hues={hues}
        pending={pending}
        percents={percents}
        setPercents={setPercents}
        hasExisting={existing !== null}
        onGiveAll={save}
        onReset={reset}
        onSave={() => save(percentsToShares(projects, percents))}
        onCancel={onClose}
      />
    </div>
  );
}
