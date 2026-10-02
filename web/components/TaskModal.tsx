"use client";

import { useEffect, useId, useRef, useState, type ReactNode, type RefObject } from "react";
import { ExternalLink, X } from "lucide-react";

import { localToday } from "@/lib/taskBoard";
import type { JiraTicket, StatusCategory, TaskRow, TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { ParentRow, TypeIcon } from "./TaskCardMeta";
import { TaskDescription } from "./TaskDescription";
import { CopyKey } from "./TaskCopyKey";
import { TaskHours } from "./TaskHours";
import { TaskModalSidebar } from "./TaskModalSidebar";
import { TaskModalSummary } from "./TaskModalSummary";
import type { Shown } from "./TaskStatusButton";
import { useModalShell } from "./useModalShell";
import { useWide } from "./useWide";
import { useTicketDetail } from "./useTicketDetail";
import { TaskWorkLog } from "./TaskWorkLog";
import { useWorkLog, type WorkLog } from "./useWorkLog";

export interface TaskModalProps {
  task: TaskRow;
  /** Board's today (YYYY-MM-DD) for the due chip; defaults to the local date. */
  today?: string;
  actions: TaskActions;
  onClose: () => void;
  onStatus: (next: { status: string | null; status_category: StatusCategory | null }) => void;
  /** The board's tickets, offered first in the work log's Move picker. */
  tickets?: JiraTicket[];
}

interface HeadProps {
  task: TaskRow;
  type: string | null;
  url: string | null;
  onCopied: (message: string) => void;
  onClose: () => void;
}

function Head({ task, type, url, onCopied, onClose }: HeadProps) {
  const wide = useWide();
  return (
    <header className="task-modal-head">
      <div className="task-crumb">
        {type && <TypeIcon type={type} />}
        <span className="task-crumb-project">{task.key.split("-")[0]}</span>
        <span className="task-crumb-sep" aria-hidden="true">
          /
        </span>
        <span className="task-key">{task.key}</span>
        <CopyKey taskKey={task.key} onCopied={onCopied} />
      </div>
      {url && (
        wide ? (
          <a href={url} target="_blank" rel="noreferrer" className="task-modal-jira">
            Open in Jira
            <ExternalLink size={12} aria-hidden="true" />
          </a>
        ) : (
          <a
            href={url}
            target="_blank"
            rel="noreferrer"
            className="task-modal-jira task-modal-jira-icon"
            aria-label={`Open ${task.key} in Jira`}
            data-tip="Open in Jira"
          >
            <ExternalLink size={16} aria-hidden="true" />
          </a>
        )
      )}
      <button type="button" className="task-modal-close" aria-label={`Close ${task.key}`} onClick={onClose}>
        <X size={16} aria-hidden="true" />
      </button>
    </header>
  );
}

/** The epic it belongs to, then the title. */
function Title({ id, task }: { id: string; task: TaskRow }) {
  return (
    <>
      {task.parent_summary && <ParentRow text={task.parent_summary} />}
      <h2 id={id} className="task-headline">
        {task.summary}
      </h2>
    </>
  );
}

/** What the dialog's parts share: the status shown, the live region and the Tempo jump. */
function useModalState(
  { task, onStatus }: TaskModalProps,
  detail: TicketDetail | null,
  dialog: RefObject<HTMLElement | null>,
  work: WorkLog,
) {
  const [changed, setChanged] = useState<Shown | null>(null);
  const [announce, setAnnounce] = useState("");
  const [jump, setJump] = useState(0);
  // Live Jira status once loaded; a change made here wins over both until the dialog closes.
  const shown: Shown = changed ?? (detail ? { status: detail.status, status_category: detail.status_category } : task);
  const report = (s: Shown) => {
    setChanged({ status_category: s.status_category, status: s.status });
    onStatus({ status: s.status, status_category: s.status_category });
  };
  // After every day has rendered, bring the first day that still needs Tempo into view.
  useEffect(() => {
    if (jump) dialog.current?.querySelector('.task-day[data-state="none"], .task-day[data-state="changed"]')?.scrollIntoView?.({ block: "center" });
  }, [jump, dialog]);
  return {
    shown,
    report,
    announce,
    setAnnounce,
    jumpTo: () => (work.setShowAll(true), setJump((n) => n + 1)),
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

/** A ticket as a Jira-style dialog: title, time, work log and description on the left; status, time and details on the right. */
export function TaskModal(props: TaskModalProps) {
  const { task, actions, onClose } = props;
  const titleId = useId();
  const dialog = useRef<HTMLDivElement>(null);
  const shell = useModalShell(dialog, onClose);
  const { load, retry } = useTicketDetail(task.key, actions);
  const work = useWorkLog(task.key, actions);
  const detail = load.s === "ok" ? load.detail : null;
  const m = useModalState(props, detail, dialog, work);
  const wide = useWide();

  return (
    <Backdrop onClose={onClose}>
      <div ref={dialog} tabIndex={-1} className="task-modal" role="dialog" aria-modal="true" aria-labelledby={titleId} onKeyDown={shell.onKeyDown}>
        <Head task={task} type={detail?.issue_type ?? task.issue_type} url={detail?.url ?? task.url} onCopied={m.setAnnounce} onClose={onClose} />
        <div className="task-modal-body">
          <div className="task-modal-main">
            <Title id={titleId} task={task} />
            {!wide && <TaskModalSummary task={task} actions={actions} shown={m.shown} load={work.load} onStatus={m.report} onTempo={m.jumpTo} />}
            <TaskHours taskKey={task.key} load={work.load} today={props.today ?? localToday()} onTempo={m.jumpTo} onRetry={work.retry} />
            <h3 className="task-label task-worklog-title">Work log</h3>
            <TaskWorkLog taskKey={task.key} actions={actions} work={work} onAnnounce={m.setAnnounce} tickets={props.tickets} />
            <details className="task-desc-fold">
              <summary>Description</summary>
              <TaskDescription taskKey={task.key} load={load} retry={retry} />
            </details>
          </div>
          <TaskModalSidebar
            task={task}
            actions={actions}
            detail={detail}
            shown={m.shown}
            syncedAt={load.s === "ok" ? load.at : null}
            load={work.load}
            onStatus={m.report}
            onTempo={m.jumpTo}
            onPulled={work.refetch}
            onRetry={work.retry}
            onAnnounce={m.setAnnounce}
            wide={wide}
          />
        </div>
        <p className="task-sr" role="status" aria-live="polite">
          {m.announce}
        </p>
      </div>
    </Backdrop>
  );
}
