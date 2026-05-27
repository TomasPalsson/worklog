"use client";

import { ReactNode, useRef, useTransition } from "react";
import { GitMerge } from "lucide-react";
import type { BlockGroup } from "@/app/[day]/page";
import { formatTotalHours } from "@/lib/format";
import { canMergeGroup } from "@/lib/group-actions";
import { mergeGroup } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  group: BlockGroup;
  day: string;
  children: ReactNode;
}

/**
 * Collapsible row that mirrors how Tempo will see the day after sync:
 * one entry per (day, ticket). Click the summary to expand and
 * see/edit the individual blocks underneath.
 *
 * Uses native <details> for keyboard + a11y for free — same pattern
 * as the personal-section in app/[day]/page.tsx.
 *
 * Assigned multi-block groups also get a "Merge all" button that
 * folds the rest of the group into the earliest-start block.
 */
export function TicketGroup({ group, day, children }: Props) {
  const blockNoun = group.blocks.length === 1 ? "block" : "blocks";
  const showMerge = canMergeGroup(group);
  const [pending, startTransition] = useTransition();
  const summaryRef = useRef<HTMLElement>(null);

  const runMerge = () => {
    if (pending) return;
    const sorted = [...group.blocks].sort((a, b) =>
      a.started_at < b.started_at ? -1 : a.started_at > b.started_at ? 1 : 0,
    );
    const primary = sorted[0]?.id;
    if (primary === undefined) return;
    const absorb = sorted.slice(1).map((b) => b.id);
    startTransition(async () => {
      const r = await mergeGroup(primary, absorb, day);
      if (!r.ok) {
        toast.error(`Merge failed — ${r.error}`);
      } else {
        toast.ok(`Merged ${absorb.length + 1} blocks on ${group.label}`);
        // After a successful merge the button itself unmounts (the
        // group now has 1 block, so canMergeGroup → false). Park focus
        // back on the group's summary row so keyboard users don't fall
        // to <body>.
        summaryRef.current?.focus();
      }
    });
  };

  // Two-pronged interception. Pointer/click: stopPropagation +
  // preventDefault keep the <details> from toggling. Keyboard: handle
  // Enter/Space on the button BEFORE the synthetic click reaches
  // <summary>, since some browsers route keyboard activation through
  // the summary's default action even when a descendant has focus.
  const onMergeClick: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    e.preventDefault();
    runMerge();
  };
  const onMergeKeyDown: React.KeyboardEventHandler<HTMLButtonElement> = (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.stopPropagation();
      e.preventDefault();
      runMerge();
    }
  };

  return (
    <details
      className={`ticket-group ${group.unassigned ? "unassigned" : "assigned"} sync-${group.syncState}`}
      open={group.defaultOpen}
    >
      <summary ref={summaryRef} tabIndex={0}>
        <span className="ticket-group-label">{group.label}</span>
        <span className="ticket-group-meta">
          {group.blocks.length} {blockNoun}
        </span>
        <span className="ticket-group-meta">{formatTotalHours(group.totalSeconds)}</span>
        <SyncChip state={group.syncState} />
        <span className="ticket-group-description" title={group.previewDescription}>
          {group.previewDescription}
        </span>
        {showMerge && (
          <button
            type="button"
            className="merge-btn"
            disabled={pending}
            aria-busy={pending || undefined}
            onClick={onMergeClick}
            onKeyDown={onMergeKeyDown}
            title={`Merge all ${group.blocks.length} blocks on ${group.label} into one`}
            aria-label={`merge all blocks on ${group.label}`}
          >
            <GitMerge aria-hidden="true" />
            {/* Text is wrapped in a polite live region so screen readers
                announce the "Merging…" → "Merge all" flip — aria-busy
                alone doesn't trigger an announcement in most ATs. */}
            <span aria-live="polite">{pending ? "Merging…" : "Merge all"}</span>
          </button>
        )}
        <span className="ticket-group-hint" aria-hidden="true" />
      </summary>
      <div className="ticket-group-body">{children}</div>
    </details>
  );
}

function SyncChip({ state }: { state: BlockGroup["syncState"] }) {
  // Labels intentionally terse — they appear inline in a dense summary
  // row and the colour carries most of the meaning.
  const label =
    state === "synced"
      ? "synced"
      : state === "dirty"
        ? "edited"
        : state === "mixed"
          ? "partial"
          : "unsynced";
  return <span className={`sync-chip sync-chip-${state}`}>{label}</span>;
}
