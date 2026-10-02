"use client";

import { useRef, useState } from "react";

import { Skeleton } from "./TaskSkeleton";
import type { DetailLoad } from "./useTicketDetail";
import { useOverflow } from "./useOverflow";

/** Four lines are shown; longer text ends in an ellipsis above a Show more toggle (only when it really overflows). */
function Prose({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const p = useRef<HTMLParagraphElement>(null);
  // The preview never spends a line on empty space: runs of blank lines become one paragraph break.
  const shown = open ? text : text.trim().replace(/\n(?:[ \t]*\n){2,}/g, "\n\n");
  const overflows = useOverflow(p, shown, open);
  return (
    <>
      <p ref={p} className="task-prose" data-open={open || undefined}>
        {shown}
      </p>
      {overflows && (
        <button type="button" className="task-more" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {open ? "Show less" : "Show more"}
        </button>
      )}
    </>
  );
}

export function TaskDescription({ taskKey, load, retry }: { taskKey: string; load: DetailLoad; retry: () => void }) {
  return (
    <section className="task-description" aria-labelledby="task-description-label">
      <h3 id="task-description-label" className="task-label">
        Description
      </h3>
      {load.s === "loading" && <Skeleton />}
      {load.s === "error" && (
        <div className="task-load-error">
          <p>{`Couldn't load ${taskKey} from Jira: ${load.error.replace(/\.$/, "")}`}</p>
          <button type="button" className="task-btn-secondary" onClick={retry}>
            Try again
          </button>
        </div>
      )}
      {load.s === "ok" &&
        (load.detail.description ? <Prose text={load.detail.description} /> : <p className="task-empty">No description in Jira.</p>)}
    </section>
  );
}
