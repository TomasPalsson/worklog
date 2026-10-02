"use client";

import { useRef, useState, type KeyboardEvent, type MutableRefObject } from "react";

import { formatDuration } from "@/lib/format";
import { formatStamp, initials, relativeWords } from "@/lib/taskBoard";
import type { TicketComment, TicketStatus } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskComposer, type Drafts } from "./TaskComposer";
import { Skeleton } from "./TaskSkeleton";
import { TaskWorkLog } from "./TaskWorkLog";
import type { DetailLoad } from "./useTicketDetail";
import { workSeconds, type WorkLog } from "./useWorkLog";

export type Tab = "work" | "comments";
const TABS: Tab[] = ["work", "comments"];
const STORE = "worklog.ticketTab";

// Per-viewer convenience only; a blocked or full store just means no memory.
function remembered(): Tab | null {
  try {
    const v = window.localStorage.getItem(STORE);
    return v === "work" || v === "comments" ? v : null;
  } catch {
    return null;
  }
}

/**
 * Which Activity tab shows. An explicit pick wins, then the remembered one, then the default: Work log when the
 * ticket has hours this week or any block in the window, else Comments. `choose(tab, true)` also remembers it.
 */
export function useActivityTab(weekSeconds: number, work: WorkLog) {
  const [picked, setPicked] = useState<Tab | null>(null);
  const [stored] = useState(remembered);
  const [locked, setLocked] = useState<Tab | null>(null);
  const hasBlocks = work.load.s === "ok" && work.load.data.days.length > 0;
  const fallback: Tab = weekSeconds > 0 || hasBlocks ? "work" : "comments";
  const tab = picked ?? stored ?? locked ?? fallback;
  // Once focus is inside Activity the default stops following the Work log load, which would swap tabs under a typist.
  const engage = () => setLocked((l) => l ?? fallback);
  const choose = (next: Tab, remember = false) => {
    setPicked(next);
    if (!remember) return;
    try {
      window.localStorage.setItem(STORE, next);
    } catch {
      /* no memory, no harm */
    }
  };
  return { tab, choose, engage };
}

function Tabs({ tab, onTab, counts }: { tab: Tab; onTab: (t: Tab) => void; counts: Record<Tab, string | null> }) {
  const refs = useRef<Record<Tab, HTMLButtonElement | null>>({ work: null, comments: null });
  function onKeyDown(e: KeyboardEvent) {
    const at = TABS.indexOf(tab);
    const to = { ArrowRight: (at + 1) % 2, ArrowLeft: (at + 1) % 2, Home: 0, End: 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    onTab(TABS[to]);
    refs.current[TABS[to]]?.focus();
  }
  const label: Record<Tab, string> = { work: "Work log", comments: "Comments" };
  return (
    <div role="tablist" aria-label="Activity" className="task-tabs" onKeyDown={onKeyDown}>
      {TABS.map((t) => (
        <button
          key={t}
          ref={(el) => void (refs.current[t] = el)}
          type="button"
          role="tab"
          id={`task-tab-${t}`}
          aria-selected={tab === t}
          aria-controls={`task-panel-${t}`}
          tabIndex={tab === t ? 0 : -1}
          onClick={() => onTab(t)}
        >
          {label[t]}
          {counts[t] && <span className="task-tab-count">{counts[t]}</span>}
        </button>
      ))}
    </div>
  );
}

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

export interface ActivityProps {
  tab: Tab;
  onTab: (t: Tab) => void;
  taskKey: string;
  actions: TaskActions;
  work: WorkLog;
  detail: DetailLoad;
  extra: TicketComment[];
  drafts?: MutableRefObject<Drafts>;
  onAnnounce: (message: string) => void;
  onPosted: (text: string) => void;
  onMoved: (next: TicketStatus) => void;
  /** Bumped by the quick actions. */
  logSignal: number;
  composeSignal: number;
  /** Called once a signal has been acted on, so a later remount of the tab does not act on it again. */
  onEngage?: () => void;
  onLogHandled?: () => void;
  onComposeHandled?: () => void;
}

/** Work log and Comments behind one tablist, so only one of them is on screen at a time. */
export function TaskActivity(p: ActivityProps) {
  const seconds = workSeconds(p.work.load);
  const comments = p.detail.s === "ok" ? p.detail.detail.comments.length + p.extra.length : null;
  const counts: Record<Tab, string | null> = {
    work: seconds ? formatDuration(seconds) : null,
    comments: comments === null ? null : String(comments),
  };
  return (
    <section className="task-activity" aria-labelledby="task-activity-label" onFocusCapture={p.onEngage}>
      <h3 id="task-activity-label" className="task-label">
        Activity
      </h3>
      <Tabs tab={p.tab} onTab={p.onTab} counts={counts} />
      <div role="tabpanel" id={`task-panel-${p.tab}`} aria-labelledby={`task-tab-${p.tab}`} className="task-tabpanel">
        {p.tab === "work" ? (
          <TaskWorkLog taskKey={p.taskKey} actions={p.actions} work={p.work} onAnnounce={p.onAnnounce} logSignal={p.logSignal} onHandled={p.onLogHandled} />
        ) : (
          <>
            <TaskComposer
              drafts={p.drafts}
              taskKey={p.taskKey}
              actions={p.actions}
              onPosted={p.onPosted}
              onMoved={p.onMoved}
              focusSignal={p.composeSignal}
              onHandled={p.onComposeHandled}
            />
            <CommentList detail={p.detail} extra={p.extra} />
          </>
        )}
      </div>
    </section>
  );
}
