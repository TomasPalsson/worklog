"use client";

import { useEffect, useRef, useState } from "react";
import { ArrowRightLeft } from "lucide-react";

import type { JiraTicket, RawBlock } from "@/lib/types";
import { TicketCombobox } from "./TicketCombobox";

/** Watches the combobox trigger's `aria-busy`: busy -> idle means the assign finished (success or not). */
function useAssignDone(panel: React.RefObject<HTMLElement | null>, active: boolean, onDone: () => void) {
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    const el = panel.current;
    if (!active || !el) return;
    let busy = false;
    const obs = new MutationObserver((records) => {
      for (const r of records) {
        const now = (r.target as Element).getAttribute("aria-busy") !== null;
        if (busy && !now) done.current();
        busy = now;
      }
    });
    obs.observe(el, { attributes: true, attributeFilter: ["aria-busy"], subtree: true });
    return () => obs.disconnect();
  }, [panel, active]);
}

/** "Move" on a block row: opens the ticket picker inline; once the block is assigned elsewhere the work log reloads. */
export function BlockMove({ block, tickets, onMoved }: { block: RawBlock; tickets: JiraTicket[]; onMoved: () => void }) {
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  useAssignDone(panel, open, onMoved);
  // Open the list straight away: "Move" then needs one click, not two.
  useEffect(() => {
    if (open) panel.current?.querySelector<HTMLButtonElement>(".ticket-chip")?.click();
  }, [open]);
  const close = () => {
    setOpen(false);
    btn.current?.focus();
  };
  // The combobox claims Esc while its list is open; otherwise Esc closes this picker, never the dialog.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    if (e.defaultPrevented) return;
    e.preventDefault();
    close();
  };
  return (
    <>
      <button
        ref={btn}
        type="button"
        className="task-icon-btn task-block-move"
        aria-label="Move this block to another ticket"
        aria-expanded={open}
        title="Move to another ticket"
        onClick={() => (open ? close() : setOpen(true))}
      >
        <ArrowRightLeft size={14} aria-hidden="true" />
      </button>
      {open && (
        <div ref={panel} className="task-block-picker" onKeyDown={onKeyDown}>
          <TicketCombobox blockId={block.id} current={block.jira_issue} tickets={tickets} day={block.day} />
          <button type="button" className="task-btn-secondary" onClick={close}>
            Cancel
          </button>
        </div>
      )}
    </>
  );
}
