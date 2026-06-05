import { ReactNode } from "react";
import type { BlockGroup } from "@/app/[day]/page";
import { formatBilledHours, formatTotalHours } from "@/lib/format";

interface Props {
  group: BlockGroup;
  children: ReactNode;
}

/**
 * Collapsible row that mirrors how Tempo will see the day after sync:
 * one entry per (day, ticket). Click the summary to expand and
 * see/edit the individual blocks underneath.
 *
 * Uses native <details> for keyboard + a11y for free — same pattern
 * as the personal-section in app/[day]/page.tsx.
 */
export function TicketGroup({ group, children }: Props) {
  const blockNoun = group.blocks.length === 1 ? "block" : "blocks";
  return (
    <details
      className={`ticket-group ${group.unassigned ? "unassigned" : "assigned"} sync-${group.syncState}`}
      open={group.defaultOpen}
    >
      <summary>
        <span className="ticket-group-label">{group.label}</span>
        <span className="ticket-group-meta">
          {group.blocks.length} {blockNoun}
        </span>
        {group.unassigned ? (
          <span className="ticket-group-meta">
            {formatTotalHours(group.totalSeconds)}
          </span>
        ) : (
          // Assigned groups sync as one Tempo worklog, rounded to the
          // nearest half hour — show what will actually be billed, with
          // the raw tracked time in the tooltip. "0h" flags a group
          // under 15 min that won't sync.
          <span
            className="ticket-group-meta"
            title={`${formatTotalHours(group.totalSeconds)} tracked`}
          >
            {formatBilledHours(group.totalSeconds)} billed
          </span>
        )}
        <SyncChip state={group.syncState} />
        <span className="ticket-group-description" title={group.previewDescription}>
          {group.previewDescription}
        </span>
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
