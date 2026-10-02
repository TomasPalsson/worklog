"use client";

import { useEffect, useState, type ReactNode } from "react";

import { formatDuration } from "@/lib/format";
import { formatStamp, localToday } from "@/lib/taskBoard";
import type { TaskRow, TicketDay, TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { DueChip, Labels, PriorityGlyph, TypeIcon } from "./TaskCardMeta";
import { chipOf } from "./TaskDayGroup";
import { StatusButton, type Shown } from "./TaskStatusButton";
import { workSeconds, type BlocksLoad } from "./useWorkLog";

export type TempoKind = "none" | "changed";

const None = () => <span className="task-none">None</span>;

function Row({ term, children }: { term: string; children: ReactNode }) {
  return (
    <div className="task-dl-row">
      <dt>{term}</dt>
      <dd>{children}</dd>
    </div>
  );
}

interface DetailsProps {
  task: TaskRow;
  detail: TicketDetail | null;
  today?: string;
  done: boolean;
}

/** Values come from the live Jira detail once it has loaded, the cached row until then. */
function Details({ task, detail, today, done }: DetailsProps) {
  const type = detail?.issue_type ?? task.issue_type;
  const priority = detail?.priority ?? task.priority;
  const updated = detail?.updated ?? task.updated;
  // The cached row only knows whether the ticket is assigned to the Owner, not to whom.
  const assignee = detail ? detail.assignee : task.assigned ? "You" : null;
  // Always open beside the main column; on small screens a toggle, closed to start with.
  const [wide, setWide] = useState(() => typeof window.matchMedia !== "function" || window.matchMedia("(min-width: 760px)").matches);
  const [folded, setFolded] = useState(false);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const q = window.matchMedia("(min-width: 760px)");
    const on = () => setWide(q.matches);
    q.addEventListener?.("change", on);
    return () => q.removeEventListener?.("change", on);
  }, []);
  return (
    <details className="task-side-card task-details" open={wide || folded} onToggle={(e) => !wide && setFolded(e.currentTarget.open)}>
      <summary className="task-label" tabIndex={wide ? -1 : undefined}>
        Details
      </summary>
      <dl>
        <Row term="Assignee">{assignee ?? <None />}</Row>
        <Row term="Priority">
          {priority ? (
            <>
              <PriorityGlyph priority={priority} />
              {priority}
            </>
          ) : (
            <None />
          )}
        </Row>
        <Row term="Type">
          {type ? (
            <>
              <TypeIcon type={type} />
              <span aria-hidden="true">{type}</span>
            </>
          ) : (
            <None />
          )}
        </Row>
        <Row term="Due">{task.due_date ? <DueChip due={task.due_date} today={today ?? localToday()} done={done} /> : <None />}</Row>
        <Row term="Labels">{task.labels.length > 0 ? <Labels labels={task.labels} /> : <None />}</Row>
        <Row term="Updated">{updated ? formatStamp(updated) : <None />}</Row>
      </dl>
    </details>
  );
}

function tempoCounts(days: TicketDay[]) {
  const states = days.map((d) => chipOf(d.blocks));
  return {
    none: states.filter((s) => s === "Not in Tempo").length,
    changed: states.filter((s) => s === "Changed since sent").length,
  };
}

const days = (n: number) => `${n} ${n === 1 ? "day" : "days"}`;

/** Which days still need Tempo, as buttons that jump to the first such day; "All sent" when none do. */
function TempoRow({ load, onTempo }: { load: BlocksLoad; onTempo: (kind: TempoKind) => void }) {
  if (load.s !== "ok" || load.data.days.length === 0) return <span className="task-none">—</span>;
  const { none, changed } = tempoCounts(load.data.days);
  if (none === 0 && changed === 0) return <span className="task-tempo" data-tone="ok">All sent</span>;
  return (
    <>
      {none > 0 && (
        <button type="button" className="task-tempo" data-tone="none" onClick={() => onTempo("none")}>
          {`${days(none)} not sent`}
        </button>
      )}
      {changed > 0 && (
        <button type="button" className="task-tempo" data-tone="changed" onClick={() => onTempo("changed")}>
          {`${changed} changed since sent`}
        </button>
      )}
    </>
  );
}

function TimeCard({ task, load, onTempo }: { task: TaskRow; load: BlocksLoad; onTempo: (kind: TempoKind) => void }) {
  const fortnight = workSeconds(load);
  return (
    <section className="task-side-card task-time" aria-labelledby="task-time-label">
      <h3 id="task-time-label" className="task-label">
        Time
      </h3>
      <dl>
        <Row term="This week">{formatDuration(task.week_seconds)}</Row>
        <Row term="Today">{task.today_seconds > 0 ? formatDuration(task.today_seconds) : "—"}</Row>
        <Row term="Last 14 days">{fortnight === null ? "—" : formatDuration(fortnight)}</Row>
        <Row term="Tempo">
          <TempoRow load={load} onTempo={onTempo} />
        </Row>
      </dl>
    </section>
  );
}

const clock = (d: Date) => d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

export interface SidebarProps {
  task: TaskRow;
  actions: TaskActions;
  today?: string;
  detail: TicketDetail | null;
  shown: Shown;
  /** When Jira last answered for this ticket; unknown until the detail has loaded. */
  syncedAt: Date | null;
  load: BlocksLoad;
  onStatus: (next: Shown) => void;
  onTempo: (kind: TempoKind) => void;
}

export function TaskModalSidebar({ task, actions, today, detail, shown, syncedAt, load, onStatus, onTempo }: SidebarProps) {
  return (
    <aside className="task-modal-side" aria-label={`Details for ${task.key}`}>
      <div className="task-side-status">
        <StatusButton taskKey={task.key} shown={shown} actions={actions} onStatus={onStatus} />
      </div>
      <TimeCard task={task} load={load} onTempo={onTempo} />
      <Details task={task} detail={detail} today={today} done={shown.status_category === "done"} />
      {syncedAt && <p className="task-side-foot">{`Jira synced at ${clock(syncedAt)}`}</p>}
    </aside>
  );
}
