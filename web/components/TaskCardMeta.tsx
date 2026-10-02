// Small presentational pieces for the richer My Tasks card and panel.

import {
  Bookmark,
  Bug,
  ChevronDown,
  ChevronUp,
  ChevronsDown,
  ChevronsUp,
  Circle,
  CornerDownRight,
  Equal,
  SquareCheck,
  Zap,
  type LucideIcon,
} from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { Column } from "@/lib/taskBoard";
import { dueState, relativeAge, shortDate, sparkHeights } from "@/lib/taskBoard";

const TYPES: Record<string, { kind: string; Icon: LucideIcon; color: string }> = {
  bug: { kind: "bug", Icon: Bug, color: "var(--terracotta-ink)" },
  story: { kind: "story", Icon: Bookmark, color: "var(--sage-ink)" },
  task: { kind: "task", Icon: SquareCheck, color: "var(--slate-ink)" },
  epic: { kind: "epic", Icon: Zap, color: "var(--violet)" },
  "sub-task": { kind: "subtask", Icon: CornerDownRight, color: "var(--fg-muted)" },
  subtask: { kind: "subtask", Icon: CornerDownRight, color: "var(--fg-muted)" },
};
const OTHER = { kind: "other", Icon: Circle, color: "var(--fg-subtle)" };

export function TypeIcon({ type }: { type: string }) {
  const { kind, Icon, color } = TYPES[type.toLowerCase()] ?? OTHER;
  return (
    <span className="task-type" data-type={kind} style={{ color }} title={type}>
      <Icon size={14} aria-hidden="true" />
      <span className="task-sr">{type}</span>
    </span>
  );
}

const PRIORITIES: Record<string, { Icon: LucideIcon; color: string }> = {
  highest: { Icon: ChevronsUp, color: "var(--terracotta-ink)" },
  high: { Icon: ChevronUp, color: "var(--terracotta-ink)" },
  medium: { Icon: Equal, color: "var(--fg-subtle)" },
  low: { Icon: ChevronDown, color: "var(--slate-ink)" },
  lowest: { Icon: ChevronsDown, color: "var(--slate-ink)" },
};

export function PriorityGlyph({ priority }: { priority: string }) {
  const p = PRIORITIES[priority.toLowerCase()];
  if (!p) return null;
  return (
    <span className="task-priority" style={{ color: p.color }} title={`Priority ${priority}`}>
      <p.Icon size={14} aria-hidden="true" />
      <span className="task-sr">{`Priority ${priority}`}</span>
    </span>
  );
}

/** Done cards never alarm: they read as neutral whatever the date says. */
export function DueChip({ due, today, done = false }: { due: string; today: string; done?: boolean }) {
  const { state, days } = dueState(due, today);
  const text = state === "overdue" ? `Overdue ${-days}d` : state === "today" ? "Due today" : `Due ${shortDate(due)}`;
  return (
    <span className="task-due" data-state={done ? "later" : state}>
      {text}
    </span>
  );
}

export function Labels({ labels }: { labels: string[] }) {
  if (labels.length === 0) return null;
  return (
    <span className="task-labels">
      {labels.slice(0, 3).map((l) => (
        <span key={l} className="task-label-chip">
          {l}
        </span>
      ))}
      {labels.length > 3 && <span className="task-label-chip">{`+${labels.length - 3}`}</span>}
    </span>
  );
}

export function ParentRow({ text }: { text: string }) {
  return (
    <span className="task-parent" title={text}>
      <CornerDownRight size={12} aria-hidden="true" />
      <span>{text}</span>
    </span>
  );
}

export function UpdatedAgo({ iso, now = new Date() }: { iso: string; now?: Date }) {
  const age = relativeAge(iso, now);
  return age ? <span className="task-updated">{`Updated ${age}`}</span> : null;
}

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** Monday-first weekday index of a YYYY-MM-DD date. */
const weekdayIndex = (ymd: string): number => {
  const [y, m, d] = ymd.split("-").map(Number);
  return (new Date(Date.UTC(y, m - 1, d)).getUTCDay() + 6) % 7;
};

/** Seven bars, Monday first, scaled to the board-wide busiest day. */
export function WeekSpark({ daySeconds, max, column, today }: {
  daySeconds: number[];
  max: number;
  column: Column;
  today: string;
}) {
  const heights = sparkHeights(daySeconds, max);
  const now = weekdayIndex(today);
  const label = `This week: ${daySeconds.map((s, i) => `${DAYS[i]} ${formatDuration(s)}`).join(", ")}`;
  return (
    <span className="task-spark" role="img" aria-label={label} data-column={column}>
      {heights.map((h, i) => (
        <i
          key={DAYS[i]}
          style={{ height: `${h}px` }}
          data-today={i === now || undefined}
          data-zero={daySeconds[i] > 0 ? undefined : true}
        />
      ))}
    </span>
  );
}
