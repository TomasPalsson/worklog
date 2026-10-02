"use client";

import { useEffect, useRef, type FocusEvent } from "react";
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
import type { TaskRow, Transition } from "@/lib/types";
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
  undo?: { to: string; run: () => void; hold: (held: boolean) => void };
  onRetry: () => void;
  onOpen: () => void;
  loadTransitions: () => ReturnType<TaskActions["loadTransitions"]>;
  onMove: (to: Column, transitions?: Transition[]) => void;
  onDismissError: () => void;
  onDragStart: (dataTransfer: DataTransfer | null) => void;
  onDragEnd: () => void;
}

function Hours({ task, pending, undoing }: Pick<TaskCardProps, "task" | "pending"> & { undoing: boolean }) {
  // The Undo strip sits over this row; keep the row's height so showing it never reflows the card.
  if (undoing) return <span className="task-card-hours" aria-hidden="true">{" "}</span>;
  if (pending) return <span className="task-card-hours">Moving…</span>;
  if (task.week_seconds === 0) return <span className="task-card-hours task-card-idle">Not worked this week</span>;
  const today = task.today_seconds > 0 ? ` · ${formatDuration(task.today_seconds)} today` : "";
  return <span className="task-card-hours">{`${formatDuration(task.week_seconds)} this week${today}`}</span>;
}

/** Hover or focus inside the card pauses the Undo countdown. */
function useUndoHold(undo: TaskCardProps["undo"]) {
  const flags = useRef({ hover: false, focus: false });
  const hold = undo?.hold;
  const set = (k: "hover" | "focus", v: boolean) => {
    flags.current[k] = v;
    hold?.(flags.current.hover || flags.current.focus);
  };
  const offered = hold !== undefined;
  useEffect(() => {
    if (offered && (flags.current.hover || flags.current.focus)) hold?.(true);
  }, [offered]); // only when a fresh strip appears under an already-held card
  return {
    onMouseEnter: () => set("hover", true),
    onMouseLeave: () => set("hover", false),
    onFocus: () => set("focus", true),
    onBlur: (e: FocusEvent) => {
      if (!e.currentTarget.contains(e.relatedTarget as Node | null)) set("focus", false);
    },
  };
}

export function TaskCard(p: TaskCardProps) {
  const { task } = p;
  const holdProps = useUndoHold(p.undo);
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
      {...holdProps}
    >
      <button type="button" className="task-card-btn" aria-expanded={p.selected} onClick={p.onOpen}>
        <span className="task-card-top">
          <span className="task-key">{task.key}</span>
          {!task.assigned && p.column !== "done" && (
            <span className="task-tag" title="Not assigned to you — shown because you logged time on it this week">
              not assigned
            </span>
          )}
        </span>
        <span className="task-summary">{task.summary}</span>
        {!p.error && <Hours task={task} pending={p.pending} undoing={!!p.undo && !p.pending} />}
      </button>
      <GripVertical className="task-grip" size={14} aria-hidden="true" />
      {!p.pending && <TaskMoveMenu taskKey={task.key} column={p.column} load={p.loadTransitions} onMove={p.onMove} />}
      {p.undo && !p.pending && (
        <div className="task-card-undo">
          <span>{`Moved to ${p.undo.to}`}</span>
          <button type="button" onClick={p.undo.run}>
            Undo
          </button>
        </div>
      )}
      {p.error && (
        <div className="task-card-error">
          <span>{p.error}</span>
          <button type="button" className="task-card-retry" onClick={p.onRetry}>
            Try again
          </button>
          <button type="button" aria-label="Dismiss error" onClick={p.onDismissError}>
            <X size={12} aria-hidden="true" />
          </button>
        </div>
      )}
    </li>
  );
}
