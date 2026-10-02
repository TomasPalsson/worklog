"use client";

import { useState, type ReactNode } from "react";

import { formatDuration } from "@/lib/format";
import { formatStamp, localToday } from "@/lib/taskBoard";
import type { TaskRow, TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { DueChip, Labels, PriorityGlyph, TypeIcon } from "./TaskCardMeta";
import { tempoState } from "./tempoState";
import { StatusButton, type Shown } from "./TaskStatusButton";
import { workSeconds, type BlocksLoad } from "./useWorkLog";

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
  wide: boolean;
}

/** Values come from the live Jira detail once it has loaded, the cached row until then; rows with no value are left out. */
function Details({ task, detail, today, done, wide }: DetailsProps) {
  const type = detail?.issue_type ?? task.issue_type;
  const priority = detail?.priority ?? task.priority;
  const updated = detail?.updated ?? task.updated;
  // The cached row only knows whether the ticket is assigned to the Owner, not to whom.
  const assignee = detail ? detail.assignee : task.assigned ? "You" : null;
  // Always open beside the main column; on small screens a toggle, closed to start with.
  const [folded, setFolded] = useState(false);
  return (
    <details className="task-side-card task-details" open={wide || folded} onToggle={(e) => !wide && setFolded(e.currentTarget.open)}>
      <summary className="task-label" tabIndex={wide ? -1 : undefined}>
        Details
      </summary>
      <dl>
        {assignee && <Row term="Assignee">{assignee}</Row>}
        {priority && (
          <Row term="Priority">
            <PriorityGlyph priority={priority} />
            {priority}
          </Row>
        )}
        {type && (
          <Row term="Type">
            <TypeIcon type={type} />
            <span aria-hidden="true">{type}</span>
          </Row>
        )}
        {task.due_date && (
          <Row term="Due">
            <DueChip due={task.due_date} today={today ?? localToday()} done={done} />
          </Row>
        )}
        {task.labels.length > 0 && (
          <Row term="Labels">
            <Labels labels={task.labels} />
          </Row>
        )}
        {updated && <Row term="Updated">{formatStamp(updated)}</Row>}
      </dl>
    </details>
  );
}

/** Said once: the state in words, and for days still to send a Review button that jumps to the first of them. */
function TempoRow({ load, onTempo }: { load: BlocksLoad; onTempo: () => void }) {
  const tempo = tempoState(load);
  if (!tempo) return <span className="task-none">—</span>;
  return (
    <>
      <span className="task-tempo" data-tone={tempo.tone}>
        {tempo.text}
      </span>
      {tempo.review && (
        <>
          <span aria-hidden="true">{" · "}</span>
          <button type="button" className="task-review" onClick={onTempo}>
            Review
          </button>
        </>
      )}
    </>
  );
}

function TimeCard({ task, load, onTempo }: { task: TaskRow; load: BlocksLoad; onTempo: () => void }) {
  const fortnight = workSeconds(load);
  return (
    <section className="task-side-card task-time" aria-labelledby="task-time-label">
      <h3 id="task-time-label" className="task-label">
        Time
      </h3>
      <dl>
        <Row term="This week">{formatDuration(task.week_seconds)}</Row>
        {task.today_seconds > 0 && <Row term="Today">{formatDuration(task.today_seconds)}</Row>}
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
  onTempo: () => void;
  /** At the widths with a sidebar; below that the status and time sit under the title instead. */
  wide: boolean;
}

export function TaskModalSidebar({ task, actions, today, detail, shown, syncedAt, load, onStatus, onTempo, wide }: SidebarProps) {
  return (
    <aside className="task-modal-side" aria-label={`Details for ${task.key}`}>
      {wide && (
        <>
          <div className="task-side-status">
            <StatusButton taskKey={task.key} shown={shown} actions={actions} onStatus={onStatus} />
          </div>
          <TimeCard task={task} load={load} onTempo={onTempo} />
        </>
      )}
      <Details task={task} detail={detail} today={today} done={shown.status_category === "done"} wide={wide} />
      {syncedAt && <p className="task-side-foot">{`Jira synced at ${clock(syncedAt)}`}</p>}
    </aside>
  );
}
