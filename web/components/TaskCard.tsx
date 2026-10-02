"use client";

import { GripVertical, X } from "lucide-react";

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
import { TaskMoveMenu } from "./TaskMoveMenu";

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
  landed: boolean;
  error: string | undefined;
  onOpen: () => void;
  onMove: (to: Column) => void;
  onDismissError: () => void;
  onDragStart: (dataTransfer: DataTransfer | null) => void;
  onDragEnd: () => void;
}

function Hours({ task, pending }: Pick<TaskCardProps, "task" | "pending">) {
  if (pending) return <span className="task-card-hours">Moving…</span>;
  if (task.week_seconds === 0) return <span className="task-card-hours task-card-idle">Not worked this week</span>;
  const today = task.today_seconds > 0 ? ` · ${formatDuration(task.today_seconds)} today` : "";
  return <span className="task-card-hours">{`${formatDuration(task.week_seconds)} this week${today}`}</span>;
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
      data-landed={p.landed || undefined}
      draggable={!p.pending}
      onDragStart={(e) => p.onDragStart(e.dataTransfer ?? null)}
      onDragEnd={p.onDragEnd}
    >
      <button type="button" className="task-card-btn" aria-expanded={p.selected} onClick={p.onOpen}>
        <span className="task-card-top">
          <span className="task-key">{task.key}</span>
          {!task.assigned && (
            <span className="task-tag" title="Not assigned to you — shown because you logged time on it this week">
              not assigned
            </span>
          )}
        </span>
        <span className="task-summary">{task.summary}</span>
        {!p.error && <Hours task={task} pending={p.pending} />}
      </button>
      <GripVertical className="task-grip" size={14} aria-hidden="true" />
      {!p.pending && <TaskMoveMenu taskKey={task.key} column={p.column} onMove={p.onMove} />}
      {p.error && (
        <div className="task-card-error">
          <span title={p.error}>{p.error}</span>
          <button type="button" aria-label="Dismiss error" onClick={p.onDismissError}>
            <X size={12} aria-hidden="true" />
          </button>
        </div>
      )}
    </li>
  );
}
