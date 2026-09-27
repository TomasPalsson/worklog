"use client";

// The day page's per-day "done elsewhere" list (spec 006, FR-05/FR-06):
// org commits/PRs whose sha is in no local clone, moved into a chosen
// block by hand. Mirrors UnsortedList's look.

import { useState, useTransition } from "react";
import { moveElsewhereEvent } from "@/app/actions-elsewhere";
import { toast } from "@/lib/toast";
import { formatEventTime } from "@/lib/format-event";
import { formatRange } from "@/lib/format";
import type { Block } from "@/lib/types";
import type { ElsewhereItem } from "@/lib/daemonElsewhere";
import { ArrowRightToLine } from "lucide-react";
import { GitHubMark } from "./SourceIcon";

interface Props {
  day: string;
  items: ElsewhereItem[];
  blocks: Block[];
}

/** The block whose time is closest to the item — the likely home. */
function nearestBlock(blocks: Block[], at: string): number | "" {
  const t = Date.parse(at);
  let best: Block | null = null;
  let bestGap = Infinity;
  for (const b of blocks) {
    const gap = Math.max(0, Date.parse(b.started_at) - t, t - Date.parse(b.ended_at));
    if (gap < bestGap) [best, bestGap] = [b, gap];
  }
  return best?.id ?? "";
}

function blockLabel(b: Block): string {
  const detail = b.description?.trim() || b.project || b.project_path || "no folder";
  return `${formatRange(b.started_at, b.ended_at)} · ${detail}`;
}

export function ElsewhereList({ day, items, blocks }: Props) {
  const [rows, setRows] = useState(items);
  if (rows.length === 0) return null;

  function removeRow(id: number) {
    setRows((prev) => prev.filter((r) => r.id !== id));
  }

  return (
    <details className="elsewhere-list" open>
      <summary>
        <span className="elsewhere-list-title">Done elsewhere</span>
        <span className="elsewhere-list-count">
          {rows.length} item{rows.length === 1 ? "" : "s"}
        </span>
        <span className="elsewhere-list-hint">
          Commits not made on this Mac — not billed unless you move them into a block
        </span>
      </summary>
      <ul className="elsewhere-list-body" role="list">
        {rows.map((item) => (
          <ElsewhereRow key={item.id} item={item} day={day} blocks={blocks} onMoved={removeRow} />
        ))}
      </ul>
    </details>
  );
}

function ElsewhereRow({
  item,
  day,
  blocks,
  onMoved,
}: {
  item: ElsewhereItem;
  day: string;
  blocks: Block[];
  onMoved: (id: number) => void;
}) {
  const [blockId, setBlockId] = useState<number | "">(() => nearestBlock(blocks, item.started_at));
  const [pending, start] = useTransition();
  const [error, setError] = useState<string | null>(null);

  function move() {
    if (blockId === "") return;
    setError(null);
    start(async () => {
      const r = await moveElsewhereEvent(item.id, blockId, day);
      if (!r.ok) {
        setError(r.error);
        toast.error(`Couldn't move — ${r.error}`);
        return;
      }
      toast.ok(`Moved "${item.title}" into the block`);
      onMoved(item.id);
    });
  }

  return (
    <li className="elsewhere-row">
      <span className="elsewhere-row-time">{formatEventTime(item.started_at)}</span>
      <span className="elsewhere-row-icon" aria-hidden="true">
        <GitHubMark size={13} />
      </span>
      <span className="elsewhere-row-main">
        <span className="elsewhere-row-title">{item.title}</span>
        {item.repo && <span className="elsewhere-row-repo">{item.repo}</span>}
      </span>
      <select
        aria-label={`Block for ${item.title}`}
        value={blockId}
        disabled={pending}
        onChange={(e) => setBlockId(e.target.value === "" ? "" : Number(e.target.value))}
      >
        <option value="">Choose a block…</option>
        {blocks.map((b) => (
          <option key={b.id} value={b.id}>
            {blockLabel(b)}
          </option>
        ))}
      </select>
      <button
        type="button"
        disabled={pending || blockId === ""}
        aria-busy={pending || undefined}
        onClick={move}
      >
        <ArrowRightToLine width={12} height={12} aria-hidden="true" />
        Move
      </button>
      {error && (
        <p className="picker-error" role="alert">
          {error}
        </p>
      )}
    </li>
  );
}
