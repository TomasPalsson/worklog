"use client";

import type { TaskRow, TicketDetail } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TimeCard } from "./TaskModalTime";
import { StatusButton, type Shown } from "./TaskStatusButton";
import type { BlocksLoad } from "./useWorkLog";

const clock = (d: Date) => d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

export interface SidebarProps {
  task: TaskRow;
  actions: TaskActions;
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

export function TaskModalSidebar({ task, actions, detail, shown, syncedAt, load, onStatus, onTempo, onPulled, onRetry, onAnnounce, wide }: SidebarProps) {
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
      {!wide && time}
      {syncedAt && <p className="task-side-foot">{`Jira synced at ${clock(syncedAt)}`}</p>}
    </aside>
  );
}
