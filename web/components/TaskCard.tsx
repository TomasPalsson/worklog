"use client";

import type {
  loadTransitions as loadTransitionsAction,
  loadTicketDetail as loadTicketDetailAction,
  transitionTicket as transitionTicketAction,
  commentOnTicket as commentOnTicketAction,
  draftTicketUpdate as draftTicketUpdateAction,
} from "@/app/actions-hub";
import { formatDuration } from "@/lib/format";
import type { Column } from "@/lib/taskBoard";
import type { TaskRow } from "@/lib/types";

export interface TaskActions {
  loadTransitions: typeof loadTransitionsAction;
  loadTicketDetail: typeof loadTicketDetailAction;
  transitionTicket: typeof transitionTicketAction;
  commentOnTicket: typeof commentOnTicketAction;
  draftTicketUpdate: typeof draftTicketUpdateAction;
}

export interface TaskCardProps {
  task: TaskRow;
  column: Column;
  selected: boolean;
  pending: boolean;
  dragging: boolean;
  error: string | undefined;
  onOpen: () => void;
  onDragStart: (dataTransfer: DataTransfer | null) => void;
  onDragEnd: () => void;
}

function Hours({ task, pending, error }: Pick<TaskCardProps, "task" | "pending" | "error">) {
  if (pending) return <span className="task-card-hours">Moving…</span>;
  if (error) return <span className="task-card-hours task-card-error">{error}</span>;
  if (task.week_seconds === 0) return <span className="task-card-hours task-card-idle">Not worked this week</span>;
  return (
    <span className="task-card-hours">
      {`${formatDuration(task.week_seconds)} this week · ${formatDuration(task.today_seconds)} today`}
    </span>
  );
}

export function TaskCard(p: TaskCardProps) {
  const { task } = p;
  return (
    <li
      className="task-card"
      data-testid={`card-${task.key}`}
      data-task-key={task.key}
      data-column={p.column}
      data-selected={p.selected || undefined}
      data-pending={p.pending || undefined}
      data-dragging={p.dragging || undefined}
      draggable={!p.pending}
      onDragStart={(e) => p.onDragStart(e.dataTransfer ?? null)}
      onDragEnd={p.onDragEnd}
    >
      <button type="button" className="task-card-btn" aria-expanded={p.selected} onClick={p.onOpen}>
        <span className="task-card-top">
          <span className="task-key">{task.key}</span>
          {!task.assigned && <span className="task-tag">worked</span>}
        </span>
        <span className="task-summary">{task.summary}</span>
        <Hours task={task} pending={p.pending} error={p.error} />
      </button>
    </li>
  );
}
