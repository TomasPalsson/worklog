"use client";

import { formatStamp, localToday } from "@/lib/taskBoard";
import type { TaskRow, TicketDetail } from "@/lib/types";
import { DueChip, Labels, PriorityGlyph, TypeIcon } from "./TaskCardMeta";
import { Row } from "./TaskModalTime";

interface DetailsProps {
  task: TaskRow;
  detail: TicketDetail | null;
  today?: string;
  done: boolean;
}

/** Values come from the live Jira detail once it has loaded, the cached row until then; rows with no value are left out. */
export function TaskDetails({ task, detail, today, done }: DetailsProps) {
  const type = detail?.issue_type ?? task.issue_type;
  const priority = detail?.priority ?? task.priority;
  const updated = detail?.updated ?? task.updated;
  const due = detail?.due_date ?? task.due_date;
  const labels = detail ? detail.labels : task.labels;
  // The cached row only knows whether the ticket is assigned to the Owner, not to whom.
  const assignee = detail ? detail.assignee : task.assigned ? "You" : null;
  return (
    <section className="task-details" aria-labelledby="task-details-label">
      <h3 id="task-details-label" className="task-label">
        Details
      </h3>
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
    </section>
  );
}
