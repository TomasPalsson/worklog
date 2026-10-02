"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";

import { formatDuration } from "@/lib/format";
import type { TicketBlocks } from "@/lib/types";
import { DayGroup } from "./TaskDayGroup";
import { TaskLogTime } from "./TaskLogTime";
import type { TaskActions } from "./TaskCard";

type Load = { s: "loading" } | { s: "error"; error: string } | { s: "ok"; data: TicketBlocks };

function Skeleton() {
  return (
    <div className="task-skel" aria-hidden="true">
      <i style={{ width: "92%" }} />
      <i style={{ width: "80%" }} />
      <i style={{ width: "64%" }} />
    </div>
  );
}

/** Loads on open / key change; `refetch` swaps data in place (no skeleton) after a write. */
function useBlocks(key: string, actions: TaskActions) {
  const [load, setLoad] = useState<Load>({ s: "loading" });
  const [attempt, setAttempt] = useState(0);
  const loader = actions.loadTicketBlocks;
  useEffect(() => {
    let live = true;
    setLoad({ s: "loading" });
    loader(key).then((res) => {
      if (live) setLoad(res.ok ? { s: "ok", data: res.data } : { s: "error", error: res.error });
    });
    return () => {
      live = false;
    };
  }, [key, attempt, loader]);
  const refetch = useCallback(async () => {
    const res = await loader(key);
    if (res.ok) setLoad({ s: "ok", data: res.data });
  }, [key, loader]);
  return { load, retry: () => setAttempt((n) => n + 1), refetch };
}

function LogButton({ onClick, btn }: { onClick: () => void; btn?: React.Ref<HTMLButtonElement> }) {
  return (
    <button ref={btn} type="button" className="task-btn-secondary task-log-open" onClick={onClick}>
      <Plus size={12} aria-hidden="true" />
      Log time
    </button>
  );
}

const summary = ({ days }: TicketBlocks) =>
  `${formatDuration(days.reduce((sum, d) => sum + d.line_seconds, 0))} over ${days.length} ${days.length === 1 ? "day" : "days"}`;

type Logged = { id: number; day: string; duration: string };

interface DaysProps {
  data: TicketBlocks;
  taskKey: string;
  actions: TaskActions;
  onSaved: () => void;
  onAnnounce: (message: string) => void;
  /** Opens the log form; undefined while it is already open. */
  onLog?: () => void;
  logged: Logged | null;
}

const RECENT_DAYS = 5;

function Days({ data, taskKey, actions, onSaved, onAnnounce, onLog, logged }: DaysProps) {
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
      {shown.map((d) => (
        <DayGroup key={d.day} day={d} taskKey={taskKey} actions={actions} onSaved={onSaved} onAnnounce={onAnnounce} logged={logged} />
      ))}
      {shown.length < n && (
        <button type="button" className="task-btn-secondary task-day-older" onClick={() => setAll(true)}>
          {`Show ${n - shown.length} older ${n - shown.length === 1 ? "day" : "days"}`}
        </button>
      )}
    </>
  );
}

export function TaskWorkLog({
  taskKey,
  actions,
  onAnnounce,
}: {
  taskKey: string;
  actions: TaskActions;
  onAnnounce: (message: string) => void;
}) {
  const { load, retry, refetch } = useBlocks(taskKey, actions);
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
    <section className="task-section task-work">
      <div ref={head} className="task-work-head">
        <div className="task-work-title">
          <h3>Work logged · last 14 days</h3>
          {load.s === "ok" && load.data.days.length > 0 && <span className="task-work-summary">{summary(load.data)}</span>}
        </div>
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
      {load.s === "ok" && <Days
          data={load.data}
          taskKey={taskKey}
          actions={actions}
          onSaved={refetch}
          onAnnounce={onAnnounce}
          logged={logged}
          onLog={logging ? undefined : () => setLogging(true)}
        />}
    </section>
  );
}
