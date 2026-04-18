import { Sparkles, CircleDashed, PenLine } from "lucide-react";

/**
 * Shows how a block's description was filled in:
 *   manual — the user typed it
 *   claude — AI estimator filled it
 *   gap    — no events, block was inferred from a gap in activity
 *            (surfaced with an amber tint so the user knows it's weak data)
 */
export function EstBadge({ kind }: { kind: string | null | undefined }) {
  if (!kind) return null;
  const meta = LABELS[kind];
  if (!meta) return null;
  const Icon = meta.icon;
  return (
    <span className="est-badge" data-kind={kind} title={meta.tooltip}>
      <Icon />
      {meta.label}
    </span>
  );
}

const LABELS: Record<
  string,
  {
    icon: React.ComponentType<{ width?: number; height?: number }>;
    label: string;
    tooltip: string;
  }
> = {
  manual: {
    icon: PenLine,
    label: "manual",
    tooltip: "You edited this block — it won't be overwritten by re-estimation",
  },
  claude: {
    icon: Sparkles,
    label: "claude",
    tooltip: "Claude filled in the ticket and description from the events",
  },
  gap: {
    icon: CircleDashed,
    label: "gap",
    tooltip:
      "Block inferred from a stretch of activity without a clear signal — review before syncing",
  },
};
