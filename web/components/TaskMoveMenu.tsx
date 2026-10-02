"use client";

import { useEffect, useRef, useState, type FocusEvent, type KeyboardEvent } from "react";
import { ArrowRightLeft } from "lucide-react";

import type { ActionResult } from "@/app/actions";
import { COLUMNS, movesInto, type Column } from "@/lib/taskBoard";
import type { Transition } from "@/lib/types";
import { menuKeys } from "./menuKeys";

/** Non-drag way to move a card: a button beside the card's main button and a menu of the other columns. */
export function TaskMoveMenu({ taskKey, column, load, onMove }: {
  taskKey: string;
  column: Column;
  load: () => Promise<ActionResult<Transition[]>>;
  /** `transitions` is what Jira offered, or undefined when the lookup failed. */
  onMove: (to: Column, transitions?: Transition[]) => void;
}) {
  const [open, setOpen] = useState(false);
  // null while Jira is being asked; false when the lookup failed (items stay enabled and the move re-checks).
  const [offered, setOffered] = useState<Transition[] | null | false>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const loader = useRef(load);
  loader.current = load;

  useEffect(() => {
    if (!open) return;
    let live = true;
    setOffered(null);
    loader.current().then((res) => live && setOffered(res.ok ? res.data : false));
    return () => {
      live = false;
    };
  }, [open]);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault(); // the panel's Esc handler sees defaultPrevented and stays open
      return close();
    }
    menuKeys(e);
  };
  const onBlur = (e: FocusEvent) => {
    if (open && !e.currentTarget.contains(e.relatedTarget as Node | null)) setOpen(false);
  };

  return (
    <div className="task-move" data-open={open || undefined} onKeyDown={onKeyDown} onBlur={onBlur}>
      <button
        ref={trigger}
        type="button"
        className="task-move-btn"
        aria-label={`Move ${taskKey}`}
        data-tip="Move"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <ArrowRightLeft size={14} aria-hidden="true" />
      </button>
      {open && (
        <div role="menu" className="task-move-menu">
          {COLUMNS.filter((c) => c.id !== column).map((c, i) => {
            const none = !!offered && movesInto(offered, c.id).length === 0;
            const blocked = offered === null || none;
            return (
              <button
                key={c.id}
                type="button"
                role="menuitem"
                aria-disabled={blocked || undefined}
                autoFocus={i === 0}
                onClick={() => {
                  if (blocked) return;
                  setOpen(false);
                  onMove(c.id, offered || undefined);
                }}
              >
                {offered === null ? `${c.title} — Checking Jira…` : none ? `${c.title} — no Jira move` : `Move to ${c.title}`}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
