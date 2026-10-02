"use client";

import { useEffect, useRef, useState, type MutableRefObject, type ReactNode, type RefObject } from "react";
import { ChevronDown, ExternalLink, X } from "lucide-react";

import { formatDuration } from "@/lib/format";
import { formatStamp, localToday, transitionLabel } from "@/lib/taskBoard";
import type { StatusCategory, TaskRow, TicketComment, TicketDetail, TicketStatus, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { DueChip, Labels, ParentRow } from "./TaskCardMeta";
import { TaskComposer, type Drafts } from "./TaskComposer";
import { menuKeys } from "./menuKeys";
import { TaskWorkLog } from "./TaskWorkLog";

type Load = { s: "loading" } | { s: "error"; error: string } | { s: "ok"; detail: TicketDetail };

export interface TaskPanelProps {
  task: TaskRow;
  /** Board's today (YYYY-MM-DD) for the due chip; defaults to the local date. */
  today?: string;
  actions: TaskActions;
  onClose: () => void;
  drafts?: MutableRefObject<Drafts>;
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

type Shown = { status: string | null; status_category: StatusCategory | null };

/** Esc (capture + preventDefault, so the panel stays open) and an outside click close the status menu. */
function useMenuDismiss(open: boolean, wrap: RefObject<HTMLElement | null>, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      close();
      wrap.current?.querySelector<HTMLElement>(".task-status")?.focus();
    };
    const onDown = (e: Event) => {
      if (!wrap.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("mousedown", onDown);
    };
  }, [open, wrap, close]);
}

function StatusChip({ task, shown, actions, onStatus }: Pick<TaskPanelProps, "task" | "actions" | "onStatus"> & { shown: Shown }) {
  const [menu, setMenu] = useState<Transition[] | null>(null);
  const wrap = useRef<HTMLSpanElement>(null);
  const [busy, setBusy] = useState(false);
  const [moving, setMoving] = useState(false);
  useMenuDismiss(menu !== null, wrap, () => setMenu(null));
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
    setMoving(true);
    setError(null);
    const res = await actions.transitionTicket(task.key, t.id);
    if (res.ok) {
      onStatus({ status: res.data.status, status_category: res.data.status_category });
      setMenu(null);
    } else setError(res.error);
    setBusy(false);
    setMoving(false);
  }

  return (
    <span ref={wrap} className="task-status-wrap">
      <button
        type="button"
        className="task-status"
        data-category={shown.status_category ?? undefined}
        data-testid={`status-${task.key}`}
        aria-haspopup="true"
        aria-expanded={menu !== null}
        disabled={busy}
        onClick={toggle}
      >
        {moving ? "Moving…" : (shown.status ?? "No status")}
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {menu && (
        <span className="task-menu" role="menu" onKeyDown={menuKeys}>
          {menu.map((t, i) => (
            <button key={t.id} type="button" role="menuitem" autoFocus={i === 0} disabled={busy} onClick={() => move(t)}>
              {transitionLabel(t)}
            </button>
          ))}
          {menu.length === 0 && <em>Jira offers no status changes for this ticket right now.</em>}
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

function Meta({ task, today, detail, shown, actions, onStatus }: TaskPanelProps & { detail: TicketDetail | null; shown: Shown }) {
  const items = [
    detail?.issue_type,
    detail?.priority && `Priority ${detail.priority}`,
    detail?.assignee && `Assignee ${detail.assignee}`,
  ];
  return (
    <>
      {task.parent_summary && <ParentRow text={task.parent_summary} />}
      <div className="task-meta">
        <StatusChip task={task} shown={shown} actions={actions} onStatus={onStatus} />
        {items.filter(Boolean).map((v) => (
          <span key={v}>{v}</span>
        ))}
        {task.due_date && (
          <span>
            <DueChip due={task.due_date} today={today ?? localToday()} done={shown.status_category === "done"} />
          </span>
        )}
        {task.labels.length > 0 && (
          <span>
            <Labels labels={task.labels} />
          </span>
        )}
      </div>
      <p className="task-hours">
        {[
          `${formatDuration(task.week_seconds)} this week`,
          task.today_seconds > 0 && `${formatDuration(task.today_seconds)} today`,
          detail?.updated && `Updated ${formatStamp(detail.updated)}`,
        ]
          .filter(Boolean)
          .join(" · ")}
      </p>
    </>
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

function Body({ load, retry, extra, taskKey, work }: { load: Load; retry: () => void; extra: TicketComment[]; taskKey: string; work: ReactNode }) {
  if (load.s === "error") {
    return (
      <>
        <div className="task-load-error">
          <p>{`Couldn't load ${taskKey} from Jira: ${load.error.replace(/\.$/, "")}`}</p>
          <button type="button" className="task-btn-secondary" onClick={retry}>
            Try again
          </button>
        </div>
        {work}
      </>
    );
  }
  if (load.s === "loading") {
    return (
      <>
        <section className="task-section"><h3>Description</h3><Skeleton /></section>
        {work}
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
      {work}
      <Comments list={[...comments, ...extra]} />
    </>
  );
}

export function TaskPanel(props: TaskPanelProps) {
  const { task, actions, onClose, onStatus, drafts } = props;
  const { load, retry } = useDetail(task.key, actions);
  const [extra, setExtra] = useState<TicketComment[]>([]);
  const dialog = useRef<HTMLElement>(null);
  const [changed, setChanged] = useState<Shown | null>(null);

  useEffect(() => dialog.current?.focus(), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !e.defaultPrevented && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const detail = load.s === "ok" ? load.detail : null;
  const url = detail?.url ?? task.url;
  const [announce, setAnnounce] = useState("");
  const posted = (body: string) => {
    setExtra((l) => [...l, { id: `local-${l.length}`, author: "You", created: new Date().toISOString(), body }]);
    setAnnounce(`Comment posted to ${task.key}.`);
  };
  useEffect(() => {
    if (extra.length === 0) return;
    dialog.current?.querySelector(".task-comments li:last-child")?.scrollIntoView?.({ block: "nearest" });
  }, [extra]);
  // Live Jira status once loaded; a change made here wins over both until the panel closes.
  const shown: Shown = changed ?? (detail ? { status: detail.status, status_category: detail.status_category } : task);
  const report = (s: Shown) => {
    setChanged({ status: s.status, status_category: s.status_category });
    onStatus({ status: s.status, status_category: s.status_category });
  };
  const moved = (s: TicketStatus) => report(s);

  return (
    <aside ref={dialog} tabIndex={-1} className="task-panel" role="dialog" aria-modal="false" aria-labelledby="task-panel-title">
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
        <h2 id="task-panel-title" className="task-headline">
          {task.summary}
        </h2>
        <Meta {...props} onStatus={report} detail={detail} shown={shown} />
        <Body load={load} retry={retry} extra={extra} taskKey={task.key} work={<TaskWorkLog taskKey={task.key} actions={actions} onAnnounce={setAnnounce} />} />
      </div>
      <p className="task-sr" role="status" aria-live="polite">
        {announce}
      </p>
      <TaskComposer drafts={drafts} taskKey={task.key} actions={actions} onPosted={posted} onMoved={moved} />
    </aside>
  );
}
