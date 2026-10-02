"use client";

import { useRef, useState, type Ref } from "react";
import { MoreHorizontal } from "lucide-react";

import type { DayOps } from "./TaskDayTools";
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
  ops: DayOps;
}

/** The "⋯" menu on a day row: the day-level tools that are not the next likely action. */
export function DayMenu({ label, byHand, busy, btn, onEditHours, onUseTracked, onEditText, ops }: DayMenuProps) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  useMenuDismiss(open, wrap, () => (setOpen(false), setConfirm(false)));
  const [confirm, setConfirm] = useState(false);
  const items: [string, () => void, string?][] = [["Edit hours", onEditHours]];
  if (byHand) items.push(["Use tracked time", onUseTracked]);
  items.push(["Edit Tempo text", onEditText]);
  items.push(["Regenerate text with AI", ops.regenerate, ops.hasLine ? undefined : "There is no Tempo text for this day yet"]);
  const canMerge = ops.blocks >= 2;

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
          {items.map(([text, run, why], i) => (
            <button
              key={text}
              type="button"
              role="menuitem"
              autoFocus={i === 0}
              disabled={busy || !!why}
              title={why}
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
          {canMerge && !confirm && (
            <button type="button" role="menuitem" disabled={busy} onClick={() => setConfirm(true)}>
              {`Merge ${ops.blocks} blocks into one`}
            </button>
          )}
          {canMerge && confirm && (
            <span className="task-menu-confirm" role="group" aria-label="Confirm merge">
              <em>{`Merge ${ops.blocks} blocks?`}</em>
              <button
                type="button"
                role="menuitem"
                autoFocus
                disabled={busy}
                onClick={() => {
                  wrap.current?.querySelector("button")?.focus();
                  setOpen(false);
                  setConfirm(false);
                  ops.merge();
                }}
              >
                Merge
              </button>
              <button type="button" role="menuitem" onClick={() => setConfirm(false)}>
                Cancel
              </button>
            </span>
          )}
        </span>
      )}
    </span>
  );
}
