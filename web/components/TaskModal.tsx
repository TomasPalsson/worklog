"use client";

import { useEffect, useId, useRef, useState, type MutableRefObject, type ReactNode, type RefObject } from "react";
import { Clock, Copy, ExternalLink, MessageSquare, X } from "lucide-react";

import type { StatusCategory, TaskRow, TicketComment, TicketDetail } from "@/lib/types";
import { TaskActivity, useActivityTab, type Tab } from "./TaskActivity";
import type { TaskActions } from "./TaskCard";
import { ParentRow, TypeIcon } from "./TaskCardMeta";
import type { Drafts } from "./TaskComposer";
import { TaskDescription } from "./TaskDescription";
import { TaskModalSidebar, type TempoKind } from "./TaskModalSidebar";
import type { Shown } from "./TaskStatusButton";
import { useModalShell } from "./useModalShell";
import { useTicketDetail } from "./useTicketDetail";
import { useWorkLog } from "./useWorkLog";

export interface TaskModalProps {
  task: TaskRow;
  /** Board's today (YYYY-MM-DD) for the due chip; defaults to the local date. */
  today?: string;
  actions: TaskActions;
  onClose: () => void;
  drafts?: MutableRefObject<Drafts>;
  onStatus: (next: { status: string | null; status_category: StatusCategory | null }) => void;
}

interface HeadProps {
  task: TaskRow;
  type: string | null;
  url: string | null;
  onCopied: (message: string) => void;
  onClose: () => void;
}

function Head({ task, type, url, onCopied, onClose }: HeadProps) {
  async function copy() {
    try {
      await navigator.clipboard.writeText(task.key);
      onCopied(`Copied ${task.key}.`);
    } catch {
      /* no clipboard (insecure origin, denied): nothing to announce */
    }
  }
  return (
    <header className="task-modal-head">
      <div className="task-crumb">
        {type && <TypeIcon type={type} />}
        <span className="task-crumb-project">{task.key.split("-")[0]}</span>
        <span className="task-crumb-sep" aria-hidden="true">
          /
        </span>
        <span className="task-key">{task.key}</span>
        <button type="button" className="task-icon-btn" aria-label={`Copy ${task.key}`} data-tip="Copy key" onClick={copy}>
          <Copy size={14} aria-hidden="true" />
        </button>
      </div>
      {url && (
        <a href={url} target="_blank" rel="noreferrer" className="task-modal-jira">
          Open in Jira
          <ExternalLink size={12} aria-hidden="true" />
        </a>
      )}
      <button type="button" className="task-modal-close" aria-label={`Close ${task.key}`} onClick={onClose}>
        <X size={16} aria-hidden="true" />
      </button>
    </header>
  );
}

/** The epic it belongs to, the title, and the two things people open a ticket to do. */
function Title({ id, task, onLog, onComment }: { id: string; task: TaskRow; onLog: () => void; onComment: () => void }) {
  return (
    <>
      {task.parent_summary && <ParentRow text={task.parent_summary} />}
      <h2 id={id} className="task-headline">
        {task.summary}
      </h2>
      <div className="task-quick">
        <button type="button" className="task-btn-secondary" onClick={onLog}>
          <Clock size={13} aria-hidden="true" />
          Log time
        </button>
        <button type="button" className="task-btn-secondary" onClick={onComment}>
          <MessageSquare size={13} aria-hidden="true" />
          Comment
        </button>
      </div>
    </>
  );
}

