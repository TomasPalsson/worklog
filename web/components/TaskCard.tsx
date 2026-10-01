"use client";

import { useState } from "react";
import { Sparkles } from "lucide-react";

import type {
  loadTransitions as loadTransitionsAction,
  transitionTicket as transitionTicketAction,
  commentOnTicket as commentOnTicketAction,
  draftTicketUpdate as draftTicketUpdateAction,
} from "@/app/actions-hub";
import { formatDuration } from "@/lib/format";
import type { TaskRow, Transition } from "@/lib/types";

export interface TaskActions {
  loadTransitions: typeof loadTransitionsAction;
  transitionTicket: typeof transitionTicketAction;
  commentOnTicket: typeof commentOnTicketAction;
  draftTicketUpdate: typeof draftTicketUpdateAction;
}

type Outcome<T> = { ok: true; data: T } | { ok: false; error: string };

function useTaskCard(task: TaskRow, actions: TaskActions) {
  const [status, setStatus] = useState(task.status);
  const [transitions, setTransitions] = useState<Transition[] | null>(null);
  const [suggestedId, setSuggestedId] = useState<string | null>(null);
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function attempt<T>(call: () => Promise<Outcome<T>>, onOk: (data: T) => void) {
    setBusy(true);
    setError(null);
    const res = await call();
    if (res.ok) onOk(res.data);
    else setError(res.error);
    setBusy(false);
  }

  return {
    status,
    transitions,
    suggestedId,
    comment,
    busy,
    error,
    setComment,
    openStatusMenu: () => attempt(() => actions.loadTransitions(task.key), setTransitions),
    move: (t: Transition) =>
      attempt(
        () => actions.transitionTicket(task.key, t.id),
        (next) => {
          setStatus(next.status);
          setTransitions(null);
          setSuggestedId(null);
        },
      ),
    post: () =>
      attempt(
        () => actions.commentOnTicket(task.key, comment.trim()),
        () => setComment(""),
      ),
    draft: () =>
      attempt(
        () => actions.draftTicketUpdate(task.key),
        (d) => {
          setComment(d.comment);
          setTransitions(d.transitions);
          setSuggestedId(d.suggested_transition_id);
        },
      ),
  };
}

function TaskHead({ task, status }: { task: TaskRow; status: string | null }) {
  return (
    <>
      <div className="task-card-head">
        {task.url ? (
          <a href={task.url} className="task-key" target="_blank" rel="noreferrer">
            {task.key}
          </a>
        ) : (
          <span className="task-key">{task.key}</span>
        )}
        <span className="task-summary">{task.summary}</span>
        <span
          className="task-status"
          data-category={task.status_category ?? undefined}
          data-testid={`status-${task.key}`}
        >
          {status}
        </span>
      </div>
      <div className="task-hours">
        <span>{formatDuration(task.week_seconds)} this week</span>
        <span>{formatDuration(task.today_seconds)} today</span>
      </div>
    </>
  );
}

export function TaskCard({ task, actions }: { task: TaskRow; actions: TaskActions }) {
  const s = useTaskCard(task, actions);
  return (
    <li className="task-card">
      <TaskHead task={task} status={s.status} />

      {s.transitions && (
        <div className="task-transitions">
          {s.transitions.map((t) => (
            <button
              key={t.id}
              type="button"
              className="merge-btn"
              disabled={s.busy}
              onClick={() => s.move(t)}
            >
              {`${t.name} → ${t.to_status}${t.id === s.suggestedId ? " (suggested)" : ""}`}
            </button>
          ))}
        </div>
      )}

      <textarea
        className="task-comment"
        aria-label={`Comment on ${task.key}`}
        rows={2}
        value={s.comment}
        onChange={(e) => s.setComment(e.target.value)}
      />
      <div className="task-actions">
        <button type="button" className="merge-btn" disabled={s.busy} onClick={s.openStatusMenu}>
          Change status
        </button>
        <button type="button" className="merge-btn" disabled={s.busy} onClick={s.draft}>
          <Sparkles size={12} aria-hidden="true" />
          Draft with AI
        </button>
        <button
          type="button"
          className="merge-btn"
          disabled={s.busy || s.comment.trim() === ""}
          onClick={s.post}
        >
          Post comment
        </button>
      </div>
      {s.error && (
        <p role="alert" className="task-error">
          {s.error}
        </p>
      )}
    </li>
  );
}
