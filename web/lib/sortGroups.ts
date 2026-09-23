// Pure grouping logic for the SortTray (rewritten UnsortedList): turns a
// flat list of unsorted browser/Slack events into one row per
// channel/DM/site, so the owner picks a project once for the whole group
// instead of once per message or page visit.

import type { RoutedEvent, RuleKind } from "./types";

export interface EventGroup {
  /** Stable React key AND the grouping identity. */
  key: string;
  /** Display name — Slack channel/DM name, hostname, or "github.com/org/repo". */
  label: string;
  source: string;
  events: RoutedEvent[];
  count: number;
  earliestAt: string;
  latestAt: string;
  /** The latest event's details — the freshest preview to show on the row. */
  latestPreview: string | null;
}

/** Which hard-rule kind an "always" tick creates, per source. */
export function ruleKindFor(source: string): RuleKind {
  return source === "slack" ? "slack_channel" : "domain";
}

function hostnameFromDetails(details: string | null): string | null {
  if (!details) return null;
  try {
    return new URL(details).hostname;
  } catch {
    return null;
  }
}

function githubRepoFromDetails(details: string | null): string | null {
  if (!details) return null;
  try {
    const segments = new URL(details).pathname.split("/").filter(Boolean);
    return segments.length >= 2 ? `${segments[0]}/${segments[1]}` : null;
  } catch {
    return null;
  }
}

/** The group identity + display label for one event. Slack groups by the
 * event's title (channel/DM name, already resolved upstream). Firefox
 * groups by hostname, except github.com groups by org/repo so separate
 * PRs/issues in the same repo land in one row. Anything without a
 * parseable URL falls back to its own title. */
function groupKeyFor(e: RoutedEvent): { key: string; label: string } {
  if (e.source === "slack") {
    return { key: `slack:${e.title}`, label: e.title };
  }
  const host = hostnameFromDetails(e.details);
  if (host === null) {
    return { key: `${e.source}:${e.title}`, label: e.title };
  }
  if (host === "github.com") {
    const repo = githubRepoFromDetails(e.details);
    const label = repo ? `github.com/${repo}` : host;
    return { key: `firefox:${label}`, label };
  }
  return { key: `firefox:${host}`, label: host };
}

/** Group events by channel/DM/hostname, sorted by count desc then latest
 * event time desc — the busiest, freshest groups need attention first. */
export function groupEvents(events: RoutedEvent[]): EventGroup[] {
  const map = new Map<string, EventGroup>();
  for (const e of events) {
    const { key, label } = groupKeyFor(e);
    let g = map.get(key);
    if (!g) {
      g = {
        key,
        label,
        source: e.source,
        events: [],
        count: 0,
        earliestAt: e.started_at,
        latestAt: e.started_at,
        latestPreview: e.details,
      };
      map.set(key, g);
    }
    g.events.push(e);
    g.count++;
    if (e.started_at < g.earliestAt) g.earliestAt = e.started_at;
    if (e.started_at > g.latestAt) {
      g.latestAt = e.started_at;
      g.latestPreview = e.details;
    }
  }

  return Array.from(map.values()).sort((a, b) => {
    if (b.count !== a.count) return b.count - a.count;
    return a.latestAt < b.latestAt ? 1 : a.latestAt > b.latestAt ? -1 : 0;
  });
}

/** The noun for a group's count badge — "7 messages" for Slack, "3
 * visits" for everything else (Firefox). */
export function countLabel(group: Pick<EventGroup, "source" | "count">): string {
  const noun = group.source === "slack" ? "message" : "visit";
  return `${group.count} ${noun}${group.count === 1 ? "" : "s"}`;
}

/**
 * The `always` rule-kind argument for each of `count` sequential
 * labelEvent/dismissEvent calls: the rule kind on the first call only
 * (which creates the hard rule), `null` for the rest — retroactive rule
 * application on the daemon side covers them, but we still label/dismiss
 * them explicitly to be safe.
 */
export function ruleKindOnFirst(count: number, ruleKind: RuleKind | null): Array<RuleKind | null> {
  return Array.from({ length: count }, (_, i) => (i === 0 ? ruleKind : null));
}
