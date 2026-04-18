import { notFound } from "next/navigation";
import {
  dayTotalSeconds,
  listBlocksForDay,
  listTickets,
  ticketCacheMeta,
} from "@/lib/db";
import { formatDayHeading, formatTotalHours } from "@/lib/format";
import { DayHeader } from "@/components/DayHeader";
import { ActionBar } from "@/components/ActionBar";
import { BlockCard } from "@/components/BlockCard";
import { EmptyState } from "@/components/EmptyState";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

// Every request reads live DB state — never prerender at build time.
export const dynamic = "force-dynamic";

export default async function DayPage({
  params,
}: {
  params: Promise<{ day: string }>;
}) {
  const { day } = await params;
  if (!DAY_RE.test(day)) notFound();

  const [blocks, tickets, cache, total] = await Promise.all([
    listBlocksForDay(day),
    listTickets(),
    ticketCacheMeta(),
    dayTotalSeconds(day),
  ]);
  const unassigned = blocks.filter((b) => !b.jira_issue).length;

  return (
    <>
      <DayHeader
        day={day}
        heading={formatDayHeading(day)}
        totalHours={formatTotalHours(total)}
        blockCount={blocks.length}
        unassigned={unassigned}
      />
      <ActionBar day={day} cacheCount={cache.count} cacheLast={cache.last_fetched} />
      {blocks.length === 0 ? (
        <EmptyState day={day} />
      ) : (
        <ul className="blocks" role="list">
          {blocks.map((b) => (
            <li key={b.id}>
              <BlockCard block={b} tickets={tickets} day={day} />
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
