"use client";

import { Fragment } from "react";

import { formatDuration } from "@/lib/format";
import type { TaskRow } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { todayView } from "./TaskModalTime";
import { tempoState } from "./tempoState";
import { StatusButton, type Shown } from "./TaskStatusButton";
import type { BlocksLoad } from "./useWorkLog";

interface Props {
  task: TaskRow;
  actions: TaskActions;
  shown: Shown;
  load: BlocksLoad;
  onStatus: (next: Shown) => void;
  onTempo: () => void;
}

/** Phones only: the status and the hours, right under the title instead of last in the sidebar. */
export function TaskModalSummary({ task, actions, shown, load, onStatus, onTempo }: Props) {
  const tempo = tempoState(load);
  const today = todayView(load.s === "ok" ? load.data.today : undefined);
  const parts = [
    <span key="week">{`This week ${formatDuration(task.week_seconds)}`}</span>,
    task.today_seconds > 0 && <span key="today">{`Today ${formatDuration(task.today_seconds)}`}</span>,
    tempo &&
      (tempo.review ? (
        <button key="tempo" type="button" className="task-review" onClick={onTempo}>
          {tempo.text}
        </button>
      ) : (
        <span key="tempo" className="task-tempo" data-tone={tempo.tone}>
          {tempo.text}
        </span>
      )),
  ].filter(Boolean);
  return (
    <div className="task-summary">
      <StatusButton taskKey={task.key} shown={shown} actions={actions} onStatus={onStatus} />
      <p className="task-summary-line">
        {parts.map((p, i) => (
          <Fragment key={i}>
            {i > 0 && " · "}
            {p}
          </Fragment>
        ))}
      </p>
      {today && <p className="task-summary-line">{`Today ${today.worked} worked ·${today.tempo}`}</p>}
    </div>
  );
}
