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

  // Split work vs personal. Personal blocks aren't candidates for
  // Jira/Tempo, so they don't count toward the unassigned amber-nag —
  // that nag fires for *work* blocks the user still needs to assign.
  const workBlocks = blocks.filter((b) => !b.is_personal);
  const personalBlocks = blocks.filter((b) => b.is_personal);
  const unassigned = workBlocks.filter((b) => !b.jira_issue).length;

  // Header total reflects work-only hours; personal time gets a
  // muted annotation so the focus is on billable time.
  const personalSeconds = personalBlocks.reduce(
    (acc, b) => acc + b.duration_seconds,
    0,
  );
  const workSeconds = Math.max(0, total - personalSeconds);
  const personalSummary =
    personalSeconds > 0 ? `${formatTotalHours(personalSeconds)} personal` : undefined;

  return (
    <>
      <DayHeader
        day={day}
        heading={formatDayHeading(day)}
        totalHours={formatTotalHours(workSeconds)}
        blockCount={workBlocks.length}
        unassigned={unassigned}
        personalSummary={personalSummary}
      />
      <ActionBar day={day} cacheCount={cache.count} cacheLast={cache.last_fetched} />
      {blocks.length === 0 ? (
        <EmptyState day={day} />
      ) : (
        <>
          {workBlocks.length > 0 ? (
            <ul className="blocks" role="list">
              {workBlocks.map((b) => (
                <li key={b.id}>
                  <BlockCard block={b} tickets={tickets} day={day} />
                </li>
              ))}
            </ul>
          ) : (
            <p className="day-empty-work">No work blocks today — only personal.</p>
          )}

          {personalBlocks.length > 0 && (
            <details className="personal-section">
              <summary>
                <span className="personal-section-count">
                  {personalBlocks.length} personal
                </span>
                <span className="personal-section-hours">
                  {formatTotalHours(personalSeconds)}
                </span>
                <span className="personal-section-hint">click to show</span>
              </summary>
              <ul className="blocks" role="list">
                {personalBlocks.map((b) => (
                  <li key={b.id}>
                    <BlockCard block={b} tickets={tickets} day={day} />
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </>
  );
}
