"use client";

import { useRef, useState } from "react";
import { EyeOff } from "lucide-react";
import type { Block } from "@/lib/types";
import { formatRange } from "@/lib/format";
import { useMenuDismiss } from "./useMenuDismiss";
import { useIgnoreActions } from "./useIgnoreActions";

interface Props {
  block: Block;
  day: string;
  /** The day's blocks; Duplicate merges into the nearest same-ticket one. */
  dayBlocks: Block[];
  /** Internal: open the ticket picker instead of ignoring. */
  onInternal: () => void;
}

/** The ignore button and its Noise / Duplicate / Internal menu (FR-06). */
export function IgnoreMenu({ block, day, dayBlocks, onInternal }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const rootRef = useRef<HTMLSpanElement>(null);
  const synced = !!block.tempo_worklog_id;
  const exported = !!block.exported_at;
  const timeRangeLabel = formatRange(block.started_at, block.ended_at);

  useMenuDismiss(menuOpen, rootRef, () => setMenuOpen(false));

  const { onIgnore, onDuplicate, isPending } = useIgnoreActions(block, day, dayBlocks, () => setMenuOpen(false));

  return (
    <span ref={rootRef} style={{ display: "contents" }}>
      <button
        type="button"
        className="icon-btn"
        title={ignoreTitle(synced, exported)}
        aria-label={`ignore ${timeRangeLabel} block`}
        aria-busy={isPending || undefined}
        disabled={synced || exported}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen((o) => !o)}
      >
        <EyeOff />
      </button>
      {menuOpen && (
        <div role="menu" className="ignore-menu">
          {([
            ["Noise", () => onIgnore("noise")],
            ["Duplicate", onDuplicate],
            ["Internal", () => (setMenuOpen(false), onInternal())],
          ] as const).map(([label, onClick]) => (
            <button key={label} type="button" role="menuitem" onClick={onClick}>{label}</button>
          ))}
        </div>
      )}
    </span>
  );
}

function ignoreTitle(synced: boolean, exported: boolean): string {
  if (synced) return "Already synced to Tempo — remove the worklog there first";
  if (exported) return "Already exported for billing";
  return "Ignore — hide from the day, billing and Tempo. Survives Rebuild.";
}
