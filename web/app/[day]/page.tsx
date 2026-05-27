import { notFound } from "next/navigation";
import { DaemonError, listTickets, loadDaySummary } from "@/lib/daemon";
import { formatDayHeading, formatTotalHours } from "@/lib/format";
import { DayHeader } from "@/components/DayHeader";
import { ActionBar } from "@/components/ActionBar";
import { BlockCard } from "@/components/BlockCard";
import { EmptyState } from "@/components/EmptyState";
import { TicketGroup } from "@/components/TicketGroup";
import type { Block } from "@/lib/types";

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

  // Group work blocks by ticket. Each group renders as a collapsible
  // <details> so the day collapses to one row per ticket — which
  // matches how Tempo will see it after `worklog sync` aggregates.
  // Unassigned blocks land in a sentinel `__unassigned__` group that
  // defaults to open so the user is nudged to assign them.
  const workGroups = groupBlocksByTicket(workBlocks);

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
          {workGroups.length > 0 ? (
            <div className="ticket-groups">
              {workGroups.map((g) => (
                <TicketGroup key={g.key} group={g} day={day}>
                  <ul className="blocks" role="list">
                    {g.blocks.map((b) => (
                      <li key={b.id}>
                        <BlockCard
                          block={b}
                          tickets={tickets}
                          day={day}
                          isSoleInGroup={g.blocks.length === 1 && !g.unassigned}
                        />
                      </li>
                    ))}
                  </ul>
                </TicketGroup>
              ))}
            </div>
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
                    <BlockCard
                      block={b}
                      tickets={tickets}
                      day={day}
                      isSoleInGroup={false}
                    />
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

/** One grouped row in the day view: a ticket plus the blocks under it. */
export interface BlockGroup {
  /** Stable React key. Either the Jira issue key or `__unassigned__`. */
  key: string;
  /** Display label — Jira issue key or "Unassigned". */
  label: string;
  /** True when the group is the sentinel unassigned bucket. */
  unassigned: boolean;
  blocks: Block[];
  totalSeconds: number;
  /** Sync state across the group's member blocks. `mixed` means some
   * blocks are synced and others aren't — the next sync will bring
   * the unsynced ones in. */
  syncState: "synced" | "dirty" | "unsynced" | "mixed";
  /** The description that would land in Tempo if the user synced now
   * (using the offline joined-descriptions fallback — Claude
   * summarisation happens at sync time, not on every page render). */
  previewDescription: string;
  /** Defaults: unassigned and dirty/unsynced groups open so attention
   * is on them. Fully-synced clean groups collapse by default. */
  defaultOpen: boolean;
}

function groupBlocksByTicket(blocks: Block[]): BlockGroup[] {
  const map = new Map<string, Block[]>();
  for (const b of blocks) {
    const key = b.jira_issue ?? "__unassigned__";
    const list = map.get(key);
    if (list) list.push(b);
    else map.set(key, [b]);
  }

  const groups: BlockGroup[] = [];
  for (const [key, members] of map) {
    const unassigned = key === "__unassigned__";
    const totalSeconds = members.reduce((acc, b) => acc + b.duration_seconds, 0);
    const syncState = computeSyncState(members);
    const previewDescription = buildPreviewDescription(
      unassigned ? null : key,
      members,
    );
    const defaultOpen =
      unassigned || syncState === "unsynced" || syncState === "dirty" || syncState === "mixed";
    groups.push({
      key,
      label: unassigned ? "Unassigned" : key,
      unassigned,
      blocks: members,
      totalSeconds,
      syncState,
      previewDescription,
      defaultOpen,
    });
  }

  // Unassigned first (it's the action-required group), then tickets
  // sorted alphabetically by key for stable, scannable ordering.
  groups.sort((a, b) => {
    if (a.unassigned && !b.unassigned) return -1;
    if (b.unassigned && !a.unassigned) return 1;
    return a.label.localeCompare(b.label);
  });
  return groups;
}

function computeSyncState(blocks: Block[]): BlockGroup["syncState"] {
  let anySynced = false;
  let anyUnsynced = false;
  let anyDirty = false;
  for (const b of blocks) {
    const synced = !!b.tempo_worklog_id && b.tempo_worklog_id.trim() !== "";
    if (synced) {
      anySynced = true;
      if (b.dirty) anyDirty = true;
    } else {
      anyUnsynced = true;
    }
  }
  if (anyDirty) return "dirty";
  if (anySynced && anyUnsynced) return "mixed";
  if (anySynced) return "synced";
  return "unsynced";
}

/** Joined preview of distinct non-empty descriptions — mirrors the
 * Rust-side fallback so the UI shows roughly what Tempo will receive
 * if Claude summarisation is unavailable. Capped at 200 chars. */
function buildPreviewDescription(issue: string | null, blocks: Block[]): string {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const b of blocks) {
    const d = b.description?.trim() ?? "";
    if (!d || seen.has(d)) continue;
    seen.add(d);
    unique.push(d);
  }
  if (unique.length === 0) {
    return issue ? `Work on ${issue}` : "No descriptions yet";
  }
  if (unique.length === 1) return cap(unique[0], 200);
  return cap(unique.join("; "), 200);
}

function cap(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1) + "…";
}
