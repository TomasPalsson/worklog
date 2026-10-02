"use client";

import { useEffect, useRef, useState } from "react";
import { ChevronDown, ExternalLink, X } from "lucide-react";

import { formatDuration } from "@/lib/format";
import { formatStamp } from "@/lib/taskBoard";
import type { StatusCategory, TaskRow, TicketComment, TicketDetail, TicketStatus, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskComposer } from "./TaskComposer";

type Load = { s: "loading" } | { s: "error"; error: string } | { s: "ok"; detail: TicketDetail };

export interface TaskPanelProps {
  task: TaskRow;
  actions: TaskActions;
  onClose: () => void;
  onStatus: (next: { status: string | null; status_category: StatusCategory | null }) => void;
}

function useDetail(key: string, actions: TaskActions) {
  const [load, setLoad] = useState<Load>({ s: "loading" });
  const [attempt, setAttempt] = useState(0);
  const loadDetail = actions.loadTicketDetail;
  useEffect(() => {
    let live = true;
    setLoad({ s: "loading" });
    loadDetail(key).then((res) => {
      if (live) setLoad(res.ok ? { s: "ok", detail: res.data } : { s: "error", error: res.error });
    });
    return () => {
      live = false;
    };
  }, [key, attempt, loadDetail]);
  return { load, retry: () => setAttempt((n) => n + 1) };
}

function Skeleton() {
  return (
    <div className="task-skel" aria-hidden="true">
      <i style={{ width: "92%" }} />
      <i style={{ width: "80%" }} />
      <i style={{ width: "64%" }} />
    </div>
  );
}

function StatusChip({ task, actions, onStatus }: Pick<TaskPanelProps, "task" | "actions" | "onStatus">) {
  const [menu, setMenu] = useState<Transition[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function toggle() {
    if (menu) return setMenu(null);
    setBusy(true);
    setError(null);
    const res = await actions.loadTransitions(task.key);
    if (res.ok) setMenu(res.data);
    else setError(res.error);
    setBusy(false);
  }

  async function move(t: Transition) {
    setBusy(true);
    setError(null);
    const res = await actions.transitionTicket(task.key, t.id);
    if (res.ok) {
      onStatus({ status: res.data.status, status_category: res.data.status_category });
      setMenu(null);
    } else setError(res.error);
    setBusy(false);
  }

  return (
    <span className="task-status-wrap">
      <button
        type="button"
        className="task-status"
        data-category={task.status_category ?? undefined}
        data-testid={`status-${task.key}`}
        aria-haspopup="true"
        aria-expanded={menu !== null}
        disabled={busy}
        onClick={toggle}
      >
        {task.status ?? "No status"}
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {menu && (
        <span className="task-menu">
          {menu.map((t) => (
            <button key={t.id} type="button" disabled={busy} onClick={() => move(t)}>
              {`${t.name} → ${t.to_status}`}
            </button>
          ))}
          {menu.length === 0 && <em>No moves available.</em>}
        </span>
      )}
      {error && (
        <span role="alert" className="task-error">
          {error}
        </span>
      )}
    </span>
  );
}

function Meta({ task, detail, actions, onStatus }: TaskPanelProps & { detail: TicketDetail | null }) {
  const items = [detail?.issue_type, detail?.priority, detail?.assignee];
  return (
    <div className="task-meta">
      <StatusChip task={task} actions={actions} onStatus={onStatus} />
      {items.filter(Boolean).map((v) => (
        <span key={v}>{v}</span>
      ))}
      {detail?.updated && <span>{`Updated ${formatStamp(detail.updated)}`}</span>}
      <span>{`${formatDuration(task.week_seconds)} this week · ${formatDuration(task.today_seconds)} today`}</span>
    </div>
  );
}

function Comments({ list }: { list: TicketComment[] }) {
  return (
    <section className="task-section">
      <h3>{`Comments · ${list.length}`}</h3>
      {list.length === 0 && <p className="task-empty">No comments yet.</p>}
      <ul className="task-comments">
        {list.map((c) => (
          <li key={c.id}>
            <div>
              <strong>{c.author}</strong>
              <time>{formatStamp(c.created)}</time>
            </div>
            <p>{c.body}</p>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Body({ load, retry, extra, taskKey }: { load: Load; retry: () => void; extra: TicketComment[]; taskKey: string }) {
  if (load.s === "error") {
    return (
      <div className="task-load-error">
        <p>{`Couldn't load ${taskKey} from Jira — ${load.error}.`}</p>
        <button type="button" className="task-btn-secondary" onClick={retry}>
          Try again
        </button>
      </div>
    );
  }
  if (load.s === "loading") {
    return (
      <>
        <section className="task-section"><h3>Description</h3><Skeleton /></section>
        <section className="task-section"><h3>Comments</h3><Skeleton /></section>
      </>
    );
  }
  const { description, comments } = load.detail;
  return (
    <>
      <section className="task-section">
        <h3>Description</h3>
        {description ? <p className="task-prose">{description}</p> : <p className="task-empty">No description in Jira.</p>}
      </section>
      <Comments list={[...comments, ...extra]} />
    </>
  );
}

export function TaskPanel(props: TaskPanelProps) {
  const { task, actions, onClose, onStatus } = props;
  const { load, retry } = useDetail(task.key, actions);
  const [extra, setExtra] = useState<TicketComment[]>([]);
  const head = useRef<HTMLHeadingElement>(null);

  useEffect(() => head.current?.focus(), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const detail = load.s === "ok" ? load.detail : null;
  const url = detail?.url ?? task.url;
  const posted = (body: string) =>
    setExtra((l) => [...l, { id: `local-${l.length}`, author: "You", created: new Date().toISOString(), body }]);
  const moved = (s: TicketStatus) => onStatus({ status: s.status, status_category: s.status_category });

  return (
    <aside className="task-panel" role="dialog" aria-modal="false" aria-labelledby="task-panel-title">
      <div className="task-panel-top">
        <span className="task-key">{task.key}</span>
        {url && (
          <a href={url} target="_blank" rel="noreferrer">
            Open in Jira
            <ExternalLink size={12} aria-hidden="true" />
          </a>
        )}
        <button type="button" className="task-panel-close" aria-label={`Close ${task.key}`} onClick={onClose}>
          <X size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="task-panel-scroll">
        <h2 id="task-panel-title" ref={head} tabIndex={-1} className="task-headline">
          {task.summary}
        </h2>
        <Meta {...props} detail={detail} />
        <Body load={load} retry={retry} extra={extra} taskKey={task.key} />
      </div>
      <TaskComposer taskKey={task.key} actions={actions} onPosted={posted} onMoved={moved} />
    </aside>
  );
}
