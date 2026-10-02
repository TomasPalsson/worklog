"use client";

import { useRef, useState } from "react";
import { ChevronDown } from "lucide-react";

import { transitionLabel } from "@/lib/taskBoard";
import type { StatusCategory, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { menuKeys } from "./menuKeys";
import { useMenuDismiss } from "./useMenuDismiss";

export type Shown = { status: string | null; status_category: StatusCategory | null };

interface Props {
  taskKey: string;
  shown: Shown;
  actions: TaskActions;
  onStatus: (next: Shown) => void;
}

/** Open/close the transitions menu and apply a pick; `busy` covers both the lookup and the move. */
function useStatusMenu({ taskKey, actions, onStatus }: Omit<Props, "shown">) {
  const [menu, setMenu] = useState<Transition[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [moving, setMoving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function toggle() {
    if (menu) return setMenu(null);
    setBusy(true);
    setError(null);
    const res = await actions.loadTransitions(taskKey);
    if (res.ok) setMenu(res.data);
    else setError(res.error);
    setBusy(false);
  }

  async function move(t: Transition) {
    setBusy(true);
    setMoving(true);
    setError(null);
    const res = await actions.transitionTicket(taskKey, t.id);
    if (res.ok) {
      onStatus({ status: res.data.status, status_category: res.data.status_category });
      setMenu(null);
    } else setError(res.error);
    setBusy(false);
    setMoving(false);
  }
  return { menu, close: () => setMenu(null), busy, moving, error, toggle, move };
}

/** The ticket's status as one prominent button; it opens Jira's own transitions for this ticket. */
export function StatusButton(props: Props) {
  const { shown, taskKey } = props;
  const s = useStatusMenu(props);
  const wrap = useRef<HTMLDivElement>(null);
  useMenuDismiss(s.menu !== null, wrap, s.close);
  return (
    <div ref={wrap} className="task-status-wrap">
      <button
        type="button"
        className="task-status"
        data-category={shown.status_category ?? undefined}
        data-testid={`status-${taskKey}`}
        aria-haspopup="true"
        aria-expanded={s.menu !== null}
        disabled={s.busy}
        onClick={s.toggle}
      >
        {s.moving ? "Moving…" : (shown.status ?? "No status")}
        <ChevronDown size={14} aria-hidden="true" />
      </button>
      {s.menu && (
        <span className="task-menu task-status-menu" role="menu" onKeyDown={menuKeys}>
          {s.menu.map((t, i) => (
            <button key={t.id} type="button" role="menuitem" autoFocus={i === 0} disabled={s.busy} onClick={() => s.move(t)}>
              {transitionLabel(t)}
            </button>
          ))}
          {s.menu.length === 0 && <em>Jira offers no status changes for this ticket right now.</em>}
        </span>
      )}
      {s.error && (
        <span role="alert" className="task-error">
          {s.error}
        </span>
      )}
    </div>
  );
}
