import {
  CalendarDays,
  Github,
  Globe,
  MessagesSquare,
  Puzzle,
  Slack as SlackIcon,
  Ticket,
} from "lucide-react";
import { sourceKind, type SourceCount } from "@/lib/types";

interface Props {
  sources: SourceCount[];
}

// firefox/slack aren't part of the shared SourceKind bucket set — they get
// their own badge kind here instead of folding into "other".
function badgeKind(raw: string): string {
  if (raw === "firefox" || raw === "slack") return raw;
  return sourceKind(raw);
}

export function SourceBadges({ sources }: Props) {
  if (sources.length === 0) return null;
  // Aggregate by kind; raw source names ("github_commit" / "github_pr")
  // collapse into a single badge with combined count.
  const byKind = new Map<string, number>();
  for (const s of sources) {
    const k = badgeKind(s.source);
    byKind.set(k, (byKind.get(k) ?? 0) + s.n);
  }
  return (
    <>
      {Array.from(byKind.entries()).map(([kind, n]) => (
        <SourceBadge key={kind} kind={kind} count={n} />
      ))}
    </>
  );
}

function SourceBadge({ kind, count }: { kind: string; count: number }) {
  const meta = BADGE[kind] ?? BADGE.other;
  const Icon = meta.icon;
  return (
    <span
      className={`source-badge ${kind}`}
      title={`${meta.label} · ${count} event${count === 1 ? "" : "s"}`}
    >
      <Icon />
      <span>{count}</span>
    </span>
  );
}

const BADGE: Record<
  string,
  { icon: React.ComponentType<{ width?: number; height?: number }>; label: string }
> = {
  github: { icon: Github, label: "GitHub" },
  claude: { icon: MessagesSquare, label: "Claude Code" },
  gcal: { icon: CalendarDays, label: "Google Calendar" },
  jira: { icon: Ticket, label: "Jira" },
  firefox: { icon: Globe, label: "Firefox" },
  slack: { icon: SlackIcon, label: "Slack" },
  other: { icon: Puzzle, label: "Other" },
};
