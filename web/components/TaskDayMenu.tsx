"use client";

import { useRef, useState, type Ref } from "react";
import { MoreHorizontal } from "lucide-react";

import { menuKeys } from "./menuKeys";
import { useMenuDismiss } from "./useMenuDismiss";

export interface DayMenuProps {
  label: string;
  /** "Use tracked time" is offered only while the hours are set by hand. */
  byHand: boolean;
  busy: boolean;
  btn: Ref<HTMLButtonElement>;
  onEditHours: () => void;
  onUseTracked: () => void;
  onEditText: () => void;
}

/** The "⋯" menu on a day row: the day-level tools that are not the next likely action. */
export function DayMenu({ label, byHand, busy, btn, onEditHours, onUseTracked, onEditText }: DayMenuProps) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  useMenuDismiss(open, wrap, () => setOpen(false));
  const items: [string, () => void][] = [["Edit hours", onEditHours]];
  if (byHand) items.push(["Use tracked time", onUseTracked]);
  items.push(["Edit Tempo text", onEditText]);

  return (
    <span ref={wrap} className="task-day-menu-wrap">
      <button
        ref={btn}
        type="button"
        className="task-icon-btn task-day-more"
        aria-label={`More for ${label}`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal size={16} aria-hidden="true" />
      </button>
      {open && (
        <span className="task-menu task-day-menu" role="menu" onKeyDown={menuKeys}>
          {items.map(([text, run], i) => (
            <button
              key={text}
              type="button"
              role="menuitem"
              autoFocus={i === 0}
              disabled={busy}
              onClick={() => {
                // The item unmounts with the menu; keep focus on the trigger rather than letting it fall to <body>.
                wrap.current?.querySelector("button")?.focus();
                setOpen(false);
                run();
              }}
            >
              {text}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}
