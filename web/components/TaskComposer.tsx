"use client";

import { useEffect, useState, type MutableRefObject } from "react";
import { Sparkles, X } from "lucide-react";

import type { TicketStatus, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";

const MAX = 5000;

/** Unsent comments by ticket key, owned by the board so they survive closing or switching cards. */
export type Drafts = Record<string, { text: string; suggested: Transition | null }>;

interface Props {
  taskKey: string;
  drafts?: MutableRefObject<Drafts>;
  actions: TaskActions;
  onPosted: (text: string) => void;
  onMoved: (next: TicketStatus) => void;
}

function useComposer({ taskKey, actions, onPosted, onMoved, drafts }: Props) {
  const saved = drafts?.current[taskKey];
  const [text, setText] = useState(saved?.text ?? "");
  const [suggested, setSuggested] = useState<Transition | null>(saved?.suggested ?? null);
  useEffect(() => {
    if (drafts) drafts.current[taskKey] = { text, suggested };
  }, [drafts, taskKey, text, suggested]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const trimmed = text.trim();

  async function draft() {
    setBusy(true);
    setError(null);
    const res = await actions.draftTicketUpdate(taskKey);
    if (res.ok) {
      setText(res.data.comment);
      setSuggested(res.data.transitions.find((t) => t.id === res.data.suggested_transition_id) ?? null);
    } else setError(res.error);
    setBusy(false);
  }

  /** Runs the kept suggestion first; a rejection stops before any comment. */
  async function applySuggestion(): Promise<boolean> {
    if (!suggested) return true;
    const moved = await actions.transitionTicket(taskKey, suggested.id);
    if (!moved.ok) {
      setError(moved.error);
      return false;
    }
    onMoved(moved.data);
    setSuggested(null);
    return true;
  }

  async function post() {
    setBusy(true);
    setError(null);
    if (await applySuggestion()) {
      const res = await actions.commentOnTicket(taskKey, trimmed);
      if (res.ok) {
        onPosted(trimmed);
        setText("");
      } else setError(res.error);
    }
    setBusy(false);
  }

  return { text, setText, suggested, dropSuggestion: () => setSuggested(null), busy, error, trimmed, draft, post };
}

function Count({ n }: { n: number }) {
  const level = n > MAX ? "over" : n > 4500 ? "near" : undefined;
  return (
    <span className="task-count" data-level={level}>
      {`${n} / ${MAX}`}
    </span>
  );
}

export function TaskComposer(props: Props) {
  const c = useComposer(props);
  return (
    <form className="task-composer" onSubmit={(e) => e.preventDefault()}>
      <label htmlFor="task-composer-text" className="task-label">
        Add a comment
      </label>
      <textarea id="task-composer-text" rows={4} value={c.text} onChange={(e) => c.setText(e.target.value)} />
      <div className="task-composer-meta">
        <Count n={c.text.length} />
        {c.suggested && (
          <span className="task-suggest">
            {`Also moves to ${c.suggested.to_status}`}
            <button type="button" aria-label="Don't change the status" disabled={c.busy} onClick={c.dropSuggestion}>
              <X size={12} aria-hidden="true" />
            </button>
          </span>
        )}
      </div>
      <div className="task-composer-actions">
        <button type="button" className="task-btn-secondary" disabled={c.busy} onClick={c.draft}>
          <Sparkles size={13} aria-hidden="true" />
          Draft with AI
        </button>
        <button
          type="button"
          className="task-btn-primary"
          disabled={c.busy || c.trimmed === "" || c.text.length > MAX}
          onClick={c.post}
        >
          {c.suggested ? `Post and move to ${c.suggested.to_status}` : "Post comment"}
        </button>
      </div>
      {c.error && (
        <p role="alert" className="task-error">
          {c.error}
        </p>
      )}
    </form>
  );
}
