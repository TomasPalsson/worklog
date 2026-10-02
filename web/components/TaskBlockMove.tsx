"use client";

import { useRef, useState } from "react";
import { ArrowRightLeft } from "lucide-react";

import { toast } from "@/lib/toast";
import type { JiraTicket, RawBlock } from "@/lib/types";
import { TicketCombobox } from "./TicketCombobox";

interface Props {
  block: RawBlock;
  tickets: JiraTicket[];
  /** Reload the work log; resolves once the new days are in. */
  onMoved: () => void | Promise<void>;
  onAnnounce?: (message: string) => void;
}

/** "Move" on a block row: opens the ticket picker inline; once the block is assigned elsewhere the work log reloads. */
export function BlockMove({ block, tickets, onMoved, onAnnounce }: Props) {
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(false);
    btn.current?.focus();
  };
  // The row unmounts with the move: land on the day's toggle, which stays.
  const assigned = async (ok: boolean, key: string | null) => {
    if (!ok) return; // the combobox has toasted the error; the picker stays for another try
    const toggle = btn.current?.closest(".task-day")?.querySelector<HTMLElement>(".task-day-toggle");
    const message = key ? `Moved to ${key}` : "Block unassigned";
    setOpen(false);
    await onMoved();
    onAnnounce?.(message);
    toast.ok(message);
    toggle?.focus();
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
        <div className="task-block-picker" onKeyDown={onKeyDown}>
          <TicketCombobox blockId={block.id} current={block.jira_issue} tickets={tickets} day={block.day} defaultOpen onAssigned={assigned} />
          <button type="button" className="task-btn-secondary" onClick={close}>
            Cancel
          </button>
        </div>
      )}
    </>
  );
}
