"use client";

import { useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { TicketBlocks } from "@/lib/types";
import { DayGroup } from "./TaskDayGroup";
import { TaskLogTime } from "./TaskLogTime";
import { Skeleton } from "./TaskSkeleton";
import type { TaskActions } from "./TaskCard";
import type { WorkLog } from "./useWorkLog";

function LogButton({ onClick, btn }: { onClick: () => void; btn?: React.Ref<HTMLButtonElement> }) {
  return (
    <button ref={btn} type="button" className="task-btn-secondary task-log-open" onClick={onClick}>
      <Plus size={12} aria-hidden="true" />
      Log time
    </button>
  );
}

const summary = ({ days }: TicketBlocks) =>
  `${formatDuration(days.reduce((sum, d) => sum + d.line_seconds, 0))} over ${days.length} ${days.length === 1 ? "day" : "days"} · last 14 days`;

type Logged = { id: number; day: string; duration: string };

interface DaysProps {
  data: TicketBlocks;
  taskKey: string;
  actions: TaskActions;
  work: WorkLog;
  onAnnounce: (message: string) => void;
  /** Opens the log form; undefined while it is already open. */
  onLog?: () => void;
  logged: Logged | null;
}

const RECENT_DAYS = 5;

function Days({ data, taskKey, actions, work, onAnnounce, onLog, logged }: DaysProps) {
  const [all, setAll] = useState(false);
  const n = data.days.length;
  if (n === 0) {
    return (
      <div className="task-empty-row">
        <p className="task-empty">{`No work logged on ${taskKey} in the last 14 days.`}</p>
        {onLog && <LogButton onClick={onLog} />}
      </div>
    );
  }
  // A day the user just logged on is never hidden behind "Show older".
  const hidden = data.days.slice(RECENT_DAYS);
  const shown = all || hidden.some((d) => d.day === logged?.day) ? data.days : data.days.slice(0, RECENT_DAYS);
  return (
    <>
      {shown.map((d, i) => (
        <DayGroup
          key={d.day}
          day={d}
          taskKey={taskKey}
          actions={actions}
          onSaved={work.refetch}
          onAnnounce={onAnnounce}
          logged={logged}
          expanded={work.isOpen(d.day, i === 0)}
          onExpand={(open) => work.setOpen(d.day, open)}
        />
      ))}
      {shown.length < n && (
        <button type="button" className="task-btn-secondary task-day-older" onClick={() => setAll(true)}>
          {`Show ${n - shown.length} older ${n - shown.length === 1 ? "day" : "days"}`}
        </button>
      )}
    </>
  );
}

/** The Work log tab: days as disclosure rows, with the Log time form directly under the head line. */
export function TaskWorkLog({
  taskKey,
  actions,
  work,
  onAnnounce,
}: {
  taskKey: string;
  actions: TaskActions;
  work: WorkLog;
  onAnnounce: (message: string) => void;
}) {
  const { load, retry, refetch } = work;
  const [logging, setLogging] = useState(false);
  const [logged, setLogged] = useState<Logged | null>(null);
  const logBtn = useRef<HTMLButtonElement>(null);
  const head = useRef<HTMLDivElement>(null);
  const refocus = useRef(false);
  const close = () => {
    refocus.current = true;
    setLogging(false);
  };
  useEffect(() => {
    if (logging) head.current?.scrollIntoView?.({ block: "nearest" });
  }, [logging]);
  useEffect(() => {
    if (!logging && refocus.current) logBtn.current?.focus();
    refocus.current = false;
  }, [logging]);

  return (
    <div className="task-work">
      <div ref={head} className="task-work-head">
        <span className="task-work-summary">{load.s === "ok" && load.data.days.length > 0 ? summary(load.data) : ""}</span>
        {!logging && <LogButton btn={logBtn} onClick={() => setLogging(true)} />}
      </div>
      {logging && (
        <TaskLogTime
          taskKey={taskKey}
          actions={actions}
          today={load.s === "ok" ? load.data.to : undefined}
          onClose={close}
          onLogged={(message, block, duration) => {
            setLogged({ id: block.id, day: block.day, duration });
            work.setOpen(block.day, true);
            close();
            onAnnounce(message);
            refetch();
          }}
        />
      )}
      {load.s === "loading" && <Skeleton />}
      {load.s === "error" && (
        <div className="task-load-error">
          <p>{`Couldn't load work for ${taskKey}: ${load.error.replace(/\.$/, "")}`}</p>
          <button type="button" className="task-btn-secondary" onClick={retry}>
            Try again
          </button>
        </div>
      )}
      {load.s === "ok" && (
        <Days
          data={load.data}
          taskKey={taskKey}
          actions={actions}
          work={work}
          onAnnounce={onAnnounce}
          logged={logged}
          onLog={logging ? undefined : () => setLogging(true)}
        />
      )}
    </div>
  );
}
