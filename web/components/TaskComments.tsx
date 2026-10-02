"use client";

import type { MutableRefObject } from "react";

import { formatStamp, initials, relativeWords } from "@/lib/taskBoard";
import type { TicketComment, TicketStatus } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskComposer, type Drafts } from "./TaskComposer";
import { Skeleton } from "./TaskSkeleton";
import type { DetailLoad } from "./useTicketDetail";

function Comment({ c, now }: { c: TicketComment; now: Date }) {
  return (
    <li>
      <span className="task-avatar" aria-hidden="true">
        {initials(c.author)}
      </span>
      <div>
        <div className="task-comment-head">
          <strong>{c.author}</strong>
          <time title={formatStamp(c.created)} dateTime={c.created}>
            {relativeWords(c.created, now) || formatStamp(c.created)}
          </time>
        </div>
        <p>{c.body}</p>
      </div>
    </li>
  );
}

function CommentList({ detail, extra }: { detail: DetailLoad; extra: TicketComment[] }) {
  if (detail.s === "loading") return <Skeleton />;
  if (detail.s === "error") return null;
  const list = [...detail.detail.comments, ...extra].reverse();
  const now = new Date();
  return list.length === 0 ? (
    <p className="task-empty">No comments yet.</p>
  ) : (
    <ul className="task-comments">
      {list.map((c) => (
        <Comment key={c.id} c={c} now={now} />
      ))}
    </ul>
  );
}

export interface CommentsProps {
  taskKey: string;
  actions: TaskActions;
  detail: DetailLoad;
  /** Comments posted from this dialog, shown before Jira's list has refreshed. */
  extra: TicketComment[];
  drafts?: MutableRefObject<Drafts>;
  onPosted: (text: string) => void;
  onMoved: (next: TicketStatus) => void;
}

/** The composer (with AI draft) above the thread, newest comment first. */
export function TaskComments({ taskKey, actions, detail, extra, drafts, onPosted, onMoved }: CommentsProps) {
  return (
    <section className="task-comments-section" aria-labelledby="task-comments-label">
      <h3 id="task-comments-label" className="task-label">
        Comments
      </h3>
      <TaskComposer drafts={drafts} taskKey={taskKey} actions={actions} onPosted={onPosted} onMoved={onMoved} />
      <CommentList detail={detail} extra={extra} />
    </section>
  );
}
