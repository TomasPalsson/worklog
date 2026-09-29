"use client";

// Quiet "N ignored blocks · Show" line under the day's groups. Ignored
// blocks are hidden from the day, billing and Tempo; Restore undoes it.
// `restore` is injectable (defaults to the real server action) because
// @/app/actions is `mock.module`d process-wide by other test files.

import { useState, useTransition } from "react";
import * as actions from "@/app/actions";
import { formatRange } from "@/lib/format";
import { toast } from "@/lib/toast";
import type { Block } from "@/lib/types";

type Restore = (
  id: number,
  ignored: boolean,
  day: string,
) => Promise<{ ok: true } | { ok: false; error: string }>;

interface Props {
  blocks: Pick<Block, "id" | "started_at" | "ended_at" | "description">[];
  day: string;
  restore?: Restore;
}

export function IgnoredLine({ blocks, day, restore = actions.setIgnored }: Props) {
  const [open, setOpen] = useState(false);
  const [, start] = useTransition();
  if (blocks.length === 0) return null;

  const onRestore = (b: Props["blocks"][number]) => {
    const range = formatRange(b.started_at, b.ended_at);
    start(async () => {
      const r = await restore(b.id, false, day);
      if (r.ok) toast.ok(`Restored ${range}`);
      else toast.error(`Restore failed — ${r.error}`);
    });
  };

  return (
    <div className="review-line">
      <span className="review-summary">
        {blocks.length} ignored block{blocks.length === 1 ? "" : "s"}
      </span>
      <button
        type="button"
        className="review-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {open ? "Hide" : "Show"}
      </button>
      {open && (
        <ul className="ignored-list" role="list">
          {blocks.map((b) => (
            <li key={b.id} className="ignored-row">
              <span className="ignored-range">{formatRange(b.started_at, b.ended_at)}</span>
              <span className="ignored-desc">
                {b.description ? b.description : <em>No description</em>}
              </span>
              <button type="button" className="review-toggle" onClick={() => onRestore(b)}>
                Restore
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
