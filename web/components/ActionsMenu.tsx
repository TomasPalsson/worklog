"use client";

import { useRef, useState, type ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { menuKeys } from "./menuKeys";
import { useMenuDismiss } from "./useMenuDismiss";

export interface MenuAction {
  label: string;
  pendingLabel: string;
  title: string;
  icon: ReactNode;
  pending: boolean;
  onClick: () => void;
}

/** The day page's "More" menu: occasional actions behind one button so the action bar stays one row. */
export function ActionsMenu({ items }: { items: MenuAction[] }) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  useMenuDismiss(open, wrap, () => setOpen(false));
  const busy = items.some((i) => i.pending);
  const firstFree = items.findIndex((i) => !i.pending);

  return (
    <span ref={wrap} className="actions-more">
      <button
        type="button"
        className="action-btn"
        aria-label="More actions"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-busy={busy || undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal />
        More
      </button>
      {open && (
        <span className="task-menu actions-more-menu" role="menu" onKeyDown={menuKeys}>
          {items.map((item, i) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              autoFocus={i === firstFree}
              disabled={item.pending}
              title={item.title}
              onClick={() => {
                // The item unmounts with the menu; keep focus on the trigger rather than letting it fall to <body>.
                wrap.current?.querySelector("button")?.focus();
                setOpen(false);
                item.onClick();
              }}
            >
              {item.icon}
              {item.pending ? item.pendingLabel : item.label}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}