/** What the dialog's parts share: the status shown, posted comments, the live region and the quick-action signals. */
function useModalState(
  { task, onStatus }: TaskModalProps,
  detail: TicketDetail | null,
  dialog: RefObject<HTMLElement | null>,
  choose: (tab: Tab) => void,
) {
  const [extra, setExtra] = useState<TicketComment[]>([]);
  const [changed, setChanged] = useState<Shown | null>(null);
  const [announce, setAnnounce] = useState("");
  const [logSignal, setLogSignal] = useState(0);
  const [composeSignal, setComposeSignal] = useState(0);
  const [jump, setJump] = useState<{ kind: TempoKind; n: number } | null>(null);
  // Live Jira status once loaded; a change made here wins over both until the dialog closes.
  const shown: Shown = changed ?? (detail ? { status: detail.status, status_category: detail.status_category } : task);
  const report = (s: Shown) => {
    setChanged({ status_category: s.status_category, status: s.status });
    onStatus({ status: s.status, status_category: s.status_category });
  };
  const posted = (body: string) => {
    setExtra((l) => [...l, { id: `local-${l.length}`, author: "You", created: new Date().toISOString(), body }]);
    setAnnounce(`Comment posted to ${task.key}.`);
  };
  // After the Work log tab has rendered, bring the first day in that Tempo state into view.
  useEffect(() => {
    if (jump) dialog.current?.querySelector(`.task-day[data-state="${jump.kind}"]`)?.scrollIntoView?.({ block: "center" });
  }, [jump, dialog]);
  return {
    extra,
    shown,
    report,
    posted,
    announce,
    setAnnounce,
    logSignal,
    composeSignal,
    logTime: () => (choose("work"), setLogSignal((n) => n + 1)),
    compose: () => (choose("comments"), setComposeSignal((n) => n + 1)),
    jumpTo: (kind: TempoKind) => (choose("work"), setJump((j) => ({ kind, n: (j?.n ?? 0) + 1 }))),
  };
}

/** Clicking the dimmed area closes; a drag that merely ends there (selecting text) does not. */
function Backdrop({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  const down = useRef(false);
  return (
    <div
      className="task-modal-backdrop"
      onMouseDown={(e) => void (down.current = e.target === e.currentTarget)}
      onClick={(e) => down.current && e.target === e.currentTarget && onClose()}
    >
      {children}
    </div>
  );
}

/** A ticket as a Jira-style dialog: title, description and activity on the left; status, time and details on the right. */
export function TaskModal(props: TaskModalProps) {
  const { task, actions, onClose, drafts } = props;
  const titleId = useId();
  const dialog = useRef<HTMLDivElement>(null);
  const shell = useModalShell(dialog, onClose);
  const { load, retry } = useTicketDetail(task.key, actions);
  const work = useWorkLog(task.key, actions);
  const { tab, choose } = useActivityTab(task.week_seconds, work);
  const detail = load.s === "ok" ? load.detail : null;
  const m = useModalState(props, detail, dialog, choose);

  return (
    <Backdrop onClose={onClose}>
      <div ref={dialog} tabIndex={-1} className="task-modal" role="dialog" aria-modal="true" aria-labelledby={titleId} onKeyDown={shell.onKeyDown}>
        <Head task={task} type={detail?.issue_type ?? task.issue_type} url={detail?.url ?? task.url} onCopied={m.setAnnounce} onClose={onClose} />
        <div className="task-modal-body">
          <div className="task-modal-main">
            <Title id={titleId} task={task} onLog={m.logTime} onComment={m.compose} />
            <TaskDescription taskKey={task.key} load={load} retry={retry} />
            <TaskActivity
              tab={tab}
              onTab={(t) => choose(t, true)}
              taskKey={task.key}
              actions={actions}
              work={work}
              detail={load}
              extra={m.extra}
              drafts={drafts}
              onAnnounce={m.setAnnounce}
              onPosted={m.posted}
              onMoved={m.report}
              logSignal={m.logSignal}
              composeSignal={m.composeSignal}
            />
          </div>
          <TaskModalSidebar
            task={task}
            actions={actions}
            today={props.today}
            detail={detail}
            shown={m.shown}
            syncedAt={load.s === "ok" ? load.at : null}
            load={work.load}
            onStatus={m.report}
            onTempo={m.jumpTo}
          />
        </div>
        <p className="task-sr" role="status" aria-live="polite">
          {m.announce}
        </p>
      </div>
    </Backdrop>
  );
}
