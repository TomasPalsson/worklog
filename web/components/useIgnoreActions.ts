"use client";

import { useTransition } from "react";
import type { Block } from "@/lib/types";
import { formatRange } from "@/lib/format";
import { mergeGroup, setIgnored, undoLastChange } from "@/app/actions";
import { toast } from "@/lib/toast";
import type { IgnoreReason } from "@/lib/time_leaks_contract";

/** Ignore and Duplicate (merge into the nearest same-ticket block) handlers; `close` runs before the action starts. */
export function useIgnoreActions(block: Block, day: string, dayBlocks: Block[], close: () => void) {
  const [isPending, start] = useTransition();
  const timeRangeLabel = formatRange(block.started_at, block.ended_at);

  const onIgnore = (reason: IgnoreReason) => {
    close();
    start(async () => {
      const r = await setIgnored(block.id, true, day, reason);
      if (!r.ok) {
        toast.error(`Ignore failed — ${r.error}`);
        return;
      }
      toast.undoable(`Ignored ${timeRangeLabel}`, () => undoLastChange(day));
    });
  };

  const onDuplicate = () => {
    const neighbour = nearestSameTicket(block, dayBlocks);
    // An overlapping twin already counts this time; merging would sum it twice.
    if (!neighbour || overlaps(block, neighbour)) return onIgnore("duplicate");
    close();
    start(async () => {
      const r = await mergeGroup(neighbour.id, [block.id], day);
      if (!r.ok) toast.error(`Merge failed — ${r.error}`);
      else toast.ok(`Merged ${timeRangeLabel} into ${formatRange(neighbour.started_at, neighbour.ended_at)}`);
    });
  };

  return { onIgnore, onDuplicate, isPending };
}

const overlaps = (a: Block, b: Block) =>
  Date.parse(a.started_at) < Date.parse(b.ended_at) && Date.parse(b.started_at) < Date.parse(a.ended_at);

/** Nearest (by gap between intervals) other live block on the same day and ticket. */
function nearestSameTicket(block: Block, blocks: Block[]): Block | undefined {
  const start = Date.parse(block.started_at);
  const end = Date.parse(block.ended_at);
  const gap = (o: Block) => Math.max(0, Date.parse(o.started_at) - end, start - Date.parse(o.ended_at));
  return blocks
    // A synced, exported or personal block is never a merge target: absorbing time would change billed hours.
    .filter((o) => o.id !== block.id && o.day === block.day && o.jira_issue && o.jira_issue === block.jira_issue
      && !o.ignored_at && !o.tempo_worklog_id && !o.exported_at && !o.is_personal)
    .reduce<Block | undefined>((best, o) => (!best || gap(o) < gap(best) ? o : best), undefined);
}
