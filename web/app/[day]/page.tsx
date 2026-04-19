import { notFound } from "next/navigation";
import { DaemonError, listTickets, loadDaySummary } from "@/lib/daemon";
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

  // Both reads go to the daemon — this is the fix for the WAL stale-read
  // bug where the container's direct bun:sqlite reader couldn't see the
  // host daemon's writes through Docker Desktop's VFS.
  let summary: Awaited<ReturnType<typeof loadDaySummary>>;
  let ticketsResp: Awaited<ReturnType<typeof listTickets>>;
  try {
    [summary, ticketsResp] = await Promise.all([
      loadDaySummary(day),
      listTickets(),
    ]);
  } catch (e) {
    // Route any daemon failure to the error boundary with a clearer
    // message than the raw fetch error. The boundary renders an empty-
    // state that tells the user how to start the daemon.
    if (e instanceof DaemonError) {
      throw new Error(
        `Can't reach the worklog daemon — start it on the host with ` +
          `\`worklog daemon\` or \`worklog daemon install\`. (${e.message})`,
      );
    }
    throw e;
  }

  const { blocks, total_seconds: total } = summary;
  const { tickets, meta: cache } = ticketsResp;
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
