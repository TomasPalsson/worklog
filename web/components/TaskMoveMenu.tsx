"use client";

import { useRef, useState, type FocusEvent, type KeyboardEvent } from "react";
import { ArrowRightLeft } from "lucide-react";

import { COLUMNS, type Column } from "@/lib/taskBoard";

/** Non-drag way to move a card: a button beside the card's main button and a menu of the other columns. */
export function TaskMoveMenu({ taskKey, column, onMove }: {
  taskKey: string;
  column: Column;
  onMove: (to: Column) => void;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (!open || e.key !== "Escape") return;
    e.preventDefault(); // the panel's Esc handler sees defaultPrevented and stays open
    close();
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
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <ArrowRightLeft size={14} aria-hidden="true" />
      </button>
      {open && (
        <div role="menu" className="task-move-menu">
          {COLUMNS.filter((c) => c.id !== column).map((c, i) => (
            <button
              key={c.id}
              type="button"
              role="menuitem"
              autoFocus={i === 0}
              onClick={() => {
                setOpen(false);
                onMove(c.id);
              }}
            >
              {`Move to ${c.title}`}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
