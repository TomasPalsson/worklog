"use client";

import { useState } from "react";

import { formatStamp, localToday } from "@/lib/taskBoard";
import type { TaskRow, TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { DueChip, Labels, PriorityGlyph, TypeIcon } from "./TaskCardMeta";
import { Row, TimeCard } from "./TaskModalTime";
import { StatusButton, type Shown } from "./TaskStatusButton";
import type { BlocksLoad } from "./useWorkLog";

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
  const due = detail?.due_date ?? task.due_date;
  const labels = detail ? detail.labels : task.labels;
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
        {detail?.reporter && <Row term="Reporter">{detail.reporter}</Row>}
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
        {due && (
          <Row term="Due">
            <DueChip due={due} today={today ?? localToday()} done={done} />
          </Row>
        )}
        {labels.length > 0 && (
          <Row term="Labels">
            <Labels labels={labels} />
          </Row>
        )}
        {detail && detail.components.length > 0 && <Row term="Components">{detail.components.join(", ")}</Row>}
        {detail && detail.fix_versions.length > 0 && <Row term="Fix versions">{detail.fix_versions.join(", ")}</Row>}
        {detail?.created && <Row term="Created">{formatStamp(detail.created)}</Row>}
        {updated && <Row term="Updated">{formatStamp(updated)}</Row>}
      </dl>
    </details>
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
  /** Reload the work log once the Time card has pulled fresh Tempo numbers. */
  onPulled?: () => void | Promise<void>;
  /** Try the work log again after it failed to load. */
  onRetry?: () => void;
  /** Dialog live region. */
  onAnnounce?: (message: string) => void;
  /** At the widths with a sidebar; below that the status and time sit under the title instead. */
  wide: boolean;
}

export function TaskModalSidebar({ task, actions, today, detail, shown, syncedAt, load, onStatus, onTempo, onPulled, onRetry, onAnnounce, wide }: SidebarProps) {
  const time = <TimeCard task={task} detail={detail} load={load} onTempo={onTempo} onPulled={onPulled} onRetry={onRetry} onAnnounce={onAnnounce} />;
  return (
    <aside className="task-modal-side" aria-label={`Details for ${task.key}`}>
      {wide && (
        <>
          <div className="task-side-status">
            <StatusButton taskKey={task.key} shown={shown} actions={actions} onStatus={onStatus} />
          </div>
          {time}
        </>
      )}
      <Details task={task} detail={detail} today={today} done={shown.status_category === "done"} wide={wide} />
      {!wide && time}
      {syncedAt && <p className="task-side-foot">{`Jira synced at ${clock(syncedAt)}`}</p>}
    </aside>
  );
}
