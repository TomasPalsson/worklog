import { notFound } from "next/navigation";
import { revalidatePath } from "next/cache";
import {
  call,
  DaemonError,
  fetchReviewLines,
  listTickets,
  loadBillingRegistry,
  loadDaySummary,
  routedForDay,
  verdictStatus,
} from "@/lib/daemon";
import { elsewhereForDay, type ElsewhereItem } from "@/lib/daemonElsewhere";
import { tempoLines } from "@/lib/daemonTempoLines";
import { billablePercent, type TempoLine } from "@/lib/tempo_line_contract";
import { formatDayHeading, formatTotalHours, todayISO } from "@/lib/format";
import { DayHeader } from "@/components/DayHeader";
import { ActionBar } from "@/components/ActionBar";
import { MirresAutoFetch } from "@/components/MirresAutoFetch";
import { BlockCard } from "@/components/BlockCard";
import { DayProgressProvider } from "@/components/DayProgressProvider";
import { DayStrip } from "@/components/DayStrip";
import { RecapBanner } from "@/components/RecapBanner";
import { ReviewSection } from "@/components/ReviewSection";
import { ElsewhereList } from "@/components/ElsewhereList";
import { EmptyState } from "@/components/EmptyState";
import { TicketGroup } from "@/components/TicketGroup";
import { IgnoredLine } from "@/components/IgnoredLine";
import { UnsortedList } from "@/components/UnsortedList";
import { VerdictBanner } from "@/components/VerdictBanner";
import { lastBlockEnd } from "@/lib/lastBlockEnd";
import type { Block, BillingRegistry, RoutedEvent } from "@/lib/types";
import type { GapAction, Recap } from "@/lib/daily_helpers_contract";

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

  const { gaps, overlaps, activity, allocations } = summary;
  // Ignored blocks (always is_personal too) are split out first so they
  // vanish from the strip, groups, totals and the personal section.
  const ignoredBlocks = summary.blocks.filter((b) => b.ignored_at);
  const blocks = summary.blocks.filter((b) => !b.ignored_at);
  const total =
    summary.total_seconds - ignoredBlocks.reduce((acc, b) => acc + b.duration_seconds, 0);
  const { tickets, meta: cache } = ticketsResp;
  const lastEnd = lastBlockEnd(blocks);

  // Browser/Slack events for the day (B12) — degrades to an empty feed on
  // a daemon hiccup rather than failing the whole page. The registry also
  // feeds the folder options below. `includeHidden`
  // pulls in noise + dismissed events too, so the zero-touch summary line
  // can report a hidden count without a second round trip when Review opens.
  let routedEvents: RoutedEvent[] = [];
  let registry: BillingRegistry | null = null;
  let elsewhereItems: ElsewhereItem[] = [];
  let lines: TempoLine[] = [];
  try {
    [routedEvents, registry, elsewhereItems, lines] = await Promise.all([
      routedForDay(day, true),
      loadBillingRegistry(),
      elsewhereForDay(day),
      tempoLines(day).catch((): TempoLine[] => []),
    ]);
  } catch {
    routedEvents = [];
    registry = null;
    elsewhereItems = [];
    lines = [];
  }
  const verdict = await verdictStatus(day).catch(() => null);
  const reviewLines =
    day === todayISO() ? await fetchReviewLines().catch(() => []) : [];
  const recap =
    day === todayISO() ? await call<Recap | null>("GET", "/recap").catch(() => null) : null;
  async function resolveGap(recapDay: string, startedAt: string, action: GapAction) {
    "use server";
    try {
      const data = await call<Recap | null>("POST", "/recap/gap", {
        day: recapDay,
        started_at: startedAt,
        ...action,
      });
      revalidatePath(`/${recapDay}`);
      return { ok: true as const, data };
    } catch (e) {
      return { ok: false as const, error: (e as Error).message };
    }
  }
  const folderOptions = registry
    ? Array.from(
        new Set([
          ...registry.folders.map((f) => f.folder),
          ...registry.unmapped.map((u) => u.folder),
        ]),
      ).sort()
    : [];

  // Split work vs personal. Personal blocks aren't candidates for
  // Jira/Tempo, so they don't count toward the unassigned amber-nag —
  // that nag fires for *work* blocks the user still needs to assign.
  const workBlocks = blocks.filter((b) => !b.is_personal);
  const personalBlocks = blocks.filter((b) => b.is_personal);
  const noTicketBlocks = workBlocks.filter((b) => !b.jira_issue);
  const unassigned = noTicketBlocks.length;

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

  // The day's billed total is the sum of the server's billed hours for
  // each ticket card. Unassigned groups, and groups with no line, don't bill.
  const billedSeconds = workGroups
    .filter((g) => !g.unassigned)
    .reduce((acc, g) => {
      const line = lines.find((l) => l.jira_issue === g.key);
      return acc + (line?.effective_seconds ?? 0);
    }, 0);

  return (
    <DayProgressProvider day={day}>
      <DayHeader
        day={day}
        heading={formatDayHeading(day)}
        totalHours={formatTotalHours(workSeconds)}
        blockCount={workBlocks.length}
        unassigned={unassigned}
        personalSummary={personalSummary}
        billedSeconds={billedSeconds}
        trackedSeconds={workSeconds}
        billablePercent={billablePercent(lines)}
      />
      <MirresAutoFetch day={day} needsFetch={lines.length > 0 && lines.every((l) => !l.billing)} />
      <ActionBar
        day={day}
        cacheCount={cache.count}
        cacheLast={cache.last_fetched}
        tickets={tickets}
        lastEnd={lastEnd}
      />
      <ReviewSection key={`review-${day}`} lines={reviewLines} />
      <RecapBanner key={`recap-${day}`} recap={recap} resolve={resolveGap} />
      <DayStrip
        day={day}
        blocks={blocks}
        gaps={gaps}
        overlaps={overlaps}
        activity={activity}
        allocations={allocations}
      />
      {verdict && <VerdictBanner key={`verdict-${day}`} status={verdict} />}
      <UnsortedList key={`unsorted-${day}`} day={day} events={routedEvents} folderOptions={folderOptions} />
      <ElsewhereList key={`elsewhere-${day}`} day={day} items={elsewhereItems} blocks={blocks} />
      {blocks.length === 0 ? (
        <EmptyState day={day} />
      ) : (
        <>
          {workGroups.length > 0 ? (
            <div className="ticket-groups">
              {workGroups.map((g) => (
                <TicketGroup
                  key={g.key}
                  group={g}
                  day={day}
                  line={g.unassigned ? undefined : lines.find((l) => l.jira_issue === g.key)}
                >
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
      <IgnoredLine blocks={ignoredBlocks} day={day} />
    </DayProgressProvider>
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
